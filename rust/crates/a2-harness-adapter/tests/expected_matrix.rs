//! Caller-expectation matrix:
//! - expected continue × observed continue → PASS
//! - expected continue × observed STOP     → FAIL classified STOP
//! - expected STOP    × observed continue  → FAIL classified STOP
//! - expected STOP    × observed STOP-same → PASS
//! - expected STOP    × observed STOP-diff → FAIL classified STOP

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, CycleClassification, ExpectedOutcome, HarnessAssertionConfig,
    MockStatusInvoker, Phase, StatusInvocation, StopCondition, StopKind,
};

mod common;
use common::*;

fn ok_continue_envelope_bytes() -> Vec<u8> {
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &["fixtures/ok/.claw/x"],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    envelope_bytes(&env)
}

fn stop_payload_sha_mismatch_bytes() -> Vec<u8> {
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
    envelope_bytes(&env)
}

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

#[test]
fn expected_continue_observed_continue_passes() {
    let mut cfg = base_cfg();
    cfg.expected_outcome = ExpectedOutcome::Continue;
    cfg.expected_phase = Some(Phase::AwaitingApproval);
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: ok_continue_envelope_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg, &classifier_cfg_allows_fixture(), &mock).expect("cycle runs");
    assert_eq!(report.classification, CycleClassification::Pass);
    assert!(report.stop_signals.is_empty(), "no STOP signals expected");
}

#[test]
fn expected_continue_observed_stop_fails_classified_stop() {
    let mut cfg = base_cfg();
    cfg.expected_outcome = ExpectedOutcome::Continue;
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: stop_payload_sha_mismatch_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg, &classifier_cfg_allows_fixture(), &mock).expect("cycle runs");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ExpectedContinueObservedStop)),
        "must surface ExpectedContinueObservedStop"
    );
}

#[test]
fn expected_stop_observed_continue_fails_classified_stop() {
    let mut cfg = base_cfg();
    cfg.expected_outcome = ExpectedOutcome::Stop {
        stop_condition: Some(StopCondition::PayloadShaMismatch),
    };
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: ok_continue_envelope_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg, &classifier_cfg_allows_fixture(), &mock).expect("cycle runs");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ExpectedStopObservedContinue)),
        "must surface ExpectedStopObservedContinue"
    );
}

#[test]
fn expected_stop_observed_matching_stop_passes() {
    let mut cfg = base_cfg();
    cfg.expected_outcome = ExpectedOutcome::Stop {
        stop_condition: Some(StopCondition::PayloadShaMismatch),
    };
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: stop_payload_sha_mismatch_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg, &classifier_cfg_allows_fixture(), &mock).expect("cycle runs");
    // Matching expected STOP: assertion passes but cycle is still
    // classified STOP because producer-emitted STOP is itself a STOP
    // signal the report surfaces.
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .assertions
            .iter()
            .any(|a| a.name == "outcome.expected_stop.stop_condition" && a.passed),
        "stop_condition match assertion must pass"
    );
}

#[test]
fn expected_stop_observed_different_stop_fails_classified_stop() {
    let mut cfg = base_cfg();
    cfg.expected_outcome = ExpectedOutcome::Stop {
        stop_condition: Some(StopCondition::ApprovalShaMismatch),
    };
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: stop_payload_sha_mismatch_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg, &classifier_cfg_allows_fixture(), &mock).expect("cycle runs");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::WrongStopValue { .. })),
        "must surface WrongStopValue"
    );
}
