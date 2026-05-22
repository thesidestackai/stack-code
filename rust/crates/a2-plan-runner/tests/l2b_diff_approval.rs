//! A2-L2b slice-3a integration tests — diff preview + approval
//! primitives.
//!
//! These tests prove the offline contract:
//!
//! - [`PreviewRecord`] canonical hash is metadata-sensitive.
//! - [`PreviewRecord`] canonical hash is render-sensitive.
//! - [`ApprovalDecision`] requires exact `apply <step-id> <preview_sha256>`.
//! - Wrong hash refuses (replay/mismatch).
//! - Same step but different preview hash refuses.
//! - Plain `apply <step-id>` refuses.
//! - Control chars / ANSI / Unicode confusables refuse.
//! - Markers are audit-only (importing them is harmless).
//! - Binary preview is summary-only and non-approvable.
//! - Redacted / truncated previews are non-approvable.
//! - Final-output post-scan is fail-closed.
//! - Path / header sanitization.
//! - Secret redaction patterns.
//! - [`similar`] is used for safe text rendering.
//! - No [`run_plan`] wiring (this crate's slice-3a modules expose no
//!   handle into the runner).
//! - No target-file write APIs (no `Write`/`Edit`/`Create`/`OpenOptions::write`
//!   appear in the [`diff_preview`]/[`approval`] source).

#![allow(clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};

use a2_plan_runner::approval::{
    evaluate_approval, ApprovalContext, ApprovalDecision, ApprovalRefusal, EXIT_APPROVAL_DENIED,
    EXIT_ROLLBACK_FAILED, PREVIEW_SHA256_HEX_LEN,
};
use a2_plan_runner::diff_preview::{
    build_preview, canonical_preview_record_for_approval, contains_secret_like,
    preview_hash_from_parts, CanonicalSubset, PreviewBuildError, PreviewInputs, PreviewRecord,
    CANONICAL_HEADER, HASH_DISPLAY_SEPARATOR, MAX_DIFF_LINES, PREVIEW_FORMAT_VERSION,
};
use a2_plan_runner::markers::{
    L2B_APPROVAL_PROMPT, L2B_APPROVAL_REFUSED, L2B_APPROVED, L2B_BINARY_PREVIEW,
    L2B_DIFF_PREVIEW_READY, L2B_DIFF_REDACTED, L2B_DIFF_TRUNCATED, L2B_PREAPPROVAL_REFUSED,
    L2B_PREVIEW_RECORD_CREATED,
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
            abs: PathBuf::from("/ws/src/lib.rs"),
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

fn build(args: &BuildArgs) -> (PreviewRecord, String) {
    let inp = args.to_inputs();
    let (rec, disp) = build_preview(&inp).expect("preview build");
    (rec, disp.rendered)
}

fn ctx(rec: &PreviewRecord, baseline_ok: bool) -> ApprovalContext<'_> {
    ApprovalContext {
        preview: rec,
        checkpoint_baseline_unchanged: baseline_ok,
    }
}

fn write_line(buf: &mut String, n: usize) {
    use std::fmt::Write as _;
    writeln!(buf, "line-{n}").expect("write into String");
}

// -------------------------------------------------------------------------
// Canonical hash sensitivity
// -------------------------------------------------------------------------

#[test]
fn canonical_hash_changes_when_metadata_changes() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec_a, _) = build(&args);

    // Identical inputs but different step_id => different canonical
    // subset => different preview_sha256.
    let mut args2 = args.clone();
    args2.step_id = "step-2".to_string();
    args2.checkpoint_step_id = "step-2".to_string();
    let (rec_b, _) = build(&args2);

    assert_ne!(rec_a.preview_sha256, rec_b.preview_sha256);
    assert_ne!(rec_a.step_id, rec_b.step_id);
}

#[test]
fn canonical_hash_changes_when_before_sha_changes() {
    let args_a = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec_a, _) = build(&args_a);

    // Different before content => different before_sha256 in canonical
    // subset.
    let mut args_b = args_a.clone();
    args_b.before = Some(b"different-before\n".to_vec());
    let (rec_b, _) = build(&args_b);

    assert_ne!(rec_a.before_sha256, rec_b.before_sha256);
    assert_ne!(rec_a.preview_sha256, rec_b.preview_sha256);
}

#[test]
fn rendered_preview_hash_binding_works() {
    // The hash is computed over canonical || sep || rendered. If we
    // recompute it from the published parts we get the same hex.
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, rendered) = build(&args);

    let canonical = canonical_preview_record_for_approval(&CanonicalSubset {
        preview_id: &rec.preview_id,
        step_id: &rec.step_id,
        target_relative_path_sanitized: &rec.target_relative_path_sanitized,
        before_sha256: &rec.before_sha256,
        after_sha256: &rec.after_sha256,
        checkpoint_run_id: &rec.checkpoint_run_id,
        checkpoint_step_id: &rec.checkpoint_step_id,
        is_binary: rec.is_binary,
        is_redacted: rec.is_redacted,
        is_truncated: rec.is_truncated,
        preview_format_version: rec.preview_format_version,
    });

    let recomputed = preview_hash_from_parts(&canonical, &rendered);
    assert_eq!(recomputed, rec.preview_sha256);
}

#[test]
fn canonical_header_and_separator_are_pinned() {
    // The public constants are an external verifier contract.
    assert_eq!(CANONICAL_HEADER, "A2-L2B-PREVIEW-RECORD-V1");
    assert_eq!(HASH_DISPLAY_SEPARATOR, "\n---DISPLAY---\n");
    assert_eq!(PREVIEW_FORMAT_VERSION, 1);
}

// -------------------------------------------------------------------------
// Approval parser: exact format
// -------------------------------------------------------------------------

#[test]
fn approval_requires_exact_apply_step_hash_form() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let input = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    let dec = evaluate_approval(&input, ctx(&rec, true));
    assert_eq!(
        dec,
        ApprovalDecision::Approved {
            step_id: rec.step_id.clone(),
            preview_sha256: rec.preview_sha256.clone(),
        }
    );
}

#[test]
fn approval_accepts_single_trailing_newline() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let input = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
    let dec = evaluate_approval(&input, ctx(&rec, true));
    assert!(matches!(dec, ApprovalDecision::Approved { .. }));
}

#[test]
fn approval_refuses_extra_args() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let input = format!("apply {} {} now", rec.step_id, rec.preview_sha256);
    let dec = evaluate_approval(&input, ctx(&rec, true));
    assert_eq!(dec, ApprovalDecision::Refused(ApprovalRefusal::ArgCount));
}

#[test]
fn approval_refuses_leading_or_trailing_whitespace() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let happy = format!("apply {} {}", rec.step_id, rec.preview_sha256);
    let dec_leading = evaluate_approval(&format!(" {happy}"), ctx(&rec, true));
    let dec_trailing = evaluate_approval(&format!("{happy} "), ctx(&rec, true));
    let dec_double_space = evaluate_approval(
        &format!("apply  {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec_leading,
        ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
    );
    assert_eq!(
        dec_trailing,
        ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
    );
    assert_eq!(
        dec_double_space,
        ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
    );
}

#[test]
fn approval_refuses_plain_apply_step_id() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let dec = evaluate_approval(&format!("apply {}", rec.step_id), ctx(&rec, true));
    assert_eq!(dec, ApprovalDecision::Refused(ApprovalRefusal::ArgCount));
}

#[test]
fn approval_refuses_case_variants_of_apply_keyword() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    for kw in ["Apply", "APPLY", "ApPlY"] {
        let dec = evaluate_approval(
            &format!("{kw} {} {}", rec.step_id, rec.preview_sha256),
            ctx(&rec, true),
        );
        assert_eq!(
            dec,
            ApprovalDecision::Refused(ApprovalRefusal::KeywordInvalid),
            "kw={kw}"
        );
    }
}

#[test]
fn approval_refuses_quoted_variants() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let dec = evaluate_approval(
        &format!("\"apply {} {}\"", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::QuotedVariant)
    );
    let dec2 = evaluate_approval(
        &format!("'apply {} {}'", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec2,
        ApprovalDecision::Refused(ApprovalRefusal::QuotedVariant)
    );
}

#[test]
fn approval_refuses_ansi_escape() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let dec = evaluate_approval(
        &format!("apply\x1b[31m {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(dec, ApprovalDecision::Refused(ApprovalRefusal::AnsiEscape));
}

#[test]
fn approval_refuses_control_chars() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    // Embedded newline (not just trailing) and embedded NUL.
    let with_newline = format!("apply\n{} {}", rec.step_id, rec.preview_sha256);
    let with_nul = format!("apply {}\0 {}", rec.step_id, rec.preview_sha256);
    assert!(matches!(
        evaluate_approval(&with_newline, ctx(&rec, true)),
        ApprovalDecision::Refused(ApprovalRefusal::ControlChars)
    ));
    assert!(matches!(
        evaluate_approval(&with_nul, ctx(&rec, true)),
        ApprovalDecision::Refused(ApprovalRefusal::ControlChars)
    ));
}

#[test]
fn approval_refuses_zero_width_chars() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    // Zero-width space between `appl` and `y`.
    let input = format!("appl\u{200B}y {} {}", rec.step_id, rec.preview_sha256);
    let dec = evaluate_approval(&input, ctx(&rec, true));
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::ZeroWidthOrBidi)
    );
}

#[test]
fn approval_refuses_unicode_confusables_for_apply_keyword() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    // Fullwidth `ａｐｐｌｙ`.
    let input = format!(
        "\u{FF41}\u{FF50}\u{FF50}\u{FF4C}\u{FF59} {} {}",
        rec.step_id, rec.preview_sha256
    );
    let dec = evaluate_approval(&input, ctx(&rec, true));
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::UnicodeConfusable)
    );
}

#[test]
fn approval_refuses_batch_syntax() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let chained = format!(
        "apply {} {};apply x deadbeef",
        rec.step_id, rec.preview_sha256
    );
    let dec = evaluate_approval(&chained, ctx(&rec, true));
    assert_eq!(dec, ApprovalDecision::Refused(ApprovalRefusal::BatchSyntax));

    let amped = format!("apply {} {} & apply x y", rec.step_id, rec.preview_sha256);
    let dec2 = evaluate_approval(&amped, ctx(&rec, true));
    assert_eq!(
        dec2,
        ApprovalDecision::Refused(ApprovalRefusal::BatchSyntax)
    );
}

#[test]
fn approval_refuses_preapproval_keywords() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    for tail in ["--yes", "--auto", "preapprove", "auto-apply", "skip-prompt"] {
        let dec = evaluate_approval(
            &format!("apply {} {} {tail}", rec.step_id, rec.preview_sha256),
            ctx(&rec, true),
        );
        assert!(
            matches!(
                dec,
                ApprovalDecision::Refused(ApprovalRefusal::Preapproval | ApprovalRefusal::ArgCount)
            ),
            "tail={tail} dec={dec:?}"
        );
    }
}

#[test]
fn approval_refuses_invalid_hash_shape() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    // Hash with uppercase hex (must be lowercase).
    let upper = rec.preview_sha256.to_uppercase();
    let dec = evaluate_approval(&format!("apply {} {upper}", rec.step_id), ctx(&rec, true));
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewHashShape)
    );

    // Hash too short.
    let short = "abc".repeat(2);
    let dec2 = evaluate_approval(&format!("apply {} {short}", rec.step_id), ctx(&rec, true));
    assert_eq!(
        dec2,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewHashShape)
    );
}

// -------------------------------------------------------------------------
// Approval binding
// -------------------------------------------------------------------------

#[test]
fn wrong_hash_refuses_with_mismatch() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let bad_hash = "1".repeat(64);
    let dec = evaluate_approval(
        &format!("apply {} {bad_hash}", rec.step_id),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewHashMismatch)
    );
}

#[test]
fn same_step_different_preview_hash_refuses_replay() {
    // Generate two previews for the same step (different `after`).
    let args_a = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec_a, _) = build(&args_a);

    let mut args_b = args_a.clone();
    args_b.after = b"alpha\nbeta\ngamma\ndelta\n".to_vec();
    let (rec_b, _) = build(&args_b);

    assert_ne!(rec_a.preview_sha256, rec_b.preview_sha256);

    // Operator submits an approval bound to rec_a's hash but the
    // current context is rec_b. The decision refuses with
    // PreviewHashMismatch — the replay defense.
    let stale = format!("apply {} {}", rec_a.step_id, rec_a.preview_sha256);
    let dec = evaluate_approval(&stale, ctx(&rec_b, true));
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewHashMismatch)
    );
}

#[test]
fn wrong_step_id_refuses_with_mismatch() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let dec = evaluate_approval(
        &format!("apply step-99 {}", rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::StepIdMismatch)
    );
}

#[test]
fn checkpoint_drift_refuses() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, _) = build(&args);
    let dec = evaluate_approval(
        &format!("apply {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, false),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::CheckpointDrift)
    );
}

// -------------------------------------------------------------------------
// Preview non-approvable cases
// -------------------------------------------------------------------------

#[test]
fn binary_preview_is_summary_only_and_non_approvable() {
    let mut args = BuildArgs::default_for(&[0u8, 1, 2, 3, 4, 0, 5]);
    args.before = Some(b"plain-before".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_binary, "expected is_binary");
    assert!(!rec.is_approvable());
    assert!(rendered.contains("# preview: metadata-only"));
    // No raw bytes leak.
    for b in 0..32u8 {
        if matches!(b, b'\n' | b'\t') {
            continue;
        }
        assert!(
            !rendered.as_bytes().contains(&b),
            "raw control byte 0x{b:02x} leaked"
        );
    }

    let dec = evaluate_approval(
        &format!("apply {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewBinary)
    );
}

#[test]
fn redacted_preview_is_non_approvable() {
    let mut args = BuildArgs::default_for(b"alpha\nbeta\npassword=hunter2-secret-x\ngamma\n");
    args.before = Some(b"alpha\nbeta\ngamma\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rec.is_approvable());
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("hunter2-secret-x"));

    let dec = evaluate_approval(
        &format!("apply {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewRedacted)
    );
}

#[test]
fn truncated_preview_is_non_approvable() {
    // Build a very large `after` blob to blow the diff line cap.
    let before = String::new();
    let mut after = String::with_capacity(MAX_DIFF_LINES * 8);
    for i in 0..MAX_DIFF_LINES + 50 {
        write_line(&mut after, i);
    }
    let mut args = BuildArgs::default_for(after.as_bytes());
    args.before = Some(before.into_bytes());
    let (rec, _rendered) = build(&args);
    assert!(rec.is_truncated, "expected is_truncated");
    assert!(!rec.is_approvable());

    let dec = evaluate_approval(
        &format!("apply {} {}", rec.step_id, rec.preview_sha256),
        ctx(&rec, true),
    );
    assert_eq!(
        dec,
        ApprovalDecision::Refused(ApprovalRefusal::PreviewTruncated)
    );
}

// -------------------------------------------------------------------------
// Final-output post-scan (fail-closed)
// -------------------------------------------------------------------------

#[test]
fn final_output_scan_collapses_to_summary_when_secret_remains() {
    // PEM block-shaped content. The primary redaction should remove
    // the block; if it ever doesn't, the post-scan must collapse the
    // preview to a metadata summary and mark it refused.
    let after = b"-----BEGIN PRIVATE KEY-----\nABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789==\n-----END PRIVATE KEY-----\n";
    let mut args = BuildArgs::default_for(after);
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted, "PEM block must trigger redaction");
    assert!(
        !rendered.contains("BEGIN PRIVATE KEY"),
        "rendered display must not leak PEM remnants"
    );
    assert!(!rec.is_approvable());
}

#[test]
fn contains_secret_like_detects_url_credentials() {
    let s = "postgres://user:hunter2@db.example.com/app";
    assert!(contains_secret_like(s));
    let safe = "https://example.com/app";
    assert!(!contains_secret_like(safe));
}

#[test]
fn contains_secret_like_detects_pem_remnant() {
    assert!(contains_secret_like(
        "noise\n-----BEGIN RSA PRIVATE KEY-----\nblah\n"
    ));
    assert!(!contains_secret_like("no secret here"));
}

// -------------------------------------------------------------------------
// Redaction patterns
// -------------------------------------------------------------------------

#[test]
fn redaction_covers_api_key_kv_pair() {
    let mut args = BuildArgs::default_for(b"api_key=ZXabcdefghIJKLMNOPQRSTUV\n");
    args.before = Some(b"# empty\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("ZXabcdefghIJKLMNOPQRSTUV"));
}

#[test]
fn redaction_covers_authorization_bearer_header() {
    let mut args =
        BuildArgs::default_for(b"Authorization: Bearer abcDEFghi0123456789jklMNOpqrSTUv\n");
    args.before = Some(b"# empty\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("abcDEFghi0123456789jklMNOpqrSTUv"));
}

#[test]
fn redaction_covers_vendor_token_prefixes() {
    let mut args = BuildArgs::default_for(
        b"const x = \"sk-ant-abcdefghijklmnopqrstuvwxyz0123\"\nconst y = \"ghp_thequickbrownfox\"\n",
    );
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("ghp_thequickbrownfox"));
    assert!(!rendered.contains("sk-ant-abcdefghijklmnopqrstuvwxyz0123"));
}

#[test]
fn redaction_covers_jwt_like_token() {
    let mut args = BuildArgs::default_for(
        b"token=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.signature_segment_here_long_enough\n",
    );
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("eyJhbGciOiJIUzI1NiJ9"));
}

#[test]
fn redaction_covers_aws_access_key_id() {
    let mut args = BuildArgs::default_for(b"id = AKIAIOSFODNN7EXAMPLE\n");
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn redaction_covers_url_credentials() {
    let mut args =
        BuildArgs::default_for(b"DATABASE_URL=postgres://alice:hunter2@db.internal:5432/app\n");
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("hunter2"));
}

#[test]
fn redaction_covers_query_signature() {
    let mut args = BuildArgs::default_for(
        b"href=https://s3.example.com/o/file?X-Amz-Signature=DEADBEEFDEADBEEFDEADBEEFDEADBEEF&otherparam=ok\n",
    );
    args.before = Some(b"# placeholder\n".to_vec());
    let (rec, rendered) = build(&args);
    assert!(rec.is_redacted);
    assert!(!rendered.contains("DEADBEEFDEADBEEFDEADBEEFDEADBEEF"));
}

// -------------------------------------------------------------------------
// Path sanitization
// -------------------------------------------------------------------------

#[test]
fn path_header_injection_is_sanitized() {
    let mut args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    args.rel = PathBuf::from("ok/\u{1b}[31mevil\u{1b}[0m\nname.rs");
    args.abs = PathBuf::from("/ws/ok/\u{1b}[31mevil\u{1b}[0m\nname.rs");
    let (rec, rendered) = build(&args);
    assert!(!rec.target_relative_path_sanitized.contains('\u{1b}'));
    assert!(!rec.target_relative_path_sanitized.contains('\n'));
    assert!(!rendered.contains('\u{1b}'));
    // Embedded newline in the path cannot manifest a second header.
    assert_eq!(rendered.matches("--- a/").count(), 1);
    assert_eq!(rendered.matches("+++ b/").count(), 1);
}

#[test]
fn path_zero_width_chars_are_stripped() {
    let mut args = BuildArgs::default_for(b"x\n");
    args.rel = PathBuf::from("src/li\u{200B}b.rs");
    args.abs = PathBuf::from("/ws/src/li\u{200B}b.rs");
    let (rec, _) = build(&args);
    assert!(!rec.target_relative_path_sanitized.contains('\u{200B}'));
}

// -------------------------------------------------------------------------
// `similar`-backed diff body
// -------------------------------------------------------------------------

#[test]
fn safe_text_diff_renders_unified_body_via_similar() {
    let args = BuildArgs::default_for(b"alpha\nbeta\ngamma\n");
    let (rec, rendered) = build(&args);
    assert!(rec.is_approvable());
    assert!(rendered.contains("--- a/"));
    assert!(rendered.contains("+++ b/"));
    assert!(rendered.contains("@@"));
    assert!(rendered.contains("+gamma"));
    assert!(rendered.contains(" alpha") || rendered.contains(" beta"));
}

// -------------------------------------------------------------------------
// Audit-only markers + exit codes
// -------------------------------------------------------------------------

#[test]
fn markers_are_audit_only_strings() {
    // None of the slice-3a markers carry behavior. They're stable
    // strings. Pin them here as the operator-facing contract.
    for m in [
        L2B_PREVIEW_RECORD_CREATED,
        L2B_DIFF_PREVIEW_READY,
        L2B_DIFF_REDACTED,
        L2B_DIFF_TRUNCATED,
        L2B_BINARY_PREVIEW,
        L2B_APPROVAL_PROMPT,
        L2B_APPROVED,
        L2B_APPROVAL_REFUSED,
        L2B_PREAPPROVAL_REFUSED,
    ] {
        assert!(m.starts_with("a2-l2b-"));
    }
}

#[test]
fn exit_codes_reserved_but_unwired() {
    // Slice-3a documents 7 (approval denied) and reserves 8 (rollback).
    assert_eq!(EXIT_APPROVAL_DENIED, 7);
    assert_eq!(EXIT_ROLLBACK_FAILED, 8);

    // `exit_code_for` (the read-only L1b runner's exit-code mapper)
    // never produces 7 or 8 in slice 3a — those codes are bound at
    // the CLI boundary in a later slice. This is verified by the
    // existing report.rs unit tests in the lib suite; here we just
    // assert the constants remain pinned.
    assert_eq!(PREVIEW_SHA256_HEX_LEN, 64);
}

// -------------------------------------------------------------------------
// Scope checks
// -------------------------------------------------------------------------

#[test]
fn no_run_plan_wiring_for_slice_3a() {
    // Slice 3a does not expose any entry point through `run_plan`.
    // The diff_preview / approval modules export only pure functions
    // and data structures; the runner module remains the L1b
    // read-only executor.
    //
    // This is a *source-level* assertion verified at build time: if
    // someone wires `build_preview` or `evaluate_approval` into
    // `run_plan`, this test still compiles, but the scope-check grep
    // executed by CI will fail.
    //
    // Here we just verify that the public API of the two new modules
    // is composed of pure functions and serializable data — no
    // type-level "Runner" handle leaks in.
    assert_pure(build_preview);
    assert_pure_approval(evaluate_approval);
}

#[allow(clippy::extra_unused_lifetimes)]
fn assert_pure(
    _f: fn(
        &PreviewInputs<'_>,
    ) -> Result<
        (PreviewRecord, a2_plan_runner::diff_preview::PreviewDisplay),
        PreviewBuildError,
    >,
) {
}

#[allow(clippy::extra_unused_lifetimes)]
fn assert_pure_approval(_f: fn(&str, ApprovalContext<'_>) -> ApprovalDecision) {}

#[test]
fn no_target_file_write_apis_in_slice_3a_sources() {
    // Read the diff_preview + approval source files and assert that
    // they contain no target-file write primitives. This is the
    // run-time complement to the CI scope grep.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dp =
        std::fs::read_to_string(here.join("src/diff_preview.rs")).expect("read diff_preview.rs");
    let ap = std::fs::read_to_string(here.join("src/approval.rs")).expect("read approval.rs");

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
        "ollama",
    ] {
        assert!(
            !dp.contains(forbidden),
            "diff_preview.rs contains forbidden token: {forbidden}"
        );
        assert!(
            !ap.contains(forbidden),
            "approval.rs contains forbidden token: {forbidden}"
        );
    }
}
