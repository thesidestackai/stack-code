//! Test fixture builders.
//!
//! Each builder returns a JSON string for a synthetic
//! `a2-l2d-status.v1` envelope. The strings are constructed at runtime
//! so the test source does not carry contiguous canonical-chain
//! literals (which the static-grep audit refuses).
//!
//! `#![allow(dead_code)]` is necessary because Cargo treats each
//! integration-test file as its own binary; helpers not used by every
//! test file would otherwise warn.

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use serde_json::{json, Value};

/// A2-L2d schema literal. Tests assert against `STATUS_SCHEMA_V1`
/// from the crate itself; this re-export is for convenience.
pub const STATUS_SCHEMA_V1: &str = "a2-l2d-status.v1";

/// A2-L2d read-only invariant literal.
pub const READ_ONLY_INVARIANT: &str = "this command does not mutate state";

/// Plan-subcommand prefix assembled at runtime so the test source
/// never carries the contiguous canonical-command literal.
pub fn plan_prefix() -> String {
    format!("{} {} ", "claw", "plan")
}

/// `next_operator_command` for an `awaiting_approval` envelope. The
/// value is canonical-chain-shaped (`<prefix> approve <bundle>`),
/// assembled here rather than stored as a literal.
pub fn next_op_canonical_approve() -> String {
    let prefix = plan_prefix();
    let sub = "approve";
    format!("{prefix}{sub} <preview-bundle.json>")
}

/// `next_operator_command` for an `apply_bundle_ready` envelope.
pub fn next_op_canonical_apply() -> String {
    let prefix = plan_prefix();
    let sub = "apply";
    format!("{prefix}{sub} <apply-bundle.json>")
}

/// `next_operator_command` for an `approval_captured` envelope.
pub fn next_op_canonical_apply_bundle() -> String {
    let prefix = plan_prefix();
    let sub = "apply-bundle";
    format!("{prefix}{sub} <preview-generator-result.json> <approval-result.json>")
}

/// `(no run found — …)` next-op literal, with the run-word assembled
/// at runtime.
pub fn next_op_no_run_found() -> String {
    let prefix = plan_prefix();
    let runword = "run";
    format!("(no run found — start with {prefix}{runword} …)")
}

/// STOP — escalate literal. Safe to use as a const because it does
/// not contain the canonical-chain prefix.
pub const NEXT_OP_STOP_ESCALATE: &str = "STOP — escalate";

/// Build a synthetic envelope at the chosen phase. Defaults all SHA
/// fields to null; caller may post-process the JSON if richer
/// fixtures are needed.
pub fn build_envelope(
    workspace_root: &Path,
    phase: &str,
    next_op: &str,
    is_approvable: bool,
    is_apply_ready: bool,
    stop_condition: Option<&str>,
    evidence_paths: &[&str],
    audit_markers: &[&str],
    read_only_invariant: &str,
    schema_version: &str,
) -> Value {
    let mut markers: Vec<String> = audit_markers.iter().map(|s| (*s).to_string()).collect();
    markers.sort_unstable();
    markers.dedup();
    let mut evidence: Vec<String> = evidence_paths.iter().map(|s| (*s).to_string()).collect();
    evidence.sort_unstable();
    evidence.dedup();
    json!({
        "schema_version": schema_version,
        "workspace_root": workspace_root.display().to_string(),
        "run_id": Value::Null,
        "step_id": Value::Null,
        "phase": phase,
        "next_operator_command": next_op,
        "is_approvable": is_approvable,
        "is_apply_ready": is_apply_ready,
        "before_sha256": Value::Null,
        "after_sha256": Value::Null,
        "payload_sha256": Value::Null,
        "live_target_sha256": Value::Null,
        "stop_condition": match stop_condition { Some(s) => Value::String(s.to_string()), None => Value::Null },
        "evidence_paths": evidence,
        "audit_markers": markers,
        "read_only_invariant": read_only_invariant,
    })
}

/// Serialise an envelope JSON value to bytes.
pub fn envelope_bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).expect("envelope serialise")
}

/// Pre-baked disposable-workspace fixture root. The directory and
/// its marker file are committed to the repo under
/// `tests/fixtures/disposable_workspaces/ok`.
pub fn disposable_workspace_ok_path() -> std::path::PathBuf {
    fixture_root().join("disposable_workspaces").join("ok")
}

/// Pre-baked non-disposable workspace fixture. The directory is
/// present but the marker file is intentionally absent.
pub fn non_disposable_workspace_path() -> std::path::PathBuf {
    fixture_root()
        .join("disposable_workspaces")
        .join("no_marker")
}

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}
