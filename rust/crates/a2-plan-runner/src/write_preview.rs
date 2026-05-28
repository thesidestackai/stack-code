//! A2-L2b run-plan write-preview artifact generator.
//!
//! Given an L2a-valid workspace-write [`PlanStep`] and a canonicalized
//! workspace root, this module:
//!
//! 1. Validates the workspace root.
//! 2. Resolves the step's `write_target` via
//!    [`crate::write_runtime::resolve_write_target`].
//! 3. Reads `after_file` bytes from disk (workspace-root-relative path,
//!    declared in the plan step). The after-bytes are the *only*
//!    authoritative source for the candidate write payload — they are
//!    NEVER inferred from model prose, diff text, or tool names.
//! 4. Reads `before` bytes from the resolved target iff it already
//!    exists (read-only — the target is never opened for write here).
//! 5. Creates a checkpoint through [`crate::CheckpointStore`].
//! 6. Builds a [`crate::PreviewRecord`] / [`crate::PreviewDisplay`] via
//!    [`crate::build_preview`].
//! 7. Writes the payload artifact (atomic) under
//!    `<workspace_root>/.claw/l2b-payloads/<run-id>/<step-id>/after.bin`
//!    plus a sibling `after.sha256` sidecar.
//! 8. Writes the preview-bundle (atomic) under
//!    `<workspace_root>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`
//!    in the same on-disk shape `claw plan approve` consumes.
//! 9. Writes a per-run manifest + status pair under
//!    `<workspace_root>/.claw/l2b-runs/<run-id>/`.
//!
//! # Hard contract
//!
//! - NEVER mutates the operator-supplied target file.
//! - NEVER calls [`crate::write_executor::execute_write`].
//! - NEVER calls [`crate::bind_after_bytes`].
//! - NEVER spawns a subprocess.
//! - NEVER reads stdin or calls the broker.
//! - NEVER follows a symlink for `after_file` or anywhere in the parent
//!   chain of the resolved target.
//! - NEVER prints raw payload bytes to stdout / stderr.
//! - All artifact writes are atomic (`tmp + rename`) under runner-owned
//!   directories. Operator targets are out of scope.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use a2_plan_schema::PlanStep;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::checkpoint::CheckpointStore;
use crate::diff_preview::{build_preview, PreviewDisplay, PreviewInputs, PreviewRecord};
use crate::write_payload::MAX_APPROVED_PAYLOAD_BYTES;
use crate::write_runtime::{resolve_write_target, WriteTargetRefusal};

/// Workspace-relative payload root, joined with `<run-id>/<step-id>/`.
/// Mirrors the layout used by the CLI's standalone `claw plan
/// preview-bundle` command so apply-bundle and apply tooling can consume
/// either source identically.
pub const PAYLOAD_ROOT_REL: &str = ".claw/l2b-payloads";

/// Workspace-relative preview-bundle root, joined with `<run-id>/<step-id>/`.
pub const PREVIEW_BUNDLE_ROOT_REL: &str = ".claw/l2b-preview-bundles";

/// Workspace-relative run manifest root, joined with `<run-id>/`. Hosts
/// `run-manifest.json` plus a `status.json` and per-step folders. Run
/// indexing is owned by this module and never bleeds into the operator's
/// target tree.
pub const RUN_ROOT_REL: &str = ".claw/l2b-runs";

/// Pinned schema version for the on-disk preview-bundle. Same value as
/// the existing CLI `claw plan preview-bundle` generator so a bundle
/// produced here round-trips into `claw plan approve` without any schema
/// widening.
pub const PREVIEW_BUNDLE_SCHEMA_V1: &str = "a2-l2b-preview-bundle.v1";

/// Pinned schema version for the run-manifest artifact.
pub const RUN_MANIFEST_SCHEMA_V1: &str = "a2-l2b-run-plan-write-preview-run-manifest.v1";

/// Pinned schema version for the run-status artifact.
pub const RUN_STATUS_SCHEMA_V1: &str = "a2-l2b-run-plan-write-preview-status.v1";

/// Pinned schema version for the per-step preview-generator-result the
/// runner writes under the per-step folder.
pub const PREVIEW_GENERATOR_RESULT_SCHEMA_V1: &str = "a2-l2b-preview-bundle-generator-result.v1";

/// On-disk preview-bundle shape produced by the runner write-preview path.
///
/// Field order + names mirror the existing CLI `PreviewBundleV1Output`
/// shape so a bundle written by either producer is consumable by
/// `claw plan approve` without schema widening.
#[derive(Debug, Serialize)]
struct PreviewBundleV1Output<'a> {
    schema_version: &'a str,
    preview_record: &'a PreviewRecord,
    preview_display: &'a PreviewDisplay,
    checkpoint_baseline_unchanged: bool,
}

/// Per-step preview-generator-result envelope, written next to the
/// per-step folder so downstream tooling can locate every artifact for
/// a single step without re-deriving paths.
///
/// Field shape mirrors the CLI's `PreviewBundleGeneratorResultV1`
/// (`is_binary` / `is_redacted` / `is_truncated` are separate booleans
/// so operators can refuse non-approvable previews without re-parsing
/// the embedded record). The struct-excessive-bools lint is allowed
/// here for the same reason.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Serialize)]
struct PreviewGeneratorResultV1 {
    schema_version: &'static str,
    ok: bool,
    run_id: String,
    step_id: String,
    preview_id: String,
    target_relative_path: String,
    preview_bundle_path: PathBuf,
    payload_path: PathBuf,
    payload_sha256_path: PathBuf,
    payload_sha256: String,
    payload_size_bytes: u64,
    checkpoint_manifest_path: PathBuf,
    is_binary: bool,
    is_redacted: bool,
    is_truncated: bool,
    audit_markers: Vec<&'static str>,
}

/// Per-run manifest: describes the operator-facing entry points for the
/// halted plan run.
#[derive(Debug, Serialize)]
struct RunManifestV1<'a> {
    schema_version: &'a str,
    run_id: &'a str,
    workspace_root: PathBuf,
    plan_name: &'a str,
    write_step_count: usize,
    pending_step_id: &'a str,
    preview_bundle_path: &'a Path,
    preview_generator_result_path: &'a Path,
    checkpoint_manifest_path: &'a Path,
    payload_path: &'a Path,
    payload_sha256: &'a str,
    status: &'a str,
    next_operator_command: &'a str,
}

/// Per-run status: a tiny pin so operators can `cat status.json` and see
/// the latest known state without re-parsing the manifest.
#[derive(Debug, Serialize)]
struct RunStatusV1<'a> {
    schema_version: &'a str,
    run_id: &'a str,
    status: &'a str,
    pending_step_id: &'a str,
    next_operator_command: &'a str,
}

/// Successfully-generated preview artifact handle. All paths are absolute
/// and live under runner-owned `.claw/l2b-*` directories — never under
/// the operator-supplied target tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePreviewArtifacts {
    pub run_id: String,
    pub pending_step_id: String,
    pub preview_id: String,
    pub workspace_root: PathBuf,
    pub target_relative_path: String,
    pub preview_bundle_path: PathBuf,
    pub preview_generator_result_path: PathBuf,
    pub checkpoint_manifest_path: PathBuf,
    pub payload_path: PathBuf,
    pub payload_sha256_path: PathBuf,
    pub payload_sha256: String,
    pub payload_size_bytes: u64,
    pub run_manifest_path: PathBuf,
    pub run_status_path: PathBuf,
    pub is_binary: bool,
    pub is_redacted: bool,
    pub is_truncated: bool,
    pub next_operator_command: String,
}

/// Why preview generation refused before producing any artifact.
///
/// Every refusal arm is structured (no embedded stack traces) so the
/// runner's report stream stays scrape-stable. All refusal arms surface
/// without mutating the operator target file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritePreviewRefusal {
    WorkspaceRootInvalid {
        message: String,
    },
    MissingAfterFileField {
        step_id: String,
    },
    MissingWriteTargetField {
        step_id: String,
    },
    AfterFileEmpty {
        step_id: String,
    },
    AfterFileAbsolute {
        step_id: String,
        path: String,
    },
    AfterFileTraversal {
        step_id: String,
        path: String,
    },
    AfterFileMissing {
        step_id: String,
        path: PathBuf,
    },
    AfterFileSymlink {
        step_id: String,
        path: PathBuf,
    },
    AfterFileNotRegular {
        step_id: String,
        path: PathBuf,
    },
    AfterFileTooLarge {
        step_id: String,
        actual: u64,
        cap: u64,
    },
    AfterFileIo {
        step_id: String,
        message: String,
    },
    TargetResolveRefused {
        step_id: String,
        marker: &'static str,
    },
    BeforeReadIo {
        step_id: String,
        message: String,
    },
    CheckpointFailed {
        step_id: String,
        message: String,
    },
    PreviewBuildFailed {
        step_id: String,
        message: String,
    },
    PayloadIo {
        step_id: String,
        message: String,
    },
    PayloadVerifyMismatch {
        step_id: String,
        expected: String,
        actual: String,
    },
    PreviewBundleIo {
        step_id: String,
        message: String,
    },
    RunArtifactIo {
        step_id: String,
        message: String,
    },
}

impl WritePreviewRefusal {
    /// Short stable discriminator for log scrapers and CLI envelopes.
    #[must_use]
    pub fn short(&self) -> &'static str {
        match self {
            Self::WorkspaceRootInvalid { .. } => "workspace-root-invalid",
            Self::MissingAfterFileField { .. } => "missing-after-file-field",
            Self::MissingWriteTargetField { .. } => "missing-write-target-field",
            Self::AfterFileEmpty { .. } => "after-file-empty",
            Self::AfterFileAbsolute { .. } => "after-file-absolute",
            Self::AfterFileTraversal { .. } => "after-file-traversal",
            Self::AfterFileMissing { .. } => "after-file-missing",
            Self::AfterFileSymlink { .. } => "after-file-symlink",
            Self::AfterFileNotRegular { .. } => "after-file-not-regular",
            Self::AfterFileTooLarge { .. } => "after-file-too-large",
            Self::AfterFileIo { .. } => "after-file-io-error",
            Self::TargetResolveRefused { .. } => "target-resolve-refused",
            Self::BeforeReadIo { .. } => "before-read-io-error",
            Self::CheckpointFailed { .. } => "checkpoint-failed",
            Self::PreviewBuildFailed { .. } => "preview-build-failed",
            Self::PayloadIo { .. } => "payload-io-error",
            Self::PayloadVerifyMismatch { .. } => "payload-verify-mismatch",
            Self::PreviewBundleIo { .. } => "preview-bundle-io-error",
            Self::RunArtifactIo { .. } => "run-artifact-io-error",
        }
    }

    /// Operator-facing reason string. Never contains raw payload bytes
    /// and never leaks internal stack traces.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::WorkspaceRootInvalid { message } => {
                format!("workspace-root-invalid: {message}")
            }
            Self::MissingAfterFileField { step_id } => {
                format!("missing-after-file-field on step {step_id}")
            }
            Self::MissingWriteTargetField { step_id } => {
                format!("missing-write-target-field on step {step_id}")
            }
            Self::AfterFileEmpty { step_id } => {
                format!("after-file-empty on step {step_id}")
            }
            Self::AfterFileAbsolute { step_id, path } => {
                format!("after-file-absolute on step {step_id}: {path}")
            }
            Self::AfterFileTraversal { step_id, path } => {
                format!("after-file-traversal on step {step_id}: {path}")
            }
            Self::AfterFileMissing { step_id, path } => {
                format!("after-file-missing on step {step_id}: {}", path.display())
            }
            Self::AfterFileSymlink { step_id, path } => {
                format!("after-file-symlink on step {step_id}: {}", path.display())
            }
            Self::AfterFileNotRegular { step_id, path } => format!(
                "after-file-not-regular on step {step_id}: {}",
                path.display()
            ),
            Self::AfterFileTooLarge {
                step_id,
                actual,
                cap,
            } => format!(
                "after-file-too-large on step {step_id}: {actual} bytes exceeds cap {cap}"
            ),
            Self::AfterFileIo { step_id, message } => {
                format!("after-file-io-error on step {step_id}: {message}")
            }
            Self::TargetResolveRefused { step_id, marker } => {
                format!("target-resolve-refused on step {step_id}: marker {marker}")
            }
            Self::BeforeReadIo { step_id, message } => {
                format!("before-read-io-error on step {step_id}: {message}")
            }
            Self::CheckpointFailed { step_id, message } => {
                format!("checkpoint-failed on step {step_id}: {message}")
            }
            Self::PreviewBuildFailed { step_id, message } => {
                format!("preview-build-failed on step {step_id}: {message}")
            }
            Self::PayloadIo { step_id, message } => {
                format!("payload-io-error on step {step_id}: {message}")
            }
            Self::PayloadVerifyMismatch {
                step_id,
                expected,
                actual,
            } => format!(
                "payload-verify-mismatch on step {step_id}: expected after_sha256={expected} actual={actual}"
            ),
            Self::PreviewBundleIo { step_id, message } => {
                format!("preview-bundle-io-error on step {step_id}: {message}")
            }
            Self::RunArtifactIo { step_id, message } => {
                format!("run-artifact-io-error on step {step_id}: {message}")
            }
        }
    }
}

/// Produce preview-only artifacts for a single workspace-write
/// [`PlanStep`].
///
/// The function NEVER mutates the operator-supplied target. The only
/// filesystem writes happen under `<workspace_root>/.claw/l2b-*`,
/// atomically via tmp + rename.
///
/// # Errors
///
/// Returns a [`WritePreviewRefusal`] for any failure arm; the runner's
/// caller surfaces it on the marker stream + structured report.
#[allow(clippy::too_many_lines)]
pub fn produce_write_preview(
    workspace_root: &Path,
    plan_name: &str,
    write_step: &PlanStep,
) -> Result<WritePreviewArtifacts, WritePreviewRefusal> {
    // 1. Canonicalize the workspace root and confirm it is a directory.
    let workspace_root_canonical =
        workspace_root
            .canonicalize()
            .map_err(|e| WritePreviewRefusal::WorkspaceRootInvalid {
                message: format!("{e}"),
            })?;
    let workspace_meta = fs::symlink_metadata(&workspace_root_canonical).map_err(|e| {
        WritePreviewRefusal::WorkspaceRootInvalid {
            message: format!("{e}"),
        }
    })?;
    if !workspace_meta.is_dir() {
        return Err(WritePreviewRefusal::WorkspaceRootInvalid {
            message: "not a directory".to_string(),
        });
    }

    // 2. Pull `after_file` + `write_target` directly from the plan step.
    // The schema validator has already lexically refused absolute paths,
    // `..` traversal, deny-component segments, and deny-glob filenames —
    // but we *re-check* the lexical invariants here as defense in depth
    // (and to map them onto runner-side refusal arms rather than the
    // L2a validator markers).
    let after_file_rel = write_step.after_file.as_deref().ok_or_else(|| {
        WritePreviewRefusal::MissingAfterFileField {
            step_id: write_step.id.clone(),
        }
    })?;
    let write_target = write_step.write_target.as_ref().ok_or_else(|| {
        WritePreviewRefusal::MissingWriteTargetField {
            step_id: write_step.id.clone(),
        }
    })?;

    // Lexical re-checks on after_file.
    if after_file_rel.is_empty() {
        return Err(WritePreviewRefusal::AfterFileEmpty {
            step_id: write_step.id.clone(),
        });
    }
    if Path::new(after_file_rel).is_absolute() {
        return Err(WritePreviewRefusal::AfterFileAbsolute {
            step_id: write_step.id.clone(),
            path: after_file_rel.to_string(),
        });
    }
    if after_file_rel.split('/').any(|c| c == "..") {
        return Err(WritePreviewRefusal::AfterFileTraversal {
            step_id: write_step.id.clone(),
            path: after_file_rel.to_string(),
        });
    }

    // 3. Build the runtime after_file absolute path and inspect it.
    let after_file_abs = workspace_root_canonical.join(after_file_rel);
    let after_size = inspect_after_file(&after_file_abs, &write_step.id)?;
    if after_size > MAX_APPROVED_PAYLOAD_BYTES {
        return Err(WritePreviewRefusal::AfterFileTooLarge {
            step_id: write_step.id.clone(),
            actual: after_size,
            cap: MAX_APPROVED_PAYLOAD_BYTES,
        });
    }

    // 4. Read after_file bytes into memory + re-check size as a TOCTOU
    //    defense.
    let after_bytes = fs::read(&after_file_abs).map_err(|e| WritePreviewRefusal::AfterFileIo {
        step_id: write_step.id.clone(),
        message: format!("{}: {e}", after_file_abs.display()),
    })?;
    let after_len_u64 = after_bytes.len() as u64;
    if after_len_u64 > MAX_APPROVED_PAYLOAD_BYTES {
        return Err(WritePreviewRefusal::AfterFileTooLarge {
            step_id: write_step.id.clone(),
            actual: after_len_u64,
            cap: MAX_APPROVED_PAYLOAD_BYTES,
        });
    }

    // 5. Resolve the write target. This is read-only filesystem syscalls
    //    only — `resolve_write_target` never opens for write.
    let resolved =
        resolve_write_target(&workspace_root_canonical, write_target).map_err(|refusal| {
            WritePreviewRefusal::TargetResolveRefused {
                step_id: write_step.id.clone(),
                marker: refusal_marker(&refusal),
            }
        })?;

    // 6. Read before_bytes if the resolved target already exists. Read-
    //    only open; the target is NEVER opened for write in this module.
    let before_bytes: Option<Vec<u8>> = if resolved.already_exists {
        Some(
            fs::read(&resolved.absolute).map_err(|e| WritePreviewRefusal::BeforeReadIo {
                step_id: write_step.id.clone(),
                message: format!("{}: {e}", resolved.absolute.display()),
            })?,
        )
    } else {
        None
    };

    // 7. Create the checkpoint. The store writes only under
    //    `<workspace_root>/.claw/l2b-checkpoints/` and never mutates the
    //    target.
    let store = CheckpointStore::new_with_generated_run_id(workspace_root_canonical.clone());
    let run_id_str = store.run_id().to_string();
    let target_relative = Path::new(&write_target.path);
    let handle = store
        .create_checkpoint(&write_step.id, &resolved.absolute, target_relative)
        .map_err(|e| WritePreviewRefusal::CheckpointFailed {
            step_id: write_step.id.clone(),
            message: format!("{e}"),
        })?;

    // 8. Build the preview record + display.
    let inputs = PreviewInputs {
        step_id: &write_step.id,
        target_relative_path: target_relative,
        target_absolute_path: &resolved.absolute,
        before: before_bytes.as_deref(),
        after: &after_bytes,
        checkpoint_run_id: store.run_id(),
        checkpoint_step_id: &write_step.id,
        created_at_utc: &now_utc_rfc3339_nanos(),
    };
    let (record, display) =
        build_preview(&inputs).map_err(|e| WritePreviewRefusal::PreviewBuildFailed {
            step_id: write_step.id.clone(),
            message: format!("{e}"),
        })?;

    // 9. Write the payload artifact (atomic) under runner-owned storage.
    let payload_dir = workspace_root_canonical
        .join(PAYLOAD_ROOT_REL)
        .join(&run_id_str)
        .join(&write_step.id);
    create_dir_0700(&payload_dir).map_err(|e| WritePreviewRefusal::PayloadIo {
        step_id: write_step.id.clone(),
        message: format!("{}: {e}", payload_dir.display()),
    })?;
    let payload_path = payload_dir.join("after.bin");
    write_file_0600_atomic(&payload_path, &after_bytes).map_err(|e| {
        WritePreviewRefusal::PayloadIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", payload_path.display()),
        }
    })?;
    let payload_sha256_path = payload_dir.join("after.sha256");
    let payload_sha256_content = format!("{}\n", record.after_sha256);
    write_file_0600_atomic(&payload_sha256_path, payload_sha256_content.as_bytes()).map_err(
        |e| WritePreviewRefusal::PayloadIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", payload_sha256_path.display()),
        },
    )?;

    // 10. Verify on-disk payload still hashes to record.after_sha256.
    let payload_redux = fs::read(&payload_path).map_err(|e| WritePreviewRefusal::PayloadIo {
        step_id: write_step.id.clone(),
        message: format!("{}: {e}", payload_path.display()),
    })?;
    let actual_sha = sha256_hex(&payload_redux);
    if actual_sha != record.after_sha256 {
        return Err(WritePreviewRefusal::PayloadVerifyMismatch {
            step_id: write_step.id.clone(),
            expected: record.after_sha256.clone(),
            actual: actual_sha,
        });
    }

    // 11. Write the preview-bundle.json (atomic). Shape mirrors
    //     PreviewBundleV1 from the CLI generator so `claw plan approve`
    //     consumes either source identically.
    let bundle_dir = workspace_root_canonical
        .join(PREVIEW_BUNDLE_ROOT_REL)
        .join(&run_id_str)
        .join(&write_step.id);
    create_dir_0700(&bundle_dir).map_err(|e| WritePreviewRefusal::PreviewBundleIo {
        step_id: write_step.id.clone(),
        message: format!("{}: {e}", bundle_dir.display()),
    })?;
    let preview_bundle_path = bundle_dir.join("preview-bundle.json");
    let bundle_out = PreviewBundleV1Output {
        schema_version: PREVIEW_BUNDLE_SCHEMA_V1,
        preview_record: &record,
        preview_display: &display,
        checkpoint_baseline_unchanged: true,
    };
    let bundle_bytes = serde_json::to_vec_pretty(&bundle_out).map_err(|e| {
        WritePreviewRefusal::PreviewBundleIo {
            step_id: write_step.id.clone(),
            message: format!("serde_json error: {e}"),
        }
    })?;
    write_file_0600_atomic(&preview_bundle_path, &bundle_bytes).map_err(|e| {
        WritePreviewRefusal::PreviewBundleIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", preview_bundle_path.display()),
        }
    })?;

    let next_operator_command = format!("claw plan approve {}", preview_bundle_path.display());

    // 12. Write per-step preview-generator-result envelope.
    let preview_generator_result_path = bundle_dir.join("preview-generator-result.json");
    let preview_gen_result = PreviewGeneratorResultV1 {
        schema_version: PREVIEW_GENERATOR_RESULT_SCHEMA_V1,
        ok: true,
        run_id: run_id_str.clone(),
        step_id: write_step.id.clone(),
        preview_id: record.preview_id.clone(),
        target_relative_path: write_target.path.clone(),
        preview_bundle_path: preview_bundle_path.clone(),
        payload_path: payload_path.clone(),
        payload_sha256_path: payload_sha256_path.clone(),
        payload_sha256: record.after_sha256.clone(),
        payload_size_bytes: after_len_u64,
        checkpoint_manifest_path: handle.manifest_path.clone(),
        is_binary: record.is_binary,
        is_redacted: record.is_redacted,
        is_truncated: record.is_truncated,
        audit_markers: vec![
            "a2-l2b-checkpoint-written",
            "a2-l2b-payload-captured",
            "a2-l2b-preview-bundle-created",
        ],
    };
    let gen_result_bytes = serde_json::to_vec_pretty(&preview_gen_result).map_err(|e| {
        WritePreviewRefusal::PreviewBundleIo {
            step_id: write_step.id.clone(),
            message: format!("serde_json error: {e}"),
        }
    })?;
    write_file_0600_atomic(&preview_generator_result_path, &gen_result_bytes).map_err(|e| {
        WritePreviewRefusal::PreviewBundleIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", preview_generator_result_path.display()),
        }
    })?;

    // 13. Write run-manifest + status (atomic). Lives under
    //     `<workspace_root>/.claw/l2b-runs/<run-id>/`.
    let run_dir = workspace_root_canonical
        .join(RUN_ROOT_REL)
        .join(&run_id_str);
    create_dir_0700(&run_dir).map_err(|e| WritePreviewRefusal::RunArtifactIo {
        step_id: write_step.id.clone(),
        message: format!("{}: {e}", run_dir.display()),
    })?;

    let run_manifest_path = run_dir.join("run-manifest.json");
    let manifest = RunManifestV1 {
        schema_version: RUN_MANIFEST_SCHEMA_V1,
        run_id: &run_id_str,
        workspace_root: workspace_root_canonical.clone(),
        plan_name,
        write_step_count: 1,
        pending_step_id: &write_step.id,
        preview_bundle_path: &preview_bundle_path,
        preview_generator_result_path: &preview_generator_result_path,
        checkpoint_manifest_path: &handle.manifest_path,
        payload_path: &payload_path,
        payload_sha256: &record.after_sha256,
        status: "write_preview_ready",
        next_operator_command: &next_operator_command,
    };
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|e| WritePreviewRefusal::RunArtifactIo {
            step_id: write_step.id.clone(),
            message: format!("serde_json error: {e}"),
        })?;
    write_file_0600_atomic(&run_manifest_path, &manifest_bytes).map_err(|e| {
        WritePreviewRefusal::RunArtifactIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", run_manifest_path.display()),
        }
    })?;

    let run_status_path = run_dir.join("status.json");
    let status = RunStatusV1 {
        schema_version: RUN_STATUS_SCHEMA_V1,
        run_id: &run_id_str,
        status: "write_preview_ready",
        pending_step_id: &write_step.id,
        next_operator_command: &next_operator_command,
    };
    let status_bytes =
        serde_json::to_vec_pretty(&status).map_err(|e| WritePreviewRefusal::RunArtifactIo {
            step_id: write_step.id.clone(),
            message: format!("serde_json error: {e}"),
        })?;
    write_file_0600_atomic(&run_status_path, &status_bytes).map_err(|e| {
        WritePreviewRefusal::RunArtifactIo {
            step_id: write_step.id.clone(),
            message: format!("{}: {e}", run_status_path.display()),
        }
    })?;

    Ok(WritePreviewArtifacts {
        run_id: run_id_str,
        pending_step_id: write_step.id.clone(),
        preview_id: record.preview_id,
        workspace_root: workspace_root_canonical,
        target_relative_path: write_target.path.clone(),
        preview_bundle_path,
        preview_generator_result_path,
        checkpoint_manifest_path: handle.manifest_path,
        payload_path,
        payload_sha256_path,
        payload_sha256: record.after_sha256,
        payload_size_bytes: after_len_u64,
        run_manifest_path,
        run_status_path,
        is_binary: record.is_binary,
        is_redacted: record.is_redacted,
        is_truncated: record.is_truncated,
        next_operator_command,
    })
}

fn refusal_marker(refusal: &WriteTargetRefusal) -> &'static str {
    refusal.marker()
}

fn inspect_after_file(path: &Path, step_id: &str) -> Result<u64, WritePreviewRefusal> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(WritePreviewRefusal::AfterFileMissing {
                step_id: step_id.to_string(),
                path: path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(WritePreviewRefusal::AfterFileIo {
                step_id: step_id.to_string(),
                message: format!("{}: {e}", path.display()),
            });
        }
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        return Err(WritePreviewRefusal::AfterFileSymlink {
            step_id: step_id.to_string(),
            path: path.to_path_buf(),
        });
    }
    if !ft.is_file() {
        return Err(WritePreviewRefusal::AfterFileNotRegular {
            step_id: step_id.to_string(),
            path: path.to_path_buf(),
        });
    }
    Ok(meta.len())
}

fn create_dir_0700(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_file_0600_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        use io::Write as _;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in &out {
        write!(hex, "{byte:02x}").expect("writing to String never fails");
    }
    hex
}

fn now_utc_rfc3339_nanos() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let days_since_epoch = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let hours = seconds_of_day / 3_600;
    let minutes = (seconds_of_day % 3_600) / 60;
    let seconds = seconds_of_day % 60;
    let (year, month, day) = civil_from_days(i64::try_from(days_since_epoch).unwrap_or(0));
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{nanos:09}Z")
}

/// Hinnant `civil_from_days`: convert days since 1970-01-01 to a
/// `(year, month, day)` Gregorian triple. Local copy so the runner
/// crate does not depend on chrono.
///
/// The casts inside are bounded by the inputs (days since UNIX epoch,
/// always within `i64` and far from `u32::MAX` for the year/month/day
/// triple), so the truncation / sign lints are deliberately allowed.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_plan_schema::{ModelTier, PlanMode, WriteTarget};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock sane")
            .as_nanos();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("a2-l2b-write-preview-{label}-{nanos}-{seq}"));
        fs::create_dir_all(&dir).expect("temp dir created");
        dir
    }

    fn write_step(target_path: &str, after_file_rel: &str) -> PlanStep {
        PlanStep {
            id: "write-it".to_string(),
            description: "write step".to_string(),
            mode: Some(PlanMode::WorkspaceWrite),
            model_tier: Some(ModelTier::Fast),
            tools: vec!["Write".to_string()],
            expected_output: None,
            write_target: Some(WriteTarget {
                path: target_path.to_string(),
                create_if_absent: true,
            }),
            expected_post_write: None,
            after_file: Some(after_file_rel.to_string()),
        }
    }

    #[test]
    fn produce_write_preview_creates_bundle_payload_and_run_manifest() {
        let ws = unique_temp_dir("happy-new-file");
        let after_dir = ws.join("materialized");
        fs::create_dir_all(&after_dir).unwrap();
        fs::write(after_dir.join("notes.after"), b"hello\nworld\n").unwrap();
        fs::create_dir_all(ws.join("notes")).unwrap();

        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let artifacts =
            produce_write_preview(&ws, "happy-plan", &step).expect("preview should succeed");

        // Bundle + payload exist and live under runner-owned dirs.
        assert!(artifacts.preview_bundle_path.exists());
        assert!(artifacts.payload_path.exists());
        assert!(artifacts.payload_sha256_path.exists());
        assert!(artifacts.checkpoint_manifest_path.exists());
        assert!(artifacts.run_manifest_path.exists());
        assert!(artifacts.run_status_path.exists());
        assert!(artifacts.preview_generator_result_path.exists());

        let ws_canonical = ws.canonicalize().unwrap();
        for p in [
            &artifacts.preview_bundle_path,
            &artifacts.payload_path,
            &artifacts.payload_sha256_path,
            &artifacts.checkpoint_manifest_path,
            &artifacts.run_manifest_path,
            &artifacts.run_status_path,
        ] {
            assert!(
                p.starts_with(&ws_canonical),
                "{} must be under canonical workspace root {}",
                p.display(),
                ws_canonical.display()
            );
        }

        // Target was NOT created (preview-only contract).
        assert!(
            !ws.join("notes/scratch.md").exists(),
            "preview must NOT mutate the target file"
        );
        // After-file was NOT mutated.
        let after_redux = fs::read(after_dir.join("notes.after")).unwrap();
        assert_eq!(after_redux, b"hello\nworld\n");

        // next_operator_command points at approve, never apply.
        assert!(artifacts
            .next_operator_command
            .starts_with("claw plan approve "));
        assert!(!artifacts.next_operator_command.contains("apply"));

        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn produce_write_preview_overwrite_path_captures_before_bytes() {
        let ws = unique_temp_dir("happy-overwrite");
        fs::create_dir_all(ws.join("notes")).unwrap();
        fs::write(ws.join("notes/scratch.md"), b"old\n").unwrap();
        let after_dir = ws.join("materialized");
        fs::create_dir_all(&after_dir).unwrap();
        fs::write(after_dir.join("notes.after"), b"new\n").unwrap();

        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let artifacts = produce_write_preview(&ws, "overwrite-plan", &step).unwrap();
        // Target file still holds the OLD bytes — preview never wrote.
        let target_redux = fs::read(ws.join("notes/scratch.md")).unwrap();
        assert_eq!(target_redux, b"old\n");
        // Payload holds the new bytes.
        let payload_redux = fs::read(&artifacts.payload_path).unwrap();
        assert_eq!(payload_redux, b"new\n");
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_missing_after_file_field() {
        let ws = unique_temp_dir("no-after-field");
        let mut step = write_step("notes/scratch.md", "materialized/notes.after");
        step.after_file = None;
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::MissingAfterFileField { .. }),
            "got {err:?}"
        );
        assert_eq!(err.short(), "missing-after-file-field");
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_missing_write_target_field() {
        let ws = unique_temp_dir("no-target-field");
        let mut step = write_step("notes/scratch.md", "materialized/notes.after");
        step.write_target = None;
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::MissingWriteTargetField { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_after_file_missing_on_disk() {
        let ws = unique_temp_dir("after-missing");
        fs::create_dir_all(ws.join("notes")).unwrap();
        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileMissing { .. }),
            "got {err:?}"
        );
        assert_eq!(err.short(), "after-file-missing");
        fs::remove_dir_all(&ws).ok();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_after_file_symlink() {
        let ws = unique_temp_dir("after-symlink");
        let after_dir = ws.join("materialized");
        fs::create_dir_all(&after_dir).unwrap();
        let real = after_dir.join("real.after");
        fs::write(&real, b"bytes\n").unwrap();
        let link = after_dir.join("notes.after");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        fs::create_dir_all(ws.join("notes")).unwrap();

        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileSymlink { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_after_file_directory() {
        let ws = unique_temp_dir("after-dir");
        let after_dir = ws.join("materialized/notes.after");
        fs::create_dir_all(&after_dir).unwrap();
        fs::create_dir_all(ws.join("notes")).unwrap();

        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileNotRegular { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_after_file_absolute_path_in_step() {
        let ws = unique_temp_dir("after-abs");
        let step = write_step("notes/scratch.md", "/etc/passwd");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileAbsolute { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_after_file_traversal_in_step() {
        let ws = unique_temp_dir("after-traversal");
        let step = write_step("notes/scratch.md", "materialized/../../escape.after");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileTraversal { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_after_file_empty_string() {
        let ws = unique_temp_dir("after-empty");
        let step = write_step("notes/scratch.md", "");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::AfterFileEmpty { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_workspace_root_not_a_directory() {
        let ws_dir = unique_temp_dir("ws-is-file");
        let ws = ws_dir.join("file");
        fs::write(&ws, b"x").unwrap();
        let step = write_step("notes/scratch.md", "materialized/notes.after");
        let err = produce_write_preview(&ws, "plan", &step).unwrap_err();
        assert!(
            matches!(err, WritePreviewRefusal::WorkspaceRootInvalid { .. }),
            "got {err:?}"
        );
        fs::remove_dir_all(&ws_dir).ok();
    }

    #[test]
    fn no_target_mutation_on_any_refusal_arm() {
        // Belt + suspenders: walk every refusal arm exercised above and
        // confirm the resolved target was NOT created. This is the
        // single most operator-critical invariant of the module.
        enum Case {
            MissingAfterField,
            MissingTargetField,
            AfterMissingOnDisk,
            AfterAbsolute,
            AfterTraversal,
        }
        let cases = [
            ("missing-after-field", Case::MissingAfterField),
            ("missing-target-field", Case::MissingTargetField),
            ("after-missing-on-disk", Case::AfterMissingOnDisk),
            ("after-absolute", Case::AfterAbsolute),
            ("after-traversal", Case::AfterTraversal),
        ];
        for (label, case) in cases {
            let ws = unique_temp_dir(&format!("no-mut-{label}"));
            fs::create_dir_all(ws.join("notes")).unwrap();
            let target = ws.join("notes/scratch.md");
            let step = match case {
                Case::MissingAfterField => {
                    let mut s = write_step("notes/scratch.md", "materialized/notes.after");
                    s.after_file = None;
                    s
                }
                Case::MissingTargetField => {
                    let mut s = write_step("notes/scratch.md", "materialized/notes.after");
                    s.write_target = None;
                    s
                }
                Case::AfterMissingOnDisk => {
                    write_step("notes/scratch.md", "materialized/notes.after")
                }
                Case::AfterAbsolute => write_step("notes/scratch.md", "/etc/passwd"),
                Case::AfterTraversal => {
                    write_step("notes/scratch.md", "materialized/../../escape.after")
                }
            };
            let _ = produce_write_preview(&ws, "plan", &step);
            assert!(
                !target.exists(),
                "{label}: target {} must not exist after refusal",
                target.display()
            );
            fs::remove_dir_all(&ws).ok();
        }
    }
}
