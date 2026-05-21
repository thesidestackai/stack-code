//! A2-L2b slice 2 — checkpoint-store integration tests.
//!
//! These tests exercise the live filesystem path of
//! [`a2_plan_runner::checkpoint::CheckpointStore`] under a hand-rolled
//! tempdir guard (no `tempfile` crate). Every test:
//!
//! - Creates an isolated workspace root under [`std::env::temp_dir`].
//! - Drives `create_checkpoint` against a target file inside that root.
//! - Asserts that the target file itself was never modified.
//! - Cleans up via [`Drop`] (best-effort).
//!
//! Slice 2 is offline only: no broker, no model calls, no subprocesses,
//! no wiring into `run_plan`. The store writes only inside
//! `<workspace_root>/.claw/l2b-checkpoints/`.

#![cfg(unix)]

use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use a2_plan_runner::checkpoint::{
    CheckpointError, CheckpointStore, Manifest, EXIT_CHECKPOINT_FAILED, MANIFEST_VERSION,
    MAX_CHECKPOINT_BYTES,
};
use a2_plan_runner::markers::{
    l2b_run_id_marker, L2B_CHECKPOINT_REFUSED, L2B_CHECKPOINT_TOO_LARGE, L2B_CHECKPOINT_WRITTEN,
    L2B_RUN_ID_PREFIX,
};
use a2_plan_runner::report::EXIT_PARSE_ERROR;
use a2_plan_runner::write_runtime::EXIT_WRITE_PATH_REFUSED;
use ulid::Ulid;

// -------------------------------------------------------------------------
// Hand-rolled TempWorkspace (no `tempfile` crate)
// -------------------------------------------------------------------------

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock not before unix epoch")
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "a2_l2b_checkpoint_{}_{}_{}",
            label,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir(&p).expect("tempdir create");
        let root = p.canonicalize().expect("tempdir canonicalize");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write_target(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let abs = self.root.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).expect("create_dir_all parent");
        }
        fs::write(&abs, bytes).expect("write target");
        abs
    }

    fn target_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn store(ws: &TempWorkspace) -> CheckpointStore {
    CheckpointStore::new_with_generated_run_id(ws.root().to_path_buf())
}

fn read_manifest(path: &Path) -> Manifest {
    let text = fs::read_to_string(path).expect("read manifest");
    serde_json::from_str::<Manifest>(&text).expect("parse manifest")
}

fn sha256_hex_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write as _;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        write!(&mut s, "{b:02x}").expect("write to String never fails");
    }
    s
}

// -------------------------------------------------------------------------
// 1. creates_checkpoint_for_existing_regular_file
// -------------------------------------------------------------------------

#[test]
fn creates_checkpoint_for_existing_regular_file() {
    let ws = TempWorkspace::new("existing");
    let abs = ws.write_target("docs/README.md", b"hello world\n");
    let s = store(&ws);

    let handle = s
        .create_checkpoint("step-1", &abs, Path::new("docs/README.md"))
        .expect("create_checkpoint");

    assert!(handle.step_dir.is_dir(), "step dir must exist");
    assert!(handle.manifest_path.is_file(), "manifest must be a file");
    let before = handle
        .before_bin_path
        .as_ref()
        .expect("existing branch must produce before.bin");
    assert!(before.is_file(), "before.bin must be a file");
    assert!(handle.manifest.pre_existed);
    assert_eq!(handle.manifest.pre_target_kind, "regular_file");
    assert_eq!(handle.manifest.pre_size_bytes, 12);
}

// -------------------------------------------------------------------------
// 2. creates_checkpoint_for_absent_target
// -------------------------------------------------------------------------

#[test]
fn creates_checkpoint_for_absent_target() {
    let ws = TempWorkspace::new("absent");
    // Create the parent dir but NOT the target file.
    fs::create_dir_all(ws.root().join("docs")).unwrap();
    let abs = ws.target_path("docs/missing.md");
    assert!(!abs.exists(), "target must not exist for absent branch");
    let s = store(&ws);

    let handle = s
        .create_checkpoint("step-1", &abs, Path::new("docs/missing.md"))
        .expect("create_checkpoint absent");

    assert!(handle.step_dir.is_dir());
    assert!(handle.manifest_path.is_file());
    assert!(
        handle.before_bin_path.is_none(),
        "absent branch must not produce before.bin"
    );
    assert!(!handle.step_dir.join("before.bin").exists());
    assert!(!handle.manifest.pre_existed);
    assert_eq!(handle.manifest.pre_target_kind, "absent");
    assert_eq!(handle.manifest.pre_size_bytes, 0);
    assert_eq!(handle.manifest.pre_sha256, "");
    assert!(handle.manifest.pre_mtime_unix_ns.is_none());
    assert!(handle.manifest.pre_permissions_octal.is_none());
    assert!(!abs.exists(), "absent target must remain absent");
}

// -------------------------------------------------------------------------
// 3. manifest_contains_all_v1_fields
// -------------------------------------------------------------------------

#[test]
fn manifest_contains_all_v1_fields() {
    let ws = TempWorkspace::new("manifest_fields");
    let abs = ws.write_target("a.txt", b"abcd");
    let s = store(&ws);
    let handle = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();
    let m = read_manifest(&handle.manifest_path);

    assert_eq!(m.manifest_version, MANIFEST_VERSION);
    let run_id_str = s.run_id().to_string();
    assert_eq!(m.plan_run_id, run_id_str);
    assert_eq!(m.step_id, "step-1");
    assert_eq!(m.target_relative_path, "a.txt");
    assert_eq!(m.target_absolute_path, abs.display().to_string());
    assert!(m.pre_existed);
    assert_eq!(m.pre_target_kind, "regular_file");
    assert_eq!(m.pre_size_bytes, 4);
    assert_eq!(m.pre_sha256.len(), 64);
    assert!(m.pre_sha256.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(
        m.pre_mtime_unix_ns.is_some(),
        "Unix should expose mtime nanos"
    );
    assert!(
        m.pre_permissions_octal
            .as_deref()
            .is_some_and(|s| s.len() == 4),
        "Unix should expose 4-digit octal mode"
    );
    // RFC3339 Z shape: 2026-05-20T22:43:23.123456789Z
    assert!(m.created_at_utc.ends_with('Z'));
    assert!(m.created_at_utc.contains('T'));
    assert!(m.created_at_utc.contains('.'));
    assert_eq!(m.runner_crate_version, env!("CARGO_PKG_VERSION"));
}

// -------------------------------------------------------------------------
// 4. before_bin_matches_original_byte_for_byte (incl. non-UTF-8 bytes)
// -------------------------------------------------------------------------

#[test]
fn before_bin_matches_original_byte_for_byte_including_non_utf8() {
    let ws = TempWorkspace::new("byte_exact");
    // Includes a stray continuation byte and a null and 0xFF — not
    // valid UTF-8, must round-trip byte-for-byte.
    let payload: Vec<u8> = vec![0x00, 0xff, 0xc3, 0x28, 0xde, 0xad, 0xbe, 0xef, 0x01];
    let abs = ws.write_target("blob.bin", &payload);
    let s = store(&ws);
    let handle = s
        .create_checkpoint("step-1", &abs, Path::new("blob.bin"))
        .unwrap();
    let bin = handle.before_bin_path.as_ref().unwrap();
    let copied = fs::read(bin).unwrap();
    assert_eq!(copied, payload);
    // Manifest hash must match a freshly-computed digest of the original.
    let m = read_manifest(&handle.manifest_path);
    assert_eq!(m.pre_sha256, sha256_hex_of(&payload));
}

// -------------------------------------------------------------------------
// 5. refuses_overwrite_of_existing_step_dir
// -------------------------------------------------------------------------

#[test]
fn refuses_overwrite_of_existing_step_dir() {
    let ws = TempWorkspace::new("overwrite");
    let abs = ws.write_target("a.txt", b"first");
    let s = store(&ws);
    let _first = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .expect("first checkpoint must succeed");

    // Second call against same (run_id, step_id) must refuse.
    let second = s.create_checkpoint("step-1", &abs, Path::new("a.txt"));
    match second {
        Err(CheckpointError::AlreadyExists { step_dir }) => {
            assert!(step_dir.ends_with("step-1"));
        }
        other => panic!("expected AlreadyExists, got {other:?}"),
    }
    // First checkpoint must be intact.
    let first_manifest = s.step_dir("step-1").unwrap().join("manifest.json");
    assert!(first_manifest.is_file(), "first manifest must remain");
    let m = read_manifest(&first_manifest);
    assert_eq!(m.pre_size_bytes, 5);
}

// -------------------------------------------------------------------------
// 6. refuses_file_larger_than_cap (sparse file; no in-memory 16 MiB alloc)
// -------------------------------------------------------------------------

#[test]
fn refuses_file_larger_than_cap() {
    let ws = TempWorkspace::new("too_large");
    let abs = ws.target_path("huge.bin");
    fs::create_dir_all(abs.parent().unwrap()).ok();
    // Build a sparse file of size > MAX_CHECKPOINT_BYTES using seek+1-byte.
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&abs)
            .unwrap();
        f.seek(SeekFrom::Start(MAX_CHECKPOINT_BYTES)).unwrap();
        f.write_all(&[0u8]).unwrap();
    }
    let md = fs::metadata(&abs).unwrap();
    assert!(md.len() > MAX_CHECKPOINT_BYTES);

    let s = store(&ws);
    let err = s
        .create_checkpoint("step-1", &abs, Path::new("huge.bin"))
        .expect_err("oversize must be refused");
    match &err {
        CheckpointError::TargetTooLarge { actual, cap } => {
            assert_eq!(*cap, MAX_CHECKPOINT_BYTES);
            assert!(*actual > MAX_CHECKPOINT_BYTES);
        }
        other => panic!("expected TargetTooLarge, got {other:?}"),
    }
    assert_eq!(err.marker(), L2B_CHECKPOINT_TOO_LARGE);
    assert_eq!(err.exit_code(), EXIT_CHECKPOINT_FAILED);
    // No leftover step dir.
    assert!(!s.step_dir("step-1").unwrap().exists());
    // Target file is unmodified (still sparse, still oversize).
    let md_after = fs::metadata(&abs).unwrap();
    assert_eq!(md.len(), md_after.len());
}

// -------------------------------------------------------------------------
// 7. refuses_invalid_step_id
// -------------------------------------------------------------------------

#[test]
fn refuses_invalid_step_id() {
    let ws = TempWorkspace::new("bad_step_id");
    let abs = ws.write_target("a.txt", b"x");
    let s = store(&ws);
    let too_long = "x".repeat(129);
    for bad in ["", ".", "..", "a/b", &too_long, "a\0b"] {
        let err = s
            .create_checkpoint(bad, &abs, Path::new("a.txt"))
            .expect_err(&format!("must refuse step_id {bad:?}"));
        match err {
            CheckpointError::StepIdInvalid { step_id } => {
                assert_eq!(step_id, bad, "echo input verbatim");
            }
            other => panic!("expected StepIdInvalid for {bad:?}, got {other:?}"),
        }
    }
    // No step dirs were ever created under this run.
    let run_dir = s.run_dir();
    if run_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&run_dir).unwrap().collect();
        assert!(
            entries.is_empty(),
            "no step dirs should exist after refusals"
        );
    }
}

// -------------------------------------------------------------------------
// 8. step_id_accepts_valid_characters
// -------------------------------------------------------------------------

#[test]
fn step_id_accepts_valid_characters() {
    let ws = TempWorkspace::new("good_ids");
    for id in ["a", "step-1", "step_1", "step.1", "ABCxyz_123-456.789"] {
        let abs = ws.write_target(&format!("{id}.txt"), b"x");
        let s = store(&ws);
        let h = s
            .create_checkpoint(id, &abs, Path::new("ignored"))
            .unwrap_or_else(|e| panic!("step_id {id:?} should be accepted: {e:?}"));
        assert!(h.step_dir.ends_with(id));
    }
}

// -------------------------------------------------------------------------
// 9. markers_and_prefix_pinned
// -------------------------------------------------------------------------

#[test]
fn markers_and_prefix_pinned() {
    assert_eq!(L2B_RUN_ID_PREFIX, "a2-l2b-run-id=");
    assert_eq!(L2B_CHECKPOINT_WRITTEN, "a2-l2b-checkpoint-written");
    assert_eq!(L2B_CHECKPOINT_TOO_LARGE, "a2-l2b-checkpoint-too-large");
    assert_eq!(L2B_CHECKPOINT_REFUSED, "a2-l2b-checkpoint-refused");
    let id = Ulid::new();
    let s = l2b_run_id_marker(&id);
    assert!(s.starts_with(L2B_RUN_ID_PREFIX));
    let body = &s[L2B_RUN_ID_PREFIX.len()..];
    assert_eq!(body.len(), 26);
    let parsed: Ulid = body.parse().expect("ULID body must parse");
    assert_eq!(parsed, id);
}

// -------------------------------------------------------------------------
// 10. target_file_is_never_mutated
// -------------------------------------------------------------------------

#[test]
fn target_file_is_never_mutated_for_existing_branch() {
    let ws = TempWorkspace::new("immutable");
    let payload = b"unchanged".to_vec();
    let abs = ws.write_target("a.txt", &payload);

    let md_before = fs::metadata(&abs).unwrap();
    let bytes_before = fs::read(&abs).unwrap();
    let hash_before = sha256_hex_of(&bytes_before);

    let s = store(&ws);
    let _ = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();

    let md_after = fs::metadata(&abs).unwrap();
    let bytes_after = fs::read(&abs).unwrap();
    let hash_after = sha256_hex_of(&bytes_after);

    assert_eq!(bytes_before, bytes_after);
    assert_eq!(hash_before, hash_after);
    assert_eq!(md_before.len(), md_after.len());
    assert_eq!(
        md_before.permissions().mode() & 0o7777,
        md_after.permissions().mode() & 0o7777
    );
    assert_eq!(
        md_before
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()),
        md_after
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()),
        "mtime must not change"
    );
}

#[test]
fn target_file_is_never_created_for_absent_branch() {
    let ws = TempWorkspace::new("immutable_absent");
    fs::create_dir_all(ws.root().join("docs")).unwrap();
    let abs = ws.target_path("docs/never.md");
    assert!(!abs.exists());

    let s = store(&ws);
    let _ = s
        .create_checkpoint("step-1", &abs, Path::new("docs/never.md"))
        .unwrap();
    assert!(
        !abs.exists(),
        "absent target must remain absent after checkpoint"
    );
}

// -------------------------------------------------------------------------
// 11. permissions_best_effort_unix
// -------------------------------------------------------------------------

#[test]
fn permissions_best_effort_unix() {
    let ws = TempWorkspace::new("perms");
    let abs = ws.write_target("a.txt", b"x");
    let s = store(&ws);
    let h = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();

    let run_dir_mode = fs::metadata(s.run_dir()).unwrap().permissions().mode() & 0o7777;
    assert_eq!(run_dir_mode, 0o700, "run dir must be 0700");

    let step_dir_mode = fs::metadata(&h.step_dir).unwrap().permissions().mode() & 0o7777;
    assert_eq!(step_dir_mode, 0o700, "step dir must be 0700");

    let manifest_mode = fs::metadata(&h.manifest_path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(manifest_mode, 0o600, "manifest.json must be 0600");

    let before_mode = fs::metadata(h.before_bin_path.as_ref().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(before_mode, 0o600, "before.bin must be 0600");
}

// -------------------------------------------------------------------------
// 12. manifest_json_round_trips
// -------------------------------------------------------------------------

#[test]
fn manifest_json_round_trips_from_disk() {
    let ws = TempWorkspace::new("round_trip");
    let abs = ws.write_target("a.txt", b"abc");
    let s = store(&ws);
    let h = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();
    let from_disk = read_manifest(&h.manifest_path);
    assert_eq!(from_disk, h.manifest);
    // Re-serialize and re-parse — bit-stable manifest.
    let json = serde_json::to_vec_pretty(&from_disk).unwrap();
    let again: Manifest = serde_json::from_slice(&json).unwrap();
    assert_eq!(again, from_disk);
}

// -------------------------------------------------------------------------
// 13. exit_code_pinned_to_nine
// -------------------------------------------------------------------------

#[test]
fn exit_code_pinned_to_nine() {
    assert_eq!(EXIT_CHECKPOINT_FAILED, 9);
}

// -------------------------------------------------------------------------
// 14. codes_seven_and_eight_remain_reserved
// -------------------------------------------------------------------------

#[test]
fn codes_seven_and_eight_remain_reserved() {
    // None of the publicly-bound exit-code constants exposed by the
    // runner crate may equal 7 or 8 in slice 2.
    for code in [
        EXIT_PARSE_ERROR,
        EXIT_WRITE_PATH_REFUSED,
        EXIT_CHECKPOINT_FAILED,
    ] {
        assert_ne!(code, 7, "code 7 is reserved for EXIT_APPROVAL_DENIED");
        assert_ne!(code, 8, "code 8 is reserved for EXIT_ROLLBACK_FAILED");
    }
}

// -------------------------------------------------------------------------
// 15. concurrent_step_dirs_under_one_run_id_are_isolated
// -------------------------------------------------------------------------

#[test]
fn concurrent_step_dirs_under_one_run_id_are_isolated() {
    let ws = TempWorkspace::new("multi_step");
    let a = ws.write_target("a.txt", b"AAA");
    let b = ws.write_target("b.txt", b"BBBB");
    let s = store(&ws);

    let h1 = s
        .create_checkpoint("step-a", &a, Path::new("a.txt"))
        .unwrap();
    let h2 = s
        .create_checkpoint("step-b", &b, Path::new("b.txt"))
        .unwrap();

    // Both step dirs sit under the same run dir.
    let run_dir = s.run_dir();
    assert!(h1.step_dir.starts_with(&run_dir));
    assert!(h2.step_dir.starts_with(&run_dir));
    assert_ne!(h1.step_dir, h2.step_dir);

    // Each manifest captured its own target's size.
    let m1 = read_manifest(&h1.manifest_path);
    let m2 = read_manifest(&h2.manifest_path);
    assert_eq!(m1.pre_size_bytes, 3);
    assert_eq!(m2.pre_size_bytes, 4);
    assert_eq!(m1.plan_run_id, s.run_id().to_string());
    assert_eq!(m2.plan_run_id, s.run_id().to_string());
}

// -------------------------------------------------------------------------
// 16. atomic_rename_leaves_no_tmp_files (optional, per plan)
// -------------------------------------------------------------------------

#[test]
fn atomic_rename_leaves_no_tmp_files() {
    let ws = TempWorkspace::new("no_tmp");
    let abs = ws.write_target("a.txt", b"xyz");
    let s = store(&ws);
    let h = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();
    let entries: Vec<_> = fs::read_dir(&h.step_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    for name in &entries {
        let s = name.to_string_lossy();
        assert!(
            !s.ends_with(".tmp"),
            "stray temp file should not survive atomic rename: {s}"
        );
    }
    // The expected set after a successful existing-branch checkpoint.
    let mut names: Vec<String> = entries
        .iter()
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["before.bin".to_string(), "manifest.json".into()]
    );
}

// -------------------------------------------------------------------------
// Bonus: run-id marker can be emitted alongside a real checkpoint.
// -------------------------------------------------------------------------

#[test]
fn run_id_marker_aligns_with_store_run_id() {
    let ws = TempWorkspace::new("run_id_marker");
    let s = store(&ws);
    let marker = l2b_run_id_marker(s.run_id());
    let body = &marker[L2B_RUN_ID_PREFIX.len()..];
    assert_eq!(body, s.run_id().to_string());

    // And the marker body matches what the on-disk manifest will record.
    let abs = ws.write_target("a.txt", b"y");
    let h = s
        .create_checkpoint("step-1", &abs, Path::new("a.txt"))
        .unwrap();
    let m = read_manifest(&h.manifest_path);
    assert_eq!(m.plan_run_id, body);
}
