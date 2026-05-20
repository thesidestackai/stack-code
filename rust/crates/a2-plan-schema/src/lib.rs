//! A2 read-only plan schema and offline validator.
//!
//! This crate is intentionally offline: it never opens network connections,
//! invokes the broker, calls Ollama, or executes tools. It exists to enforce
//! the L1 safety contract on static plan YAML before any future runner is
//! wired up.
//!
//! L1 acceptance rules:
//! - `mode: read-only` is accepted.
//! - `mode: workspace-write` is refused with marker `a2-l1-refused-write`.
//! - `model_tier: FAST` is accepted.
//! - `model_tier: DEEP` is refused with marker `a2-l1-refused-deep`.
//! - A step with no declared tools is refused with marker `a2-l1-missing-tools`.

pub mod plan_schema;
pub mod plan_validate;

pub use plan_schema::{
    parse_plan, ExpectedOutputContract, ModelTier, ParseError, Plan, PlanMode, PlanStep,
};
pub use plan_validate::{
    validate_plan, PlanClassification, PlanValidationReport, StepValidationResult,
};
