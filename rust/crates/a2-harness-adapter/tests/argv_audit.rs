//! Subprocess argv audit: the harness invokes only the read-only
//! status command with the two A2-L2d positional arguments and no
//! flags. Tests assert against the recorded argv in the mock.

use std::ffi::OsString;
use std::path::PathBuf;

use a2_harness_adapter::{
    build_status_argv, run_cycle, ClassifierConfig, HarnessAssertionConfig, MockStatusInvoker,
    StatusInvocation,
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
fn pure_argv_builder_has_no_flags_in_default_case() {
    let bin: OsString = "claw".into();
    let ws = PathBuf::from("/tmp/disposable/example");
    let argv = build_status_argv(&bin, &ws, None);
    for a in &argv {
        let s = a.to_string_lossy();
        assert!(
            !s.starts_with('-'),
            "argv MUST NOT contain any flag; saw `{s}` in {argv:?}"
        );
    }
}

#[test]
fn cycle_emits_only_plan_status_positionals_no_flags() {
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
    let _report = run_cycle(&cfg, &classifier_cfg(), &mock).expect("cycle runs");
    let calls = mock.calls();
    assert_eq!(calls.len(), 1, "exactly one invocation expected");
    let argv = &calls[0].argv;
    // Expected: [binary, "plan", "status", <workspace>] — no flags.
    assert!(argv.len() == 4 || argv.len() == 5, "argv len {argv:?}");
    assert_eq!(argv[1], OsString::from("plan"));
    assert_eq!(argv[2], OsString::from("status"));
    for a in argv {
        let s = a.to_string_lossy();
        assert!(
            !s.starts_with('-'),
            "argv MUST NOT contain any flag; saw `{s}` in {argv:?}"
        );
    }
}

#[test]
fn cycle_with_approval_result_forwards_optional_positional() {
    let mut cfg = HarnessAssertionConfig {
        workspace_root: disposable_workspace_ok_path(),
        workspace_is_disposable: true,
        ..Default::default()
    };
    let ar_path = disposable_workspace_ok_path().join("approval-result.json");
    cfg.approval_result_path = Some(ar_path.clone());

    let mock = MockStatusInvoker::new();
    mock.push_canned(StatusInvocation {
        stdout: ok_envelope_bytes(),
        stderr: Vec::new(),
        exit_code: 0,
        argv: Vec::new(),
    });
    let _report = run_cycle(&cfg, &classifier_cfg(), &mock).expect("cycle runs");
    let calls = mock.calls();
    assert_eq!(calls[0].argv.len(), 5, "five argv elements expected");
    assert_eq!(calls[0].argv[4], OsString::from(ar_path.as_os_str()));
    assert_eq!(calls[0].approval_result, Some(ar_path));
}
