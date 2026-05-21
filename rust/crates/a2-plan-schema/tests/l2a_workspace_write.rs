//! Integration tests for L2a workspace-write schema acceptance.
//!
//! These tests exercise the public crate API only: parsing YAML into a `Plan`
//! and running `validate_plan` on it. No filesystem I/O, no broker, no model
//! calls. The marker literal strings are pinned here as a contract guard for
//! downstream tooling that greps for them.

use a2_plan_schema::{parse_plan, validate_plan, PlanClassification};

// --- Marker literal pins ----------------------------------------------------
//
// These string constants intentionally duplicate the values in
// `plan_validate::markers`. They exist so a rename of the in-crate constant
// without an explicit external contract change immediately breaks this test
// file.

const M_ACCEPTED_WORKSPACE_WRITE: &str = "a2-l1-accepted-workspace-write";
const M_WRITE_MISSING_TARGET: &str = "a2-l1-write-missing-target";
const M_WRITE_TARGET_ON_READONLY: &str = "a2-l1-write-target-on-readonly";
const M_WRITE_TOOL_MISSING: &str = "a2-l1-write-tool-missing";
const M_WRITE_TOOL_ON_READONLY: &str = "a2-l1-write-tool-on-readonly";
const M_WRITE_PATH_REFUSED: &str = "a2-l1-write-path-refused";
const M_WRITE_PATH_DENYGLOB: &str = "a2-l1-write-path-denyglob";
const M_EXPECTED_POST_WRITE_ON_READONLY: &str = "a2-l1-expected-post-write-on-readonly";
const M_REPORT_OK: &str = "a2-l1-plan-validation-pass";
const M_REPORT_REFUSED: &str = "a2-l1-plan-validation-refused";

fn parse_and_validate(yaml: &str) -> a2_plan_schema::PlanValidationReport {
    let plan = parse_plan(yaml).expect("test yaml must parse");
    validate_plan(&plan)
}

fn step_has_marker(
    report: &a2_plan_schema::PlanValidationReport,
    step_id: &str,
    marker: &str,
) -> bool {
    report
        .step_results
        .iter()
        .find(|s| s.step_id == step_id)
        .is_some_and(|s| s.markers.iter().any(|m| m == marker))
}

// --- Happy path -------------------------------------------------------------

#[test]
fn minimal_workspace_write_step_with_write_tool_and_safe_target_is_accepted() {
    let yaml = r"
name: ok-write
mode: read-only
model_tier: FAST
steps:
  - id: write-step
    description: write notes
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/scratch.md
      create_if_absent: true
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Pass);
    assert!(report.markers.iter().any(|m| m == M_REPORT_OK));
    assert!(step_has_marker(
        &report,
        "write-step",
        M_ACCEPTED_WORKSPACE_WRITE
    ));
}

// --- Workspace-write structural refusals ------------------------------------

#[test]
fn workspace_write_without_write_target_emits_missing_target_marker() {
    let yaml = r"
name: bad-no-target
mode: read-only
steps:
  - id: write-step
    description: writes
    mode: workspace-write
    tools: [Write]
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(report.markers.iter().any(|m| m == M_REPORT_REFUSED));
    assert!(step_has_marker(
        &report,
        "write-step",
        M_WRITE_MISSING_TARGET
    ));
}

#[test]
fn workspace_write_without_write_tool_emits_tool_missing_marker() {
    let yaml = r"
name: bad-no-tool
mode: read-only
steps:
  - id: write-step
    description: writes without Write
    mode: workspace-write
    tools: [Read]
    write_target:
      path: notes/scratch.md
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(&report, "write-step", M_WRITE_TOOL_MISSING));
}

// --- Read-only-with-write-shape refusals ------------------------------------

#[test]
fn readonly_step_declaring_write_tool_emits_tool_on_readonly_marker() {
    let yaml = r"
name: bad-readonly-write
mode: read-only
steps:
  - id: ro-step
    description: read-only but declares Write
    tools: [Read, Write]
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "ro-step",
        M_WRITE_TOOL_ON_READONLY
    ));
}

#[test]
fn readonly_step_declaring_write_target_emits_target_on_readonly_marker() {
    let yaml = r"
name: bad-readonly-target
mode: read-only
steps:
  - id: ro-step
    description: read-only but declares a write_target
    tools: [Read]
    write_target:
      path: notes/scratch.md
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "ro-step",
        M_WRITE_TARGET_ON_READONLY
    ));
}

#[test]
fn readonly_step_declaring_expected_post_write_emits_dedicated_marker() {
    let yaml = r"
name: bad-readonly-post-write
mode: read-only
steps:
  - id: ro-step
    description: read-only but declares expected_post_write
    tools: [Read]
    expected_post_write:
      must_contain: [hello]
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "ro-step",
        M_EXPECTED_POST_WRITE_ON_READONLY
    ));
}

// --- Lexical path refusal: parent traversal and absolute paths ---------------

#[test]
fn write_target_with_parent_traversal_emits_path_refused_marker() {
    let yaml = r"
name: bad-escape
mode: read-only
steps:
  - id: write-step
    description: escapes the workspace
    mode: workspace-write
    tools: [Write]
    write_target:
      path: ../escape.txt
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(&report, "write-step", M_WRITE_PATH_REFUSED));
}

#[test]
fn write_target_with_absolute_path_emits_path_refused_marker() {
    let yaml = r"
name: bad-absolute
mode: read-only
steps:
  - id: write-step
    description: absolute path
    mode: workspace-write
    tools: [Write]
    write_target:
      path: /etc/passwd
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(&report, "write-step", M_WRITE_PATH_REFUSED));
}

// --- Lexical path refusal: deny-glob set -------------------------------------

#[test]
fn write_target_inside_dot_git_emits_denyglob_marker() {
    let yaml = r"
name: bad-git
mode: read-only
steps:
  - id: write-step
    description: writes inside .git
    mode: workspace-write
    tools: [Write]
    write_target:
      path: .git/config
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "write-step",
        M_WRITE_PATH_DENYGLOB
    ));
}

#[test]
fn write_target_named_dot_env_emits_denyglob_marker() {
    let yaml = r"
name: bad-env
mode: read-only
steps:
  - id: write-step
    description: writes .env
    mode: workspace-write
    tools: [Write]
    write_target:
      path: .env
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "write-step",
        M_WRITE_PATH_DENYGLOB
    ));
}

#[test]
fn write_target_credentials_emits_denyglob_marker() {
    let yaml = r"
name: bad-creds
mode: read-only
steps:
  - id: write-step
    description: writes credentials.json
    mode: workspace-write
    tools: [Write]
    write_target:
      path: credentials.json
";
    let report = parse_and_validate(yaml);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(step_has_marker(
        &report,
        "write-step",
        M_WRITE_PATH_DENYGLOB
    ));
}

// --- Canonical L2a example YAMLs --------------------------------------------
//
// The four L2a example files are include_str!'d so renaming or deleting any
// of them breaks the build by design.

const EX_VALID: &str = include_str!("../../../../examples/a2_l2a_valid_workspace_write_plan.yaml");
const EX_MISSING_TARGET: &str =
    include_str!("../../../../examples/a2_l2a_refused_write_missing_target.yaml");
const EX_PATH_ESCAPE: &str =
    include_str!("../../../../examples/a2_l2a_refused_write_path_escape.yaml");
const EX_DENYGLOB: &str = include_str!("../../../../examples/a2_l2a_refused_write_denyglob.yaml");

#[test]
fn example_valid_workspace_write_plan_passes() {
    let plan = parse_plan(EX_VALID).expect("example must parse");
    let report = validate_plan(&plan);
    assert_eq!(report.classification, PlanClassification::Pass);
    assert!(report
        .step_results
        .iter()
        .any(|s| s.markers.iter().any(|m| m == M_ACCEPTED_WORKSPACE_WRITE)));
}

#[test]
fn example_refused_write_missing_target_emits_missing_target_marker() {
    let plan = parse_plan(EX_MISSING_TARGET).expect("example must parse");
    let report = validate_plan(&plan);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(report
        .step_results
        .iter()
        .any(|s| s.markers.iter().any(|m| m == M_WRITE_MISSING_TARGET)));
}

#[test]
fn example_refused_write_path_escape_emits_path_refused_marker() {
    let plan = parse_plan(EX_PATH_ESCAPE).expect("example must parse");
    let report = validate_plan(&plan);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(report
        .step_results
        .iter()
        .any(|s| s.markers.iter().any(|m| m == M_WRITE_PATH_REFUSED)));
}

#[test]
fn example_refused_write_denyglob_emits_denyglob_marker() {
    let plan = parse_plan(EX_DENYGLOB).expect("example must parse");
    let report = validate_plan(&plan);
    assert_eq!(report.classification, PlanClassification::Refused);
    assert!(report
        .step_results
        .iter()
        .any(|s| s.markers.iter().any(|m| m == M_WRITE_PATH_DENYGLOB)));
}
