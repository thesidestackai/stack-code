//! Report writers + exit-code mapping.
//!
//! Two output formats, selected via `claw plan run --report-format`:
//!
//! - `markers` (default) — line-oriented stream of stable `a2-l1[ab]-*`
//!   tokens with optional context. Designed for `grep` from CI logs and
//!   operator transcripts. The marker set is a strict superset of the L1a
//!   marker set so existing scrapers keep working.
//! - `json` — structured per-step result objects suitable for tooling.
//!   Schema is stable: `{plan_name, classification, markers, steps: [...]}`.
//!
//! # Output discipline
//!
//! - All output goes to the writer the caller supplies (stdout from the
//!   CLI). This module NEVER opens a file handle for writing — operators
//!   redirect with shell `>` if persistence is required.
//! - Marker tokens appear at the start of their line, with optional
//!   space-separated context after. No log levels, no timestamps in the
//!   marker stream itself — those are the operator's environment's job.
//!
//! # Exit-code contract (CLI-facing)
//!
//! | Code | Meaning                                                          |
//! | ---- | ---------------------------------------------------------------- |
//! | 0    | `PLAN_EXEC_PASS` — every accepted step passed.                   |
//! | 1    | `PLAN_EXEC_FAIL` — at least one accepted step failed.            |
//! | 2    | `PLAN_REFUSED_PRECHECK` — schema validator refused.              |
//! | 3    | `TOOL_DISALLOWED` — step declared a non-allowlist tool.          |
//! | 4    | `SUBSTRATE_UNAVAILABLE` — broker probe failed.                   |
//! | 5    | YAML parse error (surfaced by the CLI before the runner runs).   |
//! | 6    | `EXIT_WRITE_PATH_REFUSED` — L2b write-target path safety refused. |
//! | 7    | `EXIT_APPROVAL_DENIED` — L2b approval refused / preview non-approvable. |
//! | 8    | reserved — `EXIT_ROLLBACK_FAILED`, future slice.                 |
//! | 9    | `EXIT_CHECKPOINT_FAILED` — L2b checkpoint write failed.          |
//!
//! The runner itself produces a [`crate::runner::PlanReport`]; the CLI maps
//! it to exit codes 0–4 via [`exit_code_for`]. Exit code 5 is the CLI's
//! responsibility (it's raised before a `PlanReport` even exists, on YAML
//! parse failure). Codes 6 and 9 are produced by the L2b workspace-write
//! path (slice 1 / slice 2 respectively) and surface through their own
//! refusal types — [`crate::write_runtime::WriteTargetRefusal::exit_code`]
//! and [`crate::checkpoint::CheckpointError::exit_code`]. Code 7 is bound
//! in slice 3a as [`crate::approval::EXIT_APPROVAL_DENIED`] but
//! intentionally **not** wired into [`exit_code_for`]: slice 3a is
//! offline-only and never executes through `run_plan`. Code 8 remains
//! reserved for the future rollback slice.

use std::io::{self, Write};

use serde_json::json;

use crate::markers;
use crate::runner::{PlanOutcome, PlanReport, StepFailure};

/// Exit code reserved for YAML parse failures surfaced by the CLI before
/// the runner runs. Not produced by [`exit_code_for`] (which never sees a
/// parse failure — it always has a [`PlanReport`]).
pub const EXIT_PARSE_ERROR: i32 = 5;

/// Map a [`PlanReport`] to its CLI exit code.
///
/// Inspects the marker stream to discriminate between PRECHECK refusal
/// causes (2 vs 3) and between general step failures and substrate
/// unavailability (1 vs 4).
#[must_use]
pub fn exit_code_for(report: &PlanReport) -> i32 {
    match report.outcome {
        PlanOutcome::Pass => 0,
        PlanOutcome::RefusedPrecheck => {
            if report.markers.iter().any(|m| m == markers::TOOL_DISALLOWED) {
                3
            } else {
                2
            }
        }
        PlanOutcome::Fail => {
            if report
                .markers
                .iter()
                .any(|m| m == markers::SUBSTRATE_UNAVAILABLE)
            {
                4
            } else {
                1
            }
        }
    }
}

/// Write the report as a line-oriented marker stream.
///
/// Plan-level markers appear first, followed by per-step blocks. Each step
/// emits its own markers and (for failures) a `# reason:` annotation.
///
/// # Errors
/// Returns any I/O error from the underlying writer.
pub fn write_markers<W: Write>(report: &PlanReport, mut writer: W) -> io::Result<()> {
    writeln!(writer, "# plan: {}", report.plan_name)?;
    for marker in &report.markers {
        // Insert step blocks before the plan-final marker if any of the
        // RUNNER_START / plan-final tokens appear; otherwise emit linearly.
        writeln!(writer, "{marker}")?;
    }
    for sr in &report.step_reports {
        writeln!(writer, "# step: {}", sr.step_id)?;
        for marker in &sr.markers {
            writeln!(writer, "{marker}")?;
        }
        if let Err(failure) = &sr.outcome {
            writeln!(writer, "# reason: {}", describe_failure(failure))?;
        }
    }
    Ok(())
}

/// Write the report as a single JSON document.
///
/// Schema (stable):
/// ```json
/// {
///   "plan_name": "...",
///   "outcome": "pass" | "fail" | "refused_precheck",
///   "markers": ["a2-l1b-runner-start", ...],
///   "steps": [
///     {
///       "step_id": "...",
///       "outcome": "passed" | "failed" | "skipped",
///       "markers": ["a2-l1b-step-started", ...],
///       "failure": null | { "kind": "...", "detail": "..." }
///     }
///   ]
/// }
/// ```
///
/// # Errors
/// Returns any I/O error from the underlying writer.
pub fn write_json<W: Write>(report: &PlanReport, mut writer: W) -> io::Result<()> {
    let outcome_str = match report.outcome {
        PlanOutcome::Pass => "pass",
        PlanOutcome::Fail => "fail",
        PlanOutcome::RefusedPrecheck => "refused_precheck",
    };
    let steps: Vec<_> = report
        .step_reports
        .iter()
        .map(|sr| {
            let (outcome, failure) = match &sr.outcome {
                Ok(()) => {
                    let is_skipped = sr.markers.iter().any(|m| m == markers::STEP_SKIPPED);
                    let label = if is_skipped { "skipped" } else { "passed" };
                    (label, serde_json::Value::Null)
                }
                Err(f) => (
                    "failed",
                    json!({
                        "kind": failure_kind(f),
                        "detail": describe_failure(f),
                    }),
                ),
            };
            json!({
                "step_id": sr.step_id,
                "outcome": outcome,
                "markers": sr.markers,
                "failure": failure,
            })
        })
        .collect();
    let doc = json!({
        "plan_name": report.plan_name,
        "outcome": outcome_str,
        "markers": report.markers,
        "steps": steps,
    });
    writeln!(writer, "{doc}")
}

/// Human-readable failure description for marker stream `# reason:` line
/// and JSON `failure.detail`.
fn describe_failure(failure: &StepFailure) -> String {
    match failure {
        StepFailure::SubstrateUnavailable {
            wrapper_exit,
            stderr,
        } => {
            format!("substrate-unavailable (wrapper exit {wrapper_exit}): {stderr}")
        }
        StepFailure::ExitNonZero { code, stderr } => {
            format!("claw exit {code}: {stderr}")
        }
        StepFailure::Timeout => "wall-clock timeout exceeded".to_string(),
        StepFailure::SpawnError(e) => format!("spawn failed: {e}"),
        StepFailure::ParseError(e) => format!("claw json parse error: {e}"),
        StepFailure::EmptyAssistantContent => "empty assistant content".to_string(),
        StepFailure::MissingExpectedMarker(m) => format!("missing required marker: {m}"),
    }
}

/// Short discriminator suitable for `jq` filtering on the JSON report.
fn failure_kind(failure: &StepFailure) -> &'static str {
    match failure {
        StepFailure::SubstrateUnavailable { .. } => "substrate_unavailable",
        StepFailure::ExitNonZero { .. } => "exit_nonzero",
        StepFailure::Timeout => "timeout",
        StepFailure::SpawnError(_) => "spawn_error",
        StepFailure::ParseError(_) => "parse_error",
        StepFailure::EmptyAssistantContent => "empty_assistant_content",
        StepFailure::MissingExpectedMarker(_) => "missing_expected_marker",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::PrecheckRefusal;
    use crate::runner::{
        aggregate_plan_report, refused_precheck_report, substrate_unavailable_report, StepFailure,
        StepOutcomeForReport,
    };

    fn pass_report() -> PlanReport {
        aggregate_plan_report(
            "test",
            vec![
                ("s1".to_string(), StepOutcomeForReport::Passed),
                ("s2".to_string(), StepOutcomeForReport::Passed),
            ],
        )
    }

    fn fail_report() -> PlanReport {
        aggregate_plan_report(
            "test",
            vec![
                ("s1".to_string(), StepOutcomeForReport::Passed),
                (
                    "s2".to_string(),
                    StepOutcomeForReport::Failed(StepFailure::EmptyAssistantContent),
                ),
                ("s3".to_string(), StepOutcomeForReport::Skipped),
            ],
        )
    }

    // --- exit code mapping ----------------------------------------------------

    #[test]
    fn exit_code_pass_is_zero() {
        assert_eq!(exit_code_for(&pass_report()), 0);
    }

    #[test]
    fn exit_code_step_failure_is_one() {
        assert_eq!(exit_code_for(&fail_report()), 1);
    }

    #[test]
    fn exit_code_validator_refused_is_two() {
        let r = refused_precheck_report("p", &PrecheckRefusal::ValidatorRefused);
        assert_eq!(exit_code_for(&r), 2);
    }

    #[test]
    fn exit_code_tool_disallowed_is_three() {
        let r = refused_precheck_report(
            "p",
            &PrecheckRefusal::ToolDisallowed {
                step_id: "s".into(),
                tool: "Edit".into(),
            },
        );
        assert_eq!(exit_code_for(&r), 3);
    }

    #[test]
    fn exit_code_substrate_unavailable_is_four() {
        assert_eq!(exit_code_for(&substrate_unavailable_report("p")), 4);
    }

    #[test]
    fn exit_code_parse_error_constant_is_five() {
        assert_eq!(EXIT_PARSE_ERROR, 5);
    }

    // --- marker-stream writer -------------------------------------------------

    #[test]
    fn write_markers_emits_plan_name_header() {
        let mut buf = Vec::new();
        write_markers(&pass_report(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("# plan: test"));
    }

    #[test]
    fn write_markers_emits_plan_pass_marker() {
        let mut buf = Vec::new();
        write_markers(&pass_report(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(markers::RUNNER_START));
        assert!(text.contains(markers::PLAN_EXEC_PASS));
        for s in ["s1", "s2"] {
            assert!(text.contains(&format!("# step: {s}")));
        }
        assert!(text.contains(markers::STEP_PASSED));
    }

    #[test]
    fn write_markers_emits_failure_reason_line() {
        let mut buf = Vec::new();
        write_markers(&fail_report(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(markers::PLAN_EXEC_FAIL));
        assert!(text.contains(markers::STEP_FAILED));
        assert!(text.contains(markers::STEP_SKIPPED));
        assert!(text.contains("# reason: empty assistant content"));
    }

    #[test]
    fn write_markers_for_validator_refusal_emits_refused_marker() {
        let r = refused_precheck_report("p", &PrecheckRefusal::ValidatorRefused);
        let mut buf = Vec::new();
        write_markers(&r, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(markers::PLAN_REFUSED_PRECHECK));
        assert!(!text.contains(markers::TOOL_DISALLOWED));
    }

    #[test]
    fn write_markers_for_tool_disallowed_emits_tool_marker() {
        let r = refused_precheck_report(
            "p",
            &PrecheckRefusal::ToolDisallowed {
                step_id: "s".into(),
                tool: "Edit".into(),
            },
        );
        let mut buf = Vec::new();
        write_markers(&r, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(markers::TOOL_DISALLOWED));
        assert!(!text.contains(markers::PLAN_REFUSED_PRECHECK));
    }

    // --- JSON writer ----------------------------------------------------------

    fn parse_json(buf: &[u8]) -> serde_json::Value {
        serde_json::from_slice(buf).expect("json writer must emit valid JSON")
    }

    #[test]
    fn write_json_emits_stable_shape_for_pass() {
        let mut buf = Vec::new();
        write_json(&pass_report(), &mut buf).unwrap();
        let v = parse_json(&buf);
        assert_eq!(v["plan_name"], "test");
        assert_eq!(v["outcome"], "pass");
        assert!(v["markers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == markers::PLAN_EXEC_PASS));
        let steps = v["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
        for step in steps {
            assert_eq!(step["outcome"], "passed");
            assert!(step["failure"].is_null());
        }
    }

    #[test]
    fn write_json_emits_failure_detail_block() {
        let mut buf = Vec::new();
        write_json(&fail_report(), &mut buf).unwrap();
        let v = parse_json(&buf);
        assert_eq!(v["outcome"], "fail");
        let steps = v["steps"].as_array().unwrap();
        let s2 = steps.iter().find(|s| s["step_id"] == "s2").unwrap();
        assert_eq!(s2["outcome"], "failed");
        assert_eq!(s2["failure"]["kind"], "empty_assistant_content");
        let s3 = steps.iter().find(|s| s["step_id"] == "s3").unwrap();
        assert_eq!(s3["outcome"], "skipped");
        assert!(s3["failure"].is_null());
    }

    #[test]
    fn write_json_emits_refused_precheck_outcome() {
        let r = refused_precheck_report("p", &PrecheckRefusal::ValidatorRefused);
        let mut buf = Vec::new();
        write_json(&r, &mut buf).unwrap();
        let v = parse_json(&buf);
        assert_eq!(v["outcome"], "refused_precheck");
        assert!(v["steps"].as_array().unwrap().is_empty());
    }
}
