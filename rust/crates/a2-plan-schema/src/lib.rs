//! A2 plan schema and offline validator (L1a + L2a).
//!
//! This crate is intentionally offline: it never opens network connections,
//! invokes the broker, calls Ollama, or executes tools. It exists to enforce
//! the schema safety contract on static plan YAML before any future runner
//! is wired up.
//!
//! L1a/L2a acceptance rules (offline, lexical only):
//! - `mode: read-only` is accepted (marker `a2-l1-accepted-readonly`).
//! - `mode: workspace-write` is accepted structurally when the step declares
//!   the `Write` tool, a well-formed `write_target`, AND a well-formed
//!   `after_file` (markers `a2-l1-accepted-workspace-write` and
//!   `a2-l2a-after-file-shape-accepted`).
//! - `model_tier: FAST` is accepted.
//! - `model_tier: DEEP` is refused with marker `a2-l1-refused-deep`.
//! - A step with no declared tools is refused with marker `a2-l1-missing-tools`.
//! - Workspace-write structural refusals (`a2-l1-write-missing-target`,
//!   `a2-l1-write-tool-missing`, `a2-l1-write-path-refused`,
//!   `a2-l1-write-path-denyglob`) and read-only-with-write-shape refusals
//!   (`a2-l1-write-tool-on-readonly`, `a2-l1-write-target-on-readonly`,
//!   `a2-l1-expected-post-write-on-readonly`) are emitted by the L2a layer.
//! - L2a `after_file` refusals (`a2-l2a-after-file-missing`,
//!   `a2-l2a-after-file-on-readonly`, `a2-l2a-after-file-path-refused`,
//!   `a2-l2a-after-file-path-denyglob`) gate the explicit exact-after-bytes
//!   source.
//!
//! No runner, no filesystem writes, no canonicalization, no symlink checks.
//! The `after_file` field is the workspace-root-relative path of the exact
//! after-bytes source; the validator never opens it. Runtime file checks
//! (existence, regular-file-ness, symlink rejection, size cap, byte read)
//! belong to the future runner/materializer lane.

pub mod plan_schema;
pub mod plan_validate;

pub use plan_schema::{
    parse_plan, ExpectedOutputContract, ExpectedPostWriteContract, ModelTier, ParseError, Plan,
    PlanMode, PlanStep, WriteTarget,
};
pub use plan_validate::{
    validate_plan, PlanClassification, PlanValidationReport, StepValidationResult,
};
