//! PR #43 preservation coverage.
//!
//! These four explicit-named tests preserve the PR #43-only assertions
//! that were not carried into the merged PR #44 scope card, the PR #45
//! implementation scope card, or the PR #46 harness adapter crate.
//! Each test name matches the matrix vocabulary requested in PR #43 §17
//! so the coverage is greppable.
//!
//! 1. `non_null_stop_condition_with_empty_evidence_paths_is_stop` —
//!    producer-broken signal: an A2-L2d producer always populates at
//!    least one evidence path when a STOP fires; an empty list under a
//!    non-null `stop_condition` is itself a STOP in its own right.
//! 2. `exit_12_with_refused_marker_is_accepted_as_refusal` and
//!    `exit_12_without_refused_marker_is_stop` — `EXIT_STATUS_REFUSED`
//!    envelopes MUST carry the pinned `a2-l2d-status-refused` audit
//!    marker; absence is producer-broken drift.
//! 3. `unparseable_stdout_is_stop` — raw stdout that fails to JSON-parse
//!    surfaces as a STOP and the raw bytes are preserved in the
//!    `InvocationRecord` for the operator escalation report.
//! 4. `missing_read_only_invariant_fixture_is_stop` and
//!    `substituted_read_only_invariant_fixture_is_stop` — explicit-
//!    named synthetic fixtures asserting both drift modes of the
//!    `read_only_invariant` literal surface as STOPs.

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, CycleClassification, HarnessAssertionConfig, MockStatusInvoker,
    StatusInvocation, StopCondition, StopKind, EXIT_STATUS_REFUSED, READ_ONLY_INVARIANT_LITERAL,
    REFUSED_AUDIT_MARKER,
};

mod common;
use common::*;

fn classifier_cfg_allows_fixture() -> ClassifierConfig {
    let ws = disposable_workspace_ok_path();
    let uid = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let m = std::fs::metadata(&ws).expect("readable fixture");
            Some(m.uid())
        }
        #[cfg(not(unix))]
        {
            None
        }
    };
    ClassifierConfig {
        disposable_path_prefixes: vec![ws],
        expected_owner_uid: uid,
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    }
}

fn base_cfg() -> HarnessAssertionConfig {
    HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    }
}

// -------------------------------------------------------------------
// Item 1: empty evidence_paths under non-null stop_condition is STOP
// -------------------------------------------------------------------

#[test]
fn non_null_stop_condition_with_empty_evidence_paths_is_stop() {
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "preview_ready",
        NEXT_OP_STOP_ESCALATE,
        true,
        false,
        Some("payload-sha-mismatch"),
        // Intentionally empty: producer-broken drift fixture.
        &[],
        &[
            "a2-l2d-status-read",
            "a2-l2d-status-stop-condition-detected",
        ],
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: envelope_bytes(&env),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report.stop_signals.iter().any(|s| matches!(
            &s.kind,
            StopKind::EvidencePathsEmptyUnderStopCondition(StopCondition::PayloadShaMismatch)
        )),
        "must surface EvidencePathsEmptyUnderStopCondition with the offending stop_condition verbatim"
    );
    // The producer's stop_condition STOP must still be surfaced
    // alongside; the harness does not collapse the two into one.
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ProducerStopCondition(_))),
        "ProducerStopCondition must also be surfaced (no STOP collapse)"
    );
}

#[test]
fn non_null_stop_condition_with_non_empty_evidence_paths_does_not_raise_empty_signal() {
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "preview_ready",
        NEXT_OP_STOP_ESCALATE,
        true,
        false,
        Some("payload-sha-mismatch"),
        &["fixtures/ok/.claw/payload"],
        &[
            "a2-l2d-status-read",
            "a2-l2d-status-stop-condition-detected",
        ],
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: envelope_bytes(&env),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert!(
        !report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::EvidencePathsEmptyUnderStopCondition(_))),
        "no producer-broken signal when evidence_paths is populated"
    );
}

// -------------------------------------------------------------------
// Item 2: exit 12 envelopes MUST carry a2-l2d-status-refused marker
// -------------------------------------------------------------------

fn refusal_envelope_with_markers(markers: &[&str]) -> Vec<u8> {
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "preview_ready",
        NEXT_OP_STOP_ESCALATE,
        false,
        false,
        Some("workspace-root-invalid"),
        &["fixtures/ok/.claw/refusal"],
        markers,
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    envelope_bytes(&env)
}

#[test]
fn exit_12_with_refused_marker_is_accepted_as_refusal() {
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: refusal_envelope_with_markers(&[
            "a2-l2d-status-read",
            REFUSED_AUDIT_MARKER,
            "a2-l2d-status-stop-condition-detected",
        ]),
        stderr: Vec::new(),
        exit_code: EXIT_STATUS_REFUSED,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ProducerRefused)),
        "exit-12 envelope must surface ProducerRefused"
    );
    assert!(
        !report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ExitRefusedMissingMarker { .. })),
        "refusal envelope carrying the pinned marker must NOT raise the missing-marker STOP"
    );
}

#[test]
fn exit_12_without_refused_marker_is_stop() {
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: refusal_envelope_with_markers(&[
            "a2-l2d-status-read",
            "a2-l2d-status-stop-condition-detected",
        ]),
        stderr: Vec::new(),
        exit_code: EXIT_STATUS_REFUSED,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ProducerRefused)),
        "the underlying ProducerRefused STOP is still surfaced"
    );
    let missing = report
        .stop_signals
        .iter()
        .find_map(|s| match &s.kind {
            StopKind::ExitRefusedMissingMarker { observed_markers } => Some(observed_markers),
            _ => None,
        })
        .expect("must surface ExitRefusedMissingMarker");
    assert!(
        !missing.iter().any(|m| m == REFUSED_AUDIT_MARKER),
        "observed markers must NOT contain the pinned refusal marker"
    );
    // Verbatim preservation: every observed marker is reported.
    assert!(missing.iter().any(|m| m == "a2-l2d-status-read"));
    assert!(missing
        .iter()
        .any(|m| m == "a2-l2d-status-stop-condition-detected"));
}

// -------------------------------------------------------------------
// Item 3: unparseable stdout is STOP and raw bytes are preserved
// -------------------------------------------------------------------

#[test]
fn unparseable_stdout_is_stop() {
    let raw = b"this is not json {".to_vec();
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: raw.clone(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::InvalidJson(_))),
        "must surface InvalidJson STOP"
    );
    // Raw stdout preserved verbatim on the invocation record so the
    // operator escalation report carries full fidelity.
    assert_eq!(report.invocations.len(), 1);
    assert_eq!(report.invocations[0].stdout_raw, raw);
    assert!(
        report.invocations[0].envelope_parsed.is_none(),
        "parse failed, so no parsed envelope is attached"
    );
}

// -------------------------------------------------------------------
// Item 4: missing / substituted read_only_invariant fixtures
// -------------------------------------------------------------------

#[test]
fn missing_read_only_invariant_fixture_is_stop() {
    let mut env = build_envelope(
        &disposable_workspace_ok_path(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT_LITERAL,
        STATUS_SCHEMA_V1,
    );
    env.as_object_mut()
        .expect("envelope object")
        .remove("read_only_invariant");
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: envelope_bytes(&env),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    // The parser surfaces missing-field drift as SchemaDrift since the
    // serde shape rejects the absent required field.
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::SchemaDrift(_))),
        "missing read_only_invariant must surface as schema drift STOP"
    );
}

#[test]
fn substituted_read_only_invariant_fixture_is_stop() {
    let substituted_literal = "this command does not mutate state, probably";
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        substituted_literal,
        STATUS_SCHEMA_V1,
    );
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: envelope_bytes(&env),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&base_cfg(), &classifier_cfg_allows_fixture(), &mock)
        .expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    let observed = report
        .stop_signals
        .iter()
        .find_map(|s| match &s.kind {
            StopKind::ReadOnlyInvariantAltered(v) => Some(v.clone()),
            _ => None,
        })
        .expect("must surface ReadOnlyInvariantAltered STOP");
    assert_eq!(
        observed, substituted_literal,
        "substituted literal must be preserved verbatim, not coerced"
    );
}
