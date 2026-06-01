//! Idempotency tests:
//! - byte-identical paired stdout → PASS
//! - non-byte-identical paired stdout → STOP signal (cycle classified STOP)
//! - independent-subprocess invariant: cycle invokes once per repeat
//! - per-cycle in-memory cache lifetime: invocations are independent

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, CycleClassification, HarnessAssertionConfig, MockStatusInvoker,
    StatusInvocation, StopKind,
};

mod common;
use common::*;

fn ok_continue_bytes() -> Vec<u8> {
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    envelope_bytes(&env)
}

fn ok_continue_bytes_perturbed() -> Vec<u8> {
    // Same envelope shape but with a one-byte difference (added
    // single-space evidence path) so the byte-identical comparison
    // fails.
    let env = build_envelope(
        &disposable_workspace_ok_path(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &["x"],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT,
        STATUS_SCHEMA_V1,
    );
    envelope_bytes(&env)
}

fn classifier_cfg() -> ClassifierConfig {
    let ws = disposable_workspace_ok_path();
    let uid = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(std::fs::metadata(&ws).expect("meta").uid())
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

fn cfg(repeats: u8) -> HarnessAssertionConfig {
    HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        repeat_invocations: repeats,
        ..Default::default()
    }
}

#[test]
fn byte_identical_paired_stdout_passes_idempotency() {
    let bytes = ok_continue_bytes();
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: bytes.clone(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    mock.push_canned(StatusInvocation {
        stdout: bytes,
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg(2), &classifier_cfg(), &mock).expect("cycle runs");
    // No STOP signals expected; assertion passes.
    assert!(
        report.stop_signals.is_empty(),
        "byte-identical pair must not raise STOP: {:?}",
        report.stop_signals
    );
    assert!(
        report
            .assertions
            .iter()
            .any(|a| a.name == "idempotency.byte_identical_paired_stdout" && a.passed),
        "idempotency assertion must pass"
    );
}

#[test]
fn non_byte_identical_paired_stdout_raises_idempotency_stop() {
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: ok_continue_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    mock.push_canned(StatusInvocation {
        stdout: ok_continue_bytes_perturbed(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let report = run_cycle(&cfg(2), &classifier_cfg(), &mock).expect("cycle runs");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::IdempotencyMismatch)),
        "must surface IdempotencyMismatch"
    );
    assert!(
        report
            .assertions
            .iter()
            .any(|a| a.name == "idempotency.byte_identical_paired_stdout" && !a.passed),
        "idempotency assertion must fail"
    );
    // Both raw stdouts must remain in the report at full fidelity.
    assert_eq!(report.invocations.len(), 2);
    assert_ne!(
        report.invocations[0].stdout_raw, report.invocations[1].stdout_raw,
        "report carries both raw stdouts verbatim"
    );
}

#[test]
fn independent_subprocess_invariant_paired_invocations() {
    // Each repeat must result in an independent invocation. The mock
    // records each call; we assert exactly two records exist after a
    // repeat_invocations=2 cycle.
    let bytes = ok_continue_bytes();
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: bytes.clone(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    mock.push_canned(StatusInvocation {
        stdout: bytes,
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let _ = run_cycle(&cfg(2), &classifier_cfg(), &mock).expect("cycle runs");
    assert_eq!(
        mock.calls().len(),
        2,
        "each repeat must invoke the subprocess once; cached reuse is forbidden"
    );
}
