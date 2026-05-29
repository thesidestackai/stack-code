//! A2-L2d Read-Only Artifact Inspector / Status Contract.
//!
//! Pure read function over the A2-L2b artifact tree under
//! `<workspace>/.claw/l2b-*`. Emits an `a2-l2d-status.v1` envelope on
//! stdout that aggregates state from existing artifacts, identifies the
//! latest run / pending step, names the next allowed operator command,
//! surfaces any active STOP condition, and is byte-identical on two
//! successive calls against an unchanged workspace.
//!
//! # Hard contract
//!
//! - NEVER mutates the filesystem. No `fs::write`, no `fs::create_dir`,
//!   no `fs::remove*`, no `fs::rename`, no `fs::set_permissions`, no
//!   `File::create`, no `OpenOptions::write/append/create*`.
//! - NEVER makes a network call. No broker, model, Ollama, telemetry,
//!   or any other endpoint.
//! - NEVER spawns a subprocess.
//! - NEVER reads outside `<workspace>/.claw/**` or the live target
//!   resolved from the preview record, EXCEPT for one operator-supplied
//!   approval-result JSON path passed as a positional argument. That
//!   explicit operator path is on a distinct code branch from automatic
//!   discovery so the two read sources are never conflated.
//! - NEVER surfaces raw binary payload bytes. Only SHA-256 digests.
//! - Output is deterministic: `evidence_paths` and `audit_markers` are
//!   sorted lexicographically; JSON keys follow the schema-doc order.
//! - Approval / apply gates are out of scope. This module describes
//!   state; it does not authorize or execute anything.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diff_preview::PreviewRecord;

/// Pinned schema version for the status envelope. Bumping requires a
/// separate scope-card amendment.
pub const STATUS_SCHEMA_V1: &str = "a2-l2d-status.v1";

/// Pinned read-only invariant literal, present on every emission.
/// Operators reading raw stdout get a one-line reassurance that the
/// command did not mutate state.
pub const READ_ONLY_INVARIANT_LITERAL: &str = "this command does not mutate state";

/// Exit code emitted when `claw plan status` refuses to produce a
/// state envelope (invalid workspace root, unreadable artifact tree).
///
/// Collision audit (2026-05-29 / origin/main @ `12fff14`): the runtime
/// uses 0 (`WRITE_APPLIED`), 5 (`PARSE_ERROR` / `INVALID_REQUEST` /
/// preview-refused / apply-bundle-rejected), 6 (`WRITE_PATH_REFUSED`),
/// 7 (`APPROVAL_DENIED` / `APPROVAL_REFUSED` /
/// `RUN_PLAN_WRITE_PREVIEW_READY`), 8 (`ROLLBACK_FAILED`), 9
/// (`BASELINE_MISMATCH` / `CHECKPOINT_FAILED`), 10 (`WRITE_IO_FAILED`),
/// 11 (`VALIDATION_ROLLED_BACK`). `12` is unused and outside the L2b
/// cluster.
pub const EXIT_STATUS_REFUSED: i32 = 12;

/// Closed set of A2-L2d audit markers. New markers require a separate
/// scope-card amendment.
pub const MARKER_STATUS_READ: &str = "a2-l2d-status-read";
pub const MARKER_NO_RUN_FOUND: &str = "a2-l2d-status-no-run-found";
pub const MARKER_NON_APPROVABLE: &str = "a2-l2d-status-non-approvable";
pub const MARKER_STOP_CONDITION_DETECTED: &str = "a2-l2d-status-stop-condition-detected";
pub const MARKER_IDEMPOTENT_EMIT: &str = "a2-l2d-status-idempotent-emit";
pub const MARKER_REFUSED: &str = "a2-l2d-status-refused";

/// Pinned L2b approval-result schema literal that `claw plan approve`
/// emits on stdout. Compared lexically; nothing here trusts schema-tag
/// authority — every authority check is a SHA / step-id binding.
const APPROVAL_RESULT_SCHEMA_V1: &str = "a2-l2b-approval-result.v1";

/// Pinned L2b apply-bundle schema literal. The on-disk
/// `apply-bundle.json` MUST carry this exact value or the lane STOPs
/// with `apply-bundle-schema-mismatch`.
const APPLY_BUNDLE_SCHEMA_V1: &str = "a2-l2b-apply-bundle.v1";

// Workspace-relative roots owned by A2-L2b. These are the ONLY paths
// the automatic discovery branch is allowed to read.
const RUNS_REL: &str = ".claw/l2b-runs";
const PREVIEW_BUNDLES_REL: &str = ".claw/l2b-preview-bundles";
const CHECKPOINTS_REL: &str = ".claw/l2b-checkpoints";
const PAYLOADS_REL: &str = ".claw/l2b-payloads";

/// Closed `phase` enum. The 9 values mirror scope-card section 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    NoRunFound,
    PreviewReady,
    AwaitingApproval,
    ApprovalCaptured,
    ApplyBundleReady,
    Applied,
    RolledBack,
    NonApprovable,
    Unknown,
}

/// Closed `stop_condition` enum. Each variant maps to a named STOP
/// gate in the A2-L2b handoff section 8 or to a read-time STOP that
/// the status command can detect without mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopCondition {
    WorkspaceRootInvalid,
    RunManifestUnreadable,
    PreviewBundleUnreadable,
    PayloadShaMismatch,
    LiveTargetMissing,
    LiveTargetShaChanged,
    ApprovalDecisionNotApproved,
    ApprovalShaMismatch,
    ApprovalStepIdMismatch,
    ApplyBundleSchemaMismatch,
    ApplyBundleTargetPathMismatch,
}

/// A2-L2d status envelope. Field order is the canonical JSON key order
/// pinned by `docs/a2-l2d-status-schema.md`. Changing order requires a
/// schema bump and a separate scope-card amendment.
#[derive(Debug, Clone, Serialize)]
pub struct StatusEnvelope {
    pub schema_version: &'static str,
    pub workspace_root: String,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub phase: Phase,
    pub next_operator_command: String,
    pub is_approvable: bool,
    pub is_apply_ready: bool,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
    pub payload_sha256: Option<String>,
    pub live_target_sha256: Option<String>,
    pub stop_condition: Option<StopCondition>,
    pub evidence_paths: Vec<String>,
    pub audit_markers: Vec<String>,
    pub read_only_invariant: &'static str,
}

/// Combined envelope + recommended exit code. The CLI layer maps the
/// exit code to a process exit; library callers may also inspect it.
#[derive(Debug, Clone)]
pub struct StatusResult {
    pub envelope: StatusEnvelope,
    pub exit_code: i32,
}

#[derive(Debug, Deserialize)]
struct RunManifestSubset {
    pending_step_id: String,
}

#[derive(Debug, Deserialize)]
struct PreviewBundleSubset {
    schema_version: String,
    preview_record: PreviewRecord,
}

#[derive(Debug, Deserialize)]
struct ApplyBundleSubset {
    schema_version: String,
    target_relative_path: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalResultSubset {
    schema_version: String,
    decision: String,
    step_id: String,
    preview_sha256: String,
}

/// Read the workspace and emit an A2-L2d status envelope. Reads only;
/// never mutates the workspace or any other filesystem state.
///
/// Reads scoped to:
/// - `<workspace>/.claw/l2b-*/**`
/// - the live target file referenced by the preview record (SHA only)
/// - `approval_result` (if `Some(...)`), supplied by the operator as
///   the only permitted read outside `<workspace>/.claw/**`
///
/// On any unrecoverable refusal (workspace root invalid, manifest
/// unreadable, preview bundle unreadable), the result still carries a
/// valid `a2-l2d-status.v1` envelope with `stop_condition` set and
/// `exit_code == EXIT_STATUS_REFUSED`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn read_status(workspace_root: &Path, approval_result: Option<&Path>) -> StatusResult {
    let Ok(canonical_root) = fs::canonicalize(workspace_root) else {
        return refusal(workspace_root, StopCondition::WorkspaceRootInvalid);
    };
    if !canonical_root.is_dir() {
        return refusal(workspace_root, StopCondition::WorkspaceRootInvalid);
    }

    let mut evidence: Vec<PathBuf> = Vec::new();
    let mut markers: Vec<String> = vec![MARKER_STATUS_READ.to_string()];

    let runs_root = canonical_root.join(RUNS_REL);
    let latest_run = match latest_run_id(&runs_root) {
        Ok(Some(run_id)) => run_id,
        Ok(None) => {
            markers.push(MARKER_NO_RUN_FOUND.to_string());
            markers.push(MARKER_IDEMPOTENT_EMIT.to_string());
            return StatusResult {
                envelope: build_envelope(
                    &canonical_root,
                    None,
                    None,
                    Phase::NoRunFound,
                    "(no run found — start with claw plan run …)".to_string(),
                    false,
                    false,
                    None,
                    None,
                    None,
                    None,
                    None,
                    evidence,
                    markers,
                ),
                exit_code: 0,
            };
        }
        Err(_) => return refusal(workspace_root, StopCondition::WorkspaceRootInvalid),
    };

    let latest_run_dir = runs_root.join(&latest_run);
    let run_manifest_path = latest_run_dir.join("run-manifest.json");
    let run_manifest: RunManifestSubset = match read_json_subset(&run_manifest_path) {
        Ok(m) => {
            evidence.push(run_manifest_path.clone());
            m
        }
        Err(_) => return refusal(workspace_root, StopCondition::RunManifestUnreadable),
    };
    let status_json_path = latest_run_dir.join("status.json");
    if status_json_path.is_file() {
        evidence.push(status_json_path);
    }

    let step_id = run_manifest.pending_step_id;
    let step_dir = canonical_root
        .join(PREVIEW_BUNDLES_REL)
        .join(&latest_run)
        .join(&step_id);
    let preview_bundle_path = step_dir.join("preview-bundle.json");
    let preview_subset: PreviewBundleSubset = match read_json_subset(&preview_bundle_path) {
        Ok(b) => {
            evidence.push(preview_bundle_path.clone());
            b
        }
        Err(_) => return refusal(workspace_root, StopCondition::PreviewBundleUnreadable),
    };
    if preview_subset.schema_version != crate::write_preview::PREVIEW_BUNDLE_SCHEMA_V1 {
        return refusal(workspace_root, StopCondition::PreviewBundleUnreadable);
    }
    let preview_record = preview_subset.preview_record;

    let payload_sha_path = canonical_root
        .join(PAYLOADS_REL)
        .join(&latest_run)
        .join(&step_id)
        .join("after.sha256");
    let payload_sha_on_disk: Option<String> = if payload_sha_path.is_file() {
        evidence.push(payload_sha_path.clone());
        fs::read_to_string(&payload_sha_path)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    let preview_gen_result_path = step_dir.join("preview-generator-result.json");
    if preview_gen_result_path.is_file() {
        evidence.push(preview_gen_result_path);
    }
    let checkpoint_manifest_path = canonical_root
        .join(CHECKPOINTS_REL)
        .join(&latest_run)
        .join(&step_id)
        .join("manifest.json");
    if checkpoint_manifest_path.is_file() {
        evidence.push(checkpoint_manifest_path);
    }

    // Optional explicit operator-supplied approval result. This is the
    // ONLY permitted read outside <workspace>/.claw/**. The function
    // is on a distinct code branch from `read_json_subset` so the two
    // read sources cannot be conflated.
    let approval_state = approval_result
        .map(|path| read_explicit_approval_result(path, &preview_record, &mut evidence));

    let apply_bundle_path = step_dir.join("apply-bundle.json");
    let apply_bundle_state = if apply_bundle_path.is_file() {
        Some(read_apply_bundle(
            &apply_bundle_path,
            &preview_record,
            &mut evidence,
        ))
    } else {
        None
    };

    let live_target_rel = &preview_record.target_relative_path_sanitized;
    let live_target_abs = canonical_root.join(live_target_rel);
    let live_target_sha = if live_target_abs.is_file() {
        sha256_of_file(&live_target_abs).ok()
    } else {
        None
    };

    let mut stop: Option<StopCondition> = None;

    if let Some(ref disk_sha) = payload_sha_on_disk {
        if disk_sha != &preview_record.after_sha256 {
            stop = stop.or(Some(StopCondition::PayloadShaMismatch));
        }
    }

    if live_target_sha.is_none() && !preview_record.before_sha256.is_empty() {
        // Preview record records a non-empty pre-write SHA, so the
        // target existed at preview time. Missing now is a STOP.
        stop = stop.or(Some(StopCondition::LiveTargetMissing));
    }

    if let Some(ref sha) = live_target_sha {
        if sha != &preview_record.before_sha256 && sha != &preview_record.after_sha256 {
            stop = stop.or(Some(StopCondition::LiveTargetShaChanged));
        }
    }

    if let Some(ref approval) = approval_state {
        match approval {
            ApprovalState::DecisionNotApproved => {
                stop = stop.or(Some(StopCondition::ApprovalDecisionNotApproved));
            }
            ApprovalState::StepIdMismatch => {
                stop = stop.or(Some(StopCondition::ApprovalStepIdMismatch));
            }
            ApprovalState::ShaMismatch | ApprovalState::Unreadable => {
                stop = stop.or(Some(StopCondition::ApprovalShaMismatch));
            }
            ApprovalState::Approved => {}
        }
    }

    if let Some(ref bundle) = apply_bundle_state {
        match bundle {
            ApplyBundleState::SchemaMismatch => {
                stop = stop.or(Some(StopCondition::ApplyBundleSchemaMismatch));
            }
            ApplyBundleState::TargetPathMismatch => {
                stop = stop.or(Some(StopCondition::ApplyBundleTargetPathMismatch));
            }
            ApplyBundleState::Ready | ApplyBundleState::Unreadable => {}
        }
    }

    let is_approvable = preview_record.is_approvable();
    let apply_bundle_ready = matches!(apply_bundle_state, Some(ApplyBundleState::Ready));
    let payload_ok = payload_sha_on_disk
        .as_ref()
        .is_some_and(|s| s == &preview_record.after_sha256);
    let is_apply_ready = apply_bundle_ready && payload_ok && stop.is_none();

    let phase = derive_phase(
        stop.is_some(),
        is_approvable,
        apply_bundle_ready,
        approval_state,
        approval_result.is_some(),
        live_target_sha.as_deref(),
        &preview_record,
    );

    let next_command = if stop.is_some() {
        "STOP — escalate".to_string()
    } else {
        match phase {
            Phase::NoRunFound => "(no run found — start with claw plan run …)".to_string(),
            Phase::Applied => format!(
                "claw plan run <plan.yaml> --workspace-root {} --workspace-write-preview",
                canonical_root.display()
            ),
            Phase::ApplyBundleReady => format!(
                "claw plan apply {}",
                step_dir.join("apply-bundle.json").display()
            ),
            Phase::ApprovalCaptured => {
                let approval_path = approval_result.map_or_else(
                    || "<approval-result.json>".to_string(),
                    |p| p.display().to_string(),
                );
                format!(
                    "claw plan apply-bundle {} {}",
                    step_dir.join("preview-generator-result.json").display(),
                    approval_path
                )
            }
            Phase::AwaitingApproval | Phase::PreviewReady => format!(
                "claw plan approve {}",
                step_dir.join("preview-bundle.json").display()
            ),
            Phase::NonApprovable | Phase::RolledBack | Phase::Unknown => {
                "STOP — escalate".to_string()
            }
        }
    };

    if phase == Phase::NonApprovable {
        markers.push(MARKER_NON_APPROVABLE.to_string());
    }
    if stop.is_some() {
        markers.push(MARKER_STOP_CONDITION_DETECTED.to_string());
    }
    markers.push(MARKER_IDEMPOTENT_EMIT.to_string());

    StatusResult {
        envelope: build_envelope(
            &canonical_root,
            Some(latest_run),
            Some(step_id),
            phase,
            next_command,
            is_approvable,
            is_apply_ready,
            empty_to_none(&preview_record.before_sha256),
            empty_to_none(&preview_record.after_sha256),
            payload_sha_on_disk,
            live_target_sha,
            stop,
            evidence,
            markers,
        ),
        exit_code: 0,
    }
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn derive_phase(
    has_stop: bool,
    is_approvable: bool,
    apply_bundle_ready: bool,
    approval_state: Option<ApprovalState>,
    approval_supplied: bool,
    live_target_sha: Option<&str>,
    preview_record: &PreviewRecord,
) -> Phase {
    if has_stop {
        if !is_approvable {
            return Phase::NonApprovable;
        }
        return Phase::Unknown;
    }

    if let Some(sha) = live_target_sha {
        if sha == preview_record.after_sha256 {
            return Phase::Applied;
        }
        // `RolledBack` is filesystem-indistinguishable from
        // `ApplyBundleReady` because the L2b chain does not persist an
        // apply-result on disk. We bias to `RolledBack` only when the
        // operator has supplied an approved approval-result AND the
        // apply-bundle is on disk AND the live target still matches
        // the pre-write baseline. This is the operator's strongest
        // evidence that the chain did execute and then rolled back —
        // they would not otherwise have an approval-result in hand at
        // this point of the chain. Ambiguous cases default to
        // `ApplyBundleReady` so an operator who has not yet run apply
        // sees the next-best action as "apply", not "STOP".
        if sha == preview_record.before_sha256
            && apply_bundle_ready
            && matches!(approval_state, Some(ApprovalState::Approved))
        {
            return Phase::RolledBack;
        }
    }

    if !is_approvable {
        return Phase::NonApprovable;
    }
    if apply_bundle_ready {
        return Phase::ApplyBundleReady;
    }
    if matches!(approval_state, Some(ApprovalState::Approved)) {
        return Phase::ApprovalCaptured;
    }
    if approval_supplied {
        Phase::PreviewReady
    } else {
        Phase::AwaitingApproval
    }
}

fn empty_to_none(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
fn build_envelope(
    canonical_root: &Path,
    run_id: Option<String>,
    step_id: Option<String>,
    phase: Phase,
    next_operator_command: String,
    is_approvable: bool,
    is_apply_ready: bool,
    before_sha256: Option<String>,
    after_sha256: Option<String>,
    payload_sha256: Option<String>,
    live_target_sha256: Option<String>,
    stop_condition: Option<StopCondition>,
    evidence: Vec<PathBuf>,
    mut markers: Vec<String>,
) -> StatusEnvelope {
    let mut evidence_strings: Vec<String> = evidence
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    evidence_strings.sort();
    evidence_strings.dedup();
    markers.sort();
    markers.dedup();

    StatusEnvelope {
        schema_version: STATUS_SCHEMA_V1,
        workspace_root: canonical_root.to_string_lossy().to_string(),
        run_id,
        step_id,
        phase,
        next_operator_command,
        is_approvable,
        is_apply_ready,
        before_sha256,
        after_sha256,
        payload_sha256,
        live_target_sha256,
        stop_condition,
        evidence_paths: evidence_strings,
        audit_markers: markers,
        read_only_invariant: READ_ONLY_INVARIANT_LITERAL,
    }
}

fn refusal(workspace_root: &Path, stop: StopCondition) -> StatusResult {
    let mut markers = vec![
        MARKER_STATUS_READ.to_string(),
        MARKER_REFUSED.to_string(),
        MARKER_STOP_CONDITION_DETECTED.to_string(),
    ];
    markers.sort();
    StatusResult {
        envelope: StatusEnvelope {
            schema_version: STATUS_SCHEMA_V1,
            workspace_root: workspace_root.to_string_lossy().to_string(),
            run_id: None,
            step_id: None,
            phase: Phase::Unknown,
            next_operator_command: "STOP — escalate".to_string(),
            is_approvable: false,
            is_apply_ready: false,
            before_sha256: None,
            after_sha256: None,
            payload_sha256: None,
            live_target_sha256: None,
            stop_condition: Some(stop),
            evidence_paths: Vec::new(),
            audit_markers: markers,
            read_only_invariant: READ_ONLY_INVARIANT_LITERAL,
        },
        exit_code: EXIT_STATUS_REFUSED,
    }
}

fn latest_run_id(runs_dir: &Path) -> Result<Option<String>, std::io::Error> {
    if !runs_dir.is_dir() {
        return Ok(None);
    }
    let mut best: Option<(SystemTime, String)> = None;
    for entry in fs::read_dir(runs_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let manifest = entry.path().join("run-manifest.json");
        if !manifest.is_file() {
            continue;
        }
        let mtime = fs::metadata(&manifest)?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let name = entry.file_name().to_string_lossy().to_string();
        match &best {
            Some((bt, bn)) => {
                if mtime > *bt || (mtime == *bt && name > *bn) {
                    best = Some((mtime, name));
                }
            }
            None => best = Some((mtime, name)),
        }
    }
    Ok(best.map(|(_, name)| name))
}

fn read_json_subset<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ReadError> {
    let bytes = fs::read(path).map_err(|_| ReadError::Io)?;
    serde_json::from_slice(&bytes).map_err(|_| ReadError::Parse)
}

enum ReadError {
    Io,
    Parse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalState {
    Approved,
    DecisionNotApproved,
    StepIdMismatch,
    ShaMismatch,
    Unreadable,
}

fn read_explicit_approval_result(
    explicit_path: &Path,
    preview_record: &PreviewRecord,
    evidence: &mut Vec<PathBuf>,
) -> ApprovalState {
    // ONLY read outside <workspace>/.claw/**. Distinct from
    // `read_json_subset` so the two read sources are never conflated.
    // File is parsed read-only, included in evidence_paths, never
    // modified.
    let Ok(bytes) = fs::read(explicit_path) else {
        return ApprovalState::Unreadable;
    };
    evidence.push(explicit_path.to_path_buf());
    let Ok(parsed): Result<ApprovalResultSubset, _> = serde_json::from_slice(&bytes) else {
        return ApprovalState::Unreadable;
    };
    if parsed.schema_version != APPROVAL_RESULT_SCHEMA_V1 {
        return ApprovalState::Unreadable;
    }
    if parsed.decision != "approved" {
        return ApprovalState::DecisionNotApproved;
    }
    if parsed.step_id != preview_record.step_id {
        return ApprovalState::StepIdMismatch;
    }
    if parsed.preview_sha256 != preview_record.preview_sha256 {
        return ApprovalState::ShaMismatch;
    }
    ApprovalState::Approved
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyBundleState {
    Ready,
    SchemaMismatch,
    TargetPathMismatch,
    Unreadable,
}

fn read_apply_bundle(
    path: &Path,
    preview_record: &PreviewRecord,
    evidence: &mut Vec<PathBuf>,
) -> ApplyBundleState {
    let Ok(bytes) = fs::read(path) else {
        return ApplyBundleState::Unreadable;
    };
    evidence.push(path.to_path_buf());
    let Ok(parsed): Result<ApplyBundleSubset, _> = serde_json::from_slice(&bytes) else {
        return ApplyBundleState::Unreadable;
    };
    if parsed.schema_version != APPLY_BUNDLE_SCHEMA_V1 {
        return ApplyBundleState::SchemaMismatch;
    }
    if parsed.target_relative_path != preview_record.target_relative_path_sanitized {
        return ApplyBundleState::TargetPathMismatch;
    }
    ApplyBundleState::Ready
}

fn sha256_of_file(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
