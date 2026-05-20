//! Runner-emitted operator-facing markers.
//!
//! Every marker is a stable `&'static str` token greppable from CI logs and
//! operator transcripts. L1a markers re-exported from
//! [`a2_plan_schema::plan_validate::markers`] pass through verbatim — this
//! module only owns the runner-specific tokens, all prefixed `a2-l1b-`.
//!
//! The `a2-l1b-` prefix is the **justified divergence** from L1a's
//! `a2-l1-` prefix: L1a markers are emitted by the offline validator and
//! describe schema-level acceptance; L1b markers are emitted by the runner
//! and describe execution-layer state. Existing scrapers built for L1a
//! continue to work because L1a markers still appear when the validator
//! runs as pre-flight.

/// First marker the runner emits, before any other action.
pub const RUNNER_START: &str = "a2-l1b-runner-start";

/// The schema validator returned `Refused`. The runner refuses to execute
/// (workspace-write, DEEP, or missing tools). Exit code 2.
pub const PLAN_REFUSED_PRECHECK: &str = "a2-l1b-plan-refused-precheck";

/// A step declared a tool outside the runner's read-only allowlist.
/// Exit code 3.
pub const TOOL_DISALLOWED: &str = "a2-l1b-tool-disallowed";

/// The substrate probe failed (LAW 1 refusal, missing model, or network).
/// Exit code 4.
pub const SUBSTRATE_UNAVAILABLE: &str = "a2-l1b-substrate-unavailable";

/// Every accepted step passed. Exit code 0.
pub const PLAN_EXEC_PASS: &str = "a2-l1b-plan-exec-pass";

/// At least one accepted step failed. Exit code 1.
pub const PLAN_EXEC_FAIL: &str = "a2-l1b-plan-exec-fail";

/// Per-step boundary marker, emitted immediately before subprocess spawn.
pub const STEP_STARTED: &str = "a2-l1b-step-started";

/// Step finished within `MAX_TURNS` and met its declared contract.
pub const STEP_PASSED: &str = "a2-l1b-step-passed";

/// Step failed (non-2xx / empty body / tool violation / `must_contain`
/// miss / turn-loop exceeded).
pub const STEP_FAILED: &str = "a2-l1b-step-failed";

/// Step skipped because an earlier step failed under default abort.
pub const STEP_SKIPPED: &str = "a2-l1b-step-skipped";

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinning test: marker tokens are an operator-facing contract.
    /// Renaming any of these breaks log scrapers and is a breaking change.
    #[test]
    fn marker_tokens_are_pinned() {
        assert_eq!(RUNNER_START, "a2-l1b-runner-start");
        assert_eq!(PLAN_REFUSED_PRECHECK, "a2-l1b-plan-refused-precheck");
        assert_eq!(TOOL_DISALLOWED, "a2-l1b-tool-disallowed");
        assert_eq!(SUBSTRATE_UNAVAILABLE, "a2-l1b-substrate-unavailable");
        assert_eq!(PLAN_EXEC_PASS, "a2-l1b-plan-exec-pass");
        assert_eq!(PLAN_EXEC_FAIL, "a2-l1b-plan-exec-fail");
        assert_eq!(STEP_STARTED, "a2-l1b-step-started");
        assert_eq!(STEP_PASSED, "a2-l1b-step-passed");
        assert_eq!(STEP_FAILED, "a2-l1b-step-failed");
        assert_eq!(STEP_SKIPPED, "a2-l1b-step-skipped");
    }

    #[test]
    fn all_runner_markers_use_a2_l1b_prefix() {
        for m in [
            RUNNER_START,
            PLAN_REFUSED_PRECHECK,
            TOOL_DISALLOWED,
            SUBSTRATE_UNAVAILABLE,
            PLAN_EXEC_PASS,
            PLAN_EXEC_FAIL,
            STEP_STARTED,
            STEP_PASSED,
            STEP_FAILED,
            STEP_SKIPPED,
        ] {
            assert!(
                m.starts_with("a2-l1b-"),
                "runner marker {m:?} must use a2-l1b- prefix"
            );
        }
    }
}
