//! Schema-drift / unknown-enum / `read_only_invariant` STOP coverage.

use std::path::PathBuf;

use a2_harness_adapter::{
    classify_next_operator_command, parse_envelope, EnvelopeParseError, NextOpCommandShape,
    READ_ONLY_INVARIANT_LITERAL,
};

mod common;
use common::*;

fn ws() -> PathBuf {
    disposable_workspace_ok_path()
}

#[test]
fn schema_version_mismatch_is_stop() {
    let env = build_envelope(
        &ws(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT_LITERAL,
        "a2-something-else.v999",
    );
    let bytes = envelope_bytes(&env);
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(err, EnvelopeParseError::SchemaVersionMismatch(_)));
    assert_eq!(err.observed(), Some("a2-something-else.v999"));
}

#[test]
fn unknown_phase_is_stop() {
    let env = build_envelope(
        &ws(),
        "synthesized_unknown_phase",
        NEXT_OP_STOP_ESCALATE,
        false,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT_LITERAL,
        STATUS_SCHEMA_V1,
    );
    let bytes = envelope_bytes(&env);
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(err, EnvelopeParseError::SchemaDrift(_)));
}

#[test]
fn unknown_stop_condition_is_stop() {
    let mut env = build_envelope(
        &ws(),
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
    // Inject an unknown stop_condition literal that the closed enum
    // would refuse.
    env["stop_condition"] = serde_json::Value::String("synthesized-unknown-stop".into());
    let bytes = envelope_bytes(&env);
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(err, EnvelopeParseError::SchemaDrift(_)));
}

#[test]
fn unknown_next_op_command_shape_is_drift_signal() {
    // Parse succeeds (closed enums in the envelope are still valid).
    // The harness's separate `classify_next_operator_command` flags
    // the unknown shape; the cycle treats that as a STOP signal.
    let env = build_envelope(
        &ws(),
        "applied",
        "something else entirely",
        false,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        READ_ONLY_INVARIANT_LITERAL,
        STATUS_SCHEMA_V1,
    );
    let bytes = envelope_bytes(&env);
    let parsed = parse_envelope(&bytes).expect("envelope parses");
    assert_eq!(
        classify_next_operator_command(&parsed.next_operator_command),
        NextOpCommandShape::Unknown
    );
}

#[test]
fn missing_read_only_invariant_is_stop() {
    let mut env = build_envelope(
        &ws(),
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
    let obj = env.as_object_mut().expect("envelope is an object");
    obj.remove("read_only_invariant");
    let bytes = envelope_bytes(&env);
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(err, EnvelopeParseError::SchemaDrift(_)));
}

#[test]
fn altered_read_only_invariant_is_stop() {
    let env = build_envelope(
        &ws(),
        "awaiting_approval",
        &next_op_canonical_approve(),
        true,
        false,
        None,
        &[],
        &["a2-l2d-status-read"],
        "this command does not mutate state, probably",
        STATUS_SCHEMA_V1,
    );
    let bytes = envelope_bytes(&env);
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(
        err,
        EnvelopeParseError::ReadOnlyInvariantAltered(_)
    ));
    assert_eq!(
        err.observed(),
        Some("this command does not mutate state, probably")
    );
}

#[test]
fn invalid_json_is_stop() {
    let bytes = b"this is not json {".to_vec();
    let err = parse_envelope(&bytes).expect_err("must reject");
    assert!(matches!(err, EnvelopeParseError::InvalidJson(_)));
}

#[test]
fn unknown_audit_marker_is_preserved_verbatim_on_parse() {
    // Audit markers are an array of free strings, so an unknown marker
    // parses through the schema. The harness reports it verbatim and
    // the cycle layer (not the parser) is what surfaces it as a STOP
    // signal.
    let env = build_envelope(
        &ws(),
        "applied",
        NEXT_OP_STOP_ESCALATE,
        false,
        false,
        None,
        &[],
        &["a2-l2d-status-read", "synthesized-unknown-marker"],
        READ_ONLY_INVARIANT_LITERAL,
        STATUS_SCHEMA_V1,
    );
    let bytes = envelope_bytes(&env);
    let parsed = parse_envelope(&bytes).expect("envelope parses");
    assert!(
        parsed
            .audit_markers
            .iter()
            .any(|m| m == "synthesized-unknown-marker"),
        "unknown marker must be preserved verbatim"
    );
}
