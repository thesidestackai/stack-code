//! Runner-emitted operator-facing markers.
//!
//! Every marker is a stable `&'static str` token greppable from CI logs and
//! operator transcripts. L1a markers re-exported from
//! [`a2_plan_schema::plan_validate::markers`] pass through verbatim — this
//! module only owns the runner-specific tokens, all prefixed `a2-l1b-`.
//!
//! The `a2-l1b-` prefix is the **justified divergence** from L1a's
//! `a2-l1-` prefix: L1a markers are emitted by the offline validator and
//! describe schema-level acceptance; L1b markers are emitted by the runner
//! and describe execution-layer state. Existing scrapers built for L1a
//! continue to work because L1a markers still appear when the validator
//! runs as pre-flight.

/// First marker the runner emits, before any other action.
pub const RUNNER_START: &str = "a2-l1b-runner-start";

/// The schema validator returned `Refused`. The runner refuses to execute
/// (workspace-write, DEEP, or missing tools). Exit code 2.
pub const PLAN_REFUSED_PRECHECK: &str = "a2-l1b-plan-refused-precheck";

/// A step declared a tool outside the runner's read-only allowlist.
/// Exit code 3.
pub const TOOL_DISALLOWED: &str = "a2-l1b-tool-disallowed";

/// The substrate probe failed (LAW 1 refusal, missing model, or network).
/// Exit code 4.
pub const SUBSTRATE_UNAVAILABLE: &str = "a2-l1b-substrate-unavailable";

/// Every accepted step passed. Exit code 0.
pub const PLAN_EXEC_PASS: &str = "a2-l1b-plan-exec-pass";

/// At least one accepted step failed. Exit code 1.
pub const PLAN_EXEC_FAIL: &str = "a2-l1b-plan-exec-fail";

/// Per-step boundary marker, emitted immediately before subprocess spawn.
pub const STEP_STARTED: &str = "a2-l1b-step-started";

/// Step finished within `MAX_TURNS` and met its declared contract.
pub const STEP_PASSED: &str = "a2-l1b-step-passed";

/// Step failed (non-2xx / empty body / tool violation / `must_contain`
/// miss / turn-loop exceeded).
pub const STEP_FAILED: &str = "a2-l1b-step-failed";

/// Step skipped because an earlier step failed under default abort.
pub const STEP_SKIPPED: &str = "a2-l1b-step-skipped";

// --- A2-L2b workspace-write markers --------------------------------------
//
// L2b additions are scoped to the workspace-write runner. They never appear
// in an L1b read-only run. The `a2-l2b-` prefix is a separate operator
// contract from `a2-l1b-` so scrapers can opt in or out per layer.
//
// Slice 1 only introduces the path-safety markers. Approval, checkpoint,
// diff, write, and rollback markers land in later slices.

/// First L2b marker emitted at the start of a workspace-write run.
/// Not emitted by anything in slice 1; consumers wire it in slice 2.
pub const L2B_RUNNER_START: &str = "a2-l2b-runner-start";

/// Per-step boundary marker emitted at the start of a workspace-write step.
pub const L2B_STEP_STARTED: &str = "a2-l2b-step-started";

/// Path safety resolution succeeded; the workspace-write step is permitted
/// to proceed to the (later) approval / write phases.
pub const L2B_PATH_RESOLVED: &str = "a2-l2b-path-resolved";

/// Runtime path safety refused: absolute path, `..` traversal,
/// canonical-parent prefix mismatch, deny-component, or deny-glob filename.
/// Maps to exit code 6.
pub const L2B_PATH_REFUSED_RUNTIME: &str = "a2-l2b-path-refused-runtime";

/// Symlink refused: a component in the parent chain is a symlink, or the
/// final target itself is a symlink. Maps to exit code 6.
pub const L2B_SYMLINK_REFUSED: &str = "a2-l2b-symlink-refused";

/// The parent directory of the requested write target does not exist (or
/// is not a directory). L2b refuses parent creation; that policy is
/// deferred. Maps to exit code 6.
pub const L2B_PARENT_MISSING: &str = "a2-l2b-parent-missing";

// --- A2-L2b checkpoint-store markers (slice 2) ---------------------------
//
// Slice 2 adds the checkpoint store under `<workspace_root>/.claw/`. The
// run-id marker is a *formatted* token: a stable prefix followed by the
// 26-character Crockford-base32 ULID minted once per plan run. The other
// three are plain `&'static str` constants. All four pin to the `a2-l2b-`
// operator contract established by slice 1.

/// Prefix of the per-plan-run announcement marker. Followed immediately
/// by a 26-char Crockford-base32 ULID. Operators grep `^a2-l2b-run-id=`.
/// Build the full token via [`l2b_run_id_marker`].
pub const L2B_RUN_ID_PREFIX: &str = "a2-l2b-run-id=";

/// Emitted after `checkpoint::CheckpointStore::create_checkpoint` returns
/// `Ok`. The single positive signal that the pre-write state was captured
/// durably under the runner-owned checkpoint root.
pub const L2B_CHECKPOINT_WRITTEN: &str = "a2-l2b-checkpoint-written";

/// Emitted when a target file's pre-write size exceeds
/// `checkpoint::MAX_CHECKPOINT_BYTES`. Maps to exit code 9.
pub const L2B_CHECKPOINT_TOO_LARGE: &str = "a2-l2b-checkpoint-too-large";

/// Generic checkpoint-store refusal (overwrite of an existing step dir,
/// invalid step-id, non-regular-file target, or any other I/O failure
/// during checkpoint write). Maps to exit code 9. Sized refusal is its
/// own marker via [`L2B_CHECKPOINT_TOO_LARGE`].
pub const L2B_CHECKPOINT_REFUSED: &str = "a2-l2b-checkpoint-refused";

/// Build the per-plan-run announcement marker (`a2-l2b-run-id=<ulid>`).
///
/// The result is the concatenation of [`L2B_RUN_ID_PREFIX`] and the
/// 26-character Crockford-base32 rendering of `id`. The format is stable
/// — operators grep `^a2-l2b-run-id=` and parse the 26 chars that follow
/// the `=`.
#[must_use]
pub fn l2b_run_id_marker(id: &ulid::Ulid) -> String {
    format!("{L2B_RUN_ID_PREFIX}{id}")
}

// --- A2-L2b slice-3a diff-preview + approval markers ---------------------
//
// Every token in this section is **audit-only**: the runtime decides
// approval from the [`PreviewRecord`][crate::diff_preview::PreviewRecord]
// + the [`ApprovalDecision`][crate::approval::ApprovalDecision] structure
// alone. The markers are intended for log scrapers and operator
// transcripts; emitting any of them is never authority for an approval
// outcome. Slice 3a is offline-only; nothing here is wired into
// [`crate::runner::run_plan`].

/// A [`crate::diff_preview::PreviewRecord`] was constructed for a step.
/// Emitted regardless of whether the resulting preview is approvable.
pub const L2B_PREVIEW_RECORD_CREATED: &str = "a2-l2b-preview-record-created";

/// The textual unified diff was generated, sanitized, and is ready to
/// surface to a human. Implies non-binary, non-redacted, non-truncated.
pub const L2B_DIFF_PREVIEW_READY: &str = "a2-l2b-diff-preview-ready";

/// At least one redaction pattern matched the rendered preview content
/// (key=value secrets, PEM blocks, vendor token prefixes, JWT-shaped
/// strings, URL credentials, etc.). The preview is non-approvable in
/// slice 3a.
pub const L2B_DIFF_REDACTED: &str = "a2-l2b-diff-redacted";

/// The preview hit a deterministic line- or byte-truncation limit. The
/// preview is non-approvable in slice 3a.
pub const L2B_DIFF_TRUNCATED: &str = "a2-l2b-diff-truncated";

/// The before- or after-content was detected as binary; no diff body is
/// rendered. Summary metadata only; non-approvable in slice 3a.
pub const L2B_BINARY_PREVIEW: &str = "a2-l2b-binary-preview";

/// A textual approval prompt was surfaced for a `(step_id, preview_sha256)`
/// pair. Audit-only: the prompt's appearance never implies the operator
/// will approve, and the operator's response is bound by the strict
/// parser in [`crate::approval`].
pub const L2B_APPROVAL_PROMPT: &str = "a2-l2b-approval-prompt";

/// An [`crate::approval::ApprovalDecision::Approved`] was produced for the
/// step. Mirrors the structured decision in the report stream; the
/// decision struct itself remains the source of truth.
pub const L2B_APPROVED: &str = "a2-l2b-approved";

/// An [`crate::approval::ApprovalDecision::Refused`] was produced for the
/// step (syntax violation, hash mismatch, preview non-approvable, or
/// checkpoint drift). Carries exit code 7 when surfaced by the CLI.
pub const L2B_APPROVAL_REFUSED: &str = "a2-l2b-approval-refused";

/// Slice-3a refusal of any preapproval / `--yes` / batch-apply attempt.
/// The runtime hard-rejects all such inputs ahead of the strict parser
/// and emits this marker for audit.
pub const L2B_PREAPPROVAL_REFUSED: &str = "a2-l2b-preapproval-refused";

// --- A2-L2b slice-4 write-executor markers -------------------------------
//
// Audit-only operator tokens emitted by
// [`crate::write_executor::execute_write`]. The structured
// [`crate::write_executor::WriteExecutionResult`] is authoritative;
// these markers exist for log scrapers and operator transcripts.

/// The full authority chain matched and the live baseline matched the
/// checkpoint manifest. The executor is about to attempt the atomic
/// write. Audit-only.
pub const L2B_WRITE_PREFLIGHT_OK: &str = "a2-l2b-write-preflight-ok";

/// Write refused before any I/O on the target: approval failure,
/// authority-chain mismatch, or baseline drift. Audit-only; the
/// structured outcome carries the specific cause.
pub const L2B_WRITE_REFUSED: &str = "a2-l2b-write-refused";

/// Same-directory temp file was created and the payload bytes were
/// written + fsynced. Emitted before the commit step. Audit-only.
pub const L2B_WRITE_TEMP_CREATED: &str = "a2-l2b-write-temp-created";

/// Commit rename (or no-clobber hard-link) succeeded and the parent
/// directory has been fsynced. The target now holds the approved
/// after-bytes. Audit-only.
pub const L2B_WRITE_APPLIED: &str = "a2-l2b-write-applied";

/// Post-commit hash re-read matched `payload.after_sha256`. The
/// happy-path terminal marker for [`execute_write`]. Audit-only.
pub const L2B_WRITE_VALIDATED: &str = "a2-l2b-write-validated";

/// Post-commit hash re-read did NOT match `payload.after_sha256`. The
/// executor proceeds to bounded rollback; the structured outcome
/// carries the mismatch detail. Audit-only.
pub const L2B_WRITE_VALIDATION_FAILED: &str = "a2-l2b-write-validation-failed";

/// Rollback was initiated following a post-write validation failure.
/// Audit-only.
pub const L2B_ROLLBACK_STARTED: &str = "a2-l2b-rollback-started";

/// Rollback completed: target now matches the pre-write baseline
/// (overwrite branch) or is absent again (new-file branch).
/// Audit-only.
pub const L2B_ROLLBACK_SUCCEEDED: &str = "a2-l2b-rollback-succeeded";

/// Rollback was refused because the on-disk file no longer matches
/// what the executor just wrote (external mutation between commit
/// and rollback), the target became non-regular, the parent
/// disappeared, or the checkpoint baseline was missing.
/// Audit-only; maps to exit code 8.
pub const L2B_ROLLBACK_REFUSED: &str = "a2-l2b-rollback-refused";

/// Rollback I/O failed mid-flight. The target may be in a partially-
/// rolled-back state and operator attention is required.
/// Audit-only; maps to exit code 8.
pub const L2B_ROLLBACK_FAILED: &str = "a2-l2b-rollback-failed";

// --- A2-L2b run_plan write-preview wiring markers (slice L2b-run-plan) ---
//
// Audit-only operator tokens emitted by
// [`crate::runner::run_plan_with_write_preview`]. The structured
// [`crate::runner::WritePreviewRunReport`] is authoritative; these tokens
// exist for log scrapers and operator transcripts.
//
// The run-plan write-preview path is *preview-only*: it never executes
// `execute_write`, never calls `claw plan approve`, `claw plan
// apply-bundle`, or `claw plan apply`, and never mutates the target file.
// `L2B_PLAN_HALTED` is the terminal marker that pins this contract — every
// successful write-preview run ends on it.

/// The runner detected a workspace-write step but the operator did not
/// pass `--workspace-write-preview`. Audit-only; the structured refusal
/// is authoritative.
pub const L2B_PLAN_WRITE_OPT_IN_REQUIRED: &str = "a2-l2b-plan-write-opt-in-required";

/// The runner refused a plan whose `mode: workspace-write` step count is
/// greater than one. Audit-only.
pub const L2B_PLAN_MULTI_WRITE_REFUSED: &str = "a2-l2b-plan-multi-write-refused";

/// Preview-pending state for the lone workspace-write step. Emitted after
/// the bundle is written and before [`L2B_PLAN_HALTED`]. Audit-only.
pub const L2B_APPROVAL_PENDING: &str = "a2-l2b-approval-pending";

/// The run-plan write-preview path halted before approval / apply.
/// Audit-only terminal marker; pins the preview-only contract.
pub const L2B_PLAN_HALTED: &str = "a2-l2b-plan-halted";

/// Plan-level signal: preview artifacts for the lone workspace-write
/// step are on disk and the plan halted before approval. Audit-only.
pub const L2B_RUN_PLAN_WRITE_PREVIEW_READY: &str = "a2-l2b-run-plan-write-preview-ready";

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinning test: marker tokens are an operator-facing contract.
    /// Renaming any of these breaks log scrapers and is a breaking change.
    #[test]
    fn marker_tokens_are_pinned() {
        assert_eq!(RUNNER_START, "a2-l1b-runner-start");
        assert_eq!(PLAN_REFUSED_PRECHECK, "a2-l1b-plan-refused-precheck");
        assert_eq!(TOOL_DISALLOWED, "a2-l1b-tool-disallowed");
        assert_eq!(SUBSTRATE_UNAVAILABLE, "a2-l1b-substrate-unavailable");
        assert_eq!(PLAN_EXEC_PASS, "a2-l1b-plan-exec-pass");
        assert_eq!(PLAN_EXEC_FAIL, "a2-l1b-plan-exec-fail");
        assert_eq!(STEP_STARTED, "a2-l1b-step-started");
        assert_eq!(STEP_PASSED, "a2-l1b-step-passed");
        assert_eq!(STEP_FAILED, "a2-l1b-step-failed");
        assert_eq!(STEP_SKIPPED, "a2-l1b-step-skipped");
    }

    #[test]
    fn all_runner_markers_use_a2_l1b_prefix() {
        for m in [
            RUNNER_START,
            PLAN_REFUSED_PRECHECK,
            TOOL_DISALLOWED,
            SUBSTRATE_UNAVAILABLE,
            PLAN_EXEC_PASS,
            PLAN_EXEC_FAIL,
            STEP_STARTED,
            STEP_PASSED,
            STEP_FAILED,
            STEP_SKIPPED,
        ] {
            assert!(
                m.starts_with("a2-l1b-"),
                "runner marker {m:?} must use a2-l1b- prefix"
            );
        }
    }

    /// L2b pinning test: workspace-write marker tokens are an operator
    /// contract distinct from L1b. Renaming any of these breaks scrapers
    /// keyed on L2b runs and is a breaking change.
    #[test]
    fn l2b_marker_tokens_are_pinned() {
        assert_eq!(L2B_RUNNER_START, "a2-l2b-runner-start");
        assert_eq!(L2B_STEP_STARTED, "a2-l2b-step-started");
        assert_eq!(L2B_PATH_RESOLVED, "a2-l2b-path-resolved");
        assert_eq!(L2B_PATH_REFUSED_RUNTIME, "a2-l2b-path-refused-runtime");
        assert_eq!(L2B_SYMLINK_REFUSED, "a2-l2b-symlink-refused");
        assert_eq!(L2B_PARENT_MISSING, "a2-l2b-parent-missing");
    }

    #[test]
    fn all_l2b_markers_use_a2_l2b_prefix() {
        for m in [
            L2B_RUNNER_START,
            L2B_STEP_STARTED,
            L2B_PATH_RESOLVED,
            L2B_PATH_REFUSED_RUNTIME,
            L2B_SYMLINK_REFUSED,
            L2B_PARENT_MISSING,
        ] {
            assert!(
                m.starts_with("a2-l2b-"),
                "L2b marker {m:?} must use a2-l2b- prefix"
            );
        }
    }

    /// L2b checkpoint-store marker pinning (slice 2). The literal byte
    /// values are an operator contract; renaming any of them breaks log
    /// scrapers and is a breaking change.
    #[test]
    fn l2b_checkpoint_marker_tokens_are_pinned() {
        assert_eq!(L2B_RUN_ID_PREFIX, "a2-l2b-run-id=");
        assert_eq!(L2B_CHECKPOINT_WRITTEN, "a2-l2b-checkpoint-written");
        assert_eq!(L2B_CHECKPOINT_TOO_LARGE, "a2-l2b-checkpoint-too-large");
        assert_eq!(L2B_CHECKPOINT_REFUSED, "a2-l2b-checkpoint-refused");
    }

    #[test]
    fn all_l2b_checkpoint_static_markers_use_a2_l2b_prefix() {
        // Prefix-only token is exercised by its own dedicated test; the
        // three full markers each must start with `a2-l2b-`.
        for m in [
            L2B_CHECKPOINT_WRITTEN,
            L2B_CHECKPOINT_TOO_LARGE,
            L2B_CHECKPOINT_REFUSED,
        ] {
            assert!(
                m.starts_with("a2-l2b-"),
                "L2b checkpoint marker {m:?} must use a2-l2b- prefix"
            );
        }
    }

    #[test]
    fn l2b_run_id_prefix_shape() {
        assert!(L2B_RUN_ID_PREFIX.starts_with("a2-l2b-"));
        assert!(L2B_RUN_ID_PREFIX.ends_with('='));
    }

    #[test]
    fn l2b_run_id_marker_format() {
        let id = ulid::Ulid::new();
        let s = l2b_run_id_marker(&id);
        assert!(s.starts_with(L2B_RUN_ID_PREFIX));
        // Crockford-base32 ULID rendering is always 26 ASCII chars.
        let body = &s[L2B_RUN_ID_PREFIX.len()..];
        assert_eq!(body.len(), 26);
        assert!(
            body.chars().all(|c| c.is_ascii_alphanumeric()),
            "ULID body must be ASCII alphanumeric Crockford-base32: {body:?}"
        );
        // Total length is prefix + 26.
        assert_eq!(s.len(), L2B_RUN_ID_PREFIX.len() + 26);
    }

    #[test]
    fn l2b_run_id_marker_round_trips_through_ulid_parse() {
        let id = ulid::Ulid::new();
        let s = l2b_run_id_marker(&id);
        let body = &s[L2B_RUN_ID_PREFIX.len()..];
        let parsed: ulid::Ulid = body.parse().expect("ULID body must parse");
        assert_eq!(parsed, id);
    }

    /// L2b slice-3a marker pinning. Every slice-3a token is **audit-only**:
    /// emitting one is never authority for an approval decision. Renaming
    /// any of these breaks scrapers and is a breaking change.
    #[test]
    fn l2b_slice_3a_marker_tokens_are_pinned() {
        assert_eq!(L2B_PREVIEW_RECORD_CREATED, "a2-l2b-preview-record-created");
        assert_eq!(L2B_DIFF_PREVIEW_READY, "a2-l2b-diff-preview-ready");
        assert_eq!(L2B_DIFF_REDACTED, "a2-l2b-diff-redacted");
        assert_eq!(L2B_DIFF_TRUNCATED, "a2-l2b-diff-truncated");
        assert_eq!(L2B_BINARY_PREVIEW, "a2-l2b-binary-preview");
        assert_eq!(L2B_APPROVAL_PROMPT, "a2-l2b-approval-prompt");
        assert_eq!(L2B_APPROVED, "a2-l2b-approved");
        assert_eq!(L2B_APPROVAL_REFUSED, "a2-l2b-approval-refused");
        assert_eq!(L2B_PREAPPROVAL_REFUSED, "a2-l2b-preapproval-refused");
    }

    #[test]
    fn all_l2b_slice_3a_markers_use_a2_l2b_prefix() {
        for m in [
            L2B_PREVIEW_RECORD_CREATED,
            L2B_DIFF_PREVIEW_READY,
            L2B_DIFF_REDACTED,
            L2B_DIFF_TRUNCATED,
            L2B_BINARY_PREVIEW,
            L2B_APPROVAL_PROMPT,
            L2B_APPROVED,
            L2B_APPROVAL_REFUSED,
            L2B_PREAPPROVAL_REFUSED,
        ] {
            assert!(
                m.starts_with("a2-l2b-"),
                "L2b slice-3a marker {m:?} must use a2-l2b- prefix"
            );
        }
    }

    /// L2b slice-4 write-executor marker pinning. Every token is
    /// audit-only; the structured `WriteExecutionResult` is
    /// authoritative. Renaming any of these breaks scrapers and is a
    /// breaking change.
    #[test]
    fn l2b_slice_4_marker_tokens_are_pinned() {
        assert_eq!(L2B_WRITE_PREFLIGHT_OK, "a2-l2b-write-preflight-ok");
        assert_eq!(L2B_WRITE_REFUSED, "a2-l2b-write-refused");
        assert_eq!(L2B_WRITE_TEMP_CREATED, "a2-l2b-write-temp-created");
        assert_eq!(L2B_WRITE_APPLIED, "a2-l2b-write-applied");
        assert_eq!(L2B_WRITE_VALIDATED, "a2-l2b-write-validated");
        assert_eq!(
            L2B_WRITE_VALIDATION_FAILED,
            "a2-l2b-write-validation-failed"
        );
        assert_eq!(L2B_ROLLBACK_STARTED, "a2-l2b-rollback-started");
        assert_eq!(L2B_ROLLBACK_SUCCEEDED, "a2-l2b-rollback-succeeded");
        assert_eq!(L2B_ROLLBACK_REFUSED, "a2-l2b-rollback-refused");
        assert_eq!(L2B_ROLLBACK_FAILED, "a2-l2b-rollback-failed");
    }

    #[test]
    fn all_l2b_slice_4_markers_use_a2_l2b_prefix() {
        for m in [
            L2B_WRITE_PREFLIGHT_OK,
            L2B_WRITE_REFUSED,
            L2B_WRITE_TEMP_CREATED,
            L2B_WRITE_APPLIED,
            L2B_WRITE_VALIDATED,
            L2B_WRITE_VALIDATION_FAILED,
            L2B_ROLLBACK_STARTED,
            L2B_ROLLBACK_SUCCEEDED,
            L2B_ROLLBACK_REFUSED,
            L2B_ROLLBACK_FAILED,
        ] {
            assert!(
                m.starts_with("a2-l2b-"),
                "L2b slice-4 marker {m:?} must use a2-l2b- prefix"
            );
        }
    }

    /// L2b run-plan write-preview marker pinning. Every token is
    /// audit-only; the structured `WritePreviewRunReport` is
    /// authoritative. Renaming any of these breaks scrapers and is a
    /// breaking change.
    #[test]
    fn l2b_run_plan_write_preview_marker_tokens_are_pinned() {
        assert_eq!(
            L2B_PLAN_WRITE_OPT_IN_REQUIRED,
            "a2-l2b-plan-write-opt-in-required"
        );
        assert_eq!(
            L2B_PLAN_MULTI_WRITE_REFUSED,
            "a2-l2b-plan-multi-write-refused"
        );
        assert_eq!(L2B_APPROVAL_PENDING, "a2-l2b-approval-pending");
        assert_eq!(L2B_PLAN_HALTED, "a2-l2b-plan-halted");
        assert_eq!(
            L2B_RUN_PLAN_WRITE_PREVIEW_READY,
            "a2-l2b-run-plan-write-preview-ready"
        );
    }

    #[test]
    fn all_l2b_run_plan_write_preview_markers_use_a2_l2b_prefix() {
        for m in [
            L2B_PLAN_WRITE_OPT_IN_REQUIRED,
            L2B_PLAN_MULTI_WRITE_REFUSED,
            L2B_APPROVAL_PENDING,
            L2B_PLAN_HALTED,
            L2B_RUN_PLAN_WRITE_PREVIEW_READY,
        ] {
            assert!(
                m.starts_with("a2-l2b-"),
                "L2b run-plan write-preview marker {m:?} must use a2-l2b- prefix"
            );
        }
    }
}
