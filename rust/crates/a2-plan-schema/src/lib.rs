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
//!   the `Write` tool and a well-formed `write_target` (marker
//!   `a2-l1-accepted-workspace-write`).
//! - `model_tier: FAST` is accepted.
//! - `model_tier: DEEP` is refused with marker `a2-l1-refused-deep`.
//! - A step with no declared tools is refused with marker `a2-l1-missing-tools`.
//! - Workspace-write structural refusals (`a2-l1-write-missing-target`,
//!   `a2-l1-write-tool-missing`, `a2-l1-write-path-refused`,
//!   `a2-l1-write-path-denyglob`) and read-only-with-write-shape refusals
//!   (`a2-l1-write-tool-on-readonly`, `a2-l1-write-target-on-readonly`,
//!   `a2-l1-expected-post-write-on-readonly`) are emitted by the L2a layer.
//!
//! No runner, no filesystem writes, no canonicalization, no symlink checks.

pub mod plan_schema;
pub mod plan_validate;

pub use plan_schema::{
    parse_plan, ExpectedOutputContract, ExpectedPostWriteContract, ModelTier, ParseError, Plan,
    PlanMode, PlanStep, WriteTarget,
};
pub use plan_validate::{
    validate_plan, PlanClassification, PlanValidationReport, StepValidationResult,
};
