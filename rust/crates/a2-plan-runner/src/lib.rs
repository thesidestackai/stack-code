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

pub mod markers;
pub mod preflight;
pub mod report;
pub mod runner;

// Re-exports for the CLI entry point. Kept narrow on purpose so the CLI
// has a single, audited surface area into the runner.
pub use preflight::{PrecheckRefusal, READ_ONLY_TOOLS};
pub use report::{exit_code_for, write_json, write_markers, EXIT_PARSE_ERROR};
pub use runner::{
    parse_step_timeout_seconds, refused_precheck_report, run_plan, substrate_unavailable_report,
    PlanOutcome, PlanReport, StepFailure, StepReport, DEFAULT_STEP_TIMEOUT, MAX_STEP_TIMEOUT_SECS,
    MIN_STEP_TIMEOUT_SECS,
};
