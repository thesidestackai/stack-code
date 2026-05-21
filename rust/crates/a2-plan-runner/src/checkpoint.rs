//! A2-L2b checkpoint store (slice 2).
//!
//! Captures the pre-write state of a single workspace target so that a
//! future slice can implement approval, atomic write, and rollback. The
//! checkpoint is written under a runner-owned root anchored at
//!
//! ```text
//! <workspace_root>/.claw/l2b-checkpoints/<plan-run-id>/<step-id>/
//! ```
//!
//! Every checkpoint directory carries a `manifest.json` and, for existing
//! regular-file targets, a `before.bin` byte-exact copy. Absent targets
//! record `pre_existed = false` and omit `before.bin`.
//!
//! # Hard contract (slice 2)
//!
//! - Never mutates any operator target file. The target is read with
//!   [`std::fs::metadata`] and (for existing regular files) opened
//!   read-only; the bytes are streamed into the checkpoint store while
//!   being SHA-256 hashed.
//! - Writes only inside `<workspace_root>/.claw/l2b-checkpoints/`.
//! - Refuses overwrite at the leaf step directory level: a second
//!   [`CheckpointStore::create_checkpoint`] call with the same
//!   `(run_id, step_id)` pair returns [`CheckpointError::AlreadyExists`].
//! - Refuses target files larger than [`MAX_CHECKPOINT_BYTES`] with
//!   [`CheckpointError::TargetTooLarge`].
//! - Refuses targets that exist but are not regular files.
//! - No subprocesses, no broker, no model calls, no approval prompts,
//!   no diff preview, no rollback execution, no wiring into `run_plan`.
//! - Operator approval is required for the dependency set
//!   `{sha2, ulid, serde_json}`; nothing else is pulled in.
//!
//! # Permissions
//!
//! On Unix, every directory created by the store is mode `0o700` and
//! every file is mode `0o600`. On platforms without
//! [`std::os::unix::fs::PermissionsExt`], permissions are left at the
//! OS default — best-effort.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::markers;

/// Exit code reserved for checkpoint-store write failures. Fits the
/// L1b/L2b family without collision: 0–4 (runner outcomes), 5 (parse),
/// 6 (L2b path safety), 7 and 8 reserved for approval / rollback in
/// later slices, 9 (this).
pub const EXIT_CHECKPOINT_FAILED: i32 = 9;

/// Hard upper bound on the size of a target file we are willing to
/// snapshot byte-for-byte. Above this, [`CheckpointStore::create_checkpoint`]
/// returns [`CheckpointError::TargetTooLarge`] and the marker
/// [`markers::L2B_CHECKPOINT_TOO_LARGE`] applies. Future slices may
/// introduce a streaming/sidecar regime for larger files; slice 2 does
/// not.
pub const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;

/// Workspace-relative checkpoint root. Joined with `workspace_root` to
/// produce the per-run directory tree.
pub const CHECKPOINT_ROOT_REL: &str = ".claw/l2b-checkpoints";

/// Manifest schema version. Bumping this is a breaking change for any
/// downstream consumer that reads `manifest.json`; slice 2 ships v1.
pub const MANIFEST_VERSION: u32 = 1;

/// I/O buffer used while streaming target bytes into `before.bin` and
/// the SHA-256 hasher. 64 KiB matches the libc `BUFSIZ` ballpark and is
/// large enough that the per-call syscall overhead is negligible
/// without committing the test harness to large allocations.
const COPY_BUF_BYTES: usize = 64 * 1024;

/// Maximum step-id length. The validation regex is `^[A-Za-z0-9_.-]{1,128}$`
/// with explicit refusal of bare `"."` and `".."`.
const STEP_ID_MAX_LEN: usize = 128;

// =========================================================================
// Public API: CheckpointStore + helpers
// =========================================================================

/// A per-plan-run checkpoint store anchored at one workspace root.
///
/// One [`CheckpointStore`] per plan run; reuse it across every step in
/// that run. The store owns the [`Ulid`] that identifies the run on
/// disk and in the operator marker stream.
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    workspace_root: PathBuf,
    run_id: Ulid,
}

impl CheckpointStore {
    /// Construct a store with a freshly-generated ULID.
    ///
    /// The workspace root is stored as given; the caller is expected to
    /// pass an already-canonicalized path (typically the same root used
    /// by [`crate::write_runtime::resolve_write_target`]).
    #[must_use]
    pub fn new_with_generated_run_id(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            run_id: Ulid::new(),
        }
    }

    /// Construct a store with an explicit ULID. Useful for tests and for
    /// resuming a plan run when slice-N adds resume semantics.
    #[must_use]
    pub fn new_with_run_id(workspace_root: PathBuf, run_id: Ulid) -> Self {
        Self {
            workspace_root,
            run_id,
        }
    }

    /// The ULID identifying this run on disk and in the marker stream.
    #[must_use]
    pub fn run_id(&self) -> &Ulid {
        &self.run_id
    }

    /// The workspace-root-anchored run directory:
    /// `<workspace_root>/.claw/l2b-checkpoints/<run-id>/`.
    #[must_use]
    pub fn run_dir(&self) -> PathBuf {
        let mut p = self.workspace_root.clone();
        p.push(CHECKPOINT_ROOT_REL);
        p.push(self.run_id.to_string());
        p
    }

    /// The per-step leaf directory under the run directory. Validates
    /// `step_id` but does not touch the filesystem.
    pub fn step_dir(&self, step_id: &str) -> Result<PathBuf, CheckpointError> {
        validate_step_id(step_id)?;
        let mut p = self.run_dir();
        p.push(step_id);
        Ok(p)
    }

    /// Capture a pre-write checkpoint for `target_absolute`.
    ///
    /// On success, returns a [`CheckpointHandle`] pointing at the leaf
    /// directory, the manifest, and (for existing regular-file targets)
    /// the `before.bin` snapshot. The target file is **not** modified.
    ///
    /// # Errors
    ///
    /// See [`CheckpointError`]. On any error returned after the leaf
    /// step directory was created, the partial directory is removed
    /// best-effort so a retry with the same `(run_id, step_id)` does
    /// not trip the overwrite refusal.
    pub fn create_checkpoint(
        &self,
        step_id: &str,
        target_absolute: &Path,
        target_relative: &Path,
    ) -> Result<CheckpointHandle, CheckpointError> {
        validate_step_id(step_id)?;

        // 1. Inspect the target before any directory creation; refuse
        //    oversize / non-regular without leaving any state on disk.
        let target_state = inspect_target(target_absolute)?;

        // 2. Create the run + step directory chain. The parent chain is
        //    idempotent; the leaf is exclusive.
        let step_dir = self.create_step_dir(step_id)?;

        // 3. Capture bytes (if any) + hash, write manifest, surface the
        //    handle. Any error from this block triggers best-effort
        //    cleanup of the leaf directory.
        match self.populate_step_dir(
            &step_dir,
            step_id,
            target_absolute,
            target_relative,
            &target_state,
        ) {
            Ok(handle) => Ok(handle),
            Err(e) => {
                let _ = fs::remove_dir_all(&step_dir);
                Err(e)
            }
        }
    }

    // ---- internal helpers ------------------------------------------------

    fn create_step_dir(&self, step_id: &str) -> Result<PathBuf, CheckpointError> {
        // Parent chain: workspace_root/.claw, .../l2b-checkpoints,
        // .../l2b-checkpoints/<run-id>. Idempotent (`recursive(true)`).
        let run_dir = self.run_dir();
        create_dir_recursive_0700(&run_dir)?;

        // Leaf: refuse overwrite. `create_dir` errors `AlreadyExists`
        // when the path exists.
        let step_dir = {
            let mut p = run_dir;
            p.push(step_id);
            p
        };
        match create_dir_exclusive_0700(&step_dir) {
            Ok(()) => Ok(step_dir),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                Err(CheckpointError::AlreadyExists { step_dir })
            }
            Err(e) => Err(CheckpointError::Io(e)),
        }
    }

    fn populate_step_dir(
        &self,
        step_dir: &Path,
        step_id: &str,
        target_absolute: &Path,
        target_relative: &Path,
        target_state: &TargetState,
    ) -> Result<CheckpointHandle, CheckpointError> {
        // 3a. For existing regular files, stream-copy into before.bin
        //     via a 64 KiB buffer while updating a SHA-256 hasher in
        //     the same loop. Atomic rename via `before.bin.tmp`.
        let (before_bin_path, pre_sha256) = match target_state {
            TargetState::Absent => (None, String::new()),
            TargetState::Regular { .. } => {
                let bin_path = step_dir.join("before.bin");
                let tmp_path = step_dir.join("before.bin.tmp");
                let hash_hex = stream_copy_and_hash(target_absolute, &tmp_path)
                    .map_err(CheckpointError::Io)?;
                fs::rename(&tmp_path, &bin_path).map_err(CheckpointError::Io)?;
                (Some(bin_path), hash_hex)
            }
        };

        // 3b. Build manifest.
        let (created_secs, created_nanos) = now_unix_parts().map_err(CheckpointError::Io)?;
        let manifest = Manifest {
            manifest_version: MANIFEST_VERSION,
            plan_run_id: self.run_id.to_string(),
            step_id: step_id.to_string(),
            target_relative_path: target_relative.display().to_string(),
            target_absolute_path: target_absolute.display().to_string(),
            pre_existed: matches!(target_state, TargetState::Regular { .. }),
            pre_target_kind: match target_state {
                TargetState::Absent => "absent".to_string(),
                TargetState::Regular { .. } => "regular_file".to_string(),
            },
            pre_size_bytes: match target_state {
                TargetState::Absent => 0,
                TargetState::Regular { size, .. } => *size,
            },
            pre_sha256,
            pre_mtime_unix_ns: match target_state {
                TargetState::Absent => None,
                TargetState::Regular { mtime_ns, .. } => *mtime_ns,
            },
            pre_permissions_octal: match target_state {
                TargetState::Absent => None,
                TargetState::Regular { mode_octal, .. } => mode_octal.clone(),
            },
            created_at_utc: format_rfc3339_z(created_secs, created_nanos),
            runner_crate_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // 3c. Atomic manifest write.
        let manifest_path = step_dir.join("manifest.json");
        let tmp_manifest = step_dir.join("manifest.json.tmp");
        let json = serde_json::to_vec_pretty(&manifest).map_err(io_from_json)?;
        write_file_0600(&tmp_manifest, &json).map_err(CheckpointError::Io)?;
        fs::rename(&tmp_manifest, &manifest_path).map_err(CheckpointError::Io)?;

        Ok(CheckpointHandle {
            step_dir: step_dir.to_path_buf(),
            manifest_path,
            before_bin_path,
            manifest,
        })
    }
}

/// Result of a successful [`CheckpointStore::create_checkpoint`].
#[derive(Debug, Clone)]
pub struct CheckpointHandle {
    pub step_dir: PathBuf,
    pub manifest_path: PathBuf,
    /// `Some(...)` iff the target existed as a regular file at
    /// checkpoint time; `None` for the absent branch.
    pub before_bin_path: Option<PathBuf>,
    pub manifest: Manifest,
}

/// On-disk manifest schema. Version `1` — bumping is a breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub plan_run_id: String,
    pub step_id: String,
    pub target_relative_path: String,
    pub target_absolute_path: String,
    pub pre_existed: bool,
    /// `"absent"` | `"regular_file"`.
    pub pre_target_kind: String,
    pub pre_size_bytes: u64,
    /// Lowercase hex SHA-256, 64 chars, or the empty string for the
    /// absent branch.
    pub pre_sha256: String,
    pub pre_mtime_unix_ns: Option<i128>,
    /// Four-octal-digit Unix permission bits (`mode & 0o7777`), e.g.
    /// `"0644"`. `None` on non-Unix or when metadata lookup failed.
    pub pre_permissions_octal: Option<String>,
    /// RFC 3339 UTC string with nanosecond fractional precision and
    /// trailing `Z`, e.g. `"2026-05-20T22:43:23.123456789Z"`.
    pub created_at_utc: String,
    /// `env!("CARGO_PKG_VERSION")` at compile time.
    pub runner_crate_version: String,
}

/// Refusal arm for [`CheckpointStore::create_checkpoint`].
#[derive(Debug)]
pub enum CheckpointError {
    /// The leaf `<run-id>/<step-id>/` directory already existed at
    /// checkpoint time. Surface to operators as a runtime guarantee
    /// that the store will never silently clobber an earlier snapshot.
    AlreadyExists { step_dir: PathBuf },
    /// The target file's pre-write size exceeded [`MAX_CHECKPOINT_BYTES`].
    /// Carries a dedicated marker.
    TargetTooLarge { actual: u64, cap: u64 },
    /// Step-id failed the `^[A-Za-z0-9_.-]{1,128}$` shape check or was
    /// bare `"."` / `".."`.
    StepIdInvalid { step_id: String },
    /// Any underlying I/O failure: target is a directory or other
    /// non-regular file, mkdir failed for non-AlreadyExists reasons,
    /// rename/write failed, etc.
    Io(io::Error),
}

impl CheckpointError {
    /// CLI exit code for any checkpoint refusal. Pinned at
    /// [`EXIT_CHECKPOINT_FAILED`] (`9`); slice 2 does not subdivide.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        EXIT_CHECKPOINT_FAILED
    }

    /// Stable operator-facing marker for this refusal. The size-cap
    /// arm gets its own token; everything else is the generic
    /// `a2-l2b-checkpoint-refused`.
    #[must_use]
    pub fn marker(&self) -> &'static str {
        match self {
            Self::TargetTooLarge { .. } => markers::L2B_CHECKPOINT_TOO_LARGE,
            _ => markers::L2B_CHECKPOINT_REFUSED,
        }
    }
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists { step_dir } => {
                write!(
                    f,
                    "checkpoint step dir already exists: {}",
                    step_dir.display()
                )
            }
            Self::TargetTooLarge { actual, cap } => {
                write!(
                    f,
                    "target file is {actual} bytes, exceeds checkpoint cap {cap}"
                )
            }
            Self::StepIdInvalid { step_id } => {
                write!(f, "invalid step_id: {step_id:?}")
            }
            Self::Io(e) => write!(f, "checkpoint io error: {e}"),
        }
    }
}

impl std::error::Error for CheckpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

// =========================================================================
// Internal: target inspection
// =========================================================================

/// Snapshot of a target's pre-write filesystem state. Computed once at
/// the start of `create_checkpoint` to keep the rest of the function
/// free of conditional metadata lookups.
enum TargetState {
    Absent,
    Regular {
        size: u64,
        mtime_ns: Option<i128>,
        mode_octal: Option<String>,
    },
}

fn inspect_target(target: &Path) -> Result<TargetState, CheckpointError> {
    let md = match fs::metadata(target) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(TargetState::Absent),
        Err(e) => return Err(CheckpointError::Io(e)),
    };
    if !md.is_file() {
        return Err(CheckpointError::Io(io::Error::new(
            ErrorKind::InvalidInput,
            "target exists but is not a regular file",
        )));
    }
    let size = md.len();
    if size > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::TargetTooLarge {
            actual: size,
            cap: MAX_CHECKPOINT_BYTES,
        });
    }
    let mtime_ns = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()));
    let mode_octal = unix_mode_octal(&md);
    Ok(TargetState::Regular {
        size,
        mtime_ns,
        mode_octal,
    })
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)] // platform-uniform Option signature
fn unix_mode_octal(md: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = md.permissions().mode() & 0o7777;
    Some(format!("{mode:04o}"))
}

#[cfg(not(unix))]
fn unix_mode_octal(_md: &fs::Metadata) -> Option<String> {
    None
}

// =========================================================================
// Internal: filesystem primitives
// =========================================================================

fn create_dir_recursive_0700(path: &Path) -> Result<(), CheckpointError> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(CheckpointError::Io)
}

fn create_dir_exclusive_0700(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn write_file_0600(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    // Best-effort fsync. Non-Unix platforms may not support sync_all
    // on regular files in all cases, so we ignore the error rather
    // than failing the checkpoint over a flush issue.
    let _ = f.sync_all();
    Ok(())
}

/// Stream the bytes of `src` into `dst` while updating a SHA-256
/// hasher; return the 64-char lowercase hex digest.
fn stream_copy_and_hash(src: &Path, dst: &Path) -> io::Result<String> {
    let mut input = File::open(src)?;
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut output = opts.open(dst)?;
    let mut hasher = Sha256::new();
    // Heap-allocated to keep the per-call stack footprint flat regardless
    // of COPY_BUF_BYTES.
    let mut buf = vec![0u8; COPY_BUF_BYTES];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        output.write_all(&buf[..n])?;
    }
    let _ = output.sync_all();
    Ok(sha256_hex_lowercase(&hasher.finalize()))
}

fn sha256_hex_lowercase(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

// =========================================================================
// Internal: step_id validation
// =========================================================================

fn validate_step_id(step_id: &str) -> Result<(), CheckpointError> {
    let invalid = || CheckpointError::StepIdInvalid {
        step_id: step_id.to_string(),
    };
    if step_id.is_empty() || step_id.len() > STEP_ID_MAX_LEN {
        return Err(invalid());
    }
    if step_id == "." || step_id == ".." {
        return Err(invalid());
    }
    if !step_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(invalid());
    }
    Ok(())
}

// =========================================================================
// Internal: time formatting (no chrono / no time crate)
// =========================================================================

fn now_unix_parts() -> io::Result<(u64, u32)> {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok((d.as_secs(), d.subsec_nanos()))
}

/// Format a UNIX-epoch instant as an RFC 3339 UTC string with
/// nanosecond fractional precision: `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ`.
/// Calendar conversion via Howard Hinnant's `civil_from_days` (public
/// domain). Falls back to `1970-01-01T00:00:00.000000000Z` if `secs`
/// is too large to convert to `i64` (unreachable for any plausible
/// system clock).
fn format_rfc3339_z(secs: u64, nanos: u32) -> String {
    let days_u = secs / 86_400;
    let days = i64::try_from(days_u).unwrap_or(0);
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day / 60) % 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}

/// Convert days-since-Unix-epoch (1970-01-01 == day 0) into a proleptic
/// Gregorian (year, month, day). Public-domain algorithm by Howard
/// Hinnant; correctness range covers all years a 64-bit `secs` can
/// represent.
///
/// All intermediate arithmetic is signed; the algorithm guarantees
/// `doe ∈ [0, 146_096]`, `mp ∈ [0, 11]`, `d ∈ [1, 31]`, and
/// `m ∈ [1, 12]`, so the boundary `TryFrom` calls are infallible in
/// practice and fall back to safe defaults if the algorithm is ever
/// fed inputs outside its design range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe: i64 = z - era * 146_097; // [0, 146_096]
    let yoe: i64 = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y: i64 = yoe + era * 400;
    let doy: i64 = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp: i64 = (5 * doy + 2) / 153; // [0, 11]
    let d_i64 = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m_i64 = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let d = u32::try_from(d_i64).unwrap_or(1);
    let m = u32::try_from(m_i64).unwrap_or(1);
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

// =========================================================================
// Internal: misc
// =========================================================================

fn io_from_json(e: serde_json::Error) -> CheckpointError {
    CheckpointError::Io(io::Error::other(e))
}

// =========================================================================
// Unit tests (pure helpers + invariants)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- step_id validation table -----------------------------------------

    #[test]
    fn step_id_accepts_alphanumeric_and_punct() {
        for ok in [
            "a",
            "step-1",
            "step_1",
            "step.1",
            "ABC",
            "123",
            "a-b_c.d",
            &"x".repeat(128),
        ] {
            assert!(
                validate_step_id(ok).is_ok(),
                "expected {ok:?} to be a valid step_id"
            );
        }
    }

    #[test]
    fn step_id_refuses_invalid_shapes() {
        let too_long = "x".repeat(129);
        for bad in [
            "", ".", "..", "a/b", "a b", "a\nb", "a\tb", "a:b", "a*b", "a?b", "a\\b", "a~b", "a@b",
            &too_long,
        ] {
            assert!(
                validate_step_id(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn step_id_refuses_nul_byte() {
        assert!(validate_step_id("a\0b").is_err());
    }

    // ---- manifest JSON round-trip -----------------------------------------

    #[test]
    fn manifest_round_trips_through_json() {
        let m = Manifest {
            manifest_version: MANIFEST_VERSION,
            plan_run_id: Ulid::new().to_string(),
            step_id: "step-1".into(),
            target_relative_path: "docs/README.md".into(),
            target_absolute_path: "/tmp/x/docs/README.md".into(),
            pre_existed: true,
            pre_target_kind: "regular_file".into(),
            pre_size_bytes: 42,
            pre_sha256: "0".repeat(64),
            pre_mtime_unix_ns: Some(1_700_000_000_000_000_000),
            pre_permissions_octal: Some("0644".into()),
            created_at_utc: "2026-05-20T22:43:23.000000000Z".into(),
            runner_crate_version: env!("CARGO_PKG_VERSION").into(),
        };
        let json = serde_json::to_string_pretty(&m).expect("serialize");
        let parsed: Manifest = serde_json::from_str(&json).expect("parse");
        assert_eq!(m, parsed);
    }

    #[test]
    fn manifest_absent_branch_uses_empty_sha_and_no_optionals() {
        let m = Manifest {
            manifest_version: MANIFEST_VERSION,
            plan_run_id: Ulid::new().to_string(),
            step_id: "step-1".into(),
            target_relative_path: "docs/missing.md".into(),
            target_absolute_path: "/tmp/x/docs/missing.md".into(),
            pre_existed: false,
            pre_target_kind: "absent".into(),
            pre_size_bytes: 0,
            pre_sha256: String::new(),
            pre_mtime_unix_ns: None,
            pre_permissions_octal: None,
            created_at_utc: "2026-05-20T22:43:23.000000000Z".into(),
            runner_crate_version: env!("CARGO_PKG_VERSION").into(),
        };
        let json = serde_json::to_string(&m).expect("serialize");
        let parsed: Manifest = serde_json::from_str(&json).expect("parse");
        assert_eq!(m, parsed);
        assert!(json.contains("\"pre_mtime_unix_ns\":null"));
        assert!(json.contains("\"pre_permissions_octal\":null"));
        assert!(json.contains("\"pre_sha256\":\"\""));
    }

    // ---- constant pinning -------------------------------------------------

    #[test]
    fn max_checkpoint_bytes_pin() {
        assert_eq!(MAX_CHECKPOINT_BYTES, 16 * 1024 * 1024);
    }

    #[test]
    fn exit_checkpoint_failed_is_nine() {
        assert_eq!(EXIT_CHECKPOINT_FAILED, 9);
    }

    #[test]
    fn manifest_version_pin() {
        assert_eq!(MANIFEST_VERSION, 1);
    }

    #[test]
    fn checkpoint_root_rel_pin() {
        assert_eq!(CHECKPOINT_ROOT_REL, ".claw/l2b-checkpoints");
    }

    // ---- ULIDs ------------------------------------------------------------

    #[test]
    fn generated_ulids_differ_and_parse() {
        let a = CheckpointStore::new_with_generated_run_id(PathBuf::from("/tmp/x")).run_id;
        let b = CheckpointStore::new_with_generated_run_id(PathBuf::from("/tmp/x")).run_id;
        assert_ne!(a, b);
        let s_a = a.to_string();
        let parsed: Ulid = s_a.parse().expect("ULID parses");
        assert_eq!(parsed, a);
        assert_eq!(s_a.len(), 26);
    }

    // ---- CheckpointError::marker mapping ----------------------------------

    #[test]
    fn marker_for_target_too_large() {
        let e = CheckpointError::TargetTooLarge {
            actual: 1,
            cap: MAX_CHECKPOINT_BYTES,
        };
        assert_eq!(e.marker(), markers::L2B_CHECKPOINT_TOO_LARGE);
        assert_eq!(e.exit_code(), EXIT_CHECKPOINT_FAILED);
    }

    #[test]
    fn marker_for_other_errors_is_generic_refused() {
        let mk_io = || CheckpointError::Io(io::Error::new(ErrorKind::InvalidInput, "x"));
        for e in [
            CheckpointError::AlreadyExists {
                step_dir: PathBuf::from("/tmp/x"),
            },
            CheckpointError::StepIdInvalid {
                step_id: "bad".into(),
            },
            mk_io(),
        ] {
            assert_eq!(e.marker(), markers::L2B_CHECKPOINT_REFUSED);
            assert_eq!(e.exit_code(), EXIT_CHECKPOINT_FAILED);
        }
    }

    // ---- sha256 hex -------------------------------------------------------

    #[test]
    fn sha256_hex_of_empty_input() {
        // Standard SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = Sha256::new();
        let digest = h.finalize();
        let s = sha256_hex_lowercase(&digest);
        assert_eq!(
            s,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(s.len(), 64);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    // ---- date formatting --------------------------------------------------

    #[test]
    fn format_rfc3339_z_epoch() {
        assert_eq!(format_rfc3339_z(0, 0), "1970-01-01T00:00:00.000000000Z");
    }

    #[test]
    fn format_rfc3339_z_known_date() {
        // 2026-05-20T22:43:23.123456789Z
        // 2026-05-20 is 20_593 days after 1970-01-01:
        //   1970-01-01 -> 2026-01-01 = 56*365 + 14 leap days = 20_454
        //   + 31 (Jan) + 28 (Feb, non-leap) + 31 (Mar) + 30 (Apr) + 19 = 139
        //   = 20_593
        // 22:43:23 = 22*3600 + 43*60 + 23 = 81_803.
        let secs = 20_593_u64 * 86_400 + 81_803;
        let s = format_rfc3339_z(secs, 123_456_789);
        assert_eq!(s, "2026-05-20T22:43:23.123456789Z");
    }

    #[test]
    fn format_rfc3339_z_leap_day() {
        // 2024-02-29T00:00:00.000000000Z
        // Days from 1970-01-01 to 2024-02-29: 19_782.
        let secs = 19_782_u64 * 86_400;
        let s = format_rfc3339_z(secs, 0);
        assert_eq!(s, "2024-02-29T00:00:00.000000000Z");
    }

    // ---- run_dir / step_dir path shape ------------------------------------

    #[test]
    fn run_dir_anchors_to_workspace_root() {
        let id = Ulid::new();
        let store = CheckpointStore::new_with_run_id(PathBuf::from("/tmp/ws"), id);
        let rd = store.run_dir();
        let expected = PathBuf::from("/tmp/ws/.claw/l2b-checkpoints").join(id.to_string());
        assert_eq!(rd, expected);
    }

    #[test]
    fn step_dir_appends_step_id() {
        let id = Ulid::new();
        let store = CheckpointStore::new_with_run_id(PathBuf::from("/tmp/ws"), id);
        let sd = store.step_dir("step-1").expect("valid");
        let expected = PathBuf::from("/tmp/ws/.claw/l2b-checkpoints")
            .join(id.to_string())
            .join("step-1");
        assert_eq!(sd, expected);
    }

    #[test]
    fn step_dir_refuses_invalid_id() {
        let store = CheckpointStore::new_with_generated_run_id(PathBuf::from("/tmp/ws"));
        for bad in ["", ".", "..", "a/b"] {
            match store.step_dir(bad) {
                Err(CheckpointError::StepIdInvalid { step_id }) => {
                    assert_eq!(step_id, bad);
                }
                other => panic!("expected StepIdInvalid for {bad:?}, got {other:?}"),
            }
        }
    }
}
