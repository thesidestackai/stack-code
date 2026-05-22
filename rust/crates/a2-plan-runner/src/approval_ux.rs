//! A2-L2b workspace-write approval UX helpers (slice 3b).
//!
//! Pure helpers that render the Slice 3a preview / approval primitives
//! for future operator-facing UX, and a thin convenience that evaluates
//! one supplied operator input string against a
//! [`crate::diff_preview::PreviewRecord`].
//!
//! # Hard contract (slice 3b)
//!
//! - [`crate::diff_preview::PreviewRecord`] is authoritative.
//! - [`crate::diff_preview::PreviewDisplay`] is non-authoritative.
//! - [`crate::approval::ApprovalDecision`] is authoritative.
//! - Markers are audit-only — surfacing them through
//!   [`ApprovalPromptRender::audit_markers`] is never authority for an
//!   approval outcome.
//! - No filesystem writes, no child-process invocations, no broker, no
//!   LLM, no network, no terminal I/O of any kind.
//! - Not wired into the L1b runner's plan-executor entry point.
//! - All output is returned by value: rendered strings and structured
//!   decisions only.
//! - Rendered text is sanitized for terminal safety: no ANSI escape
//!   sequences, no C0/C1 control bytes other than `\n` and `\t`, no NUL
//!   bytes, no zero-width characters, no bidi-override characters.
//! - The absolute target path is never included in operator-facing
//!   output by default; only the sanitized relative path is surfaced.

use crate::approval::{evaluate_approval, ApprovalContext, ApprovalDecision};
use crate::diff_preview::{PreviewDisplay, PreviewRecord};
use crate::markers::{
    L2B_APPROVAL_PROMPT, L2B_BINARY_PREVIEW, L2B_DIFF_PREVIEW_READY, L2B_DIFF_REDACTED,
    L2B_DIFF_TRUNCATED,
};

/// Pure result of an operator-prompt rendering.
///
/// `text` is safe for printing to a terminal under the slice-3b
/// terminal-safety rule: no ANSI escape sequences, no C0/C1 control
/// bytes other than `\n` and `\t`, no NUL bytes, no zero-width
/// characters, no bidi-override characters.
///
/// `audit_markers` are stable operator-facing tokens drawn from
/// [`crate::markers`]. They are **audit-only**: emitting or surfacing
/// them is never authority for an approval outcome. The structured
/// [`ApprovalDecision`] produced by
/// [`crate::approval::evaluate_approval`] (or its convenience wrapper
/// [`evaluate_operator_input`]) is the single source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalPromptRender {
    pub text: String,
    pub audit_markers: Vec<&'static str>,
}

/// Render an operator-facing preview view for `record`.
///
/// Includes the canonical metadata header and the sanitized diff body
/// from `display.rendered`. Does **not** include the "To approve, type
/// exactly" instruction line — use [`render_approval_prompt`] for that.
///
/// If `record` is non-approvable, the output collapses to the same
/// metadata-only summary produced by [`render_non_approvable_summary`].
#[must_use]
pub fn render_preview_for_operator(
    record: &PreviewRecord,
    display: &PreviewDisplay,
) -> ApprovalPromptRender {
    if !record.is_approvable() {
        return render_non_approvable_summary(record);
    }
    let mut text = String::with_capacity(512 + display.rendered.len());
    push_header(&mut text, record);
    text.push('\n');
    text.push_str("--- Diff Preview ---\n");
    text.push_str(&display.rendered);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    if !is_terminal_safe(&text) {
        return render_non_approvable_summary(record);
    }
    ApprovalPromptRender {
        text,
        audit_markers: vec![L2B_DIFF_PREVIEW_READY],
    }
}

/// Render the full operator approval prompt for `record`.
///
/// Identical to [`render_preview_for_operator`] except the prompt
/// embeds the exact, single-line approval command
///
/// ```text
/// apply <step-id> <preview_sha256>
/// ```
///
/// ahead of the diff body. If `record` is non-approvable, returns the
/// same output as [`render_non_approvable_summary`] — no approval
/// command is surfaced for a non-approvable preview.
#[must_use]
pub fn render_approval_prompt(
    record: &PreviewRecord,
    display: &PreviewDisplay,
) -> ApprovalPromptRender {
    if !record.is_approvable() {
        return render_non_approvable_summary(record);
    }
    let mut text = String::with_capacity(512 + display.rendered.len());
    push_header(&mut text, record);
    text.push('\n');
    text.push_str("To approve, type exactly:\n");
    text.push_str("apply ");
    text.push_str(&record.step_id);
    text.push(' ');
    text.push_str(&record.preview_sha256);
    text.push('\n');
    text.push('\n');
    text.push_str("--- Diff Preview ---\n");
    text.push_str(&display.rendered);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    if !is_terminal_safe(&text) {
        return render_non_approvable_summary(record);
    }
    ApprovalPromptRender {
        text,
        audit_markers: vec![L2B_DIFF_PREVIEW_READY, L2B_APPROVAL_PROMPT],
    }
}

/// Render a non-approvable summary for `record`.
///
/// Lists every applicable reason (`binary`, `redacted`, `truncated`)
/// and states explicitly that no approval command is accepted. Never
/// embeds a diff body, never embeds the absolute target path.
#[must_use]
pub fn render_non_approvable_summary(record: &PreviewRecord) -> ApprovalPromptRender {
    let mut text = String::with_capacity(256);
    text.push_str("A2-L2b preview is not approvable\n");
    text.push('\n');
    text.push_str("Reason:\n");
    let mut markers: Vec<&'static str> = Vec::new();
    if record.is_binary {
        text.push_str("- binary\n");
        markers.push(L2B_BINARY_PREVIEW);
    }
    if record.is_redacted {
        text.push_str("- redacted\n");
        markers.push(L2B_DIFF_REDACTED);
    }
    if record.is_truncated {
        text.push_str("- truncated\n");
        markers.push(L2B_DIFF_TRUNCATED);
    }
    if !record.is_binary && !record.is_redacted && !record.is_truncated {
        // Defensive: a caller may pass an approvable record by mistake;
        // surface a stable "not approvable" line so the operator output
        // remains consistent with the approval-refused contract.
        text.push_str("- not_approvable\n");
    }
    text.push('\n');
    text.push_str("No approval command is accepted for this preview.\n");
    if !is_terminal_safe(&text) {
        // All strings owned by this helper are ASCII; the branch is
        // unreachable under the slice-3b sources. The fallback preserves
        // the "no approval command" contract on the off chance it ever
        // triggers from a future change.
        return ApprovalPromptRender {
            text: "A2-L2b preview is not approvable\n\nNo approval command is accepted for this preview.\n"
                .to_string(),
            audit_markers: markers,
        };
    }
    ApprovalPromptRender {
        text,
        audit_markers: markers,
    }
}

/// Evaluate a raw operator approval string against `preview`.
///
/// Convenience wrapper around the authoritative
/// [`crate::approval::evaluate_approval`] parser: folds
/// `checkpoint_baseline_unchanged` and `preview` into an
/// [`ApprovalContext`] in one place so callers do not have to.
///
/// The return value is the same [`ApprovalDecision`] the underlying
/// parser would produce; this helper adds no parsing of its own and
/// never overrides the parser's verdict.
#[must_use]
pub fn evaluate_operator_input(
    preview: &PreviewRecord,
    input: &str,
    checkpoint_baseline_unchanged: bool,
) -> ApprovalDecision {
    evaluate_approval(
        input,
        ApprovalContext {
            preview,
            checkpoint_baseline_unchanged,
        },
    )
}

// =========================================================================
// Internal
// =========================================================================

fn push_header(buf: &mut String, record: &PreviewRecord) {
    use std::fmt::Write as _;
    let _ = writeln!(buf, "A2-L2b approval required");
    buf.push('\n');
    let _ = writeln!(buf, "Step: {}", record.step_id);
    let _ = writeln!(buf, "Target: {}", record.target_relative_path_sanitized);
    let _ = writeln!(buf, "Preview ID: {}", record.preview_id);
    let _ = writeln!(buf, "Before SHA256: {}", record.before_sha256);
    let _ = writeln!(buf, "After SHA256: {}", record.after_sha256);
    let _ = writeln!(buf, "Preview SHA256: {}", record.preview_sha256);
    let _ = writeln!(
        buf,
        "Checkpoint: {}/{}",
        record.checkpoint_run_id, record.checkpoint_step_id
    );
}

fn is_terminal_safe(s: &str) -> bool {
    for ch in s.chars() {
        if ch == '\n' || ch == '\t' {
            continue;
        }
        if ch == '\u{1B}' {
            return false;
        }
        if ch.is_control() {
            return false;
        }
        if is_zero_width(ch) || is_bidi_control(ch) {
            return false;
        }
    }
    true
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

// =========================================================================
// Unit tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_terminal_safe_accepts_plain_ascii() {
        assert!(is_terminal_safe("hello world\nline two\tindented\n"));
    }

    #[test]
    fn is_terminal_safe_rejects_ansi_escape() {
        assert!(!is_terminal_safe("hello \u{1B}[31mred\u{1B}[0m"));
    }

    #[test]
    fn is_terminal_safe_rejects_zero_width() {
        assert!(!is_terminal_safe("a\u{200B}b"));
    }

    #[test]
    fn is_terminal_safe_rejects_bidi_override() {
        assert!(!is_terminal_safe("a\u{202E}b"));
    }

    #[test]
    fn is_terminal_safe_rejects_nul() {
        assert!(!is_terminal_safe("a\0b"));
    }
}
