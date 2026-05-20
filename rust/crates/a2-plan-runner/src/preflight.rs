//! Pre-execution refusal checks.
//!
//! Pure functions over a parsed [`a2_plan_schema::Plan`] and its
//! [`a2_plan_schema::PlanValidationReport`]. No I/O, no broker, no subprocess.
//!
//! Two refusal cases, applied in order:
//!
//! 1. **`ValidatorRefused`** — the L1a schema validator marked the plan as
//!    Refused (workspace-write, DEEP, or missing tools). Surfaces as runner
//!    marker [`crate::markers::PLAN_REFUSED_PRECHECK`] and exit code 2.
//! 2. **`ToolDisallowed`** — the plan is L1a-valid but at least one step
//!    declares a tool outside [`READ_ONLY_TOOLS`]. Surfaces as runner
//!    marker [`crate::markers::TOOL_DISALLOWED`] and exit code 3. This is
//!    the runner's own trust boundary: it MUST NOT defer to claw or the
//!    wrapper.
//!
//! The allowlist is a `const` — never read from CLI flags, env vars, config
//! files, or the plan itself.

use a2_plan_schema::{Plan, PlanValidationReport};

/// Static read-only tool allowlist. `const` so it cannot be configured at
/// runtime, on the CLI, in env vars, or in the plan itself.
pub const READ_ONLY_TOOLS: &[&str] = &["Read", "Grep", "Glob", "LS"];

/// Why the precheck refused to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecheckRefusal {
    /// The schema validator marked the plan as Refused — workspace-write,
    /// DEEP, or missing tools. Surfaces as runner marker
    /// [`crate::markers::PLAN_REFUSED_PRECHECK`] and exit code 2.
    ValidatorRefused,
    /// At least one step declared a tool outside [`READ_ONLY_TOOLS`].
    /// Surfaces as runner marker [`crate::markers::TOOL_DISALLOWED`] and
    /// exit code 3.
    ToolDisallowed { step_id: String, tool: String },
}

/// Pre-execution refusal check.
///
/// Two-layer refusal, applied in this order:
///   1. If `validator_report` is not Pass, refuse with
///      [`PrecheckRefusal::ValidatorRefused`].
///   2. Walk every step's declared `tools`; the first tool outside
///      [`READ_ONLY_TOOLS`] refuses with
///      [`PrecheckRefusal::ToolDisallowed`] carrying the offending pair.
///
/// Pure: no I/O, no broker, no subprocess. This function MUST return before
/// any execution path can run.
pub fn precheck(
    plan: &Plan,
    validator_report: &PlanValidationReport,
) -> Result<(), PrecheckRefusal> {
    if !validator_report.is_pass() {
        return Err(PrecheckRefusal::ValidatorRefused);
    }
    for step in &plan.steps {
        for tool in &step.tools {
            if !READ_ONLY_TOOLS.contains(&tool.as_str()) {
                return Err(PrecheckRefusal::ToolDisallowed {
                    step_id: step.id.clone(),
                    tool: tool.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_plan_schema::{parse_plan, validate_plan};

    // --- Inline disallowed-tool fixture --------------------------------------
    //
    // The saved Phase 2 prompt asks for `examples/a2_l1b_disallowed_tool.yaml`,
    // but the operator's Phase 2 STOP gate forbids changes outside
    // `rust/crates/a2-plan-runner/**`. The fixture is inlined here as a raw
    // string instead. When Phase 5 lands the other L1b example YAMLs under
    // `examples/`, this fixture can be migrated there for symmetry; the test
    // intent (an L1a-valid plan that L1b refuses on tool allowlist) is
    // preserved.
    const DISALLOWED_TOOL_YAML: &str = r"
name: edit-attempt-via-tools
mode: read-only
model_tier: FAST
steps:
  - id: edit-step
    description: Tries to use Edit through L1b
    tools: [Edit]
";

    // --- L1a canonical corpus, include_str!'d from repo `examples/` ----------
    //
    // Same 4-level traversal as the schema crate. Renaming or deleting any
    // of these breaks the build by design.
    const L1A_VALID: &str = include_str!("../../../../examples/a2_l1a_valid_readonly_plan.yaml");
    const L1A_REFUSED_WRITE: &str =
        include_str!("../../../../examples/a2_l1a_refused_workspace_write.yaml");
    const L1A_REFUSED_DEEP: &str = include_str!("../../../../examples/a2_l1a_refused_deep.yaml");
    const L1A_MISSING_TOOLS: &str = include_str!("../../../../examples/a2_l1a_missing_tools.yaml");

    fn run(yaml: &str) -> Result<(), PrecheckRefusal> {
        let plan = parse_plan(yaml).expect("fixture must parse");
        let report = validate_plan(&plan);
        precheck(&plan, &report)
    }

    // --- Allowlist integrity --------------------------------------------------

    #[test]
    fn read_only_tools_allowlist_is_pinned() {
        // Operator-facing trust boundary. Any change here is a breaking
        // change requiring a new design doc.
        assert_eq!(READ_ONLY_TOOLS, &["Read", "Grep", "Glob", "LS"]);
    }

    // --- L1a canonical corpus -------------------------------------------------

    #[test]
    fn precheck_accepts_valid_readonly_l1a_plan() {
        assert_eq!(run(L1A_VALID), Ok(()));
    }

    /// Carry-forward requirement #10: workspace-write MUST be refused before
    /// any execution path can run. This test asserts the precheck — the only
    /// path between a parsed plan and execution — returns `ValidatorRefused`
    /// for the canonical workspace-write fixture.
    #[test]
    fn precheck_refuses_workspace_write_before_execution() {
        assert_eq!(
            run(L1A_REFUSED_WRITE),
            Err(PrecheckRefusal::ValidatorRefused)
        );
    }

    /// Carry-forward requirement #10: DEEP MUST be refused before any
    /// execution path can run. Symmetric to the workspace-write proof above.
    #[test]
    fn precheck_refuses_deep_before_execution() {
        assert_eq!(
            run(L1A_REFUSED_DEEP),
            Err(PrecheckRefusal::ValidatorRefused)
        );
    }

    #[test]
    fn precheck_refuses_missing_tools() {
        assert_eq!(
            run(L1A_MISSING_TOOLS),
            Err(PrecheckRefusal::ValidatorRefused)
        );
    }

    // --- L1b-specific disallowed-tool refusal --------------------------------

    #[test]
    fn precheck_refuses_l1a_valid_plan_that_declares_disallowed_tool() {
        let result = run(DISALLOWED_TOOL_YAML);
        match result {
            Err(PrecheckRefusal::ToolDisallowed { step_id, tool }) => {
                assert_eq!(step_id, "edit-step");
                assert_eq!(tool, "Edit");
            }
            other => panic!("expected ToolDisallowed for Edit, got {other:?}"),
        }
    }

    #[test]
    fn precheck_refuses_first_disallowed_tool_only() {
        // Two disallowed tools across two steps; precheck must return the
        // FIRST one encountered (walk order: step order, then tool order)
        // so operator output is deterministic.
        //
        // Note: after A2-L2a, a read-only step declaring `Write` is
        // refused at the schema layer (WRITE_TOOL_ON_READONLY) and never
        // reaches the runner's ToolDisallowed walk. To exercise the
        // runner-layer determinism in isolation, both disallowed tools
        // chosen here are tools the schema validator still accepts
        // (Edit and Bash) so the offending pair surfaces deterministically.
        let yaml = r"
name: multi-disallowed
mode: read-only
model_tier: FAST
steps:
  - id: s1
    description: first
    tools: [Read, Edit]
  - id: s2
    description: second
    tools: [Bash]
";
        match run(yaml) {
            Err(PrecheckRefusal::ToolDisallowed { step_id, tool }) => {
                assert_eq!(step_id, "s1");
                assert_eq!(tool, "Edit");
            }
            other => panic!("expected first-encountered ToolDisallowed, got {other:?}"),
        }
    }

    // --- Allowlist whitelist proofs ------------------------------------------

    #[test]
    fn precheck_accepts_each_individual_allowlist_tool() {
        for tool in READ_ONLY_TOOLS {
            let yaml = format!(
                "
name: single-tool
mode: read-only
model_tier: FAST
steps:
  - id: only
    description: uses {tool}
    tools: [{tool}]
"
            );
            assert_eq!(run(&yaml), Ok(()), "tool {tool} must be accepted");
        }
    }

    #[test]
    fn precheck_refuses_explicitly_disallowed_examples() {
        // Make the operator carry-forward concrete: every common write or
        // shell tool is refused before the runner's subprocess path can run.
        //
        // After A2-L2a there are two refusal layers that BOTH count as
        // "refused before execution":
        //
        //   - schema validator (`ValidatorRefused`): a read-only step that
        //     declares `Write` is invalid at the schema layer
        //     (`WRITE_TOOL_ON_READONLY`).
        //   - runner precheck (`ToolDisallowed`): a schema-valid plan that
        //     declares a tool outside the runner allowlist
        //     (Edit / Bash / NotebookEdit / WebFetch on a read-only step).
        //
        // Both refusal kinds halt before execution, which is what the
        // operator carry-forward actually requires. The test below asserts
        // exactly that split.
        for forbidden in ["Edit", "Bash", "NotebookEdit", "WebFetch"] {
            let yaml = format!(
                "
name: forbidden-{forbidden}
mode: read-only
model_tier: FAST
steps:
  - id: s
    description: uses {forbidden}
    tools: [{forbidden}]
"
            );
            match run(&yaml) {
                Err(PrecheckRefusal::ToolDisallowed { tool, .. }) => {
                    assert_eq!(tool, forbidden);
                }
                other => panic!("expected {forbidden} refused via ToolDisallowed, got {other:?}"),
            }
        }

        // Write on a read-only step is now schema-invalid, so the schema
        // validator refuses it first and the runner sees
        // `ValidatorRefused`. This is still "refused before execution".
        let write_yaml = r"
name: forbidden-Write
mode: read-only
model_tier: FAST
steps:
  - id: s
    description: uses Write
    tools: [Write]
";
        match run(write_yaml) {
            Err(PrecheckRefusal::ValidatorRefused) => {}
            other => {
                panic!("expected Write on read-only refused via ValidatorRefused, got {other:?}")
            }
        }
    }

    // --- Pure function discipline --------------------------------------------

    #[test]
    fn precheck_is_pure_and_deterministic() {
        let plan = parse_plan(L1A_VALID).unwrap();
        let report = validate_plan(&plan);
        let r1 = precheck(&plan, &report);
        let r2 = precheck(&plan, &report);
        assert_eq!(r1, r2);
    }
}
