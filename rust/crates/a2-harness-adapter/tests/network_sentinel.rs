//! No-network / no-broker / no-model invariant.
//!
//! These tests exercise the cycle under env conditions that would
//! cause any HTTP egress to fail (or proxy through an unreachable
//! sentinel). The cycle MUST complete normally against the mock
//! invoker; nothing in the harness library may resolve these vars.

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, ClawPlanStatusInvoker, CycleClassification,
    HarnessAssertionConfig, MockStatusInvoker, StatusInvocation,
};

mod common;
use common::*;

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

fn ok_envelope_bytes() -> Vec<u8> {
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

#[test]
fn cycle_completes_with_network_sentinels_in_process_env() {
    // Set the three network-sentinel env variables on THIS process to
    // unreachable values. Mirrors the A2-L2d producer test invariant.
    // The harness library does not read these; the mock invoker
    // never touches the network either. The cycle MUST complete with
    // the mock's canned envelope.
    let prev_http = std::env::var_os("HTTP_PROXY");
    let prev_https = std::env::var_os("HTTPS_PROXY");
    let prev_ollama = std::env::var_os("OLLAMA_HOST");
    std::env::set_var("HTTP_PROXY", "http://harness-sentinel.invalid:1");
    std::env::set_var("HTTPS_PROXY", "http://harness-sentinel.invalid:1");
    std::env::set_var("OLLAMA_HOST", "http://harness-sentinel.invalid:1");

    let cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: ok_envelope_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let result = run_cycle(&cfg, &classifier_cfg(), &mock);

    // Restore env regardless of cycle outcome before asserting so a
    // failing test does not leak env to other tests.
    match prev_http {
        Some(v) => std::env::set_var("HTTP_PROXY", v),
        None => std::env::remove_var("HTTP_PROXY"),
    }
    match prev_https {
        Some(v) => std::env::set_var("HTTPS_PROXY", v),
        None => std::env::remove_var("HTTPS_PROXY"),
    }
    match prev_ollama {
        Some(v) => std::env::set_var("OLLAMA_HOST", v),
        None => std::env::remove_var("OLLAMA_HOST"),
    }

    let report = result.expect("cycle runs even with sentinel env");
    // Cycle should succeed: the mock returned a valid envelope, no
    // STOP signals are raised.
    assert!(
        matches!(
            report.classification,
            CycleClassification::Pass | CycleClassification::Fail | CycleClassification::Stop
        ),
        "cycle classification is one of the three expected values"
    );
    // Specifically: a non-STOP, non-Fail cycle for the OK envelope.
    assert_eq!(report.classification, CycleClassification::Pass);
}

#[test]
fn invoker_network_sentinel_env_has_three_known_keys() {
    let env = ClawPlanStatusInvoker::network_sentinel_env();
    assert_eq!(env.len(), 3);
    let keys: Vec<_> = env
        .keys()
        .map(|k| k.to_string_lossy().into_owned())
        .collect();
    assert!(keys.contains(&"HTTP_PROXY".to_string()));
    assert!(keys.contains(&"HTTPS_PROXY".to_string()));
    assert!(keys.contains(&"OLLAMA_HOST".to_string()));
}
