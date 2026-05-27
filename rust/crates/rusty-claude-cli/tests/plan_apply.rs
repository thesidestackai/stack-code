//! A2-L2b Slice L2b-CLI-Apply — integration tests for
//! `claw plan apply <apply-bundle.json>`.
//!
//! All tests are no-broker / no-network by construction. They invoke the
//! built `claw` binary as a subprocess with a fully isolated workspace
//! tempdir, drive the full Slice-1..4a authority pipeline via the public
//! `a2-plan-runner` API to manufacture the apply bundle, and assert on
//! the structured `a2-l2b-apply-result.v1` JSON envelope plus exit code.
//!
//! Required operator claims covered here:
//!   * `claw plan apply <bundle>` exists.
//!   * Missing-bundle arg surfaces a usage error.
//!   * Pre-approval flags (`--yes`, `--auto`, `--force`, `--allow-write`,
//!     `--preapproved`, `--batch`) are rejected outright.
//!   * Bundle load / parse / schema / authority failures exit 5 with a
//!     structured `bundle_rejected` envelope.
//!   * Embedded approval mismatches exit 7.
//!   * Baseline drift exits 9.
//!   * The happy path writes exactly one file and exits 0.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use a2_plan_runner::{
    bind_after_bytes, build_preview, CheckpointStore, PreviewInputs, PreviewRecord,
};

const APPLY_BUNDLE_SCHEMA_V1: &str = "a2-l2b-apply-bundle.v1";
const APPLY_RESULT_SCHEMA_V1: &str = "a2-l2b-apply-result.v1";
const APPROVAL_RESULT_SCHEMA_V1: &str = "a2-l2b-approval-result.v1";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "a2-l2b-apply-it-{}-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
        seq
    ));
    fs::create_dir_all(&dir).expect("tempdir create");
    dir.canonicalize().expect("tempdir canonicalize")
}

/// Authority objects produced by `manufacture_authority` for a happy-path
/// apply. Holds everything needed to build an apply bundle JSON.
struct AuthorityChain {
    workspace_root: PathBuf,
    target_rel: String,
    target_abs: PathBuf,
    preview: PreviewRecord,
    manifest_path: PathBuf,
    payload_path: PathBuf,
    payload_bytes: Vec<u8>,
}

/// Drive the full Slice-1..4a pipeline against a fresh workspace tempdir.
/// `before` is `Some(bytes)` for an overwrite scenario, `None` for a
/// new-file create.
fn manufacture_authority(
    label: &str,
    target_rel: &str,
    before: Option<&[u8]>,
    after: &[u8],
) -> AuthorityChain {
    let workspace_root = unique_temp_dir(label);
    let target_abs = workspace_root.join(target_rel);
    if let Some(b) = before {
        if let Some(parent) = target_abs.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&target_abs, b).expect("seed target");
    } else if let Some(parent) = target_abs.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }

    // Slice 2: checkpoint.
    let store = CheckpointStore::new_with_generated_run_id(workspace_root.clone());
    let step_id = "step-1";
    let checkpoint = store
        .create_checkpoint(step_id, &target_abs, Path::new(target_rel))
        .expect("create_checkpoint");

    // Slice 3a: build preview.
    let run_id = *store.run_id();
    let target_rel_path = PathBuf::from(target_rel);
    let inputs = PreviewInputs {
        step_id,
        target_relative_path: &target_rel_path,
        target_absolute_path: &target_abs,
        before,
        after,
        checkpoint_run_id: &run_id,
        checkpoint_step_id: step_id,
        created_at_utc: "2026-05-26T00:00:00.000000000Z",
    };
    let (preview, _display) = build_preview(&inputs).expect("build_preview");

    // Slice 4a: bind (proves the payload would be acceptable). We retain
    // the raw bytes for writing to the payload file below.
    let _ = bind_after_bytes(&preview, target_rel_path.clone(), after.to_vec())
        .expect("bind_after_bytes succeeds on canonical inputs");

    // Write the payload file into a sibling tempdir (NOT inside the
    // workspace, so the resolver's deny-component list doesn't fire on
    // anything stray).
    let payload_dir = unique_temp_dir(&format!("{label}-payload"));
    let payload_path = payload_dir.join("after.bin");
    fs::write(&payload_path, after).expect("write payload file");

    AuthorityChain {
        workspace_root,
        target_rel: target_rel.to_string(),
        target_abs,
        preview,
        manifest_path: checkpoint.manifest_path.clone(),
        payload_path,
        payload_bytes: after.to_vec(),
    }
}

/// Build a canonical approval-result JSON object for `auth`. The fields
/// here mirror what `claw plan approve` emits on stdout for an approved
/// decision.
fn approved_result_for(auth: &AuthorityChain) -> serde_json::Value {
    serde_json::json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": "approved",
        "preview_id": auth.preview.preview_id,
        "step_id": auth.preview.step_id,
        "preview_sha256": auth.preview.preview_sha256,
        "checkpoint_baseline_unchanged": true,
        "exit_code_hint": 0,
        "audit_markers": [],
    })
}

/// Build a canonical apply-bundle JSON value for `auth` + `approval`.
fn apply_bundle_for(auth: &AuthorityChain, approval: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version": APPLY_BUNDLE_SCHEMA_V1,
        "workspace_root": auth.workspace_root,
        "target_relative_path": auth.target_rel,
        "preview_record": auth.preview,
        "approval_result": approval,
        "checkpoint": { "manifest_path": auth.manifest_path },
        "payload": {
            "kind": "file",
            "path": auth.payload_path,
            "after_sha256": auth.preview.after_sha256,
            "after_size_bytes": auth.payload_bytes.len() as u64,
        },
    })
}

fn write_bundle(dir: &Path, bundle: &serde_json::Value) -> PathBuf {
    let path = dir.join("apply-bundle.json");
    fs::write(&path, serde_json::to_vec(bundle).expect("serialize bundle")).expect("write bundle");
    path
}

fn run_claw_apply(bundle_path: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", &bundle_path.to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("claw should launch");
    {
        // Apply must NEVER read stdin. Close it immediately to prove the
        // child does not block on it.
        let stdin = child.stdin.as_mut().expect("child stdin pipe");
        let _ = stdin.write_all(b"");
    }
    child.wait_with_output().expect("claw should exit")
}

fn parse_stdout_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
}

// -- Required claim 1: `claw plan apply <bundle>` exists -----------------

#[test]
fn plan_apply_command_exists_missing_bundle_arg_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply"])
        .output()
        .expect("claw should launch");
    assert!(
        !out.status.success(),
        "missing bundle path must not exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("missing apply bundle"),
        "expected usage error mentioning missing apply bundle; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_yes_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--yes", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--yes"),
        "apply must reject --yes; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_auto_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--auto", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--auto"),
        "apply must reject --auto; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_force_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--force", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--force"),
        "apply must reject --force; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_allow_write_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--allow-write", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--allow-write"),
        "apply must reject --allow-write; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_preapproved_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--preapproved", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--preapproved"),
        "apply must reject --preapproved; stderr={stderr}"
    );
}

#[test]
fn plan_apply_rejects_batch_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "apply", "--batch", "/tmp/x.json"])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--batch"),
        "apply must reject --batch; stderr={stderr}"
    );
}

// -- Required claim 2: bundle load failures exit 5 ---------------------

#[test]
fn plan_apply_missing_bundle_file_exits_five() {
    let dir = unique_temp_dir("missing-bundle");
    let missing = dir.join("does-not-exist.json");
    let out = run_claw_apply(&missing);
    assert_eq!(
        out.status.code(),
        Some(5),
        "missing bundle file must exit 5; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], APPLY_RESULT_SCHEMA_V1);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert_eq!(json["exit_code"], 5);
}

#[test]
fn plan_apply_invalid_bundle_json_exits_five() {
    let dir = unique_temp_dir("bad-json");
    let path = dir.join("broken.json");
    fs::write(&path, b"{not json").unwrap();
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-json-parse-error"));
}

#[test]
fn plan_apply_wrong_schema_version_exits_five() {
    let dir = unique_temp_dir("wrong-schema");
    let auth = manufacture_authority("wrong-schema", "src/lib.rs", Some(b"old\n"), b"new\n");
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["schema_version"] = serde_json::json!("a2-l2b-apply-bundle.v999");
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-schema-version-mismatch"));
}

#[test]
fn plan_apply_unknown_field_exits_five() {
    let dir = unique_temp_dir("unknown-field");
    let auth = manufacture_authority("unknown-field", "src/lib.rs", Some(b"old\n"), b"new\n");
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["smuggled_field"] = serde_json::json!("danger");
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-json-parse-error"));
}

#[test]
fn plan_apply_unknown_field_in_payload_exits_five() {
    let dir = unique_temp_dir("unknown-payload-field");
    let auth = manufacture_authority(
        "unknown-payload-field",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["payload"]["execute_bytes"] = serde_json::json!(true);
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
}

#[test]
fn plan_apply_missing_payload_file_exits_five() {
    let dir = unique_temp_dir("missing-payload");
    let auth = manufacture_authority("missing-payload", "src/lib.rs", Some(b"old\n"), b"new\n");
    // Delete the payload file BEFORE running apply.
    fs::remove_file(&auth.payload_path).expect("remove payload file");
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("payload-io-error"));
}

#[test]
fn plan_apply_payload_size_mismatch_exits_five() {
    let dir = unique_temp_dir("payload-size-mismatch");
    let auth = manufacture_authority(
        "payload-size-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    // Declare a size that disagrees with the on-disk payload.
    bundle["payload"]["after_size_bytes"] = serde_json::json!(999);
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("payload-size-mismatch"));
}

#[test]
fn plan_apply_payload_preview_after_sha_mismatch_exits_five() {
    let dir = unique_temp_dir("payload-preview-after-mismatch");
    let auth = manufacture_authority(
        "payload-preview-after-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    // Declare a hash that disagrees with preview_record.after_sha256.
    bundle["payload"]["after_sha256"] = serde_json::json!("0".repeat(64));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "payload-preview-after-sha-mismatch"
    );
}

#[test]
fn plan_apply_payload_bytes_hash_mismatch_exits_five() {
    let dir = unique_temp_dir("payload-bytes-hash-mismatch");
    let auth = manufacture_authority(
        "payload-bytes-hash-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    // Tamper the on-disk payload bytes WITHOUT updating the bundle hash
    // (so size still matches but bytes don't hash to preview.after_sha256).
    let mut tampered = auth.payload_bytes.clone();
    // Flip last byte; len unchanged.
    let last = tampered.len() - 1;
    tampered[last] = tampered[last].wrapping_add(1);
    fs::write(&auth.payload_path, &tampered).expect("rewrite tampered payload");
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("payload-hash-mismatch"));
}

#[test]
fn plan_apply_unsupported_payload_kind_exits_five() {
    let dir = unique_temp_dir("unsupported-payload-kind");
    let auth = manufacture_authority(
        "unsupported-payload-kind",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["payload"]["kind"] = serde_json::json!("stdin");
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-unsupported-payload-kind"));
}

#[test]
fn plan_apply_target_path_mismatch_exits_five() {
    let dir = unique_temp_dir("target-path-mismatch");
    let auth = manufacture_authority(
        "target-path-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["target_relative_path"] = serde_json::json!("src/other.rs");
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "bundle-target-path-mismatch"
    );
}

#[test]
fn plan_apply_workspace_root_missing_exits_five() {
    let dir = unique_temp_dir("workspace-missing");
    let auth = manufacture_authority("workspace-missing", "src/lib.rs", Some(b"old\n"), b"new\n");
    let mut bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    bundle["workspace_root"] = serde_json::json!("/nonexistent/abs/path/that/should/not/exist");
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("bundle-workspace-root-invalid"));
}

#[test]
fn plan_apply_manifest_missing_exits_five() {
    let dir = unique_temp_dir("manifest-missing");
    let auth = manufacture_authority("manifest-missing", "src/lib.rs", Some(b"old\n"), b"new\n");
    // Remove the manifest file so the load step refuses.
    fs::remove_file(&auth.manifest_path).expect("remove manifest");
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "bundle_rejected");
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("checkpoint-manifest-io-error"));
}

// -- Required claim 3: approval mismatches exit 7 ---------------------

#[test]
fn plan_apply_approval_decision_not_approved_exits_seven() {
    let dir = unique_temp_dir("approval-not-approved");
    let auth = manufacture_authority(
        "approval-not-approved",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut approval = approved_result_for(&auth);
    approval["decision"] = serde_json::json!("refused");
    let bundle = apply_bundle_for(&auth, &approval);
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "refused");
    assert_eq!(json["exit_code"], 7);
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("approval-decision-not-approved"));
}

#[test]
fn plan_apply_approval_preview_sha_mismatch_exits_seven() {
    let dir = unique_temp_dir("approval-preview-mismatch");
    let auth = manufacture_authority(
        "approval-preview-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut approval = approved_result_for(&auth);
    approval["preview_sha256"] = serde_json::json!("d".repeat(64));
    let bundle = apply_bundle_for(&auth, &approval);
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "refused");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "approval-preview-sha-mismatch"
    );
}

#[test]
fn plan_apply_approval_step_id_mismatch_exits_seven() {
    let dir = unique_temp_dir("approval-step-mismatch");
    let auth = manufacture_authority(
        "approval-step-mismatch",
        "src/lib.rs",
        Some(b"old\n"),
        b"new\n",
    );
    let mut approval = approved_result_for(&auth);
    approval["step_id"] = serde_json::json!("step-99");
    let bundle = apply_bundle_for(&auth, &approval);
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(7));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "refused");
    assert_eq!(
        json["reason"].as_str().unwrap(),
        "approval-step-id-mismatch"
    );
}

// -- Required claim 4: baseline drift exits 9 -------------------------

#[test]
fn plan_apply_baseline_drift_exits_nine() {
    let dir = unique_temp_dir("baseline-drift");
    let auth = manufacture_authority(
        "baseline-drift",
        "src/lib.rs",
        Some(b"original\n"),
        b"new bytes\n",
    );
    // Mutate the target on disk so its current hash != manifest.pre_sha256.
    fs::write(&auth.target_abs, b"drifted bytes\n").expect("rewrite target post-checkpoint");
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(out.status.code(), Some(9));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "refused");
    assert_eq!(json["exit_code"], 9);
    assert!(json["reason"]
        .as_str()
        .unwrap()
        .starts_with("baseline-drift"));
    // Target on disk MUST be unchanged from the drifted state — the apply
    // refused before any write.
    let on_disk = fs::read(&auth.target_abs).expect("read target");
    assert_eq!(on_disk, b"drifted bytes\n");
}

// -- Required claim 5: happy path writes exactly one file ---------------

#[test]
fn plan_apply_happy_path_overwrite_succeeds() {
    let dir = unique_temp_dir("happy-overwrite");
    let auth = manufacture_authority(
        "happy-overwrite",
        "src/lib.rs",
        Some(b"alpha\nbeta\n"),
        b"alpha\nbeta\ngamma\n",
    );
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "happy path must exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], APPLY_RESULT_SCHEMA_V1);
    assert_eq!(json["outcome"], "applied");
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["step_id"], auth.preview.step_id);
    assert_eq!(json["preview_id"], auth.preview.preview_id);
    assert_eq!(json["preview_sha256"], auth.preview.preview_sha256);
    assert_eq!(
        json["target_relative_path"],
        auth.preview.target_relative_path_sanitized
    );
    let markers = json["markers"].as_array().expect("markers is array");
    let marker_strs: Vec<&str> = markers.iter().map(|v| v.as_str().unwrap_or("")).collect();
    assert!(
        marker_strs.contains(&"a2-l2b-write-validated"),
        "markers should include write-validated; got {marker_strs:?}"
    );
    assert!(
        marker_strs.contains(&"a2-l2b-write-applied"),
        "markers should include write-applied; got {marker_strs:?}"
    );

    // Target bytes match the payload.
    let on_disk = fs::read(&auth.target_abs).expect("read target");
    assert_eq!(on_disk, b"alpha\nbeta\ngamma\n");
}

#[test]
fn plan_apply_happy_path_new_file_succeeds() {
    let dir = unique_temp_dir("happy-new-file");
    let auth = manufacture_authority(
        "happy-new-file",
        "src/new_file.rs",
        None,
        b"first line\nsecond line\n",
    );
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "happy path (new file) must exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["outcome"], "applied");
    let on_disk = fs::read(&auth.target_abs).expect("read target");
    assert_eq!(on_disk, b"first line\nsecond line\n");
}

// -- Required claim 6: stdout is a single JSON line -------------------

#[test]
fn plan_apply_stdout_is_single_json_envelope() {
    let dir = unique_temp_dir("single-json");
    let missing = dir.join("does-not-exist.json");
    let out = run_claw_apply(&missing);
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = text.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "stdout must be a single JSON line, got: {text:?}"
    );
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("stdout is one JSON value");
}

#[test]
fn plan_apply_happy_path_stdout_is_single_json_envelope() {
    let dir = unique_temp_dir("happy-single-json");
    let auth = manufacture_authority("happy-single-json", "src/lib.rs", Some(b"x\n"), b"y\n");
    let bundle = apply_bundle_for(&auth, &approved_result_for(&auth));
    let path = write_bundle(&dir, &bundle);
    let out = run_claw_apply(&path);
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = text.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "happy-path stdout must be a single JSON line, got: {text:?}"
    );
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("stdout is one JSON value");
}
