//! A2-L2b slice-4a integration tests — `ApprovedWritePayload`.
//!
//! These tests prove the offline contract:
//!
//! - Valid payload binds successfully.
//! - Hash mismatch refuses.
//! - Non-approvable previews refuse (binary, redacted, truncated).
//! - Payload size cap refuses past `MAX_APPROVED_PAYLOAD_BYTES`.
//! - `after_size_bytes` is recorded.
//! - `after_bytes()` accessor returns a read-only borrow.
//! - All identity fields (`step_id`, `preview_id`, `preview_sha256`,
//!   `before_sha256`, `after_sha256`) are copied from the record.
//! - Target relative path is preserved verbatim.
//! - Target mismatch refuses.
//! - Absolute / `..` / OS-prefix target refuses.
//! - No serde derive: any attempt to serialize would fail to compile;
//!   we encode that intent in a documented compile-fence test.
//! - `PreviewDisplay` cannot drive payload construction (the
//!   constructor signature requires `&PreviewRecord`).
//! - `ApprovalDecision` alone cannot drive payload construction.
//! - The new module touches no target-write API, no `run_plan`
//!   wiring, no CLI, no broker.

#![allow(clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};

use a2_plan_runner::diff_preview::{
    build_preview, PreviewInputs, PreviewRecord, PREVIEW_FORMAT_VERSION,
};
use a2_plan_runner::write_payload::{
    bind_after_bytes, ApprovedWritePayload, BindError, MAX_APPROVED_PAYLOAD_BYTES,
};
use ulid::Ulid;

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

fn ulid_zero() -> Ulid {
    Ulid::from_parts(0, 0)
}

fn build_real_record(after: &[u8]) -> (PreviewRecord, PathBuf, Vec<u8>) {
    let target = PathBuf::from("src/lib.rs");
    let run_id = ulid_zero();
    let inputs = PreviewInputs {
        step_id: "step-1",
        target_relative_path: &target,
        target_absolute_path: Path::new("/tmp/ws/src/lib.rs"),
        before: Some(b"alpha\nbeta\n"),
        after,
        checkpoint_run_id: &run_id,
        checkpoint_step_id: "step-1",
        created_at_utc: "2026-05-26T00:00:00.000000000Z",
    };
    let (record, _display) = build_preview(&inputs).expect("preview build");
    (record, target, after.to_vec())
}

fn synth_record(
    is_binary: bool,
    is_redacted: bool,
    is_truncated: bool,
    target_rel: &str,
    after_sha256: &str,
) -> PreviewRecord {
    PreviewRecord {
        preview_id: "01HZZZZZZZZZZZZZZZZZZZZZZ0".into(),
        step_id: "step-1".into(),
        target_relative_path_sanitized: target_rel.into(),
        target_absolute_path_sanitized: format!("/tmp/ws/{target_rel}"),
        before_sha256: "a".repeat(64),
        after_sha256: after_sha256.to_string(),
        preview_sha256: "c".repeat(64),
        checkpoint_run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
        checkpoint_step_id: "step-1".into(),
        is_binary,
        is_redacted,
        is_truncated,
        created_at_utc: "2026-05-26T00:00:00.000000000Z".into(),
        preview_format_version: PREVIEW_FORMAT_VERSION,
    }
}

// -------------------------------------------------------------------------
// Happy path
// -------------------------------------------------------------------------

#[test]
fn valid_payload_binds_successfully() {
    let after = b"alpha\nbeta\ngamma\n";
    let (record, target, bytes) = build_real_record(after);
    let payload = bind_after_bytes(&record, target.clone(), bytes.clone()).expect("bind ok");

    // Identity fields copied from the record.
    assert_eq!(payload.step_id, record.step_id);
    assert_eq!(payload.preview_id, record.preview_id);
    assert_eq!(payload.preview_sha256, record.preview_sha256);
    assert_eq!(payload.before_sha256, record.before_sha256);
    assert_eq!(payload.after_sha256, record.after_sha256);

    // Target path preserved.
    assert_eq!(payload.target_relative_path, target);

    // Size + bytes.
    assert_eq!(payload.after_size_bytes, bytes.len() as u64);
    assert_eq!(payload.after_bytes(), bytes.as_slice());
}

#[test]
fn after_bytes_accessor_returns_borrow() {
    let after = b"hello-world\n";
    let (record, target, bytes) = build_real_record(after);
    let payload = bind_after_bytes(&record, target, bytes).expect("bind ok");
    // `after_bytes()` returns &[u8] — verifying via type-equality.
    let borrow: &[u8] = payload.after_bytes();
    assert_eq!(borrow, after.as_slice());
}

// -------------------------------------------------------------------------
// Hash mismatch
// -------------------------------------------------------------------------

#[test]
fn hash_mismatch_refuses() {
    let after = b"alpha\nbeta\ngamma\n";
    let (record, target, _) = build_real_record(after);
    let tampered = b"alpha\nbeta\ngamma-tampered\n".to_vec();
    match bind_after_bytes(&record, target, tampered) {
        Err(BindError::PayloadHashMismatch { expected, actual }) => {
            assert_eq!(expected, record.after_sha256);
            assert_ne!(actual, record.after_sha256);
            assert_eq!(actual.len(), 64);
        }
        other => panic!("expected PayloadHashMismatch, got {other:?}"),
    }
}

#[test]
fn empty_bytes_against_nonempty_hash_refuses() {
    let after = b"alpha\nbeta\ngamma\n";
    let (record, target, _) = build_real_record(after);
    let res = bind_after_bytes(&record, target, Vec::new());
    assert!(matches!(res, Err(BindError::PayloadHashMismatch { .. })));
}

// -------------------------------------------------------------------------
// Non-approvable previews
// -------------------------------------------------------------------------

#[test]
fn binary_preview_refuses() {
    // Force a binary record via NUL bytes.
    let after = vec![0u8, 1, 2, 3, 4];
    let target = PathBuf::from("src/lib.rs");
    let run_id = ulid_zero();
    let inputs = PreviewInputs {
        step_id: "step-1",
        target_relative_path: &target,
        target_absolute_path: Path::new("/tmp/ws/src/lib.rs"),
        before: Some(b"alpha\nbeta\n"),
        after: &after,
        checkpoint_run_id: &run_id,
        checkpoint_step_id: "step-1",
        created_at_utc: "2026-05-26T00:00:00.000000000Z",
    };
    let (record, _) = build_preview(&inputs).expect("preview build");
    assert!(record.is_binary);
    assert!(!record.is_approvable());
    let res = bind_after_bytes(&record, target, after);
    assert_eq!(res.err(), Some(BindError::PreviewNotApprovable));
}

#[test]
fn redacted_preview_refuses_via_synth() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, true, false, "src/lib.rs", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("src/lib.rs"), after);
    assert_eq!(res.err(), Some(BindError::PreviewNotApprovable));
}

#[test]
fn truncated_preview_refuses_via_synth() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, true, "src/lib.rs", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("src/lib.rs"), after);
    assert_eq!(res.err(), Some(BindError::PreviewNotApprovable));
}

#[test]
fn binary_flag_alone_refuses_via_synth() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(true, false, false, "src/lib.rs", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("src/lib.rs"), after);
    assert_eq!(res.err(), Some(BindError::PreviewNotApprovable));
}

// -------------------------------------------------------------------------
// Size cap
// -------------------------------------------------------------------------

#[test]
fn payload_too_large_refuses() {
    let cap = usize::try_from(MAX_APPROVED_PAYLOAD_BYTES).expect("cap fits in usize");
    let after = vec![0u8; cap + 1];
    // We never need the hash to match — size check trips first.
    let record = synth_record(false, false, false, "src/lib.rs", &"0".repeat(64));
    match bind_after_bytes(&record, PathBuf::from("src/lib.rs"), after) {
        Err(BindError::PayloadTooLarge { actual, cap: c }) => {
            assert_eq!(actual, MAX_APPROVED_PAYLOAD_BYTES + 1);
            assert_eq!(c, MAX_APPROVED_PAYLOAD_BYTES);
        }
        other => panic!("expected PayloadTooLarge, got {other:?}"),
    }
}

#[test]
fn payload_exactly_at_cap_is_accepted_modulo_hash() {
    // Past the size check we must still pass the hash check. Build a
    // synthetic record whose `after_sha256` matches a cap-sized zero
    // buffer, and bind it.
    let cap = usize::try_from(MAX_APPROVED_PAYLOAD_BYTES).expect("cap fits in usize");
    let bytes = vec![0u8; cap];
    let sha = sha256_hex(&bytes);
    let record = synth_record(false, false, false, "src/lib.rs", &sha);
    let payload =
        bind_after_bytes(&record, PathBuf::from("src/lib.rs"), bytes.clone()).expect("bind ok");
    assert_eq!(payload.after_size_bytes, MAX_APPROVED_PAYLOAD_BYTES);
    assert_eq!(
        payload.after_bytes().len() as u64,
        MAX_APPROVED_PAYLOAD_BYTES
    );
}

// -------------------------------------------------------------------------
// Target path safety + identity
// -------------------------------------------------------------------------

#[test]
fn absolute_target_path_refuses() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "/tmp/ws/src/lib.rs", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("/etc/passwd"), after);
    assert_eq!(res.err(), Some(BindError::InvalidTargetPath));
}

#[test]
fn parent_traversal_target_refuses() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "../escape.txt", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("../escape.txt"), after);
    assert_eq!(res.err(), Some(BindError::InvalidTargetPath));
}

#[test]
fn empty_target_path_refuses() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "", &sha);
    let res = bind_after_bytes(&record, PathBuf::new(), after);
    assert_eq!(res.err(), Some(BindError::InvalidTargetPath));
}

#[test]
fn target_mismatch_refuses() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "src/lib.rs", &sha);
    let res = bind_after_bytes(&record, PathBuf::from("src/main.rs"), after);
    assert_eq!(res.err(), Some(BindError::TargetPathMismatch));
}

#[test]
fn target_mismatch_via_subdir_refuses() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "src/lib.rs", &sha);
    // Producer typed `lib.rs`; caller hands us `lib/lib.rs`. Different
    // file. Mismatch.
    let res = bind_after_bytes(&record, PathBuf::from("lib/lib.rs"), after);
    assert_eq!(res.err(), Some(BindError::TargetPathMismatch));
}

#[test]
fn target_preserves_caller_pathbuf() {
    let after = b"hello\n";
    let (record, target, bytes) = build_real_record(after);
    let payload = bind_after_bytes(&record, target.clone(), bytes).expect("bind ok");
    assert_eq!(payload.target_relative_path, target);
    // The payload's path renders identically to the canonical
    // sanitized path the producer recorded.
    assert_eq!(
        payload.target_relative_path.to_string_lossy(),
        record.target_relative_path_sanitized
    );
}

// -------------------------------------------------------------------------
// Refusal precedence
// -------------------------------------------------------------------------
//
// Refusal order is: PreviewNotApprovable → InvalidTargetPath →
// TargetPathMismatch → PayloadTooLarge → PayloadHashMismatch.
// A single test per pair would be overkill; one representative
// crossover suffices for documentation.

#[test]
fn non_approvable_takes_precedence_over_other_issues() {
    // Build a non-approvable (binary) record AND give it a wrong
    // path / wrong bytes. We must see PreviewNotApprovable first.
    let record = synth_record(true, false, false, "src/lib.rs", &"0".repeat(64));
    let res = bind_after_bytes(&record, PathBuf::from("../escape"), b"wrong".to_vec());
    assert_eq!(res.err(), Some(BindError::PreviewNotApprovable));
}

#[test]
fn invalid_path_takes_precedence_over_target_mismatch() {
    let after = b"x".to_vec();
    let sha = sha256_hex(&after);
    let record = synth_record(false, false, false, "src/lib.rs", &sha);
    // Absolute path -> InvalidTargetPath, not TargetPathMismatch.
    let res = bind_after_bytes(&record, PathBuf::from("/abs"), after);
    assert_eq!(res.err(), Some(BindError::InvalidTargetPath));
}

#[test]
fn size_cap_takes_precedence_over_hash_mismatch() {
    let cap = usize::try_from(MAX_APPROVED_PAYLOAD_BYTES).expect("cap fits in usize");
    let after = vec![0u8; cap + 1];
    // Synth record's `after_sha256` is "0"*64 which does not match
    // sha256(zeros). Size check trips before hash check.
    let record = synth_record(false, false, false, "src/lib.rs", &"0".repeat(64));
    let res = bind_after_bytes(&record, PathBuf::from("src/lib.rs"), after);
    match res {
        Err(BindError::PayloadTooLarge { .. }) => {}
        other => panic!("expected PayloadTooLarge, got {other:?}"),
    }
}

// -------------------------------------------------------------------------
// Constructor signature: cannot be driven by PreviewDisplay /
// ApprovalDecision alone.
// -------------------------------------------------------------------------

/// If `bind_after_bytes` ever changes shape (e.g. accepts
/// `&PreviewDisplay` or `&ApprovalDecision` instead of
/// `&PreviewRecord`), this assignment fails to type-check. The test
/// is a function-pointer pin against the public signature.
#[test]
fn constructor_signature_pinned_to_preview_record() {
    // Anonymous wildcard binding — no value name, no underscore-prefix
    // lint, but type-checks the public signature shape.
    let _: fn(&PreviewRecord, PathBuf, Vec<u8>) -> Result<ApprovedWritePayload, BindError> =
        bind_after_bytes;
}

// -------------------------------------------------------------------------
// Serialization absence
// -------------------------------------------------------------------------
//
// `ApprovedWritePayload` deliberately does NOT derive `Serialize` /
// `Deserialize`. Encoding the negative as a compile-time assertion
// requires either `static_assertions` (not a workspace dependency) or
// a `trybuild` compile-fail harness. The slice-4a contract enforces
// the absence by:
//
//   1. The source file having no `#[derive(Serialize)]` or
//      `#[derive(Deserialize)]` on `ApprovedWritePayload`.
//   2. The workspace scope audit (see PR description) grepping for
//      serde derives in the new module.
//   3. This module-level test which documents the contract and
//      provides a stable place for a future `static_assertions`-based
//      negative trait bound, should the workspace adopt that crate.
#[test]
fn approved_write_payload_has_no_serde_derives() {
    // The body is a documented assertion; the contract is enforced
    // by the source-level absence of derives. This test exists so
    // that anyone modifying write_payload.rs runs straight into the
    // contract via `cargo test` output.
    let contract = include_str!("../src/write_payload.rs");
    assert!(
        !contract.contains("#[derive(Serialize"),
        "ApprovedWritePayload must not derive Serialize"
    );
    assert!(
        !contract.contains("#[derive(Deserialize"),
        "ApprovedWritePayload must not derive Deserialize"
    );
    // Belt-and-braces: the module must not even import serde.
    assert!(
        !contract.contains("use serde"),
        "write_payload.rs must not import serde"
    );
}

// -------------------------------------------------------------------------
// Module purity: no target-write APIs, no run_plan wiring, no broker.
// -------------------------------------------------------------------------

#[test]
fn write_payload_module_has_no_target_write_apis() {
    let src = include_str!("../src/write_payload.rs");
    for forbidden in [
        "File::create",
        "OpenOptions",
        "std::fs::write",
        "write_all",
        "fs::rename",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::create_dir",
        "Command::new",
        "spawn(",
        "11434",
        "11435",
        "OPENAI_BASE_URL",
        "vram-broker",
        "SideStackAI",
        "sidestackai",
        "run_plan",
    ] {
        assert!(
            !src.contains(forbidden),
            "write_payload.rs must not reference `{forbidden}`"
        );
    }
}

// -------------------------------------------------------------------------
// Local helper: re-derive sha256 hex to keep tests independent of
// the runner's pub(crate) helper.
// -------------------------------------------------------------------------

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    let mut s = String::with_capacity(d.len() * 2);
    for &b in &d {
        s.push(HEX_LOWER[(b >> 4) as usize] as char);
        s.push(HEX_LOWER[(b & 0x0f) as usize] as char);
    }
    s
}
