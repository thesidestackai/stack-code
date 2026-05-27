//! A2-L2b Slice L2b-CLI-Preview-Bundle — integration tests for
//! `claw plan preview-bundle <workspace-root> <target-relative-path> <after-file>`.
//!
//! All tests are no-broker / no-network by construction. They invoke the
//! built `claw` binary as a subprocess with a fully isolated workspace
//! tempdir, drive the generator end-to-end, and assert on the structured
//! `a2-l2b-preview-bundle-generator-result.v1` JSON envelope plus the on-
//! disk artifacts (`after.bin`, `after.sha256`, `preview-bundle.json`,
//! checkpoint manifest).
//!
//! Required operator claims covered here:
//!   * `claw plan preview-bundle` exists and accepts exactly three
//!     positional arguments.
//!   * Pre-approval / batch flags (`--yes`, `--auto`, `--force`,
//!     `--allow-write`, `--preapproved`, `--batch`) are rejected outright.
//!   * Missing / non-regular / symlinked after-files refuse cleanly.
//!   * Path-escape and deny-component target paths refuse cleanly
//!     through the Slice-1 resolver.
//!   * The happy path produces a runner-owned payload artifact, a
//!     sidecar `after.sha256`, a checkpoint, and a `preview-bundle.json`
//!     consumable by `claw plan approve`.
//!   * Target file is NEVER mutated by the generator.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const RESULT_SCHEMA_V1: &str = "a2-l2b-preview-bundle-generator-result.v1";
const PREVIEW_BUNDLE_SCHEMA_V1: &str = "a2-l2b-preview-bundle.v1";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sane")
        .as_nanos();
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "a2-l2b-pbg-it-{}-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
        seq
    ));
    fs::create_dir_all(&dir).expect("tempdir create");
    dir.canonicalize().expect("tempdir canonicalize")
}

fn run_claw_preview_bundle(
    workspace_root: &std::path::Path,
    target_rel: &str,
    after_file: &std::path::Path,
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            &workspace_root.to_string_lossy(),
            target_rel,
            &after_file.to_string_lossy(),
        ])
        .output()
        .expect("claw should launch")
}

fn parse_stdout_json(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf8");
    serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
}

// ------------------------------------------------------------------------
// Parser-layer claims — exactly 3 positionals, no flags.
// ------------------------------------------------------------------------

#[test]
fn preview_bundle_missing_all_args_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "preview-bundle"])
        .output()
        .expect("claw should launch");
    assert!(
        !out.status.success(),
        "missing args must not exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("preview-bundle") || stderr.contains("positional"),
        "expected usage error; stderr={stderr}"
    );
}

#[test]
fn preview_bundle_missing_after_file_arg_is_usage_error() {
    let dir = unique_temp_dir("usage-2args");
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "preview-bundle", &dir.to_string_lossy(), "src/x.rs"])
        .output()
        .expect("claw should launch");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("3 positional"));
}

#[test]
fn preview_bundle_too_many_args_is_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
            "extra",
        ])
        .output()
        .expect("claw should launch");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unexpected positional"),
        "expected extra-positional usage error; stderr={stderr}"
    );
}

#[test]
fn preview_bundle_rejects_yes_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--yes",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported flag") || stderr.contains("--yes"),
        "must reject --yes; stderr={stderr}"
    );
}

#[test]
fn preview_bundle_rejects_auto_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--auto",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--auto"));
}

#[test]
fn preview_bundle_rejects_force_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--force",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--force"));
}

#[test]
fn preview_bundle_rejects_allow_write_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--allow-write",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--allow-write"));
}

#[test]
fn preview_bundle_rejects_preapproved_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--preapproved",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--preapproved"));
}

#[test]
fn preview_bundle_rejects_batch_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args([
            "plan",
            "preview-bundle",
            "--batch",
            "/tmp/ws",
            "src/x.rs",
            "/tmp/after.bin",
        ])
        .output()
        .expect("claw should launch");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported flag") || stderr.contains("--batch"));
}

// ------------------------------------------------------------------------
// Refusals at runtime — workspace root + after-file + target path.
// ------------------------------------------------------------------------

#[test]
fn preview_bundle_workspace_root_missing_refuses() {
    let dir = unique_temp_dir("ws-missing-parent");
    let nonexistent = dir.join("does-not-exist");
    let after_dir = unique_temp_dir("ws-missing-parent-after");
    let after_file = after_dir.join("after.bin");
    fs::write(&after_file, b"hi").unwrap();
    let out = run_claw_preview_bundle(&nonexistent, "src/lib.rs", &after_file);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], RESULT_SCHEMA_V1);
    assert_eq!(json["ok"], false);
    assert_eq!(json["refusal"], "workspace-root-invalid");
}

#[test]
fn preview_bundle_after_file_missing_refuses() {
    let workspace = unique_temp_dir("after-missing-ws");
    let after_dir = unique_temp_dir("after-missing-after");
    let after_file = after_dir.join("nope.bin");
    let out = run_claw_preview_bundle(&workspace, "src/lib.rs", &after_file);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "after-file-missing");
    assert_eq!(json["ok"], false);
}

#[test]
fn preview_bundle_after_file_is_directory_refuses() {
    let workspace = unique_temp_dir("after-dir-ws");
    let after_dir = unique_temp_dir("after-dir-after");
    // `after_dir` itself is the directory we hand to the generator.
    let out = run_claw_preview_bundle(&workspace, "src/lib.rs", &after_dir);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "after-file-not-regular");
}

#[test]
fn preview_bundle_after_file_symlink_refuses() {
    let workspace = unique_temp_dir("after-symlink-ws");
    let after_dir = unique_temp_dir("after-symlink-after");
    let real = after_dir.join("real.bin");
    fs::write(&real, b"hi").unwrap();
    let link = after_dir.join("link.bin");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let out = run_claw_preview_bundle(&workspace, "src/lib.rs", &link);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "after-file-symlink");
}

#[test]
fn preview_bundle_target_path_escape_refuses() {
    let workspace = unique_temp_dir("escape-ws");
    let after_dir = unique_temp_dir("escape-after");
    let after_file = after_dir.join("after.bin");
    fs::write(&after_file, b"hi").unwrap();
    let out = run_claw_preview_bundle(&workspace, "../escape.txt", &after_file);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "target-path-escape");
}

#[test]
fn preview_bundle_target_deny_component_refuses() {
    let workspace = unique_temp_dir("deny-ws");
    let after_dir = unique_temp_dir("deny-after");
    let after_file = after_dir.join("after.bin");
    fs::write(&after_file, b"hi").unwrap();
    let out = run_claw_preview_bundle(&workspace, ".git/config", &after_file);
    assert_eq!(out.status.code(), Some(5));
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["refusal"], "target-deny-component");
}

// ------------------------------------------------------------------------
// Happy path — absent target.
// ------------------------------------------------------------------------

#[test]
fn preview_bundle_absent_target_succeeds() {
    let workspace = unique_temp_dir("absent-target");
    // Slice-1 resolver requires the parent directory to exist even for
    // an absent target (it refuses parent creation in slice 1). Mirror
    // the plan_apply integration tests by pre-creating the parent dir.
    fs::create_dir_all(workspace.join("src")).unwrap();
    let after_dir = unique_temp_dir("absent-after");
    let after_file = after_dir.join("after.bin");
    let after_bytes = b"hello world\n";
    fs::write(&after_file, after_bytes).unwrap();

    let out = run_claw_preview_bundle(&workspace, "src/new.txt", &after_file);
    assert_eq!(
        out.status.code(),
        Some(0),
        "generator must succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["schema_version"], RESULT_SCHEMA_V1);
    assert_eq!(json["ok"], true);
    assert_eq!(json["target_relative_path"], "src/new.txt");
    assert_eq!(json["payload_size_bytes"], after_bytes.len() as u64);
    assert_eq!(json["is_binary"], false);
    assert_eq!(json["is_redacted"], false);
    assert_eq!(json["is_truncated"], false);

    let bundle_path = PathBuf::from(json["preview_bundle_path"].as_str().unwrap());
    assert!(bundle_path.exists(), "preview-bundle.json must exist");
    let bundle_text = fs::read_to_string(&bundle_path).unwrap();
    let bundle: serde_json::Value = serde_json::from_str(&bundle_text).unwrap();
    assert_eq!(bundle["schema_version"], PREVIEW_BUNDLE_SCHEMA_V1);
    assert_eq!(bundle["checkpoint_baseline_unchanged"], true);
    // Empty before_sha256 means the target was absent.
    assert_eq!(bundle["preview_record"]["before_sha256"], "");
    assert!(!bundle["preview_record"]["after_sha256"]
        .as_str()
        .unwrap()
        .is_empty());

    let payload_path = PathBuf::from(json["payload_path"].as_str().unwrap());
    assert!(payload_path.exists());
    let payload_disk = fs::read(&payload_path).unwrap();
    assert_eq!(payload_disk, after_bytes, "payload bytes must match input");

    let sha_path = PathBuf::from(json["payload_sha256_path"].as_str().unwrap());
    assert!(sha_path.exists());
    let sha_text = fs::read_to_string(&sha_path).unwrap();
    let expected_sha = json["payload_sha256"].as_str().unwrap();
    assert!(
        sha_text.starts_with(expected_sha),
        "sha file content {sha_text:?} must start with {expected_sha:?}"
    );

    let manifest_path = PathBuf::from(json["checkpoint_manifest_path"].as_str().unwrap());
    assert!(manifest_path.exists(), "checkpoint manifest must exist");

    // Audit markers are advertised but must NOT be authority — just
    // confirm they are present.
    let markers = json["audit_markers"].as_array().unwrap();
    let marker_strings: Vec<&str> = markers.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(marker_strings.contains(&"a2-l2b-preview-bundle-created"));
    assert!(marker_strings.contains(&"a2-l2b-payload-captured"));
    assert!(marker_strings.contains(&"a2-l2b-checkpoint-written"));

    // The target file MUST NOT have been created or mutated.
    assert!(!workspace.join("src/new.txt").exists());

    // Stdout MUST NOT contain raw payload bytes inline.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("hello world"));
}

// ------------------------------------------------------------------------
// Happy path — existing target.
// ------------------------------------------------------------------------

#[test]
fn preview_bundle_existing_target_succeeds_and_target_unchanged() {
    let workspace = unique_temp_dir("existing-target");
    let target_rel = "src/lib.rs";
    let target_abs = workspace.join(target_rel);
    fs::create_dir_all(target_abs.parent().unwrap()).unwrap();
    let before_bytes = b"old contents\n";
    fs::write(&target_abs, before_bytes).unwrap();

    let after_dir = unique_temp_dir("existing-after");
    let after_file = after_dir.join("after.bin");
    let after_bytes = b"new contents\n";
    fs::write(&after_file, after_bytes).unwrap();

    let out = run_claw_preview_bundle(&workspace, target_rel, &after_file);
    assert_eq!(
        out.status.code(),
        Some(0),
        "generator must succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let json = parse_stdout_json(&out.stdout);
    assert_eq!(json["ok"], true);

    let bundle_path = PathBuf::from(json["preview_bundle_path"].as_str().unwrap());
    let bundle_text = fs::read_to_string(&bundle_path).unwrap();
    let bundle: serde_json::Value = serde_json::from_str(&bundle_text).unwrap();
    // before_sha256 non-empty since target existed.
    assert!(!bundle["preview_record"]["before_sha256"]
        .as_str()
        .unwrap()
        .is_empty());
    let after_sha = bundle["preview_record"]["after_sha256"].as_str().unwrap();
    assert!(!after_sha.is_empty());

    // Sanity: declared payload sha256 == preview_record.after_sha256.
    assert_eq!(json["payload_sha256"].as_str().unwrap(), after_sha);

    // Target file on disk must still equal the original before bytes.
    let on_disk = fs::read(&target_abs).unwrap();
    assert_eq!(
        on_disk, before_bytes,
        "generator must NEVER mutate the target file"
    );

    // Payload artifact contains the operator-supplied after bytes.
    let payload_path = PathBuf::from(json["payload_path"].as_str().unwrap());
    let payload_disk = fs::read(&payload_path).unwrap();
    assert_eq!(payload_disk, after_bytes);
}

// ------------------------------------------------------------------------
// Bundle is consumable by `claw plan approve` (deny=).
// ------------------------------------------------------------------------

#[test]
fn preview_bundle_output_is_consumable_by_claw_plan_approve() {
    let workspace = unique_temp_dir("approve-handshake");
    fs::create_dir_all(workspace.join("docs")).unwrap();
    let target_rel = "docs/notes.md";
    let after_dir = unique_temp_dir("approve-handshake-after");
    let after_file = after_dir.join("after.bin");
    fs::write(&after_file, b"note\n").unwrap();

    let out = run_claw_preview_bundle(&workspace, target_rel, &after_file);
    assert_eq!(out.status.code(), Some(0));
    let json = parse_stdout_json(&out.stdout);
    let bundle_path = PathBuf::from(json["preview_bundle_path"].as_str().unwrap());

    // Hand the bundle to `claw plan approve`. It does NOT need any
    // approval input to validate schema + record/display binding — those
    // checks happen at load time and a parse / binding failure exits 5
    // with `bundle_*` reason. We close stdin so approve cannot block.
    let mut child = Command::new(env!("CARGO_BIN_EXE_claw"))
        .args(["plan", "approve", &bundle_path.to_string_lossy()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("claw should launch");
    {
        // Send a single "deny" line so approve can finish without a TTY.
        // The point of this test is the bundle was parseable; we are not
        // approving anything.
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        let _ = stdin.write_all(b"deny\n");
    }
    let approve_out = child.wait_with_output().expect("approve should exit");
    let approve_stdout = String::from_utf8_lossy(&approve_out.stdout);
    let approve_stderr = String::from_utf8_lossy(&approve_out.stderr);
    // The bundle must NOT be rejected at schema / binding load: an
    // approve invocation that gets to the prompt and reads "deny" exits
    // non-zero, but with a clean approval_result envelope — NOT a
    // bundle_rejected schema/binding failure.
    assert!(
        !approve_stdout.contains("bundle-schema-version-mismatch"),
        "bundle must be schema-valid; approve stdout={approve_stdout} stderr={approve_stderr}"
    );
    assert!(
        !approve_stdout.contains("bundle-record-display-binding-mismatch"),
        "bundle binding must be intact; approve stdout={approve_stdout} stderr={approve_stderr}"
    );
    assert!(
        !approve_stdout.contains("bundle-json-parse-error"),
        "bundle must parse cleanly; approve stdout={approve_stdout} stderr={approve_stderr}"
    );
}

// ------------------------------------------------------------------------
// Scope-guard source grep: no forbidden APIs / phrases inside the
// generator's implementation block.
// ------------------------------------------------------------------------

fn read_generator_block_source() -> String {
    let main_rs = include_str!("../src/main.rs");
    // Unique section-header marker. The `CliAction::PlanPreviewBundle`
    // doc-comment and the parser docstring also mention the slice name,
    // so we match the exact section header instead.
    let start_marker =
        "// A2-L2b Slice L2b-CLI-Preview-Bundle — `claw plan preview-bundle` command";
    let end_marker = "END A2-L2b Slice L2b-CLI-Preview-Bundle";
    let start = main_rs
        .find(start_marker)
        .expect("generator block start sentinel must exist");
    let end = main_rs[start..]
        .find(end_marker)
        .expect("generator block end sentinel must exist");
    main_rs[start..start + end].to_string()
}

/// Strip line-comments + doc-comments from the generator source so the
/// scope-guard greps only see executable code. Operator docstrings
/// legitimately reference the APIs we forbid from invoking; the audit
/// only cares about real call sites.
fn read_generator_block_code_only() -> String {
    let src = read_generator_block_source();
    let mut buf = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    buf
}

#[test]
fn generator_source_does_not_invoke_run_plan_or_broker() {
    let src = read_generator_block_code_only();
    // Forbidden actual-call / wire patterns. Each token is something
    // that, if present in the code (not the comments), would mean this
    // lane wired one of the explicitly-prohibited paths.
    for forbidden in [
        "a2_plan_runner::run_plan(",
        "broker.py",
        "11434",
        "11435",
        "OPENAI_BASE_URL",
        "vram-broker",
        "Command::new",
        "execute_write(",
        "bind_after_bytes(",
        "WriteExecutionRequest",
    ] {
        assert!(
            !src.contains(forbidden),
            "generator block must not invoke `{forbidden}`; the L2b-Preview-Bundle \
             slice never wires apply / broker / subprocess APIs"
        );
    }
}

#[test]
fn generator_source_does_not_print_raw_payload() {
    let src = read_generator_block_code_only();
    for forbidden in [
        "println!(\"{}\", after_bytes",
        "println!(\"{after_bytes",
        "writeln!(stdout, \"{}\", after_bytes",
        "base64::",
        "Engine::encode",
    ] {
        assert!(
            !src.contains(forbidden),
            "generator block must not print raw payload bytes; saw `{forbidden}`"
        );
    }
}

#[test]
fn generator_source_does_not_accept_bypass_flags() {
    let src = read_generator_block_code_only();
    for forbidden in [
        "--yes",
        "--auto",
        "--allow-write",
        "--preapproved",
        "--batch",
    ] {
        assert!(
            !src.contains(forbidden),
            "generator block must not accept bypass flag `{forbidden}` \
             (the parser is in a separate function and rejects them at the CLI layer)"
        );
    }
}
