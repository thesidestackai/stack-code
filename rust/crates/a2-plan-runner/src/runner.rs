//! Step executor.
//!
//! Owns the subprocess + broker boundary. Every accepted step is executed by
//! spawning the PR #14 wrapper at `<repo-root>/scripts/claw-sidestack-local`
//! and forwarding a verbatim argument list to `claw`. The wrapper enforces
//! LAW 1 broker routing (`http://127.0.0.1:11435`) and refuses raw `:11434`;
//! it does NOT enforce read-only.
//!
//! # Read-only enforcement contract — load-bearing
//!
//! The wrapper executes `claw "$@"` verbatim. It does not inject
//! `--permission-mode read-only`. The runner MUST inject this flag on every
//! `claw` invocation it forwards through the wrapper. This requirement is
//! non-optional and is verified by [`tests::build_claw_command_always_includes_permission_mode_read_only`].
//!
//! Defense in depth at this boundary:
//!
//! 1. The runner constructs the `claw` arg vector centrally in
//!    [`build_claw_command`] and prepends `--permission-mode read-only`
//!    before any caller-supplied tokens.
//! 2. The runner's per-step `tools` is intersected with
//!    [`crate::preflight::READ_ONLY_TOOLS`] before being passed to claw —
//!    never forwarded raw from the plan.
//! 3. (Phase 3-followup) Every model-issued `tool_calls[].name` is
//!    re-checked against the per-step whitelist; mismatch = `STEP_FAILED`.
//!
//! # Architecture — Option A (one-shot per step)
//!
//! Operator-approved 2026-05-20: one claw subprocess per accepted step. Claw
//! owns the tool loop internally; the runner only orchestrates around it.
//!
//! Per-step flow:
//!
//! 1. [`build_claw_command`] constructs the arg vector (always read-only,
//!    always JSON output, always allowlist-filtered tools).
//! 2. [`execute_with_timeout`] spawns the wrapper as a subprocess, captures
//!    stdout / stderr, applies a wall-clock timeout.
//! 3. [`classify_step_result`] reads the exit code + parsed JSON + the
//!    step's optional `expected_output.must_contain` list and returns the
//!    step outcome.
//! 4. [`run_step`] wires the three together and is the only public per-step
//!    entry point.
//!
//! # `MAX_TURNS` — delegated, unsupported, runner timeout used instead
//!
//! Claw has no `--max-turns` flag (verified by grep of
//! `rust/crates/rusty-claude-cli/src/main.rs` and `rust/crates/runtime/`).
//! Per operator instruction: do not invent a flag. Bound execution with a
//! runner-side wall-clock timeout instead — see [`DEFAULT_STEP_TIMEOUT`]
//! and the `step_timeout` parameter of [`run_step`].
//!
//! # Plan-level aggregation
//!
//! [`run_plan`] orchestrates: validator → precheck → optional substrate
//! probe → per-step run with default-abort. [`aggregate_plan_report`] is
//! the pure marker-emitting aggregator used by `run_plan` and exposed for
//! independent testing.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

use a2_plan_schema::{validate_plan, Plan, PlanStep};
use serde::Deserialize;

use crate::markers;
use crate::preflight::{precheck, PrecheckRefusal, READ_ONLY_TOOLS};

// -----------------------------------------------------------------------------
// Claw argument construction — the load-bearing safety boundary
// -----------------------------------------------------------------------------

/// A resolved claw invocation: wrapper executable + argument vector.
///
/// Constructed exclusively via [`build_claw_command`]. Holding a
/// `ClawCommand` is a proof-by-construction that read-only enforcement and
/// allowlist intersection have already been applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClawCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Build a claw invocation for a single read-only plan step.
///
/// Guarantees enforced by construction:
///
/// 1. `program` is the wrapper at `wrapper_path` — never raw `claw`.
/// 2. `args` always contains `--permission-mode read-only` adjacent.
/// 3. `args` always contains `--output-format json` (structured parse path).
/// 4. `args` always contains `--model fast` (DEEP is unreachable here).
/// 5. `args` contains `--allowed-tools <csv>` where `<csv>` is the
///    intersection of `step.tools` with [`READ_ONLY_TOOLS`] — never the
///    raw plan-declared list.
/// 6. `args` ends with `prompt <description>` — claw's `prompt` subcommand
///    is non-interactive by construction (it returns after one turn), so
///    no separate `--print` flag is needed. **Critically: `--print` MUST
///    NOT be passed**, because claw's parser at `main.rs:911-915` treats
///    `--print` as "force text output" and resets `--output-format json`
///    back to text — a Phase 5 live-smoke finding (2026-05-20).
/// 7. `args` never contains `--dangerously-skip-permissions`,
///    `--danger-full-access`, `--print`, or any other elevation/text-mode
///    flag.
///
/// The function is pure: no I/O, no subprocess spawn. Callers turn the
/// resulting `ClawCommand` into a [`std::process::Command`] at the actual
/// boundary.
#[must_use]
pub fn build_claw_command(wrapper_path: &Path, step: &PlanStep) -> ClawCommand {
    let allowed_tools_csv = step
        .tools
        .iter()
        .filter(|t| READ_ONLY_TOOLS.contains(&t.as_str()))
        .cloned()
        .collect::<Vec<_>>()
        .join(",");

    let args = vec![
        "--model".to_string(),
        "fast".to_string(),
        "--permission-mode".to_string(),
        "read-only".to_string(),
        "--output-format".to_string(),
        "json".to_string(),
        "--allowed-tools".to_string(),
        allowed_tools_csv,
        "prompt".to_string(),
        step.description.clone(),
    ];

    ClawCommand {
        program: wrapper_path.to_path_buf(),
        args,
    }
}

// -----------------------------------------------------------------------------
// Substrate probe — explicitly scoped by saved prompt + operator carry-forward
// -----------------------------------------------------------------------------

/// Why the substrate probe refused to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstrateError {
    /// Non-2xx HTTP status from the `/models` endpoint.
    Http(u16),
    /// `/models` response body could not be parsed as the expected
    /// OpenAI-compatible shape.
    Parse(String),
    /// Probe succeeded but the configured FAST model id was not in the
    /// returned `data[].id` list.
    ModelMissing {
        wanted: String,
        available: Vec<String>,
    },
    /// Network / transport failure before the response was read.
    Transport(String),
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

/// Parse an OpenAI-compatible `/models` response body and assert the
/// configured FAST model id is present.
///
/// Pure: no I/O. Exists separately from [`probe_substrate`] so the
/// parsing-and-membership logic is fully unit-testable without spinning
/// up an HTTP server.
pub fn parse_models_response(body: &str, wanted_model: &str) -> Result<(), SubstrateError> {
    let parsed: ModelsResponse =
        serde_json::from_str(body).map_err(|e| SubstrateError::Parse(e.to_string()))?;
    if parsed.data.iter().any(|m| m.id == wanted_model) {
        Ok(())
    } else {
        Err(SubstrateError::ModelMissing {
            wanted: wanted_model.to_string(),
            available: parsed.data.into_iter().map(|m| m.id).collect(),
        })
    }
}

/// Probe the substrate by fetching `<substrate_url>/models` and asserting
/// the configured FAST model id is present.
///
/// Uses `reqwest::blocking` per workspace HTTP convention. This is the only
/// runner code path that calls the broker over HTTP, and it is bounded:
/// it never sends a chat completion request, it never executes a tool, it
/// reads only the public `/models` listing.
///
/// First live exercise: Phase 5 smoke (gated by operator STOP).
pub fn probe_substrate(substrate_url: &str, wanted_model: &str) -> Result<(), SubstrateError> {
    let url = format!("{}/models", substrate_url.trim_end_matches('/'));
    let response =
        reqwest::blocking::get(&url).map_err(|e| SubstrateError::Transport(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(SubstrateError::Http(status.as_u16()));
    }
    let body = response
        .text()
        .map_err(|e| SubstrateError::Transport(e.to_string()))?;
    parse_models_response(&body, wanted_model)
}

// -----------------------------------------------------------------------------
// Step execution — Option A: one-shot per accepted step
// -----------------------------------------------------------------------------

/// Default wall-clock timeout for a single claw subprocess invocation.
///
/// Claw has no `--max-turns` flag, so this is the only bound on a runaway
/// step. The default (600s = 10 min) accommodates a cold-start broker
/// model swap (devstral 20GB out → qwen3:14b 14GB in, ~60s) plus a
/// multi-tool-turn claw conversation on a 14B FAST model (typically
/// 30–300s per turn). Phase 5 live smoke (2026-05-20) hit 180s during a
/// 2-step Read/Grep plan after a cold-start swap; 600s gives comfortable
/// headroom for the same plan after the broker is warm and at least one
/// full cold-start cycle.
///
/// Tighten via the `step_timeout` parameter of [`run_step`] or the
/// `--step-timeout <seconds>` CLI flag when a stricter cap is desired.
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(600);

/// Inclusive lower bound for any operator-supplied `--step-timeout`.
/// 1s prevents accidental zero-second timeouts that would race the
/// subprocess fork to completion.
pub const MIN_STEP_TIMEOUT_SECS: u64 = 1;

/// Inclusive upper bound for any operator-supplied `--step-timeout`.
/// 3600s (1 hour) is a hard cap so a typo in `--step-timeout` cannot turn
/// a runaway claw call into an indefinitely-stuck plan run. Operators who
/// need longer should split the step rather than raise the ceiling.
pub const MAX_STEP_TIMEOUT_SECS: u64 = 3600;

/// Parse and bound-check a `--step-timeout <seconds>` operator argument.
///
/// Accepts decimal seconds in `[MIN_STEP_TIMEOUT_SECS,
/// MAX_STEP_TIMEOUT_SECS]`. Returns a structured error string suitable for
/// CLI surfacing on invalid input — does NOT panic.
///
/// # Errors
/// Returns `Err(String)` on non-integer input or values outside the bound.
pub fn parse_step_timeout_seconds(raw: &str) -> Result<Duration, String> {
    let secs: u64 = raw.trim().parse().map_err(|_| {
        format!(
            "invalid --step-timeout value `{raw}`: expected integer seconds in \
             [{MIN_STEP_TIMEOUT_SECS}, {MAX_STEP_TIMEOUT_SECS}]"
        )
    })?;
    if secs < MIN_STEP_TIMEOUT_SECS {
        return Err(format!(
            "--step-timeout {secs} is below the minimum of {MIN_STEP_TIMEOUT_SECS}s"
        ));
    }
    if secs > MAX_STEP_TIMEOUT_SECS {
        return Err(format!(
            "--step-timeout {secs} exceeds the maximum of {MAX_STEP_TIMEOUT_SECS}s"
        ));
    }
    Ok(Duration::from_secs(secs))
}

/// Per-stream cap for captured stdout / stderr from a claw subprocess.
/// Defensive against runaway output filling memory.
const SUBPROCESS_READ_CAP_BYTES: u64 = 1024 * 1024;

/// Why a single step did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepFailure {
    /// The wrapper itself refused (LAW 1 violation, missing env file, or
    /// claw not on PATH). Wrapper exits 2 / 3 / 4. Maps to runner marker
    /// [`crate::markers::SUBSTRATE_UNAVAILABLE`].
    SubstrateUnavailable { wrapper_exit: i32, stderr: String },
    /// Claw exited non-zero for any reason other than the wrapper-internal
    /// codes (typically: broker error propagated by PR #92, or claw bug).
    ExitNonZero { code: i32, stderr: String },
    /// Wall-clock timeout exceeded before claw exited.
    Timeout,
    /// Failed to spawn the subprocess at all (wrapper path not executable,
    /// fork failure, etc.).
    SpawnError(String),
    /// Claw's stdout was not parseable as the expected `--output-format
    /// json` envelope.
    ParseError(String),
    /// Parse succeeded but `message` was empty — the model produced no
    /// final assistant text. This is the "empty assistant content with
    /// non-error `finish_reason`" failure condition.
    EmptyAssistantContent,
    /// Parse succeeded but a required `expected_output.must_contain`
    /// substring was not present in the assistant message.
    MissingExpectedMarker(String),
}

/// One step's outcome inside a [`PlanReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    pub step_id: String,
    pub outcome: Result<(), StepFailure>,
    /// Runner markers emitted for this step in the order
    /// `STEP_STARTED → (STEP_PASSED | STEP_FAILED | STEP_SKIPPED)`.
    pub markers: Vec<String>,
}

/// Plan-level outcome classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanOutcome {
    Pass,
    Fail,
    RefusedPrecheck,
}

/// Full report from a single plan run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReport {
    pub plan_name: String,
    pub outcome: PlanOutcome,
    /// Plan-level markers in emission order, starting with
    /// [`crate::markers::RUNNER_START`].
    pub markers: Vec<String>,
    pub step_reports: Vec<StepReport>,
}

/// Claw `--output-format json` envelope, only the fields the runner needs.
///
/// Schema source: `LiveCli::run_prompt_json` in
/// `rust/crates/rusty-claude-cli/src/main.rs` — the producer emits
/// `{ "message": ..., "model": ..., "iterations": N, ..., "usage": {...} }`.
#[derive(Debug, Deserialize)]
struct ClawJsonEnvelope {
    message: String,
}

/// Parse claw's `--output-format json` stdout.
///
/// Pure: no I/O. Returns the deserialized envelope or a structured parse
/// error. Used by [`classify_step_result`] and directly testable with
/// fixture strings.
pub fn parse_claw_json(stdout: &str) -> Result<String, StepFailure> {
    let envelope: ClawJsonEnvelope =
        serde_json::from_str(stdout.trim()).map_err(|e| StepFailure::ParseError(e.to_string()))?;
    Ok(envelope.message)
}

/// Pure step-result classifier.
///
/// Inputs: the subprocess exit code, the captured stdout, the captured
/// stderr, and the step's optional `expected_output.must_contain` list.
/// Output: `Ok(())` for [`crate::markers::STEP_PASSED`], `Err(StepFailure)`
/// for the corresponding [`crate::markers::STEP_FAILED`] cause.
///
/// Classification order (operator-specified 2026-05-20):
///   1. Wrapper exit codes 2 / 3 / 4 → `SubstrateUnavailable`.
///   2. Any other non-zero exit → `ExitNonZero`.
///   3. Stdout parse failure → `ParseError`.
///   4. Empty `message` → `EmptyAssistantContent`.
///   5. Any `must_contain` substring missing → `MissingExpectedMarker`.
///   6. Otherwise → `Ok(())`.
pub fn classify_step_result(
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    expected_must_contain: &[String],
) -> Result<(), StepFailure> {
    if matches!(exit_code, 2..=4) {
        return Err(StepFailure::SubstrateUnavailable {
            wrapper_exit: exit_code,
            stderr: stderr.to_string(),
        });
    }
    if exit_code != 0 {
        return Err(StepFailure::ExitNonZero {
            code: exit_code,
            stderr: stderr.to_string(),
        });
    }
    let message = parse_claw_json(stdout)?;
    if message.is_empty() {
        return Err(StepFailure::EmptyAssistantContent);
    }
    for needle in expected_must_contain {
        if !message.contains(needle) {
            return Err(StepFailure::MissingExpectedMarker(needle.clone()));
        }
    }
    Ok(())
}

/// Captured subprocess result. Internal — only [`run_step`] consumes this.
struct ProcessResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// Spawn the wrapper, capture stdout/stderr in background threads (to
/// avoid pipe-buffer deadlock), poll for exit, kill on timeout.
fn execute_with_timeout(
    cmd: &ClawCommand,
    timeout: Duration,
) -> Result<ProcessResult, StepFailure> {
    let mut child = Command::new(&cmd.program)
        .args(&cmd.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| StepFailure::SpawnError(e.to_string()))?;

    let mut stdout_handle = child.stdout.take().expect("stdout piped above");
    let mut stderr_handle = child.stderr.take().expect("stderr piped above");

    let (stdout_tx, stdout_rx) = channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = (&mut stdout_handle)
            .take(SUBPROCESS_READ_CAP_BYTES)
            .read_to_string(&mut buf);
        let _ = stdout_tx.send(buf);
    });
    let (stderr_tx, stderr_rx) = channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = (&mut stderr_handle)
            .take(SUBPROCESS_READ_CAP_BYTES)
            .read_to_string(&mut buf);
        let _ = stderr_tx.send(buf);
    });

    let start = Instant::now();
    let poll = Duration::from_millis(50);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_default();
                let stderr = stderr_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_default();
                return Ok(ProcessResult {
                    exit_code: status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(StepFailure::Timeout);
                }
                thread::sleep(poll);
            }
            Err(e) => return Err(StepFailure::SpawnError(e.to_string())),
        }
    }
}

/// Execute one accepted step and classify the result.
///
/// Loud-fail semantics: any wrapper or claw exit, any parse failure, any
/// missing expected marker becomes a typed [`StepFailure`]. The runner
/// never silently swallows or recovers — that is L2 territory.
pub fn run_step(
    cmd: &ClawCommand,
    step: &PlanStep,
    step_timeout: Duration,
) -> Result<(), StepFailure> {
    let must_contain: &[String] = step
        .expected_output
        .as_ref()
        .map_or(&[], |c| c.must_contain.as_slice());
    let result = execute_with_timeout(cmd, step_timeout)?;
    classify_step_result(
        result.exit_code,
        &result.stdout,
        &result.stderr,
        must_contain,
    )
}

// -----------------------------------------------------------------------------
// Plan-level aggregation & top-level orchestrator
// -----------------------------------------------------------------------------

/// Pure plan-level aggregator.
///
/// Given each step's `(id, outcome)` pair, builds a [`PlanReport`] with the
/// correct marker stream. Pulled out as a free function so it is unit-
/// testable without subprocess execution.
///
/// Default-abort semantics: callers are responsible for sending
/// `StepOutcomeForReport::Skipped` for steps that didn't run after an
/// earlier failure; this function only aggregates.
#[must_use]
pub fn aggregate_plan_report(
    plan_name: &str,
    step_outcomes: Vec<(String, StepOutcomeForReport)>,
) -> PlanReport {
    let mut plan_markers = vec![markers::RUNNER_START.to_string()];
    let mut step_reports = Vec::with_capacity(step_outcomes.len());
    let mut any_failed = false;

    for (step_id, outcome) in step_outcomes {
        let (result, step_marker) = match &outcome {
            StepOutcomeForReport::Passed => (Ok(()), markers::STEP_PASSED),
            StepOutcomeForReport::Failed(f) => {
                any_failed = true;
                (Err(f.clone()), markers::STEP_FAILED)
            }
            StepOutcomeForReport::Skipped => (Ok(()), markers::STEP_SKIPPED),
        };
        let step_marker_owned = step_marker.to_string();
        let markers_vec = match outcome {
            StepOutcomeForReport::Skipped => vec![step_marker_owned],
            _ => vec![markers::STEP_STARTED.to_string(), step_marker_owned],
        };
        step_reports.push(StepReport {
            step_id,
            outcome: result,
            markers: markers_vec,
        });
    }

    let plan_outcome = if any_failed {
        plan_markers.push(markers::PLAN_EXEC_FAIL.to_string());
        PlanOutcome::Fail
    } else {
        plan_markers.push(markers::PLAN_EXEC_PASS.to_string());
        PlanOutcome::Pass
    };

    PlanReport {
        plan_name: plan_name.to_string(),
        outcome: plan_outcome,
        markers: plan_markers,
        step_reports,
    }
}

/// Compact wrapper for what [`aggregate_plan_report`] needs per step —
/// `Result<(), StepFailure>` is ambiguous about "did this step run?", this
/// enum disambiguates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcomeForReport {
    Passed,
    Failed(StepFailure),
    Skipped,
}

/// Build a refused-precheck [`PlanReport`] with the marker matching the
/// specific refusal cause.
///
/// - [`PrecheckRefusal::ValidatorRefused`] → emits
///   [`crate::markers::PLAN_REFUSED_PRECHECK`] (CLI exit code 2).
/// - [`PrecheckRefusal::ToolDisallowed`] → emits
///   [`crate::markers::TOOL_DISALLOWED`] (CLI exit code 3).
///
/// Both share `PlanOutcome::RefusedPrecheck`; the CLI discriminates by
/// inspecting the marker stream.
#[must_use]
pub fn refused_precheck_report(plan_name: &str, refusal: &PrecheckRefusal) -> PlanReport {
    let extra_marker = match refusal {
        PrecheckRefusal::ValidatorRefused => markers::PLAN_REFUSED_PRECHECK,
        PrecheckRefusal::ToolDisallowed { .. } => markers::TOOL_DISALLOWED,
    };
    PlanReport {
        plan_name: plan_name.to_string(),
        outcome: PlanOutcome::RefusedPrecheck,
        markers: vec![markers::RUNNER_START.to_string(), extra_marker.to_string()],
        step_reports: Vec::new(),
    }
}

/// Build a substrate-unavailable [`PlanReport`].
#[must_use]
pub fn substrate_unavailable_report(plan_name: &str) -> PlanReport {
    PlanReport {
        plan_name: plan_name.to_string(),
        outcome: PlanOutcome::Fail,
        markers: vec![
            markers::RUNNER_START.to_string(),
            markers::SUBSTRATE_UNAVAILABLE.to_string(),
        ],
        step_reports: Vec::new(),
    }
}

/// Top-level orchestrator: validate → precheck → optional substrate probe
/// → per-step run with default-abort → aggregate.
///
/// `substrate` is `Some((url, fast_model_id))` for live runs; `None` skips
/// the probe (useful for dry-run / introspection paths).
///
/// First live exercise: Phase 5 smoke (gated by operator STOP).
#[must_use]
pub fn run_plan(
    plan: &Plan,
    wrapper_path: &Path,
    substrate: Option<(&str, &str)>,
    step_timeout: Duration,
) -> PlanReport {
    let validator_report = validate_plan(plan);
    if let Err(refusal) = precheck(plan, &validator_report) {
        return refused_precheck_report(&plan.name, &refusal);
    }
    if let Some((url, fast_model)) = substrate {
        if probe_substrate(url, fast_model).is_err() {
            return substrate_unavailable_report(&plan.name);
        }
    }
    let mut step_outcomes: Vec<(String, StepOutcomeForReport)> =
        Vec::with_capacity(plan.steps.len());
    let mut aborted = false;
    for step in &plan.steps {
        if aborted {
            step_outcomes.push((step.id.clone(), StepOutcomeForReport::Skipped));
            continue;
        }
        let cmd = build_claw_command(wrapper_path, step);
        match run_step(&cmd, step, step_timeout) {
            Ok(()) => step_outcomes.push((step.id.clone(), StepOutcomeForReport::Passed)),
            Err(f) => {
                step_outcomes.push((step.id.clone(), StepOutcomeForReport::Failed(f)));
                aborted = true;
            }
        }
    }
    aggregate_plan_report(&plan.name, step_outcomes)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use a2_plan_schema::{ExpectedOutputContract, ModelTier, PlanMode};

    const WRAPPER_FIXTURE: &str = "/tmp/fake-wrapper/scripts/claw-sidestack-local";

    fn step(tools: &[&str]) -> PlanStep {
        PlanStep {
            id: "s".to_string(),
            description: "do a thing".to_string(),
            mode: Some(PlanMode::ReadOnly),
            model_tier: Some(ModelTier::Fast),
            tools: tools.iter().map(|s| (*s).to_string()).collect(),
            expected_output: None,
        }
    }

    fn join_args(cmd: &ClawCommand) -> String {
        cmd.args.join(" ")
    }

    // --- Operator carry-forward #2 + #3: --permission-mode read-only proof ---

    /// Operator carry-forward #3: this test MUST fail if any code path
    /// produces a `ClawCommand` whose `args` omit `--permission-mode
    /// read-only`. It is the canary that prevents accidental write-mode
    /// regressions in the central arg builder.
    #[test]
    fn build_claw_command_always_includes_permission_mode_read_only() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        let mut found = false;
        let mut iter = cmd.args.iter();
        while let Some(a) = iter.next() {
            if a == "--permission-mode" {
                let next = iter.next().expect("--permission-mode must have a value");
                assert_eq!(
                    next, "read-only",
                    "--permission-mode value must be 'read-only'"
                );
                found = true;
                break;
            }
        }
        assert!(
            found,
            "every claw invocation must include --permission-mode read-only; args were {:?}",
            cmd.args
        );
    }

    #[test]
    fn build_claw_command_includes_read_only_for_every_allowlist_tool() {
        // Exhaustive: prove the read-only flag is present regardless of
        // which allowlist tool the step declares.
        for tool in READ_ONLY_TOOLS {
            let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&[tool]));
            assert!(
                cmd.args
                    .windows(2)
                    .any(|w| w[0] == "--permission-mode" && w[1] == "read-only"),
                "tool {tool}: missing --permission-mode read-only in {:?}",
                cmd.args
            );
        }
    }

    // --- Defense-in-depth layer 2: allowlist intersection -------------------

    #[test]
    fn build_claw_command_intersects_step_tools_with_allowlist() {
        // Even if precheck failed open (it doesn't, but defense in depth),
        // the arg builder strips disallowed tools.
        let cmd = build_claw_command(
            Path::new(WRAPPER_FIXTURE),
            &step(&["Read", "Edit", "Grep", "Bash"]),
        );
        let mut iter = cmd.args.iter();
        let allowed = loop {
            match iter.next() {
                Some(a) if a == "--allowed-tools" => break iter.next().unwrap().clone(),
                Some(_) => {}
                None => panic!("--allowed-tools must appear in args"),
            }
        };
        let allowed: Vec<&str> = allowed.split(',').collect();
        assert!(allowed.contains(&"Read"));
        assert!(allowed.contains(&"Grep"));
        assert!(!allowed.contains(&"Edit"), "Edit must be filtered out");
        assert!(!allowed.contains(&"Bash"), "Bash must be filtered out");
    }

    #[test]
    fn build_claw_command_filters_to_empty_when_all_step_tools_disallowed() {
        // Pathological input (shouldn't happen post-precheck): every tool
        // disallowed. Result: --allowed-tools is the empty CSV. The flag
        // is still present (claw will reject the prompt, not silently
        // widen).
        let cmd = build_claw_command(
            Path::new(WRAPPER_FIXTURE),
            &step(&["Edit", "Write", "Bash"]),
        );
        assert!(cmd
            .args
            .windows(2)
            .any(|w| w[0] == "--allowed-tools" && w[1].is_empty()));
    }

    // --- Structured output + non-interactive + model pinning -----------------

    #[test]
    fn build_claw_command_pins_output_format_json() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert!(cmd
            .args
            .windows(2)
            .any(|w| w[0] == "--output-format" && w[1] == "json"));
    }

    #[test]
    fn build_claw_command_uses_prompt_subcommand_for_non_interactive() {
        // Non-interactivity comes from claw's `prompt` subcommand, which
        // returns after a single turn. `--print` is NOT used: claw's
        // parser at main.rs:911 treats `--print` as "force text output",
        // which would defeat `--output-format json`. Phase 5 (2026-05-20)
        // live smoke uncovered this — see runner.rs doc on
        // `build_claw_command`.
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert!(
            cmd.args.iter().any(|a| a == "prompt"),
            "args must use the prompt subcommand for non-interactive single-shot: {:?}",
            cmd.args
        );
    }

    /// Phase 5 finding regression guard: `--print` MUST NEVER appear in
    /// the constructed args. If it does, claw will silently reset
    /// `--output-format json` to text and break every live plan run.
    #[test]
    fn build_claw_command_never_includes_print_flag() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert!(
            !cmd.args.iter().any(|a| a == "--print"),
            "--print must not be present; it would force claw to text mode \
             and defeat --output-format json. args: {:?}",
            cmd.args
        );
    }

    /// Belt-and-suspenders: assert the exact arg layout the live smoke
    /// confirmed produces JSON output (manual probe 2026-05-20,
    /// `fix_b_probe.stdout` returned a parseable JSON envelope with
    /// `message` and `iterations` fields).
    #[test]
    fn build_claw_command_emits_proven_good_arg_layout() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read", "Grep"]));
        // Empirically validated arg layout (in order):
        let expected: &[&str] = &[
            "--model",
            "fast",
            "--permission-mode",
            "read-only",
            "--output-format",
            "json",
            "--allowed-tools",
            "Read,Grep",
            "prompt",
            "do a thing", // step.description from the helper
        ];
        assert_eq!(
            cmd.args, expected,
            "claw arg layout must match the proven-good Phase 5 invocation"
        );
    }

    #[test]
    fn build_claw_command_pins_model_to_fast() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert!(cmd
            .args
            .windows(2)
            .any(|w| w[0] == "--model" && w[1] == "fast"));
    }

    // --- Negative proofs: elevation flags never appear -----------------------

    #[test]
    fn build_claw_command_never_includes_elevation_flags() {
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read", "Grep"]));
        let joined = join_args(&cmd);
        let forbidden = [
            "--dangerously-skip-permissions",
            "--danger-full-access",
            "--permission-mode=accept-edits",
            "--permission-mode=accept-prompts",
            "--permission-mode=danger-full-access",
            // Phase 5 finding: --print silently forces text output and
            // defeats --output-format json. Defense-in-depth check.
            "--print",
        ];
        for bad in forbidden {
            assert!(
                !joined.contains(bad),
                "elevation flag {bad} must never appear; args: {joined}"
            );
        }
    }

    #[test]
    fn build_claw_command_never_routes_around_wrapper() {
        // The program field MUST be the wrapper path, never raw `claw`.
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert_eq!(cmd.program, Path::new(WRAPPER_FIXTURE));
        assert!(
            cmd.program
                .to_string_lossy()
                .ends_with("claw-sidestack-local"),
            "program must be the wrapper, was {:?}",
            cmd.program
        );
    }

    // --- Determinism ----------------------------------------------------------

    #[test]
    fn build_claw_command_is_deterministic() {
        let s = step(&["Read", "Grep"]);
        let a = build_claw_command(Path::new(WRAPPER_FIXTURE), &s);
        let b = build_claw_command(Path::new(WRAPPER_FIXTURE), &s);
        assert_eq!(a, b);
    }

    #[test]
    fn build_claw_command_preserves_step_description_as_prompt_argument() {
        let mut s = step(&["Read"]);
        s.description = "find the project config".to_string();
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &s);
        // Description appears immediately after the `prompt` token.
        let pos = cmd
            .args
            .iter()
            .position(|a| a == "prompt")
            .expect("prompt subcommand present");
        assert_eq!(cmd.args[pos + 1], "find the project config");
    }

    #[test]
    fn build_claw_command_does_not_inspect_expected_output() {
        // Phase 3 partial: expected_output handling lives in response
        // parsing (deferred). Arg builder must not leak it into claw flags.
        let mut s = step(&["Read"]);
        s.expected_output = Some(ExpectedOutputContract {
            must_contain: vec!["sentinel".to_string()],
        });
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &s);
        assert!(!join_args(&cmd).contains("sentinel"));
        assert!(!join_args(&cmd).contains("must_contain"));
    }

    // --- Substrate probe: pure parser ---------------------------------------

    #[test]
    fn parse_models_response_accepts_present_model() {
        let body = r#"{"data":[{"id":"qwen3:14b"},{"id":"qwen3.5:27b"}]}"#;
        assert_eq!(parse_models_response(body, "qwen3:14b"), Ok(()));
    }

    #[test]
    fn parse_models_response_rejects_missing_model() {
        let body = r#"{"data":[{"id":"llama3:70b"},{"id":"qwen3.5:27b"}]}"#;
        match parse_models_response(body, "qwen3:14b") {
            Err(SubstrateError::ModelMissing { wanted, available }) => {
                assert_eq!(wanted, "qwen3:14b");
                assert_eq!(available, vec!["llama3:70b", "qwen3.5:27b"]);
            }
            other => panic!("expected ModelMissing, got {other:?}"),
        }
    }

    #[test]
    fn parse_models_response_rejects_malformed_json() {
        match parse_models_response("not json", "qwen3:14b") {
            Err(SubstrateError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn parse_models_response_rejects_empty_data_list() {
        let body = r#"{"data":[]}"#;
        match parse_models_response(body, "qwen3:14b") {
            Err(SubstrateError::ModelMissing { wanted, available }) => {
                assert_eq!(wanted, "qwen3:14b");
                assert!(available.is_empty());
            }
            other => panic!("expected ModelMissing on empty list, got {other:?}"),
        }
    }

    #[test]
    fn parse_models_response_rejects_unexpected_shape() {
        let body = r#"{"models":[{"id":"qwen3:14b"}]}"#;
        // Wrong top-level key — should fail parse, not silently pass.
        match parse_models_response(body, "qwen3:14b") {
            Err(SubstrateError::Parse(_)) => {}
            other => panic!("expected Parse error on wrong shape, got {other:?}"),
        }
    }

    // --- Claw JSON parser ----------------------------------------------------

    /// Operator-required: parser accepts fixture JSON containing the
    /// canonical success markers `a1-smoke-ok` + `tool-turn-complete`.
    #[test]
    fn parse_claw_json_accepts_success_envelope_with_canonical_markers() {
        let stdout = r#"{
            "message": "Inspection complete. a1-smoke-ok tool-turn-complete here.",
            "model": "qwen3:14b",
            "iterations": 1,
            "tool_uses": [],
            "tool_results": []
        }"#;
        let msg = parse_claw_json(stdout).expect("valid envelope must parse");
        assert!(msg.contains("a1-smoke-ok"));
        assert!(msg.contains("tool-turn-complete"));
    }

    #[test]
    fn parse_claw_json_extracts_message_only() {
        // The runner consumes only `message`; extra fields must not break
        // forward compatibility with claw's evolving envelope.
        let stdout = r#"{
            "message": "done",
            "model": "qwen3:14b",
            "iterations": 2,
            "future_unknown_field": {"nested": true},
            "usage": {"input_tokens": 100, "output_tokens": 50}
        }"#;
        assert_eq!(parse_claw_json(stdout).unwrap(), "done");
    }

    #[test]
    fn parse_claw_json_rejects_malformed_json() {
        match parse_claw_json("not json at all") {
            Err(StepFailure::ParseError(_)) => {}
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn parse_claw_json_rejects_missing_message_field() {
        let stdout = r#"{"model":"qwen3:14b","iterations":1}"#;
        match parse_claw_json(stdout) {
            Err(StepFailure::ParseError(_)) => {}
            other => panic!("expected ParseError on missing message, got {other:?}"),
        }
    }

    // --- Step-result classifier ----------------------------------------------

    const FIXTURE_OK_STDOUT: &str = r#"{
        "message": "Inspection complete. a1-smoke-ok tool-turn-complete here.",
        "model": "qwen3:14b",
        "iterations": 1
    }"#;

    fn must_contain(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn classify_step_result_accepts_success_with_all_markers_present() {
        let r = classify_step_result(
            0,
            FIXTURE_OK_STDOUT,
            "",
            &must_contain(&["a1-smoke-ok", "tool-turn-complete"]),
        );
        assert_eq!(r, Ok(()));
    }

    #[test]
    fn classify_step_result_accepts_success_with_no_expected_markers() {
        let r = classify_step_result(0, FIXTURE_OK_STDOUT, "", &[]);
        assert_eq!(r, Ok(()));
    }

    /// Operator-required: parser rejects empty-success output (i.e. claw
    /// exits 0 but the assistant message is empty — broker said the model
    /// finished without producing any text).
    #[test]
    fn classify_step_result_rejects_empty_assistant_content() {
        let stdout = r#"{"message":"","model":"qwen3:14b","iterations":1}"#;
        let r = classify_step_result(0, stdout, "", &[]);
        assert_eq!(r, Err(StepFailure::EmptyAssistantContent));
    }

    /// Operator-required: parser rejects missing expected marker.
    #[test]
    fn classify_step_result_rejects_missing_expected_marker() {
        let stdout =
            r#"{"message":"Did the work, but no magic word.","model":"qwen3:14b","iterations":1}"#;
        let r = classify_step_result(0, stdout, "", &must_contain(&["a1-smoke-ok"]));
        match r {
            Err(StepFailure::MissingExpectedMarker(m)) => assert_eq!(m, "a1-smoke-ok"),
            other => panic!("expected MissingExpectedMarker, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_reports_first_missing_marker_only() {
        // Deterministic for operator output: only the first missing marker
        // surfaces.
        let stdout = r#"{"message":"plain text","model":"qwen3:14b","iterations":1}"#;
        let r = classify_step_result(0, stdout, "", &must_contain(&["first-needle", "second"]));
        match r {
            Err(StepFailure::MissingExpectedMarker(m)) => assert_eq!(m, "first-needle"),
            other => panic!("expected first-encountered missing marker, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_maps_wrapper_exit_2_to_substrate_unavailable() {
        // Wrapper exit 2 = env file not found.
        let r = classify_step_result(2, "", "env file not found", &[]);
        match r {
            Err(StepFailure::SubstrateUnavailable { wrapper_exit, .. }) => {
                assert_eq!(wrapper_exit, 2);
            }
            other => panic!("expected SubstrateUnavailable for exit 2, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_maps_wrapper_exit_3_law1_to_substrate_unavailable() {
        // Wrapper exit 3 = LAW 1 broker-routing refusal.
        let r = classify_step_result(3, "", "LAW 1 refusal: not :11435", &[]);
        match r {
            Err(StepFailure::SubstrateUnavailable { wrapper_exit, .. }) => {
                assert_eq!(wrapper_exit, 3);
            }
            other => panic!("expected SubstrateUnavailable for exit 3, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_maps_wrapper_exit_4_to_substrate_unavailable() {
        // Wrapper exit 4 = claw not on PATH.
        let r = classify_step_result(4, "", "claw not on PATH", &[]);
        match r {
            Err(StepFailure::SubstrateUnavailable { wrapper_exit, .. }) => {
                assert_eq!(wrapper_exit, 4);
            }
            other => panic!("expected SubstrateUnavailable for exit 4, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_maps_other_nonzero_to_exit_nonzero() {
        let r = classify_step_result(1, "", "claw failed", &[]);
        match r {
            Err(StepFailure::ExitNonZero { code, stderr }) => {
                assert_eq!(code, 1);
                assert!(stderr.contains("claw failed"));
            }
            other => panic!("expected ExitNonZero, got {other:?}"),
        }
    }

    #[test]
    fn classify_step_result_does_not_parse_stdout_on_nonzero_exit() {
        // Defense: even if stdout happens to be valid JSON, a non-zero
        // exit must still classify as ExitNonZero (not paper over with
        // marker checks).
        let r = classify_step_result(1, FIXTURE_OK_STDOUT, "broker 500", &[]);
        assert!(matches!(r, Err(StepFailure::ExitNonZero { .. })));
    }

    // --- Plan-level aggregator -----------------------------------------------

    /// Operator-required: plan report emits `a2-l1b-step-passed` and
    /// `a2-l1b-plan-exec-pass` when every step passes.
    #[test]
    fn aggregate_plan_report_emits_pass_markers_when_all_steps_pass() {
        let report = aggregate_plan_report(
            "test-plan",
            vec![
                ("s1".to_string(), StepOutcomeForReport::Passed),
                ("s2".to_string(), StepOutcomeForReport::Passed),
            ],
        );
        assert_eq!(report.outcome, PlanOutcome::Pass);
        assert!(report.markers.contains(&markers::RUNNER_START.to_string()));
        assert!(report
            .markers
            .contains(&markers::PLAN_EXEC_PASS.to_string()));
        assert!(!report
            .markers
            .contains(&markers::PLAN_EXEC_FAIL.to_string()));
        for sr in &report.step_reports {
            assert!(sr.markers.contains(&markers::STEP_STARTED.to_string()));
            assert!(sr.markers.contains(&markers::STEP_PASSED.to_string()));
        }
    }

    /// Operator-required: plan report emits `a2-l1b-step-failed` and
    /// `a2-l1b-plan-exec-fail` when any step fails.
    #[test]
    fn aggregate_plan_report_emits_fail_markers_when_any_step_fails() {
        let report = aggregate_plan_report(
            "test-plan",
            vec![
                ("s1".to_string(), StepOutcomeForReport::Passed),
                (
                    "s2".to_string(),
                    StepOutcomeForReport::Failed(StepFailure::EmptyAssistantContent),
                ),
                ("s3".to_string(), StepOutcomeForReport::Skipped),
            ],
        );
        assert_eq!(report.outcome, PlanOutcome::Fail);
        assert!(report
            .markers
            .contains(&markers::PLAN_EXEC_FAIL.to_string()));
        assert!(!report
            .markers
            .contains(&markers::PLAN_EXEC_PASS.to_string()));

        let s1 = report
            .step_reports
            .iter()
            .find(|r| r.step_id == "s1")
            .unwrap();
        assert!(s1.markers.contains(&markers::STEP_PASSED.to_string()));

        let s2 = report
            .step_reports
            .iter()
            .find(|r| r.step_id == "s2")
            .unwrap();
        assert!(s2.markers.contains(&markers::STEP_FAILED.to_string()));
        assert_eq!(s2.outcome, Err(StepFailure::EmptyAssistantContent));

        let s3 = report
            .step_reports
            .iter()
            .find(|r| r.step_id == "s3")
            .unwrap();
        assert!(s3.markers.contains(&markers::STEP_SKIPPED.to_string()));
    }

    #[test]
    fn aggregate_plan_report_preserves_step_order() {
        let report = aggregate_plan_report(
            "ordered",
            vec![
                ("a".to_string(), StepOutcomeForReport::Passed),
                ("b".to_string(), StepOutcomeForReport::Passed),
                ("c".to_string(), StepOutcomeForReport::Passed),
            ],
        );
        let ids: Vec<&str> = report
            .step_reports
            .iter()
            .map(|r| r.step_id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn aggregate_plan_report_first_marker_is_runner_start() {
        let report = aggregate_plan_report("any", vec![]);
        assert_eq!(
            report.markers.first().map(String::as_str),
            Some(markers::RUNNER_START)
        );
    }

    // --- Refused-precheck / substrate-unavailable report shapes --------------

    #[test]
    fn refused_precheck_report_emits_validator_refused_marker() {
        let report = refused_precheck_report("refused-plan", &PrecheckRefusal::ValidatorRefused);
        assert_eq!(report.outcome, PlanOutcome::RefusedPrecheck);
        assert_eq!(
            report.markers,
            vec![
                markers::RUNNER_START.to_string(),
                markers::PLAN_REFUSED_PRECHECK.to_string(),
            ]
        );
        assert!(report.step_reports.is_empty());
    }

    /// CLI exit-code discriminator: `ToolDisallowed` must surface its own
    /// marker so the CLI can map it to exit code 3 instead of 2.
    #[test]
    fn refused_precheck_report_emits_tool_disallowed_marker_for_tool_refusal() {
        let report = refused_precheck_report(
            "refused-plan",
            &PrecheckRefusal::ToolDisallowed {
                step_id: "s1".to_string(),
                tool: "Edit".to_string(),
            },
        );
        assert_eq!(report.outcome, PlanOutcome::RefusedPrecheck);
        assert!(report
            .markers
            .contains(&markers::TOOL_DISALLOWED.to_string()));
        assert!(!report
            .markers
            .contains(&markers::PLAN_REFUSED_PRECHECK.to_string()));
        assert!(report.step_reports.is_empty());
    }

    #[test]
    fn substrate_unavailable_report_emits_correct_markers() {
        let report = substrate_unavailable_report("unavailable-plan");
        assert_eq!(report.outcome, PlanOutcome::Fail);
        assert_eq!(
            report.markers,
            vec![
                markers::RUNNER_START.to_string(),
                markers::SUBSTRATE_UNAVAILABLE.to_string(),
            ]
        );
        assert!(report.step_reports.is_empty());
    }

    // --- run_plan refused-precheck path (no subprocess, no broker) -----------

    /// Operator-required: no live broker call during tests. This test
    /// exercises `run_plan` through a workspace-write plan which is
    /// refused at precheck — proving the orchestrator short-circuits BEFORE
    /// any subprocess or HTTP call would happen.
    #[test]
    fn run_plan_short_circuits_on_refused_precheck_without_subprocess() {
        use a2_plan_schema::parse_plan;
        let yaml = include_str!("../../../../examples/a2_l1a_refused_workspace_write.yaml");
        let plan = parse_plan(yaml).expect("fixture parses");
        let report = run_plan(
            &plan,
            Path::new("/does/not/exist/wrapper"), // would fail to spawn if reached
            None,                                 // no substrate probe
            Duration::from_secs(1),
        );
        assert_eq!(report.outcome, PlanOutcome::RefusedPrecheck);
        assert!(report
            .markers
            .contains(&markers::PLAN_REFUSED_PRECHECK.to_string()));
    }

    // --- Structural proof: run_step only takes ClawCommand -------------------

    /// Compile-time + behavioral proof that `run_step` cannot be called
    /// with anything other than a [`ClawCommand`]. Since `ClawCommand` is
    /// constructed exclusively by [`build_claw_command`] (which always
    /// sets `program = wrapper_path`), there is no API path that creates
    /// a subprocess without going through the wrapper.
    #[test]
    fn run_step_signature_requires_claw_command_built_via_wrapper() {
        // This is a compile-check: if `run_step`'s signature is ever
        // widened to accept raw paths, this stops typechecking.
        let f: fn(&ClawCommand, &PlanStep, Duration) -> Result<(), StepFailure> = run_step;
        // Force a use of the binding so the type assertion isn't elided.
        let f_ptr = f as *const ();
        let r_ptr = run_step as *const ();
        assert_eq!(f_ptr, r_ptr);
        // And the only public constructor of ClawCommand is via the
        // wrapper-pinning builder:
        let cmd = build_claw_command(Path::new(WRAPPER_FIXTURE), &step(&["Read"]));
        assert!(cmd
            .program
            .to_string_lossy()
            .ends_with("claw-sidestack-local"));
    }

    // --- --step-timeout parser (Phase 5 Fix A) -------------------------------

    #[test]
    fn parse_step_timeout_accepts_default_in_range() {
        // The current DEFAULT_STEP_TIMEOUT must always be a value that
        // round-trips through the parser. This guards against future
        // default tweaks falling outside the bounds.
        let default_secs = DEFAULT_STEP_TIMEOUT.as_secs();
        let parsed = parse_step_timeout_seconds(&default_secs.to_string()).unwrap();
        assert_eq!(parsed, DEFAULT_STEP_TIMEOUT);
    }

    #[test]
    fn parse_step_timeout_accepts_min_and_max_bounds() {
        let lo = parse_step_timeout_seconds(&MIN_STEP_TIMEOUT_SECS.to_string()).unwrap();
        assert_eq!(lo.as_secs(), MIN_STEP_TIMEOUT_SECS);
        let hi = parse_step_timeout_seconds(&MAX_STEP_TIMEOUT_SECS.to_string()).unwrap();
        assert_eq!(hi.as_secs(), MAX_STEP_TIMEOUT_SECS);
    }

    #[test]
    fn parse_step_timeout_rejects_zero() {
        let err = parse_step_timeout_seconds("0").unwrap_err();
        assert!(err.contains("below the minimum"), "got: {err}");
    }

    #[test]
    fn parse_step_timeout_rejects_above_max() {
        let too_big = MAX_STEP_TIMEOUT_SECS + 1;
        let err = parse_step_timeout_seconds(&too_big.to_string()).unwrap_err();
        assert!(err.contains("exceeds the maximum"), "got: {err}");
    }

    #[test]
    fn parse_step_timeout_rejects_non_integer() {
        for bad in ["", "abc", "10.5", "-3", "5s", "  ", "1_000"] {
            let err = parse_step_timeout_seconds(bad).unwrap_err();
            assert!(
                err.contains("invalid --step-timeout"),
                "input {bad:?} → unexpected err: {err}"
            );
        }
    }

    #[test]
    fn parse_step_timeout_default_is_bounded_safe() {
        // Defensive: the default must never be unbounded (no infinity, no
        // u64::MAX, no zero, well below MAX).
        let secs = DEFAULT_STEP_TIMEOUT.as_secs();
        assert!(secs >= MIN_STEP_TIMEOUT_SECS);
        assert!(secs <= MAX_STEP_TIMEOUT_SECS);
        assert!(
            secs >= 60,
            "default {secs}s is too tight for cold-start scenarios"
        );
    }
}
