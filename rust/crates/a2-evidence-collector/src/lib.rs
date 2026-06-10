//! A2 Tier 3 read-only evidence collector (library).
//!
//! Realizes `docs/a2-tier3-evidence-collector-design.md` within the bounds of
//! `docs/a2-tier3-evidence-collector-impl-scope-card.md`. This crate is a **pure
//! read-only observer**: it reads git state and a worktree `.claw` artifact tree,
//! computes statuses, and produces one immutable `a2-tier3-evidence-snapshot.v0`
//! object. It never runs claw, never runs the orchestrator, never approves, never
//! writes a target, and never calls a model / broker / runtime / Vault.
//!
//! All filesystem access here is read-only. The crate opens no write-capable
//! handle and spawns no mutating process; the only process calls (in `git`) use
//! read-only verbs (see [`gather_git_state`]).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Pinned snapshot schema version (scope card §9).
pub const SNAPSHOT_SCHEMA_VERSION: &str = "a2-tier3-evidence-snapshot.v0";

/// Fixed evidence-classification statuses (contract §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    ReadyWithNotes,
    Blocked,
    Partial,
    Stale,
    Unknown,
    DoNotRun,
}

impl Status {
    /// Contract-canonical wire string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ready => "READY",
            Status::ReadyWithNotes => "READY_WITH_NOTES",
            Status::Blocked => "BLOCKED",
            Status::Partial => "PARTIAL",
            Status::Stale => "STALE",
            Status::Unknown => "UNKNOWN",
            Status::DoNotRun => "DO_NOT_RUN",
        }
    }
}

/// How the apply outcome is evidenced (contract field `apply_result_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResultMode {
    StdoutOnly,
    PersistedFile,
    Unknown,
}

impl ApplyResultMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyResultMode::StdoutOnly => "stdout_only",
            ApplyResultMode::PersistedFile => "persisted_file",
            ApplyResultMode::Unknown => "unknown",
        }
    }
}

/// Fixed next-safe-action labels (contract §10).
pub mod next_action {
    pub const REVIEW_EVIDENCE: &str = "Review evidence";
    pub const OPEN_RUNBOOK: &str = "Open runbook";
    pub const RUN_OPERATOR_SMOKE: &str = "Run operator-terminal smoke";
    pub const DO_NOT_RUN_INCOMPLETE: &str = "Do not run — evidence incomplete";
    pub const CLEANUP_NEEDS_APPROVAL: &str = "Cleanup requires explicit operator approval";
    pub const PROCEED_COLLECTOR_DESIGN: &str = "Proceed to read-only collector design";
}

/// Read-only git observations of the control checkout.
///
/// `dirty` is `None` when cleanliness could not be observed. `captured_base` is
/// the base commit the named-worktree evidence was recorded against (when known);
/// `current_origin_main` is the local `origin/main` tip. The collector performs no
/// network fetch — it compares against whatever ref already exists locally.
#[derive(Debug, Clone, Default)]
pub struct GitState {
    pub dirty: Option<bool>,
    pub current_origin_main: Option<String>,
    pub captured_base: Option<String>,
    /// Completeness of each smoke worktree (`true` == complete success set).
    pub smoke_worktree_completeness: Vec<bool>,
}

/// Read-only scan of one worktree's `.claw` evidence tree.
///
/// This is a flat artifact-presence record; the several booleans each mark one
/// independent artifact, so a single struct is the clearest representation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeEvidence {
    pub approval_result_path: Option<String>,
    pub apply_bundle_path: Option<String>,
    pub preview_bundle_present: bool,
    pub preview_generator_present: bool,
    pub checkpoint_manifest_path: Option<String>,
    pub run_manifest_present: bool,
    pub status_present: bool,
    pub after_bin_present: bool,
    pub payload_sha256: Option<String>,
    pub last_written_file: Option<String>,
    /// `Some(true)` when the written file's recomputed sha256 matches `after.sha256`.
    pub written_file_sha_matches: Option<bool>,
    pub persisted_apply_result: bool,
}

impl WorktreeEvidence {
    /// A "complete success artifact set" per collector design §6 / scope card §9.
    #[must_use]
    pub fn is_complete_success(&self) -> bool {
        self.approval_result_path.is_some()
            && self.apply_bundle_path.is_some()
            && self.preview_bundle_present
            && self.preview_generator_present
            && self.checkpoint_manifest_path.is_some()
            && self.run_manifest_present
            && self.status_present
            && self.after_bin_present
            && self.payload_sha256.is_some()
            && self.last_written_file.is_some()
            && self.written_file_sha_matches == Some(true)
    }

    /// Whether any success artifact at all is present (distinguishes PARTIAL from UNKNOWN).
    #[must_use]
    pub fn any_artifact_present(&self) -> bool {
        self.approval_result_path.is_some()
            || self.apply_bundle_path.is_some()
            || self.preview_bundle_present
            || self.preview_generator_present
            || self.checkpoint_manifest_path.is_some()
            || self.run_manifest_present
            || self.status_present
            || self.after_bin_present
            || self.payload_sha256.is_some()
    }

    /// Apply-result mode for this worktree (contract field).
    #[must_use]
    pub fn apply_result_mode(&self) -> ApplyResultMode {
        if self.persisted_apply_result {
            ApplyResultMode::PersistedFile
        } else if self.is_complete_success() {
            ApplyResultMode::StdoutOnly
        } else {
            ApplyResultMode::Unknown
        }
    }
}

// ---------------------------------------------------------------------------
// Pure classifiers
// ---------------------------------------------------------------------------

/// Canonical-success status for a scanned worktree.
#[must_use]
pub fn canonical_status(ev: &WorktreeEvidence) -> Status {
    if ev.is_complete_success() {
        if ev.persisted_apply_result {
            Status::Ready
        } else {
            Status::ReadyWithNotes
        }
    } else if ev.any_artifact_present() {
        Status::Partial
    } else {
        Status::Unknown
    }
}

/// Control-checkout status from a read-only cleanliness observation.
#[must_use]
pub fn control_checkout_status(dirty: Option<bool>) -> Status {
    match dirty {
        Some(false) => Status::Ready,
        Some(true) => Status::Blocked,
        None => Status::Unknown,
    }
}

/// Freshness status: STALE when the captured base differs from current origin/main.
#[must_use]
pub fn freshness_status(captured_base: Option<&str>, current_origin_main: Option<&str>) -> Status {
    match (captured_base, current_origin_main) {
        (Some(base), Some(cur)) if base == cur => Status::Ready,
        (Some(_), Some(_)) => Status::Stale,
        _ => Status::Unknown,
    }
}

/// Count of partial (non-complete) smoke worktrees.
#[must_use]
pub fn partial_smoke_count(completeness: &[bool]) -> usize {
    completeness.iter().filter(|complete| !**complete).count()
}

/// Roll-up `tier3_status` from the per-subject statuses (contract §6 precedence).
#[must_use]
pub fn rollup_tier3_status(control: Status, freshness: Status, canonical: Status) -> Status {
    if control == Status::Blocked {
        return Status::Blocked;
    }
    if freshness == Status::Stale {
        return Status::Stale;
    }
    canonical
}

/// Map the roll-up status to one of the contract §10 fixed labels.
#[must_use]
pub fn next_safe_action(tier3: Status) -> &'static str {
    match tier3 {
        Status::Ready | Status::ReadyWithNotes | Status::Unknown => next_action::REVIEW_EVIDENCE,
        Status::Stale | Status::Blocked | Status::Partial | Status::DoNotRun => {
            next_action::DO_NOT_RUN_INCOMPLETE
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot (serializable, deterministic)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GeneratedFrom {
    pub control_checkout: String,
    pub named_worktree: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SubjectStatus {
    pub subject: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct Links {
    pub closure_doc: String,
    pub runbook: String,
}

#[derive(Debug, Serialize)]
pub struct Fields {
    pub last_successful_smoke_at: Option<String>,
    pub canonical_success_worktree: Option<String>,
    pub last_written_file: Option<String>,
    pub approval_result_path: Option<String>,
    pub apply_bundle_path: Option<String>,
    pub checkpoint_manifest_path: Option<String>,
    pub payload_sha256: Option<String>,
    pub apply_result_mode: String,
    pub control_checkout_status: String,
    pub partial_smoke_count: usize,
    pub next_safe_action: String,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub schema_version: String,
    pub generated_from: GeneratedFrom,
    pub tier3_status: String,
    pub fields: Fields,
    pub subjects: Vec<SubjectStatus>,
    pub links: Links,
    pub caveats: Vec<String>,
}

/// Build the immutable snapshot from observed git state and an optional scanned
/// worktree evidence set. Pure: deterministic for identical inputs.
#[must_use]
pub fn build_snapshot(
    control_checkout: &str,
    named_worktree: Option<&str>,
    git: &GitState,
    ev: Option<&WorktreeEvidence>,
    last_successful_smoke_at: Option<String>,
) -> Snapshot {
    let control = control_checkout_status(git.dirty);
    let freshness = freshness_status(
        git.captured_base.as_deref(),
        git.current_origin_main.as_deref(),
    );
    let canonical = ev.map_or(Status::Unknown, canonical_status);
    let tier3 = rollup_tier3_status(control, freshness, canonical);

    let blocked_reason = if control == Status::Blocked {
        Some("control checkout dirty".to_string())
    } else {
        None
    };

    let apply_result_mode = ev.map_or(
        ApplyResultMode::Unknown,
        WorktreeEvidence::apply_result_mode,
    );

    let mut caveats = Vec::new();
    if apply_result_mode == ApplyResultMode::StdoutOnly {
        caveats.push(
            "apply-result evidenced on stdout only; no persisted apply-result.json file on this build"
                .to_string(),
        );
    }
    if freshness == Status::Stale {
        caveats.push(
            "origin/main advanced past captured base; re-verify before trusting readiness"
                .to_string(),
        );
    }

    let canonical_success_worktree = match (named_worktree, canonical) {
        (Some(p), Status::Ready | Status::ReadyWithNotes) => Some(p.to_string()),
        _ => None,
    };

    let subjects = vec![
        SubjectStatus {
            subject: "control checkout".to_string(),
            status: control.as_str().to_string(),
        },
        SubjectStatus {
            subject: "approval gate".to_string(),
            status: Status::DoNotRun.as_str().to_string(),
        },
        SubjectStatus {
            subject: "canonical success evidence".to_string(),
            status: canonical.as_str().to_string(),
        },
        SubjectStatus {
            subject: "current disposable worktree".to_string(),
            status: freshness.as_str().to_string(),
        },
    ];

    let fields = Fields {
        last_successful_smoke_at,
        canonical_success_worktree,
        last_written_file: ev.and_then(|e| e.last_written_file.clone()),
        approval_result_path: ev.and_then(|e| e.approval_result_path.clone()),
        apply_bundle_path: ev.and_then(|e| e.apply_bundle_path.clone()),
        checkpoint_manifest_path: ev.and_then(|e| e.checkpoint_manifest_path.clone()),
        payload_sha256: ev.and_then(|e| e.payload_sha256.clone()),
        apply_result_mode: apply_result_mode.as_str().to_string(),
        control_checkout_status: control.as_str().to_string(),
        partial_smoke_count: partial_smoke_count(&git.smoke_worktree_completeness),
        next_safe_action: next_safe_action(tier3).to_string(),
        blocked_reason,
    };

    Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
        generated_from: GeneratedFrom {
            control_checkout: control_checkout.to_string(),
            named_worktree: named_worktree.map(ToString::to_string),
        },
        tier3_status: tier3.as_str().to_string(),
        fields,
        subjects,
        links: Links {
            closure_doc: "handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md"
                .to_string(),
            runbook: "handoffs/a2_tier3_orchestrator_live_smoke_runbook_2026-06-09.md".to_string(),
        },
        caveats,
    }
}

// ---------------------------------------------------------------------------
// Read-only filesystem scan
// ---------------------------------------------------------------------------

fn file_present(p: &Path) -> bool {
    p.is_file()
}

fn opt_path(p: &Path) -> Option<String> {
    if p.is_file() {
        Some(p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Lexicographically-greatest immediate sub-directory name under `dir` (ULIDs sort
/// by creation time, so this selects the most recent run/step deterministically).
fn latest_subdir(dir: &Path) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    entries.pop()
}

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

fn sha256_hex_of_file(p: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(p).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(format!("{:x}", hasher.finalize()))
}

fn target_relative_path_from_bundle(bundle: &Path) -> Option<String> {
    let text = std::fs::read_to_string(bundle).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Accept either a top-level field or a nested preview_record field.
    value
        .get("target_relative_path")
        .or_else(|| {
            value
                .get("preview_record")
                .and_then(|r| r.get("target_relative_path_sanitized"))
        })
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

/// Recursively check whether any `apply-result*.json` file exists under `dir`.
fn has_persisted_apply_result(dir: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if has_persisted_apply_result(&path) {
                return true;
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("json"));
            if name.starts_with("apply-result") && is_json {
                return true;
            }
        }
    }
    false
}

/// Read-only scan of a worktree's `.claw` evidence tree.
#[must_use]
pub fn scan_worktree(worktree: &Path) -> WorktreeEvidence {
    let claw = worktree.join(".claw");
    let mut ev = WorktreeEvidence {
        approval_result_path: opt_path(&claw.join("approval-result.json")),
        persisted_apply_result: has_persisted_apply_result(&claw),
        ..WorktreeEvidence::default()
    };

    // run id = latest dir under .claw/l2b-runs
    if let Some(run_dir) = latest_subdir(&claw.join("l2b-runs")) {
        ev.run_manifest_present = file_present(&run_dir.join("run-manifest.json"));
        ev.status_present = file_present(&run_dir.join("status.json"));
        let run_id = run_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());

        if let Some(run_id) = run_id {
            // step = latest dir under l2b-preview-bundles/<run>
            let pv_run = claw.join("l2b-preview-bundles").join(&run_id);
            if let Some(step_dir) = latest_subdir(&pv_run) {
                ev.preview_bundle_present = file_present(&step_dir.join("preview-bundle.json"));
                ev.preview_generator_present =
                    file_present(&step_dir.join("preview-generator-result.json"));
                ev.apply_bundle_path = opt_path(&step_dir.join("apply-bundle.json"));

                let step_id = step_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                if let Some(step_id) = step_id {
                    let cp = claw
                        .join("l2b-checkpoints")
                        .join(&run_id)
                        .join(&step_id)
                        .join("manifest.json");
                    ev.checkpoint_manifest_path = opt_path(&cp);

                    let payload = claw.join("l2b-payloads").join(&run_id).join(&step_id);
                    ev.after_bin_present = file_present(&payload.join("after.bin"));
                    ev.payload_sha256 = read_trimmed(&payload.join("after.sha256"));
                }

                // last written file from the apply-bundle, cross-checked against after.sha256
                if let Some(bundle) = ev.apply_bundle_path.as_deref() {
                    if let Some(rel) = target_relative_path_from_bundle(Path::new(bundle)) {
                        let written = worktree.join(&rel);
                        if written.is_file() {
                            ev.last_written_file = Some(rel);
                            if let (Some(recorded), Some(actual)) =
                                (ev.payload_sha256.as_deref(), sha256_hex_of_file(&written))
                            {
                                ev.written_file_sha_matches =
                                    Some(recorded.eq_ignore_ascii_case(&actual));
                            }
                        }
                    }
                }
            }
        }
    }

    ev
}

// ---------------------------------------------------------------------------
// Read-only git state (process calls use read-only verbs only)
// ---------------------------------------------------------------------------

fn git_capture(checkout: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// List Tier 3 smoke worktrees via the read-only `git worktree list --porcelain`,
/// filtered to paths whose final component names a Tier 3 live smoke worktree
/// (contains both `tier3` and `smoke`). Read-only; mutates nothing. The narrow
/// filter keeps `partial_smoke_count` scoped to Tier 3 evidence rather than every
/// unrelated `*-smoke-*` worktree in the repo.
#[must_use]
pub fn list_smoke_worktrees(checkout: &Path) -> Vec<PathBuf> {
    let Some(out) = git_capture(checkout, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("tier3") && n.contains("smoke"))
        })
        .collect()
}

/// Gather read-only git observations of the control checkout. Uses only read-only
/// verbs (`rev-parse`, `status --porcelain`); performs no fetch and mutates nothing.
#[must_use]
pub fn gather_git_state(checkout: &Path) -> GitState {
    let dirty = git_capture(checkout, &["status", "--porcelain"]).map(|s| !s.is_empty());
    let current_origin_main = git_capture(checkout, &["rev-parse", "origin/main"]);
    GitState {
        dirty,
        current_origin_main,
        captured_base: None,
        smoke_worktree_completeness: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const RUN: &str = "01KTQRQHAPZN9RANMT78MQ2B45";
    const STEP: &str = "write-smoke-notes";

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("a2-evcol-{nanos}-{seq}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn w(p: &Path, body: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    /// Build a worktree with a complete success artifact set. `persist_apply_result`
    /// optionally drops a persisted apply-result.json. Returns the worktree root.
    fn make_complete_worktree(persist_apply_result: bool) -> PathBuf {
        let wt = tmp();
        let claw = wt.join(".claw");
        let written = "SMOKE_NOTES.md";
        let body = b"smoke notes payload\n";
        let sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(body);
            format!("{:x}", h.finalize())
        };
        // the written file
        w(&wt.join(written), "smoke notes payload\n");
        // approval-result
        w(
            &claw.join("approval-result.json"),
            r#"{"decision":"approved"}"#,
        );
        // run
        w(
            &claw.join("l2b-runs").join(RUN).join("run-manifest.json"),
            "{}",
        );
        w(&claw.join("l2b-runs").join(RUN).join("status.json"), "{}");
        // preview bundles / step
        let step = claw.join("l2b-preview-bundles").join(RUN).join(STEP);
        w(&step.join("preview-bundle.json"), "{}");
        w(&step.join("preview-generator-result.json"), "{}");
        w(
            &step.join("apply-bundle.json"),
            r#"{"target_relative_path":"SMOKE_NOTES.md"}"#,
        );
        // checkpoint
        w(
            &claw
                .join("l2b-checkpoints")
                .join(RUN)
                .join(STEP)
                .join("manifest.json"),
            "{}",
        );
        // payload
        let payload = claw.join("l2b-payloads").join(RUN).join(STEP);
        w(&payload.join("after.bin"), "smoke notes payload\n");
        w(&payload.join("after.sha256"), &format!("{sha}\n"));
        if persist_apply_result {
            w(&step.join("apply-result.json"), r#"{"outcome":"applied"}"#);
        }
        wt
    }

    // T1 — complete set, no persisted apply-result -> READY_WITH_NOTES + stdout_only
    #[test]
    fn t1_complete_no_persisted_is_ready_with_notes_stdout_only() {
        let wt = make_complete_worktree(false);
        let ev = scan_worktree(&wt);
        assert!(ev.is_complete_success(), "evidence: {ev:?}");
        assert_eq!(canonical_status(&ev), Status::ReadyWithNotes);
        assert_eq!(ev.apply_result_mode(), ApplyResultMode::StdoutOnly);
    }

    // T2 — complete set + persisted apply-result -> READY + persisted_file
    #[test]
    fn t2_complete_with_persisted_is_ready_persisted_file() {
        let wt = make_complete_worktree(true);
        let ev = scan_worktree(&wt);
        assert!(ev.is_complete_success());
        assert_eq!(canonical_status(&ev), Status::Ready);
        assert_eq!(ev.apply_result_mode(), ApplyResultMode::PersistedFile);
    }

    // T3 — missing approval-result.json -> PARTIAL, approval_result_path None
    #[test]
    fn t3_missing_approval_result_is_partial() {
        let wt = make_complete_worktree(false);
        fs::remove_file(wt.join(".claw").join("approval-result.json")).unwrap();
        let ev = scan_worktree(&wt);
        assert_eq!(ev.approval_result_path, None);
        assert!(!ev.is_complete_success());
        assert_eq!(canonical_status(&ev), Status::Partial);
    }

    // T4 — missing apply-bundle.json -> PARTIAL
    #[test]
    fn t4_missing_apply_bundle_is_partial() {
        let wt = make_complete_worktree(false);
        fs::remove_file(
            wt.join(".claw")
                .join("l2b-preview-bundles")
                .join(RUN)
                .join(STEP)
                .join("apply-bundle.json"),
        )
        .unwrap();
        let ev = scan_worktree(&wt);
        assert_eq!(ev.apply_bundle_path, None);
        assert_eq!(canonical_status(&ev), Status::Partial);
    }

    // T5 — malformed apply-bundle JSON -> last_written_file UNKNOWN, no crash, PARTIAL
    #[test]
    fn t5_malformed_bundle_yields_unknown_written_file_no_crash() {
        let wt = make_complete_worktree(false);
        w(
            &wt.join(".claw")
                .join("l2b-preview-bundles")
                .join(RUN)
                .join(STEP)
                .join("apply-bundle.json"),
            "{ not json",
        );
        let ev = scan_worktree(&wt);
        assert_eq!(ev.last_written_file, None);
        assert!(!ev.is_complete_success());
        assert_eq!(canonical_status(&ev), Status::Partial);
    }

    // T6 — control checkout dirty -> BLOCKED + blocked_reason
    #[test]
    fn t6_control_dirty_is_blocked_with_reason() {
        assert_eq!(control_checkout_status(Some(true)), Status::Blocked);
        let git = GitState {
            dirty: Some(true),
            ..GitState::default()
        };
        let snap = build_snapshot("/ctl", None, &git, None, None);
        assert_eq!(snap.tier3_status, "BLOCKED");
        assert_eq!(
            snap.fields.blocked_reason.as_deref(),
            Some("control checkout dirty")
        );
    }

    // T7 — origin/main advanced past captured base -> STALE
    #[test]
    fn t7_origin_main_drift_is_stale() {
        assert_eq!(freshness_status(Some("aaa"), Some("bbb")), Status::Stale);
        assert_eq!(freshness_status(Some("aaa"), Some("aaa")), Status::Ready);
        let git = GitState {
            dirty: Some(false),
            current_origin_main: Some("bbb".to_string()),
            captured_base: Some("aaa".to_string()),
            ..GitState::default()
        };
        let snap = build_snapshot("/ctl", Some("/wt"), &git, None, None);
        assert_eq!(snap.tier3_status, "STALE");
    }

    // T8 — no canonical worktree present -> UNKNOWN canonical; partial count still computed
    #[test]
    fn t8_no_canonical_worktree_unknown_but_partial_count_computed() {
        let empty = tmp(); // no .claw
        let ev = scan_worktree(&empty);
        assert!(!ev.any_artifact_present());
        assert_eq!(canonical_status(&ev), Status::Unknown);
        assert_eq!(partial_smoke_count(&[false, false, true]), 2);
    }

    // T9 — snapshot determinism: identical inputs -> identical bytes
    #[test]
    fn t9_snapshot_is_deterministic() {
        let wt = make_complete_worktree(false);
        let ev = scan_worktree(&wt);
        let git = GitState {
            dirty: Some(false),
            current_origin_main: Some("aaa".to_string()),
            captured_base: Some("aaa".to_string()),
            smoke_worktree_completeness: vec![true, false],
        };
        let a = serde_json::to_string(&build_snapshot(
            "/ctl",
            Some(wt.to_str().unwrap()),
            &git,
            Some(&ev),
            Some("2026-06-09".to_string()),
        ))
        .unwrap();
        let b = serde_json::to_string(&build_snapshot(
            "/ctl",
            Some(wt.to_str().unwrap()),
            &git,
            Some(&ev),
            Some("2026-06-09".to_string()),
        ))
        .unwrap();
        assert_eq!(a, b);
        assert!(a.contains("\"schema_version\":\"a2-tier3-evidence-snapshot.v0\""));
        assert!(a.contains("\"tier3_status\":\"READY_WITH_NOTES\""));
    }

    // T10 — next_safe_action only ever emits fixed contract labels
    #[test]
    fn t10_next_safe_action_uses_only_fixed_labels() {
        let allowed = [
            next_action::REVIEW_EVIDENCE,
            next_action::OPEN_RUNBOOK,
            next_action::RUN_OPERATOR_SMOKE,
            next_action::DO_NOT_RUN_INCOMPLETE,
            next_action::CLEANUP_NEEDS_APPROVAL,
            next_action::PROCEED_COLLECTOR_DESIGN,
        ];
        for s in [
            Status::Ready,
            Status::ReadyWithNotes,
            Status::Blocked,
            Status::Partial,
            Status::Stale,
            Status::Unknown,
            Status::DoNotRun,
        ] {
            assert!(
                allowed.contains(&next_safe_action(s)),
                "label for {s:?} not in fixed set"
            );
        }
    }
}
