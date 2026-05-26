//! A2-L2b Slice 3d — integration tests for `claw plan approve <bundle>`.
//!
//! All tests are no-broker / no-network by construction. They invoke the
//! built `claw` binary as a subprocess and feed it preview bundles
//! constructed from the public `a2-plan-runner` API. No tests rely on a
//! real TTY (the binary's non-TTY guard surfaces a refusal, exit 7,
//! which the tests assert against).
//!
//! Required operator claims covered here:
//!   * `claw plan approve <bundle>` exists.
//!   * Missing-bundle, invalid-bundle, and integrity-broken bundles
//!     exit 5 with a structured `bundle_rejected` JSON envelope.
//!   * Non-TTY stdin on an approvable bundle refuses with exit 7.
//!   * No `--yes` / `--auto` / `--force` / `--allow-write` flag exists
//!     on the `approve` subcommand.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use a2_plan_runner::{
    canonical_preview_record_for_approval, preview_hash_from_parts, CanonicalSubset,
    PreviewDisplay, PreviewRecord, PREVIEW_FORMAT_VERSION,
};

const STEP: &str = "step-1";
const PREVIEW_ID: &str = "01HZZZZZZZZZZZZZZZZZZZZZZ0";
const RUN_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const BUNDLE_SCHEMA_V1: &str = "a2-l2b-preview-bundle.v1";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("a2-l2b-approve-it-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("tempdir create");
    dir
}

fn build_pair(
    is_binary: bool,
    is_redacted: bool,
    is_truncated: bool,
    rendered: &str,
) -> (PreviewRecord, PreviewDisplay) {
    let before_sha = "a".repeat(64);
    let after_sha = "b".repeat(64);
    let rel = "src/lib.rs".to_string();
    let subset = CanonicalSubset {
        preview_id: PREVIEW_ID,
        step_id: STEP,
        target_relative_path_sanitized: &rel,
        before_sha256: &before_sha,
        after_sha256: &after_sha,
        checkpoint_run_id: RUN_ID,
        checkpoint_step_id: STEP,
        is_binary,
        is_redacted,
        is_truncated,
        preview_format_version: PREVIEW_FORMAT_VERSION,
    };
    let canonical = canonical_preview_record_for_approval(&subset);
    let hash = preview_hash_from_parts(&canonical, rendered);
    let record = PreviewRecord {
        preview_id: PREVIEW_ID.to_string(),
        step_id: STEP.to_string(),
        target_relative_path_sanitized: rel,
        target_absolute_path_sanitized: "/ws/src/lib.rs".to_string(),
        before_sha256: before_sha,
        after_sha256: after_sha,
        preview_sha256: hash,
        checkpoint_run_id: RUN_ID.to_string(),
        checkpoint_step_id: STEP.to_string(),
        is_binary,
        is_redacted,
        is_truncated,
        created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
        preview_format_version: PREVIEW_FORMAT_VERSION,
    };
    let display = PreviewDisplay {
        rendered: rendered.to_string(),
    };
    (record, display)
}

fn write_bundle(
    dir: &Path,
    record: &PreviewRecord,
    display: &PreviewDisplay,
    baseline_unchanged: bool,
) -> PathBuf {
    let bundle = serde_json::json!({
        "schema_version": BUNDLE_SCHEMA_V1,
        "preview_record": record,
        "preview_display": display,
        "checkpoint_baseline_unchanged": baseline_unchanged,
    });
    let path = dir.join("bundle.json");
    fs::write(
        &path,
        serde_json::to_vec(&bundle).expect("serialize bundle"),
    )
    .expect("write bundle");
    path
}

/// Invoke `claw plan approve <path>` with the given stdin bytes. The
/// stdin is a non-TTY pipe; the binary's non-TTY guard refuses
/// approvable bundles outright (exit 7). Use this helper for tests
/// that exercise bundle-load failures, refusal envelopes, and the
/// parse/usage surface — NOT the happy path (which requires a real
/// TTY and is covered by the inline unit tests).
fn run_claw_approve(bundle_path: &Path, stdin_bytes: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", &bundle_path.to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("claw should launch");
    {
        let stdin = child.stdin.as_mut().expect("child stdin pipe");
        stdin.write_all(stdin_bytes).expect("write stdin");
    }
    child.wait_with_output().expect("claw should exit")
}

fn parse_stdout_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
}

// -- Required claim 1: `claw plan approve <bundle>` exists --------------

#[test]
fn plan_approve_command_exists_missing_bundle_arg_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve"])
        .output()
        .expect("claw should launch");
    assert!(
        !out.status.success(),
        "missing bundle path must not exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("missing preview bundle"),
        "expected usage error mentioning missing bundle; stderr={stderr}"
    );
}

#[test]
fn plan_approve_command_rejects_yes_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", "--yes", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--yes"),
        "approve must reject --yes; stderr={stderr}"
    );
}

#[test]
fn plan_approve_command_rejects_auto_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", "--auto", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--auto"),
        "approve must reject --auto; stderr={stderr}"
    );
}

#[test]
fn plan_approve_command_rejects_force_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", "--force", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--force"),
        "approve must reject --force; stderr={stderr}"
    );
}

#[test]
fn plan_approve_command_rejects_allow_write_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", "--allow-write", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--allow-write"),
        "approve must reject --allow-write; stderr={stderr}"
    );
}

// -- Required claim 2: bundle load failures exit 5 ----------------------

#[test]
fn plan_approve_missing_bundle_file_exits_five() {
    let dir = unique_temp_dir();
    let missing = dir.join("does-not-exist.json");
    let out = run_claw_approve(&missing, b"");
    assert_eq!(
        out.status.code(),
        Some(5),
        "missing bundle file must exit 5; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "bundle_rejected");
    assert_eq!(json["exit_code_hint"], 5);
}

#[test]
fn plan_approve_invalid_bundle_json_exits_five() {
    let dir = unique_temp_dir();
    let path = dir.join("broken.json");
    fs::write(&path, b"{not json").unwrap();
    let out = run_claw_approve(&path, b"");
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-json-parse-error"));
}

#[test]
fn plan_approve_wrong_schema_version_exits_five() {
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(false, false, false, "preview\n");
    let bundle = serde_json::json!({
        "schema_version": "a2-l2b-preview-bundle.v999",
        "preview_record": rec,
        "preview_display": disp,
        "checkpoint_baseline_unchanged": true,
    });
    let path = dir.join("bundle.json");
    fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let out = run_claw_approve(&path, b"");
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-schema-version-mismatch"));
}

#[test]
fn plan_approve_record_display_binding_mismatch_exits_five() {
    let dir = unique_temp_dir();
    let (rec, _disp) = build_pair(false, false, false, "preview\n");
    let tampered = PreviewDisplay {
        rendered: "tampered bytes that change the hash\n".to_string(),
    };
    let path = write_bundle(&dir, &rec, &tampered, true);
    let out = run_claw_approve(&path, b"");
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "bundle_rejected");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "bundle-record-display-binding-mismatch"
    );
}

#[test]
fn plan_approve_smuggled_approval_decision_field_exits_five() {
    // `serde(deny_unknown_fields)` MUST reject any bundle that tries to
    // smuggle a trusted approval into the binary. The CLI never reads
    // an embedded approval and must surface a parse error before any
    // approval interaction.
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(false, false, false, "preview\n");
    let bundle = serde_json::json!({
        "schema_version": BUNDLE_SCHEMA_V1,
        "preview_record": rec,
        "preview_display": disp,
        "checkpoint_baseline_unchanged": true,
        "approval_decision": "approved",
    });
    let path = dir.join("bundle.json");
    fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let out = run_claw_approve(&path, b"");
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "bundle_rejected");
}

// -- Required claim 3: non-TTY stdin on approvable bundle refuses -------

#[test]
fn plan_approve_non_tty_stdin_for_approvable_bundle_refuses_exit_seven() {
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(false, false, false, "approvable preview\n");
    let path = write_bundle(&dir, &rec, &disp, true);
    // Subprocess stdin is a pipe, never a TTY. The binary's non-TTY
    // guard MUST short-circuit before reading any byte.
    let out = run_claw_approve(&path, b"");
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "refused");
    assert_eq!(json["reason"].as_str().unwrap(), "approval-stdin-not-tty");
}

// -- Required claim 4: non-approvable bundles refuse without reading ----

#[test]
fn plan_approve_binary_bundle_refuses_without_reading_stdin() {
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(true, false, false, "binary placeholder\n");
    let path = write_bundle(&dir, &rec, &disp, true);
    // Pass an approval-shaped string to prove the binary IGNORES it.
    let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
    let out = run_claw_approve(&path, approval.as_bytes());
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "refused");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "preview-binary-non-approvable"
    );
}

#[test]
fn plan_approve_redacted_bundle_refuses_without_reading_stdin() {
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(false, true, false, "redacted placeholder\n");
    let path = write_bundle(&dir, &rec, &disp, true);
    let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
    let out = run_claw_approve(&path, approval.as_bytes());
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "refused");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "preview-redacted-non-approvable"
    );
}

#[test]
fn plan_approve_truncated_bundle_refuses_without_reading_stdin() {
    let dir = unique_temp_dir();
    let (rec, disp) = build_pair(false, false, true, "truncated placeholder\n");
    let path = write_bundle(&dir, &rec, &disp, true);
    let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
    let out = run_claw_approve(&path, approval.as_bytes());
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["decision"], "refused");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "preview-truncated-non-approvable"
    );
}

// -- Required claim 5: stdout is a single JSON line ---------------------

#[test]
fn plan_approve_stdout_is_single_json_envelope() {
    let dir = unique_temp_dir();
    let missing = dir.join("does-not-exist.json");
    let out = run_claw_approve(&missing, b"");
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = text.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "stdout must be a single JSON line, got: {text:?}"
    );
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("stdout is one JSON value");
}
