//! No-write / no-mutation invariant.
//!
//! Snapshot the fixture-tree byte contents before and after a cycle
//! and assert byte-identical equality afterwards. If the harness ever
//! writes inside the fixture tree, this test fails.
//!
//! Reads only — `fs::read_dir` and `fs::read` — no writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use a2_harness_adapter::{
    run_cycle, ClassifierConfig, HarnessAssertionConfig, MockStatusInvoker, StatusInvocation,
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

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        if meta.is_file() {
            if let Ok(bytes) = std::fs::read(&p) {
                out.insert(p, bytes);
            }
        } else if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&p) {
                for e in entries.flatten() {
                    stack.push(e.path());
                }
            }
        }
    }
    out
}

#[test]
fn fixture_tree_is_byte_identical_before_and_after_cycle() {
    let ws = disposable_workspace_ok_path();
    let before = snapshot_tree(&ws);
    assert!(
        !before.is_empty(),
        "fixture tree must contain at least one file"
    );

    let cfg = HarnessAssertionConfig {
        workspace_root: ws.clone(),
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

    let after = snapshot_tree(&ws);
    assert_eq!(
        before, after,
        "fixture tree byte-snapshots MUST match exactly; the harness MUST NOT write to the workspace"
    );
}

#[test]
fn marker_file_unchanged_after_classifier_run() {
    let ws = disposable_workspace_ok_path();
    let marker_path = ws.join(".claw/harness-disposable.marker");
    let marker_before = std::fs::read(&marker_path).expect("marker present");
    let _ = a2_harness_adapter::classify_workspace(&ws, &classifier_cfg());
    let marker_after = std::fs::read(&marker_path).expect("marker still present");
    assert_eq!(
        marker_before, marker_after,
        "classifier must not touch the marker file"
    );
}
