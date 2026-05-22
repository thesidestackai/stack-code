//! A2-L2b workspace-write approval primitives (slice 3a).
//!
//! This module turns a raw operator approval string + a
//! [`crate::diff_preview::PreviewRecord`] into a structured
//! [`ApprovalDecision`]. The parser is *strict*: only the exact form
//!
//! ```text
//! apply <step-id> <preview_sha256>
//! ```
//!
//! is accepted, where `<step-id>` satisfies `^[A-Za-z0-9_.-]{1,128}$`
//! (with bare `.` / `..` refused) and `<preview_sha256>` is a 64-character
//! lowercase hex string. Any deviation — case variants of `apply`,
//! quoted variants, ANSI escapes, zero-width chars, Unicode confusables,
//! batch-apply syntax, preapproval (`--yes`, `auto`) — is refused
//! ahead of any hash check.
//!
//! # Hard contract (slice 3a)
//!
//! - No filesystem writes, no child-process invocations, no broker, no
//!   model calls.
//! - Not wired into the L1b runner's plan-executor entry point.
//! - Markers are audit-only: an
//!   [`crate::markers::L2B_APPROVED`] emission is **not** authority for
//!   any downstream action — the structured [`ApprovalDecision`] is the
//!   single source of truth.
//! - Approval is bound to BOTH `step_id` and `preview_sha256`. Replay
//!   of a prior approval against a different preview is refused.
//! - Checkpoint baseline is verified via the caller-supplied
//!   [`ApprovalContext::checkpoint_baseline_unchanged`] flag. The
//!   primitive does not perform filesystem IO; the caller is expected
//!   to re-check the on-disk baseline via the slice-2 checkpoint store
//!   and pass the boolean in.

use crate::diff_preview::{is_valid_step_id, PreviewRecord};

/// Exit code reserved for an approval refusal surfaced by the CLI.
/// Slice 3a documents the binding; nothing in this module actually
/// terminates the process — that remains a CLI responsibility.
pub const EXIT_APPROVAL_DENIED: i32 = 7;

/// Exit code reserved for a rollback failure in a later slice. Pinned
/// here so the lib-level re-export shape is stable; slice 3a never
/// emits this.
pub const EXIT_ROLLBACK_FAILED: i32 = 8;

/// Hex length of a SHA-256 digest (32 bytes, lowercase hex).
pub const PREVIEW_SHA256_HEX_LEN: usize = 64;

/// Result of evaluating a candidate approval string against a
/// [`PreviewRecord`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Operator input is well-formed and binds to the supplied
    /// preview. The caller MAY (in a future slice) proceed with the
    /// write; slice 3a defines only the decision.
    Approved {
        step_id: String,
        preview_sha256: String,
    },
    /// Operator input was refused. The variant carries the precise
    /// reason for audit and operator feedback.
    Refused(ApprovalRefusal),
}

/// Reasons an approval may be refused. Each variant is mutually
/// exclusive; the parser returns the *first* matching refusal so
/// operators see one stable cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRefusal {
    /// Input contained ANSI escape sequences (`ESC` / `\x1b`).
    AnsiEscape,
    /// Input contained C0/C1 control bytes other than the single
    /// terminating newline.
    ControlChars,
    /// Input contained zero-width or bidirectional-override
    /// characters.
    ZeroWidthOrBidi,
    /// Input contained Unicode confusables for ASCII separators or
    /// the `apply` keyword (e.g., fullwidth `ａｐｐｌｙ`).
    UnicodeConfusable,
    /// Input contained quote characters (`"`, `'`, backticks, smart
    /// quotes).
    QuotedVariant,
    /// Input contained a batch / multi-step approval form (multiple
    /// `apply` tokens, semicolons, ampersands, or newlines in the
    /// payload).
    BatchSyntax,
    /// Input requested preapproval (`--yes`, `--auto`, `auto-apply`,
    /// `preapprove`, etc.).
    Preapproval,
    /// First token is not the exact ASCII-lowercase `apply` keyword.
    KeywordInvalid,
    /// Wrong number of tokens (must be exactly three:
    /// `apply <step-id> <preview_sha256>`).
    ArgCount,
    /// Step-id failed `^[A-Za-z0-9_.-]{1,128}$` or was bare `.`/`..`.
    StepIdShape,
    /// Preview-hash token was not 64 lowercase hex chars.
    PreviewHashShape,
    /// Step-id was well-formed but did not match
    /// `PreviewRecord::step_id`.
    StepIdMismatch,
    /// Preview-hash was well-formed but did not match
    /// `PreviewRecord::preview_sha256`. Catches replay of a prior
    /// approval against a different preview.
    PreviewHashMismatch,
    /// `PreviewRecord::is_binary == true`.
    PreviewBinary,
    /// `PreviewRecord::is_redacted == true`.
    PreviewRedacted,
    /// `PreviewRecord::is_truncated == true`.
    PreviewTruncated,
    /// `ApprovalContext::checkpoint_baseline_unchanged == false`.
    CheckpointDrift,
}

impl ApprovalRefusal {
    /// CLI exit code for any refusal: [`EXIT_APPROVAL_DENIED`].
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        EXIT_APPROVAL_DENIED
    }

    /// Stable human-readable cause. The audit log uses this; the
    /// variant is the contract for programmatic consumers.
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match self {
            Self::AnsiEscape => "ansi-escape-in-approval",
            Self::ControlChars => "control-chars-in-approval",
            Self::ZeroWidthOrBidi => "zero-width-or-bidi-in-approval",
            Self::UnicodeConfusable => "unicode-confusable-in-approval",
            Self::QuotedVariant => "quoted-variant-in-approval",
            Self::BatchSyntax => "batch-syntax-in-approval",
            Self::Preapproval => "preapproval-refused",
            Self::KeywordInvalid => "apply-keyword-invalid",
            Self::ArgCount => "approval-arg-count-invalid",
            Self::StepIdShape => "approval-step-id-shape-invalid",
            Self::PreviewHashShape => "approval-preview-hash-shape-invalid",
            Self::StepIdMismatch => "approval-step-id-mismatch",
            Self::PreviewHashMismatch => "approval-preview-hash-mismatch",
            Self::PreviewBinary => "preview-binary-non-approvable",
            Self::PreviewRedacted => "preview-redacted-non-approvable",
            Self::PreviewTruncated => "preview-truncated-non-approvable",
            Self::CheckpointDrift => "checkpoint-baseline-changed",
        }
    }
}

/// External context an approval evaluation depends on. Slice 3a keeps
/// the structure small; future slices may grow it.
#[derive(Debug, Clone, Copy)]
pub struct ApprovalContext<'a> {
    pub preview: &'a PreviewRecord,
    /// Whether the checkpoint store still observes the same pre-write
    /// baseline that produced `preview`. The primitive does not look
    /// at the filesystem; the caller re-verifies via the checkpoint
    /// manifest and passes the result here.
    pub checkpoint_baseline_unchanged: bool,
}

/// Evaluate a raw approval string against a [`PreviewRecord`].
///
/// The function is total: it always returns either
/// [`ApprovalDecision::Approved`] or
/// [`ApprovalDecision::Refused`]. There is no `Err` channel — every
/// refusal cause is enumerated.
#[must_use]
pub fn evaluate_approval(raw: &str, ctx: ApprovalContext<'_>) -> ApprovalDecision {
    // 1. Reject any character-level red flags before parsing.
    if let Some(refusal) = pre_parse_char_refusal(raw) {
        return ApprovalDecision::Refused(refusal);
    }

    // 2. Strict tokenization on raw ASCII space, requiring no leading
    //    or trailing whitespace except a single optional trailing
    //    newline.
    let Some(payload) = strip_single_trailing_newline(raw) else {
        return ApprovalDecision::Refused(ApprovalRefusal::ControlChars);
    };
    if payload.starts_with(' ') || payload.ends_with(' ') {
        return ApprovalDecision::Refused(ApprovalRefusal::ArgCount);
    }
    if payload.contains("  ") {
        return ApprovalDecision::Refused(ApprovalRefusal::ArgCount);
    }

    // 3. Preapproval / batch-syntax refusal pre-parse.
    if contains_batch_syntax(payload) {
        return ApprovalDecision::Refused(ApprovalRefusal::BatchSyntax);
    }
    if looks_like_preapproval(payload) {
        return ApprovalDecision::Refused(ApprovalRefusal::Preapproval);
    }

    let tokens: Vec<&str> = payload.split(' ').collect();
    if tokens.len() != 3 {
        return ApprovalDecision::Refused(ApprovalRefusal::ArgCount);
    }

    // 4. Keyword: exact ASCII lowercase `apply`.
    if tokens[0] != "apply" {
        if tokens[0].eq_ignore_ascii_case("apply") {
            return ApprovalDecision::Refused(ApprovalRefusal::KeywordInvalid);
        }
        return ApprovalDecision::Refused(ApprovalRefusal::KeywordInvalid);
    }

    // 5. Step-id shape.
    let step_id = tokens[1];
    if !is_valid_step_id(step_id) {
        return ApprovalDecision::Refused(ApprovalRefusal::StepIdShape);
    }

    // 6. Preview-hash shape.
    let preview_sha = tokens[2];
    if !is_lowercase_hex_64(preview_sha) {
        return ApprovalDecision::Refused(ApprovalRefusal::PreviewHashShape);
    }

    // 7. Bind step-id and preview-hash to the record.
    if step_id != ctx.preview.step_id {
        return ApprovalDecision::Refused(ApprovalRefusal::StepIdMismatch);
    }
    if preview_sha != ctx.preview.preview_sha256 {
        return ApprovalDecision::Refused(ApprovalRefusal::PreviewHashMismatch);
    }

    // 8. Preview must be approvable.
    if ctx.preview.is_binary {
        return ApprovalDecision::Refused(ApprovalRefusal::PreviewBinary);
    }
    if ctx.preview.is_redacted {
        return ApprovalDecision::Refused(ApprovalRefusal::PreviewRedacted);
    }
    if ctx.preview.is_truncated {
        return ApprovalDecision::Refused(ApprovalRefusal::PreviewTruncated);
    }

    // 9. Checkpoint baseline must still match.
    if !ctx.checkpoint_baseline_unchanged {
        return ApprovalDecision::Refused(ApprovalRefusal::CheckpointDrift);
    }

    ApprovalDecision::Approved {
        step_id: step_id.to_string(),
        preview_sha256: preview_sha.to_string(),
    }
}

// =========================================================================
// Helpers
// =========================================================================

fn pre_parse_char_refusal(raw: &str) -> Option<ApprovalRefusal> {
    for ch in raw.chars() {
        if ch == '\u{1B}' {
            return Some(ApprovalRefusal::AnsiEscape);
        }
        if is_zero_width(ch) || is_bidi_control(ch) {
            return Some(ApprovalRefusal::ZeroWidthOrBidi);
        }
        if is_quote_like(ch) {
            return Some(ApprovalRefusal::QuotedVariant);
        }
        if is_confusable(ch) {
            return Some(ApprovalRefusal::UnicodeConfusable);
        }
        // Allow only ASCII space, the digits/letters/_/./- of step-id
        // and hex, and a single trailing newline (handled separately).
        // C0 controls other than '\n' get refused; '\n' is allowed
        // exactly once at the very end via strip_single_trailing_newline.
        if ch.is_control() && ch != '\n' {
            return Some(ApprovalRefusal::ControlChars);
        }
    }
    None
}

fn strip_single_trailing_newline(raw: &str) -> Option<&str> {
    let stripped = raw.strip_suffix('\n').unwrap_or(raw);
    if stripped.contains('\n') {
        // Embedded newline beyond the single trailing one.
        return None;
    }
    Some(stripped)
}

fn contains_batch_syntax(s: &str) -> bool {
    if s.contains(';') || s.contains('&') || s.contains('|') {
        return true;
    }
    // Multiple `apply` tokens.
    if s.matches("apply ").count() > 1 {
        return true;
    }
    false
}

fn looks_like_preapproval(s: &str) -> bool {
    const FORBIDDEN: &[&str] = &[
        "--yes",
        "-y",
        "--auto",
        "--force",
        "preapprove",
        "preapproval",
        "auto-apply",
        "autoapply",
        "always",
        "skip-prompt",
    ];
    let lower = s.to_ascii_lowercase();
    for f in FORBIDDEN {
        for tok in lower.split(' ') {
            if tok == *f {
                return true;
            }
        }
    }
    false
}

fn is_lowercase_hex_64(s: &str) -> bool {
    s.len() == PREVIEW_SHA256_HEX_LEN && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_zero_width(c: char) -> bool {
    matches!(
        c as u32,
        0x200B..=0x200D | 0xFEFF | 0x2060..=0x2064 | 0x180E
    )
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c as u32,
        0x202A..=0x202E | 0x2066..=0x2069
    )
}

fn is_quote_like(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '`'
            | '\u{2018}'
            | '\u{2019}'
            | '\u{201C}'
            | '\u{201D}'
            | '\u{2032}'
            | '\u{2033}'
    )
}

fn is_confusable(c: char) -> bool {
    matches!(
        c,
        // Fullwidth ASCII letters / space / digits.
        '\u{FF01}'
            ..='\u{FF5E}'
            | '\u{3000}'
            // Cyrillic confusables for ASCII a, p, l, y, x, e.
            | '\u{0430}'
            | '\u{0440}'
            | '\u{0435}'
            | '\u{0445}'
            | '\u{04CF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        step: &str,
        hash: &str,
        binary: bool,
        redacted: bool,
        truncated: bool,
    ) -> PreviewRecord {
        PreviewRecord {
            preview_id: "01HZZZZZZZZZZZZZZZZZZZZZZ0".to_string(),
            step_id: step.to_string(),
            target_relative_path_sanitized: "src/lib.rs".to_string(),
            target_absolute_path_sanitized: "/ws/src/lib.rs".to_string(),
            before_sha256: "a".repeat(64),
            after_sha256: "b".repeat(64),
            preview_sha256: hash.to_string(),
            checkpoint_run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            checkpoint_step_id: step.to_string(),
            is_binary: binary,
            is_redacted: redacted,
            is_truncated: truncated,
            created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
            preview_format_version: 1,
        }
    }

    fn ctx(rec: &PreviewRecord, baseline_ok: bool) -> ApprovalContext<'_> {
        ApprovalContext {
            preview: rec,
            checkpoint_baseline_unchanged: baseline_ok,
        }
    }

    #[test]
    fn happy_path_approves() {
        let hash = "c".repeat(64);
        let rec = record("step-1", &hash, false, false, false);
        let input = format!("apply step-1 {hash}");
        let out = evaluate_approval(&input, ctx(&rec, true));
        assert_eq!(
            out,
            ApprovalDecision::Approved {
                step_id: "step-1".to_string(),
                preview_sha256: hash,
            }
        );
    }

    #[test]
    fn missing_hash_refuses_arg_count() {
        let rec = record("step-1", &"c".repeat(64), false, false, false);
        let out = evaluate_approval("apply step-1", ctx(&rec, true));
        assert_eq!(out, ApprovalDecision::Refused(ApprovalRefusal::ArgCount));
    }

    #[test]
    fn case_variant_refuses_keyword() {
        let hash = "c".repeat(64);
        let rec = record("step-1", &hash, false, false, false);
        let out = evaluate_approval(&format!("APPLY step-1 {hash}"), ctx(&rec, true));
        assert_eq!(
            out,
            ApprovalDecision::Refused(ApprovalRefusal::KeywordInvalid)
        );
    }
}
