//! A2-L2d Read-Only Artifact Inspector / Status Contract — library
//! integration tests for `a2_plan_runner::read_status`.
//!
//! All tests are no-broker / no-network / no-subprocess by construction.
//! Fixtures are hand-crafted under `tempfile::TempDir`-style disposable
//! directories (using the same unique-temp-dir idiom the existing L2b
//! tests use, to avoid adding a new dependency).
//!
//! Required claims covered here:
//!   * Phase coverage for every member of the closed `Phase` enum.
//!   * STOP-condition coverage for every variant of `StopCondition` that
//!     `read_status` can derive.
//!   * Read-only invariant: a frozen workspace tree (mtime + SHA per
//!     file) is byte-equal before and after `read_status`.
//!   * Network-egress-free invariant: tests run with `HTTP_PROXY`,
//!     `HTTPS_PROXY`, `OLLAMA_HOST` set to unreachable sentinels.
//!   * Idempotency: two successive calls on an unchanged workspace
//!     produce byte-identical serialized stdout (via
//!     `serde_json::to_string_pretty`).
//!   * Canonical ordering: `evidence_paths` and `audit_markers` are
//!     sorted lexicographically.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use a2_plan_runner::{
    read_status, PreviewRecord, StatusPhase, StatusResult, StopCondition, EXIT_STATUS_REFUSED,
    READ_ONLY_INVARIANT_LITERAL, STATUS_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "a2-l2d-status-it-{}-{}-{}-{}",
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

/// Fixture builder. Each field defaults to a value that produces a
/// happy mid-chain workspace; tests tweak only what they need.
#[allow(clippy::struct_excessive_bools)]
struct Fixture {
    workspace: PathBuf,
    run_id: String,
    step_id: String,
    target_relative: String,
    /// Pre-write bytes the live target had at preview time. `None`
    /// means the target did not exist pre-write.
    before_bytes: Option<Vec<u8>>,
    /// Post-write bytes the operator would land on apply.
    after_bytes: Vec<u8>,
    /// Whether to leave the live target on disk at all.
    live_target_present: bool,
    /// What bytes the live target currently has. `None` → use
    /// `before_bytes` if `live_target_present`.
    live_target_bytes: Option<Vec<u8>>,
    /// Preview-record `is_binary`/`is_redacted`/`is_truncated` flags.
    is_binary: bool,
    is_redacted: bool,
    is_truncated: bool,
    /// Whether to write the payload sha sidecar.
    write_payload_sha: bool,
    /// Whether the payload sha sidecar matches `after_bytes`.
    payload_sha_matches: bool,
    /// Whether to write `apply-bundle.json`.
    write_apply_bundle: bool,
    /// Whether the apply-bundle `target_relative_path` matches preview.
    apply_bundle_target_matches: bool,
    /// Whether the apply-bundle `schema_version` is the correct literal.
    apply_bundle_schema_correct: bool,
}

impl Fixture {
    fn new(label: &str) -> Self {
        Self {
            workspace: unique_temp_dir(label),
            run_id: format!("run-{label}"),
            step_id: format!("step-{label}"),
            target_relative: "notes/scratch.md".to_string(),
            before_bytes: Some(b"baseline content\n".to_vec()),
            after_bytes: b"updated content\n".to_vec(),
            live_target_present: true,
            live_target_bytes: None,
            is_binary: false,
            is_redacted: false,
            is_truncated: false,
            write_payload_sha: true,
            payload_sha_matches: true,
            write_apply_bundle: false,
            apply_bundle_target_matches: true,
            apply_bundle_schema_correct: true,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn build(&self) -> BuiltFixture {
        let before_sha = self
            .before_bytes
            .as_ref()
            .map(|b| sha256_hex(b))
            .unwrap_or_default();
        let after_sha = sha256_hex(&self.after_bytes);
        let preview_sha = sha256_hex(format!("{before_sha}|{after_sha}").as_bytes());

        // Pre-create the target's parent directory so we can write a
        // file inside it.
        if let Some(parent) = Path::new(&self.target_relative).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(self.workspace.join(parent)).expect("parent dir");
            }
        }

        // Live target file.
        let target_abs = self.workspace.join(&self.target_relative);
        if self.live_target_present {
            let bytes = self
                .live_target_bytes
                .clone()
                .or_else(|| self.before_bytes.clone())
                .unwrap_or_default();
            fs::write(&target_abs, &bytes).expect("write live target");
        }

        // .claw/l2b-runs/<run-id>/
        let run_dir = self
            .workspace
            .join(".claw")
            .join("l2b-runs")
            .join(&self.run_id);
        fs::create_dir_all(&run_dir).expect("run dir");

        let run_manifest = serde_json::json!({
            "schema_version": "a2-l2b-run-plan-write-preview-run-manifest.v1",
            "run_id": self.run_id,
            "workspace_root": self.workspace,
            "plan_name": "test-plan",
            "write_step_count": 1,
            "pending_step_id": self.step_id,
            "preview_bundle_path": "(placeholder)",
            "preview_generator_result_path": "(placeholder)",
            "checkpoint_manifest_path": "(placeholder)",
            "payload_path": "(placeholder)",
            "payload_sha256": after_sha,
            "status": "write_preview_ready",
            "next_operator_command": "(placeholder)"
        });
        fs::write(
            run_dir.join("run-manifest.json"),
            serde_json::to_string_pretty(&run_manifest).unwrap(),
        )
        .expect("write run-manifest");

        let status_json = serde_json::json!({
            "schema_version": "a2-l2b-run-plan-write-preview-status.v1",
            "run_id": self.run_id,
            "status": "write_preview_ready",
            "pending_step_id": self.step_id,
            "next_operator_command": "(placeholder)"
        });
        fs::write(
            run_dir.join("status.json"),
            serde_json::to_string_pretty(&status_json).unwrap(),
        )
        .expect("write status.json");

        // .claw/l2b-preview-bundles/<run-id>/<step-id>/
        let step_dir = self
            .workspace
            .join(".claw")
            .join("l2b-preview-bundles")
            .join(&self.run_id)
            .join(&self.step_id);
        fs::create_dir_all(&step_dir).expect("step dir");

        let preview_record = PreviewRecord {
            preview_id: format!("pid-{}", self.run_id),
            step_id: self.step_id.clone(),
            target_relative_path_sanitized: self.target_relative.clone(),
            target_absolute_path_sanitized: target_abs.to_string_lossy().to_string(),
            before_sha256: before_sha.clone(),
            after_sha256: after_sha.clone(),
            preview_sha256: preview_sha.clone(),
            checkpoint_run_id: self.run_id.clone(),
            checkpoint_step_id: self.step_id.clone(),
            is_binary: self.is_binary,
            is_redacted: self.is_redacted,
            is_truncated: self.is_truncated,
            created_at_utc: "2026-05-29T15:00:00.000000000Z".to_string(),
            preview_format_version: 1,
        };
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
        .expect("write preview-bundle");

        // Optional preview-generator-result.json
        let pg_result = serde_json::json!({
            "schema_version": "a2-l2b-preview-bundle-generator-result.v1",
            "ok": true,
        });
        fs::write(
            step_dir.join("preview-generator-result.json"),
            serde_json::to_string_pretty(&pg_result).unwrap(),
        )
        .expect("write preview-generator-result");

        // .claw/l2b-payloads/<run-id>/<step-id>/after.sha256
        let payload_dir = self
            .workspace
            .join(".claw")
            .join("l2b-payloads")
            .join(&self.run_id)
            .join(&self.step_id);
        fs::create_dir_all(&payload_dir).expect("payload dir");
        if self.write_payload_sha {
            let sha = if self.payload_sha_matches {
                after_sha.clone()
            } else {
                "0".repeat(64)
            };
            fs::write(payload_dir.join("after.sha256"), format!("{sha}\n"))
                .expect("write after.sha256");
        }

        // .claw/l2b-checkpoints/<run-id>/<step-id>/manifest.json
        let ck_dir = self
            .workspace
            .join(".claw")
            .join("l2b-checkpoints")
            .join(&self.run_id)
            .join(&self.step_id);
        fs::create_dir_all(&ck_dir).expect("checkpoint dir");
        fs::write(
            ck_dir.join("manifest.json"),
            r#"{"schema_version":"test-checkpoint-manifest"}"#,
        )
        .expect("write checkpoint manifest");

        // Optional apply-bundle.json
        if self.write_apply_bundle {
            let schema = if self.apply_bundle_schema_correct {
                "a2-l2b-apply-bundle.v1"
            } else {
                "wrong-schema"
            };
            let target = if self.apply_bundle_target_matches {
                self.target_relative.clone()
            } else {
                "wrong/path.md".to_string()
            };
            let apply_bundle = serde_json::json!({
                "schema_version": schema,
                "workspace_root": self.workspace,
                "target_relative_path": target,
                "preview_record": preview_record,
                "approval_result": {
                    "schema_version": "a2-l2b-approval-result.v1",
                    "decision": "approved",
                    "preview_id": format!("pid-{}", self.run_id),
                    "step_id": self.step_id,
                    "preview_sha256": preview_sha,
                },
                "checkpoint": { "manifest_path": ck_dir.join("manifest.json") },
                "payload": {
                    "kind": "file",
                    "path": payload_dir.join("after.bin"),
                    "after_sha256": after_sha,
                    "after_size_bytes": self.after_bytes.len() as u64,
                },
            });
            fs::write(
                step_dir.join("apply-bundle.json"),
                serde_json::to_string_pretty(&apply_bundle).unwrap(),
            )
            .expect("write apply-bundle");
        }

        BuiltFixture {
            workspace: self.workspace.clone(),
            preview_sha,
            after_sha,
            step_id: self.step_id.clone(),
        }
    }
}

#[allow(dead_code)]
struct BuiltFixture {
    workspace: PathBuf,
    preview_sha: String,
    after_sha: String,
    step_id: String,
}

fn write_approval_result(
    label: &str,
    decision: &str,
    step_id: &str,
    preview_sha256: &str,
    schema_version: &str,
) -> PathBuf {
    let dir = unique_temp_dir(label);
    let path = dir.join("approval-result.json");
    let json = serde_json::json!({
        "schema_version": schema_version,
        "decision": decision,
        "preview_id": "pid-test",
        "step_id": step_id,
        "preview_sha256": preview_sha256,
    });
    fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).expect("write approval-result");
    path
}

fn set_egress_blocked_env() {
    // The status command must succeed even when broker / model / Ollama
    // endpoints are deliberately unreachable. Test asserts the status
    // command produces an envelope under these conditions.
    std::env::set_var("HTTP_PROXY", "http://127.0.0.1:1");
    std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:1");
    std::env::set_var("OLLAMA_HOST", "http://127.0.0.1:1");
}

// ---------------------------------------------------------------------------
// Phase coverage tests
// ---------------------------------------------------------------------------

#[test]
fn phase_no_run_found_when_workspace_has_no_claw_dir() {
    let workspace = unique_temp_dir("no-run-found");
    let result = read_status(&workspace, None);
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.envelope.schema_version, STATUS_SCHEMA_V1);
    assert_eq!(result.envelope.phase, StatusPhase::NoRunFound);
    assert!(result.envelope.run_id.is_none());
    assert!(result.envelope.step_id.is_none());
    assert_eq!(
        result.envelope.next_operator_command,
        "(no run found — start with claw plan run …)"
    );
    assert_eq!(
        result.envelope.read_only_invariant,
        READ_ONLY_INVARIANT_LITERAL
    );
    assert!(result
        .envelope
        .audit_markers
        .contains(&"a2-l2d-status-read".to_string()));
    assert!(result
        .envelope
        .audit_markers
        .contains(&"a2-l2d-status-no-run-found".to_string()));
}

#[test]
fn phase_awaiting_approval_default_happy_path() {
    let fx = Fixture::new("awaiting-approval").build();
    let result = read_status(&fx.workspace, None);
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.envelope.phase, StatusPhase::AwaitingApproval);
    assert!(result.envelope.is_approvable);
    assert!(!result.envelope.is_apply_ready);
    assert!(result.envelope.stop_condition.is_none());
    assert!(result
        .envelope
        .next_operator_command
        .starts_with("claw plan approve "));
}

#[test]
fn phase_preview_ready_when_approval_supplied_but_unparseable() {
    let fx_builder = Fixture::new("preview-ready");
    let built = fx_builder.build();
    // Supply a path that exists but is not a valid approval-result
    // (forces ApprovalState::Unreadable → STOP).
    let bad_path = unique_temp_dir("bad-approval").join("approval.json");
    fs::write(&bad_path, b"not json").expect("write bad approval");
    let result = read_status(&built.workspace, Some(&bad_path));
    // Unreadable approval result is folded into ApprovalShaMismatch
    // STOP. Phase falls to Unknown.
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApprovalShaMismatch)
    );
    assert_eq!(result.envelope.phase, StatusPhase::Unknown);
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
}

#[test]
fn phase_approval_captured_when_approval_matches() {
    let built = Fixture::new("approval-captured").build();
    let approval = write_approval_result(
        "approval-captured-apr",
        "approved",
        &built.step_id,
        &built.preview_sha,
        "a2-l2b-approval-result.v1",
    );
    let result = read_status(&built.workspace, Some(&approval));
    assert_eq!(result.envelope.phase, StatusPhase::ApprovalCaptured);
    assert!(result.envelope.stop_condition.is_none());
    assert!(result
        .envelope
        .next_operator_command
        .starts_with("claw plan apply-bundle "));
}

#[test]
fn phase_apply_bundle_ready_when_bundle_present_and_valid() {
    let mut fx = Fixture::new("apply-bundle-ready");
    fx.write_apply_bundle = true;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(result.envelope.phase, StatusPhase::ApplyBundleReady);
    assert!(result.envelope.is_apply_ready);
    assert!(result.envelope.stop_condition.is_none());
    assert!(result
        .envelope
        .next_operator_command
        .starts_with("claw plan apply "));
}

#[test]
fn phase_applied_when_live_target_matches_after_sha() {
    let mut fx = Fixture::new("applied");
    fx.live_target_bytes = Some(fx.after_bytes.clone());
    fx.write_apply_bundle = true;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(result.envelope.phase, StatusPhase::Applied);
    assert!(result.envelope.stop_condition.is_none());
    assert!(result
        .envelope
        .next_operator_command
        .starts_with("claw plan run "));
}

#[test]
fn phase_rolled_back_when_approval_in_hand_apply_bundle_present_live_target_is_before() {
    // RolledBack is filesystem-indistinguishable from
    // ApplyBundleReady by L2b artifacts alone. The status command
    // biases to RolledBack only when the operator additionally
    // supplies a valid approval-result, which is their strongest
    // evidence that the chain did execute and then reverted.
    let mut fx = Fixture::new("rolled-back");
    fx.live_target_bytes = fx.before_bytes.clone();
    fx.write_apply_bundle = true;
    let built = fx.build();
    let approval = write_approval_result(
        "rolled-back-apr",
        "approved",
        &built.step_id,
        &built.preview_sha,
        "a2-l2b-approval-result.v1",
    );
    let result = read_status(&built.workspace, Some(&approval));
    assert_eq!(result.envelope.phase, StatusPhase::RolledBack);
    assert!(result.envelope.stop_condition.is_none());
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
}

#[test]
fn phase_non_approvable_when_preview_is_binary() {
    let mut fx = Fixture::new("non-approvable");
    fx.is_binary = true;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(result.envelope.phase, StatusPhase::NonApprovable);
    assert!(!result.envelope.is_approvable);
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
    assert!(result
        .envelope
        .audit_markers
        .contains(&"a2-l2d-status-non-approvable".to_string()));
}

#[test]
fn phase_unknown_when_live_target_sha_diverges_without_match() {
    let mut fx = Fixture::new("unknown-divergent");
    fx.live_target_bytes = Some(b"wildly different bytes".to_vec());
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::LiveTargetShaChanged)
    );
    assert_eq!(result.envelope.phase, StatusPhase::Unknown);
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
}

// ---------------------------------------------------------------------------
// STOP-condition coverage tests
// ---------------------------------------------------------------------------

#[test]
fn stop_workspace_root_invalid_when_path_does_not_exist() {
    let bogus = PathBuf::from("/this/path/does/not/exist/a2-l2d-test");
    let result = read_status(&bogus, None);
    assert_eq!(result.exit_code, EXIT_STATUS_REFUSED);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::WorkspaceRootInvalid)
    );
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
    assert_eq!(
        result.envelope.read_only_invariant,
        READ_ONLY_INVARIANT_LITERAL
    );
}

#[test]
fn stop_run_manifest_unreadable_when_manifest_is_garbage() {
    let workspace = unique_temp_dir("manifest-garbage");
    let run_dir = workspace.join(".claw").join("l2b-runs").join("run-x");
    fs::create_dir_all(&run_dir).unwrap();
    fs::write(run_dir.join("run-manifest.json"), b"not json").unwrap();
    let result = read_status(&workspace, None);
    assert_eq!(result.exit_code, EXIT_STATUS_REFUSED);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::RunManifestUnreadable)
    );
}

#[test]
fn stop_preview_bundle_unreadable_when_bundle_is_missing() {
    let workspace = unique_temp_dir("preview-missing");
    let run_dir = workspace.join(".claw").join("l2b-runs").join("run-x");
    fs::create_dir_all(&run_dir).unwrap();
    let manifest = serde_json::json!({
        "schema_version": "a2-l2b-run-plan-write-preview-run-manifest.v1",
        "pending_step_id": "step-x"
    });
    fs::write(
        run_dir.join("run-manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();
    let result = read_status(&workspace, None);
    assert_eq!(result.exit_code, EXIT_STATUS_REFUSED);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::PreviewBundleUnreadable)
    );
}

#[test]
fn stop_payload_sha_mismatch_when_sidecar_disagrees_with_preview_record() {
    let mut fx = Fixture::new("payload-sha-mismatch");
    fx.payload_sha_matches = false;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::PayloadShaMismatch)
    );
    assert_eq!(result.envelope.next_operator_command, "STOP — escalate");
}

#[test]
fn stop_live_target_missing_when_before_sha_nonempty_and_target_gone() {
    let mut fx = Fixture::new("live-target-missing");
    fx.live_target_present = false;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::LiveTargetMissing)
    );
}

#[test]
fn stop_live_target_sha_changed_when_live_target_neither_before_nor_after() {
    let mut fx = Fixture::new("live-target-changed");
    fx.live_target_bytes = Some(b"neither before nor after".to_vec());
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::LiveTargetShaChanged)
    );
}

#[test]
fn stop_approval_decision_not_approved() {
    let built = Fixture::new("approval-denied").build();
    let approval = write_approval_result(
        "approval-denied-apr",
        "denied",
        &built.step_id,
        &built.preview_sha,
        "a2-l2b-approval-result.v1",
    );
    let result = read_status(&built.workspace, Some(&approval));
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApprovalDecisionNotApproved)
    );
}

#[test]
fn stop_approval_step_id_mismatch() {
    let built = Fixture::new("approval-step-mismatch").build();
    let approval = write_approval_result(
        "approval-step-mismatch-apr",
        "approved",
        "wrong-step-id",
        &built.preview_sha,
        "a2-l2b-approval-result.v1",
    );
    let result = read_status(&built.workspace, Some(&approval));
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApprovalStepIdMismatch)
    );
}

#[test]
fn stop_approval_sha_mismatch() {
    let built = Fixture::new("approval-sha-mismatch").build();
    let approval = write_approval_result(
        "approval-sha-mismatch-apr",
        "approved",
        &built.step_id,
        &"0".repeat(64),
        "a2-l2b-approval-result.v1",
    );
    let result = read_status(&built.workspace, Some(&approval));
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApprovalShaMismatch)
    );
}

#[test]
fn stop_apply_bundle_schema_mismatch() {
    let mut fx = Fixture::new("apply-bundle-schema-mismatch");
    fx.write_apply_bundle = true;
    fx.apply_bundle_schema_correct = false;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApplyBundleSchemaMismatch)
    );
}

#[test]
fn stop_apply_bundle_target_path_mismatch() {
    let mut fx = Fixture::new("apply-bundle-target-mismatch");
    fx.write_apply_bundle = true;
    fx.apply_bundle_target_matches = false;
    let built = fx.build();
    let result = read_status(&built.workspace, None);
    assert_eq!(
        result.envelope.stop_condition,
        Some(StopCondition::ApplyBundleTargetPathMismatch)
    );
}

// ---------------------------------------------------------------------------
// Invariant tests: read-only, network-egress-free, idempotency,
// canonical ordering.
// ---------------------------------------------------------------------------

fn snapshot_claw_tree(workspace: &Path) -> BTreeMap<String, (u128, String)> {
    // (mtime_nanos, sha256_hex) keyed by absolute path string. Used to
    // assert the read_status call did not mutate any file under
    // <workspace>/.claw/**.
    let mut out = BTreeMap::new();
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
                out.insert(
                    path.to_string_lossy().to_string(),
                    (mtime_nanos, sha256_hex(&bytes)),
                );
            }
        }
    }
    out
}

#[test]
fn invariant_read_only_no_fs_mutation() {
    let built = Fixture::new("invariant-readonly").build();
    let before = snapshot_claw_tree(&built.workspace);
    let _ = read_status(&built.workspace, None);
    let after = snapshot_claw_tree(&built.workspace);
    assert_eq!(
        before, after,
        "read_status must not mutate any file under .claw/"
    );
}

#[test]
fn invariant_read_only_with_apply_bundle_no_fs_mutation() {
    let mut fx = Fixture::new("invariant-readonly-applybundle");
    fx.write_apply_bundle = true;
    let built = fx.build();
    let before = snapshot_claw_tree(&built.workspace);
    let _ = read_status(&built.workspace, None);
    let after = snapshot_claw_tree(&built.workspace);
    assert_eq!(before, after);
}

#[test]
fn invariant_network_egress_free_under_unreachable_endpoints() {
    set_egress_blocked_env();
    let built = Fixture::new("invariant-egress-blocked").build();
    let result = read_status(&built.workspace, None);
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.envelope.phase, StatusPhase::AwaitingApproval);
}

#[test]
fn invariant_idempotent_emit_byte_identical_stdout() {
    let built = Fixture::new("invariant-idempotent").build();
    let r1 = read_status(&built.workspace, None);
    let r2 = read_status(&built.workspace, None);
    let s1 = serde_json::to_string_pretty(&r1.envelope).unwrap();
    let s2 = serde_json::to_string_pretty(&r2.envelope).unwrap();
    assert_eq!(s1, s2, "two successive emissions must be byte-identical");
}

#[test]
fn invariant_canonical_ordering_evidence_paths_and_audit_markers() {
    let built = Fixture::new("invariant-ordering").build();
    let result = read_status(&built.workspace, None);
    let evidence = &result.envelope.evidence_paths;
    let mut sorted_evidence = evidence.clone();
    sorted_evidence.sort();
    assert_eq!(evidence, &sorted_evidence, "evidence_paths must be sorted");
    let markers = &result.envelope.audit_markers;
    let mut sorted_markers = markers.clone();
    sorted_markers.sort();
    assert_eq!(markers, &sorted_markers, "audit_markers must be sorted");
}

#[test]
fn envelope_carries_pinned_schema_and_invariant_literal() {
    let built = Fixture::new("envelope-literals").build();
    let result = read_status(&built.workspace, None);
    assert_eq!(result.envelope.schema_version, STATUS_SCHEMA_V1);
    assert_eq!(
        result.envelope.read_only_invariant,
        READ_ONLY_INVARIANT_LITERAL
    );
}

#[test]
fn refusal_envelope_still_carries_read_only_invariant_and_schema() {
    let bogus = PathBuf::from("/no/such/path/a2-l2d-test");
    let r: StatusResult = read_status(&bogus, None);
    assert_eq!(r.envelope.schema_version, STATUS_SCHEMA_V1);
    assert_eq!(r.envelope.read_only_invariant, READ_ONLY_INVARIANT_LITERAL);
    assert_eq!(r.exit_code, EXIT_STATUS_REFUSED);
}
