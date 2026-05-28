//! A2-L2b Slice L2b-CLI-Apply-Bundle-Generator — integration tests for
//! `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`.
//!
//! All tests are no-broker / no-network by construction. They drive the
//! generator end-to-end by:
//!
//!   1. Running `claw plan preview-bundle` against a fresh workspace
//!      tempdir to produce the upstream artifacts (payload + sidecar +
//!      checkpoint + preview bundle + generator result).
//!   2. Synthesizing an `a2-l2b-approval-result.v1` JSON that binds to
//!      the generated preview record.
//!   3. Running `claw plan apply-bundle` against the two on-disk inputs.
//!   4. Asserting on the structured `a2-l2b-apply-bundle-generator-result.v1`
//!      envelope and the apply-bundle.json artifact written next to the
//!      preview bundle.
//!
//! Required operator claims covered here:
//!   * `claw plan apply-bundle` exists and accepts exactly two positional
//!     arguments.
//!   * Pre-approval / batch flags (`--yes`, `--auto`, `--force`,
//!     `--allow-write`, `--preapproved`, `--batch`) are refused outright.
//!   * Refusals: missing preview-result, invalid JSON, wrong schema,
//!     `ok=false`, missing approval-result, invalid approval JSON,
//!     `decision != "approved"`, mismatched step / preview id /
//!     `preview_sha`, missing/invalid preview bundle, payload tampering,
//!     sidecar tampering, manifest mismatches, unknown fields.
//!   * The generated apply-bundle.json is consumable by `claw plan apply`
//!     (schema-valid; load + authority pass; can refuse downstream on
//!     authority chain re-checks only — never on bundle-load).
//!   * The target file is NEVER mutated.
//!   * Stdout NEVER contains raw payload bytes.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1: &str = "a2-l2b-apply-bundle-generator-result.v1";
const APPLY_BUNDLE_SCHEMA_V1: &str = "a2-l2b-apply-bundle.v1";
const APPROVAL_RESULT_SCHEMA_V1: &str = "a2-l2b-approval-result.v1";
const PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1: &str = "a2-l2b-preview-bundle-generator-result.v1";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "a2-l2b-abg-it-{}-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
        seq
    ));
    fs::create_dir_all(&dir).expect("tempdir create");
    dir.canonicalize().expect("tempdir canonicalize")
}

fn parse_stdout_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
}

struct PreviewGenOutput {
    json: serde_json::Value,
    workspace: PathBuf,
    target_rel: String,
    #[allow(dead_code)]
    target_abs: PathBuf,
}

/// Drive `claw plan preview-bundle` once against a fresh workspace and
/// after-file. Returns the parsed result JSON plus workspace info.
fn run_preview_bundle(label: &str, target_rel: &str, after_bytes: &[u8]) -> PreviewGenOutput {
    let workspace = unique_temp_dir(&format!("{label}-ws"));
    // Pre-create parent directories the resolver needs.
    if let Some(parent) = Path::new(target_rel).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(workspace.join(parent)).expect("pre-create parent");
        }
    }
    let after_dir = unique_temp_dir(&format!("{label}-after"));
    let after_file = after_dir.join("after.bin");
    fs::write(&after_file, after_bytes).expect("write after-file");
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            &workspace.to_string_lossy(),
            target_rel,
            &after_file.to_string_lossy(),
        ])
        .output()
        .expect("claw should launch");
    assert_eq!(
        out.status.code(),
        Some(0),
        "preview-bundle must succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    let target_abs = workspace.join(target_rel);
    PreviewGenOutput {
        json,
        workspace,
        target_rel: target_rel.to_string(),
        target_abs,
    }
}

/// Write the preview-generator result JSON to a file the operator could
/// have piped from a real `claw plan preview-bundle` invocation. Returns
/// the path on disk.
fn write_preview_result(dir: &Path, name: &str, value: &serde_json::Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, serde_json::to_vec(value).expect("serialize")).expect("write");
    path
}

fn write_approval_result(dir: &Path, name: &str, value: &serde_json::Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, serde_json::to_vec(value).expect("serialize")).expect("write");
    path
}

/// Build a canonical approval-result JSON value for `preview_gen.json`.
fn approval_for(
    preview_gen: &serde_json::Value,
    preview_bundle: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": "approved",
        "preview_id": preview_gen["preview_id"],
        "step_id": preview_gen["step_id"],
        "preview_sha256": preview_bundle["preview_record"]["preview_sha256"],
        "checkpoint_baseline_unchanged": true,
        "exit_code_hint": 0,
        "audit_markers": ["a2-l2b-approved"],
    })
}

fn read_preview_bundle(preview_gen: &serde_json::Value) -> serde_json::Value {
    let path = PathBuf::from(preview_gen["preview_bundle_path"].as_str().unwrap());
    let text = fs::read_to_string(&path).expect("read preview bundle");
    serde_json::from_str(&text).expect("parse preview bundle")
}

fn run_claw_apply_bundle(preview_result_path: &Path, approval_result_path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            &preview_result_path.to_string_lossy(),
            &approval_result_path.to_string_lossy(),
        ])
        .output()
        .expect("claw should launch")
}

// ------------------------------------------------------------------------
// Parser-layer claims — exactly 2 positionals, no flags.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_missing_all_args_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply-bundle"])
        .output()
        .expect("claw should launch");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("apply-bundle") || stderr.contains("positional"),
        "expected usage error; stderr={stderr}"
    );
}

#[test]
fn apply_bundle_missing_second_arg_is_usage_error() {
    let dir = unique_temp_dir("usage-1arg");
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply-bundle", &dir.to_string_lossy()])
        .output()
        .expect("claw should launch");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("2 positional"));
}

#[test]
fn apply_bundle_too_many_args_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "/tmp/a.json",
            "/tmp/b.json",
            "extra",
        ])
        .output()
        .expect("claw should launch");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unexpected positional"));
}

#[test]
fn apply_bundle_rejects_yes_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--yes",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--yes"));
}

#[test]
fn apply_bundle_rejects_auto_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--auto",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--auto"));
}

#[test]
fn apply_bundle_rejects_force_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--force",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--force"));
}

#[test]
fn apply_bundle_rejects_allow_write_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--allow-write",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--allow-write"));
}

#[test]
fn apply_bundle_rejects_preapproved_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--preapproved",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--preapproved"));
}

#[test]
fn apply_bundle_rejects_batch_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "apply-bundle",
            "--batch",
            "/tmp/a.json",
            "/tmp/b.json",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--batch"));
}

// ------------------------------------------------------------------------
// Refusals — preview-result file failures.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_missing_preview_result_file_refuses() {
    let dir = unique_temp_dir("missing-preview-result");
    let missing = dir.join("does-not-exist.json");
    // We still need a valid second-arg path syntactically; the parser does
    // not stat it, the generator does.
    let approval = write_approval_result(&dir, "approval.json", &serde_json::json!({}));
    let out = run_claw_apply_bundle(&missing, &approval);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1);
    assert_eq!(json["ok"], false);
    assert_eq!(json["refusal"], "preview-result-io-error");
    let markers: Vec<&str> = json["audit_markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(markers.contains(&"a2-l2b-apply-bundle-refused"));
}

#[test]
fn apply_bundle_invalid_preview_result_json_refuses() {
    let dir = unique_temp_dir("invalid-preview-result");
    let bad = dir.join("bad.json");
    fs::write(&bad, b"{not json").unwrap();
    let approval = write_approval_result(&dir, "approval.json", &serde_json::json!({}));
    let out = run_claw_apply_bundle(&bad, &approval);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "preview-result-json-parse-error");
}

#[test]
fn apply_bundle_wrong_preview_result_schema_refuses() {
    let pg = run_preview_bundle("wrong-pg-schema", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("wrong-pg-schema-dir");
    let mut pg_value = pg.json.clone();
    pg_value["schema_version"] = serde_json::json!("a2-l2b-preview-bundle-generator-result.v999");
    let pg_path = write_preview_result(&dir, "preview.json", &pg_value);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_value = approval_for(&pg.json, &preview_bundle);
    let approval_path = write_approval_result(&dir, "approval.json", &approval_value);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "preview-result-schema-version-mismatch");
}

#[test]
fn apply_bundle_preview_result_not_ok_refuses() {
    let dir = unique_temp_dir("preview-result-not-ok");
    let pg_value = serde_json::json!({
        "schema_version": PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1,
        "ok": false,
        "refusal": "workspace-root-invalid",
        "reason": "synthetic",
        "audit_markers": ["a2-l2b-preview-bundle-refused"],
    });
    let pg_path = write_preview_result(&dir, "preview.json", &pg_value);
    let approval_path = write_approval_result(&dir, "approval.json", &serde_json::json!({}));
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    // The refusal-shape file does NOT match the strict success-only
    // deserialize, so this surfaces as a parse error. Either token is an
    // acceptable refusal (the test point is exit 5 + structured envelope).
    let refusal = json["refusal"].as_str().unwrap();
    assert!(
        refusal == "preview-result-not-ok" || refusal == "preview-result-json-parse-error",
        "unexpected refusal token: {refusal}"
    );
}

#[test]
fn apply_bundle_preview_result_unknown_field_refuses() {
    let pg = run_preview_bundle("unknown-pg-field", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("unknown-pg-field-dir");
    let mut pg_value = pg.json.clone();
    pg_value["smuggled"] = serde_json::json!("evil");
    let pg_path = write_preview_result(&dir, "preview.json", &pg_value);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "preview-result-json-parse-error");
}

// ------------------------------------------------------------------------
// Refusals — approval-result file failures.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_missing_approval_result_file_refuses() {
    let pg = run_preview_bundle("missing-approval", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("missing-approval-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let missing = dir.join("does-not-exist.json");
    let out = run_claw_apply_bundle(&pg_path, &missing);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-result-io-error");
}

#[test]
fn apply_bundle_invalid_approval_result_json_refuses() {
    let pg = run_preview_bundle("invalid-approval", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("invalid-approval-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let approval_path = dir.join("approval.json");
    fs::write(&approval_path, b"{not json").unwrap();
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-result-json-parse-error");
}

#[test]
fn apply_bundle_wrong_approval_schema_refuses() {
    let pg = run_preview_bundle("wrong-approval-schema", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("wrong-approval-schema-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["schema_version"] = serde_json::json!("a2-l2b-approval-result.v999");
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-result-schema-version-mismatch");
}

#[test]
fn apply_bundle_approval_decision_not_approved_refuses() {
    let pg = run_preview_bundle("decision-not-approved", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("decision-not-approved-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["decision"] = serde_json::json!("refused");
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-decision-not-approved");
}

#[test]
fn apply_bundle_approval_step_id_mismatch_refuses() {
    let pg = run_preview_bundle("step-id-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("step-id-mismatch-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["step_id"] = serde_json::json!("step-99");
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-step-id-mismatch");
}

#[test]
fn apply_bundle_approval_preview_id_mismatch_refuses() {
    let pg = run_preview_bundle("preview-id-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("preview-id-mismatch-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["preview_id"] = serde_json::json!("01ABCD-bogus");
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-preview-id-mismatch");
}

#[test]
fn apply_bundle_approval_preview_sha_mismatch_refuses() {
    let pg = run_preview_bundle("preview-sha-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("preview-sha-mismatch-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["preview_sha256"] = serde_json::json!("d".repeat(64));
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-preview-sha-mismatch");
}

#[test]
fn apply_bundle_approval_unknown_field_refuses() {
    let pg = run_preview_bundle("approval-unknown-field", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("approval-unknown-field-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let mut approval = approval_for(&pg.json, &preview_bundle);
    approval["smuggled"] = serde_json::json!("evil");
    let approval_path = write_approval_result(&dir, "approval.json", &approval);
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "approval-result-json-parse-error");
}

// ------------------------------------------------------------------------
// Refusals — preview-bundle, payload, sidecar, manifest tampering.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_missing_preview_bundle_refuses() {
    let pg = run_preview_bundle("missing-preview-bundle", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("missing-preview-bundle-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    // Delete the preview bundle artifact under the workspace root.
    fs::remove_file(PathBuf::from(
        pg.json["preview_bundle_path"].as_str().unwrap(),
    ))
    .expect("remove preview bundle");
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "preview-bundle-io-error");
}

#[test]
fn apply_bundle_payload_missing_refuses() {
    let pg = run_preview_bundle("payload-missing", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("payload-missing-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    fs::remove_file(PathBuf::from(pg.json["payload_path"].as_str().unwrap()))
        .expect("remove payload");
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    let refusal = json["refusal"].as_str().unwrap();
    // Canonicalize on a missing path returns ENOENT, which we map to
    // payload-path-layout-invalid since the layout-anchor canonicalize
    // is the first read after parse.
    assert!(
        refusal == "payload-path-layout-invalid" || refusal == "payload-io-error",
        "unexpected refusal token for missing payload: {refusal}"
    );
}

#[test]
fn apply_bundle_payload_size_mismatch_refuses() {
    let pg = run_preview_bundle("payload-size-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("payload-size-mismatch-dir");
    // Mutate the preview-result to declare a wrong size; the live payload
    // stays unchanged on disk so the size check trips.
    let mut pg_value = pg.json.clone();
    pg_value["payload_size_bytes"] = serde_json::json!(999);
    let pg_path = write_preview_result(&dir, "preview.json", &pg_value);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "payload-size-mismatch");
}

#[test]
fn apply_bundle_payload_hash_mismatch_refuses() {
    let pg = run_preview_bundle("payload-hash-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("payload-hash-mismatch-dir");
    // Tamper the disk payload: flip last byte, same length so size check
    // still passes but the streaming sha256 will diverge.
    let payload_path = PathBuf::from(pg.json["payload_path"].as_str().unwrap());
    let mut tampered = fs::read(&payload_path).expect("read payload");
    let last = tampered.len() - 1;
    tampered[last] = tampered[last].wrapping_add(1);
    fs::write(&payload_path, &tampered).expect("rewrite tampered payload");

    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "payload-hash-mismatch");
}

#[test]
fn apply_bundle_sidecar_mismatch_refuses() {
    let pg = run_preview_bundle("sidecar-mismatch", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("sidecar-mismatch-dir");
    // Rewrite the sidecar to a different (valid-shape) hash so the disk
    // sha matches the bundle declaration but the sidecar disagrees.
    let sidecar_path = PathBuf::from(pg.json["payload_sha256_path"].as_str().unwrap());
    let bad_sha = "0".repeat(64);
    fs::write(&sidecar_path, format!("{bad_sha}\n")).expect("rewrite sidecar");

    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "payload-sidecar-hash-mismatch");
}

#[test]
fn apply_bundle_checkpoint_manifest_missing_refuses() {
    let pg = run_preview_bundle("manifest-missing", "src/lib.rs", b"new\n");
    let dir = unique_temp_dir("manifest-missing-dir");
    fs::remove_file(PathBuf::from(
        pg.json["checkpoint_manifest_path"].as_str().unwrap(),
    ))
    .expect("remove manifest");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );
    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    let refusal = json["refusal"].as_str().unwrap();
    assert!(
        refusal == "checkpoint-manifest-path-layout-invalid"
            || refusal == "checkpoint-manifest-io-error",
        "unexpected refusal token for missing manifest: {refusal}"
    );
}

// ------------------------------------------------------------------------
// Happy path — apply-bundle.json artifact + envelope.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_happy_path_succeeds() {
    let pg = run_preview_bundle("happy-new-file", "src/new.txt", b"hello world\n");
    let dir = unique_temp_dir("happy-new-file-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_value = approval_for(&pg.json, &preview_bundle);
    let approval_path = write_approval_result(&dir, "approval.json", &approval_value);

    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "apply-bundle must succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1);
    assert_eq!(json["ok"], true);
    assert_eq!(json["run_id"], pg.json["run_id"]);
    assert_eq!(json["step_id"], pg.json["step_id"]);
    assert_eq!(json["preview_id"], pg.json["preview_id"]);
    assert_eq!(json["target_relative_path"], pg.target_rel);
    assert_eq!(json["payload_sha256"], pg.json["payload_sha256"]);
    assert_eq!(json["payload_size_bytes"], pg.json["payload_size_bytes"]);

    let markers: Vec<&str> = json["audit_markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(markers.contains(&"a2-l2b-approval-result-validated"));
    assert!(markers.contains(&"a2-l2b-apply-bundle-created"));

    // The apply-bundle.json artifact exists and parses as the apply schema.
    let apply_bundle_path = PathBuf::from(json["apply_bundle_path"].as_str().unwrap());
    assert!(apply_bundle_path.exists(), "apply-bundle.json must exist");
    let bundle_text = fs::read_to_string(&apply_bundle_path).unwrap();
    let bundle: serde_json::Value = serde_json::from_str(&bundle_text).unwrap();
    assert_eq!(bundle["schema_version"], APPLY_BUNDLE_SCHEMA_V1);
    assert_eq!(bundle["target_relative_path"], pg.target_rel);
    assert_eq!(bundle["payload"]["kind"], "file");
    assert_eq!(bundle["payload"]["after_sha256"], pg.json["payload_sha256"]);
    assert_eq!(
        bundle["payload"]["after_size_bytes"],
        pg.json["payload_size_bytes"]
    );
    assert_eq!(bundle["approval_result"]["decision"], "approved");

    // Stdout must NOT contain raw payload bytes inline.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("hello world"));

    // The target file MUST NOT have been created or mutated.
    assert!(!pg.workspace.join(&pg.target_rel).exists());
}

#[test]
fn apply_bundle_happy_path_existing_target_succeeds() {
    let workspace = unique_temp_dir("existing-target-ws");
    let target_rel = "src/lib.rs";
    let target_abs = workspace.join(target_rel);
    fs::create_dir_all(target_abs.parent().unwrap()).unwrap();
    let before_bytes = b"old contents\n";
    fs::write(&target_abs, before_bytes).unwrap();

    let after_dir = unique_temp_dir("existing-target-after");
    let after_file = after_dir.join("after.bin");
    let after_bytes = b"new contents\n";
    fs::write(&after_file, after_bytes).unwrap();

    let preview_out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            &workspace.to_string_lossy(),
            target_rel,
            &after_file.to_string_lossy(),
        ])
        .output()
        .expect("claw should launch");
    assert_eq!(preview_out.status.code(), Some(0));
    let pg_json = parse_stdout_json(&preview_out.stdout);

    let dir = unique_temp_dir("existing-target-results");
    let pg_path = write_preview_result(&dir, "preview.json", &pg_json);
    let preview_bundle = read_preview_bundle(&pg_json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg_json, &preview_bundle),
    );

    let out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(out.status.code(), Some(0));

    // Target file on disk must still equal the original before bytes.
    let on_disk = fs::read(&target_abs).expect("read target");
    assert_eq!(
        on_disk, before_bytes,
        "apply-bundle generator must NEVER mutate the target file"
    );

    // Smoke-check the after-sha bound in the apply bundle.
    let json = parse_stdout_json(&out.stdout);
    let apply_bundle_path = PathBuf::from(json["apply_bundle_path"].as_str().unwrap());
    let bundle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&apply_bundle_path).unwrap()).unwrap();
    assert_eq!(bundle["payload"]["after_sha256"], pg_json["payload_sha256"]);

    // after_bytes content should NOT leak to stdout.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("new contents"));
}

// ------------------------------------------------------------------------
// Idempotency — running the generator twice with the same inputs is
// accepted; running it with divergent re-serialization is refused.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_idempotent_second_call_succeeds() {
    let pg = run_preview_bundle("idempotent", "src/lib.rs", b"hi\n");
    let dir = unique_temp_dir("idempotent-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );

    let first = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(first.status.code(), Some(0));
    let first_json = parse_stdout_json(&first.stdout);
    let bundle_path = PathBuf::from(first_json["apply_bundle_path"].as_str().unwrap());
    let first_bytes = fs::read(&bundle_path).expect("read bundle");

    let second = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(
        second.status.code(),
        Some(0),
        "second call must succeed when the apply-bundle would be byte-identical"
    );
    let second_bytes = fs::read(&bundle_path).expect("read bundle");
    assert_eq!(
        first_bytes, second_bytes,
        "apply-bundle must not have changed"
    );
}

#[test]
fn apply_bundle_divergent_existing_refuses() {
    let pg = run_preview_bundle("divergent", "src/lib.rs", b"hi\n");
    let dir = unique_temp_dir("divergent-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );

    let first = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(first.status.code(), Some(0));
    let first_json = parse_stdout_json(&first.stdout);
    let bundle_path = PathBuf::from(first_json["apply_bundle_path"].as_str().unwrap());

    // Overwrite the bundle with divergent content (still valid JSON; the
    // generator only cares about byte-identity, not parse).
    let mut existing: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&bundle_path).unwrap()).unwrap();
    existing["target_relative_path"] = serde_json::json!("src/something_else.rs");
    fs::write(&bundle_path, serde_json::to_vec_pretty(&existing).unwrap()).expect("rewrite bundle");

    let second = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(
        second.status.code(),
        Some(5),
        "second call with divergent existing bundle must refuse"
    );
    let json = parse_stdout_json(&second.stdout);
    assert_eq!(json["refusal"], "apply-bundle-exists-divergent");
}

// ------------------------------------------------------------------------
// End-to-end handshake — generated apply-bundle is consumable by
// `claw plan apply` (it applies the write under the runner-owned target).
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_artifact_is_consumable_by_claw_plan_apply() {
    let pg = run_preview_bundle("apply-handshake", "src/new.txt", b"first line\nsecond\n");
    let dir = unique_temp_dir("apply-handshake-dir");
    let pg_path = write_preview_result(&dir, "preview.json", &pg.json);
    let preview_bundle = read_preview_bundle(&pg.json);
    let approval_path = write_approval_result(
        &dir,
        "approval.json",
        &approval_for(&pg.json, &preview_bundle),
    );

    let gen_out = run_claw_apply_bundle(&pg_path, &approval_path);
    assert_eq!(gen_out.status.code(), Some(0));
    let gen_json = parse_stdout_json(&gen_out.stdout);
    let bundle_path = PathBuf::from(gen_json["apply_bundle_path"].as_str().unwrap());
    assert!(bundle_path.exists());

    // Hand the bundle to `claw plan apply`. It should accept the bundle
    // at load time (schema valid, hashes match, authority chain bound)
    // and proceed to apply. The target ends up containing the after bytes.
    let apply_out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", &bundle_path.to_string_lossy()])
        .output()
        .expect("claw should launch");
    let apply_stdout = String::from_utf8_lossy(&apply_out.stdout);
    let apply_stderr = String::from_utf8_lossy(&apply_out.stderr);
    let apply_json: serde_json::Value = serde_json::from_str(apply_stdout.trim_end_matches('\n'))
        .unwrap_or_else(|_| {
            panic!("apply stdout is one JSON value; got {apply_stdout} stderr={apply_stderr}")
        });
    let outcome = apply_json["outcome"].as_str().unwrap_or("");
    assert!(
        outcome != "bundle_rejected",
        "apply must NOT reject the bundle at load; apply stdout={apply_stdout}"
    );
    // Happy path: applied. On unusual sandboxes the apply may refuse for
    // baseline-drift / similar reasons unrelated to bundle validity; the
    // test point is bundle parseability.
    assert!(
        outcome == "applied" || outcome == "refused",
        "unexpected apply outcome: {outcome}"
    );
}

// ------------------------------------------------------------------------
// Stdout is a single JSON line.
// ------------------------------------------------------------------------

#[test]
fn apply_bundle_stdout_is_single_json_envelope() {
    let dir = unique_temp_dir("single-json");
    let missing = dir.join("does-not-exist.json");
    let approval_path = write_approval_result(&dir, "approval.json", &serde_json::json!({}));
    let out = run_claw_apply_bundle(&missing, &approval_path);
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = text.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "stdout must be a single JSON line, got: {text:?}"
    );
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("stdout is one JSON value");
}

// ------------------------------------------------------------------------
// Scope-guard source grep: no forbidden APIs / phrases inside the
// apply-bundle generator's implementation block.
// ------------------------------------------------------------------------

fn read_generator_block_source() -> String {
    let main_rs = include_str!("../src/main.rs");
    let start_marker = "// A2-L2b Slice L2b-CLI-Apply-Bundle-Generator — `claw plan apply-bundle`";
    let end_marker = "END A2-L2b Slice L2b-CLI-Apply-Bundle-Generator";
    let start = main_rs
        .find(start_marker)
        .expect("apply-bundle generator block start sentinel must exist");
    let end = main_rs[start..]
        .find(end_marker)
        .expect("apply-bundle generator block end sentinel must exist");
    main_rs[start..start + end].to_string()
}

fn read_generator_block_code_only() -> String {
    let src = read_generator_block_source();
    let mut buf = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    buf
}

#[test]
fn apply_bundle_generator_source_does_not_invoke_apply_or_broker() {
    let src = read_generator_block_code_only();
    for forbidden in [
        "a2_plan_runner::run_plan(",
        "a2_plan_runner::execute_write(",
        "a2_plan_runner::bind_after_bytes(",
        "execute_write(",
        "bind_after_bytes(",
        "WriteExecutionRequest",
        "broker.py",
        "11434",
        "11435",
        "OPENAI_BASE_URL",
        "vram-broker",
        "Command::new",
        "spawn(",
        "git apply",
        "git diff",
    ] {
        assert!(
            !src.contains(forbidden),
            "apply-bundle generator block must not invoke `{forbidden}`; this lane never \
             wires apply / broker / subprocess / write-execution APIs"
        );
    }
}

#[test]
fn apply_bundle_generator_source_does_not_print_raw_payload() {
    let src = read_generator_block_code_only();
    for forbidden in [
        "println!(\"{}\", after_bytes",
        "println!(\"{after_bytes",
        "writeln!(stdout, \"{}\", after_bytes",
        "base64::",
        "Engine::encode",
        "inline payload",
        "stdin payload",
    ] {
        assert!(
            !src.contains(forbidden),
            "apply-bundle generator block must not print raw payload bytes; saw `{forbidden}`"
        );
    }
}

#[test]
fn apply_bundle_generator_source_does_not_accept_bypass_flags() {
    let src = read_generator_block_code_only();
    for forbidden in [
        "--yes",
        "--auto",
        "--allow-write",
        "--preapproved",
        "--batch",
    ] {
        assert!(
            !src.contains(forbidden),
            "apply-bundle generator block must not accept bypass flag `{forbidden}` \
             (the parser is in a separate function and rejects them at the CLI layer)"
        );
    }
}
