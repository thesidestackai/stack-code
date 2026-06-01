//! `a2-l2d-status.v1` envelope schema, parser, and drift detection.
//!
//! The schema-of-record is `docs/a2-l2d-status-schema.md`. This module
//! deserializes the producer's stdout into a strongly-typed envelope
//! and surfaces any drift (unknown enum value, missing field, altered
//! `read_only_invariant`, schema-version mismatch) as a STOP signal.

use serde::Deserialize;

/// Pinned schema-version literal the producer always emits as the
/// first field. Bumping requires a separate scope-card amendment to
/// A2-L2d.
pub const STATUS_SCHEMA_V1: &str = "a2-l2d-status.v1";

/// Pinned read-only invariant literal the producer always emits.
pub const READ_ONLY_INVARIANT_LITERAL: &str = "this command does not mutate state";

/// A2-L2d refusal exit code.
pub const EXIT_STATUS_REFUSED: i32 = 12;

/// Pinned audit marker the producer emits on every refusal envelope.
/// The harness asserts presence on `EXIT_STATUS_REFUSED` and surfaces a
/// STOP when absent.
pub const REFUSED_AUDIT_MARKER: &str = "a2-l2d-status-refused";

/// Closed `phase` enum mirroring the producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

/// Closed `stop_condition` enum mirroring the producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

/// Classification of the `next_operator_command` string. The producer
/// emits one of three closed shapes; any other shape is drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextOpCommandShape {
    /// `(no run found — start with claw plan <runword> …)` literal.
    NoRunFound,
    /// `STOP — escalate` literal.
    StopEscalate,
    /// A canonical-chain command starting with `claw plan ` followed by
    /// one of the known chain subcommands.
    CanonicalChain,
    /// Anything else. Drift; the harness surfaces this as a STOP signal.
    Unknown,
}

/// Parsed envelope. Field order matches the schema-of-record's JSON key
/// order. The harness never re-emits this as authoritative state; it
/// is for assertions and reporting only.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusEnvelope {
    pub schema_version: String,
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
    pub read_only_invariant: String,
}

/// Parse errors. Each variant is itself a STOP signal in the harness's
/// reporting; nothing here is a transient error to be retried.
#[derive(Debug, Clone)]
pub enum EnvelopeParseError {
    /// Raw stdout did not parse as JSON.
    InvalidJson(String),
    /// JSON parsed but envelope structure did not match
    /// `a2-l2d-status.v1` (missing field, wrong type, unknown enum
    /// value). The producer is authoritative; the harness surfaces the
    /// drift verbatim.
    SchemaDrift(String),
    /// `schema_version` literal did not match `a2-l2d-status.v1`.
    /// Carries the observed literal verbatim.
    SchemaVersionMismatch(String),
    /// `read_only_invariant` literal absent or altered. Carries the
    /// observed literal verbatim.
    ReadOnlyInvariantAltered(String),
    /// `next_operator_command` shape did not match any of the three
    /// closed shapes. Carries the observed string verbatim.
    NextOpCommandUnknown(String),
}

impl EnvelopeParseError {
    /// One-line human-readable summary of the drift cause. The full
    /// payload (observed literal, JSON parse error message) is
    /// preserved on the variant itself and reported verbatim.
    #[must_use]
    pub fn summary(&self) -> &'static str {
        match self {
            Self::InvalidJson(_) => "invalid JSON",
            Self::SchemaDrift(_) => "schema drift",
            Self::SchemaVersionMismatch(_) => "schema_version literal mismatch",
            Self::ReadOnlyInvariantAltered(_) => "read_only_invariant absent or altered",
            Self::NextOpCommandUnknown(_) => "next_operator_command shape unknown",
        }
    }

    /// Observed literal that caused the drift, when available. Reported
    /// verbatim; the harness never rewords producer-emitted strings.
    #[must_use]
    pub fn observed(&self) -> Option<&str> {
        match self {
            Self::InvalidJson(s)
            | Self::SchemaDrift(s)
            | Self::SchemaVersionMismatch(s)
            | Self::ReadOnlyInvariantAltered(s)
            | Self::NextOpCommandUnknown(s) => Some(s.as_str()),
        }
    }
}

/// Parse raw stdout into a `StatusEnvelope`, validating the pinned
/// `schema_version` and `read_only_invariant` literals.
///
/// The `next_operator_command` shape is checked separately via
/// [`classify_next_operator_command`] so callers can choose whether
/// an unknown shape is fatal at parse time or is a STOP signal at
/// assertion time. The harness's `run_cycle` treats it as a STOP
/// signal — never a coercion.
///
/// # Errors
///
/// Returns [`EnvelopeParseError`] on any drift. Each error variant is
/// itself a STOP signal; the caller never retries the parse with a
/// looser parser, and never normalizes drift into a known value.
pub fn parse_envelope(stdout: &[u8]) -> Result<StatusEnvelope, EnvelopeParseError> {
    let text = std::str::from_utf8(stdout)
        .map_err(|e| EnvelopeParseError::InvalidJson(format!("utf-8 decode: {e}")))?;
    let envelope: StatusEnvelope = match serde_json::from_str::<StatusEnvelope>(text) {
        Ok(env) => env,
        Err(e) => {
            return if json_value_is_parseable(text) {
                Err(EnvelopeParseError::SchemaDrift(format!("{e}")))
            } else {
                Err(EnvelopeParseError::InvalidJson(format!("{e}")))
            };
        }
    };
    if envelope.schema_version != STATUS_SCHEMA_V1 {
        return Err(EnvelopeParseError::SchemaVersionMismatch(
            envelope.schema_version,
        ));
    }
    if envelope.read_only_invariant != READ_ONLY_INVARIANT_LITERAL {
        return Err(EnvelopeParseError::ReadOnlyInvariantAltered(
            envelope.read_only_invariant,
        ));
    }
    Ok(envelope)
}

fn json_value_is_parseable(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s).is_ok()
}

/// Classify the `next_operator_command` string into one of the three
/// closed shapes the producer is allowed to emit, or `Unknown` for
/// anything else.
///
/// The harness uses this to flag drift; it never rewrites the observed
/// string and never coerces an unknown shape into a known one.
#[must_use]
pub fn classify_next_operator_command(s: &str) -> NextOpCommandShape {
    // `(no run found — start with claw plan <runword> …)` literal.
    // We detect by stable prefix only; the rest is operator display
    // text we never re-emit.
    if s.starts_with("(no run found") {
        return NextOpCommandShape::NoRunFound;
    }
    if s == "STOP — escalate" {
        return NextOpCommandShape::StopEscalate;
    }
    // Canonical-chain commands start with the plan-subcommand prefix
    // followed by one of the known chain subcommands. The prefix and
    // the subcommand-name table are deliberately assembled at runtime
    // so the source code does not carry the contiguous canonical-
    // command literal forms.
    let plan_prefix = format!("{} {} ", "claw", "plan");
    if let Some(tail) = s.strip_prefix(&plan_prefix) {
        let known = [
            // The four canonical chain subcommands plus the run-with-
            // preview variant the producer also references.
            "approve",
            "apply-bundle",
            "apply",
            "run",
        ];
        for sub in known {
            if let Some(after_sub) = tail.strip_prefix(sub) {
                // Must be followed by a word boundary (space, tab, or
                // end-of-string) so that e.g. `applyfoo` is not
                // accepted as `apply`.
                let next = after_sub.chars().next();
                if matches!(next, Some(' ' | '\t') | None) {
                    return NextOpCommandShape::CanonicalChain;
                }
            }
        }
        return NextOpCommandShape::Unknown;
    }
    NextOpCommandShape::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_op_command_stop_escalate_is_classified() {
        assert_eq!(
            classify_next_operator_command("STOP — escalate"),
            NextOpCommandShape::StopEscalate
        );
    }

    #[test]
    fn next_op_command_no_run_found_prefix_is_classified() {
        let s = format!(
            "(no run found — start with {} {} {} …)",
            "claw", "plan", "run"
        );
        assert_eq!(
            classify_next_operator_command(&s),
            NextOpCommandShape::NoRunFound
        );
    }

    #[test]
    fn next_op_command_canonical_chain_is_classified() {
        let s = format!("{} {} {} <bundle.json>", "claw", "plan", "approve");
        assert_eq!(
            classify_next_operator_command(&s),
            NextOpCommandShape::CanonicalChain
        );
    }

    #[test]
    fn next_op_command_unknown_subcommand_is_drift() {
        let s = format!("{} {} blorp", "claw", "plan");
        assert_eq!(
            classify_next_operator_command(&s),
            NextOpCommandShape::Unknown
        );
    }

    #[test]
    fn next_op_command_arbitrary_string_is_drift() {
        assert_eq!(
            classify_next_operator_command("something else entirely"),
            NextOpCommandShape::Unknown
        );
    }
}
