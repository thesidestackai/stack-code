//! Offline L1a + L2a validator.
//!
//! Pure function over a parsed `Plan`. Never executes anything, never opens
//! a socket, never touches the broker, never touches the filesystem.
//!
//! L2a extends L1a with workspace-write *shape* validation: a step may
//! structurally declare `mode: workspace-write` if it also names a
//! `write_target` whose path passes a small lexical safety check and
//! includes the `Write` tool in `tools`. No writes are executed here.

use crate::plan_schema::{ModelTier, Plan, PlanMode, PlanStep};

/// Markers emitted by the validator. Kept as `&'static str` constants so
/// downstream tooling (CI, log scrapers) can grep for stable tokens.
pub mod markers {
    // L1a step-level markers retained from the original schema lane.
    pub const ACCEPTED_READONLY: &str = "a2-l1-accepted-readonly";
    pub const ACCEPTED_WORKSPACE_WRITE: &str = "a2-l1-accepted-workspace-write";
    pub const REFUSED_DEEP: &str = "a2-l1-refused-deep";
    pub const MISSING_TOOLS: &str = "a2-l1-missing-tools";

    // L2a workspace-write shape markers.
    pub const WRITE_MISSING_TARGET: &str = "a2-l1-write-missing-target";
    pub const WRITE_TARGET_ON_READONLY: &str = "a2-l1-write-target-on-readonly";
    pub const WRITE_TOOL_MISSING: &str = "a2-l1-write-tool-missing";
    pub const WRITE_TOOL_ON_READONLY: &str = "a2-l1-write-tool-on-readonly";
    pub const WRITE_PATH_REFUSED: &str = "a2-l1-write-path-refused";
    pub const WRITE_PATH_DENYGLOB: &str = "a2-l1-write-path-denyglob";
    pub const EXPECTED_POST_WRITE_ON_READONLY: &str = "a2-l1-expected-post-write-on-readonly";

    // Plan-level rollup markers.
    pub const REPORT_OK: &str = "a2-l1-plan-validation-pass";
    pub const REPORT_REFUSED: &str = "a2-l1-plan-validation-refused";
}

/// The `Write` tool string that gates workspace-write acceptance.
const WRITE_TOOL: &str = "Write";

/// Special directory names whose appearance anywhere in the path is denied.
/// `.git`, `.claw`, and `.claude` are treated under the deny-glob marker
/// because they are structural deny categories rather than traversal bugs.
const DENY_DIR_COMPONENTS: &[&str] = &[".git", ".claw", ".claude"];

/// Deny patterns evaluated against each path component.
/// `prefix` matches `startswith(prefix)`; `suffix` matches `endswith(suffix)`.
/// `.env` is covered by the `.env` prefix entry.
const DENY_PREFIXES: &[&str] = &[".env", "secret", "credentials"];
const DENY_SUFFIXES: &[&str] = &[".pem", ".key"];

/// Classification for the whole plan after validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanClassification {
    /// Every step accepted under L1a/L2a rules.
    Pass,
    /// At least one step was refused. The plan as a whole is not runnable.
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

/// Validate a parsed `Plan` against L1a + L2a rules.
///
/// This is pure: no I/O, no network, no broker calls, no tool execution,
/// no filesystem access.
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

    if effective_tier == ModelTier::Deep {
        step_markers.push(markers::REFUSED_DEEP.to_string());
        accepted = false;
    }
    if step.tools.is_empty() {
        step_markers.push(markers::MISSING_TOOLS.to_string());
        accepted = false;
    }

    let declares_write_tool = step.tools.iter().any(|t| t == WRITE_TOOL);

    match effective_mode {
        PlanMode::ReadOnly => {
            if declares_write_tool {
                step_markers.push(markers::WRITE_TOOL_ON_READONLY.to_string());
                accepted = false;
            }
            if step.write_target.is_some() {
                step_markers.push(markers::WRITE_TARGET_ON_READONLY.to_string());
                accepted = false;
            }
            if step.expected_post_write.is_some() {
                step_markers.push(markers::EXPECTED_POST_WRITE_ON_READONLY.to_string());
                accepted = false;
            }
        }
        PlanMode::WorkspaceWrite => {
            match &step.write_target {
                None => {
                    step_markers.push(markers::WRITE_MISSING_TARGET.to_string());
                    accepted = false;
                }
                Some(target) => {
                    if let Some(marker) = check_write_path(&target.path) {
                        step_markers.push(marker.to_string());
                        accepted = false;
                    }
                }
            }
            if !declares_write_tool {
                step_markers.push(markers::WRITE_TOOL_MISSING.to_string());
                accepted = false;
            }
        }
    }

    if accepted {
        match effective_mode {
            PlanMode::ReadOnly => {
                step_markers.push(markers::ACCEPTED_READONLY.to_string());
            }
            PlanMode::WorkspaceWrite => {
                step_markers.push(markers::ACCEPTED_WORKSPACE_WRITE.to_string());
            }
        }
    }

    StepValidationResult {
        step_id: step.id.clone(),
        accepted,
        markers: step_markers,
    }
}

/// Lexical-only write-path safety check. Returns the first marker that
/// applies, or `None` if the path is structurally acceptable.
///
/// IMPORTANT: This function does **not** touch the filesystem. It does not
/// canonicalize, it does not follow symlinks, it does not check parent
/// directories. Those checks belong to a future runner/write lane.
fn check_write_path(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some(markers::WRITE_PATH_REFUSED);
    }
    if path.starts_with('/') {
        return Some(markers::WRITE_PATH_REFUSED);
    }

    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();

    for component in &components {
        if *component == ".." {
            return Some(markers::WRITE_PATH_REFUSED);
        }
    }

    for component in &components {
        if DENY_DIR_COMPONENTS.contains(component) {
            return Some(markers::WRITE_PATH_DENYGLOB);
        }
        if matches_deny_pattern(component) {
            return Some(markers::WRITE_PATH_DENYGLOB);
        }
    }

    None
}

fn matches_deny_pattern(component: &str) -> bool {
    for prefix in DENY_PREFIXES {
        if component.starts_with(prefix) {
            return true;
        }
    }
    for suffix in DENY_SUFFIXES {
        if component.ends_with(suffix) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_schema::{parse_plan, WriteTarget};

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
    fn workspace_write_without_target_is_refused() {
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
        assert!(step
            .markers
            .contains(&markers::WRITE_MISSING_TARGET.to_string()));
    }

    #[test]
    fn top_level_workspace_write_refuses_when_step_misses_target() {
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
        let step = &report.step_results[0];
        assert!(!step.accepted);
        assert!(step
            .markers
            .contains(&markers::WRITE_MISSING_TARGET.to_string()));
        assert!(step
            .markers
            .contains(&markers::WRITE_TOOL_MISSING.to_string()));
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
    fn validator_emits_combined_refusal_markers() {
        let plan = parse(
            r"
name: combined
mode: read-only
model_tier: FAST
steps:
  - id: triple
    description: every violation at once
    mode: workspace-write
    model_tier: DEEP
    tools: []
",
        );
        let report = validate_plan(&plan);
        let s = &report.step_results[0];
        assert!(!s.accepted);
        assert!(s.markers.contains(&markers::REFUSED_DEEP.to_string()));
        assert!(s.markers.contains(&markers::MISSING_TOOLS.to_string()));
        assert!(s
            .markers
            .contains(&markers::WRITE_MISSING_TARGET.to_string()));
        assert!(s.markers.contains(&markers::WRITE_TOOL_MISSING.to_string()));
        assert!(!s.markers.contains(&markers::ACCEPTED_READONLY.to_string()));
        assert!(!s
            .markers
            .contains(&markers::ACCEPTED_WORKSPACE_WRITE.to_string()));
    }

    /// Compile-time + behavioral proof that the validator never invokes
    /// network/broker/tool/filesystem code: the function signature takes
    /// only `&Plan` and returns a plain value with no side effects.
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
        let r1 = validate_plan(&plan);
        let r2 = validate_plan(&plan);
        assert_eq!(r1, r2);
    }

    // --- Example YAML coverage --------------------------------------------------
    //
    // The L1a corpus under `examples/` is included at compile time so renaming
    // or deleting it breaks the build — the desired regression signal.

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
        // The L1a example declares mode: workspace-write but lacks
        // write_target and lacks the Write tool, so under L2a it is still
        // refused (classification unchanged).
        let plan = parse(EX_REFUSED_WRITE);
        let report = validate_plan(&plan);
        assert_eq!(report.classification, PlanClassification::Refused);
        let any_target_marker = report.step_results.iter().any(|s| {
            s.markers
                .contains(&markers::WRITE_MISSING_TARGET.to_string())
        });
        assert!(
            any_target_marker,
            "expected WRITE_MISSING_TARGET marker on a step"
        );
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

    // --- L2a path safety unit coverage ------------------------------------------

    fn write_target(path: &str) -> WriteTarget {
        WriteTarget {
            path: path.to_string(),
            create_if_absent: false,
        }
    }

    #[test]
    fn path_safety_rejects_absolute_with_path_refused() {
        assert_eq!(
            check_write_path("/etc/passwd"),
            Some(markers::WRITE_PATH_REFUSED)
        );
    }

    #[test]
    fn path_safety_rejects_traversal_with_path_refused() {
        assert_eq!(
            check_write_path("../escape.txt"),
            Some(markers::WRITE_PATH_REFUSED)
        );
        assert_eq!(
            check_write_path("a/../b"),
            Some(markers::WRITE_PATH_REFUSED)
        );
    }

    #[test]
    fn path_safety_rejects_git_with_denyglob() {
        assert_eq!(
            check_write_path(".git/config"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path(".claw/state"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path(".claude/settings.json"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
    }

    #[test]
    fn path_safety_rejects_env_secret_creds_with_denyglob() {
        assert_eq!(check_write_path(".env"), Some(markers::WRITE_PATH_DENYGLOB));
        assert_eq!(
            check_write_path(".env.local"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path("secrets.yaml"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path("credentials.json"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path("tls/server.pem"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
        assert_eq!(
            check_write_path("tls/server.key"),
            Some(markers::WRITE_PATH_DENYGLOB)
        );
    }

    #[test]
    fn path_safety_accepts_relative_safe_paths() {
        assert_eq!(check_write_path("notes/scratch.md"), None);
        assert_eq!(check_write_path("README.md"), None);
        assert_eq!(check_write_path("src/foo/bar.rs"), None);
    }

    #[test]
    fn read_only_with_write_target_uses_target_on_readonly_marker() {
        let plan = Plan {
            name: "ro-with-target".into(),
            mode: PlanMode::ReadOnly,
            model_tier: ModelTier::Fast,
            steps: vec![PlanStep {
                id: "s1".into(),
                description: "d".into(),
                mode: None,
                model_tier: None,
                tools: vec!["Read".into()],
                expected_output: None,
                write_target: Some(write_target("notes/scratch.md")),
                expected_post_write: None,
            }],
        };
        let report = validate_plan(&plan);
        let step = &report.step_results[0];
        assert!(!step.accepted);
        assert!(step
            .markers
            .contains(&markers::WRITE_TARGET_ON_READONLY.to_string()));
    }
}
