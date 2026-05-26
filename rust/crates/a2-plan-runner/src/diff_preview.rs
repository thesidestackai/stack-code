//! A2-L2b workspace-write diff-preview primitives (slice 3a).
//!
//! This module produces a *structured* [`PreviewRecord`] alongside a
//! sanitized human-readable [`PreviewDisplay`] for a candidate
//! workspace-write change. The structured record is authoritative for
//! approval binding; the human display is for review only and is
//! non-authoritative.
//!
//! # Hard contract (slice 3a)
//!
//! - No filesystem writes anywhere in this module.
//! - No child-process invocations: no `std::process` calls, no git
//!   plumbing, no patch/apply shell-outs.
//! - No broker, no LLM provider, no model calls.
//! - Not wired into the L1b runner's plan-executor entry point.
//! - Display is non-authoritative: only the [`PreviewRecord`] (and the
//!   `preview_sha256` it pins) ever binds an [`crate::approval::ApprovalDecision`].
//! - The [`similar`] crate is used for display-only unified-diff rendering.
//!   It is **never** used as a patch/apply format — slice 3a writes nothing
//!   to a target file.
//!
//! # Operator contract
//!
//! - On any safe text change, [`build_preview`] returns
//!   `(PreviewRecord, PreviewDisplay)` with `is_binary = false`,
//!   `is_redacted = false`, `is_truncated = false`, and an embedded
//!   unified diff in `PreviewDisplay::rendered`.
//! - Binary detection (NUL byte or invalid UTF-8) yields a record with
//!   `is_binary = true` and a *summary-only* display; no bytes leak.
//! - Truncation (line cap or byte cap) yields `is_truncated = true` and
//!   a non-approvable preview.
//! - Redaction (any matched secret-like pattern after primary render)
//!   yields `is_redacted = true` and a non-approvable preview. The
//!   final-output post-scan is **fail-closed**: if any residual
//!   secret-like material remains after the first redaction pass, the
//!   display is replaced with a safe metadata summary and the preview
//!   is marked refused.
//! - The `preview_sha256` is:
//!
//!   ```text
//!   sha256(canonical_preview_record_for_approval
//!          || "\n---DISPLAY---\n"
//!          || rendered_sanitized_preview)
//!   ```
//!
//!   The canonical subset is a fixed, deterministically-ordered text
//!   block over the approval-binding fields only (see
//!   [`canonical_preview_record_for_approval`]). Changing any field in
//!   the subset, or changing the rendered display bytes, changes the
//!   hash.
//!
//! # Non-goals (slice 3a)
//!
//! - No target-file writes.
//! - No rollback execution.
//! - No CLI flags for approval bypass.
//! - No interactive prompts in this crate (the CLI owns prompting).

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use ulid::Ulid;

/// Schema version for the on-disk / on-wire shape of [`PreviewRecord`].
/// Bumping this is a breaking change for any approval consumer that
/// recomputes `preview_sha256`. Slice 3a ships v1.
pub const PREVIEW_FORMAT_VERSION: u32 = 1;

/// Hard cap on lines emitted in the unified diff body. Beyond this the
/// preview is marked truncated and made non-approvable.
pub const MAX_DIFF_LINES: usize = 2_000;

/// Hard cap on bytes emitted in the unified diff body. Beyond this the
/// preview is marked truncated and made non-approvable.
pub const MAX_DIFF_BYTES: usize = 256 * 1024;

/// Hard cap on the size (in bytes) of *either* the `before` or `after`
/// content blob considered for a text preview. Anything larger is
/// surfaced as a metadata summary only.
pub const MAX_CONTENT_BYTES_FOR_DIFF: usize = 1_000_000;

/// Separator between the canonical record and the rendered display in
/// the `preview_sha256` pre-image. Public so downstream verifiers can
/// reproduce the hash independently of this crate.
pub const HASH_DISPLAY_SEPARATOR: &str = "\n---DISPLAY---\n";

/// Discriminator string used inside the canonical record header. Pins
/// the hash semantics to the slice-3a contract and prevents accidental
/// collision with arbitrary text.
pub const CANONICAL_HEADER: &str = "A2-L2B-PREVIEW-RECORD-V1";

/// Structured, authoritative record for a candidate workspace-write
/// preview.
///
/// Approval is bound to `preview_sha256` (and the `step_id` it carries).
/// Operators and CI consumers compare this struct — never the rendered
/// display bytes — to decide whether an inbound approval matches the
/// preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRecord {
    /// ULID identifying this preview. New on every [`build_preview`]
    /// call, even for identical inputs. Operators can grep the ULID
    /// suffix off the audit marker stream.
    pub preview_id: String,

    /// Plan step the preview belongs to. Validated against the same
    /// step-id shape used by the checkpoint store.
    pub step_id: String,

    /// Sanitized workspace-relative target path. Control characters,
    /// ANSI escapes, zero-width chars, and Unicode confusables are
    /// stripped/escaped before this field is filled.
    pub target_relative_path_sanitized: String,

    /// Sanitized absolute target path. Same sanitization as
    /// `target_relative_path_sanitized`.
    pub target_absolute_path_sanitized: String,

    /// Lowercase hex SHA-256 of the pre-write content, or the empty
    /// string when no pre-write content exists (file did not exist).
    pub before_sha256: String,

    /// Lowercase hex SHA-256 of the post-write content. Always present
    /// for slice-3a (caller supplies the after-bytes).
    pub after_sha256: String,

    /// Lowercase hex SHA-256 binding the canonical subset to the
    /// rendered display. See module docs.
    pub preview_sha256: String,

    /// Checkpoint run-id (ULID rendering) this preview pins to.
    pub checkpoint_run_id: String,

    /// Checkpoint step-id this preview pins to. Usually matches
    /// `step_id`; kept separate so a future slice can re-checkpoint
    /// under a derived id without breaking the binding.
    pub checkpoint_step_id: String,

    /// Either side of the diff was detected as binary (NUL byte or
    /// invalid UTF-8). Non-approvable in slice 3a.
    pub is_binary: bool,

    /// At least one redaction pattern matched. Non-approvable in slice 3a.
    pub is_redacted: bool,

    /// The rendered diff hit a deterministic line or byte cap.
    /// Non-approvable in slice 3a.
    pub is_truncated: bool,

    /// RFC 3339 UTC string with nanosecond fractional precision and
    /// trailing `Z`, e.g. `"2026-05-21T22:43:23.123456789Z"`.
    pub created_at_utc: String,

    /// Schema version. Slice-3a ships [`PREVIEW_FORMAT_VERSION`].
    pub preview_format_version: u32,
}

impl PreviewRecord {
    /// Returns `true` iff the preview is eligible for an
    /// [`crate::approval::ApprovalDecision::Approved`]. Slice 3a refuses
    /// binary, redacted, and truncated previews as non-approvable.
    #[must_use]
    pub fn is_approvable(&self) -> bool {
        !self.is_binary && !self.is_redacted && !self.is_truncated
    }
}

/// Sanitized human-readable preview. **Non-authoritative** for approval.
///
/// `rendered` is the bytes hashed into `PreviewRecord::preview_sha256`.
/// Any change to `rendered` changes the hash; the inverse is **not**
/// guaranteed (records with identical render but different metadata
/// also produce different hashes).
///
/// Serde derives are pinned to allow A2-L2b CLI consumers to round-trip
/// a `PreviewDisplay` through a preview bundle without forking the
/// in-memory shape. The non-authoritative contract is unchanged: the
/// CLI re-derives `preview_sha256` from the record subset plus this
/// `rendered` text and refuses any bundle whose hash does not match the
/// embedded record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewDisplay {
    /// Sanitized text. Safe to print to a TTY. Always ends with a
    /// trailing `\n`. Never contains raw control bytes, raw ANSI
    /// escapes, raw zero-width characters, or unredacted secret-shaped
    /// material.
    pub rendered: String,
}

/// Inputs to [`build_preview`].
#[derive(Debug, Clone)]
pub struct PreviewInputs<'a> {
    pub step_id: &'a str,
    pub target_relative_path: &'a Path,
    pub target_absolute_path: &'a Path,
    pub before: Option<&'a [u8]>,
    pub after: &'a [u8],
    pub checkpoint_run_id: &'a Ulid,
    pub checkpoint_step_id: &'a str,
    /// Caller-provided creation timestamp. Used as-is. Format is the
    /// caller's responsibility; the canonical hash treats it as opaque.
    pub created_at_utc: &'a str,
}

/// Why [`build_preview`] refused outright (before producing a record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewBuildError {
    /// Empty step-id or step-id failing `^[A-Za-z0-9_.-]{1,128}$`.
    InvalidStepId,
}

impl std::fmt::Display for PreviewBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStepId => write!(f, "invalid step_id"),
        }
    }
}

impl std::error::Error for PreviewBuildError {}

// =========================================================================
// Public API: build_preview
// =========================================================================

/// Construct a [`PreviewRecord`] + [`PreviewDisplay`] from raw content
/// blobs.
///
/// This function never opens a file handle, never invokes a child
/// process, and never calls the broker. All input is taken by
/// reference; the caller owns the I/O cost of producing `before` /
/// `after`.
///
/// # Errors
/// Returns [`PreviewBuildError::InvalidStepId`] for an empty or
/// shape-invalid `step_id`. All other "refusal" outcomes (binary,
/// truncated, redacted) are represented as flags on the returned
/// record, not as `Err` — they still produce a usable record for the
/// audit trail.
pub fn build_preview(
    inputs: &PreviewInputs<'_>,
) -> Result<(PreviewRecord, PreviewDisplay), PreviewBuildError> {
    if !is_valid_step_id(inputs.step_id) {
        return Err(PreviewBuildError::InvalidStepId);
    }
    let preview_id = Ulid::new().to_string();

    let rel = sanitize_path_display(inputs.target_relative_path);
    let abs = sanitize_path_display(inputs.target_absolute_path);

    let before_bytes = inputs.before.unwrap_or(&[]);
    let after_bytes = inputs.after;

    let before_sha = if inputs.before.is_some() {
        sha256_hex(before_bytes)
    } else {
        String::new()
    };
    let after_sha = sha256_hex(after_bytes);

    // Binary / oversize content gets a metadata-only render.
    let before_too_big = before_bytes.len() > MAX_CONTENT_BYTES_FOR_DIFF;
    let after_too_big = after_bytes.len() > MAX_CONTENT_BYTES_FOR_DIFF;
    let before_binary = is_binary_blob(before_bytes);
    let after_binary = is_binary_blob(after_bytes);
    let oversize = before_too_big || after_too_big;
    let binary = before_binary || after_binary;

    let mut is_binary_flag = false;
    let mut is_truncated_flag = false;
    let mut is_redacted_flag = false;

    let rendered = if binary || oversize {
        is_binary_flag = binary;
        if oversize && !binary {
            is_truncated_flag = true;
        }
        render_metadata_summary(
            &rel,
            &abs,
            inputs.before.is_some(),
            before_bytes.len(),
            after_bytes.len(),
            binary,
            oversize,
        )
    } else {
        // Both sides decode as UTF-8 here (binary check ruled out
        // invalid UTF-8). Lossless decode for the diff input.
        let before_text = std::str::from_utf8(before_bytes).unwrap_or("");
        let after_text = std::str::from_utf8(after_bytes).unwrap_or("");
        let (diff_body, truncated) = render_unified_diff(&rel, before_text, after_text);
        is_truncated_flag = truncated;
        let redacted_body = redact_preview_body(&diff_body);
        if redacted_body.was_modified {
            is_redacted_flag = true;
        }
        let mut candidate = redacted_body.text;
        // Fail-closed post-scan: any residual secret-shaped material
        // after the primary redaction pass collapses the preview to a
        // safe metadata-only summary.
        if contains_secret_like(&candidate) {
            is_redacted_flag = true;
            candidate = render_redaction_refused_summary(&rel, &abs);
        }
        candidate
    };

    let rendered = ensure_trailing_newline(rendered);

    let canonical = canonical_preview_record_for_approval(&CanonicalSubset {
        preview_id: &preview_id,
        step_id: inputs.step_id,
        target_relative_path_sanitized: &rel,
        before_sha256: &before_sha,
        after_sha256: &after_sha,
        checkpoint_run_id: &inputs.checkpoint_run_id.to_string(),
        checkpoint_step_id: inputs.checkpoint_step_id,
        is_binary: is_binary_flag,
        is_redacted: is_redacted_flag,
        is_truncated: is_truncated_flag,
        preview_format_version: PREVIEW_FORMAT_VERSION,
    });

    let preview_sha = preview_hash_from_parts(&canonical, &rendered);

    let record = PreviewRecord {
        preview_id,
        step_id: inputs.step_id.to_string(),
        target_relative_path_sanitized: rel,
        target_absolute_path_sanitized: abs,
        before_sha256: before_sha,
        after_sha256: after_sha,
        preview_sha256: preview_sha,
        checkpoint_run_id: inputs.checkpoint_run_id.to_string(),
        checkpoint_step_id: inputs.checkpoint_step_id.to_string(),
        is_binary: is_binary_flag,
        is_redacted: is_redacted_flag,
        is_truncated: is_truncated_flag,
        created_at_utc: inputs.created_at_utc.to_string(),
        preview_format_version: PREVIEW_FORMAT_VERSION,
    };

    Ok((record, PreviewDisplay { rendered }))
}

// =========================================================================
// Canonical record subset used in preview_sha256 pre-image
// =========================================================================

/// Approval-binding subset of [`PreviewRecord`]. Field order is the
/// canonical order; field set is the canonical set. Slice-3a freezes
/// both via [`PREVIEW_FORMAT_VERSION`] = 1.
#[derive(Debug)]
pub struct CanonicalSubset<'a> {
    pub preview_id: &'a str,
    pub step_id: &'a str,
    pub target_relative_path_sanitized: &'a str,
    pub before_sha256: &'a str,
    pub after_sha256: &'a str,
    pub checkpoint_run_id: &'a str,
    pub checkpoint_step_id: &'a str,
    pub is_binary: bool,
    pub is_redacted: bool,
    pub is_truncated: bool,
    pub preview_format_version: u32,
}

/// Render the canonical text used as the **first** half of the
/// `preview_sha256` pre-image.
///
/// Format is a fixed, line-oriented `KEY=VALUE` block with the
/// [`CANONICAL_HEADER`] discriminator on line one. Order is the
/// declaration order of [`CanonicalSubset`]; bumping
/// [`PREVIEW_FORMAT_VERSION`] is the only way to change either order
/// or membership without breaking external verifiers.
#[must_use]
pub fn canonical_preview_record_for_approval(subset: &CanonicalSubset<'_>) -> String {
    let mut out = String::with_capacity(512);
    let _ = writeln!(out, "{CANONICAL_HEADER}");
    let _ = writeln!(
        out,
        "preview_format_version={}",
        subset.preview_format_version
    );
    let _ = writeln!(out, "preview_id={}", subset.preview_id);
    let _ = writeln!(out, "step_id={}", subset.step_id);
    let _ = writeln!(
        out,
        "target_relative_path_sanitized={}",
        subset.target_relative_path_sanitized
    );
    let _ = writeln!(out, "before_sha256={}", subset.before_sha256);
    let _ = writeln!(out, "after_sha256={}", subset.after_sha256);
    let _ = writeln!(out, "checkpoint_run_id={}", subset.checkpoint_run_id);
    let _ = writeln!(out, "checkpoint_step_id={}", subset.checkpoint_step_id);
    let _ = writeln!(out, "is_binary={}", subset.is_binary);
    let _ = writeln!(out, "is_redacted={}", subset.is_redacted);
    let _ = writeln!(out, "is_truncated={}", subset.is_truncated);
    out
}

/// Compute the `preview_sha256` hex digest from its two pre-image
/// halves. Public so downstream consumers can verify the binding
/// without re-deriving canonical formatting.
#[must_use]
pub fn preview_hash_from_parts(canonical: &str, rendered_display: &str) -> String {
    let mut h = Sha256::new();
    h.update(canonical.as_bytes());
    h.update(HASH_DISPLAY_SEPARATOR.as_bytes());
    h.update(rendered_display.as_bytes());
    hex_lower(&h.finalize())
}

// =========================================================================
// Internal: rendering
// =========================================================================

fn render_unified_diff(rel_path: &str, before: &str, after: &str) -> (String, bool) {
    let diff = TextDiff::from_lines(before, after);
    let mut out = String::with_capacity(2_048);
    // Sanitized file headers. Use generic `a/<path>` / `b/<path>`
    // labels: the canonical struct carries the path already; the
    // header is for human display only.
    let _ = writeln!(out, "--- a/{rel_path}");
    let _ = writeln!(out, "+++ b/{rel_path}");

    let mut line_count: usize = 2; // headers count toward the cap
    let mut truncated = false;
    'outer: for group in diff.grouped_ops(3) {
        if group.is_empty() {
            continue;
        }
        // Synthesize a sanitized hunk header.
        let (old_start, old_len, new_start, new_len) = hunk_extents(&group);
        let header = format!(
            "@@ -{},{} +{},{} @@\n",
            // Display 1-based starts; clamp empty hunks to 0 for the
            // canonical `0,0` form like a textual unified-diff emits.
            display_start(old_start, old_len),
            old_len,
            display_start(new_start, new_len),
            new_len
        );
        if !push_line_capped(&mut out, &header, &mut line_count) {
            truncated = true;
            break 'outer;
        }
        for op in &group {
            for change in diff.iter_inline_changes(op) {
                let tag = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                let mut line = String::with_capacity(64);
                line.push(tag);
                for (_, slice) in change.iter_strings_lossy() {
                    line.push_str(&slice);
                }
                if !line.ends_with('\n') {
                    line.push('\n');
                }
                if !push_line_capped(&mut out, &line, &mut line_count) {
                    truncated = true;
                    break 'outer;
                }
            }
        }
    }
    // Sanitize all bytes (control / ANSI / zero-width). Pass over the
    // accumulated diff once: the input is already line-bounded.
    let sanitized = sanitize_display_bytes(&out);
    (sanitized, truncated)
}

fn hunk_extents(group: &[similar::DiffOp]) -> (usize, usize, usize, usize) {
    let mut old_start = usize::MAX;
    let mut old_end = 0usize;
    let mut new_start = usize::MAX;
    let mut new_end = 0usize;
    for op in group {
        let (os, ol, ns, nl) = match *op {
            similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => (old_index, len, new_index, len),
            similar::DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        old_start = old_start.min(os);
        old_end = old_end.max(os + ol);
        new_start = new_start.min(ns);
        new_end = new_end.max(ns + nl);
    }
    let old_len = old_end.saturating_sub(old_start);
    let new_len = new_end.saturating_sub(new_start);
    (old_start, old_len, new_start, new_len)
}

fn display_start(start: usize, len: usize) -> usize {
    if len == 0 && start == 0 {
        0
    } else {
        start + 1
    }
}

/// Push `line` onto `out` if doing so leaves the running total under
/// both [`MAX_DIFF_LINES`] and [`MAX_DIFF_BYTES`]. Returns `true` on
/// success; `false` if the cap is exceeded (caller marks the preview
/// truncated and stops accumulating).
fn push_line_capped(out: &mut String, line: &str, line_count: &mut usize) -> bool {
    if *line_count >= MAX_DIFF_LINES {
        return false;
    }
    if out.len() + line.len() > MAX_DIFF_BYTES {
        return false;
    }
    out.push_str(line);
    *line_count += 1;
    true
}

fn render_metadata_summary(
    rel: &str,
    abs: &str,
    pre_existed: bool,
    before_len: usize,
    after_len: usize,
    binary: bool,
    oversize: bool,
) -> String {
    let mut out = String::with_capacity(256);
    let _ = writeln!(out, "# preview: metadata-only");
    let _ = writeln!(out, "# target_relative_path: {rel}");
    let _ = writeln!(out, "# target_absolute_path: {abs}");
    let _ = writeln!(out, "# pre_existed: {pre_existed}");
    let _ = writeln!(out, "# before_size_bytes: {before_len}");
    let _ = writeln!(out, "# after_size_bytes: {after_len}");
    let _ = writeln!(out, "# is_binary: {binary}");
    let _ = writeln!(out, "# is_truncated_oversize: {oversize}");
    let _ = writeln!(out, "# body_omitted_reason: binary_or_oversize");
    sanitize_display_bytes(&out)
}

fn render_redaction_refused_summary(rel: &str, abs: &str) -> String {
    let mut out = String::with_capacity(256);
    let _ = writeln!(out, "# preview: redaction-refused");
    let _ = writeln!(out, "# target_relative_path: {rel}");
    let _ = writeln!(out, "# target_absolute_path: {abs}");
    let _ = writeln!(
        out,
        "# body_omitted_reason: residual_secret_after_redaction_fail_closed"
    );
    sanitize_display_bytes(&out)
}

fn ensure_trailing_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

// =========================================================================
// Internal: binary detection
// =========================================================================

fn is_binary_blob(bytes: &[u8]) -> bool {
    if bytes.contains(&0u8) {
        return true;
    }
    // Probe the first 8 KiB for non-UTF-8 / non-printable density.
    let head = if bytes.len() > 8192 {
        &bytes[..8192]
    } else {
        bytes
    };
    if std::str::from_utf8(head).is_err() {
        return true;
    }
    // Any C0 control byte that is not \t, \n, \r is treated as binary.
    head.iter()
        .any(|&b| matches!(b, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F))
}

// =========================================================================
// Internal: path + byte sanitization
// =========================================================================

fn sanitize_path_display(p: &Path) -> String {
    let raw = p.as_os_str().to_string_lossy().into_owned();
    sanitize_path_string(&raw)
}

fn sanitize_path_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if is_zero_width(ch) || is_bidi_control(ch) {
            out.push('\u{FFFD}');
            continue;
        }
        if ch == '\u{1B}' {
            // Escape any ANSI escape introducer.
            out.push_str("\\x1b");
            continue;
        }
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            out.push('\u{FFFD}');
            continue;
        }
        // Forbid newlines / tabs in a path display so injection of
        // synthetic header lines is impossible.
        if matches!(ch, '\n' | '\r' | '\t') {
            out.push('\u{FFFD}');
            continue;
        }
        if is_confusable_replacement(ch) {
            out.push(ascii_for_confusable(ch));
            continue;
        }
        out.push(ch);
    }
    out
}

fn sanitize_display_bytes(s: &str) -> String {
    // Display sanitization for the rendered diff body. Newlines are
    // preserved (they delimit diff lines). Tabs are preserved. Any
    // other C0/C1 control byte, ANSI escape, zero-width char, or
    // bidi-override gets escaped to a printable token.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' | '\t' => out.push(ch),
            // CR alone is suspicious in a unified diff — escape to
            // remove any chance of overstriking ANSI on terminals.
            '\r' => out.push_str("\\r"),
            '\u{1B}' => out.push_str("\\x1b"),
            c if is_zero_width(c) || is_bidi_control(c) => out.push('\u{FFFD}'),
            c if c.is_control() => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
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

fn is_confusable_replacement(c: char) -> bool {
    matches!(
        c,
        '\u{2044}' // FRACTION SLASH
            | '\u{2215}' // DIVISION SLASH
            | '\u{FF0F}' // FULLWIDTH SOLIDUS
            | '\u{FE68}' // SMALL REVERSE SOLIDUS
            | '\u{2010}'..='\u{2015}' // various hyphens
    )
}

fn ascii_for_confusable(c: char) -> char {
    match c {
        '\u{2044}' | '\u{2215}' | '\u{FF0F}' => '/',
        '\u{FE68}' => '\\',
        '\u{2010}'..='\u{2015}' => '-',
        other => other,
    }
}

// =========================================================================
// Internal: redaction
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedactionPass {
    text: String,
    was_modified: bool,
}

const REDACTED: &str = "[REDACTED]";

/// Apply the canonical slice-3a redaction patterns to a candidate
/// preview body. Returns the redacted text and a flag indicating
/// whether at least one pattern matched.
///
/// Intentionally conservative: pattern matches that hit are replaced
/// with [`REDACTED`] in-place, preserving surrounding context for
/// human review.
fn redact_preview_body(input: &str) -> RedactionPass {
    let mut text = input.to_string();
    let mut modified = false;

    // 1) Multi-line PEM-style key blocks. Replace whole block.
    text = redact_pem_blocks(&text, &mut modified);

    // 2) Bearer/Basic auth headers + cookie + amazon/google sig'd URLs.
    text = redact_header_like(&text, &mut modified);

    // 3) `key=value` and `key: value` patterns (case-insensitive),
    //    keys taken from a curated list.
    text = redact_kv_secrets(&text, &mut modified);

    // 4) URL credentials: `scheme://user:pass@host`.
    text = redact_url_credentials(&text, &mut modified);

    // 5) Standalone tokens by prefix (vendor-specific).
    text = redact_vendor_tokens(&text, &mut modified);

    // 6) JWT-shaped strings.
    text = redact_jwt_like(&text, &mut modified);

    // 7) AWS access key IDs (`AKIA...`, `ASIA...`).
    text = redact_aws_keyids(&text, &mut modified);

    // 8) `X-Amz-Signature` / `X-Goog-Signature` / SAS `sig=` query
    //    params already covered by kv_secrets via key matching, but
    //    re-pass to catch the URL-query form.
    text = redact_query_signatures(&text, &mut modified);

    RedactionPass {
        text,
        was_modified: modified,
    }
}

fn redact_pem_blocks(text: &str, modified: &mut bool) -> String {
    const BLOCKS: &[&str] = &[
        "PRIVATE KEY",
        "OPENSSH PRIVATE KEY",
        "RSA PRIVATE KEY",
        "EC PRIVATE KEY",
        "DSA PRIVATE KEY",
        "ENCRYPTED PRIVATE KEY",
        "PGP PRIVATE KEY",
        "PGP PRIVATE KEY BLOCK",
    ];
    let mut out = text.to_string();
    for label in BLOCKS {
        let begin = format!("-----BEGIN {label}-----");
        let end = format!("-----END {label}-----");
        while let Some(b_idx) = out.find(&begin) {
            let after_b = b_idx + begin.len();
            let Some(rel_end) = out[after_b..].find(&end) else {
                // Half-open block: redact from begin to end of buffer.
                out.replace_range(b_idx.., REDACTED);
                *modified = true;
                break;
            };
            let e_idx = after_b + rel_end + end.len();
            out.replace_range(b_idx..e_idx, REDACTED);
            *modified = true;
        }
    }
    out
}

fn redact_header_like(text: &str, modified: &mut bool) -> String {
    const HEADERS: &[&str] = &[
        "Authorization: Bearer",
        "Authorization: Basic",
        "Proxy-Authorization:",
        "Cookie:",
        "Set-Cookie:",
        "X-Api-Key:",
        "x-api-key:",
        "X-Amz-Security-Token:",
    ];
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let mut redact = false;
        for h in HEADERS {
            if line.to_ascii_lowercase().contains(&h.to_ascii_lowercase()) {
                redact = true;
                break;
            }
        }
        if redact {
            // Preserve any leading diff marker (`+`/`-`/` `).
            let (prefix, _rest) = split_diff_marker(line);
            out.push_str(prefix);
            out.push_str(REDACTED);
            if !out.ends_with('\n') {
                out.push('\n');
            }
            *modified = true;
        } else {
            out.push_str(line);
        }
    }
    out
}

fn split_diff_marker(line: &str) -> (&str, &str) {
    if let Some(rest) = line.strip_prefix('+') {
        ("+", rest)
    } else if let Some(rest) = line.strip_prefix('-') {
        ("-", rest)
    } else if let Some(rest) = line.strip_prefix(' ') {
        (" ", rest)
    } else {
        ("", line)
    }
}

fn redact_kv_secrets(text: &str, modified: &mut bool) -> String {
    const KEYS: &[&str] = &[
        "password",
        "passwd",
        "pwd",
        "passphrase",
        "api_key",
        "apikey",
        "api-key",
        "access_key",
        "secret_key",
        "client_secret",
        "private_key",
        "aws_secret_access_key",
        "aws_access_key_id",
        "bearer_token",
        "refresh_token",
        "id_token",
        "session_token",
        "_authtoken",
        "authtoken",
        "auth_token",
        "database_url",
        "token",
        "key",
        "sig",
        "signature",
        "x-amz-signature",
        "x-goog-signature",
    ];
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let lower = line.to_ascii_lowercase();
        let mut hit_idx: Option<usize> = None;
        let mut hit_key_len = 0usize;
        for key in KEYS {
            if let Some(pos) = find_kv_key(&lower, key) {
                hit_idx = Some(pos);
                hit_key_len = key.len();
                break;
            }
        }
        if hit_idx.is_some() {
            let (prefix, rest) = split_diff_marker(line);
            // Replace from the value side of the separator onward.
            let Some((head, tail)) = split_value_side(rest, hit_key_len) else {
                out.push_str(line);
                continue;
            };
            out.push_str(prefix);
            out.push_str(head);
            out.push_str(REDACTED);
            // Preserve trailing newline / continuation only.
            let trailing: String = tail
                .chars()
                .rev()
                .take_while(|c| matches!(c, '\n' | '\r'))
                .collect();
            let trailing: String = trailing.chars().rev().collect();
            if trailing.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            } else {
                out.push_str(&trailing);
            }
            *modified = true;
        } else {
            out.push_str(line);
        }
    }
    out
}

fn find_kv_key(lower: &str, key: &str) -> Option<usize> {
    let mut search_start = 0usize;
    while let Some(rel) = lower[search_start..].find(key) {
        let pos = search_start + rel;
        let before_ok = pos == 0
            || lower.as_bytes()[pos - 1].is_ascii_whitespace()
            || matches!(
                lower.as_bytes()[pos - 1],
                b'"' | b'\'' | b'{' | b',' | b';' | b'(' | b'+' | b'-' | b'\t' | b'<' | b'&'
            );
        let after = pos + key.len();
        let after_ok = after < lower.len() && {
            // Skip any spaces between the key and the separator.
            let mut i = after;
            while i < lower.len() && lower.as_bytes()[i] == b' ' {
                i += 1;
            }
            i < lower.len() && matches!(lower.as_bytes()[i], b'=' | b':')
        };
        if before_ok && after_ok {
            return Some(pos);
        }
        search_start = pos + key.len();
    }
    None
}

fn split_value_side(rest: &str, key_len: usize) -> Option<(&str, &str)> {
    // `rest` starts at the diff-marker-stripped line content. Find the
    // first `=` or `:` *after* the key, allowing optional spaces.
    let bytes = rest.as_bytes();
    // The key itself can sit anywhere; locate it again by looking for
    // the first separator after position 0. This is a robust heuristic:
    // for canonical key=value diff lines the separator is the first
    // `=` or `:` past the key.
    let _ = key_len;
    let mut sep_idx = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' || b == b':' {
            sep_idx = Some(i + 1);
            break;
        }
    }
    let sep = sep_idx?;
    Some(rest.split_at(sep))
}

fn redact_url_credentials(text: &str, modified: &mut bool) -> String {
    let out = redact_url_creds_cursor(text);
    if out != text {
        *modified = true;
    }
    out
}

fn redact_url_creds_cursor(text: &str) -> String {
    const SCHEMES: &[&str] = &[
        "http://",
        "https://",
        "ftp://",
        "postgres://",
        "postgresql://",
        "mysql://",
        "redis://",
        "amqp://",
        "mongodb://",
        "mongodb+srv://",
    ];
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while cursor < text.len() {
        let mut earliest: Option<(usize, &&str)> = None;
        for sch in SCHEMES {
            if let Some(rel) = lower[cursor..].find(sch) {
                let abs = cursor + rel;
                if earliest.is_none_or(|(prev, _)| abs < prev) {
                    earliest = Some((abs, sch));
                }
            }
        }
        let Some((scheme_idx, sch)) = earliest else {
            out.push_str(&text[cursor..]);
            break;
        };
        out.push_str(&text[cursor..scheme_idx]);
        let scheme_end = scheme_idx + sch.len();
        out.push_str(&text[scheme_idx..scheme_end]);
        let tail = &text[scheme_end..];
        let stop = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '>' | '<' | ')'))
            .unwrap_or(tail.len());
        let url_body = &tail[..stop];
        if let Some(at_pos) = url_body.find('@') {
            if url_body[..at_pos].contains(':') {
                out.push_str(REDACTED);
                out.push('@');
                out.push_str(&url_body[at_pos + 1..]);
                cursor = scheme_end + stop;
                continue;
            }
        }
        out.push_str(url_body);
        cursor = scheme_end + stop;
    }
    out
}

fn redact_vendor_tokens(text: &str, modified: &mut bool) -> String {
    // Vendor token prefixes. The match is intentionally token-bounded
    // (whitespace / quote / common punctuation) so prose mentions of
    // a prefix do not over-redact.
    const PREFIXES: &[&str] = &[
        "ghp_",
        "github_pat_",
        "sk-proj-",
        "sk-ant-",
        "sk_live_",
        "sk-",
        "xoxa-",
        "xoxb-",
        "xoxp-",
        "xoxs-",
        "xoxr-",
        "AIza",
    ];
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut current = String::new();
    while let Some(c) = chars.next() {
        if is_token_separator(c) {
            redact_token_into(&current, &mut out, PREFIXES, modified);
            current.clear();
            out.push(c);
        } else {
            current.push(c);
        }
        if chars.peek().is_none() {
            redact_token_into(&current, &mut out, PREFIXES, modified);
            current.clear();
        }
    }
    out
}

fn is_token_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | ',' | ';' | '(' | ')' | '<' | '>' | '{' | '}' | '`'
        )
}

fn redact_token_into(token: &str, out: &mut String, prefixes: &[&str], modified: &mut bool) {
    for p in prefixes {
        if token.starts_with(p) && token.len() >= p.len() + 8 {
            out.push_str(REDACTED);
            *modified = true;
            return;
        }
    }
    out.push_str(token);
}

fn redact_jwt_like(text: &str, modified: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut buf = String::new();
    for ch in text.chars() {
        if is_token_separator(ch) {
            replace_jwt_token(&buf, &mut out, modified);
            buf.clear();
            out.push(ch);
        } else {
            buf.push(ch);
        }
    }
    replace_jwt_token(&buf, &mut out, modified);
    out
}

fn replace_jwt_token(buf: &str, out: &mut String, modified: &mut bool) {
    if buf.starts_with("eyJ") && buf.matches('.').count() == 2 && buf.len() >= 32 {
        let parts: Vec<&str> = buf.split('.').collect();
        if parts.iter().all(|p| !p.is_empty()) {
            out.push_str(REDACTED);
            *modified = true;
            return;
        }
    }
    out.push_str(buf);
}

fn redact_aws_keyids(text: &str, modified: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut buf = String::new();
    for ch in text.chars() {
        if is_token_separator(ch) {
            replace_aws_keyid(&buf, &mut out, modified);
            buf.clear();
            out.push(ch);
        } else {
            buf.push(ch);
        }
    }
    replace_aws_keyid(&buf, &mut out, modified);
    out
}

fn replace_aws_keyid(buf: &str, out: &mut String, modified: &mut bool) {
    if (buf.starts_with("AKIA") || buf.starts_with("ASIA"))
        && buf.len() == 20
        && buf
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        out.push_str(REDACTED);
        *modified = true;
        return;
    }
    out.push_str(buf);
}

fn redact_query_signatures(text: &str, modified: &mut bool) -> String {
    const KEYS: &[&str] = &[
        "x-amz-signature",
        "x-goog-signature",
        "signature",
        "sig",
        "x-amz-security-token",
    ];
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(next) = find_query_signature(bytes, &lower, KEYS, i) else {
            i += 1;
            continue;
        };
        out.push_str(&text[last..next.value_start]);
        out.push_str(REDACTED);
        last = next.value_end;
        *modified = true;
        i = next.value_end;
    }
    out.push_str(&text[last..]);
    out
}

struct QuerySigSpan {
    value_start: usize,
    value_end: usize,
}

fn find_query_signature(
    bytes: &[u8],
    lower: &str,
    keys: &[&str],
    start: usize,
) -> Option<QuerySigSpan> {
    if bytes[start] != b'?' && bytes[start] != b'&' {
        return None;
    }
    let rest = &lower[start + 1..];
    for key in keys {
        if !rest.starts_with(key) {
            continue;
        }
        let after_key = start + 1 + key.len();
        if after_key >= bytes.len() || bytes[after_key] != b'=' {
            continue;
        }
        let value_start = after_key + 1;
        let mut end = value_start;
        while end < bytes.len() && !is_query_value_terminator(bytes[end]) {
            end += 1;
        }
        if end > value_start {
            return Some(QuerySigSpan {
                value_start,
                value_end: end,
            });
        }
    }
    None
}

fn is_query_value_terminator(b: u8) -> bool {
    matches!(b, b'&' | b'"' | b'\'' | b')' | b'>') || b.is_ascii_whitespace()
}

/// Vendor token prefixes the post-scan checks for residual entropy past.
const POST_SCAN_VENDOR_PREFIXES: &[&str] = &[
    "ghp_",
    "github_pat_",
    "sk-proj-",
    "sk-ant-",
    "sk_live_",
    "xoxa-",
    "xoxb-",
    "xoxp-",
    "xoxs-",
];

/// URL schemes the post-scan inspects for embedded credentials.
const POST_SCAN_URL_SCHEMES: &[&str] = &[
    "http://",
    "https://",
    "ftp://",
    "postgres://",
    "postgresql://",
    "mysql://",
    "redis://",
    "amqp://",
    "mongodb://",
    "mongodb+srv://",
];

/// Final-output post-scan. Returns `true` if any residual secret-shaped
/// material remains in `text` after the primary redaction pass.
/// Fail-closed: when this returns `true`, the caller collapses the
/// preview to a metadata summary.
#[must_use]
pub fn contains_secret_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // PEM block remnants.
    if lower.contains("-----begin ") && lower.contains("private key") {
        return true;
    }
    for p in POST_SCAN_VENDOR_PREFIXES {
        if let Some(pos) = lower.find(p) {
            let after = pos + p.len();
            // Trailing entropy: at least 8 chars before a separator.
            let rest = &text[after..];
            let n = rest.chars().take_while(|c| !is_token_separator(*c)).count();
            if n >= 8 {
                return true;
            }
        }
    }
    // JWT-shaped triplets.
    for tok in text.split(|c: char| is_token_separator(c)) {
        if tok.starts_with("eyJ") && tok.matches('.').count() == 2 && tok.len() >= 32 {
            let parts: Vec<&str> = tok.split('.').collect();
            if parts.iter().all(|p| !p.is_empty()) {
                return true;
            }
        }
    }
    // URL credentials.
    for scheme in POST_SCAN_URL_SCHEMES {
        let mut cursor = 0usize;
        while let Some(rel) = lower[cursor..].find(scheme) {
            let abs = cursor + rel + scheme.len();
            let tail = &text[abs..];
            let stop = tail
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '>' | '<' | ')'))
                .unwrap_or(tail.len());
            let url_body = &tail[..stop];
            if let Some(at) = url_body.find('@') {
                if url_body[..at].contains(':') {
                    return true;
                }
            }
            cursor = abs + stop;
        }
    }
    // AKIA/ASIA shapes that survived tokenization.
    for tok in text.split(|c: char| is_token_separator(c)) {
        if (tok.starts_with("AKIA") || tok.starts_with("ASIA"))
            && tok.len() == 20
            && tok
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

// =========================================================================
// Internal: helpers
// =========================================================================

#[must_use]
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

fn hex_lower(digest: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(digest.len() * 2);
    for &b in digest {
        s.push(TABLE[(b >> 4) as usize] as char);
        s.push(TABLE[(b & 0x0f) as usize] as char);
    }
    s
}

/// Validate a step-id with the same shape as the checkpoint store
/// (`^[A-Za-z0-9_.-]{1,128}$`, with bare `.` / `..` refused).
#[must_use]
pub fn is_valid_step_id(s: &str) -> bool {
    if s.is_empty() || s.len() > 128 || s == "." || s == ".." {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

#[allow(dead_code)]
fn debug_os_str(s: &OsStr) -> String {
    s.to_string_lossy().into_owned()
}

// =========================================================================
// Unit tests (the bulk of behavior is exercised by the integration
// suite at tests/l2b_diff_approval.rs).
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ulid_zero() -> Ulid {
        Ulid::from_parts(0, 0)
    }

    fn mk_inputs<'a>(
        run_id: &'a Ulid,
        before: Option<&'a [u8]>,
        after: &'a [u8],
    ) -> PreviewInputs<'a> {
        PreviewInputs {
            step_id: "step-1",
            target_relative_path: Path::new("src/lib.rs"),
            target_absolute_path: Path::new("/tmp/ws/src/lib.rs"),
            before,
            after,
            checkpoint_run_id: run_id,
            checkpoint_step_id: "step-1",
            created_at_utc: "2026-05-21T00:00:00.000000000Z",
        }
    }

    #[test]
    fn build_preview_for_safe_text_is_approvable() {
        let run_id = ulid_zero();
        let inp = mk_inputs(&run_id, Some(b"alpha\nbeta\n"), b"alpha\nbeta\ngamma\n");
        let (rec, disp) = build_preview(&inp).unwrap();
        assert!(rec.is_approvable());
        assert!(!rec.is_binary);
        assert!(!rec.is_redacted);
        assert!(!rec.is_truncated);
        assert!(disp.rendered.contains("--- a/src/lib.rs"));
        assert!(disp.rendered.contains("+++ b/src/lib.rs"));
        assert!(disp.rendered.contains("+gamma"));
    }

    #[test]
    fn binary_blob_marks_record_binary_and_non_approvable() {
        let run_id = ulid_zero();
        let inp = mk_inputs(&run_id, Some(&[0, 1, 2, 0, 4]), b"after");
        let (rec, _disp) = build_preview(&inp).unwrap();
        assert!(rec.is_binary);
        assert!(!rec.is_approvable());
    }

    #[test]
    fn invalid_step_id_refused() {
        let run_id = ulid_zero();
        let mut inp = mk_inputs(&run_id, None, b"");
        inp.step_id = "";
        assert_eq!(build_preview(&inp), Err(PreviewBuildError::InvalidStepId));
    }

    #[test]
    fn canonical_includes_format_version() {
        let s = canonical_preview_record_for_approval(&CanonicalSubset {
            preview_id: "id",
            step_id: "s",
            target_relative_path_sanitized: "p",
            before_sha256: "a",
            after_sha256: "b",
            checkpoint_run_id: "r",
            checkpoint_step_id: "cs",
            is_binary: false,
            is_redacted: false,
            is_truncated: false,
            preview_format_version: PREVIEW_FORMAT_VERSION,
        });
        assert!(s.contains(CANONICAL_HEADER));
        assert!(s.contains(&format!("preview_format_version={PREVIEW_FORMAT_VERSION}")));
    }

    #[test]
    fn step_id_shape_rejects_dot_and_dotdot() {
        assert!(!is_valid_step_id("."));
        assert!(!is_valid_step_id(".."));
        assert!(!is_valid_step_id(""));
        assert!(is_valid_step_id("step-1.foo_bar"));
        let long: String = "a".repeat(129);
        assert!(!is_valid_step_id(&long));
    }

    #[test]
    fn sanitize_path_strips_ansi_and_newlines() {
        let p = PathBuf::from("ok/\u{1b}[31mred\u{1b}[0m\nname");
        let out = sanitize_path_display(&p);
        assert!(!out.contains('\u{1b}'));
        assert!(!out.contains('\n'));
    }
}
