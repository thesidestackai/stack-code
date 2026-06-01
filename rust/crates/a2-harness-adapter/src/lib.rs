//! A2-L3 Harness Adapter — read-only observer + assertion/reporting
//! layer over the A2-L2d `claw plan status` command and the
//! `a2-l2d-status.v1` envelope schema.
//!
//! Scope:
//!
//! * The adapter MAY invoke only `claw plan status <workspace>
//!   [<approval-result.json>]`.
//! * The adapter MAY parse `a2-l2d-status.v1` envelopes.
//! * The adapter MAY assert on schema, invariants, STOP conditions,
//!   evidence paths, audit markers, and idempotency.
//! * The adapter MAY emit structured pass/fail reports at full
//!   envelope fidelity.
//!
//! Out of scope (forbidden by construction):
//!
//! * Invoking any non-status plan subcommand.
//! * Generating an approval-result or apply-bundle artifact.
//! * Mutating `.claw/` or any workspace file.
//! * Running against a non-disposable workspace without an explicit
//!   per-deployment scope-card reference.
//! * Calling broker, model, Ollama, telemetry, or any other network
//!   endpoint.
//! * Coercing or normalising any unknown envelope value.
//!
//! See
//! [`docs/a2-l3-harness-adapter-scope-card.md`](../../../docs/a2-l3-harness-adapter-scope-card.md)
//! and
//! [`docs/a2-l3-harness-adapter-implementation-scope-card.md`](../../../docs/a2-l3-harness-adapter-implementation-scope-card.md)
//! for the full behavioural and implementation boundary.

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod classifier;
pub mod config;
pub mod cycle;
pub mod envelope;
pub mod invoker;
pub mod report;
pub mod stop;

pub use classifier::{
    classify_workspace, ClassifierConfig, ClassifierSignals, WorkspaceClassification,
    DISPOSABLE_MARKER_REL_PATH,
};
pub use config::{ConfigError, ExpectedOutcome, HarnessAssertionConfig, REPEAT_INVOCATION_CAP};
pub use cycle::{run_cycle, CycleError};
pub use envelope::{
    classify_next_operator_command, parse_envelope, EnvelopeParseError, NextOpCommandShape, Phase,
    StatusEnvelope, StopCondition, EXIT_STATUS_REFUSED, READ_ONLY_INVARIANT_LITERAL,
    REFUSED_AUDIT_MARKER, STATUS_SCHEMA_V1,
};
pub use invoker::{
    build_status_argv, ClawPlanStatusInvoker, MockInvocationRecord, MockStatusInvoker,
    StatusInvocation, StatusInvoker,
};
pub use report::{AssertionEntry, CycleClassification, HarnessRunReport, InvocationRecord};
pub use stop::{phase_is_stop_bearing, StopKind, StopSignal};
