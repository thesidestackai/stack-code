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
//!   chain matches, and otherwise mutates nothing. Multiple-file
//!   writes are out of scope. No parent-directory creation, no chmod /
//!   chown, no symlink creation, no special-file creation.
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
    //
    // Test-only seam: a hook may mutate the on-disk file between commit
    // and the validation re-read. This is the only way to drive a real
    // hash mismatch through the public surface on a healthy fs.
    #[cfg(test)]
    if let Some(hook) = test_hooks::snapshot().before_validate_hook {
        hook(&commit.target_absolute);
    }
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

#[allow(clippy::too_many_lines)]
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
    #[cfg(test)]
    if let Some(kind) = test_hooks::snapshot().inject_temp_create_err {
        return Err(AtomicWriteError {
            stage: WriteStage::TempCreate,
            message: format!("test-injected: temp create {kind:?}"),
        });
    }
    let mut temp_file = create_new_temp(&temp_path).map_err(|e| AtomicWriteError {
        stage: WriteStage::TempCreate,
        message: format!("create temp {}: {e}", temp_path.display()),
    })?;
    markers.push(markers::L2B_WRITE_TEMP_CREATED);

    // 3. Write the bytes.
    #[cfg(test)]
    if let Some(kind) = test_hooks::snapshot().inject_temp_write_err {
        drop(temp_file);
        let _ = fs::remove_file(&temp_path);
        return Err(AtomicWriteError {
            stage: WriteStage::TempWrite,
            message: format!("test-injected: temp write {kind:?}"),
        });
    }
    temp_file
        .write_all(req.payload.after_bytes())
        .map_err(|e| AtomicWriteError {
            stage: WriteStage::TempWrite,
            message: format!("write temp: {e}"),
        })?;

    // 4. fsync temp.
    #[cfg(test)]
    if let Some(kind) = test_hooks::snapshot().inject_temp_fsync_err {
        drop(temp_file);
        let _ = fs::remove_file(&temp_path);
        return Err(AtomicWriteError {
            stage: WriteStage::TempFsync,
            message: format!("test-injected: temp fsync {kind:?}"),
        });
    }
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

    // Test-only race window: AFTER baseline-recheck passes, BEFORE the
    // commit primitive runs. The hook may create the target slot to
    // exercise the no-clobber refusal on the new-file branch.
    #[cfg(test)]
    {
        if let Some(hook) = test_hooks::snapshot().before_commit_hook {
            hook(&target);
        }
        if let Some(kind) = test_hooks::snapshot().inject_commit_err {
            let _ = fs::remove_file(&temp_path);
            return Err(AtomicWriteError {
                stage: WriteStage::Commit,
                message: format!("test-injected: commit {kind:?}"),
            });
        }
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
    #[cfg(test)]
    if let Some(kind) = test_hooks::snapshot().inject_parent_fsync_err {
        return Err(AtomicWriteError {
            stage: WriteStage::ParentFsync,
            message: format!("test-injected: parent fsync {kind:?}"),
        });
    }
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
    #[cfg(test)]
    if test_hooks::snapshot().force_validation_mismatch {
        return Err("test-injected: forced post-write validation mismatch".to_string());
    }
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

    // Test-only seam: an external process may mutate the on-disk file
    // between commit-success and the pre-rollback drift gate. Used to
    // drive the `ExternalMutationBeforeRollback` refusal deterministically.
    #[cfg(test)]
    if let Some(hook) = test_hooks::snapshot().before_rollback_rehash_hook {
        hook(&commit.target_absolute);
    }

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
// Test-only fault-injection seams (private; `#[cfg(test)]`-only).
// =========================================================================
//
// These hooks exist so the slice-4 unit suite can reach states inside
// [`execute_write`] that a healthy filesystem will not otherwise produce:
//
//   - forced post-write validation mismatch (drives the rollback path),
//   - synthesized I/O errors at each atomic-write stage (drives
//     [`WriteStage`] discrimination),
//   - a "between baseline-recheck and commit" hook (drives the
//     [`std::fs::hard_link`] no-clobber refusal arm),
//   - a "between commit and post-write validation" hook (drives a real
//     hash-mismatch detection without unsafe),
//   - a "between validation failure and rollback re-hash" hook (drives
//     the rollback's pre-rollback drift gate).
//
// The module is `#[cfg(test)]`-only and never compiles for production
// builds. Production callers (`run_plan`, the CLI, downstream crates)
// see no surface change whatsoever — no public API, no feature flag,
// no env-var, no `#[doc(hidden)] pub`. Integration tests in the sibling
// `tests/` crate also cannot see these hooks; the fault-coverage tests
// therefore live as unit tests in this same file (see [`fault_tests`]).
#[cfg(test)]
pub(crate) mod test_hooks {
    use std::cell::RefCell;
    use std::io;
    use std::path::Path;
    use std::sync::Arc;

    pub(crate) type PathHook = Arc<dyn Fn(&Path) + Send + Sync>;

    /// Per-thread fault-injection state consulted by the executor at
    /// `#[cfg(test)]`-only inspection points.
    ///
    /// Every field defaults to the "no fault" value; a healthy executor
    /// run with default hooks behaves identically whether the module
    /// is compiled with or without `#[cfg(test)]`.
    #[derive(Default, Clone)]
    pub(crate) struct TestHooks {
        /// Force [`super::atomic_write`] to short-circuit with a
        /// `WriteStage::TempCreate` failure carrying the given error
        /// kind. The real temp file is never created.
        pub inject_temp_create_err: Option<io::ErrorKind>,
        /// Short-circuit with a `WriteStage::TempWrite` failure after
        /// the real temp create succeeds. The temp file is cleaned up.
        pub inject_temp_write_err: Option<io::ErrorKind>,
        /// Short-circuit with a `WriteStage::TempFsync` failure after
        /// the temp write succeeds. The temp file is cleaned up.
        pub inject_temp_fsync_err: Option<io::ErrorKind>,
        /// Short-circuit with a `WriteStage::Commit` failure after the
        /// pre-commit baseline recheck passes — mimics e.g. an `EXDEV`
        /// rename failure or a hard-link no-clobber refusal. The temp
        /// file is cleaned up.
        pub inject_commit_err: Option<io::ErrorKind>,
        /// Short-circuit with a `WriteStage::ParentFsync` failure after
        /// a successful commit, before the parent directory `sync_all`.
        /// The target stays committed; the executor surfaces this as
        /// `AtomicWriteIoFailed` so the operator re-runs.
        pub inject_parent_fsync_err: Option<io::ErrorKind>,
        /// Run between the second baseline recheck and the commit
        /// rename/link. Used to model "another process raced us into
        /// the target slot" deterministically.
        pub before_commit_hook: Option<PathHook>,
        /// Run between commit success and the post-write hash re-read.
        /// Used to drive a real on-disk hash mismatch without
        /// `unsafe`.
        pub before_validate_hook: Option<PathHook>,
        /// Run between post-write validation failure and the rollback's
        /// pre-rollback re-hash check. Used to drive
        /// `RollbackFailureCause::ExternalMutationBeforeRollback`.
        pub before_rollback_rehash_hook: Option<PathHook>,
        /// When `true`, the executor's [`super::validate_post_write`]
        /// returns `Err` regardless of on-disk content. Combined with
        /// the rollback hooks, drives both rollback-success and
        /// rollback-refused exercises.
        pub force_validation_mismatch: bool,
    }

    thread_local! {
        static HOOKS: RefCell<TestHooks> = const { RefCell::new(TestHooks {
            inject_temp_create_err: None,
            inject_temp_write_err: None,
            inject_temp_fsync_err: None,
            inject_commit_err: None,
            inject_parent_fsync_err: None,
            before_commit_hook: None,
            before_validate_hook: None,
            before_rollback_rehash_hook: None,
            force_validation_mismatch: false,
        }) };
    }

    /// Mutate the per-thread hook state.
    pub(crate) fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&mut TestHooks) -> R,
    {
        HOOKS.with(|h| f(&mut h.borrow_mut()))
    }

    /// Cheap clone-snapshot of the current per-thread hook state. The
    /// executor only reads hooks via this snapshot so a panicking hook
    /// closure cannot leave the `RefCell` borrowed across calls.
    pub(crate) fn snapshot() -> TestHooks {
        HOOKS.with(|h| h.borrow().clone())
    }

    /// RAII guard that resets per-thread hooks to default both on
    /// construction (so the test starts from a clean slate even if a
    /// previous test on this thread panicked mid-flight) and on drop.
    pub(crate) struct Reset;

    impl Reset {
        pub(crate) fn new() -> Self {
            HOOKS.with(|h| *h.borrow_mut() = TestHooks::default());
            Reset
        }
    }

    impl Drop for Reset {
        fn drop(&mut self) {
            HOOKS.with(|h| *h.borrow_mut() = TestHooks::default());
        }
    }
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

// =========================================================================
// Fault-injection coverage (cfg(test)-only, unix-only).
// =========================================================================
//
// These tests sit inside the library crate so they can consult the
// `test_hooks` module — that module is `pub(crate)` and only compiled
// under `#[cfg(test)]`. The sibling integration suite in
// `tests/l2b_write_executor.rs` cannot see the hooks (no public
// surface change) and therefore cannot impersonate fault conditions;
// the fault tests must live here.
//
// Coverage targets (P2 carryover from PR #28):
//   A. Post-write validation failure triggers bounded rollback.
//   B. Rollback refuses if the on-disk file changed externally
//      between commit and the pre-rollback drift gate.
//   C. New-file no-clobber race between baseline-recheck and the
//      `hard_link` commit primitive.
//   D. Per-stage error mapping for temp create / write / fsync /
//      commit / parent fsync.
//   E. EXDEV-class commit failures map to `AtomicWriteIoFailed`
//      (no copy fallback exists).
#[cfg(all(test, unix))]
mod fault_tests {
    use super::test_hooks;
    use super::{
        execute_write, AtomicWriteError, AuthorityMismatch, BaselineDrift, RollbackFailureCause,
        WriteExecutionOutcome, WriteExecutionRequest, WriteStage, EXIT_ROLLBACK_FAILED,
        EXIT_VALIDATION_ROLLED_BACK, EXIT_WRITE_IO_FAILED,
    };

    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::approval::ApprovalDecision;
    use crate::checkpoint::{CheckpointHandle, CheckpointStore};
    use crate::diff_preview::{build_preview, PreviewInputs, PreviewRecord};
    use crate::markers::{
        L2B_ROLLBACK_FAILED, L2B_ROLLBACK_REFUSED, L2B_ROLLBACK_STARTED, L2B_ROLLBACK_SUCCEEDED,
        L2B_WRITE_APPLIED, L2B_WRITE_PREFLIGHT_OK, L2B_WRITE_TEMP_CREATED, L2B_WRITE_VALIDATED,
        L2B_WRITE_VALIDATION_FAILED,
    };
    use crate::write_payload::{bind_after_bytes, ApprovedWritePayload};
    use crate::write_runtime::{resolve_write_target, ResolvedWriteTarget};
    use a2_plan_schema::WriteTarget;

    // -------------------------------------------------------------------
    // Hand-rolled tempdir + fixture (parallel to tests/l2b_write_executor.rs)
    // -------------------------------------------------------------------

    struct TempWorkspace {
        root: PathBuf,
    }

    impl TempWorkspace {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock not before unix epoch")
                .as_nanos();
            let mut p = std::env::temp_dir();
            p.push(format!(
                "a2_l2b_write_executor_fault_{}_{}_{}",
                label,
                std::process::id(),
                nanos
            ));
            fs::create_dir(&p).expect("tempdir create");
            let root = p.canonicalize().expect("tempdir canonicalize");
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    struct Fixture {
        ws: TempWorkspace,
        resolved: ResolvedWriteTarget,
        checkpoint: CheckpointHandle,
        preview: PreviewRecord,
        approval: ApprovalDecision,
        payload: ApprovedWritePayload,
    }

    impl Fixture {
        fn target_absolute(&self) -> PathBuf {
            self.resolved.absolute.clone()
        }
        fn workspace_root(&self) -> &Path {
            self.ws.root()
        }
        fn request(&self) -> WriteExecutionRequest<'_> {
            WriteExecutionRequest {
                workspace_root: self.workspace_root(),
                resolved: &self.resolved,
                checkpoint: &self.checkpoint,
                preview: &self.preview,
                approval: &self.approval,
                payload: &self.payload,
            }
        }
    }

    fn build_fixture(
        label: &str,
        target_rel: &str,
        before: Option<&[u8]>,
        after: &[u8],
    ) -> Fixture {
        let ws = TempWorkspace::new(label);
        let target_abs = ws.root().join(target_rel);
        if let Some(b) = before {
            if let Some(parent) = target_abs.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(&target_abs, b).expect("seed target");
        } else if let Some(parent) = target_abs.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let resolved = resolve_write_target(
            ws.root(),
            &WriteTarget {
                path: target_rel.into(),
                create_if_absent: before.is_none(),
            },
        )
        .expect("resolve_write_target");
        let store = CheckpointStore::new_with_generated_run_id(ws.root().to_path_buf());
        let step_id = "fault-step";
        let checkpoint = store
            .create_checkpoint(step_id, &resolved.absolute, Path::new(target_rel))
            .expect("create_checkpoint");
        let run_id = *store.run_id();
        let target_rel_path = PathBuf::from(target_rel);
        let inputs = PreviewInputs {
            step_id,
            target_relative_path: &target_rel_path,
            target_absolute_path: &resolved.absolute,
            before,
            after,
            checkpoint_run_id: &run_id,
            checkpoint_step_id: step_id,
            created_at_utc: "2026-05-27T00:00:00.000000000Z",
        };
        let (preview, _display) = build_preview(&inputs).expect("build_preview");
        let approval = ApprovalDecision::Approved {
            step_id: preview.step_id.clone(),
            preview_sha256: preview.preview_sha256.clone(),
        };
        let payload =
            bind_after_bytes(&preview, target_rel_path, after.to_vec()).expect("bind_after_bytes");
        Fixture {
            ws,
            resolved,
            checkpoint,
            preview,
            approval,
            payload,
        }
    }

    // -------------------------------------------------------------------
    // A. Post-write validation failure → bounded rollback succeeds.
    // -------------------------------------------------------------------

    #[test]
    fn fault_validation_failure_rolls_back_existing_file() {
        let _r = test_hooks::Reset::new();
        test_hooks::with(|h| h.force_validation_mismatch = true);

        let fix = build_fixture(
            "validfail-existing",
            "src/lib.rs",
            Some(b"baseline-bytes\n"),
            b"approved-bytes\n",
        );
        let result = execute_write(&fix.request());

        assert_eq!(result.exit_code, EXIT_VALIDATION_ROLLED_BACK);
        match &result.outcome {
            WriteExecutionOutcome::ValidationFailedRolledBack { message } => {
                assert!(
                    message.contains("test-injected"),
                    "expected injected message, got {message:?}"
                );
            }
            other => panic!("expected ValidationFailedRolledBack, got {other:?}"),
        }
        // Required markers in order: preflight, applied, validation-failed,
        // rollback-started, rollback-succeeded.
        let m = &result.markers;
        assert!(m.contains(&L2B_WRITE_PREFLIGHT_OK), "markers: {m:?}");
        assert!(m.contains(&L2B_WRITE_APPLIED), "markers: {m:?}");
        assert!(m.contains(&L2B_WRITE_VALIDATION_FAILED), "markers: {m:?}");
        assert!(m.contains(&L2B_ROLLBACK_STARTED), "markers: {m:?}");
        assert!(m.contains(&L2B_ROLLBACK_SUCCEEDED), "markers: {m:?}");
        assert!(!m.contains(&L2B_ROLLBACK_REFUSED), "markers: {m:?}");
        assert!(!m.contains(&L2B_ROLLBACK_FAILED), "markers: {m:?}");
        assert!(!m.contains(&L2B_WRITE_VALIDATED), "markers: {m:?}");

        // Existing-file branch: rollback restores baseline bytes.
        let restored = fs::read(fix.target_absolute()).expect("read target");
        assert_eq!(restored, b"baseline-bytes\n");
    }

    #[test]
    fn fault_validation_failure_rolls_back_new_file() {
        let _r = test_hooks::Reset::new();
        test_hooks::with(|h| h.force_validation_mismatch = true);

        let fix = build_fixture("validfail-new", "docs/new.md", None, b"# new\n");
        let result = execute_write(&fix.request());

        assert_eq!(result.exit_code, EXIT_VALIDATION_ROLLED_BACK);
        assert!(matches!(
            result.outcome,
            WriteExecutionOutcome::ValidationFailedRolledBack { .. }
        ));
        let m = &result.markers;
        assert!(m.contains(&L2B_ROLLBACK_STARTED), "markers: {m:?}");
        assert!(m.contains(&L2B_ROLLBACK_SUCCEEDED), "markers: {m:?}");

        // New-file branch: rollback removes the target.
        let exists = fs::symlink_metadata(fix.target_absolute()).is_ok();
        assert!(
            !exists,
            "rollback on new-file branch should have removed target"
        );
    }

    // -------------------------------------------------------------------
    // B. Rollback refuses external mutation between commit and re-hash.
    // -------------------------------------------------------------------

    #[test]
    fn fault_rollback_refuses_when_target_externally_mutated_existing() {
        let _r = test_hooks::Reset::new();
        test_hooks::with(|h| {
            h.force_validation_mismatch = true;
            h.before_rollback_rehash_hook = Some(Arc::new(|target: &Path| {
                fs::write(target, b"external-third-party-content\n").expect("external mutation");
            }));
        });

        let fix = build_fixture(
            "extmut-existing",
            "src/lib.rs",
            Some(b"baseline\n"),
            b"approved-after\n",
        );
        let result = execute_write(&fix.request());

        assert_eq!(result.exit_code, EXIT_ROLLBACK_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::RollbackFailed {
                cause: RollbackFailureCause::ExternalMutationBeforeRollback,
            } => {}
            other => panic!("expected ExternalMutationBeforeRollback, got {other:?}"),
        }
        let m = &result.markers;
        assert!(m.contains(&L2B_ROLLBACK_STARTED), "markers: {m:?}");
        assert!(m.contains(&L2B_ROLLBACK_REFUSED), "markers: {m:?}");
        assert!(!m.contains(&L2B_ROLLBACK_SUCCEEDED), "markers: {m:?}");

        // Critically: rollback did NOT clobber the external write.
        let on_disk = fs::read(fix.target_absolute()).expect("read target");
        assert_eq!(on_disk, b"external-third-party-content\n");
    }

    #[test]
    fn fault_rollback_refuses_when_new_file_externally_removed() {
        let _r = test_hooks::Reset::new();
        test_hooks::with(|h| {
            h.force_validation_mismatch = true;
            h.before_rollback_rehash_hook = Some(Arc::new(|target: &Path| {
                let _ = fs::remove_file(target);
            }));
        });

        let fix = build_fixture("extmut-new", "docs/draft.md", None, b"# draft\n");
        let result = execute_write(&fix.request());

        assert_eq!(result.exit_code, EXIT_ROLLBACK_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::RollbackFailed {
                cause: RollbackFailureCause::ExternalMutationBeforeRollback,
            } => {}
            other => panic!("expected ExternalMutationBeforeRollback, got {other:?}"),
        }
        assert!(result.markers.contains(&L2B_ROLLBACK_REFUSED));
        // Target was externally removed; we don't recreate it.
        assert!(fs::symlink_metadata(fix.target_absolute()).is_err());
    }

    // -------------------------------------------------------------------
    // C. New-file no-clobber race at the commit primitive.
    // -------------------------------------------------------------------

    #[test]
    fn fault_no_clobber_race_between_baseline_recheck_and_hard_link() {
        // After the second baseline-recheck passes (target slot is
        // empty), an external process drops a file into the slot
        // before the executor's `hard_link` runs. The hard_link must
        // refuse — that's the slice-4 no-clobber guarantee.
        let _r = test_hooks::Reset::new();
        test_hooks::with(|h| {
            h.before_commit_hook = Some(Arc::new(|target: &Path| {
                fs::write(target, b"racer-was-here\n").expect("racer write");
            }));
        });

        let fix = build_fixture("noclobber-race", "docs/race.md", None, b"# our-new\n");
        let result = execute_write(&fix.request());

        // The hard_link refusal surfaces as AtomicWriteIoFailed at the
        // Commit stage; the target file was not overwritten.
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::Commit,
                message,
            } => {
                assert!(
                    message.contains("hard_link") || message.contains("no-clobber"),
                    "expected hard_link refusal message, got {message:?}"
                );
            }
            other => panic!("expected AtomicWriteIoFailed{{Commit}}, got {other:?}"),
        }
        // The racer's content is intact.
        let on_disk = fs::read(fix.target_absolute()).expect("read target");
        assert_eq!(on_disk, b"racer-was-here\n");
        // No stray executor temp left behind.
        let stale = find_stale_temps(fix.workspace_root());
        assert!(stale.is_empty(), "stale temps: {stale:?}");
    }

    fn find_stale_temps(root: &Path) -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    let looks_like_executor_temp = name.starts_with(".a2-l2b-write-");
                    let has_tmp_extension = Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"));
                    if looks_like_executor_temp && has_tmp_extension {
                        out.push(p);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out
    }

    // -------------------------------------------------------------------
    // D. Per-stage atomic-write error mapping.
    // -------------------------------------------------------------------

    fn run_with_injected_stage_err(
        label: &str,
        target_rel: &str,
        before: Option<&[u8]>,
        after: &[u8],
        set_hook: impl FnOnce(&mut test_hooks::TestHooks),
    ) -> (super::WriteExecutionResult, Fixture) {
        let _r = test_hooks::Reset::new();
        test_hooks::with(set_hook);
        let fix = build_fixture(label, target_rel, before, after);
        let result = execute_write(&fix.request());
        (result, fix)
    }

    #[test]
    fn fault_temp_create_failure_maps_to_temp_create_stage() {
        let (result, fix) = run_with_injected_stage_err(
            "tempcreate-err",
            "src/lib.rs",
            Some(b"a\n"),
            b"b\n",
            |h| h.inject_temp_create_err = Some(io::ErrorKind::PermissionDenied),
        );
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::TempCreate,
                ..
            } => {}
            other => panic!("expected TempCreate stage, got {other:?}"),
        }
        // Target unchanged (no temp was ever created).
        assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
        assert!(find_stale_temps(fix.workspace_root()).is_empty());
        // The temp-created marker must NOT have been emitted.
        assert!(!result.markers.contains(&L2B_WRITE_TEMP_CREATED));
    }

    #[test]
    fn fault_temp_write_failure_maps_to_temp_write_stage_and_cleans_up() {
        let (result, fix) =
            run_with_injected_stage_err("tempwrite-err", "src/lib.rs", Some(b"a\n"), b"b\n", |h| {
                h.inject_temp_write_err = Some(io::ErrorKind::Other);
            });
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::TempWrite,
                ..
            } => {}
            other => panic!("expected TempWrite stage, got {other:?}"),
        }
        assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
        assert!(
            find_stale_temps(fix.workspace_root()).is_empty(),
            "temp must be cleaned up on TempWrite failure"
        );
    }

    #[test]
    fn fault_temp_fsync_failure_maps_to_temp_fsync_stage_and_cleans_up() {
        let (result, fix) =
            run_with_injected_stage_err("tempfsync-err", "src/lib.rs", Some(b"a\n"), b"b\n", |h| {
                h.inject_temp_fsync_err = Some(io::ErrorKind::Other);
            });
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::TempFsync,
                ..
            } => {}
            other => panic!("expected TempFsync stage, got {other:?}"),
        }
        assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
        assert!(find_stale_temps(fix.workspace_root()).is_empty());
    }

    #[test]
    fn fault_commit_failure_maps_to_commit_stage_and_cleans_up_temp() {
        let (result, fix) =
            run_with_injected_stage_err("commit-err", "src/lib.rs", Some(b"a\n"), b"b\n", |h| {
                h.inject_commit_err = Some(io::ErrorKind::CrossesDevices);
            });
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::Commit,
                ..
            } => {}
            other => panic!("expected Commit stage, got {other:?}"),
        }
        // Target unchanged — commit never ran.
        assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
        assert!(find_stale_temps(fix.workspace_root()).is_empty());
    }

    #[test]
    fn fault_parent_fsync_failure_maps_to_parent_fsync_stage() {
        // Inject on the overwrite branch so commit has already
        // happened — the rename is durable in the page cache but the
        // parent sync_all has not yet been confirmed.
        let (result, fix) = run_with_injected_stage_err(
            "parentfsync-err",
            "src/lib.rs",
            Some(b"a\n"),
            b"b\n",
            |h| h.inject_parent_fsync_err = Some(io::ErrorKind::Other),
        );
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        match &result.outcome {
            WriteExecutionOutcome::AtomicWriteIoFailed {
                stage: WriteStage::ParentFsync,
                ..
            } => {}
            other => panic!("expected ParentFsync stage, got {other:?}"),
        }
        // The bytes are on disk (rename completed); ParentFsync only
        // signals durability uncertainty.
        let on_disk = fs::read(fix.target_absolute()).unwrap();
        assert_eq!(on_disk, b"b\n");
    }

    // -------------------------------------------------------------------
    // E. EXDEV policy: commit failure (incl. EXDEV-class) maps to
    //    AtomicWriteIoFailed at the Commit stage. No copy fallback
    //    is permitted; the executor refuses rather than copying
    //    bytes across a device boundary.
    // -------------------------------------------------------------------

    #[test]
    fn fault_exdev_class_commit_failure_surfaces_as_io_failed_commit() {
        // Synthesize an EXDEV-class error at the commit step. The
        // executor must surface AtomicWriteIoFailed{Commit} and leave
        // the target unchanged — no copy fallback path exists.
        let (result, fix) = run_with_injected_stage_err(
            "exdev",
            "src/lib.rs",
            Some(b"baseline\n"),
            b"after\n",
            |h| h.inject_commit_err = Some(io::ErrorKind::CrossesDevices),
        );
        assert_eq!(result.exit_code, EXIT_WRITE_IO_FAILED);
        let WriteExecutionOutcome::AtomicWriteIoFailed { stage, .. } = &result.outcome else {
            panic!("expected AtomicWriteIoFailed, got {:?}", result.outcome);
        };
        assert_eq!(*stage, WriteStage::Commit);
        // Target untouched — exactly the no-copy-fallback guarantee.
        assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"baseline\n");
    }

    // (Static-source scan for the no-copy-fallback policy lives in
    // tests/l2b_write_executor.rs — placing it here would self-grep
    // and false-positive on the forbidden literals.)

    // -------------------------------------------------------------------
    // Hook hygiene: ensure the Reset guard restores defaults between
    // tests on the same thread (cargo's test runner reuses worker
    // threads).
    // -------------------------------------------------------------------

    #[test]
    fn test_hooks_reset_guard_restores_defaults_on_drop() {
        {
            let _r = test_hooks::Reset::new();
            test_hooks::with(|h| {
                h.force_validation_mismatch = true;
                h.inject_temp_create_err = Some(io::ErrorKind::PermissionDenied);
            });
            let snap = test_hooks::snapshot();
            assert!(snap.force_validation_mismatch);
            assert_eq!(
                snap.inject_temp_create_err,
                Some(io::ErrorKind::PermissionDenied)
            );
        }
        // After drop, state is back to default for the next test on
        // this worker thread.
        let snap = test_hooks::snapshot();
        assert!(!snap.force_validation_mismatch);
        assert!(snap.inject_temp_create_err.is_none());
    }

    // -------------------------------------------------------------------
    // Sanity: an `AtomicWriteError` from injection still surfaces with
    // a populated message (defensive — prevents a future refactor from
    // accidentally collapsing the message field).
    // -------------------------------------------------------------------

    #[test]
    fn injected_atomic_write_error_message_is_non_empty() {
        let (result, _fix) =
            run_with_injected_stage_err("msg-non-empty", "src/lib.rs", Some(b"a\n"), b"b\n", |h| {
                h.inject_temp_create_err = Some(io::ErrorKind::PermissionDenied);
            });
        let WriteExecutionOutcome::AtomicWriteIoFailed { message, stage } = &result.outcome else {
            panic!("expected AtomicWriteIoFailed, got {:?}", result.outcome);
        };
        assert_eq!(*stage, WriteStage::TempCreate);
        assert!(message.contains("test-injected"), "got {message:?}");
        // Round-trip the message via Debug on the error type to pin
        // that the struct shape didn't change.
        let synthesized = AtomicWriteError {
            stage: WriteStage::TempCreate,
            message: message.clone(),
        };
        let _debug_round_trip = format!("{synthesized:?}");
        // Reference unused enum variants so future code-removal lints
        // surface explicitly here, not as dead-code warnings on
        // production-side types.
        let _ = AuthorityMismatch::PreviewNotApprovable;
        let _ = BaselineDrift::ExpectedAbsentButPresent;
    }
}
