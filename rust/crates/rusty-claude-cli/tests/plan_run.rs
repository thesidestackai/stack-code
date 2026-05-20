//! Integration tests for the additive `claw plan run <file>` entry point.
//!
//! All tests are no-live-broker by construction:
//!   - `--dry-run` paths exercise validator + precheck + report writers only,
//!     never spawning the wrapper.
//!   - Refused-plan fixtures short-circuit inside `run_plan` BEFORE the
//!     wrapper-subprocess code path is reached.
//!
//! Operator-required tests (Phase 4):
//!   1. `claw plan run <file>` exists (typo / missing-arg produces helpful errors).
//!   2. Valid plan path reaches a2-plan-runner (proven by marker stream).
//!   3. Refused plan exits non-zero (workspace-write → 2, disallowed-tool → 3).
//!   4. No live broker call in CLI tests (`--substrate-url` never passed;
//!      `--dry-run` is used or refused plans short-circuit).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("a2-plan-run-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("temp dir should exist");
    dir
}

fn write_plan(dir: &Path, name: &str, yaml: &str) -> PathBuf {
    let path = dir.join(format!("{name}.yaml"));
    fs::write(&path, yaml).expect("write plan fixture");
    path
}

fn run_claw_plan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(args)
        .output()
        .expect("claw should launch")
}

const VALID_READONLY_PLAN: &str = r"
name: readonly-discovery
mode: read-only
model_tier: FAST
steps:
  - id: locate
    description: locate the readme
    tools: [Read]
";

const WORKSPACE_WRITE_PLAN: &str = r"
name: attempted-edit
mode: read-only
model_tier: FAST
steps:
  - id: edit
    description: would edit a file
    mode: workspace-write
    tools: [Edit]
";

const DEEP_TIER_PLAN: &str = r"
name: attempted-deep
mode: read-only
model_tier: FAST
steps:
  - id: deep
    description: requests DEEP
    model_tier: DEEP
    tools: [Read]
";

const DISALLOWED_TOOL_PLAN: &str = r"
name: disallowed-tool-via-l1b
mode: read-only
model_tier: FAST
steps:
  - id: edit-attempt
    description: tries to use Edit through L1b
    tools: [Edit]
";

// --- 1. `claw plan run <file>` exists + helpful errors -----------------------

#[test]
fn bare_claw_plan_preserves_existing_slash_command_guidance() {
    // Hard rule #2 regression guard: `/plan` was an existing slash command
    // BEFORE the L1b additive subcommand landed. Bare `claw plan` must
    // still surface that guidance — the new `plan run` form lives one
    // level deeper and does not steal bare-word dispatch.
    let output = run_claw_plan(&["plan"]);
    assert!(!output.status.success(), "bare `claw plan` still errors");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("slash command"),
        "bare `claw plan` must still route to slash-command guidance, got: {stderr}"
    );
}

#[test]
fn plan_run_without_file_errors_with_usage() {
    let output = run_claw_plan(&["plan", "run"]);
    assert!(!output.status.success(), "missing file must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing plan file"),
        "error should mention missing plan file, got: {stderr}"
    );
}

#[test]
fn plan_run_rejects_unsupported_elevation_flags() {
    // Hard rule: --allow-write / --force must be rejected at parse time
    // BEFORE any plan file is loaded — proves the runner-pinned contract
    // is enforced at the CLI boundary too.
    for bad in ["--allow-write", "--force"] {
        let output = run_claw_plan(&["plan", "run", bad, "any.yaml"]);
        assert!(!output.status.success(), "{bad} must be rejected");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not supported"),
            "{bad} rejection should explain why, got: {stderr}"
        );
    }
}

// --- 2. Valid plan path reaches a2-plan-runner -------------------------------

#[test]
fn plan_run_dry_run_on_valid_plan_emits_runner_markers_and_exits_zero() {
    let dir = unique_temp_dir("dry-run-pass");
    let plan = write_plan(&dir, "valid", VALID_READONLY_PLAN);

    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap(), "--dry-run"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "valid plan dry-run should exit 0; stderr={stderr}"
    );
    assert!(
        stdout.contains("a2-l1b-runner-start"),
        "stdout missing runner-start marker: {stdout}"
    );
    assert!(
        stdout.contains("a2-l1b-plan-exec-pass"),
        "stdout missing pass marker: {stdout}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn plan_run_dry_run_json_format_emits_structured_report() {
    let dir = unique_temp_dir("dry-run-json");
    let plan = write_plan(&dir, "valid", VALID_READONLY_PLAN);

    let output = run_claw_plan(&[
        "plan",
        "run",
        plan.to_str().unwrap(),
        "--dry-run",
        "--report-format",
        "json",
    ]);

    assert!(output.status.success(), "valid dry-run must exit 0");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");
    assert_eq!(parsed["plan_name"], "readonly-discovery");
    assert_eq!(parsed["outcome"], "pass");

    fs::remove_dir_all(&dir).ok();
}

// --- 3. Refused plan exits non-zero ------------------------------------------

#[test]
fn plan_run_on_workspace_write_plan_exits_two() {
    let dir = unique_temp_dir("ws-write");
    let plan = write_plan(&dir, "ws-write", WORKSPACE_WRITE_PLAN);

    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap(), "--dry-run"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "workspace-write plan must exit 2; got status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a2-l1b-plan-refused-precheck"),
        "stdout missing refusal marker: {stdout}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn plan_run_on_deep_tier_plan_exits_two() {
    let dir = unique_temp_dir("deep");
    let plan = write_plan(&dir, "deep", DEEP_TIER_PLAN);

    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap(), "--dry-run"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "DEEP plan must exit 2; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn plan_run_on_disallowed_tool_plan_exits_three() {
    // L1a-valid (read-only + FAST + non-empty tools) but L1b-refused
    // because Edit is outside the runner allowlist → exit 3.
    let dir = unique_temp_dir("disallowed");
    let plan = write_plan(&dir, "disallowed", DISALLOWED_TOOL_PLAN);

    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap(), "--dry-run"]);

    assert_eq!(
        output.status.code(),
        Some(3),
        "disallowed-tool plan must exit 3; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a2-l1b-tool-disallowed"),
        "stdout missing tool-disallowed marker: {stdout}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn plan_run_on_missing_file_exits_five() {
    let output = run_claw_plan(&["plan", "run", "/does/not/exist/plan.yaml"]);
    assert_eq!(
        output.status.code(),
        Some(5),
        "missing file must exit 5 (parse-error path)"
    );
}

#[test]
fn plan_run_on_malformed_yaml_exits_five() {
    let dir = unique_temp_dir("malformed");
    let plan = write_plan(&dir, "junk", "this is not yaml: -- broken {");
    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(5),
        "malformed YAML must exit 5; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(&dir).ok();
}

// --- 4. Regression guards: existing CLI behavior unchanged -------------------

#[test]
fn version_subcommand_still_works_after_plan_addition() {
    let output = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["version"])
        .output()
        .expect("claw version should launch");
    assert!(output.status.success());
}
