//! A2-L2b slice-3b integration tests — approval UX helpers.
//!
//! These tests prove the offline contract:
//!
//! - Approvable previews render the exact `apply <step-id>
//!   <preview_sha256>` command and surface the sanitized relative path.
//! - The absolute target path never appears in operator-facing output.
//! - Non-approvable previews (binary / redacted / truncated) surface
//!   no approval command and list every applicable reason.
//! - Pasted markers and marker-plus-junk inputs cannot satisfy the
//!   strict approval parser.
//! - Wrong / correct hash, checkpoint drift, control / ANSI / zero-width
//!   chars are routed through the authoritative parser unchanged.
//! - Rendered output contains no ANSI escapes, no NUL, no C0/C1
//!   controls (other than `\n` / `\t`), no zero-width or bidi-override
//!   characters.
//! - Rendering is deterministic for identical inputs.
//! - Slice-3b sources contain no target-write APIs, no stdin/stdout
//!   side effects, no `run_plan` wiring, no broker/model references.

#![allow(clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};

use a2_plan_runner::approval::{ApprovalDecision, ApprovalRefusal};
use a2_plan_runner::approval_ux::{
    evaluate_operator_input, render_approval_prompt, render_non_approvable_summary,
    render_preview_for_operator, ApprovalPromptRender,
};
use a2_plan_runner::diff_preview::{
    build_preview, PreviewDisplay, PreviewInputs, PreviewRecord, MAX_DIFF_LINES,
};
use a2_plan_runner::markers::{
    L2B_APPROVAL_PROMPT, L2B_APPROVED, L2B_BINARY_PREVIEW, L2B_DIFF_PREVIEW_READY,
    L2B_DIFF_REDACTED, L2B_DIFF_TRUNCATED,
};
use ulid::Ulid;

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

fn ulid_zero() -> Ulid {
    Ulid::from_parts(0, 0)
}

#[derive(Clone)]
struct BuildArgs {
    step_id: String,
    rel: PathBuf,
    abs: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
    checkpoint_run_id: Ulid,
    checkpoint_step_id: String,
    created_at_utc: String,
}

impl BuildArgs {
    fn default_for(after: &[u8]) -> Self {
        Self {
            step_id: "step-1".to_string(),
            rel: PathBuf::from("src/lib.rs"),
            abs: PathBuf::from("/abs-only/ws/src/lib.rs"),
            before: Some(b"alpha\nbeta\n".to_vec()),
            after: after.to_vec(),
            checkpoint_run_id: ulid_zero(),
            checkpoint_step_id: "step-1".to_string(),
            created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
        }
    }

    fn to_inputs(&self) -> PreviewInputs<'_> {
        PreviewInputs {
            step_id: &self.step_id,
            target_relative_path: &self.rel,
            target_absolute_path: &self.abs,
            before: self.before.as_deref(),
            after: &self.after,
            checkpoint_run_id: &self.checkpoint_run_id,
            checkpoint_step_id: &self.checkpoint_step_id,
            created_at_utc: &self.created_at_utc,
        }
    }
}

fn build(args: &BuildArgs) -> (PreviewRecord, PreviewDisplay) {
    let inp = args.to_inputs();
    build_preview(&inp).expect("preview build")
}

fn write_line(buf: &mut String, n: usize) {
    use std::fmt::Write as _;
    writeln!(buf, "line-{n}").expect("write into String");
}

fn assert_terminal_safe(text: &str) {
    for (i, ch) in text.char_indices() {
        if ch == '\n' || ch == '\t' {
            continue;
        }
        assert_ne!(ch, '\u{1B}', "ANSI escape at byte {i} in: {text:?}");
        assert!(
            !ch.is_control(),
            "control char {ch:?} (U+{cp:04X}) at byte {i} in: {text:?}",
            cp = ch as u32
        );
        assert!(
            !matches!(
                ch as u32,
                0x200B..=0x200D | 0xFEFF | 0x2060..=0x2064 | 0x180E
            ),
            "zero-width char U+{cp:04X} at byte {i} in: {text:?}",
            cp = ch as u32
        );
        assert!(
            !matches!(
                ch as u32,
                0x202A..=0x202E | 0x2066..=0x2069
            ),
            "bidi override U+{cp:04X} at byte {i} in: {text:?}",
            cp = ch as u32
        );
    }
}

// -------------------------------------------------------------------------
// Approvable preview rendering
// -------------------------------------------------------------------------

#[test]
fn approvable_preview_renders_exact_approval_command() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    assert!(rec.is_approvable());

    let out = render_approval_prompt(&rec, &disp);
    let exact_command = format!("\napply {} {}\n", rec.step_id, rec.preview_sha256);
    assert!(
        out.text.contains(&exact_command),
        "rendered prompt missing exact approval command line.\nText:\n{}",
        out.text
    );
    assert!(out.text.contains("To approve, type exactly:"));
    assert!(out.audit_markers.contains(&L2B_APPROVAL_PROMPT));
    assert!(out.audit_markers.contains(&L2B_DIFF_PREVIEW_READY));
}

#[test]
fn rendered_prompt_includes_preview_sha256() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    let out = render_approval_prompt(&rec, &disp);
    assert!(out.text.contains(&rec.preview_sha256));
    assert!(out.text.contains("Preview SHA256:"));
}

#[test]
fn rendered_prompt_excludes_absolute_path() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    let out = render_approval_prompt(&rec, &disp);
    // The default abs path uses a unique prefix not present in the rel
    // path, so the absolute prefix must not appear anywhere in the
    // operator-facing render.
    assert!(
        !out.text.contains("/abs-only/"),
        "rendered prompt unexpectedly contains absolute path prefix.\nText:\n{}",
        out.text
    );
    assert!(
        !out.text.contains(&rec.target_absolute_path_sanitized),
        "rendered prompt unexpectedly contains full absolute path.\nText:\n{}",
        out.text
    );
}

#[test]
fn rendered_prompt_includes_sanitized_relative_path() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    let out = render_approval_prompt(&rec, &disp);
    let target_line = format!("Target: {}", rec.target_relative_path_sanitized);
    assert!(
        out.text.contains(&target_line),
        "rendered prompt missing sanitized relative target line.\nText:\n{}",
        out.text
    );
}

#[test]
fn rendered_preview_view_excludes_approval_command_line() {
    // render_preview_for_operator surfaces the same header + diff body
    // but never the "To approve, type exactly" instruction.
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    let out = render_preview_for_operator(&rec, &disp);
    assert!(out.text.contains("A2-L2b approval required"));
    assert!(!out.text.contains("To approve, type exactly:"));
    let command = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    assert!(
        !out.text.contains(&command),
        "preview view unexpectedly contains the approval command line.\nText:\n{}",
        out.text
    );
    assert!(!out.audit_markers.contains(&L2B_APPROVAL_PROMPT));
}

// -------------------------------------------------------------------------
// Non-approvable previews
// -------------------------------------------------------------------------

#[test]
fn non_approvable_binary_preview_shows_no_approval_command() {
    let mut args = BuildArgs::default_for(&[0u8, 1, 2, 3, 0, 4, 5]);
    args.before = Some(b"plain-before".to_vec());
    let (rec, disp) = build(&args);
    assert!(rec.is_binary);
    assert!(!rec.is_approvable());

    // Both rendering helpers must collapse to a non-approvable summary.
    for out in [
        render_approval_prompt(&rec, &disp),
        render_preview_for_operator(&rec, &disp),
        render_non_approvable_summary(&rec),
    ] {
        assert!(out.text.contains("A2-L2b preview is not approvable"));
        assert!(out.text.contains("- binary"));
        assert!(!out.text.contains("To approve, type exactly:"));
        let command = format!("apply {} {}", rec.step_id, rec.preview_sha256);
        assert!(
            !out.text.contains(&command),
            "non-approvable binary preview must not contain approval command.\nText:\n{}",
            out.text
        );
        assert!(out.text.contains("No approval command is accepted"));
        assert!(out.audit_markers.contains(&L2B_BINARY_PREVIEW));
        assert!(!out.audit_markers.contains(&L2B_APPROVAL_PROMPT));
        assert_terminal_safe(&out.text);
    }
}

#[test]
fn non_approvable_redacted_preview_shows_no_approval_command() {
    let mut args = BuildArgs::default_for(b"alpha\nbeta\npassword=hunter2-secret-x\ngamma\n");
    args.before = Some(b"alpha\nbeta\ngamma\n".to_vec());
    let (rec, disp) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rec.is_approvable());

    let out = render_approval_prompt(&rec, &disp);
    assert!(out.text.contains("- redacted"));
    assert!(!out.text.contains("To approve, type exactly:"));
    let command = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    assert!(!out.text.contains(&command));
    assert!(out.audit_markers.contains(&L2B_DIFF_REDACTED));
    assert!(!out.audit_markers.contains(&L2B_APPROVAL_PROMPT));
    assert_terminal_safe(&out.text);
}

#[test]
fn non_approvable_truncated_preview_shows_no_approval_command() {
    let before = String::new();
    let mut after = String::with_capacity(MAX_DIFF_LINES * 8);
    for i in 0..MAX_DIFF_LINES + 50 {
        write_line(&mut after, i);
    }
    let mut args = BuildArgs::default_for(after.as_bytes());
    args.before = Some(before.into_bytes());
    let (rec, disp) = build(&args);
    assert!(rec.is_truncated);
    assert!(!rec.is_approvable());

    let out = render_approval_prompt(&rec, &disp);
    assert!(out.text.contains("- truncated"));
    assert!(!out.text.contains("To approve, type exactly:"));
    let command = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    assert!(!out.text.contains(&command));
    assert!(out.audit_markers.contains(&L2B_DIFF_TRUNCATED));
    assert!(!out.audit_markers.contains(&L2B_APPROVAL_PROMPT));
    assert_terminal_safe(&out.text);
}

#[test]
fn multiple_non_approvable_reasons_are_all_listed() {
    // Synthesize a PreviewRecord with all three flags set. We do not
    // round-trip through build_preview here because a single input
    // cannot simultaneously hit binary + redacted + truncated paths;
    // the summary helper must surface every applicable reason
    // regardless.
    let rec = PreviewRecord {
        preview_id: "01HZZZZZZZZZZZZZZZZZZZZZZ0".to_string(),
        step_id: "step-1".to_string(),
        target_relative_path_sanitized: "src/lib.rs".to_string(),
        target_absolute_path_sanitized: "/abs-only/ws/src/lib.rs".to_string(),
        before_sha256: "a".repeat(64),
        after_sha256: "b".repeat(64),
        preview_sha256: "c".repeat(64),
        checkpoint_run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
        checkpoint_step_id: "step-1".to_string(),
        is_binary: true,
        is_redacted: true,
        is_truncated: true,
        created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
        preview_format_version: 1,
    };
    assert!(!rec.is_approvable());

    let out = render_non_approvable_summary(&rec);
    assert!(out.text.contains("- binary"));
    assert!(out.text.contains("- redacted"));
    assert!(out.text.contains("- truncated"));
    assert!(out.text.contains("No approval command is accepted"));
    let command = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    assert!(!out.text.contains(&command));
    assert!(out.audit_markers.contains(&L2B_BINARY_PREVIEW));
    assert!(out.audit_markers.contains(&L2B_DIFF_REDACTED));
    assert!(out.audit_markers.contains(&L2B_DIFF_TRUNCATED));
    assert_terminal_safe(&out.text);
}

// -------------------------------------------------------------------------
// evaluate_operator_input — pasted markers / junk cannot approve
// -------------------------------------------------------------------------

#[test]
fn pasted_a2_l2b_approved_marker_text_cannot_approve() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);
    // The audit marker `a2-l2b-approved` is not a parseable approval
    // command; the strict parser must refuse.
    let dec = evaluate_operator_input(&rec, L2B_APPROVED, true);
    match dec {
        ApprovalDecision::Refused(_) => (),
        other @ ApprovalDecision::Approved { .. } => {
            panic!("pasted marker should refuse, got: {other:?}")
        }
    }
    // Trailing whitespace / a newline after the marker is still not a
    // valid command.
    let dec2 = evaluate_operator_input(&rec, &format!("{L2B_APPROVED}\n"), true);
    assert!(matches!(dec2, ApprovalDecision::Refused(_)));
}

#[test]
fn marker_plus_valid_command_pasted_as_junk_cannot_approve() {
    // An operator pastes the marker on the same line as a valid-shaped
    // command. The strict parser must refuse: even though the command
    // suffix is well-formed, the leading marker token violates either
    // the keyword or arg-count constraint.
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);

    let input = format!(
        "{} apply {} {}",
        L2B_APPROVED, rec.step_id, rec.preview_sha256
    );
    let dec = evaluate_operator_input(&rec, &input, true);
    assert!(
        matches!(dec, ApprovalDecision::Refused(_)),
        "marker prefix should refuse, got: {dec:?}"
    );

    // Marker on its own line followed by the valid command on the next
    // line — embedded newline must refuse via ControlChars (the parser
    // accepts only a single optional trailing newline).
    let multi = format!(
        "{}\napply {} {}",
        L2B_APPROVED, rec.step_id, rec.preview_sha256
    );
    let dec2 = evaluate_operator_input(&rec, &multi, true);
    assert!(
        matches!(dec2, ApprovalDecision::Refused(_)),
        "marker + newline + command should refuse, got: {dec2:?}"
    );
}

// -------------------------------------------------------------------------
// evaluate_operator_input — hash / baseline / control chars
// -------------------------------------------------------------------------

#[test]
fn correct_hash_approves_via_operator_input() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);
    let input = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    let dec = evaluate_operator_input(&rec, &input, true);
    assert_eq!(
        dec,
        ApprovalDecision::Approved {
            step_id: rec.step_id.clone(),
            preview_sha256: rec.preview_sha256.clone(),
        }
    );
}

#[test]
fn wrong_hash_refuses_via_operator_input() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);
    let bad_hash = "1".repeat(64);
    let input = format!("apply {} {bad_hash}", rec.step_id);
    let dec = evaluate_operator_input(&rec, &input, true);
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewHashMismatch)
    );
}

#[test]
fn checkpoint_baseline_changed_refuses_even_with_correct_hash() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);
    let input = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    let dec = evaluate_operator_input(&rec, &input, false);
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::CheckpointDrift)
    );
}

#[test]
fn control_chars_refuse_via_operator_input() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _disp) = build(&args);
    // Embedded NUL.
    let with_nul = format!("apply {}\0 {}", rec.step_id, rec.preview_sha256);
    let dec = evaluate_operator_input(&rec, &with_nul, true);
    assert!(matches!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::ControlChars)
    ));
    // ANSI escape sequence.
    let with_ansi = format!("apply\x1b[31m {} {}", rec.step_id, rec.preview_sha256);
    let dec2 = evaluate_operator_input(&rec, &with_ansi, true);
    assert!(matches!(
        dec2,
        ApprovalDecision::Refused(ApprovalRefusal::AnsiEscape)
    ));
}

// -------------------------------------------------------------------------
// Terminal safety of rendered output
// -------------------------------------------------------------------------

#[test]
fn rendered_output_is_terminal_safe_for_approvable_preview() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);
    assert_terminal_safe(&render_approval_prompt(&rec, &disp).text);
    assert_terminal_safe(&render_preview_for_operator(&rec, &disp).text);
}

#[test]
fn rendered_output_is_terminal_safe_when_path_contains_ansi() {
    // The slice-3a builder sanitizes the path before it ever reaches
    // the record fields. The UX renderer must still emit terminal-safe
    // output for these adversarial inputs.
    let mut args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    args.rel = PathBuf::from("ok/\u{1b}[31mevil\u{1b}[0m\nname.rs");
    args.abs = PathBuf::from("/abs-only/ws/ok/\u{1b}[31mevil\u{1b}[0m\nname.rs");
    let (rec, disp) = build(&args);
    let out = render_approval_prompt(&rec, &disp);
    assert!(!out.text.contains('\u{1B}'));
    assert_terminal_safe(&out.text);
}

// -------------------------------------------------------------------------
// Determinism
// -------------------------------------------------------------------------

#[test]
fn rendering_is_deterministic_for_identical_inputs() {
    // Build two PreviewRecord+Display pairs from the same inputs and
    // synthesize identical records (build_preview mints a fresh ULID
    // for `preview_id` on each call, so direct comparison would fail
    // — clone the record from one build for the second render).
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, disp) = build(&args);

    let prompt_a = render_approval_prompt(&rec, &disp);
    let prompt_b = render_approval_prompt(&rec, &disp);
    assert_eq!(prompt_a, prompt_b);

    let preview_a = render_preview_for_operator(&rec, &disp);
    let preview_b = render_preview_for_operator(&rec, &disp);
    assert_eq!(preview_a, preview_b);

    let summary_a = render_non_approvable_summary(&rec);
    let summary_b = render_non_approvable_summary(&rec);
    assert_eq!(summary_a, summary_b);
}

// -------------------------------------------------------------------------
// Type-level purity (no run_plan / Runner handles in the helper signatures)
// -------------------------------------------------------------------------

#[allow(clippy::extra_unused_lifetimes)]
fn assert_render_pure(_f: fn(&PreviewRecord, &PreviewDisplay) -> ApprovalPromptRender) {}

#[allow(clippy::extra_unused_lifetimes)]
fn assert_summary_pure(_f: fn(&PreviewRecord) -> ApprovalPromptRender) {}

#[allow(clippy::extra_unused_lifetimes)]
fn assert_evaluate_pure(_f: fn(&PreviewRecord, &str, bool) -> ApprovalDecision) {}

#[test]
fn slice_3b_helpers_are_pure_functions_no_run_plan_wiring() {
    // The public surface of approval_ux is exclusively pure functions
    // over PreviewRecord / PreviewDisplay / &str. There is no path
    // from a UX helper to the L1b runner's plan-executor entry point;
    // any future change that smuggles a Runner handle through these
    // signatures fails to compile.
    assert_render_pure(render_preview_for_operator);
    assert_render_pure(render_approval_prompt);
    assert_summary_pure(render_non_approvable_summary);
    assert_evaluate_pure(evaluate_operator_input);
}

// -------------------------------------------------------------------------
// Source-level scope checks (write APIs, broker / network, terminal I/O)
// -------------------------------------------------------------------------

#[test]
fn no_target_write_apis_in_slice_3b_sources() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src =
        std::fs::read_to_string(here.join("src/approval_ux.rs")).expect("read approval_ux.rs");
    for forbidden in [
        "OpenOptions::new",
        "File::create",
        "fs::write(",
        "fs::create_dir",
        "Command::new",
        ".spawn(",
        "git diff",
        "git apply",
        "11434",
        "11435",
        "OPENAI_BASE_URL",
        "vram-broker",
        "SideStackAI",
        "sidestackai",
        "broker.py",
        "rusty-claude-cli",
        "ollama",
    ] {
        assert!(
            !src.contains(forbidden),
            "approval_ux.rs contains forbidden token: {forbidden}"
        );
    }
}

#[test]
fn no_terminal_io_macros_in_slice_3b_sources() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src =
        std::fs::read_to_string(here.join("src/approval_ux.rs")).expect("read approval_ux.rs");
    for forbidden in ["println!", "eprintln!", "print!", "eprint!"] {
        assert!(
            !src.contains(forbidden),
            "approval_ux.rs contains forbidden terminal-I/O macro: {forbidden}"
        );
    }
}
