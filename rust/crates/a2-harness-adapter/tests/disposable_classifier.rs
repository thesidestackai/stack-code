//! Disposable-workspace classifier coverage.
//!
//! AND-semantics: all four signals must pass for `Disposable`. The
//! caller-declared flag alone is insufficient. A missing marker file
//! (or missing path-prefix, or missing owner) classifies as refused
//! unless the caller supplies a per-deployment authorisation doc.

use std::path::{Path, PathBuf};

use a2_harness_adapter::{
    classify_workspace, ClassifierConfig, WorkspaceClassification, DISPOSABLE_MARKER_REL_PATH,
};

mod common;
use common::*;

/// Resolve the owner uid of an existing path. Read-only `fs::metadata`
/// only; no syscalls outside the standard library's safe wrappers.
#[cfg(unix)]
fn path_owner_uid(p: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(p).expect("fixture path readable");
    meta.uid()
}

#[cfg(not(unix))]
fn path_owner_uid(_p: &Path) -> u32 {
    0
}

#[test]
fn all_signals_pass_classifies_disposable() {
    let ws = disposable_workspace_ok_path();
    let cfg = ClassifierConfig {
        disposable_path_prefixes: vec![ws.clone()],
        expected_owner_uid: Some(path_owner_uid(&ws)),
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    };
    let decision = classify_workspace(&ws, &cfg);
    match decision {
        WorkspaceClassification::Disposable { signals } => {
            assert!(signals.path_prefix_allowed, "path-prefix signal must pass");
            assert!(signals.marker_file_present, "marker signal must pass");
            assert!(signals.owner_matches, "owner signal must pass");
            assert!(signals.caller_declared, "caller-declared signal must pass");
        }
        other => panic!("expected Disposable; got {other:?}"),
    }
}

#[test]
fn caller_declaration_alone_is_insufficient() {
    let ws = disposable_workspace_ok_path();
    let cfg = ClassifierConfig {
        // intentionally empty -> path-prefix signal fails
        disposable_path_prefixes: Vec::new(),
        // intentionally None -> owner signal fails
        expected_owner_uid: None,
        // only this is set
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    };
    let decision = classify_workspace(&ws, &cfg);
    assert!(
        matches!(
            decision,
            WorkspaceClassification::NonDisposableAndRefused { .. }
        ),
        "caller declaration alone must NOT classify as disposable; got {decision:?}"
    );
}

#[test]
fn missing_marker_file_refuses_classification() {
    let ws = non_disposable_workspace_path();
    let cfg = ClassifierConfig {
        disposable_path_prefixes: vec![ws.clone()],
        expected_owner_uid: Some(path_owner_uid(&ws)),
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    };
    let decision = classify_workspace(&ws, &cfg);
    assert!(
        matches!(
            decision,
            WorkspaceClassification::NonDisposableAndRefused { .. }
        ),
        "missing marker file must refuse; got {decision:?}"
    );
    let signals = decision.signals();
    assert!(!signals.marker_file_present, "marker signal MUST be false");
}

#[test]
fn marker_relpath_is_the_pinned_constant() {
    // Sanity check on the pinned relative path constant. Bumping
    // requires a separate scope-card amendment.
    assert_eq!(
        DISPOSABLE_MARKER_REL_PATH,
        ".claw/harness-disposable.marker"
    );
}

#[test]
fn non_disposable_with_authorization_doc_yields_authorized_decision() {
    let ws = non_disposable_workspace_path();
    let cfg = ClassifierConfig {
        disposable_path_prefixes: vec![PathBuf::from("/nonexistent-allowlist-root")],
        expected_owner_uid: None,
        caller_declared_disposable: false,
        non_disposable_authorization_doc: Some(
            "docs/per-deployment/example-non-disposable.md".to_string(),
        ),
    };
    let decision = classify_workspace(&ws, &cfg);
    match decision {
        WorkspaceClassification::NonDisposableButAuthorizedBy {
            authorization_doc, ..
        } => {
            assert_eq!(
                authorization_doc, "docs/per-deployment/example-non-disposable.md",
                "authorization doc reference recorded verbatim"
            );
        }
        other => panic!("expected NonDisposableButAuthorizedBy; got {other:?}"),
    }
}

#[test]
fn unauthorized_non_disposable_path_refused_even_with_caller_declaration() {
    let cfg = ClassifierConfig {
        disposable_path_prefixes: Vec::new(),
        expected_owner_uid: None,
        caller_declared_disposable: true,
        non_disposable_authorization_doc: None,
    };
    // Use the repository root (definitely not on any caller-supplied
    // allowlist). This proves the classifier refuses production
    // checkout paths by default.
    let suspicious_path = PathBuf::from("/home/suki/stack-code");
    let decision = classify_workspace(&suspicious_path, &cfg);
    assert!(
        decision.is_refused(),
        "non-disposable production-like path must be refused; got {decision:?}"
    );
}
