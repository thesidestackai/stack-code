//! A2-L2b single-file write executor (slice 4).
//!
//! First mutation-capable A2-L2b slice. Applies exactly one approved
//! workspace-write under the full authority chain assembled by slices
//! 1-4a:
//!
//! - [`crate::write_runtime::ResolvedWriteTarget`] — canonical, parent-
//!   anchored target path inside the workspace root.
//! - [`crate::checkpoint::CheckpointHandle`] — pre-write baseline +
//!   manifest pinned on disk.
//! - [`crate::diff_preview::PreviewRecord`] — approval/audit identity
//!   (hashes + canonical metadata).
//! - [`crate::approval::ApprovalDecision::Approved`] — operator
//!   approval, bound by `(step_id, preview_sha256)`.
//! - [`crate::write_payload::ApprovedWritePayload`] — exact after-bytes,
//!   hash-bound to `PreviewRecord.after_sha256`.
//!
//! # Hard contract (slice 4)
//!
//! - This module mutates **exactly one** file when the full authority
//!   chain matches, and otherwise mutates nothing. No multi-file
//!   writes, no parent-directory creation, no chmod / chown, no
//!   symlink creation, no special-file creation.
//! - Same-directory temp + atomic rename, followed by parent-directory
//!   fsync. Cross-device rename is impossible by construction (same
//!   directory); if `EXDEV` ever surfaces, the executor aborts the
//!   write and never falls back to copy.
//! - No-clobber semantics on new-file create are enforced via
//!   `std::fs::hard_link`, which refuses to overwrite an existing
//!   path. Existing-file overwrite uses `std::fs::rename` for atomic
//!   in-place replacement.
//! - Pre-write baseline is verified twice — once before the temp file
//!   is created and once again immediately before the commit rename.
//! - Post-write the committed bytes are reopened from disk and
//!   re-hashed; a hash mismatch triggers bounded rollback.
//! - Rollback is automatic ONLY for immediate post-write validation
//!   failure. Rollback is refused if the on-disk state has drifted
//!   from what the executor itself just wrote, if the checkpoint is
//!   missing/corrupt, if the parent directory changed, or if the
//!   target is no longer a regular file.
//! - No subprocess invocation. No shell-outs to porcelain patch /
//!   diff tooling. No broker / model / network. No DEEP. No CLI
//!   integration and no plan-runner wiring.
//! - No `unsafe` (workspace-wide `unsafe_code = "forbid"`).
//!
//! # What lives elsewhere
//!
//! - Operator approval evaluation: [`crate::approval`].
//! - Diff preview + sanitization: [`crate::diff_preview`].
//! - Path safety / canonicalization: [`crate::write_runtime`].
//! - Pre-write baseline capture: [`crate::checkpoint`].
//! - Hash-bound after-bytes carrier: [`crate::write_payload`].
//! - CLI plumbing / `run_plan` integration: deliberately NOT in this
//!   slice. The executor is a library entry point only.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::approval::ApprovalDecision;
use crate::checkpoint::CheckpointHandle;
use crate::diff_preview::PreviewRecord;
use crate::markers;
use crate::write_payload::ApprovedWritePayload;
use crate::write_runtime::ResolvedWriteTarget;

// =========================================================================
// Exit codes
// =========================================================================

/// Exit code emitted when the executor writes the approved bytes and
/// the post-write hash validates.
pub const EXIT_WRITE_APPLIED: i32 = 0;

/// Exit code for malformed authority chain (mismatched step ids,
/// mismatched hashes between payload/preview/checkpoint, target path
/// mismatches between resolved/payload/checkpoint, etc.). Mirrors
/// [`crate::report::EXIT_PARSE_ERROR`]: the request itself is invalid;
/// no I/O attempted.
pub const EXIT_INVALID_REQUEST: i32 = 5;

/// Exit code emitted when the supplied [`ApprovalDecision`] is
/// `Refused`, or its `(step_id, preview_sha256)` does not bind to the
/// supplied [`PreviewRecord`]. Mirrors
/// [`crate::approval::EXIT_APPROVAL_DENIED`].
pub const EXIT_APPROVAL_REFUSED: i32 = 7;

/// Exit code emitted when rollback after a post-write validation
/// failure was itself refused or failed. Mirrors
/// [`crate::approval::EXIT_ROLLBACK_FAILED`].
pub const EXIT_ROLLBACK_FAILED: i32 = 8;

/// Exit code emitted when the live target's pre-write state does not
/// match the checkpoint manifest's recorded baseline (either content
/// drift on an existing file, an existing file where the manifest
/// recorded `absent`, or an absent file where the manifest recorded
/// `regular_file`).
pub const EXIT_BASELINE_MISMATCH: i32 = 9;

/// Exit code emitted when atomic write I/O failed BEFORE the commit
/// rename — i.e. target file is unchanged. Stages covered: temp
/// create, payload write, fsync, `hard_link` / rename refusal.
pub const EXIT_WRITE_IO_FAILED: i32 = 10;

/// Exit code emitted when the post-write hash validation failed and
/// rollback succeeded. Distinct from
/// [`EXIT_WRITE_IO_FAILED`] (target was not modified) and
/// [`EXIT_ROLLBACK_FAILED`] (target may be in a partially-rolled-back
/// state — operator attention required).
pub const EXIT_VALIDATION_ROLLED_BACK: i32 = 11;

// =========================================================================
// Request / Result types
// =========================================================================

/// Input to [`execute_write`].
///
/// All five authority objects are required. Slice-4a + earlier slices
/// already enforce the internal invariants of each object; the executor
/// re-verifies the cross-object bindings as defense in depth.
#[derive(Debug)]
pub struct WriteExecutionRequest<'a> {
    pub workspace_root: &'a Path,
    pub resolved: &'a ResolvedWriteTarget,
    pub checkpoint: &'a CheckpointHandle,
    pub preview: &'a PreviewRecord,
    pub approval: &'a ApprovalDecision,
    pub payload: &'a ApprovedWritePayload,
}

/// Authoritative outcome of [`execute_write`]. The enum is the source
/// of truth; the marker list is audit-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteExecutionOutcome {
    /// Write committed and post-write hash validated.
    Applied {
        target_absolute: PathBuf,
        wrote_size_bytes: u64,
    },
    /// One or more authority objects disagreed before any I/O was
    /// attempted. Distinct from approval refusal.
    RefusedAuthorityMismatch { cause: AuthorityMismatch },
    /// Supplied approval was `Refused`, or its identity tuple did not
    /// bind to the preview.
    RefusedApproval { cause: ApprovalRefusalCause },
    /// Live target's pre-write state differed from the checkpoint
    /// manifest baseline; no I/O attempted on the target.
    RefusedBaselineDrift { cause: BaselineDrift },
    /// Atomic write I/O failed before the commit step — target file
    /// is unchanged.
    AtomicWriteIoFailed { stage: WriteStage, message: String },
    /// Post-write hash did not match `payload.after_sha256`; rollback
    /// was attempted and succeeded.
    ValidationFailedRolledBack { message: String },
    /// Rollback was refused (drift, missing checkpoint, etc.) or
    /// failed mid-flight. Operator attention required.
    RollbackFailed { cause: RollbackFailureCause },
}

/// Aggregate executor result. `outcome` is authoritative; `markers`
/// are audit-only; `exit_code` mirrors the appropriate
/// `EXIT_*` constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteExecutionResult {
    pub outcome: WriteExecutionOutcome,
    pub markers: Vec<&'static str>,
    pub exit_code: i32,
}

/// Specific authority-chain disagreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityMismatch {
    /// `preview.is_approvable() == false`.
    PreviewNotApprovable,
    /// `payload.step_id` ≠ `preview.step_id`.
    PayloadStepIdMismatch,
    /// `payload.preview_id` ≠ `preview.preview_id`.
    PayloadPreviewIdMismatch,
    /// `payload.preview_sha256` ≠ `preview.preview_sha256`.
    PayloadPreviewShaMismatch,
    /// `payload.before_sha256` ≠ `preview.before_sha256`.
    PayloadBeforeShaMismatch,
    /// `payload.after_sha256` ≠ `preview.after_sha256`.
    PayloadAfterShaMismatch,
    /// Recomputed `sha256(payload.after_bytes())` ≠
    /// `preview.after_sha256`. Should be unreachable if Slice-4a is
    /// intact; checked as defense in depth.
    PayloadBytesAfterShaMismatch,
    /// `checkpoint.manifest.step_id` ≠ `preview.step_id`.
    CheckpointStepIdMismatch,
    /// `checkpoint.manifest.target_relative_path` ≠
    /// `payload.target_relative_path`.
    CheckpointTargetPathMismatch,
    /// `checkpoint.manifest.pre_sha256` ≠ `preview.before_sha256`.
    CheckpointPreShaMismatch,
    /// `resolved.absolute` not equal to
    /// `workspace_root.join(payload.target_relative_path)` after
    /// lexical comparison, OR `resolved.parent` not canonical (does
    /// not equal `resolved.absolute.parent()` joined), OR
    /// `resolved.parent` does not start with `workspace_root`.
    ResolvedTargetPathMismatch,
}

/// Approval-specific refusal cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRefusalCause {
    /// Supplied [`ApprovalDecision::Refused`].
    NotApproved,
    /// `approval.step_id` ≠ `preview.step_id`.
    StepIdMismatch,
    /// `approval.preview_sha256` ≠ `preview.preview_sha256`.
    PreviewShaMismatch,
}

/// Baseline-state drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaselineDrift {
    /// Manifest recorded `pre_existed = true`, but the target is not
    /// present on disk as a regular file.
    ExpectedExistingButMissing,
    /// Manifest recorded `pre_existed = false`, but a path already
    /// exists at the target.
    ExpectedAbsentButPresent,
    /// Target exists, but its current bytes hash to a value different
    /// from `manifest.pre_sha256` (= `preview.before_sha256`).
    ExistingContentDrift {
        expected_sha256: String,
        actual_sha256: String,
    },
    /// Target exists but is not a regular file (symlink, dir, socket,
    /// device, etc.). The executor refuses to touch non-regular
    /// targets in this slice.
    TargetNotRegularFile,
    /// Resolved parent directory does not exist or is not a directory
    /// at write time. Slice-1 proved this at resolve time; if it
    /// changed since, refuse.
    ParentDirectoryUnavailable,
}

/// Which atomic-write step failed before the commit rename. The
/// target file is unchanged on every variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteStage {
    /// `parent` directory could not be opened read-only for the
    /// post-write fsync handle.
    ParentOpen,
    /// Temp file create (with `create_new` / exclusive semantics)
    /// failed.
    TempCreate,
    /// `write_all` on the temp file failed.
    TempWrite,
    /// `sync_all` on the temp file failed.
    TempFsync,
    /// `std::fs::rename` (overwrite) or `std::fs::hard_link`
    /// (no-clobber create) refused. For new-file create this includes
    /// the no-clobber refusal arm.
    Commit,
    /// `sync_all` on the parent directory failed after a successful
    /// rename/link. The bytes are on disk; the rename may not be
    /// crash-durable. Surfaced as I/O failure so the operator
    /// re-runs.
    ParentFsync,
    /// Cleanup of the temp file (after a successful `hard_link`)
    /// failed. The committed target is intact; a stale temp may
    /// remain.
    TempCleanup,
}

/// Why rollback was refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackFailureCause {
    /// The on-disk file at the target no longer matches what the
    /// executor just wrote (hash != `payload.after_sha256`). An
    /// external process intervened between commit and validation;
    /// rolling back to the checkpoint baseline would clobber that
    /// process's write.
    ExternalMutationBeforeRollback,
    /// `checkpoint.before_bin_path` is `None` (manifest claimed
    /// `pre_existed = true`) — corrupt or absent baseline.
    CheckpointBaselineMissing,
    /// The target became a non-regular file (symlink, dir, etc.)
    /// between commit and rollback.
    TargetNotRegularFile,
    /// Parent directory disappeared or became non-directory between
    /// commit and rollback.
    ParentDirectoryUnavailable,
    /// I/O error during rollback. Target may be in a partially
    /// rolled-back state.
    RollbackIoError {
        stage: RollbackStage,
        message: String,
    },
}

/// Which rollback step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackStage {
    /// Could not open the checkpoint `before.bin` for reading.
    BaselineOpen,
    /// Rollback temp create / write / fsync failed.
    RollbackTemp,
    /// Final rename of the rollback temp into the target failed.
    RollbackCommit,
    /// `remove_file` on the new-file branch failed.
    RemoveNewFile,
    /// `sync_all` on the parent directory failed during rollback.
    ParentFsync,
}

// =========================================================================
// Public entry point
// =========================================================================

/// Execute the approved single-file write described by `request`.
///
/// Returns a [`WriteExecutionResult`]; never panics on operator-
/// caused conditions. Performs at most one filesystem mutation to the
/// target file, and only when every authority check passes.
///
/// The function is the only public mutation entry point in
/// `a2-plan-runner`; everything else in the crate is read-only.
#[must_use]
pub fn execute_write(request: &WriteExecutionRequest<'_>) -> WriteExecutionResult {
    let mut markers: Vec<&'static str> = Vec::new();

    // 1. Approval refusal check (cheap, distinct error class).
    let (approval_step_id, approval_preview_sha256) = match request.approval {
        ApprovalDecision::Approved {
            step_id,
            preview_sha256,
        } => (step_id.as_str(), preview_sha256.as_str()),
        ApprovalDecision::Refused(_) => {
            markers.push(markers::L2B_WRITE_REFUSED);
            return WriteExecutionResult {
                outcome: WriteExecutionOutcome::RefusedApproval {
                    cause: ApprovalRefusalCause::NotApproved,
                },
                markers,
                exit_code: EXIT_APPROVAL_REFUSED,
            };
        }
    };

    if approval_step_id != request.preview.step_id {
        markers.push(markers::L2B_WRITE_REFUSED);
        return WriteExecutionResult {
            outcome: WriteExecutionOutcome::RefusedApproval {
                cause: ApprovalRefusalCause::StepIdMismatch,
            },
            markers,
            exit_code: EXIT_APPROVAL_REFUSED,
        };
    }
    if approval_preview_sha256 != request.preview.preview_sha256 {
        markers.push(markers::L2B_WRITE_REFUSED);
        return WriteExecutionResult {
            outcome: WriteExecutionOutcome::RefusedApproval {
                cause: ApprovalRefusalCause::PreviewShaMismatch,
            },
            markers,
            exit_code: EXIT_APPROVAL_REFUSED,
        };
    }

    // 2. Authority-chain mismatch checks (defense in depth — Slice-4a
    //    + Slice-2 enforce most of these at construction time).
    if let Some(mismatch) = verify_authority_chain(request) {
        markers.push(markers::L2B_WRITE_REFUSED);
        return WriteExecutionResult {
            outcome: WriteExecutionOutcome::RefusedAuthorityMismatch { cause: mismatch },
            markers,
            exit_code: EXIT_INVALID_REQUEST,
        };
    }

    // 3. Baseline check against the live filesystem.
    match verify_baseline(request) {
        Ok(()) => {}
        Err(drift) => {
            markers.push(markers::L2B_WRITE_REFUSED);
            return WriteExecutionResult {
                outcome: WriteExecutionOutcome::RefusedBaselineDrift { cause: drift },
                markers,
                exit_code: EXIT_BASELINE_MISMATCH,
            };
        }
    }

    markers.push(markers::L2B_WRITE_PREFLIGHT_OK);

    // 4. Atomic write.
    let commit = match atomic_write(request, &mut markers) {
        Ok(commit) => commit,
        Err(AtomicWriteError { stage, message }) => {
            return WriteExecutionResult {
                outcome: WriteExecutionOutcome::AtomicWriteIoFailed { stage, message },
                markers,
                exit_code: EXIT_WRITE_IO_FAILED,
            };
        }
    };

    markers.push(markers::L2B_WRITE_APPLIED);

    // 5. Post-write validation: reopen target, hash bytes, compare
    //    against payload.after_sha256.
    match validate_post_write(&commit.target_absolute, &request.payload.after_sha256) {
        Ok(()) => {
            markers.push(markers::L2B_WRITE_VALIDATED);
            WriteExecutionResult {
                outcome: WriteExecutionOutcome::Applied {
                    target_absolute: commit.target_absolute,
                    wrote_size_bytes: request.payload.after_size_bytes,
                },
                markers,
                exit_code: EXIT_WRITE_APPLIED,
            }
        }
        Err(message) => {
            markers.push(markers::L2B_WRITE_VALIDATION_FAILED);
            // Run rollback. The post-write hash mismatch is the only
            // automatic-rollback trigger in slice 4.
            match rollback_after_validation_failure(request, &commit, &mut markers) {
                Ok(()) => WriteExecutionResult {
                    outcome: WriteExecutionOutcome::ValidationFailedRolledBack { message },
                    markers,
                    exit_code: EXIT_VALIDATION_ROLLED_BACK,
                },
                Err(cause) => WriteExecutionResult {
                    outcome: WriteExecutionOutcome::RollbackFailed { cause },
                    markers,
                    exit_code: EXIT_ROLLBACK_FAILED,
                },
            }
        }
    }
}

// =========================================================================
// Internals
// =========================================================================

#[allow(clippy::too_many_lines)]
fn verify_authority_chain(req: &WriteExecutionRequest<'_>) -> Option<AuthorityMismatch> {
    if !req.preview.is_approvable() {
        return Some(AuthorityMismatch::PreviewNotApprovable);
    }

    // payload ↔ preview
    if req.payload.step_id != req.preview.step_id {
        return Some(AuthorityMismatch::PayloadStepIdMismatch);
    }
    if req.payload.preview_id != req.preview.preview_id {
        return Some(AuthorityMismatch::PayloadPreviewIdMismatch);
    }
    if req.payload.preview_sha256 != req.preview.preview_sha256 {
        return Some(AuthorityMismatch::PayloadPreviewShaMismatch);
    }
    if req.payload.before_sha256 != req.preview.before_sha256 {
        return Some(AuthorityMismatch::PayloadBeforeShaMismatch);
    }
    if req.payload.after_sha256 != req.preview.after_sha256 {
        return Some(AuthorityMismatch::PayloadAfterShaMismatch);
    }

    // Re-hash the payload bytes vs the record's after_sha256. Slice-4a
    // already guarantees this at bind time; this is the executor's
    // belt-and-braces check.
    let recomputed = sha256_hex(req.payload.after_bytes());
    if recomputed != req.preview.after_sha256 {
        return Some(AuthorityMismatch::PayloadBytesAfterShaMismatch);
    }

    // checkpoint manifest ↔ preview / payload
    if req.checkpoint.manifest.step_id != req.preview.step_id {
        return Some(AuthorityMismatch::CheckpointStepIdMismatch);
    }
    if !path_strings_equal(
        &req.checkpoint.manifest.target_relative_path,
        &req.payload.target_relative_path,
    ) {
        return Some(AuthorityMismatch::CheckpointTargetPathMismatch);
    }
    // The manifest's pre_sha256 is the empty string for the
    // pre_existed=false branch; preview.before_sha256 is the empty
    // string for the same branch (see diff_preview build_preview).
    if req.checkpoint.manifest.pre_sha256 != req.preview.before_sha256 {
        return Some(AuthorityMismatch::CheckpointPreShaMismatch);
    }

    // resolved ↔ workspace_root + payload.target_relative_path
    let expected_absolute = req.workspace_root.join(&req.payload.target_relative_path);
    if req.resolved.absolute != expected_absolute {
        return Some(AuthorityMismatch::ResolvedTargetPathMismatch);
    }
    // Resolved parent must be the parent of resolved.absolute and
    // must start with workspace_root.
    let Some(actual_parent) = req.resolved.absolute.parent() else {
        return Some(AuthorityMismatch::ResolvedTargetPathMismatch);
    };
    if req.resolved.parent != actual_parent {
        return Some(AuthorityMismatch::ResolvedTargetPathMismatch);
    }
    if !req.resolved.parent.starts_with(req.workspace_root) {
        return Some(AuthorityMismatch::ResolvedTargetPathMismatch);
    }

    None
}

fn verify_baseline(req: &WriteExecutionRequest<'_>) -> Result<(), BaselineDrift> {
    // Parent must still be a directory.
    match fs::symlink_metadata(&req.resolved.parent) {
        Ok(m) if m.is_dir() => {}
        _ => return Err(BaselineDrift::ParentDirectoryUnavailable),
    }

    let pre_existed = req.checkpoint.manifest.pre_existed;
    match fs::symlink_metadata(&req.resolved.absolute) {
        Ok(meta) => {
            if !pre_existed {
                return Err(BaselineDrift::ExpectedAbsentButPresent);
            }
            let ft = meta.file_type();
            if ft.is_symlink() || !meta.is_file() {
                return Err(BaselineDrift::TargetNotRegularFile);
            }
            let actual = hash_file(&req.resolved.absolute).map_err(|_| {
                BaselineDrift::ExistingContentDrift {
                    expected_sha256: req.preview.before_sha256.clone(),
                    actual_sha256: String::new(),
                }
            })?;
            if actual != req.preview.before_sha256 {
                return Err(BaselineDrift::ExistingContentDrift {
                    expected_sha256: req.preview.before_sha256.clone(),
                    actual_sha256: actual,
                });
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if pre_existed {
                Err(BaselineDrift::ExpectedExistingButMissing)
            } else {
                Ok(())
            }
        }
        Err(_) => Err(BaselineDrift::ParentDirectoryUnavailable),
    }
}

/// Per-write committed state. Used by rollback to know which commit
/// path was taken (overwrite vs new-file).
#[derive(Debug)]
struct CommitState {
    target_absolute: PathBuf,
    pre_existed: bool,
}

#[derive(Debug)]
struct AtomicWriteError {
    stage: WriteStage,
    message: String,
}

fn atomic_write(
    req: &WriteExecutionRequest<'_>,
    markers: &mut Vec<&'static str>,
) -> Result<CommitState, AtomicWriteError> {
    let parent = req.resolved.parent.as_path();
    let target = req.resolved.absolute.clone();
    let pre_existed = req.checkpoint.manifest.pre_existed;

    // 1. Open parent for the post-commit fsync handle.
    let parent_dir = OpenOptions::new()
        .read(true)
        .open(parent)
        .map_err(|e| AtomicWriteError {
            stage: WriteStage::ParentOpen,
            message: format!("open parent {}: {e}", parent.display()),
        })?;

    // 2. Create same-directory temp file with exclusive create.
    let temp_path = same_dir_temp_path(parent, &req.resolved.file_name);
    let mut temp_file = create_new_temp(&temp_path).map_err(|e| AtomicWriteError {
        stage: WriteStage::TempCreate,
        message: format!("create temp {}: {e}", temp_path.display()),
    })?;
    markers.push(markers::L2B_WRITE_TEMP_CREATED);

    // 3. Write the bytes.
    temp_file
        .write_all(req.payload.after_bytes())
        .map_err(|e| AtomicWriteError {
            stage: WriteStage::TempWrite,
            message: format!("write temp: {e}"),
        })?;

    // 4. fsync temp.
    temp_file.sync_all().map_err(|e| AtomicWriteError {
        stage: WriteStage::TempFsync,
        message: format!("fsync temp: {e}"),
    })?;
    drop(temp_file);

    // 5. Re-check baseline immediately before commit. If the live
    //    target drifted between the first baseline check and now,
    //    refuse the commit and clean up the temp.
    if let Err(drift) = verify_baseline(req) {
        let _ = fs::remove_file(&temp_path);
        return Err(AtomicWriteError {
            stage: WriteStage::Commit,
            message: format!("baseline drift before commit: {drift:?}"),
        });
    }

    // 6. Commit.
    if pre_existed {
        // Existing-file overwrite: rename is atomic same-fs. Cross-fs
        // is impossible by construction (temp is in `parent`).
        fs::rename(&temp_path, &target).map_err(|e| AtomicWriteError {
            stage: WriteStage::Commit,
            message: format!(
                "rename {} -> {}: {e}",
                temp_path.display(),
                target.display()
            ),
        })?;
    } else {
        // No-clobber new-file create: hard_link refuses when target
        // exists. Followed by temp cleanup.
        match fs::hard_link(&temp_path, &target) {
            Ok(()) => {}
            Err(e) => {
                let _ = fs::remove_file(&temp_path);
                return Err(AtomicWriteError {
                    stage: WriteStage::Commit,
                    message: format!(
                        "hard_link {} -> {} (no-clobber): {e}",
                        temp_path.display(),
                        target.display()
                    ),
                });
            }
        }
        // Remove the temp link; the target retains the file.
        if let Err(e) = fs::remove_file(&temp_path) {
            // Target is committed; temp cleanup failure is surfaced
            // as I/O failure so the operator notices. The target is
            // intact, so this is recoverable.
            return Err(AtomicWriteError {
                stage: WriteStage::TempCleanup,
                message: format!("remove temp {}: {e}", temp_path.display()),
            });
        }
    }

    // 7. fsync parent directory so the rename / link is durable.
    parent_dir.sync_all().map_err(|e| AtomicWriteError {
        stage: WriteStage::ParentFsync,
        message: format!("fsync parent {}: {e}", parent.display()),
    })?;

    Ok(CommitState {
        target_absolute: target,
        pre_existed,
    })
}

fn validate_post_write(target: &Path, expected_after_sha256: &str) -> Result<(), String> {
    let actual = hash_file(target).map_err(|e| format!("reopen target for hash: {e}"))?;
    if actual != expected_after_sha256 {
        return Err(format!(
            "post-write hash mismatch: expected {expected_after_sha256}, actual {actual}"
        ));
    }
    Ok(())
}

fn rollback_after_validation_failure(
    req: &WriteExecutionRequest<'_>,
    commit: &CommitState,
    markers: &mut Vec<&'static str>,
) -> Result<(), RollbackFailureCause> {
    markers.push(markers::L2B_ROLLBACK_STARTED);

    let parent = req.resolved.parent.as_path();

    // Common pre-rollback drift check: the on-disk file must still
    // match what the executor just wrote (sha256 == payload.after_sha256).
    // If it doesn't, an external process has touched the file since
    // commit, and rolling back to baseline would clobber that change.
    let current = match fs::symlink_metadata(&commit.target_absolute) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // For new-file branch the file SHOULD still exist (we
            // just committed it). For overwrite branch likewise.
            // Either way absence here is external drift.
            markers.push(markers::L2B_ROLLBACK_REFUSED);
            return Err(RollbackFailureCause::ExternalMutationBeforeRollback);
        }
        Err(_) => {
            markers.push(markers::L2B_ROLLBACK_REFUSED);
            return Err(RollbackFailureCause::TargetNotRegularFile);
        }
    };
    if current.file_type().is_symlink() || !current.is_file() {
        markers.push(markers::L2B_ROLLBACK_REFUSED);
        return Err(RollbackFailureCause::TargetNotRegularFile);
    }
    let on_disk = match hash_file(&commit.target_absolute) {
        Ok(h) => h,
        Err(e) => {
            markers.push(markers::L2B_ROLLBACK_FAILED);
            return Err(RollbackFailureCause::RollbackIoError {
                stage: RollbackStage::BaselineOpen,
                message: format!("hash post-commit target: {e}"),
            });
        }
    };
    if on_disk != req.payload.after_sha256 {
        markers.push(markers::L2B_ROLLBACK_REFUSED);
        return Err(RollbackFailureCause::ExternalMutationBeforeRollback);
    }

    if !commit.pre_existed {
        // New-file: remove. Parent fsync after.
        if let Err(e) = fs::remove_file(&commit.target_absolute) {
            markers.push(markers::L2B_ROLLBACK_FAILED);
            return Err(RollbackFailureCause::RollbackIoError {
                stage: RollbackStage::RemoveNewFile,
                message: format!("remove {}: {e}", commit.target_absolute.display()),
            });
        }
        fsync_parent_for_rollback(parent, markers)?;
        markers.push(markers::L2B_ROLLBACK_SUCCEEDED);
        return Ok(());
    }

    // Existing-file: restore from before.bin via temp+rename+fsync.
    let Some(baseline) = req.checkpoint.before_bin_path.clone() else {
        markers.push(markers::L2B_ROLLBACK_FAILED);
        return Err(RollbackFailureCause::CheckpointBaselineMissing);
    };
    let parent_dir = OpenOptions::new().read(true).open(parent).map_err(|_| {
        markers.push(markers::L2B_ROLLBACK_FAILED);
        RollbackFailureCause::ParentDirectoryUnavailable
    })?;

    let rollback_temp = same_dir_temp_path(
        parent,
        &OsString::from(format!(
            "{}.rollback",
            file_name_str(&req.resolved.file_name)
        )),
    );
    if let Err(e) = stream_file_to_temp(&baseline, &rollback_temp) {
        let _ = fs::remove_file(&rollback_temp);
        markers.push(markers::L2B_ROLLBACK_FAILED);
        return Err(RollbackFailureCause::RollbackIoError {
            stage: RollbackStage::RollbackTemp,
            message: format!(
                "stream baseline {} -> {}: {e}",
                baseline.display(),
                rollback_temp.display()
            ),
        });
    }
    if let Err(e) = fs::rename(&rollback_temp, &commit.target_absolute) {
        let _ = fs::remove_file(&rollback_temp);
        markers.push(markers::L2B_ROLLBACK_FAILED);
        return Err(RollbackFailureCause::RollbackIoError {
            stage: RollbackStage::RollbackCommit,
            message: format!(
                "rename {} -> {}: {e}",
                rollback_temp.display(),
                commit.target_absolute.display()
            ),
        });
    }
    if let Err(e) = parent_dir.sync_all() {
        markers.push(markers::L2B_ROLLBACK_FAILED);
        return Err(RollbackFailureCause::RollbackIoError {
            stage: RollbackStage::ParentFsync,
            message: format!("fsync parent: {e}"),
        });
    }
    markers.push(markers::L2B_ROLLBACK_SUCCEEDED);
    Ok(())
}

fn fsync_parent_for_rollback(
    parent: &Path,
    markers: &mut Vec<&'static str>,
) -> Result<(), RollbackFailureCause> {
    let parent_dir = OpenOptions::new().read(true).open(parent).map_err(|_| {
        markers.push(markers::L2B_ROLLBACK_FAILED);
        RollbackFailureCause::ParentDirectoryUnavailable
    })?;
    parent_dir.sync_all().map_err(|e| {
        markers.push(markers::L2B_ROLLBACK_FAILED);
        RollbackFailureCause::RollbackIoError {
            stage: RollbackStage::ParentFsync,
            message: format!("fsync parent: {e}"),
        }
    })
}

// =========================================================================
// FS primitives
// =========================================================================

fn same_dir_temp_path(parent: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    // ULID gives lexicographic time-ordering + sufficient entropy for
    // collision-free temp names across concurrent runs.
    let ulid = Ulid::new();
    let leaf = format!(".a2-l2b-write-{}-{}.tmp", file_name_str(file_name), ulid);
    parent.join(leaf)
}

fn file_name_str(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().into_owned()
}

fn create_new_temp(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

fn stream_file_to_temp(src: &Path, dst: &Path) -> io::Result<()> {
    let mut input = File::open(src)?;
    let mut output = create_new_temp(dst)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
    }
    output.sync_all()?;
    Ok(())
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

fn hex_lower(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(digest.len() * 2);
    for &b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn path_strings_equal(manifest_str: &str, payload_path: &Path) -> bool {
    // Manifest target_relative_path is `payload.target_relative_path.display().to_string()`
    // at checkpoint write time. Compare on the same display form.
    payload_path.display().to_string() == manifest_str
}

// =========================================================================
// Unit tests (pure helpers — FS-touching behavior is covered by the
// tests/l2b_write_executor.rs integration suite)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_pins() {
        assert_eq!(EXIT_WRITE_APPLIED, 0);
        assert_eq!(EXIT_INVALID_REQUEST, 5);
        assert_eq!(EXIT_APPROVAL_REFUSED, 7);
        assert_eq!(EXIT_ROLLBACK_FAILED, 8);
        assert_eq!(EXIT_BASELINE_MISMATCH, 9);
        assert_eq!(EXIT_WRITE_IO_FAILED, 10);
        assert_eq!(EXIT_VALIDATION_ROLLED_BACK, 11);
    }

    #[test]
    fn sha256_hex_matches_known_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn path_strings_equal_matches_pathbuf_display() {
        let p = PathBuf::from("src/lib.rs");
        assert!(path_strings_equal("src/lib.rs", &p));
        assert!(!path_strings_equal("src/main.rs", &p));
    }

    #[test]
    fn same_dir_temp_path_has_ulid_suffix() {
        let p = same_dir_temp_path(Path::new("/tmp"), std::ffi::OsStr::new("lib.rs"));
        let leaf = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(leaf.starts_with(".a2-l2b-write-lib.rs-"), "got {leaf}");
        assert!(
            Path::new(&leaf)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp")),
            "got {leaf}"
        );
    }
}
