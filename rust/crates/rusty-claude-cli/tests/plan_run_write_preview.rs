//! Integration tests for `claw plan run --workspace-write-preview`.
//!
//! Test boundaries:
//!
//! - The runner's wrapper subprocess is never exercised. We either pass
//!   a `--wrapper` path that doesn't exist (read-only-only plans surface
//!   substrate-unavailable) or use a plan whose lone step is the
//!   workspace-write step (no read-only subprocess fires).
//! - No live broker call: substrate URL is either omitted (None path on
//!   single-write plan) or pointed at an unreachable localhost port,
//!   surfacing substrate-unavailable BEFORE the broker is touched.
//! - Preview-only contract: the target file is NEVER created and the
//!   after-file is NEVER mutated.
//! - The CLI must NEVER accept `--yes` / `--auto` / `--preapproved` /
//!   `--batch` / `--allow-write` / `--force`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("a2-cli-runplan-wp-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("temp dir created");
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

const SINGLE_WORKSPACE_WRITE_PLAN: &str = r"
name: single-write
mode: read-only
model_tier: FAST
steps:
  - id: write-it
    description: write a scratch note
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/scratch.md
      create_if_absent: true
    after_file: materialized/notes_scratch.after
";

const TWO_WRITE_PLAN: &str = r"
name: multi-write
mode: read-only
model_tier: FAST
steps:
  - id: a
    description: write a
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/a.md
      create_if_absent: true
    after_file: materialized/a.after
  - id: b
    description: write b
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/b.md
      create_if_absent: true
    after_file: materialized/b.after
";

fn seed_after(ws: &Path, rel: &str, content: &[u8]) {
    let path = ws.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
}

// --- Read-only-only path is unchanged when --workspace-write-preview is OFF -

#[test]
fn workspace_write_plan_without_opt_in_still_refuses_at_precheck() {
    // Hard contract: without --workspace-write-preview, workspace-write
    // plans refuse via existing L1b precheck (exit 3 — TOOL_DISALLOWED
    // for Write on a workspace-write step).
    let dir = unique_temp_dir("no-opt-in");
    let plan = write_plan(&dir, "p", SINGLE_WORKSPACE_WRITE_PLAN);
    let output = run_claw_plan(&["plan", "run", plan.to_str().unwrap(), "--dry-run"]);
    // Without the opt-in, the existing precheck fires before any
    // subprocess (Write is outside the read-only allowlist), giving 3.
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a2-l1b-tool-disallowed"));
    fs::remove_dir_all(&dir).ok();
}

// --- CLI flag rejection: pre-approval / autonomous-write inputs are refused

#[test]
fn plan_run_rejects_yes_flag() {
    let output = run_claw_plan(&["plan", "run", "--yes", "any.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not supported"));
}

#[test]
fn plan_run_rejects_auto_flag() {
    let output = run_claw_plan(&["plan", "run", "--auto", "any.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not supported"));
}

#[test]
fn plan_run_rejects_preapproved_flag() {
    let output = run_claw_plan(&["plan", "run", "--preapproved", "any.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not supported"));
}

#[test]
fn plan_run_rejects_batch_flag() {
    let output = run_claw_plan(&["plan", "run", "--batch", "any.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not supported"));
}

#[test]
fn plan_run_rejects_workspace_write_preview_combined_with_dry_run() {
    let output = run_claw_plan(&[
        "plan",
        "run",
        "--workspace-write-preview",
        "--dry-run",
        "any.yaml",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be combined"));
}

#[test]
fn plan_run_rejects_workspace_root_without_opt_in() {
    let output = run_claw_plan(&["plan", "run", "--workspace-root", "/tmp", "any.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--workspace-write-preview"));
}

// --- Opt-in happy path: produces preview bundle, halts pre-approval ---------

#[test]
fn workspace_write_preview_opt_in_produces_bundle_and_exits_seven() {
    let dir = unique_temp_dir("opt-in-happy");
    let plan = write_plan(&dir, "p", SINGLE_WORKSPACE_WRITE_PLAN);
    fs::create_dir_all(dir.join("notes")).unwrap();
    seed_after(&dir, "materialized/notes_scratch.after", b"hello\nworld\n");

    let output = run_claw_plan(&[
        "plan",
        "run",
        plan.to_str().unwrap(),
        "--workspace-write-preview",
        "--workspace-root",
        dir.to_str().unwrap(),
        // Pass a missing wrapper deliberately: the lone write step never
        // spawns the wrapper. If the runner ever wires workspace-write
        // through the subprocess path, this test will fail loudly.
        "--wrapper",
        "/does/not/exist/claw-sidestack-local",
        "--substrate-url",
        "http://127.0.0.1:1/v1",
        "--fast-model",
        "qwen3:14b",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(7),
        "write-preview-ready must exit 7; stderr={stderr}, stdout={stdout}"
    );
    assert!(stdout.contains("a2-l2b-run-plan-write-preview-ready"));
    assert!(stdout.contains("a2-l2b-approval-pending"));
    assert!(stdout.contains("a2-l2b-plan-halted"));
    assert!(stdout.contains("preview_bundle_path:"));
    assert!(stdout.contains("payload_path:"));
    assert!(stdout.contains("run_manifest_path:"));
    assert!(stdout.contains("next_operator_command: claw plan approve "));

    // Target was NOT created.
    assert!(
        !dir.join("notes/scratch.md").exists(),
        "preview must not mutate the target file"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_write_preview_json_report_emits_authoritative_fields() {
    let dir = unique_temp_dir("opt-in-json");
    let plan = write_plan(&dir, "p", SINGLE_WORKSPACE_WRITE_PLAN);
    fs::create_dir_all(dir.join("notes")).unwrap();
    seed_after(&dir, "materialized/notes_scratch.after", b"x\n");

    let output = run_claw_plan(&[
        "plan",
        "run",
        plan.to_str().unwrap(),
        "--workspace-write-preview",
        "--workspace-root",
        dir.to_str().unwrap(),
        "--wrapper",
        "/does/not/exist/claw-sidestack-local",
        "--report-format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(7));
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");
    assert_eq!(parsed["status"], "write_preview_ready");
    assert_eq!(parsed["write_step_count"], 1);
    assert_eq!(parsed["exit_code_hint"], 7);
    assert_eq!(
        parsed["schema_version"],
        "a2-l2b-run-plan-write-preview-report.v1"
    );
    let artifacts = &parsed["preview_artifacts"];
    assert!(artifacts.is_object());
    assert!(artifacts["preview_bundle_path"].is_string());
    assert!(artifacts["payload_path"].is_string());
    let next_cmd = parsed["next_operator_command"].as_str().unwrap();
    assert!(next_cmd.starts_with("claw plan approve "));
    assert!(!next_cmd.contains("plan apply"));

    // Target was NOT created.
    assert!(!dir.join("notes/scratch.md").exists());
    fs::remove_dir_all(&dir).ok();
}

// --- Multi-write refusal under opt-in ---------------------------------------

#[test]
fn workspace_write_preview_opt_in_refuses_multi_write_plan_before_execution() {
    let dir = unique_temp_dir("multi-write-cli");
    let plan = write_plan(&dir, "p", TWO_WRITE_PLAN);
    fs::create_dir_all(dir.join("notes")).unwrap();
    seed_after(&dir, "materialized/a.after", b"a\n");
    seed_after(&dir, "materialized/b.after", b"b\n");

    let output = run_claw_plan(&[
        "plan",
        "run",
        plan.to_str().unwrap(),
        "--workspace-write-preview",
        "--workspace-root",
        dir.to_str().unwrap(),
        "--wrapper",
        "/does/not/exist/claw-sidestack-local",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(5),
        "multi-write refusal exit code mismatch; stderr={stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a2-l2b-plan-multi-write-refused"));
    // No write artifacts produced.
    assert!(!dir.join(".claw/l2b-preview-bundles").exists());
    assert!(!dir.join(".claw/l2b-payloads").exists());
    // Neither target created.
    assert!(!dir.join("notes/a.md").exists());
    assert!(!dir.join("notes/b.md").exists());
    fs::remove_dir_all(&dir).ok();
}

// --- Runtime after_file refusal -------------------------------------------

#[test]
fn workspace_write_preview_opt_in_refuses_when_after_file_missing_on_disk() {
    let dir = unique_temp_dir("missing-after");
    let plan = write_plan(&dir, "p", SINGLE_WORKSPACE_WRITE_PLAN);
    fs::create_dir_all(dir.join("notes")).unwrap();
    // No materialized/notes_scratch.after on disk.

    let output = run_claw_plan(&[
        "plan",
        "run",
        plan.to_str().unwrap(),
        "--workspace-write-preview",
        "--workspace-root",
        dir.to_str().unwrap(),
        "--wrapper",
        "/does/not/exist/claw-sidestack-local",
    ]);
    let code = output.status.code();
    assert!(
        code == Some(5),
        "missing after_file should refuse with exit 5; got {code:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a2-l2b-plan-halted"));
    // Target NOT created.
    assert!(!dir.join("notes/scratch.md").exists());
    fs::remove_dir_all(&dir).ok();
}
