//! A2 read-only plan runner (L1b).
//!
//! Layered on top of [`a2_plan_schema`]: this crate parses an L1a-validated
//! plan, refuses anything the schema validator marked Refused, refuses any
//! step that declares a tool outside the runner's static read-only allowlist
//! (`Read`, `Grep`, `Glob`, `LS`), and executes only accepted steps against
//! the local FAST substrate via the PR #14 `claw-sidestack-local` wrapper.
//!
//! # Hard contract
//!
//! - READ-ONLY ONLY. The runner never opens a write-capable file handle,
//!   never invokes `Edit`/`Write`/`Bash`/`NotebookEdit`, and never shells out
//!   except to the `claw-sidestack-local` wrapper.
//! - DEEP is unreachable. No CLI flag, env var, or alias may select DEEP.
//! - The runner never writes to disk. All output is stdout/stderr.
//! - Tool allowlist is a `const` in code, never operator-configurable.
//! - Workspace-write is A2-L2 and entirely out of scope here.
//!
//! # Substrate
//!
//! - broker: `http://127.0.0.1:11435/v1`
//! - FAST model: `qwen3:14b`
//! - broker PRs #92 (4xx propagation) and #93 (tool-args translation) are
//!   prerequisites; the runner does not duplicate that logic.
//! - claw `git_sha 95d3409` or later (PR #17 envelope fix in place).
//!
//! # Module layout
//!
//! - [`markers`] — stable operator-facing report markers.
//! - [`preflight`] — pure pre-execution refusal checks.
//! - [`runner`] — step executor (subprocess + broker boundary).
//! - [`report`] — marker-stream / JSON writers.
//! - [`write_runtime`] — A2-L2b workspace-write runtime path-safety
//!   resolver (slice 1 only; not yet wired into [`runner::run_plan`],
//!   never performs filesystem writes, never spawns subprocesses, never
//!   prompts for approval).
//! - [`checkpoint`] — A2-L2b workspace-write checkpoint store (slice 2;
//!   captures pre-write target state under
//!   `<workspace_root>/.claw/l2b-checkpoints/`, never mutates the target
//!   file, never wires into [`runner::run_plan`]).
//! - [`diff_preview`] — A2-L2b workspace-write diff-preview primitive
//!   (slice 3a; produces a structured [`diff_preview::PreviewRecord`]
//!   plus a sanitized human display, never writes to the target,
//!   never wires into [`runner::run_plan`]).
//! - [`approval`] — A2-L2b workspace-write approval primitive
//!   (slice 3a; strict parser + structured
//!   [`approval::ApprovalDecision`]; markers in [`markers`] are
//!   audit-only and never authority for an approval outcome).
//! - [`approval_ux`] — A2-L2b workspace-write approval UX helpers
//!   (slice 3b; pure renderers for operator preview / approval
//!   prompts plus a convenience wrapper around
//!   [`approval::evaluate_approval`]. Never writes the target, never
//!   wires into [`runner::run_plan`], never reads stdin or writes
//!   stdout).
//! - [`write_payload`] — A2-L2b workspace-write byte-authority object
//!   (slice 4a; binds exact after-bytes to an approved
//!   [`diff_preview::PreviewRecord`] via SHA-256. Carries the only
//!   in-memory raw-bytes channel between preview producer and the
//!   future Slice-4 executor. Never serialized, never written to
//!   disk, never wired into the L1b runner).
//! - [`write_executor`] — A2-L2b single-file write executor (slice 4;
//!   first mutation-capable A2 surface. Same-directory temp + atomic
//!   rename + parent fsync + post-write hash validation + bounded
//!   rollback. Library entry point only — no CLI wiring and no
//!   integration with the L1b runner in this slice).

pub mod approval;
pub mod approval_ux;
pub mod checkpoint;
pub mod diff_preview;
pub mod markers;
pub mod preflight;
pub mod report;
pub mod runner;
pub mod status;
pub mod write_executor;
pub mod write_payload;
pub mod write_preview;
pub mod write_runtime;

// Re-exports for the CLI entry point. Kept narrow on purpose so the CLI
// has a single, audited surface area into the runner.
pub use approval::{
    evaluate_approval, ApprovalContext, ApprovalDecision, ApprovalRefusal, EXIT_APPROVAL_DENIED,
    EXIT_ROLLBACK_FAILED, PREVIEW_SHA256_HEX_LEN,
};
pub use approval_ux::{
    evaluate_operator_input, render_approval_prompt, render_non_approvable_summary,
    render_preview_for_operator, ApprovalPromptRender,
};
pub use checkpoint::{
    CheckpointError, CheckpointHandle, CheckpointStore, Manifest, EXIT_CHECKPOINT_FAILED,
    MANIFEST_VERSION, MAX_CHECKPOINT_BYTES,
};
pub use diff_preview::{
    build_preview, canonical_preview_record_for_approval, contains_secret_like,
    preview_hash_from_parts, CanonicalSubset, PreviewBuildError, PreviewDisplay, PreviewInputs,
    PreviewRecord, CANONICAL_HEADER, HASH_DISPLAY_SEPARATOR, MAX_CONTENT_BYTES_FOR_DIFF,
    MAX_DIFF_BYTES, MAX_DIFF_LINES, PREVIEW_FORMAT_VERSION,
};
pub use preflight::{
    is_workspace_write_step, precheck_with_write_preview, PrecheckRefusal, READ_ONLY_TOOLS,
    WORKSPACE_WRITE_TOOL,
};
pub use report::{exit_code_for, write_json, write_markers, EXIT_PARSE_ERROR};
pub use runner::{
    parse_step_timeout_seconds, refused_precheck_report, run_plan, run_plan_with_write_preview,
    substrate_unavailable_report, PlanOutcome, PlanReport, StepFailure, StepReport,
    WritePreviewPlanStatus, WritePreviewRunReport, DEFAULT_STEP_TIMEOUT,
    EXIT_RUN_PLAN_WRITE_PREVIEW_READY, EXIT_RUN_PLAN_WRITE_PREVIEW_REFUSED, MAX_STEP_TIMEOUT_SECS,
    MIN_STEP_TIMEOUT_SECS,
};
pub use status::{
    read_status, Phase as StatusPhase, StatusEnvelope, StatusResult, StopCondition,
    EXIT_STATUS_REFUSED, READ_ONLY_INVARIANT_LITERAL, STATUS_SCHEMA_V1,
};
pub use write_executor::{
    execute_write, ApprovalRefusalCause as WriteApprovalRefusalCause, AuthorityMismatch,
    BaselineDrift, RollbackFailureCause, RollbackStage, WriteExecutionOutcome,
    WriteExecutionRequest, WriteExecutionResult, WriteStage, EXIT_APPROVAL_REFUSED,
    EXIT_BASELINE_MISMATCH, EXIT_INVALID_REQUEST, EXIT_VALIDATION_ROLLED_BACK, EXIT_WRITE_APPLIED,
    EXIT_WRITE_IO_FAILED,
};
pub use write_payload::{
    bind_after_bytes, ApprovedWritePayload, BindError, MAX_APPROVED_PAYLOAD_BYTES,
};
pub use write_preview::{
    produce_write_preview, WritePreviewArtifacts, WritePreviewRefusal, PAYLOAD_ROOT_REL,
    PREVIEW_BUNDLE_ROOT_REL, PREVIEW_BUNDLE_SCHEMA_V1, PREVIEW_GENERATOR_RESULT_SCHEMA_V1,
    RUN_MANIFEST_SCHEMA_V1, RUN_ROOT_REL, RUN_STATUS_SCHEMA_V1,
};
