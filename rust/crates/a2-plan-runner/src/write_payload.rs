//! A2-L2b workspace-write byte-authority object (slice 4a).
//!
//! Binds exact after-bytes to an approved
//! [`crate::diff_preview::PreviewRecord`] via a SHA-256 round-trip
//! check. The resulting [`ApprovedWritePayload`] is the **single**
//! in-memory carrier of raw write bytes between the producer that
//! built the preview and the future Slice-4 write executor. Bytes
//! never flow through [`crate::diff_preview::PreviewDisplay`], never
//! round-trip through a serialized preview bundle, never enter the
//! audit-marker stream, and never appear in any approval-result JSON.
//!
//! # Hard contract (slice 4a)
//!
//! - No filesystem writes anywhere in this module: nothing in this
//!   module opens a write-capable file handle, moves any path,
//!   deletes any file or directory, or builds any directory tree.
//! - No filesystem reads against the target: the constructor is
//!   purely in-memory. It does not stat, follow symlinks, or
//!   canonicalize the supplied path.
//! - No subprocesses, no broker, no model calls, no network.
//! - Not wired into the L1b runner's plan-executor entry point;
//!   nothing changes about the read-only runner.
//! - [`ApprovedWritePayload`] does **not** implement `Serialize` or
//!   `Deserialize`. Raw after-bytes never enter any on-disk artifact,
//!   any preview bundle, any approval-result envelope, any audit
//!   stream. The absence of derives is enforced by code review and
//!   the workspace scope audit; see the explicit assertion in
//!   [`crate::write_payload`]'s integration tests.
//! - [`ApprovedWritePayload`] has a manual `Debug` impl that omits
//!   the raw `after_bytes` field. The `Debug` impl prints metadata
//!   only; bytes are summarized as a length token.
//! - The constructor refuses any binding whose recomputed
//!   `sha256(after_bytes)` does not equal
//!   [`crate::diff_preview::PreviewRecord::after_sha256`]; the
//!   payload's existence is itself proof of binding.
//! - The constructor refuses any binding whose
//!   `target_relative_path` does not match
//!   [`crate::diff_preview::PreviewRecord::target_relative_path_sanitized`]
//!   verbatim. The sanitized field is part of the canonical subset
//!   that produces `preview_sha256` (slice 3a), so matching it pins
//!   the payload to the exact file the approval covers.
//! - The constructor refuses absolute paths and any `..` traversal in
//!   the target relative path, mirroring the L2a schema and the
//!   Slice-1 runtime path-safety resolver.
//!
//! # What lives elsewhere
//!
//! - Path safety against the live filesystem (symlinks, canonical
//!   prefix, parent existence): [`crate::write_runtime`].
//! - Pre-write baseline capture: [`crate::checkpoint`].
//! - Operator approval evaluation: [`crate::approval`].
//! - Preview / display rendering: [`crate::diff_preview`].
//! - Single-file write execution: A2-L2b slice 4 (not yet wired).

use std::path::{Component, Path, PathBuf};

use crate::diff_preview::{sha256_hex, PreviewRecord};

/// Hard upper bound on the byte length of an after-bytes payload bound
/// through [`bind_after_bytes`]. Mirrors
/// [`crate::checkpoint::MAX_CHECKPOINT_BYTES`] so a future Slice-4
/// executor never accepts a payload it could not roll back from the
/// matching checkpoint.
pub const MAX_APPROVED_PAYLOAD_BYTES: u64 = 16 * 1024 * 1024;

/// Hash-bound raw after-bytes for a single approved workspace-write.
///
/// Construct exclusively via [`bind_after_bytes`]. Its existence is
/// proof that:
///
/// 1. `sha256(after_bytes()) == after_sha256`,
/// 2. `target_relative_path` matches the approval-binding subset of
///    the originating
///    [`crate::diff_preview::PreviewRecord`] (cryptographically pinned
///    through `target_relative_path_sanitized` in the canonical
///    preview-hash pre-image),
/// 3. the originating record was approvable (no binary / redacted /
///    truncated flag set).
///
/// The struct is **not** `Serialize` / `Deserialize`. Raw bytes do
/// not flow through any on-disk artifact, preview bundle, audit
/// stream, or approval-result envelope. The manual `Debug` impl
/// prints metadata only — the raw `after_bytes` are omitted so
/// operator logs never leak candidate file contents.
pub struct ApprovedWritePayload {
    /// `PreviewRecord::step_id`, copied at bind time.
    pub step_id: String,
    /// Workspace-relative target path. Validated to be non-empty,
    /// non-absolute, free of `..` traversal, and an exact match to
    /// `record.target_relative_path_sanitized`.
    pub target_relative_path: PathBuf,
    /// `PreviewRecord::preview_id`, copied at bind time.
    pub preview_id: String,
    /// `PreviewRecord::preview_sha256`, copied at bind time.
    pub preview_sha256: String,
    /// `PreviewRecord::before_sha256`, copied at bind time.
    pub before_sha256: String,
    /// `PreviewRecord::after_sha256`, copied at bind time. Equal to
    /// `sha256(after_bytes())` by construction.
    pub after_sha256: String,
    /// `after_bytes().len() as u64`, stored explicitly for length
    /// cross-checks without a second pass through the buffer.
    pub after_size_bytes: u64,
    after_bytes: Vec<u8>,
}

impl ApprovedWritePayload {
    /// Read-only borrow of the bound after-bytes. The only path by
    /// which a Slice-4 write executor can obtain the raw bytes.
    #[must_use]
    pub fn after_bytes(&self) -> &[u8] {
        &self.after_bytes
    }
}

impl std::fmt::Debug for ApprovedWritePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovedWritePayload")
            .field("step_id", &self.step_id)
            .field("target_relative_path", &self.target_relative_path)
            .field("preview_id", &self.preview_id)
            .field("preview_sha256", &self.preview_sha256)
            .field("before_sha256", &self.before_sha256)
            .field("after_sha256", &self.after_sha256)
            .field("after_size_bytes", &self.after_size_bytes)
            .field(
                "after_bytes",
                &format_args!("<{} bytes elided>", self.after_size_bytes),
            )
            .finish()
    }
}

/// Why [`bind_after_bytes`] refused to mint an
/// [`ApprovedWritePayload`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    /// Recomputed `sha256(after_bytes)` did not equal
    /// `record.after_sha256`. The payload is the wrong content for
    /// the approved preview.
    PayloadHashMismatch { expected: String, actual: String },
    /// The originating preview is non-approvable
    /// (`is_binary || is_redacted || is_truncated`). Slice 3a refuses
    /// approval on those branches; Slice 4a refuses binding too,
    /// defense in depth.
    PreviewNotApprovable,
    /// `target_relative_path` did not match
    /// `record.target_relative_path_sanitized`. The bytes may still
    /// be correct for some file, but not this approved one.
    TargetPathMismatch,
    /// `target_relative_path` was empty, absolute, contained `..`,
    /// or contained an OS-specific prefix.
    InvalidTargetPath,
    /// `after_bytes.len() > MAX_APPROVED_PAYLOAD_BYTES`. Mirrors the
    /// checkpoint store cap so the executor never accepts a payload
    /// that could not be rolled back.
    PayloadTooLarge { actual: u64, cap: u64 },
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PayloadHashMismatch { expected, actual } => write!(
                f,
                "payload hash mismatch: expected after_sha256={expected}, actual={actual}"
            ),
            Self::PreviewNotApprovable => f.write_str(
                "preview is non-approvable (is_binary, is_redacted, or is_truncated)",
            ),
            Self::TargetPathMismatch => f.write_str(
                "target_relative_path does not match record.target_relative_path_sanitized",
            ),
            Self::InvalidTargetPath => f.write_str(
                "target_relative_path is empty, absolute, contains '..' traversal, or has an OS prefix",
            ),
            Self::PayloadTooLarge { actual, cap } => {
                write!(f, "payload is {actual} bytes, exceeds cap {cap}")
            }
        }
    }
}

impl std::error::Error for BindError {}

/// Bind raw `after_bytes` to an approved
/// [`crate::diff_preview::PreviewRecord`].
///
/// Returns [`ApprovedWritePayload`] only when ALL of the following
/// hold:
///
/// 1. `record.is_approvable()` (no binary/redacted/truncated flag).
/// 2. `target_relative_path` is non-empty, non-absolute, free of
///    `..` traversal, and has no OS-specific prefix.
/// 3. `target_relative_path.to_string_lossy() ==
///    record.target_relative_path_sanitized`. The sanitized field is
///    part of the canonical preview-hash pre-image, so a matching
///    path is cryptographically pinned against `preview_sha256`.
/// 4. `after_bytes.len() as u64 <= MAX_APPROVED_PAYLOAD_BYTES`.
/// 5. `sha256(after_bytes) == record.after_sha256`.
///
/// No filesystem access of any kind. No subprocess. No network. The
/// function consumes `target_relative_path` and `after_bytes` so the
/// payload owns its bytes; the caller cannot retain a parallel
/// reference into the buffer after a successful bind.
///
/// # Errors
///
/// See [`BindError`]. Refusals are checked in this order: preview
/// approvability → lexical path safety → target identity match →
/// size cap → hash match.
pub fn bind_after_bytes(
    record: &PreviewRecord,
    target_relative_path: PathBuf,
    after_bytes: Vec<u8>,
) -> Result<ApprovedWritePayload, BindError> {
    // 1. Preview must be approvable.
    if !record.is_approvable() {
        return Err(BindError::PreviewNotApprovable);
    }

    // 2. Lexical path safety on the caller-supplied PathBuf.
    refuse_unsafe_relative_path(&target_relative_path)?;

    // 3. Target identity match against the record's sanitized
    //    relative path. The sanitized form is part of the canonical
    //    subset that produces preview_sha256; matching it here pins
    //    the payload to the exact file the approval covers.
    if target_relative_path.to_string_lossy() != record.target_relative_path_sanitized {
        return Err(BindError::TargetPathMismatch);
    }

    // 4. Size cap (cheap check before the hash pass).
    // `usize` always fits in `u64` on the runner's supported
    // 64-bit Unix targets; the `as` cast is lossless there.
    let len_u64 = after_bytes.len() as u64;
    if len_u64 > MAX_APPROVED_PAYLOAD_BYTES {
        return Err(BindError::PayloadTooLarge {
            actual: len_u64,
            cap: MAX_APPROVED_PAYLOAD_BYTES,
        });
    }

    // 5. Hash check.
    let actual = sha256_hex(&after_bytes);
    if actual != record.after_sha256 {
        return Err(BindError::PayloadHashMismatch {
            expected: record.after_sha256.clone(),
            actual,
        });
    }

    Ok(ApprovedWritePayload {
        step_id: record.step_id.clone(),
        target_relative_path,
        preview_id: record.preview_id.clone(),
        preview_sha256: record.preview_sha256.clone(),
        before_sha256: record.before_sha256.clone(),
        after_sha256: record.after_sha256.clone(),
        after_size_bytes: len_u64,
        after_bytes,
    })
}

fn refuse_unsafe_relative_path(p: &Path) -> Result<(), BindError> {
    if p.as_os_str().is_empty() {
        return Err(BindError::InvalidTargetPath);
    }
    if p.is_absolute() {
        return Err(BindError::InvalidTargetPath);
    }
    for component in p.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BindError::InvalidTargetPath);
            }
            Component::Normal(_) | Component::CurDir => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_approved_payload_bytes_pin() {
        assert_eq!(MAX_APPROVED_PAYLOAD_BYTES, 16 * 1024 * 1024);
    }

    #[test]
    fn refuse_unsafe_relative_path_rejects_empty() {
        let res = refuse_unsafe_relative_path(Path::new(""));
        assert_eq!(res, Err(BindError::InvalidTargetPath));
    }

    #[test]
    fn refuse_unsafe_relative_path_rejects_absolute() {
        let res = refuse_unsafe_relative_path(Path::new("/etc/passwd"));
        assert_eq!(res, Err(BindError::InvalidTargetPath));
    }

    #[test]
    fn refuse_unsafe_relative_path_rejects_parent_traversal() {
        let res = refuse_unsafe_relative_path(Path::new("../escape.txt"));
        assert_eq!(res, Err(BindError::InvalidTargetPath));
    }

    #[test]
    fn refuse_unsafe_relative_path_rejects_mid_path_traversal() {
        let res = refuse_unsafe_relative_path(Path::new("src/../etc/x"));
        assert_eq!(res, Err(BindError::InvalidTargetPath));
    }

    #[test]
    fn refuse_unsafe_relative_path_accepts_normal_relative() {
        for ok in ["src/lib.rs", "docs/notes/scratch.md", "a/b/c/d.txt"] {
            let res = refuse_unsafe_relative_path(Path::new(ok));
            assert!(res.is_ok(), "expected {ok} to be accepted, got {res:?}");
        }
    }

    #[test]
    fn refuse_unsafe_relative_path_accepts_curdir_segments() {
        // `./foo/bar.rs` is fine; CurDir is a no-op in path semantics.
        let res = refuse_unsafe_relative_path(Path::new("./src/lib.rs"));
        assert!(res.is_ok(), "got {res:?}");
    }

    #[test]
    fn bind_error_display_includes_short_cause() {
        let e = BindError::PreviewNotApprovable;
        let s = format!("{e}");
        assert!(s.contains("non-approvable"), "got {s:?}");

        let e = BindError::TargetPathMismatch;
        let s = format!("{e}");
        assert!(s.contains("target_relative_path"), "got {s:?}");

        let e = BindError::InvalidTargetPath;
        let s = format!("{e}");
        assert!(
            s.contains("absolute") || s.contains("traversal") || s.contains("empty"),
            "got {s:?}"
        );

        let e = BindError::PayloadTooLarge { actual: 1, cap: 0 };
        let s = format!("{e}");
        assert!(s.contains("exceeds cap"), "got {s:?}");

        let e = BindError::PayloadHashMismatch {
            expected: "a".into(),
            actual: "b".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("hash mismatch"), "got {s:?}");
    }

    #[test]
    fn approved_payload_debug_omits_raw_bytes() {
        // Construct directly (private field access is fine inside the
        // crate's test module) to exercise the Debug impl without
        // going through the public constructor.
        let payload = ApprovedWritePayload {
            step_id: "step-1".into(),
            target_relative_path: PathBuf::from("src/lib.rs"),
            preview_id: "pid".into(),
            preview_sha256: "p".repeat(64),
            before_sha256: "b".repeat(64),
            after_sha256: "a".repeat(64),
            after_size_bytes: 5,
            after_bytes: b"hello".to_vec(),
        };
        let dbg = format!("{payload:?}");
        // Metadata fields appear.
        assert!(dbg.contains("ApprovedWritePayload"));
        assert!(dbg.contains("after_size_bytes"));
        // Bytes are summarized, not printed.
        assert!(dbg.contains("bytes elided"));
        // The literal payload "hello" must not leak.
        assert!(!dbg.contains("hello"), "Debug leaked raw bytes: {dbg}");
    }
}
