//! A2-L2d Read-Only Artifact Inspector / Status Contract — CLI
//! integration tests for `claw plan status <workspace> [<approval-result.json>]`.
//!
//! All tests are no-broker / no-network / no-subprocess-of-claw by
//! construction (the binary itself is the subprocess under test). The
//! fixtures hand-craft minimal L2b artifact trees under disposable
//! `tempdir`-style directories so we never run the real L2b chain.
//!
//! Required claims covered here:
//!   * `claw plan status` exists as a subcommand under `claw plan`.
//!   * One required positional (`<workspace>`) and one optional
//!     positional (`<approval-result.json>`).
//!   * Stdout is a valid `a2-l2d-status.v1` envelope on both success
//!     and read-time refusal.
//!   * Refusal sets `exit_code == EXIT_STATUS_REFUSED == 12`,
//!     `stop_condition` set, `next_operator_command == "STOP — escalate"`.
//!   * Optional approval-result positional arg participates in the
//!     state machine (phase advances to `approval_captured` when
//!     valid).
//!   * Refuses every write-adjacent flag (`--yes`, `--apply`, `--clean`,
//!     `--all-runs`, etc.).
//!   * Read-only invariant: status calls do not mutate `.claw/`.

#![cfg(unix)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const STATUS_SCHEMA_V1: &str = "a2-l2d-status.v1";
const EXIT_STATUS_REFUSED: i32 = 12;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "a2-l2d-status-cli-{}-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
        seq
    ));
    fs::create_dir_all(&dir).expect("tempdir create");
    dir.canonicalize().expect("tempdir canonicalize")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in &digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn parse_stdout_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
}

struct BuiltCliFixture {
    workspace: PathBuf,
    step_id: String,
    preview_sha: String,
}

/// Build a minimal happy-path L2b artifact tree under a fresh tempdir.
fn build_happy_workspace(label: &str) -> BuiltCliFixture {
    let workspace = unique_temp_dir(label);
    let run_id = format!("run-{label}");
    let step_id = format!("step-{label}");
    let target_rel = "notes/scratch.md";
    let before_bytes = b"baseline\n";
    let after_bytes = b"updated\n";
    let before_sha = sha256_hex(before_bytes);
    let after_sha = sha256_hex(after_bytes);
    let preview_sha = sha256_hex(format!("{before_sha}|{after_sha}").as_bytes());

    fs::create_dir_all(workspace.join("notes")).unwrap();
    fs::write(workspace.join(target_rel), before_bytes).unwrap();

    let run_dir = workspace.join(".claw").join("l2b-runs").join(&run_id);
    fs::create_dir_all(&run_dir).unwrap();
    let manifest = serde_json::json!({
        "schema_version": "a2-l2b-run-plan-write-preview-run-manifest.v1",
        "pending_step_id": step_id,
    });
    fs::write(
        run_dir.join("run-manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        run_dir.join("status.json"),
        r#"{"status":"write_preview_ready"}"#,
    )
    .unwrap();

    let target_abs = workspace.join(target_rel);
    let preview_record = serde_json::json!({
        "preview_id": format!("pid-{run_id}"),
        "step_id": step_id,
        "target_relative_path_sanitized": target_rel,
        "target_absolute_path_sanitized": target_abs.to_string_lossy(),
        "before_sha256": before_sha,
        "after_sha256": after_sha,
        "preview_sha256": preview_sha,
        "checkpoint_run_id": run_id,
        "checkpoint_step_id": step_id,
        "is_binary": false,
        "is_redacted": false,
        "is_truncated": false,
        "created_at_utc": "2026-05-29T15:00:00.000000000Z",
        "preview_format_version": 1u32,
    });

    let step_dir = workspace
        .join(".claw")
        .join("l2b-preview-bundles")
        .join(&run_id)
        .join(&step_id);
    fs::create_dir_all(&step_dir).unwrap();
    let preview_bundle = serde_json::json!({
        "schema_version": "a2-l2b-preview-bundle.v1",
        "preview_record": preview_record,
        "preview_display": { "rendered": "(test)" },
        "checkpoint_baseline_unchanged": true,
    });
    fs::write(
        step_dir.join("preview-bundle.json"),
        serde_json::to_string_pretty(&preview_bundle).unwrap(),
    )
    .unwrap();
    fs::write(
        step_dir.join("preview-generator-result.json"),
        r#"{"schema_version":"a2-l2b-preview-bundle-generator-result.v1","ok":true}"#,
    )
    .unwrap();

    let payload_dir = workspace
        .join(".claw")
        .join("l2b-payloads")
        .join(&run_id)
        .join(&step_id);
    fs::create_dir_all(&payload_dir).unwrap();
    fs::write(payload_dir.join("after.sha256"), format!("{after_sha}\n")).unwrap();

    let ck_dir = workspace
        .join(".claw")
        .join("l2b-checkpoints")
        .join(&run_id)
        .join(&step_id);
    fs::create_dir_all(&ck_dir).unwrap();
    fs::write(
        ck_dir.join("manifest.json"),
        r#"{"schema_version":"test-checkpoint-manifest"}"#,
    )
    .unwrap();

    BuiltCliFixture {
        workspace,
        step_id,
        preview_sha,
    }
}

fn run_claw_plan_status(args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut cmd_args = vec!["plan", "status"];
    cmd_args.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(&cmd_args)
        .output()
        .expect("claw should launch");
    (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
}

// --------------------------------------------------------------------------
// Happy-path tests: command exists and emits a valid envelope.
// --------------------------------------------------------------------------

#[test]
fn cli_no_args_refuses_with_usage() {
    let (code, _stdout, stderr) = run_claw_plan_status(&[]);
    assert_ne!(code, 0, "no-arg invocation must refuse");
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    assert!(
        stderr_text.contains("workspace") || stderr_text.contains("status"),
        "stderr must reference the workspace argument: {stderr_text}"
    );
}

#[test]
fn cli_no_run_workspace_emits_no_run_found_envelope() {
    let workspace = unique_temp_dir("cli-no-run");
    let (code, stdout, _) = run_claw_plan_status(&[&workspace.to_string_lossy()]);
    assert_eq!(code, 0, "no-run workspace must exit 0");
    let json = parse_stdout_json(&stdout);
    assert_eq!(json["schema_version"], STATUS_SCHEMA_V1);
    assert_eq!(json["phase"], "no_run_found");
    assert_eq!(
        json["next_operator_command"],
        "(no run found — start with claw plan run …)"
    );
    assert_eq!(
        json["read_only_invariant"],
        "this command does not mutate state"
    );
}

#[test]
fn cli_happy_workspace_emits_awaiting_approval_envelope() {
    let fx = build_happy_workspace("cli-happy");
    let (code, stdout, _) = run_claw_plan_status(&[&fx.workspace.to_string_lossy()]);
    assert_eq!(code, 0);
    let json = parse_stdout_json(&stdout);
    assert_eq!(json["schema_version"], STATUS_SCHEMA_V1);
    assert_eq!(json["phase"], "awaiting_approval");
    assert_eq!(json["is_approvable"], true);
    assert_eq!(json["is_apply_ready"], false);
    let cmd = json["next_operator_command"].as_str().unwrap();
    assert!(
        cmd.starts_with("claw plan approve "),
        "next_operator_command: {cmd}"
    );
}

#[test]
fn cli_with_approval_result_advances_to_approval_captured() {
    let fx = build_happy_workspace("cli-approval");
    let approval_dir = unique_temp_dir("cli-approval-apr");
    let approval_path = approval_dir.join("approval.json");
    let approval = serde_json::json!({
        "schema_version": "a2-l2b-approval-result.v1",
        "decision": "approved",
        "preview_id": "pid-test",
        "step_id": fx.step_id,
        "preview_sha256": fx.preview_sha,
    });
    fs::write(
        &approval_path,
        serde_json::to_string_pretty(&approval).unwrap(),
    )
    .unwrap();

    let (code, stdout, _) = run_claw_plan_status(&[
        &fx.workspace.to_string_lossy(),
        &approval_path.to_string_lossy(),
    ]);
    assert_eq!(code, 0);
    let json = parse_stdout_json(&stdout);
    assert_eq!(json["phase"], "approval_captured");
    let cmd = json["next_operator_command"].as_str().unwrap();
    assert!(
        cmd.starts_with("claw plan apply-bundle "),
        "next_operator_command: {cmd}"
    );
    let evidence = json["evidence_paths"].as_array().unwrap();
    let approval_str = approval_path.to_string_lossy().to_string();
    assert!(
        evidence
            .iter()
            .any(|v| v.as_str() == Some(approval_str.as_str())),
        "approval-result path must be in evidence_paths: {evidence:?}"
    );
}

// --------------------------------------------------------------------------
// Refusal envelope test.
// --------------------------------------------------------------------------

#[test]
fn cli_invalid_workspace_emits_refusal_envelope_with_exit_12() {
    let (code, stdout, _) = run_claw_plan_status(&["/this/path/does/not/exist/a2-l2d-cli"]);
    assert_eq!(code, EXIT_STATUS_REFUSED);
    let json = parse_stdout_json(&stdout);
    assert_eq!(json["schema_version"], STATUS_SCHEMA_V1);
    assert_eq!(json["stop_condition"], "workspace-root-invalid");
    assert_eq!(json["next_operator_command"], "STOP — escalate");
    assert_eq!(
        json["read_only_invariant"],
        "this command does not mutate state"
    );
}

// --------------------------------------------------------------------------
// Flag refusal tests.
// --------------------------------------------------------------------------

#[test]
fn cli_refuses_yes_flag() {
    let fx = build_happy_workspace("cli-yes");
    let (code, _stdout, stderr) = run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--yes"]);
    assert_ne!(code, 0);
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    assert!(stderr_text.contains("read-only") || stderr_text.contains("--yes"));
}

#[test]
fn cli_refuses_apply_flag() {
    let fx = build_happy_workspace("cli-apply");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--apply"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_auto_flag() {
    let fx = build_happy_workspace("cli-auto");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--auto"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_skip_approval_flag() {
    let fx = build_happy_workspace("cli-skip-approval");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--skip-approval"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_no_prompt_flag() {
    let fx = build_happy_workspace("cli-no-prompt");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--no-prompt"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_all_runs_flag() {
    let fx = build_happy_workspace("cli-all-runs");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--all-runs"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_clean_flag() {
    let fx = build_happy_workspace("cli-clean");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--clean"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_refuses_cache_flag() {
    let fx = build_happy_workspace("cli-cache");
    let (code, _stdout, _stderr) =
        run_claw_plan_status(&[&fx.workspace.to_string_lossy(), "--cache"]);
    assert_ne!(code, 0);
}

// --------------------------------------------------------------------------
// Read-only invariant test against the binary.
// --------------------------------------------------------------------------

fn snapshot_claw_tree_string(workspace: &Path) -> Vec<(String, u128, String)> {
    let mut out = Vec::new();
    let claw = workspace.join(".claw");
    if !claw.is_dir() {
        return out;
    }
    let mut stack: Vec<PathBuf> = vec![claw];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let mtime_nanos = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                let bytes = fs::read(&path).unwrap_or_default();
                out.push((
                    path.to_string_lossy().to_string(),
                    mtime_nanos,
                    sha256_hex(&bytes),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn cli_does_not_mutate_claw_tree() {
    let fx = build_happy_workspace("cli-readonly");
    let before = snapshot_claw_tree_string(&fx.workspace);
    let (code, _stdout, _stderr) = run_claw_plan_status(&[&fx.workspace.to_string_lossy()]);
    assert_eq!(code, 0);
    let after = snapshot_claw_tree_string(&fx.workspace);
    assert_eq!(
        before, after,
        "claw plan status must not mutate any file under .claw/"
    );
}
