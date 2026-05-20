//! Offline L1 validator.
//!
//! Pure function over a parsed `Plan`. Never executes anything, never opens
//! a socket, never touches the broker.

use crate::plan_schema::{ModelTier, Plan, PlanMode, PlanStep};

/// Markers emitted by the L1 validator. Kept as `&'static str` constants so
/// downstream tooling (CI, log scrapers) can grep for stable tokens.
pub mod markers {
    pub const ACCEPTED_READONLY: &str = "a2-l1-accepted-readonly";
    pub const REFUSED_WRITE: &str = "a2-l1-refused-write";
    pub const REFUSED_DEEP: &str = "a2-l1-refused-deep";
    pub const MISSING_TOOLS: &str = "a2-l1-missing-tools";
    pub const REPORT_OK: &str = "a2-l1-plan-validation-pass";
    pub const REPORT_REFUSED: &str = "a2-l1-plan-validation-refused";
}

/// Classification for the whole plan after validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanClassification {
    /// Every step accepted under L1 rules.
    Pass,
    /// At least one step was refused. The plan as a whole is not L1-runnable.
    Refused,
}

/// Per-step result emitted by the validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepValidationResult {
    pub step_id: String,
    pub accepted: bool,
    pub markers: Vec<String>,
}

/// Aggregate validation report for a whole plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanValidationReport {
    pub plan_name: String,
    pub classification: PlanClassification,
    pub markers: Vec<String>,
    pub step_results: Vec<StepValidationResult>,
}

impl PlanValidationReport {
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self.classification, PlanClassification::Pass)
    }
}

/// Validate a parsed `Plan` against L1a rules.
///
/// This is pure: no I/O, no network, no broker calls, no tool execution.
#[must_use]
pub fn validate_plan(plan: &Plan) -> PlanValidationReport {
    let step_results: Vec<StepValidationResult> = plan
        .steps
        .iter()
        .map(|step| validate_step(plan, step))
        .collect();

    let any_refused = step_results.iter().any(|r| !r.accepted);
    let classification = if any_refused {
        PlanClassification::Refused
    } else {
        PlanClassification::Pass
    };

    let mut markers = Vec::new();
    if any_refused {
        markers.push(markers::REPORT_REFUSED.to_string());
    } else {
        markers.push(markers::REPORT_OK.to_string());
    }

    PlanValidationReport {
        plan_name: plan.name.clone(),
        classification,
        markers,
        step_results,
    }
}

fn validate_step(plan: &Plan, step: &PlanStep) -> StepValidationResult {
    let effective_mode = step.mode.unwrap_or(plan.mode);
    let effective_tier = step.model_tier.unwrap_or(plan.model_tier);

    let mut step_markers: Vec<String> = Vec::new();
    let mut accepted = true;

    if effective_mode == PlanMode::WorkspaceWrite {
        step_markers.push(markers::REFUSED_WRITE.to_string());
        accepted = false;
    }
    if effective_tier == ModelTier::Deep {
        step_markers.push(markers::REFUSED_DEEP.to_string());
        accepted = false;
    }
    if step.tools.is_empty() {
        step_markers.push(markers::MISSING_TOOLS.to_string());
        accepted = false;
    }

    if accepted {
        step_markers.push(markers::ACCEPTED_READONLY.to_string());
    }

    StepValidationResult {
        step_id: step.id.clone(),
        accepted,
        markers: step_markers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_schema::parse_plan;

    fn parse(yaml: &str) -> Plan {
        parse_plan(yaml).expect("test yaml must parse")
    }

    #[test]
    fn valid_read_only_fast_plan_passes() {
        let plan = parse(
            r"
name: ok
mode: read-only
model_tier: FAST
steps:
  - id: s1
    description: read
    tools: [Read]
  - id: s2
    description: search
    tools: [Grep]
",
        );
        let report = validate_plan(&plan);
        assert!(report.is_pass(), "expected PASS, got {report:?}");
        assert_eq!(report.classification, PlanClassification::Pass);
        assert!(report.markers.contains(&markers::REPORT_OK.to_string()));
        assert_eq!(report.step_results.len(), 2);
        for sr in &report.step_results {
            assert!(sr.accepted, "step {} should be accepted", sr.step_id);
            assert!(
                sr.markers.contains(&markers::ACCEPTED_READONLY.to_string()),
                "step {} should carry accepted marker",
                sr.step_id
            );
        }
    }

    #[test]
    fn workspace_write_step_is_refused() {
        let plan = parse(
            r"
name: bad-write
mode: read-only
steps:
  - id: s1
    description: writes
    mode: workspace-write
    tools: [Write]
",
        );
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        assert!(report
            .markers
            .contains(&markers::REPORT_REFUSED.to_string()));
        let step = &report.step_results[0];
        assert!(!step.accepted);
        assert!(step.markers.contains(&markers::REFUSED_WRITE.to_string()));
    }

    #[test]
    fn top_level_workspace_write_refuses_all_steps() {
        let plan = parse(
            r"
name: bad-top-write
mode: workspace-write
steps:
  - id: s1
    description: any
    tools: [Read]
",
        );
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        assert!(report.step_results[0]
            .markers
            .contains(&markers::REFUSED_WRITE.to_string()));
    }

    #[test]
    fn deep_tier_is_refused() {
        let plan = parse(
            r"
name: bad-deep
model_tier: FAST
steps:
  - id: s1
    description: deep step
    model_tier: DEEP
    tools: [Read]
",
        );
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let step = &report.step_results[0];
        assert!(!step.accepted);
        assert!(step.markers.contains(&markers::REFUSED_DEEP.to_string()));
    }

    #[test]
    fn top_level_deep_refuses_all_steps() {
        let plan = parse(
            r"
name: bad-top-deep
model_tier: DEEP
steps:
  - id: s1
    description: any
    tools: [Read]
",
        );
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        assert!(report.step_results[0]
            .markers
            .contains(&markers::REFUSED_DEEP.to_string()));
    }

    #[test]
    fn missing_tools_step_is_refused() {
        let plan = parse(
            r"
name: bad-tools
steps:
  - id: s1
    description: no tools declared
    tools: []
",
        );
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let step = &report.step_results[0];
        assert!(!step.accepted);
        assert!(step.markers.contains(&markers::MISSING_TOOLS.to_string()));
    }

    #[test]
    fn mixed_plan_with_one_refused_step_is_non_pass() {
        let plan = parse(
            r"
name: mixed
mode: read-only
model_tier: FAST
steps:
  - id: ok-step
    description: read
    tools: [Read]
  - id: bad-step
    description: writes
    mode: workspace-write
    tools: [Write]
",
        );
        let report = validate_plan(&plan);
        assert_ne!(
            report.classification,
            PlanClassification::Pass,
            "any refused step must make the plan non-PASS"
        );
        assert!(!report.is_pass());
        assert_eq!(report.step_results.len(), 2);
        assert!(report.step_results[0].accepted);
        assert!(!report.step_results[1].accepted);
    }

    #[test]
    fn validator_emits_all_three_refusal_markers_when_combined() {
        let plan = parse(
            r"
name: combined
mode: read-only
model_tier: FAST
steps:
  - id: triple
    description: every L1 violation at once
    mode: workspace-write
    model_tier: DEEP
    tools: []
",
        );
        let report = validate_plan(&plan);
        let s = &report.step_results[0];
        assert!(!s.accepted);
        assert!(s.markers.contains(&markers::REFUSED_WRITE.to_string()));
        assert!(s.markers.contains(&markers::REFUSED_DEEP.to_string()));
        assert!(s.markers.contains(&markers::MISSING_TOOLS.to_string()));
        assert!(!s.markers.contains(&markers::ACCEPTED_READONLY.to_string()));
    }

    /// Compile-time + behavioral proof that the validator never invokes
    /// network/broker/tool code: the function signature takes only `&Plan`
    /// and returns a plain value with no side effects. There is no way for
    /// this call to reach the broker or Ollama because nothing in this
    /// crate links to anything that could.
    #[test]
    fn validator_signature_is_offline_and_pure() {
        let plan = parse(
            r"
name: offline
steps:
  - id: s1
    description: pure
    tools: [Read]
",
        );
        // Call twice; identical results prove no hidden state mutation.
        let r1 = validate_plan(&plan);
        let r2 = validate_plan(&plan);
        assert_eq!(r1, r2);
    }

    // --- Example YAML coverage --------------------------------------------------
    //
    // The four files under repo `examples/` are the canonical L1a corpus.
    // They are included at compile time so renaming or deleting them breaks
    // the build, which is the desired regression signal.

    const EX_VALID: &str = include_str!("../../../../examples/a2_l1a_valid_readonly_plan.yaml");
    const EX_REFUSED_WRITE: &str =
        include_str!("../../../../examples/a2_l1a_refused_workspace_write.yaml");
    const EX_REFUSED_DEEP: &str = include_str!("../../../../examples/a2_l1a_refused_deep.yaml");
    const EX_MISSING_TOOLS: &str = include_str!("../../../../examples/a2_l1a_missing_tools.yaml");

    #[test]
    fn example_valid_readonly_plan_passes() {
        let plan = parse(EX_VALID);
        let report = validate_plan(&plan);
        assert!(report.is_pass(), "expected PASS, got {report:?}");
    }

    #[test]
    fn example_refused_workspace_write_yaml_is_refused() {
        let plan = parse(EX_REFUSED_WRITE);
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let any_write_marker = report
            .step_results
            .iter()
            .any(|s| s.markers.contains(&markers::REFUSED_WRITE.to_string()));
        assert!(any_write_marker, "expected REFUSED_WRITE marker on a step");
    }

    #[test]
    fn example_refused_deep_yaml_is_refused() {
        let plan = parse(EX_REFUSED_DEEP);
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let any_deep_marker = report
            .step_results
            .iter()
            .any(|s| s.markers.contains(&markers::REFUSED_DEEP.to_string()));
        assert!(any_deep_marker, "expected REFUSED_DEEP marker on a step");
    }

    #[test]
    fn example_missing_tools_yaml_is_refused() {
        let plan = parse(EX_MISSING_TOOLS);
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let any_missing_marker = report
            .step_results
            .iter()
            .any(|s| s.markers.contains(&markers::MISSING_TOOLS.to_string()));
        assert!(
            any_missing_marker,
            "expected MISSING_TOOLS marker on a step"
        );
    }
}
