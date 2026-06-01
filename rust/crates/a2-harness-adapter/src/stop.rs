//! STOP signal classification.
//!
//! The harness emits a STOP signal whenever the producer surfaces one
//! through the envelope (non-null `stop_condition`, STOP-bearing
//! phase, `STOP — escalate` next-operator-command) and whenever the
//! harness itself detects drift or a violated invariant (unknown
//! enum value, altered `read_only_invariant`, idempotency mismatch,
//! refused config, classifier refusal).
//!
//! STOP signals are surfaced verbatim — the harness never debounces,
//! summarises, or downgrades a STOP into a warning.

use crate::envelope::{Phase, StopCondition};

/// Origin and detail of a STOP signal. Each variant carries the
/// producer-emitted or harness-detected literal verbatim so the
/// operator escalation report carries full fidelity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopKind {
    /// Producer emitted a non-null `stop_condition`.
    ProducerStopCondition(StopCondition),
    /// Producer emitted a STOP-bearing phase
    /// (`non_approvable`, `rolled_back`, or `unknown`).
    StopBearingPhase(Phase),
    /// Producer emitted `next_operator_command: "STOP — escalate"`.
    ProducerStopEscalate,
    /// Producer emitted an unknown enum literal in one of the four
    /// closed enums (`phase`, `stop_condition`, `next_operator_command`,
    /// marker). Carries the observed literal verbatim.
    UnknownEnumLiteral { field: &'static str, value: String },
    /// Producer emitted an envelope whose `schema_version` did not
    /// match the pinned literal. Carries the observed value.
    SchemaVersionMismatch(String),
    /// `read_only_invariant` literal absent or altered.
    ReadOnlyInvariantAltered(String),
    /// Stdout did not parse as JSON.
    InvalidJson(String),
    /// JSON parsed but structure did not match
    /// `a2-l2d-status.v1`.
    SchemaDrift(String),
    /// Two paired status invocations produced non-byte-identical
    /// stdout. The harness MUST surface both captures at full
    /// fidelity; this variant carries a short summary, with the raw
    /// captures emitted in the report alongside.
    IdempotencyMismatch,
    /// Producer subprocess exited with `EXIT_STATUS_REFUSED == 12`.
    ProducerRefused,
    /// Producer subprocess exited with `EXIT_STATUS_REFUSED == 12`
    /// but the envelope's `audit_markers` did not include the pinned
    /// `a2-l2d-status-refused` literal. The producer is authoritative
    /// on the refusal contract; absence of the marker is producer-
    /// broken drift and the harness surfaces the observed marker list
    /// verbatim.
    ExitRefusedMissingMarker { observed_markers: Vec<String> },
    /// Producer emitted a non-null `stop_condition` but `evidence_paths`
    /// was empty. The A2-L2d producer always populates at least one
    /// evidence path when a STOP fires; an empty list under a non-null
    /// STOP is a producer-broken signal the harness raises in its own
    /// right. Carries the offending `stop_condition` verbatim.
    EvidencePathsEmptyUnderStopCondition(StopCondition),
    /// Caller configuration referenced a chain-write subcommand. The
    /// harness refuses such configs at parse time, not at invocation
    /// time.
    ConfigReferencedChainWriteCommand(String),
    /// Disposable-workspace classifier refused the configured
    /// workspace.
    NonDisposableWorkspaceRefused(String),
    /// Caller expected to continue but observed a STOP.
    ExpectedContinueObservedStop,
    /// Caller expected a STOP but observed continuation.
    ExpectedStopObservedContinue,
    /// Caller expected a specific STOP but observed a different STOP.
    WrongStopValue { expected: String, observed: String },
}

/// A single STOP signal emitted by the harness. The harness's report
/// lists every STOP it observed in a single cycle; each is rendered
/// verbatim in the order it was raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSignal {
    pub kind: StopKind,
}

impl StopSignal {
    #[must_use]
    pub const fn new(kind: StopKind) -> Self {
        Self { kind }
    }

    /// Short verbatim summary suitable for diagnostic output. The
    /// underlying enum value carries the full literal; this is for
    /// quick scanning only.
    #[must_use]
    pub fn summary(&self) -> String {
        match &self.kind {
            StopKind::ProducerStopCondition(_) => "producer stop_condition".into(),
            StopKind::StopBearingPhase(_) => "producer stop-bearing phase".into(),
            StopKind::ProducerStopEscalate => "producer STOP — escalate".into(),
            StopKind::UnknownEnumLiteral { field, .. } => {
                format!("unknown enum literal in `{field}`")
            }
            StopKind::SchemaVersionMismatch(_) => "schema_version literal mismatch".into(),
            StopKind::ReadOnlyInvariantAltered(_) => "read_only_invariant absent or altered".into(),
            StopKind::InvalidJson(_) => "invalid JSON".into(),
            StopKind::SchemaDrift(_) => "schema drift".into(),
            StopKind::IdempotencyMismatch => "idempotency mismatch".into(),
            StopKind::ProducerRefused => "producer refusal envelope".into(),
            StopKind::ExitRefusedMissingMarker { .. } => {
                "exit 12 envelope missing `a2-l2d-status-refused` marker".into()
            }
            StopKind::EvidencePathsEmptyUnderStopCondition(_) => {
                "non-null stop_condition with empty evidence_paths".into()
            }
            StopKind::ConfigReferencedChainWriteCommand(_) => {
                "config referenced chain-write subcommand".into()
            }
            StopKind::NonDisposableWorkspaceRefused(_) => "non-disposable workspace refused".into(),
            StopKind::ExpectedContinueObservedStop => "expected continue / observed STOP".into(),
            StopKind::ExpectedStopObservedContinue => "expected STOP / observed continue".into(),
            StopKind::WrongStopValue { .. } => "wrong STOP value".into(),
        }
    }
}

/// Whether a phase by itself is a STOP-bearing phase. The closed enum
/// `phase` includes three STOP-bearing values per the schema-of-record.
#[must_use]
pub const fn phase_is_stop_bearing(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::NonApprovable | Phase::RolledBack | Phase::Unknown
    )
}
