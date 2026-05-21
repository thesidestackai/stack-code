//! A2-L2b workspace-write runtime path-safety resolver (slice 1).
//!
//! This module performs the **runtime** counterpart to the offline L2a
//! schema's lexical path checks. L2a has already refused absolute paths,
//! `..` traversal, deny-component path segments, and deny-glob filenames
//! by the time a plan reaches the runner; this resolver re-proves those
//! invariants against the live filesystem and additionally refuses:
//!
//! - symlinks anywhere in the parent chain
//! - symlink target files
//! - canonical-parent paths that do not start with the operator-supplied
//!   workspace root
//! - missing or non-directory parents
//!
//! # Hard contract (slice 1)
//!
//! - No filesystem writes anywhere in this module.
//! - No directory creation.
//! - No subprocesses, no broker, no Ollama, no model calls.
//! - No approval prompts, no checkpoints, no diff preview.
//! - Not yet called by [`crate::runner::run_plan`]; consumers wire it in
//!   slice 2.
//! - Read-only filesystem APIs only: [`std::fs::symlink_metadata`] and
//!   [`std::path::Path::canonicalize`].
//!
//! # Operator contract
//!
//! - On success the resolver returns a [`ResolvedWriteTarget`] with the
//!   canonicalized parent path joined with the original file name, plus
//!   a boolean indicating whether the file already exists.
//! - On refusal the resolver returns a [`WriteTargetRefusal`] variant
//!   that carries the operator-facing marker and the runner exit code
//!   (`6` — runtime path safety refused).
//!
//! Exit code `6` extends the L1b runner's `0..=4` family and avoids
//! collision with [`crate::report::EXIT_PARSE_ERROR`] (`5`).

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use a2_plan_schema::WriteTarget;

use crate::markers;

/// Exit code emitted by the CLI when the workspace-write runtime path
/// safety check refuses a step. Pinned `&'static` integer so downstream
/// tooling can match on it without re-importing the runner crate.
pub const EXIT_WRITE_PATH_REFUSED: i32 = 6;

/// Deny-component path segments. Matches the L2a schema's component
/// deny list verbatim; re-enforced here as defense in depth.
const DENY_COMPONENTS: &[&str] = &[".git", ".claw", ".claude"];

/// Successfully resolved workspace-write target.
///
/// `absolute` is `parent.join(file_name)` where `parent` has been
/// canonicalized through the live filesystem and confirmed to live
/// strictly under the operator-supplied workspace root. `already_exists`
/// reflects the live state of `absolute` at resolution time (no
/// guarantee against TOCTOU; that is the dirfd-anchored write path's
/// responsibility in slice 2+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWriteTarget {
    pub absolute: PathBuf,
    pub parent: PathBuf,
    pub file_name: OsString,
    pub already_exists: bool,
}

/// Why the resolver refused to admit a write target.
///
/// Each variant maps to exactly one operator-facing marker via
/// [`WriteTargetRefusal::marker`] and to [`EXIT_WRITE_PATH_REFUSED`]
/// via [`WriteTargetRefusal::exit_code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTargetRefusal {
    /// Absolute path, `..` traversal, canonical-parent prefix mismatch
    /// with the workspace root, or any other structural escape that is
    /// not specifically a symlink or a deny-component / deny-glob hit.
    PathEscape,
    /// The parent directory of the requested target does not exist or
    /// is not a directory. L2b refuses parent creation in slice 1.
    ParentMissing,
    /// A component in the path equals `.git`, `.claw`, or `.claude`.
    DenyComponent,
    /// The final filename matches the deny-glob set
    /// (`.env*`, `secret*`, `credentials*`, `*.pem`, `*.key`).
    DenyGlobFilename,
    /// The final target file exists and is itself a symlink.
    SymlinkTarget,
    /// A component in the parent chain is a symlink.
    SymlinkParent,
}

impl WriteTargetRefusal {
    /// Stable operator-facing marker string for this refusal.
    #[must_use]
    pub fn marker(&self) -> &'static str {
        match self {
            Self::PathEscape | Self::DenyComponent | Self::DenyGlobFilename => {
                markers::L2B_PATH_REFUSED_RUNTIME
            }
            Self::ParentMissing => markers::L2B_PARENT_MISSING,
            Self::SymlinkTarget | Self::SymlinkParent => markers::L2B_SYMLINK_REFUSED,
        }
    }

    /// Runner exit code for this refusal. Currently every variant maps
    /// to [`EXIT_WRITE_PATH_REFUSED`]; the helper exists so callers do
    /// not have to know that and so slice-2+ can refine the mapping
    /// without churn at call sites.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        EXIT_WRITE_PATH_REFUSED
    }
}

/// Resolve a [`WriteTarget`] against an already-canonicalized
/// workspace root.
///
/// # Caller contract
///
/// - `workspace_root` MUST be canonicalized by the caller. The resolver
///   does not re-canonicalize it on every call to avoid quadratic
///   syscalls across a multi-step plan; passing a non-canonical root
///   may cause `PathEscape` refusal even for safe targets.
/// - `target.path` is treated as workspace-relative. The L2a schema
///   has already refused absolute paths, `..` segments, deny-component
///   segments, and deny-glob filenames at parse time; this function
///   re-proves those invariants and adds the live-filesystem checks.
///
/// # Errors
///
/// Returns a [`WriteTargetRefusal`] for any refusal arm. Never panics
/// on bad input.
///
/// # No side effects
///
/// This function performs only read-only filesystem syscalls
/// ([`std::fs::symlink_metadata`] and [`Path::canonicalize`]). It never
/// creates, opens for write, deletes, renames, chmods, or chowns any
/// path.
pub fn resolve_write_target(
    workspace_root: &Path,
    target: &WriteTarget,
) -> Result<ResolvedWriteTarget, WriteTargetRefusal> {
    // 1. Lexical re-checks (defense in depth vs L2a).
    let original = Path::new(&target.path);
    if original.as_os_str().is_empty() {
        return Err(WriteTargetRefusal::PathEscape);
    }
    if original.is_absolute() {
        return Err(WriteTargetRefusal::PathEscape);
    }

    let mut last_normal: Option<&std::ffi::OsStr> = None;
    for component in original.components() {
        match component {
            Component::Normal(name) => {
                let n = name.to_string_lossy();
                if DENY_COMPONENTS.iter().any(|d| *d == n) {
                    return Err(WriteTargetRefusal::DenyComponent);
                }
                last_normal = Some(name);
            }
            Component::ParentDir => return Err(WriteTargetRefusal::PathEscape),
            // `CurDir` (`.`) is allowed mid-path; the canonical step
            // strips it. RootDir / Prefix are caught by `is_absolute`
            // above. Treat any other unexpected variant as escape.
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {
                return Err(WriteTargetRefusal::PathEscape);
            }
        }
    }
    let file_name = match last_normal {
        Some(n) => n.to_os_string(),
        None => return Err(WriteTargetRefusal::PathEscape),
    };

    if matches_deny_glob(&file_name.to_string_lossy()) {
        return Err(WriteTargetRefusal::DenyGlobFilename);
    }

    // 2. Build the unresolved candidate path and split off the parent.
    let candidate = workspace_root.join(original);
    let parent_unresolved = candidate
        .parent()
        .ok_or(WriteTargetRefusal::PathEscape)?
        .to_path_buf();

    // 3. Walk the parent components from `workspace_root` and refuse
    //    on any symlink encountered along the way. This catches both
    //    "symlink that points inside the workspace" (which would still
    //    leak data through a sneaky alias) and "symlink that points
    //    outside" (which would also fail the canonical prefix check).
    walk_parent_for_symlinks(workspace_root, &parent_unresolved)?;

    // 4. Canonicalize the parent. After the symlink walk above, this
    //    should succeed if every component exists and is a regular dir.
    let parent_canonical = parent_unresolved
        .canonicalize()
        .map_err(|_| WriteTargetRefusal::ParentMissing)?;

    // Confirm the canonical parent is still a directory. `canonicalize`
    // succeeds on files too; we want a dir here.
    let parent_meta = std::fs::symlink_metadata(&parent_canonical)
        .map_err(|_| WriteTargetRefusal::ParentMissing)?;
    if !parent_meta.is_dir() {
        return Err(WriteTargetRefusal::ParentMissing);
    }

    // 5. Workspace-root prefix re-check post-canonicalization.
    if !parent_canonical.starts_with(workspace_root) {
        return Err(WriteTargetRefusal::PathEscape);
    }

    // 6. Recompose the absolute target path.
    let absolute = parent_canonical.join(&file_name);

    // 7. Inspect the final target (if it exists). It must NOT be a
    //    symlink. If it exists, it must be a regular file — refusing
    //    overwrites of directories, sockets, devices, etc.
    let already_exists = match std::fs::symlink_metadata(&absolute) {
        Ok(meta) => {
            let ft = meta.file_type();
            if ft.is_symlink() {
                return Err(WriteTargetRefusal::SymlinkTarget);
            }
            if !meta.is_file() {
                return Err(WriteTargetRefusal::PathEscape);
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        // Any other I/O error on the final path (permission denied,
        // ENOTDIR mid-component, etc.) is reported as ParentMissing —
        // we can't safely admit the write.
        Err(_) => return Err(WriteTargetRefusal::ParentMissing),
    };

    Ok(ResolvedWriteTarget {
        absolute,
        parent: parent_canonical,
        file_name,
        already_exists,
    })
}

/// Walk every component of `parent_unresolved` starting from
/// `workspace_root` and refuse if any intermediate path component is a
/// symlink. Returns [`WriteTargetRefusal::ParentMissing`] if any
/// component does not exist or is not a directory.
fn walk_parent_for_symlinks(
    workspace_root: &Path,
    parent_unresolved: &Path,
) -> Result<(), WriteTargetRefusal> {
    // `parent_unresolved` is always `workspace_root.join(...)` so the
    // strip should succeed; if it doesn't, treat as escape.
    let rel = parent_unresolved
        .strip_prefix(workspace_root)
        .map_err(|_| WriteTargetRefusal::PathEscape)?;

    let mut acc = workspace_root.to_path_buf();
    for component in rel.components() {
        match component {
            Component::Normal(name) => {
                acc.push(name);
                let meta = std::fs::symlink_metadata(&acc)
                    .map_err(|_| WriteTargetRefusal::ParentMissing)?;
                if meta.file_type().is_symlink() {
                    return Err(WriteTargetRefusal::SymlinkParent);
                }
                if !meta.is_dir() {
                    return Err(WriteTargetRefusal::ParentMissing);
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(WriteTargetRefusal::PathEscape);
            }
        }
    }
    Ok(())
}

/// Deny-glob matcher for the final filename. Mirrors the L2a schema's
/// `matches_deny_pattern` byte-for-byte so the runtime check and the
/// schema check stay in lockstep — a plan accepted by L2a must produce
/// the same refusal decision here.
///
/// The case-sensitive `ends_with` calls intentionally mirror the
/// schema's behavior. Diverging to case-insensitive matching here
/// without also updating L2a would create a schema-vs-runtime gap where
/// a plan with e.g. `SERVER.PEM` would pass the offline validator and
/// then surprise the operator at runtime. Any case-sensitivity change
/// must land in both crates together.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn matches_deny_glob(name: &str) -> bool {
    if name == ".env" || name.starts_with(".env") {
        return true;
    }
    if name.starts_with("secret") || name.starts_with("credentials") {
        return true;
    }
    if name.ends_with(".pem") || name.ends_with(".key") {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_is_six() {
        assert_eq!(EXIT_WRITE_PATH_REFUSED, 6);
    }

    #[test]
    fn refusal_markers_map_to_expected_constants() {
        assert_eq!(
            WriteTargetRefusal::PathEscape.marker(),
            markers::L2B_PATH_REFUSED_RUNTIME
        );
        assert_eq!(
            WriteTargetRefusal::DenyComponent.marker(),
            markers::L2B_PATH_REFUSED_RUNTIME
        );
        assert_eq!(
            WriteTargetRefusal::DenyGlobFilename.marker(),
            markers::L2B_PATH_REFUSED_RUNTIME
        );
        assert_eq!(
            WriteTargetRefusal::ParentMissing.marker(),
            markers::L2B_PARENT_MISSING
        );
        assert_eq!(
            WriteTargetRefusal::SymlinkTarget.marker(),
            markers::L2B_SYMLINK_REFUSED
        );
        assert_eq!(
            WriteTargetRefusal::SymlinkParent.marker(),
            markers::L2B_SYMLINK_REFUSED
        );
    }

    #[test]
    fn every_refusal_variant_exits_six() {
        for refusal in [
            WriteTargetRefusal::PathEscape,
            WriteTargetRefusal::ParentMissing,
            WriteTargetRefusal::DenyComponent,
            WriteTargetRefusal::DenyGlobFilename,
            WriteTargetRefusal::SymlinkTarget,
            WriteTargetRefusal::SymlinkParent,
        ] {
            assert_eq!(refusal.exit_code(), 6);
        }
    }

    #[test]
    fn deny_glob_matches_expected_names() {
        for name in [
            ".env",
            ".env.local",
            ".env.production",
            "secret.txt",
            "secrets.yaml",
            "credentials.json",
            "tls/server.pem", // not relevant via this fn (called on filename only)
            "server.pem",
            "id_rsa.key",
        ] {
            let last = name.rsplit('/').next().unwrap();
            assert!(matches_deny_glob(last), "should deny: {name}");
        }
    }

    #[test]
    fn deny_glob_allows_normal_names() {
        for name in [
            "README.md",
            "notes/scratch.md",
            "src/foo/bar.rs",
            ".gitignore",
        ] {
            let last = name.rsplit('/').next().unwrap();
            assert!(!matches_deny_glob(last), "should allow: {name}");
        }
    }

    #[test]
    fn empty_path_is_refused() {
        let target = WriteTarget {
            path: String::new(),
            create_if_absent: false,
        };
        assert_eq!(
            resolve_write_target(Path::new("/tmp"), &target),
            Err(WriteTargetRefusal::PathEscape)
        );
    }

    #[test]
    fn absolute_path_is_refused_without_filesystem_access() {
        let target = WriteTarget {
            path: "/etc/passwd".into(),
            create_if_absent: false,
        };
        // `/nonexistent` doesn't need to exist — the absolute-path
        // refusal happens lexically before any syscall.
        assert_eq!(
            resolve_write_target(Path::new("/nonexistent/workspace"), &target),
            Err(WriteTargetRefusal::PathEscape)
        );
    }

    #[test]
    fn parent_dir_traversal_is_refused() {
        let target = WriteTarget {
            path: "../escape.txt".into(),
            create_if_absent: false,
        };
        assert_eq!(
            resolve_write_target(Path::new("/nonexistent/workspace"), &target),
            Err(WriteTargetRefusal::PathEscape)
        );
    }

    #[test]
    fn deny_component_anywhere_is_refused() {
        for path in [".git/config", ".claw/state", ".claude/settings.json"] {
            let target = WriteTarget {
                path: path.into(),
                create_if_absent: false,
            };
            assert_eq!(
                resolve_write_target(Path::new("/nonexistent/workspace"), &target),
                Err(WriteTargetRefusal::DenyComponent),
                "path {path} should be refused via DenyComponent"
            );
        }
    }

    #[test]
    fn deny_glob_filename_is_refused_lexically() {
        for path in [
            ".env",
            "config/.env.local",
            "secrets.yaml",
            "credentials.json",
            "tls/server.pem",
            "ssh/id_rsa.key",
        ] {
            let target = WriteTarget {
                path: path.into(),
                create_if_absent: false,
            };
            assert_eq!(
                resolve_write_target(Path::new("/nonexistent/workspace"), &target),
                Err(WriteTargetRefusal::DenyGlobFilename),
                "path {path} should be refused via DenyGlobFilename"
            );
        }
    }
}
