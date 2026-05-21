//! A2-L2b slice 1 — workspace-write path-safety integration tests.
//!
//! These tests create real (Unix) tempdirs, real (regular) directories,
//! real regular files, and real symlinks, then exercise every refusal
//! arm of [`a2_plan_runner::write_runtime::resolve_write_target`] plus
//! the two happy paths (new file in an existing parent; existing
//! regular file). No subprocesses, no broker, no model calls, no
//! writes outside the per-test tempdir.
//!
//! Tempdir cleanup is best-effort via a [`Drop`] guard. If a test panics
//! before `Drop` runs, the leftover dir under `/tmp` is harmless and
//! easy to clean by hand.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use a2_plan_runner::write_runtime::{
    resolve_write_target, WriteTargetRefusal, EXIT_WRITE_PATH_REFUSED,
};
use a2_plan_schema::WriteTarget;

/// Tempdir guard. Creates a uniquely-named directory under `/tmp` (via
/// [`std::env::temp_dir`]) and removes it on drop. Returns the
/// **canonicalized** root path so the resolver's caller-contract about
/// pre-canonicalized roots is satisfied.
struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock not before unix epoch")
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "a2_l2b_path_safety_{}_{}_{}",
            prefix,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir(&p).expect("tempdir create");
        let root = p.canonicalize().expect("tempdir canonicalize");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn mkdir_p(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for c in rel.split('/').filter(|s| !s.is_empty()) {
            p.push(c);
        }
        std::fs::create_dir_all(&p).expect("mkdir_p");
        p
    }

    fn touch(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for c in rel.split('/').filter(|s| !s.is_empty()) {
            p.push(c);
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("touch: create parent");
        }
        std::fs::write(&p, b"").expect("touch: write empty");
        p
    }

    /// Symlink `link_rel` -> `target` (target may be absolute or
    /// relative to the link's parent).
    fn symlink_to(&self, link_rel: &str, target: &Path) -> PathBuf {
        let mut link = self.root.clone();
        for c in link_rel.split('/').filter(|s| !s.is_empty()) {
            link.push(c);
        }
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent).expect("symlink: create parent");
        }
        symlink(target, &link).expect("symlink");
        link
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn target(path: &str) -> WriteTarget {
    WriteTarget {
        path: path.into(),
        create_if_absent: false,
    }
}

// -------------------------------------------------------------------------
// Happy paths
// -------------------------------------------------------------------------

#[test]
fn happy_new_file_in_existing_parent_returns_already_exists_false() {
    let ws = TempWorkspace::new("happy_new");
    ws.mkdir_p("notes");
    let resolved = resolve_write_target(ws.root(), &target("notes/scratch.md"))
        .expect("happy new-file resolution");
    assert_eq!(resolved.parent, ws.root().join("notes"));
    assert_eq!(resolved.file_name, OsString::from("scratch.md"));
    assert_eq!(resolved.absolute, ws.root().join("notes/scratch.md"));
    assert!(!resolved.already_exists);
}

#[test]
fn happy_existing_regular_file_returns_already_exists_true() {
    let ws = TempWorkspace::new("happy_existing");
    ws.touch("notes/scratch.md");
    let resolved = resolve_write_target(ws.root(), &target("notes/scratch.md"))
        .expect("happy existing-file resolution");
    assert_eq!(resolved.absolute, ws.root().join("notes/scratch.md"));
    assert!(resolved.already_exists);
}

#[test]
fn happy_curdir_segment_is_normalized_away() {
    // `./notes/scratch.md` is structurally equivalent to
    // `notes/scratch.md` after Component normalization.
    let ws = TempWorkspace::new("happy_curdir");
    ws.mkdir_p("notes");
    let resolved = resolve_write_target(ws.root(), &target("./notes/scratch.md"))
        .expect("`./` prefix must be accepted");
    assert_eq!(resolved.absolute, ws.root().join("notes/scratch.md"));
}

// -------------------------------------------------------------------------
// Lexical refusals (no live filesystem needed beyond a workspace root)
// -------------------------------------------------------------------------

#[test]
fn absolute_path_is_refused() {
    let ws = TempWorkspace::new("abs");
    assert_eq!(
        resolve_write_target(ws.root(), &target("/etc/passwd")),
        Err(WriteTargetRefusal::PathEscape)
    );
}

#[test]
fn parent_traversal_is_refused() {
    let ws = TempWorkspace::new("parent_traversal");
    assert_eq!(
        resolve_write_target(ws.root(), &target("../escape.txt")),
        Err(WriteTargetRefusal::PathEscape)
    );
    assert_eq!(
        resolve_write_target(ws.root(), &target("notes/../../escape.txt")),
        Err(WriteTargetRefusal::PathEscape)
    );
}

#[test]
fn deny_component_anywhere_is_refused() {
    let ws = TempWorkspace::new("deny_component");
    for path in [".git/config", ".claw/state", ".claude/settings.json"] {
        assert_eq!(
            resolve_write_target(ws.root(), &target(path)),
            Err(WriteTargetRefusal::DenyComponent),
            "{path} should be refused via DenyComponent"
        );
    }
}

#[test]
fn deny_glob_filename_is_refused() {
    let ws = TempWorkspace::new("deny_glob");
    for path in [
        ".env",
        "config/.env.local",
        "secrets.yaml",
        "credentials.json",
        "tls/server.pem",
        "ssh/id_rsa.key",
    ] {
        assert_eq!(
            resolve_write_target(ws.root(), &target(path)),
            Err(WriteTargetRefusal::DenyGlobFilename),
            "{path} should be refused via DenyGlobFilename"
        );
    }
}

// -------------------------------------------------------------------------
// Filesystem-anchored refusals (require real dirs / files / symlinks)
// -------------------------------------------------------------------------

#[test]
fn parent_missing_is_refused() {
    let ws = TempWorkspace::new("parent_missing");
    // notes/ does NOT exist.
    assert_eq!(
        resolve_write_target(ws.root(), &target("notes/scratch.md")),
        Err(WriteTargetRefusal::ParentMissing)
    );
}

#[test]
fn parent_that_is_a_file_is_refused_as_parent_missing() {
    let ws = TempWorkspace::new("parent_is_file");
    ws.touch("notes"); // `notes` is a regular file, not a directory
    assert_eq!(
        resolve_write_target(ws.root(), &target("notes/scratch.md")),
        Err(WriteTargetRefusal::ParentMissing)
    );
}

#[test]
fn symlink_target_file_is_refused() {
    let ws = TempWorkspace::new("symlink_target");
    ws.touch("notes/real.md");
    // Create a symlink notes/link.md -> real.md
    ws.symlink_to("notes/link.md", Path::new("real.md"));
    assert_eq!(
        resolve_write_target(ws.root(), &target("notes/link.md")),
        Err(WriteTargetRefusal::SymlinkTarget)
    );
}

#[test]
fn symlink_parent_inside_workspace_is_refused() {
    // A symlinked parent dir is refused even if it points back inside
    // the workspace — symlinks anywhere in the chain are out.
    let ws = TempWorkspace::new("symlink_parent_in");
    ws.mkdir_p("real_dir");
    ws.symlink_to("alias_dir", Path::new("real_dir"));
    assert_eq!(
        resolve_write_target(ws.root(), &target("alias_dir/file.md")),
        Err(WriteTargetRefusal::SymlinkParent)
    );
}

#[test]
fn symlink_parent_pointing_outside_is_refused() {
    // Build two separate tempdirs: one is the workspace root, the other
    // is the symlink destination. A symlinked parent dir pointing to
    // the outside tempdir must be refused. Whether the symlink walk
    // catches it first (SymlinkParent) or the canonical prefix check
    // catches it (PathEscape) is acceptable; both refusals exit 6.
    let ws = TempWorkspace::new("symlink_parent_out_ws");
    let outside = TempWorkspace::new("symlink_parent_out_outside");
    ws.symlink_to("escape_dir", outside.root());
    match resolve_write_target(ws.root(), &target("escape_dir/file.md")) {
        Err(WriteTargetRefusal::SymlinkParent | WriteTargetRefusal::PathEscape) => {}
        other => panic!("expected SymlinkParent or PathEscape, got {other:?}"),
    }
}

#[test]
fn target_is_a_directory_is_refused() {
    let ws = TempWorkspace::new("target_is_dir");
    ws.mkdir_p("notes/subdir");
    // The target path is a directory, not a regular file.
    assert_eq!(
        resolve_write_target(ws.root(), &target("notes/subdir")),
        Err(WriteTargetRefusal::PathEscape)
    );
}

// -------------------------------------------------------------------------
// Exit code surface
// -------------------------------------------------------------------------

#[test]
fn every_refusal_exits_six() {
    let ws = TempWorkspace::new("exit_six");
    // Just need any refusal to fetch the exit code; PathEscape is
    // cheapest because it is lexical.
    let refusal = resolve_write_target(ws.root(), &target("../x")).unwrap_err();
    assert_eq!(refusal.exit_code(), EXIT_WRITE_PATH_REFUSED);
    assert_eq!(EXIT_WRITE_PATH_REFUSED, 6);
}

// -------------------------------------------------------------------------
// Drop helpers (sanity check; not exercising production code)
// -------------------------------------------------------------------------

#[test]
fn tempdir_guard_drops_cleanly_for_happy_path() {
    let path = {
        let ws = TempWorkspace::new("drop_guard");
        ws.touch("hello.md");
        assert!(ws.root().exists());
        ws.root().to_path_buf()
    };
    // Best-effort cleanup; the directory should be gone after drop.
    // We assert non-existence to catch regressions in the guard.
    assert!(!path.exists(), "drop guard should have removed {path:?}");
}
