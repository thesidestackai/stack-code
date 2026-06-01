//! Harness reporting output.
//!
//! The report carries every field of the parsed envelope at full
//! fidelity, the raw stdout capture (for byte-identical idempotency
//! verification), the exit code, the per-assertion summary, the
//! disposable-workspace classifier decision, and any STOP signals
//! the harness raised. Nothing is summarised, redacted, or
//! rate-limited.

use serde::Serialize;

use crate::classifier::WorkspaceClassification;
use crate::envelope::StatusEnvelope;
use crate::stop::StopSignal;

/// Overall pass/fail classification of one cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleClassification {
    Pass,
    Fail,
    Stop,
}

/// One assertion's expected/observed pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionEntry {
    pub name: String,
    pub expected: String,
    pub observed: String,
    pub passed: bool,
}

/// One status subprocess invocation's record.
#[derive(Debug, Clone)]
pub struct InvocationRecord {
    pub argv: Vec<String>,
    pub stdout_raw: Vec<u8>,
    pub exit_code: i32,
    pub envelope_parsed: Option<StatusEnvelope>,
}

impl InvocationRecord {
    #[must_use]
    pub fn stdout_lossy_utf8(&self) -> String {
        String::from_utf8_lossy(&self.stdout_raw).into_owned()
    }
}

/// Full harness report. Implements `Debug` for tests; production
/// callers serialize the structured fields directly.
#[derive(Debug, Clone)]
pub struct HarnessRunReport {
    pub classification: CycleClassification,
    pub diagnostic: String,
    pub classifier_decision: WorkspaceClassification,
    pub invocations: Vec<InvocationRecord>,
    pub assertions: Vec<AssertionEntry>,
    pub stop_signals: Vec<StopSignal>,
}

impl HarnessRunReport {
    /// `true` if the report carries any STOP signal in any source.
    #[must_use]
    pub fn has_any_stop(&self) -> bool {
        !self.stop_signals.is_empty() || matches!(self.classification, CycleClassification::Stop)
    }
}
