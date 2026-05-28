//! A2-L2b run-plan write-preview integration tests.
//!
//! Exercises [`a2_plan_runner::run_plan_with_write_preview`] without
//! invoking the wrapper, the broker, or any subprocess. All tests are
//! either:
//!
//! - Read-only-only plans where the wrapper path is intentionally
//!   absent — the runner's pre-existing path emits a substrate-
//!   unavailable signal in that case, but for the write-preview path
//!   we never reach that code unless a prior read-only step exists.
//! - Workspace-write-only plans where the runner never spawns the
//!   wrapper at all (the lone write step halts at preview time).
//!
//! Hard contract verifications:
//!
//! - Read-only-only plans behave equivalently to [`a2_plan_runner::run_plan`].
//! - Workspace-write plans without the opt-in still fail through the
//!   strict read-only precheck (covered by existing tests; this file
//!   focuses on the OPT-IN path).
//! - Multi-write plans refuse BEFORE any step executes.
//! - The lone workspace-write step produces a preview bundle, payload,
//!   checkpoint, and run manifest — and NEVER mutates the target.
//! - Sequencing: read-only steps before the write step run; steps after
//!   are skipped.
//! - Runtime `after_file` refusals (missing, symlink, directory) refuse
//!   without leaving any state on the target.
//! - The result envelope NEVER contains a reference to
//!   `claw plan apply` or `claw plan apply-bundle`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use a2_plan_runner::runner::WritePreviewPlanRefusal;
use a2_plan_runner::{
    run_plan_with_write_preview, WritePreviewPlanStatus, EXIT_RUN_PLAN_WRITE_PREVIEW_READY,
    EXIT_RUN_PLAN_WRITE_PREVIEW_REFUSED,
};
use a2_plan_schema::parse_plan;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("a2-l2b-runplan-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("temp dir created");
    dir
}

fn missing_wrapper() -> PathBuf {
    PathBuf::from("/does/not/exist/claw-sidestack-local")
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

const TWO_WORKSPACE_WRITE_PLAN: &str = r"
name: multi-write
mode: read-only
model_tier: FAST
steps:
  - id: write-a
    description: first write
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/a.md
      create_if_absent: true
    after_file: materialized/a.after
  - id: write-b
    description: second write
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/b.md
      create_if_absent: true
    after_file: materialized/b.after
";

const READ_ONLY_THEN_WRITE_PLAN: &str = r"
name: read-then-write
mode: read-only
model_tier: FAST
steps:
  - id: ro
    description: read first
    tools: [Read]
  - id: write-it
    description: write the note
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/scratch.md
      create_if_absent: true
    after_file: materialized/notes_scratch.after
";

const WRITE_THEN_READ_ONLY_PLAN: &str = r"
name: write-then-read
mode: read-only
model_tier: FAST
steps:
  - id: write-it
    description: write the note
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/scratch.md
      create_if_absent: true
    after_file: materialized/notes_scratch.after
  - id: after
    description: should be skipped
    tools: [Read]
";

const REFUSED_DEEP_PLAN: &str = r"
name: refused-deep
mode: read-only
model_tier: FAST
steps:
  - id: s
    description: deep
    model_tier: DEEP
    tools: [Read]
";

const DISALLOWED_TOOL_ON_NONWRITE_PLAN: &str = r"
name: disallowed-on-ro
mode: read-only
model_tier: FAST
steps:
  - id: ro
    description: tries Edit
    tools: [Edit]
  - id: w
    description: write
    mode: workspace-write
    tools: [Write]
    write_target:
      path: notes/scratch.md
      create_if_absent: true
    after_file: materialized/notes_scratch.after
";

fn seed_after_file(ws: &Path, content: &[u8]) {
    let after_dir = ws.join("materialized");
    fs::create_dir_all(&after_dir).unwrap();
    fs::write(after_dir.join("notes_scratch.after"), content).unwrap();
}

// --- Read-only preservation -------------------------------------------------

#[test]
fn read_only_only_plan_without_write_step_delegates_to_read_only_path() {
    // No workspace-write step → behavior matches run_plan: precheck +
    // substrate probe would run, but our wrapper is missing so the
    // substrate-unavailable path fires (exit 4). This is the existing
    // L1b read-only contract — the opt-in must not change it.
    let plan = parse_plan(VALID_READONLY_PLAN).unwrap();
    let ws = unique_temp_dir("ro-no-write-step");
    let report = run_plan_with_write_preview(
        &plan,
        &missing_wrapper(),
        Some(("http://invalid.localhost:0", "qwen3:14b")),
        Duration::from_secs(1),
        &ws,
    );
    assert_eq!(report.write_step_count, 0);
    assert!(report.preview_artifacts.is_none());
    assert!(report.next_operator_command.is_none());
    // The read-only path is unchanged. Either ReadOnlyComplete or
    // ReadOnlyFailedBeforeWrite (substrate unavailable / step failure)
    // is acceptable; the opt-in must not synthesize a write-preview
    // state.
    assert!(matches!(
        report.status,
        WritePreviewPlanStatus::ReadOnlyComplete
            | WritePreviewPlanStatus::ReadOnlyFailedBeforeWrite
    ));
    fs::remove_dir_all(&ws).ok();
}

// --- Workspace-write happy path --------------------------------------------

#[test]
fn lone_workspace_write_step_produces_preview_bundle_and_halts() {
    let plan = parse_plan(SINGLE_WORKSPACE_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("lone-write");
    fs::create_dir_all(ws.join("notes")).unwrap();
    seed_after_file(&ws, b"hello\nworld\n");

    let report = run_plan_with_write_preview(
        &plan,
        &missing_wrapper(),
        None, // no substrate; no prior read-only step
        Duration::from_secs(1),
        &ws,
    );

    assert_eq!(report.status, WritePreviewPlanStatus::WritePreviewReady);
    assert_eq!(report.write_step_count, 1);
    assert_eq!(report.exit_code_hint, EXIT_RUN_PLAN_WRITE_PREVIEW_READY);
    let artifacts = report.preview_artifacts.expect("artifacts must be present");
    assert!(artifacts.preview_bundle_path.exists());
    assert!(artifacts.payload_path.exists());
    assert!(artifacts.checkpoint_manifest_path.exists());
    assert!(artifacts.run_manifest_path.exists());

    // Target was NOT created.
    assert!(
        !ws.join("notes/scratch.md").exists(),
        "preview must not mutate the target file"
    );

    // After-file was NOT mutated.
    let after_redux = fs::read(ws.join("materialized/notes_scratch.after")).unwrap();
    assert_eq!(after_redux, b"hello\nworld\n");

    // Marker stream contains the operator-facing halt + approval-pending.
    assert!(report
        .markers
        .iter()
        .any(|m| m == "a2-l2b-run-plan-write-preview-ready"));
    assert!(report
        .markers
        .iter()
        .any(|m| m == "a2-l2b-approval-pending"));
    assert!(report.markers.iter().any(|m| m == "a2-l2b-plan-halted"));

    // Next operator command points at approve — never apply.
    let next = report.next_operator_command.expect("next cmd populated");
    assert!(next.starts_with("claw plan approve "));
    assert!(!next.contains(" plan apply"));
    assert!(!next.contains("plan apply-bundle"));

    fs::remove_dir_all(&ws).ok();
}

#[test]
fn read_only_step_then_write_step_skips_write_step_substrate_probe_when_unreachable() {
    // The plan has one prior read-only step plus the lone write step.
    // We pass a deliberately-unreachable substrate URL; this should
    // surface as substrate-unavailable BEFORE any write-preview work
    // happens, preserving "no preview artifact produced" on failure.
    let plan = parse_plan(READ_ONLY_THEN_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("ro-then-write-no-subst");
    fs::create_dir_all(ws.join("notes")).unwrap();
    seed_after_file(&ws, b"x\n");

    let report = run_plan_with_write_preview(
        &plan,
        &missing_wrapper(),
        Some(("http://127.0.0.1:1/v1", "qwen3:14b")),
        Duration::from_secs(1),
        &ws,
    );
    // Substrate probe must fail (network unreachable). We expect the
    // ReadOnlyFailedBeforeWrite outcome with no preview artifacts.
    assert!(matches!(
        report.status,
        WritePreviewPlanStatus::ReadOnlyFailedBeforeWrite
    ));
    assert!(report.preview_artifacts.is_none());
    // Target was NOT created.
    assert!(!ws.join("notes/scratch.md").exists());
    fs::remove_dir_all(&ws).ok();
}

#[test]
fn step_after_workspace_write_is_skipped_not_executed() {
    let plan = parse_plan(WRITE_THEN_READ_ONLY_PLAN).unwrap();
    let ws = unique_temp_dir("write-then-after");
    fs::create_dir_all(ws.join("notes")).unwrap();
    seed_after_file(&ws, b"x\n");

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::WritePreviewReady);

    // The post-write step appears in step_reports as skipped.
    let after_step = report
        .step_reports
        .iter()
        .find(|sr| sr.step_id == "after")
        .expect("after step must appear in step reports");
    assert!(after_step
        .markers
        .iter()
        .any(|m| m == "a2-l1b-step-skipped"));
    fs::remove_dir_all(&ws).ok();
}

// --- Multi-write refusal ----------------------------------------------------

#[test]
fn multi_workspace_write_plan_refuses_before_any_step_executes() {
    let plan = parse_plan(TWO_WORKSPACE_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("multi-write");
    fs::create_dir_all(ws.join("notes")).unwrap();
    fs::create_dir_all(ws.join("materialized")).unwrap();
    fs::write(ws.join("materialized/a.after"), b"a\n").unwrap();
    fs::write(ws.join("materialized/b.after"), b"b\n").unwrap();

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);

    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert_eq!(report.write_step_count, 2);
    assert!(report.preview_artifacts.is_none());
    assert_eq!(report.exit_code_hint, EXIT_RUN_PLAN_WRITE_PREVIEW_REFUSED);
    match report.refusal {
        Some(WritePreviewPlanRefusal::MultipleWorkspaceWriteSteps { count }) => {
            assert_eq!(count, 2);
        }
        other => panic!("expected MultipleWorkspaceWriteSteps, got {other:?}"),
    }
    // No step executed → no .claw artifacts created under workspace_root.
    assert!(!ws.join(".claw/l2b-preview-bundles").exists());
    assert!(!ws.join(".claw/l2b-payloads").exists());
    assert!(!ws.join(".claw/l2b-checkpoints").exists());
    assert!(!ws.join(".claw/l2b-runs").exists());
    // Neither target was created.
    assert!(!ws.join("notes/a.md").exists());
    assert!(!ws.join("notes/b.md").exists());

    fs::remove_dir_all(&ws).ok();
}

// --- Schema / precheck refusals carry through ------------------------------

#[test]
fn deep_tier_plan_refuses_at_validator() {
    let plan = parse_plan(REFUSED_DEEP_PLAN).unwrap();
    let ws = unique_temp_dir("deep");
    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert_eq!(report.exit_code_hint, 2);
    assert!(matches!(
        report.refusal,
        Some(WritePreviewPlanRefusal::ValidatorRefused)
    ));
    fs::remove_dir_all(&ws).ok();
}

#[test]
fn disallowed_tool_on_non_write_step_still_refuses() {
    // The write-preview relaxation only allows Write on workspace-write
    // steps. A non-write step that declares Edit must still refuse.
    let plan = parse_plan(DISALLOWED_TOOL_ON_NONWRITE_PLAN).unwrap();
    let ws = unique_temp_dir("disallowed-ro");
    fs::create_dir_all(ws.join("notes")).unwrap();
    seed_after_file(&ws, b"x\n");

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert_eq!(report.exit_code_hint, 3);
    match report.refusal {
        Some(WritePreviewPlanRefusal::ToolDisallowed { tool, .. }) => {
            assert_eq!(tool, "Edit");
        }
        other => panic!("expected ToolDisallowed for Edit, got {other:?}"),
    }
    // No write-preview artifacts produced.
    assert!(!ws.join(".claw/l2b-preview-bundles").exists());
    fs::remove_dir_all(&ws).ok();
}

// --- Runtime after_file refusals -------------------------------------------

#[test]
fn workspace_write_with_missing_after_file_refuses() {
    let plan = parse_plan(SINGLE_WORKSPACE_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("after-missing");
    fs::create_dir_all(ws.join("notes")).unwrap();
    // No materialized/notes_scratch.after on disk.

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert!(report.preview_artifacts.is_none());
    assert!(!ws.join("notes/scratch.md").exists());
    fs::remove_dir_all(&ws).ok();
}

#[cfg(unix)]
#[test]
fn workspace_write_with_symlink_after_file_refuses() {
    let plan = parse_plan(SINGLE_WORKSPACE_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("after-symlink");
    fs::create_dir_all(ws.join("notes")).unwrap();
    let after_dir = ws.join("materialized");
    fs::create_dir_all(&after_dir).unwrap();
    let real = after_dir.join("real.after");
    fs::write(&real, b"bytes\n").unwrap();
    let link = after_dir.join("notes_scratch.after");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert!(report.preview_artifacts.is_none());
    assert!(!ws.join("notes/scratch.md").exists());
    fs::remove_dir_all(&ws).ok();
}

#[test]
fn workspace_write_with_directory_after_file_refuses() {
    let plan = parse_plan(SINGLE_WORKSPACE_WRITE_PLAN).unwrap();
    let ws = unique_temp_dir("after-dir");
    fs::create_dir_all(ws.join("notes")).unwrap();
    fs::create_dir_all(ws.join("materialized/notes_scratch.after")).unwrap();

    let report =
        run_plan_with_write_preview(&plan, &missing_wrapper(), None, Duration::from_secs(1), &ws);
    assert_eq!(report.status, WritePreviewPlanStatus::Refused);
    assert!(report.preview_artifacts.is_none());
    assert!(!ws.join("notes/scratch.md").exists());
    fs::remove_dir_all(&ws).ok();
}

// --- Scope guards (source-level) -------------------------------------------

#[test]
fn run_plan_with_write_preview_source_does_not_call_execute_write() {
    let src = include_str!("../src/runner.rs");
    assert!(
        !src.contains("execute_write("),
        "runner::run_plan_with_write_preview must not call execute_write"
    );
}

#[test]
fn write_preview_module_source_does_not_call_execute_write_or_bind() {
    let src = include_str!("../src/write_preview.rs");
    for forbidden in [
        "execute_write(",
        "bind_after_bytes(",
        "Command::new(",
        "git apply",
    ] {
        assert!(
            !src.contains(forbidden),
            "write_preview module must not contain {forbidden}"
        );
    }
}

#[test]
fn write_preview_artifacts_never_emit_apply_subcommand_in_operator_text() {
    // The structured `next_operator_command` MUST point at `claw plan
    // approve` — never at `claw plan apply` or `claw plan apply-bundle`.
    // This is verified at runtime in the happy-path test above; the
    // scope check here re-asserts the contract at the type level by
    // inspecting the source of the produce_write_preview helper.
    let src = include_str!("../src/write_preview.rs");
    // Doc-comments may reference apply for negative-assertion language;
    // string literals that could surface to operators must not.
    let literal_apply_bundle = "\"claw plan apply-bundle";
    let literal_apply_space = "\"claw plan apply ";
    assert!(
        !src.contains(literal_apply_bundle),
        "write_preview must not surface a `claw plan apply-bundle` string literal"
    );
    assert!(
        !src.contains(literal_apply_space),
        "write_preview must not surface a `claw plan apply` string literal"
    );
}
