//! Static plan schema for A2 L1a.
//!
//! Types are deserialize-only. There is no executor, runner, or I/O here.

use serde::Deserialize;

/// Execution mode for a step. L1 only accepts `ReadOnly`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanMode {
    ReadOnly,
    WorkspaceWrite,
}

/// Model tier for a step. L1 only accepts `Fast`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ModelTier {
    Fast,
    Deep,
}

/// Optional declared output contract. Advisory only at L1a.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedOutputContract {
    #[serde(default)]
    pub must_contain: Vec<String>,
}

/// A single step inside a plan.
///
/// `tools` MUST be declared explicitly. An empty or missing list is a
/// validation refusal, not a parse error, so the validator can attach
/// the `a2-l1-missing-tools` marker to a specific step.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub mode: Option<PlanMode>,
    #[serde(default)]
    pub model_tier: Option<ModelTier>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub expected_output: Option<ExpectedOutputContract>,
}

/// A static plan document.
///
/// Top-level `mode` and `model_tier` set defaults for steps that omit them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub name: String,
    #[serde(default = "default_mode")]
    pub mode: PlanMode,
    #[serde(default = "default_tier")]
    pub model_tier: ModelTier,
    pub steps: Vec<PlanStep>,
}

const fn default_mode() -> PlanMode {
    PlanMode::ReadOnly
}

const fn default_tier() -> ModelTier {
    ModelTier::Fast
}

/// Parse error returned when YAML fails to deserialize into a `Plan`.
///
/// We wrap `serde_yaml::Error` rather than re-exporting it so downstream
/// callers do not take a direct dependency on the YAML implementation.
#[derive(Debug)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a2 plan parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse a YAML document into a `Plan`. Returns a structured parse error
/// for unknown enum variants, unknown fields, or shape mismatches.
///
/// This function performs no I/O and no network calls.
pub fn parse_plan(yaml: &str) -> Result<Plan, ParseError> {
    serde_yaml::from_str::<Plan>(yaml).map_err(|err| ParseError {
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_read_only_fast_plan() {
        let yaml = r"
name: minimal
mode: read-only
model_tier: FAST
steps:
  - id: s1
    description: read a file
    tools: [Read]
";
        let plan = parse_plan(yaml).expect("valid yaml should parse");
        assert_eq!(plan.name, "minimal");
        assert_eq!(plan.mode, PlanMode::ReadOnly);
        assert_eq!(plan.model_tier, ModelTier::Fast);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].id, "s1");
        assert_eq!(plan.steps[0].tools, vec!["Read".to_string()]);
    }

    #[test]
    fn defaults_top_level_to_read_only_fast() {
        let yaml = r"
name: defaulted
steps:
  - id: s1
    description: d
    tools: [Read]
";
        let plan = parse_plan(yaml).expect("should parse with defaults");
        assert_eq!(plan.mode, PlanMode::ReadOnly);
        assert_eq!(plan.model_tier, ModelTier::Fast);
    }

    #[test]
    fn rejects_unknown_mode() {
        let yaml = r"
name: bad
mode: yolo
steps: []
";
        let err = parse_plan(yaml).expect_err("unknown mode must fail to parse");
        assert!(
            err.message().contains("yolo") || err.message().contains("mode"),
            "parse error should mention the bad field: {err}"
        );
    }

    #[test]
    fn rejects_unknown_tier() {
        let yaml = r"
name: bad
model_tier: TURBO
steps: []
";
        let err = parse_plan(yaml).expect_err("unknown tier must fail to parse");
        assert!(
            err.message().contains("TURBO") || err.message().contains("tier"),
            "parse error should mention the bad field: {err}"
        );
    }

    #[test]
    fn rejects_unknown_top_level_fields() {
        let yaml = r"
name: bad
extra_field: nope
steps: []
";
        let err = parse_plan(yaml).expect_err("unknown top-level field must fail");
        assert!(
            err.message().contains("extra_field") || err.message().contains("unknown"),
            "parse error should mention the unknown field: {err}"
        );
    }

    #[test]
    fn missing_tools_field_defaults_to_empty_list() {
        // Missing tools is NOT a parse error so the validator can attach
        // a step-scoped marker. Validation enforces non-emptiness.
        let yaml = r"
name: x
steps:
  - id: s1
    description: d
";
        let plan = parse_plan(yaml).expect("missing tools should still parse");
        assert!(plan.steps[0].tools.is_empty());
    }
}
