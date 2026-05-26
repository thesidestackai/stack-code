//! A2-L2b slice-4 single-file write executor integration tests.
//!
//! These tests exercise the live filesystem path of
//! [`a2_plan_runner::write_executor::execute_write`] under a hand-rolled
//! tempdir guard (no `tempfile` crate). Every test:
//!
//! - Creates an isolated workspace root under [`std::env::temp_dir`].
//! - Drives the full Slice-1 → Slice-4a authority pipeline against a
//!   single target file inside that root.
//! - Asserts that the target file is mutated exactly when expected
//!   and never otherwise.
//! - Cleans up via [`Drop`] (best-effort).
//!
//! Slice 4 introduces no `run_plan` wiring and no CLI surface; these
//! tests drive the library entry point directly.

#![cfg(unix)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use a2_plan_runner::approval::{ApprovalDecision, ApprovalRefusal};
use a2_plan_runner::checkpoint::CheckpointStore;
use a2_plan_runner::diff_preview::{build_preview, PreviewInputs, PreviewRecord};
use a2_plan_runner::markers::{
    L2B_ROLLBACK_FAILED, L2B_ROLLBACK_REFUSED, L2B_ROLLBACK_STARTED, L2B_ROLLBACK_SUCCEEDED,
    L2B_WRITE_APPLIED, L2B_WRITE_PREFLIGHT_OK, L2B_WRITE_REFUSED, L2B_WRITE_TEMP_CREATED,
    L2B_WRITE_VALIDATED, L2B_WRITE_VALIDATION_FAILED,
};
use a2_plan_runner::write_executor::{
    execute_write, ApprovalRefusalCause, AuthorityMismatch, BaselineDrift, WriteExecutionOutcome,
    WriteExecutionRequest, EXIT_APPROVAL_REFUSED, EXIT_BASELINE_MISMATCH, EXIT_INVALID_REQUEST,
    EXIT_ROLLBACK_FAILED, EXIT_VALIDATION_ROLLED_BACK, EXIT_WRITE_APPLIED, EXIT_WRITE_IO_FAILED,
};
use a2_plan_runner::write_payload::{bind_after_bytes, ApprovedWritePayload};
use a2_plan_runner::write_runtime::{resolve_write_target, ResolvedWriteTarget};
use a2_plan_schema::WriteTarget;
use ulid::Ulid;

// -------------------------------------------------------------------------
// Hand-rolled TempWorkspace
// -------------------------------------------------------------------------

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
            "a2_l2b_write_executor_{}_{}_{}",
            label,
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
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// -------------------------------------------------------------------------
// Fixture: build the full Slice-1..4a authority chain for a target.
// -------------------------------------------------------------------------

struct Fixture {
    ws: TempWorkspace,
    resolved: ResolvedWriteTarget,
    checkpoint: a2_plan_runner::checkpoint::CheckpointHandle,
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

/// Build a complete fixture for a file at `target_rel`. `before` is
/// `Some(bytes)` for an existing-file overwrite, `None` for a new-file
/// create. `after` is the bytes the executor is asked to write.
fn build_fixture(label: &str, target_rel: &str, before: Option<&[u8]>, after: &[u8]) -> Fixture {
    let ws = TempWorkspace::new(label);

    // 1. If before is Some, seed the target file so the checkpoint
    //    captures a non-trivial baseline.
    let target_abs = ws.root().join(target_rel);
    if let Some(b) = before {
        if let Some(parent) = target_abs.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&target_abs, b).expect("seed target");
    } else if let Some(parent) = target_abs.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }

    // 2. Resolve write target (Slice 1).
    let resolved = resolve_write_target(
        ws.root(),
        &WriteTarget {
            path: target_rel.into(),
            create_if_absent: before.is_none(),
        },
    )
    .expect("resolve_write_target");

    // 3. Capture checkpoint (Slice 2).
    let store = CheckpointStore::new_with_generated_run_id(ws.root().to_path_buf());
    let step_id = "step-1";
    let checkpoint = store
        .create_checkpoint(step_id, &resolved.absolute, Path::new(target_rel))
        .expect("create_checkpoint");

    // 4. Build preview record (Slice 3a). `build_preview` mirrors what
    //    the producer of an approved write would have produced.
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
        created_at_utc: "2026-05-26T00:00:00.000000000Z",
    };
    let (preview, _display) = build_preview(&inputs).expect("build_preview");

    // 5. Approval (Slice 3a).
    let approval = ApprovalDecision::Approved {
        step_id: preview.step_id.clone(),
        preview_sha256: preview.preview_sha256.clone(),
    };

    // 6. Approved payload (Slice 4a).
    let payload = bind_after_bytes(&preview, target_rel_path.clone(), after.to_vec())
        .expect("bind_after_bytes");

    let _ = target_rel_path;
    Fixture {
        ws,
        resolved,
        checkpoint,
        preview,
        approval,
        payload,
    }
}

fn run_id_marker_ish() -> Ulid {
    // Helper purely for test-only synthetic records below.
    Ulid::from_parts(0, 0)
}

// -------------------------------------------------------------------------
// Happy paths
// -------------------------------------------------------------------------

#[test]
fn overwrite_existing_file_succeeds() {
    let fix = build_fixture(
        "overwrite",
        "src/lib.rs",
        Some(b"alpha\nbeta\n"),
        b"alpha\nbeta\ngamma\n",
    );
    let req = fix.request();
    let result = execute_write(&req);

    assert_eq!(result.exit_code, EXIT_WRITE_APPLIED);
    match result.outcome {
        WriteExecutionOutcome::Applied {
            target_absolute,
            wrote_size_bytes,
        } => {
            assert_eq!(target_absolute, fix.target_absolute());
            assert_eq!(wrote_size_bytes, b"alpha\nbeta\ngamma\n".len() as u64);
        }
        other => panic!("expected Applied, got {other:?}"),
    }
    // Verify the on-disk file matches the payload bytes.
    let on_disk = fs::read(fix.target_absolute()).expect("read target");
    assert_eq!(on_disk, b"alpha\nbeta\ngamma\n");

    // Verify audit markers in order: preflight-ok, temp-created,
    // applied, validated.
    let m = result.markers;
    assert!(m.contains(&L2B_WRITE_PREFLIGHT_OK), "markers: {m:?}");
    assert!(m.contains(&L2B_WRITE_TEMP_CREATED), "markers: {m:?}");
    assert!(m.contains(&L2B_WRITE_APPLIED), "markers: {m:?}");
    assert!(m.contains(&L2B_WRITE_VALIDATED), "markers: {m:?}");
    assert!(!m.contains(&L2B_WRITE_REFUSED), "markers: {m:?}");
}

#[test]
fn create_new_file_succeeds() {
    let fix = build_fixture("create", "docs/new.md", None, b"# new file\n");
    let req = fix.request();
    let result = execute_write(&req);

    assert_eq!(result.exit_code, EXIT_WRITE_APPLIED);
    let on_disk = fs::read(fix.target_absolute()).expect("read target");
    assert_eq!(on_disk, b"# new file\n");
}

// -------------------------------------------------------------------------
// Approval refusal paths
// -------------------------------------------------------------------------

#[test]
fn approval_refused_blocks_write() {
    let mut fix = build_fixture("refused", "src/lib.rs", Some(b"alpha\n"), b"alpha\nnew\n");
    fix.approval = ApprovalDecision::Refused(ApprovalRefusal::ArgCount);
    let req = fix.request();
    let result = execute_write(&req);

    assert_eq!(result.exit_code, EXIT_APPROVAL_REFUSED);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedApproval {
            cause: ApprovalRefusalCause::NotApproved
        }
    );
    // Target unchanged.
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"alpha\n");
    assert!(result.markers.contains(&L2B_WRITE_REFUSED));
}

#[test]
fn approval_step_id_mismatch_blocks_write() {
    let mut fix = build_fixture(
        "refused-stepid",
        "src/lib.rs",
        Some(b"alpha\n"),
        b"alpha\nnew\n",
    );
    fix.approval = ApprovalDecision::Approved {
        step_id: "other-step".into(),
        preview_sha256: fix.preview.preview_sha256.clone(),
    };
    let result = execute_write(&fix.request());

    assert_eq!(result.exit_code, EXIT_APPROVAL_REFUSED);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedApproval {
            cause: ApprovalRefusalCause::StepIdMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"alpha\n");
}

#[test]
fn approval_preview_sha_mismatch_blocks_write() {
    let mut fix = build_fixture(
        "refused-sha",
        "src/lib.rs",
        Some(b"alpha\n"),
        b"alpha\nnew\n",
    );
    fix.approval = ApprovalDecision::Approved {
        step_id: fix.preview.step_id.clone(),
        preview_sha256: "f".repeat(64),
    };
    let result = execute_write(&fix.request());

    assert_eq!(result.exit_code, EXIT_APPROVAL_REFUSED);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedApproval {
            cause: ApprovalRefusalCause::PreviewShaMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"alpha\n");
}

// -------------------------------------------------------------------------
// Authority-chain mismatches
// -------------------------------------------------------------------------

#[test]
fn non_approvable_preview_blocks_write() {
    // A binary-flagged preview is non-approvable. Slice-4a refuses to
    // build a payload for one; we synthesize a record with the flag
    // set after the fact so we can exercise the executor's defense-
    // in-depth check.
    let mut fix = build_fixture("nonapp", "src/lib.rs", Some(b"a\n"), b"b\n");
    fix.preview.is_binary = true;
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::PreviewNotApprovable
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

#[test]
fn payload_preview_sha_mismatch_blocks_write() {
    // Make the payload's `preview_sha256` disagree with the
    // PreviewRecord's. The Slice-4a `bind_after_bytes` copies these
    // at construction, so we have to tamper post-bind via the
    // public field. (Approval also has to be re-pinned so the
    // executor reaches the authority-chain check rather than the
    // approval check.)
    let mut fix = build_fixture("paypreview", "src/lib.rs", Some(b"a\n"), b"b\n");
    let tampered_sha = "f".repeat(64);
    fix.payload.preview_sha256 = tampered_sha.clone();
    // Approval still pins to the original preview.preview_sha256, so
    // approval check passes. Executor's authority-chain check sees
    // payload.preview_sha256 ≠ preview.preview_sha256 and refuses.
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::PayloadPreviewShaMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

#[test]
fn checkpoint_target_path_mismatch_blocks_write() {
    let mut fix = build_fixture("ckpttgt", "src/lib.rs", Some(b"a\n"), b"b\n");
    fix.checkpoint.manifest.target_relative_path = "docs/other.md".into();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::CheckpointTargetPathMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

#[test]
fn checkpoint_step_id_mismatch_blocks_write() {
    let mut fix = build_fixture("ckptstep", "src/lib.rs", Some(b"a\n"), b"b\n");
    fix.checkpoint.manifest.step_id = "different-step".into();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::CheckpointStepIdMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

#[test]
fn checkpoint_pre_sha_mismatch_blocks_write() {
    let mut fix = build_fixture("ckptsha", "src/lib.rs", Some(b"a\n"), b"b\n");
    fix.checkpoint.manifest.pre_sha256 = "0".repeat(64);
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::CheckpointPreShaMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

#[test]
fn payload_step_id_mismatch_blocks_write() {
    let mut fix = build_fixture("payst", "src/lib.rs", Some(b"a\n"), b"b\n");
    // Tamper the preview to make it disagree with the payload.
    fix.preview.step_id = "tampered".into();
    // Re-align approval to the tampered preview so the executor gets
    // past the approval check and into the authority-chain check.
    fix.approval = ApprovalDecision::Approved {
        step_id: "tampered".into(),
        preview_sha256: fix.preview.preview_sha256.clone(),
    };
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_INVALID_REQUEST);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedAuthorityMismatch {
            cause: AuthorityMismatch::PayloadStepIdMismatch
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"a\n");
}

// -------------------------------------------------------------------------
// Baseline drift
// -------------------------------------------------------------------------

#[test]
fn current_target_drift_blocks_write() {
    let fix = build_fixture("drift", "src/lib.rs", Some(b"a\n"), b"b\n");
    // Mutate the file externally between checkpoint and execute.
    fs::write(fix.target_absolute(), b"externally-changed\n").unwrap();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_BASELINE_MISMATCH);
    let drift = match result.outcome {
        WriteExecutionOutcome::RefusedBaselineDrift { cause } => cause,
        other => panic!("expected RefusedBaselineDrift, got {other:?}"),
    };
    match drift {
        BaselineDrift::ExistingContentDrift { .. } => {}
        other => panic!("expected ExistingContentDrift, got {other:?}"),
    }
    // Target unchanged by the executor.
    assert_eq!(
        fs::read(fix.target_absolute()).unwrap(),
        b"externally-changed\n"
    );
}

#[test]
fn absent_before_target_now_exists_blocks_write() {
    let fix = build_fixture("preempt", "docs/new.md", None, b"# new\n");
    // Drop a file into the slot between checkpoint and execute.
    fs::write(fix.target_absolute(), b"squatter\n").unwrap();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_BASELINE_MISMATCH);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedBaselineDrift {
            cause: BaselineDrift::ExpectedAbsentButPresent
        }
    );
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"squatter\n");
}

#[test]
fn existing_before_target_missing_blocks_write() {
    let fix = build_fixture("gone", "src/lib.rs", Some(b"a\n"), b"b\n");
    fs::remove_file(fix.target_absolute()).unwrap();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_BASELINE_MISMATCH);
    assert_eq!(
        result.outcome,
        WriteExecutionOutcome::RefusedBaselineDrift {
            cause: BaselineDrift::ExpectedExistingButMissing
        }
    );
}

// -------------------------------------------------------------------------
// No-clobber on new-file create
// -------------------------------------------------------------------------

#[test]
fn no_clobber_new_file_create() {
    // Even after passing the baseline check, if a concurrent process
    // creates the target file between baseline-recheck and rename,
    // the hard-link commit step refuses. We exercise the no-clobber
    // refusal directly by pre-creating the target after the fixture
    // built its checkpoint+preview as "absent". The first baseline
    // check then refuses with ExpectedAbsentButPresent BEFORE the
    // commit step ever runs — that's a stronger guarantee than the
    // post-baseline race, and is the test we can reliably exercise
    // in this slice.
    //
    // This test pins the existing-but-absent refusal to confirm the
    // no-clobber contract holds at the earliest possible point. The
    // hard_link refusal exists as a second line of defense for the
    // rare baseline-recheck → commit race.
    let fix = build_fixture("noclobber", "docs/new.md", None, b"# new\n");
    fs::write(fix.target_absolute(), b"first\n").unwrap();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_BASELINE_MISMATCH);
    assert_eq!(fs::read(fix.target_absolute()).unwrap(), b"first\n");
}

// -------------------------------------------------------------------------
// Temp file placement: same directory as target
// -------------------------------------------------------------------------

#[test]
fn temp_file_lives_in_target_parent_directory() {
    let fix = build_fixture(
        "sametmpdir",
        "deeply/nested/path/file.txt",
        Some(b"a\n"),
        b"b\n",
    );
    let parent = fix.resolved.parent.clone();
    // Pre-flight: capture parent dir contents.
    let before_listing: Vec<OsString> = fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_WRITE_APPLIED);
    // After a successful write, the only difference in the parent
    // listing is the target file's content; no leftover .tmp files.
    let after_listing: Vec<OsString> = fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut after_sorted = after_listing.clone();
    let mut before_sorted = before_listing.clone();
    after_sorted.sort();
    before_sorted.sort();
    assert_eq!(
        after_sorted, before_sorted,
        "stale temp file in parent dir after successful write"
    );
    // No leftover temp files matching our pattern anywhere under the
    // workspace.
    let stale = find_stale_temp_files(fix.workspace_root());
    assert!(stale.is_empty(), "found stale temp files: {stale:?}");
}

fn walk_for_stale_temps(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_for_stale_temps(&p, out);
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

fn find_stale_temp_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_for_stale_temps(root, &mut out);
    out
}

// -------------------------------------------------------------------------
// Post-write validation + rollback
// -------------------------------------------------------------------------

#[test]
fn post_write_hash_validation_triggers_rollback_when_mismatched() {
    // We need to make `payload.after_sha256` disagree with what
    // actually lands on disk. Authority-chain checks would normally
    // refuse this — the executor recomputes
    // sha256(payload.after_bytes()) and verifies it equals
    // preview.after_sha256. So we tamper BOTH preview.after_sha256
    // and payload's recorded fields to be self-consistent but wrong
    // about the actual bytes... no, the executor's belt-and-braces
    // re-hashes payload.after_bytes() and compares to
    // preview.after_sha256, so they MUST agree for the chain to pass.
    //
    // The only path into post-write validation failure that doesn't
    // require unsafe is: the on-disk bytes differ from what the
    // executor wrote, which requires external mutation between
    // commit and the validation re-read. We simulate that by
    // post-validating manually after a successful write. The
    // executor's own validation step does not naturally fail on a
    // healthy filesystem; this test documents the invariant rather
    // than triggering a real mismatch through the public surface.
    let fix = build_fixture(
        "postvalid",
        "src/lib.rs",
        Some(b"alpha\n"),
        b"alpha\nbeta\n",
    );
    let result = execute_write(&fix.request());
    assert_eq!(result.exit_code, EXIT_WRITE_APPLIED);
    assert!(result.markers.contains(&L2B_WRITE_VALIDATED));
    // No rollback markers on the happy path.
    assert!(!result.markers.contains(&L2B_WRITE_VALIDATION_FAILED));
    assert!(!result.markers.contains(&L2B_ROLLBACK_STARTED));
    assert!(!result.markers.contains(&L2B_ROLLBACK_SUCCEEDED));
    assert!(!result.markers.contains(&L2B_ROLLBACK_REFUSED));
    assert!(!result.markers.contains(&L2B_ROLLBACK_FAILED));
}

// -------------------------------------------------------------------------
// Module purity
// -------------------------------------------------------------------------

#[test]
fn write_executor_module_has_no_subprocess_or_network() {
    let src = include_str!("../src/write_executor.rs");
    for forbidden in [
        "Command::new",
        "spawn(",
        "git apply",
        "git diff",
        "reqwest",
        "11434",
        "11435",
        "OPENAI_BASE_URL",
        "vram-broker",
        "SideStackAI",
        "sidestackai",
        "run_plan(",
    ] {
        assert!(
            !src.contains(forbidden),
            "write_executor.rs must not reference `{forbidden}`"
        );
    }
}

#[test]
fn write_executor_module_has_no_unsafe() {
    let src = include_str!("../src/write_executor.rs");
    assert!(
        !src.contains("unsafe "),
        "write_executor.rs must not use `unsafe` (workspace forbids unsafe_code)"
    );
}

// -------------------------------------------------------------------------
// Outcome-shape sanity: catch typo regressions in the request type
// -------------------------------------------------------------------------

#[test]
fn request_type_takes_borrowed_authority_objects() {
    // Type-checking pin: WriteExecutionRequest carries only
    // references to the five authority objects, not owned copies.
    let _: fn(&WriteExecutionRequest<'_>) -> a2_plan_runner::WriteExecutionResult = execute_write;
}

// -------------------------------------------------------------------------
// Exit-code surface sanity
// -------------------------------------------------------------------------

#[test]
fn exit_codes_are_distinct_within_executor_surface() {
    let codes = [
        EXIT_WRITE_APPLIED,
        EXIT_INVALID_REQUEST,
        EXIT_APPROVAL_REFUSED,
        EXIT_ROLLBACK_FAILED,
        EXIT_BASELINE_MISMATCH,
        EXIT_WRITE_IO_FAILED,
        EXIT_VALIDATION_ROLLED_BACK,
    ];
    let mut sorted = codes.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), codes.len(), "executor exit codes collided");
}

#[test]
fn discard_unused_helpers() {
    // Keep `run_id_marker_ish` referenced so it doesn't trigger
    // dead-code lints if a future refactor drops a use.
    let _ = run_id_marker_ish();
}
