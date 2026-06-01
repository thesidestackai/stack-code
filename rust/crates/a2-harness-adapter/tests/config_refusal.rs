//! Caller-config refusal coverage. The harness refuses any config
//! that would direct it to invoke a non-status plan subcommand, at
//! config-parse time, before any subprocess is spawned.

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, ConfigError, CycleClassification, HarnessAssertionConfig,
    MockStatusInvoker, StopKind,
};

mod common;
use common::*;

fn classifier_cfg() -> ClassifierConfig {
    ClassifierConfig {
        disposable_path_prefixes: vec![disposable_workspace_ok_path()],
        expected_owner_uid: {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                Some(
                    std::fs::metadata(disposable_workspace_ok_path())
                        .expect("meta")
                        .uid(),
                )
            }
            #[cfg(not(unix))]
            {
                None
            }
        },
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    }
}

#[test]
fn config_with_chain_write_approve_reference_refused_at_parse_time() {
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let prefix = plan_prefix();
    let needle_sub = "approve";
    cfg.expected_evidence_substrings = vec![format!("{prefix}{needle_sub} <preview-bundle.json>")];
    let err = cfg.validate().expect_err("must refuse");
    assert!(matches!(
        err,
        ConfigError::ChainWriteSubcommandReferenced(_)
    ));
}

#[test]
fn config_with_chain_write_apply_reference_refused_at_parse_time() {
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let prefix = plan_prefix();
    let needle_sub = "apply";
    cfg.expected_evidence_substrings = vec![format!("{prefix}{needle_sub} <apply-bundle.json>")];
    let err = cfg.validate().expect_err("must refuse");
    assert!(matches!(
        err,
        ConfigError::ChainWriteSubcommandReferenced(_)
    ));
}

#[test]
fn config_with_chain_write_apply_bundle_reference_refused_at_parse_time() {
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let prefix = plan_prefix();
    let needle_sub = "apply-bundle";
    cfg.expected_evidence_substrings = vec![format!("{prefix}{needle_sub} <a> <b>")];
    let err = cfg.validate().expect_err("must refuse");
    assert!(matches!(
        err,
        ConfigError::ChainWriteSubcommandReferenced(_)
    ));
}

#[test]
fn config_with_chain_write_run_reference_refused_at_parse_time() {
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let prefix = plan_prefix();
    let needle_sub = "run";
    cfg.expected_evidence_substrings =
        vec![format!("{prefix}{needle_sub} plan.yaml --workspace-root x")];
    let err = cfg.validate().expect_err("must refuse");
    assert!(matches!(
        err,
        ConfigError::ChainWriteSubcommandReferenced(_)
    ));
}

#[test]
fn cycle_with_refused_config_classifies_stop_and_skips_invocation() {
    // Use a refused config and run a cycle; the cycle MUST classify
    // STOP, emit a config-refusal STOP signal, and MUST NOT spawn any
    // subprocess.
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let prefix = plan_prefix();
    cfg.expected_evidence_substrings = vec![format!("{prefix}approve")];

    let mock = MockStatusInvoker::new();
    let report = run_cycle(&cfg, &classifier_cfg(), &mock).expect("cycle returns Ok report");
    assert_eq!(report.classification, CycleClassification::Stop);
    assert!(
        report
            .stop_signals
            .iter()
            .any(|s| matches!(s.kind, StopKind::ConfigReferencedChainWriteCommand(_))),
        "must surface ConfigReferencedChainWriteCommand"
    );
    assert_eq!(
        mock.calls().len(),
        0,
        "refused config MUST NOT spawn the subprocess"
    );
}
