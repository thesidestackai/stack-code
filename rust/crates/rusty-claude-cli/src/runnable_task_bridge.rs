#![allow(
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::missing_panics_doc,
    clippy::unnecessary_wraps
)]

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use a2_plan_runner::{ApprovalContext, ApprovalDecision};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TASK_SCHEMA_V1: &str = "stack-code-runnable-task.v1";
const RESULT_SCHEMA_V1: &str = "stack-code-runnable-task-result.v1";
const RECEIPT_SCHEMA_V1: &str = "stack-code-runnable-task-receipt.v1";
const APPROVAL_RESULT_SCHEMA_V1: &str = "a2-l2b-approval-result.v1";
const DEFAULT_BROKER_URL: &str = "http://127.0.0.1:11435";
const DEFAULT_MODEL: &str = "fast";
const VALIDATION_DOCS_ONLY: &str = "docs-only";
const EXIT_REFUSED: i32 = 5;
const EXIT_APPLY_REFUSED: i32 = 7;
const EXIT_VALIDATION_FAILED: i32 = 13;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnableTaskSpec {
    schema_version: String,
    task_id: String,
    objective: String,
    worktree: PathBuf,
    allowed_paths: Vec<String>,
    #[serde(default)]
    target_path: Option<String>,
    validation_profile: String,
    caller_id: String,
    task_type: String,
    #[serde(default)]
    broker_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    operator_approval: bool,
    after_text: String,
}

#[derive(Debug, Clone)]
struct NormalizedTask {
    task_id: String,
    objective: String,
    worktree: PathBuf,
    base_sha: String,
    branch: String,
    allowed_paths: Vec<String>,
    target_path: String,
    validation_profile: String,
    caller_id: String,
    task_type: String,
    broker_url: String,
    model: String,
    after_text: String,
    receipt_dir: PathBuf,
}

#[derive(Debug)]
struct PlannerRequest {
    task_id: String,
    objective: String,
    worktree: PathBuf,
    base_sha: String,
    allowed_paths: Vec<String>,
    target_path: String,
    broker_url: String,
    model: String,
}

#[derive(Debug)]
struct PlannerCandidate {
    planner_output_json: String,
    broker_route: String,
    resolved_model: String,
    response_sha256: String,
}

trait PlannerClient {
    fn request_plan(&self, request: &PlannerRequest) -> Result<PlannerCandidate, BridgeRefusal>;
}

trait PlannerValidator {
    fn validate(
        &self,
        task: &NormalizedTask,
        candidate_json: &str,
    ) -> Result<PathBuf, BridgeRefusal>;
}

#[derive(Debug)]
struct PythonBrokerPlanner;

#[derive(Debug)]
struct ScriptPlannerValidator;

#[derive(Debug)]
struct BridgeRefusal {
    kind: &'static str,
    reason: String,
    exit_code: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBundleRead {
    schema_version: String,
    preview_record: a2_plan_runner::PreviewRecord,
    #[allow(dead_code)]
    preview_display: a2_plan_runner::PreviewDisplay,
    checkpoint_baseline_unchanged: bool,
}

pub(crate) fn run_task_command(task_spec: &Path, stdout: &mut dyn Write) -> i32 {
    let planner = PythonBrokerPlanner;
    let validator = ScriptPlannerValidator;
    match run_task_from_path(task_spec, &planner, &validator) {
        Ok(result) => {
            let _ = serde_json::to_writer_pretty(&mut *stdout, &result);
            let _ = stdout.write_all(b"\n");
            0
        }
        Err(refusal) => {
            let payload = json!({
                "schema_version": RESULT_SCHEMA_V1,
                "ok": false,
                "status": "refused",
                "refusal": refusal.kind,
                "reason": refusal.reason,
                "exit_code": refusal.exit_code,
            });
            let _ = serde_json::to_writer_pretty(&mut *stdout, &payload);
            let _ = stdout.write_all(b"\n");
            refusal.exit_code
        }
    }
}

fn run_task_from_path(
    task_spec_path: &Path,
    planner: &dyn PlannerClient,
    validator: &dyn PlannerValidator,
) -> Result<Value, BridgeRefusal> {
    let bytes = fs::read(task_spec_path).map_err(|e| {
        refusal(
            "task-spec-io-error",
            format!("could not read task spec {}: {e}", task_spec_path.display()),
        )
    })?;
    let spec: RunnableTaskSpec = serde_json::from_slice(&bytes).map_err(|e| {
        refusal(
            "task-spec-json-error",
            format!("task spec is not valid {TASK_SCHEMA_V1}: {e}"),
        )
    })?;
    run_task(spec, planner, validator)
}

fn run_task(
    spec: RunnableTaskSpec,
    planner: &dyn PlannerClient,
    validator: &dyn PlannerValidator,
) -> Result<Value, BridgeRefusal> {
    let task = normalize_task(spec)?;
    if let Some(done) = completed_task_result_if_current(&task)? {
        return Ok(done);
    }
    verify_clean_start(&task.worktree)?;
    create_dir_0700(&task.receipt_dir)?;
    write_receipt(
        &task.receipt_dir,
        "task-acceptance.json",
        &task_acceptance_receipt(&task, "accepted"),
    )?;

    let request = PlannerRequest {
        task_id: task.task_id.clone(),
        objective: task.objective.clone(),
        worktree: task.worktree.clone(),
        base_sha: task.base_sha.clone(),
        allowed_paths: task.allowed_paths.clone(),
        target_path: task.target_path.clone(),
        broker_url: task.broker_url.clone(),
        model: task.model.clone(),
    };
    let candidate = planner.request_plan(&request)?;
    write_receipt(
        &task.receipt_dir,
        "broker-plan.json",
        &json!({
            "schema_version": RECEIPT_SCHEMA_V1,
            "receipt": "broker_plan",
            "task_id": task.task_id,
            "timestamp": timestamp(),
            "status": "received",
            "worktree": task.worktree,
            "base_sha": task.base_sha,
            "caller_id": task.caller_id,
            "broker_route": candidate.broker_route,
            "resolved_model": candidate.resolved_model,
            "response_sha256": candidate.response_sha256,
        }),
    )?;

    reject_secret_like_candidate(&candidate.planner_output_json)?;
    let planner_output_path = validator.validate(&task, &candidate.planner_output_json)?;
    verify_planner_paths(&task, &candidate.planner_output_json)?;
    write_receipt(
        &task.receipt_dir,
        "planner-validation.json",
        &json!({
            "schema_version": RECEIPT_SCHEMA_V1,
            "receipt": "planner_validation",
            "task_id": task.task_id,
            "timestamp": timestamp(),
            "status": "valid",
            "worktree": task.worktree,
            "base_sha": task.base_sha,
            "caller_id": task.caller_id,
            "broker_route": task.broker_url,
            "resolved_model": task.model,
            "allowed_paths": task.allowed_paths,
            "planner_output_path": planner_output_path,
            "planner_output_sha256": sha256_hex(candidate.planner_output_json.as_bytes()),
        }),
    )?;

    let after_path = task.receipt_dir.join("after.bin");
    write_file_new(&after_path, task.after_text.as_bytes())?;
    let preview_result =
        crate::try_run_plan_preview_bundle(&task.worktree, &task.target_path, &after_path)
            .map_err(|e| {
                refusal(
                    "a2-preview-refused",
                    format!("claw plan preview-bundle refused: {}", e.reason()),
                )
            })?;
    let preview_bundle = read_preview_bundle(&preview_result.preview_bundle_path)?;
    let approval_result_path = preview_result
        .preview_bundle_path
        .parent()
        .ok_or_else(|| refusal("a2-preview-layout-invalid", "preview bundle has no parent"))?
        .join("approval-result.json");
    write_approval_result(&approval_result_path, &preview_bundle)?;
    write_receipt(
        &task.receipt_dir,
        "mutation-authorization.json",
        &json!({
            "schema_version": RECEIPT_SCHEMA_V1,
            "receipt": "mutation_authorization",
            "task_id": task.task_id,
            "timestamp": timestamp(),
            "status": "approved",
            "worktree": task.worktree,
            "base_sha": task.base_sha,
            "caller_id": task.caller_id,
            "broker_route": task.broker_url,
            "resolved_model": task.model,
            "allowed_paths": task.allowed_paths,
            "target_path": task.target_path,
            "preview_bundle_path": preview_result.preview_bundle_path,
            "approval_result_path": approval_result_path,
            "preview_sha256": preview_bundle.preview_record.preview_sha256,
        }),
    )?;

    let preview_result_path = task.receipt_dir.join("preview-generator-result.json");
    let preview_result_json = serde_json::to_value(&preview_result).map_err(|e| {
        refusal(
            "preview-result-serialize-failed",
            format!("could not serialize preview result: {e}"),
        )
    })?;
    write_json_pretty_new(&preview_result_path, &preview_result_json)?;
    let apply_bundle_path =
        run_apply_bundle_generator(&preview_result_path, &approval_result_path)?;
    let apply_result = run_apply(&apply_bundle_path)?;
    write_receipt(&task.receipt_dir, "a2-apply-result.json", &apply_result)?;
    if let Some(parent) = apply_bundle_path.parent() {
        let colocated = parent.join("apply-result.json");
        if !colocated.exists() {
            write_json_pretty_new(&colocated, &apply_result)?;
        }
    }

    let changed_paths = verify_changed_paths(&task)?;
    write_receipt(
        &task.receipt_dir,
        "changed-file-verification.json",
        &json!({
            "schema_version": RECEIPT_SCHEMA_V1,
            "receipt": "changed_file_verification",
            "task_id": task.task_id,
            "timestamp": timestamp(),
            "status": "verified",
            "worktree": task.worktree,
            "base_sha": task.base_sha,
            "caller_id": task.caller_id,
            "broker_route": task.broker_url,
            "resolved_model": task.model,
            "allowed_paths": task.allowed_paths,
            "actual_changed_paths": changed_paths,
        }),
    )?;
    run_validation(&task)?;
    write_receipt(
        &task.receipt_dir,
        "validation.json",
        &json!({
            "schema_version": RECEIPT_SCHEMA_V1,
            "receipt": "validation",
            "task_id": task.task_id,
            "timestamp": timestamp(),
            "status": "passed",
            "worktree": task.worktree,
            "base_sha": task.base_sha,
            "caller_id": task.caller_id,
            "broker_route": task.broker_url,
            "resolved_model": task.model,
            "validation_profile": task.validation_profile,
            "command": "docs-only content guard + git diff --check",
        }),
    )?;

    let approved_lane_path = write_approved_lane(&task)?;
    let package_handoff = json!({
        "schema_version": RECEIPT_SCHEMA_V1,
        "receipt": "package_handoff_readiness",
        "task_id": task.task_id,
        "timestamp": timestamp(),
        "status": "ready",
        "worktree": task.worktree,
        "base_sha": task.base_sha,
        "caller_id": task.caller_id,
        "broker_route": task.broker_url,
        "resolved_model": task.model,
        "allowed_paths": task.allowed_paths,
        "actual_changed_paths": changed_paths,
        "approved_lane_path": approved_lane_path,
        "package_rungs": [
            "scripts/a2-tier3-write-orchestrator.sh package-plan",
            "scripts/a2-tier3-write-orchestrator.sh package-commit",
            "scripts/a2-tier3-write-orchestrator.sh package-push",
            "scripts/a2-tier3-write-orchestrator.sh package-pr"
        ],
    });
    write_receipt(
        &task.receipt_dir,
        "package-handoff-readiness.json",
        &package_handoff,
    )?;
    let complete = final_success_result(&task, &changed_paths, &approved_lane_path);
    write_receipt(&task.receipt_dir, "task-complete.json", &complete)?;
    Ok(complete)
}

fn normalize_task(spec: RunnableTaskSpec) -> Result<NormalizedTask, BridgeRefusal> {
    if spec.schema_version != TASK_SCHEMA_V1 {
        return Err(refusal(
            "task-spec-schema-version",
            format!("unsupported schema_version {}", spec.schema_version),
        ));
    }
    validate_task_id(&spec.task_id)?;
    if spec.objective.trim().is_empty() {
        return Err(refusal(
            "task-objective-empty",
            "objective must not be empty",
        ));
    }
    if !spec.operator_approval {
        return Err(BridgeRefusal {
            kind: "a2-authorization-invalid",
            reason: "operator_approval must be true before mutation evidence is generated"
                .to_string(),
            exit_code: EXIT_APPLY_REFUSED,
        });
    }
    if spec.validation_profile != VALIDATION_DOCS_ONLY {
        return Err(refusal(
            "validation-profile-unsupported",
            format!(
                "unsupported validation_profile {}; only {VALIDATION_DOCS_ONLY} is allowed",
                spec.validation_profile
            ),
        ));
    }
    if spec.task_type != "code" {
        return Err(refusal("task-type-unsupported", "task_type must be code"));
    }
    let worktree = canonical_worktree(&spec.worktree)?;
    let base_sha = git_stdout(&worktree, &["rev-parse", "HEAD"])?;
    let branch = git_stdout(&worktree, &["branch", "--show-current"])?;
    if branch.is_empty() {
        return Err(refusal(
            "worktree-branch-detached",
            "task worktree must be on a named branch",
        ));
    }
    let allowed_paths = normalize_allowed_paths(&worktree, &spec.allowed_paths)?;
    let target_path = match spec.target_path {
        Some(path) => normalize_one_allowed_path(&worktree, &path)?,
        None if allowed_paths.len() == 1 => allowed_paths[0].clone(),
        None => {
            return Err(refusal(
                "target-path-required",
                "target_path is required when allowed_paths contains more than one path",
            ));
        }
    };
    if !allowed_paths.iter().any(|path| path == &target_path) {
        return Err(refusal(
            "target-path-not-allowed",
            format!("target_path {target_path} is not in allowed_paths"),
        ));
    }
    let broker_url = canonical_broker_url(spec.broker_url.as_deref())?;
    let model = spec.model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    if model.trim().is_empty() {
        return Err(refusal("model-empty", "model must not be empty"));
    }
    let receipt_dir = worktree
        .join(".claw")
        .join("runnable-task-bridge")
        .join(&spec.task_id);
    Ok(NormalizedTask {
        task_id: spec.task_id,
        objective: spec.objective,
        worktree,
        base_sha,
        branch,
        allowed_paths,
        target_path,
        validation_profile: spec.validation_profile,
        caller_id: spec.caller_id,
        task_type: spec.task_type,
        broker_url,
        model,
        after_text: spec.after_text,
        receipt_dir,
    })
}

impl PlannerClient for PythonBrokerPlanner {
    fn request_plan(&self, request: &PlannerRequest) -> Result<PlannerCandidate, BridgeRefusal> {
        let context_path = write_context_summary(request)?;
        let composed_task = format!(
            "{}\n\nAllowed paths: {}\nTarget path: {}\nReturn candidate_files containing only the allowed target path.",
            request.objective.trim(),
            request.allowed_paths.join(", "),
            request.target_path
        );
        let output = Command::new("python3")
            .current_dir(&request.worktree)
            .arg("scripts/invoke_planner_model_via_broker.py")
            .arg("--task")
            .arg(&composed_task)
            .arg("--context-summary")
            .arg(&context_path)
            .arg("--broker-url")
            .arg(&request.broker_url)
            .arg("--model")
            .arg(&request.model)
            .arg("--allow-live-broker-call")
            .arg("--json")
            .output()
            .map_err(|e| refusal("broker-adapter-launch-failed", format!("{e}")))?;
        if !output.status.success() {
            return Err(refusal(
                "broker-adapter-refused",
                format!(
                    "planner adapter exited {:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        let wrapper: Value = serde_json::from_slice(&output.stdout).map_err(|e| {
            refusal(
                "broker-adapter-json-error",
                format!("planner adapter stdout was not valid JSON: {e}"),
            )
        })?;
        let content = wrapper
            .get("response")
            .and_then(|response| response.get("choices"))
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                refusal(
                    "broker-response-content-missing",
                    "broker response did not contain choices[0].message.content",
                )
            })?
            .trim()
            .to_string();
        Ok(PlannerCandidate {
            response_sha256: sha256_hex(&output.stdout),
            planner_output_json: content,
            broker_route: request.broker_url.clone(),
            resolved_model: request.model.clone(),
        })
    }
}

impl PlannerValidator for ScriptPlannerValidator {
    fn validate(
        &self,
        task: &NormalizedTask,
        candidate_json: &str,
    ) -> Result<PathBuf, BridgeRefusal> {
        let candidate_path = task.receipt_dir.join("planner-output.json");
        write_file_new(&candidate_path, candidate_json.as_bytes())?;
        let output = Command::new("python3")
            .current_dir(&task.worktree)
            .arg("scripts/validate_planner_output_schema.py")
            .arg(&candidate_path)
            .output()
            .map_err(|e| refusal("planner-validator-launch-failed", format!("{e}")))?;
        if output.status.success() {
            Ok(candidate_path)
        } else {
            Err(refusal(
                "planner-output-validation-refused",
                format!(
                    "validator exited {:?}: {}{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }
}

fn verify_planner_paths(task: &NormalizedTask, candidate_json: &str) -> Result<(), BridgeRefusal> {
    let doc: Value = serde_json::from_str(candidate_json).map_err(|e| {
        refusal(
            "planner-output-json-error",
            format!("planner output was not valid JSON after validator pass: {e}"),
        )
    })?;
    let candidates = doc
        .get("candidate_files")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            refusal(
                "planner-output-candidate-files-missing",
                "planner output must name candidate_files for the bridge target",
            )
        })?;
    let allowed: BTreeSet<&str> = task.allowed_paths.iter().map(String::as_str).collect();
    let mut saw_target = false;
    for value in candidates {
        let Some(path) = value.as_str() else {
            return Err(refusal(
                "planner-output-candidate-path-invalid",
                "candidate_files entries must be strings",
            ));
        };
        if !allowed.contains(path) {
            return Err(refusal(
                "planner-output-unauthorized-path",
                format!("planner output named unauthorized candidate file {path}"),
            ));
        }
        if path == task.target_path {
            saw_target = true;
        }
    }
    if !saw_target {
        return Err(refusal(
            "planner-output-target-missing",
            format!(
                "planner output did not name target_path {}",
                task.target_path
            ),
        ));
    }
    Ok(())
}

fn read_preview_bundle(path: &Path) -> Result<PreviewBundleRead, BridgeRefusal> {
    let bytes = fs::read(path).map_err(|e| {
        refusal(
            "preview-bundle-read-error",
            format!("could not read preview bundle {}: {e}", path.display()),
        )
    })?;
    let bundle: PreviewBundleRead = serde_json::from_slice(&bytes).map_err(|e| {
        refusal(
            "preview-bundle-json-error",
            format!("preview bundle was not valid JSON: {e}"),
        )
    })?;
    if bundle.schema_version != "a2-l2b-preview-bundle.v1" {
        return Err(refusal(
            "preview-bundle-schema-version",
            format!("unexpected preview bundle schema {}", bundle.schema_version),
        ));
    }
    Ok(bundle)
}

fn write_approval_result(path: &Path, bundle: &PreviewBundleRead) -> Result<(), BridgeRefusal> {
    let line = format!(
        "apply {} {}\n",
        bundle.preview_record.step_id, bundle.preview_record.preview_sha256
    );
    let decision = a2_plan_runner::evaluate_approval(
        &line,
        ApprovalContext {
            preview: &bundle.preview_record,
            checkpoint_baseline_unchanged: bundle.checkpoint_baseline_unchanged,
        },
    );
    let ApprovalDecision::Approved { .. } = decision else {
        return Err(BridgeRefusal {
            kind: "a2-approval-refused",
            reason: "operator-owned bridge approval did not bind to preview".to_string(),
            exit_code: EXIT_APPLY_REFUSED,
        });
    };
    let payload = json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": "approved",
        "preview_id": bundle.preview_record.preview_id,
        "step_id": bundle.preview_record.step_id,
        "preview_sha256": bundle.preview_record.preview_sha256,
        "checkpoint_baseline_unchanged": bundle.checkpoint_baseline_unchanged,
        "exit_code_hint": 0,
        "audit_markers": [
            "a2-l2b-approved",
            "stack-code-runnable-task-bridge-operator-authorized"
        ],
    });
    write_json_pretty_new(path, &payload)
}

fn run_apply_bundle_generator(
    preview_result_path: &Path,
    approval_result_path: &Path,
) -> Result<PathBuf, BridgeRefusal> {
    let mut stdout = Vec::new();
    let code = crate::run_plan_apply_bundle(preview_result_path, approval_result_path, &mut stdout);
    let value: Value = serde_json::from_slice(&stdout).map_err(|e| {
        refusal(
            "a2-apply-bundle-output-json-error",
            format!("apply-bundle output was not JSON: {e}"),
        )
    })?;
    if code != 0 {
        return Err(refusal(
            "a2-apply-bundle-refused",
            format!("apply-bundle generator exited {code}: {value}"),
        ));
    }
    value
        .get("apply_bundle_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            refusal(
                "a2-apply-bundle-path-missing",
                "apply-bundle success output lacked apply_bundle_path",
            )
        })
}

fn run_apply(apply_bundle_path: &Path) -> Result<Value, BridgeRefusal> {
    let mut stdout = Vec::new();
    let code = crate::run_plan_apply(apply_bundle_path, &mut stdout);
    let value: Value = serde_json::from_slice(&stdout).map_err(|e| {
        refusal(
            "a2-apply-output-json-error",
            format!("apply output was not JSON: {e}"),
        )
    })?;
    if code != 0 {
        return Err(BridgeRefusal {
            kind: "a2-apply-refused",
            reason: format!("claw plan apply exited {code}: {value}"),
            exit_code: code,
        });
    }
    if value.get("outcome").and_then(Value::as_str) != Some("applied") {
        return Err(refusal(
            "a2-apply-not-applied",
            format!("apply output did not report applied: {value}"),
        ));
    }
    Ok(value)
}

fn run_validation(task: &NormalizedTask) -> Result<(), BridgeRefusal> {
    if task.validation_profile != VALIDATION_DOCS_ONLY {
        return Err(refusal(
            "validation-profile-unsupported",
            "only docs-only validation is implemented",
        ));
    }
    validate_docs_only_target(task)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&task.worktree)
        .arg("diff")
        .arg("--check")
        .output()
        .map_err(|e| refusal("validation-launch-failed", format!("{e}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(BridgeRefusal {
            kind: "validation-failed",
            reason: format!(
                "git diff --check failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            exit_code: EXIT_VALIDATION_FAILED,
        })
    }
}

fn validate_docs_only_target(task: &NormalizedTask) -> Result<(), BridgeRefusal> {
    if !Path::new(&task.target_path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(BridgeRefusal {
            kind: "validation-failed",
            reason: format!(
                "docs-only validation requires a Markdown target; got {}",
                task.target_path
            ),
            exit_code: EXIT_VALIDATION_FAILED,
        });
    }
    let target = task.worktree.join(&task.target_path);
    let bytes = fs::read(&target).map_err(|e| BridgeRefusal {
        kind: "validation-failed",
        reason: format!("could not read validation target {}: {e}", target.display()),
        exit_code: EXIT_VALIDATION_FAILED,
    })?;
    let text = String::from_utf8(bytes).map_err(|e| BridgeRefusal {
        kind: "validation-failed",
        reason: format!("validation target is not UTF-8: {e}"),
        exit_code: EXIT_VALIDATION_FAILED,
    })?;
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        if line.starts_with("<<<<<<<") || line.starts_with("=======") || line.starts_with(">>>>>>>")
        {
            return Err(BridgeRefusal {
                kind: "validation-failed",
                reason: format!("conflict marker in {}:{line_no}", task.target_path),
                exit_code: EXIT_VALIDATION_FAILED,
            });
        }
        if line.ends_with(' ') || line.ends_with('\t') {
            return Err(BridgeRefusal {
                kind: "validation-failed",
                reason: format!("trailing whitespace in {}:{line_no}", task.target_path),
                exit_code: EXIT_VALIDATION_FAILED,
            });
        }
    }
    Ok(())
}

fn write_approved_lane(task: &NormalizedTask) -> Result<PathBuf, BridgeRefusal> {
    let target_abs = task.worktree.join(&task.target_path);
    let path = task.receipt_dir.join("approved-lane.json");
    let payload = json!({
        "schema_version": "stack-code-runnable-task-approved-lane.v1",
        "operatorApproved": true,
        "worktreePlan": {
            "worktreePath": task.worktree,
            "branch": task.branch,
            "base": "origin/main",
        },
        "declaredPaths": [target_abs],
        "proposedWrites": [target_abs],
        "proposedCommands": [],
        "taskId": task.task_id,
        "callerId": task.caller_id,
        "taskType": task.task_type,
    });
    write_json_pretty_new(&path, &payload)?;
    Ok(path)
}

fn final_success_result(
    task: &NormalizedTask,
    changed_paths: &[String],
    approved_lane: &Path,
) -> Value {
    json!({
        "schema_version": RESULT_SCHEMA_V1,
        "ok": true,
        "status": "applied",
        "task_id": task.task_id,
        "timestamp": timestamp(),
        "worktree": task.worktree,
        "base_sha": task.base_sha,
        "caller_id": task.caller_id,
        "broker_route": task.broker_url,
        "resolved_model": task.model,
        "allowed_paths": task.allowed_paths,
        "actual_changed_paths": changed_paths,
        "receipt_dir": task.receipt_dir,
        "approved_lane_path": approved_lane,
        "package_ready": true,
        "manual_git_packaging_required": false,
    })
}

fn completed_task_result_if_current(task: &NormalizedTask) -> Result<Option<Value>, BridgeRefusal> {
    let complete_path = task.receipt_dir.join("task-complete.json");
    if !complete_path.exists() {
        return Ok(None);
    }
    let target = task.worktree.join(&task.target_path);
    if !target.is_file() {
        return Err(refusal(
            "completed-task-target-missing",
            "task-complete receipt exists but target file is missing",
        ));
    }
    let expected = sha256_hex(task.after_text.as_bytes());
    let actual = sha256_file_hex(&target).map_err(|e| {
        refusal(
            "completed-task-target-hash-error",
            format!("could not hash completed target {}: {e}", target.display()),
        )
    })?;
    if expected != actual {
        return Err(refusal(
            "completed-task-target-mismatch",
            "task-complete receipt exists but target bytes differ from requested after_text",
        ));
    }
    let bytes = fs::read(&complete_path).map_err(|e| {
        refusal(
            "completed-task-receipt-read-error",
            format!("could not read {}: {e}", complete_path.display()),
        )
    })?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|e| {
        refusal(
            "completed-task-receipt-json-error",
            format!("completed receipt was not valid JSON: {e}"),
        )
    })?;
    value["status"] = Value::String("idempotent_complete".to_string());
    Ok(Some(value))
}

fn task_acceptance_receipt(task: &NormalizedTask, status: &str) -> Value {
    json!({
        "schema_version": RECEIPT_SCHEMA_V1,
        "receipt": "task_acceptance",
        "task_id": task.task_id,
        "timestamp": timestamp(),
        "status": status,
        "worktree": task.worktree,
        "base_sha": task.base_sha,
        "caller_id": task.caller_id,
        "task_type": task.task_type,
        "broker_route": task.broker_url,
        "resolved_model": task.model,
        "allowed_paths": task.allowed_paths,
        "target_path": task.target_path,
        "validation_profile": task.validation_profile,
        "after_sha256": sha256_hex(task.after_text.as_bytes()),
    })
}

fn verify_clean_start(worktree: &Path) -> Result<(), BridgeRefusal> {
    let status = git_stdout(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        Err(refusal(
            "worktree-dirty",
            format!("worktree must be clean before task run; git status: {status}"),
        ))
    }
}

fn verify_changed_paths(task: &NormalizedTask) -> Result<Vec<String>, BridgeRefusal> {
    let status = git_stdout(
        &task.worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    let mut changed = Vec::new();
    for line in status.lines().filter(|line| !line.trim().is_empty()) {
        let path = porcelain_path(line);
        if path == ".claw" || path.starts_with(".claw/") {
            continue;
        }
        changed.push(path.to_string());
    }
    changed.sort();
    changed.dedup();
    if changed == [task.target_path.clone()] {
        Ok(changed)
    } else {
        Err(refusal(
            "changed-paths-mismatch",
            format!(
                "actual changed paths {:?} did not equal target {:?}",
                changed, task.target_path
            ),
        ))
    }
}

fn porcelain_path(line: &str) -> &str {
    if line.len() >= 3 {
        &line[3..]
    } else {
        line
    }
}

fn normalize_allowed_paths(
    worktree: &Path,
    paths: &[String],
) -> Result<Vec<String>, BridgeRefusal> {
    if paths.is_empty() {
        return Err(refusal(
            "allowed-paths-empty",
            "allowed_paths must not be empty",
        ));
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for path in paths {
        let normalized = normalize_one_allowed_path(worktree, path)?;
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    Ok(out)
}

fn normalize_one_allowed_path(worktree: &Path, path: &str) -> Result<String, BridgeRefusal> {
    if path.trim().is_empty() {
        return Err(refusal("path-empty", "allowed path must not be empty"));
    }
    if path.contains(char::is_whitespace) {
        return Err(refusal(
            "path-whitespace-unsupported",
            format!("path contains whitespace: {path}"),
        ));
    }
    let rel = Path::new(path);
    if rel.is_absolute() {
        return Err(refusal(
            "path-absolute-refused",
            format!("path must be repository-relative: {path}"),
        ));
    }
    if rel
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(refusal(
            "path-traversal-refused",
            format!("path must not contain traversal: {path}"),
        ));
    }
    if rel.components().any(|component| {
        matches!(
            component,
            Component::RootDir | Component::Prefix(_) | Component::CurDir
        )
    }) {
        return Err(refusal(
            "path-component-refused",
            format!("path contains an unsupported component: {path}"),
        ));
    }
    let target = worktree.join(rel);
    let Some(parent) = target.parent() else {
        return Err(refusal("path-parent-missing", "target has no parent"));
    };
    let parent_canonical = parent.canonicalize().map_err(|e| {
        refusal(
            "path-parent-unavailable",
            format!("parent for {path} must exist inside the worktree: {e}"),
        )
    })?;
    if !parent_canonical.starts_with(worktree) {
        return Err(refusal(
            "path-parent-escape",
            format!("parent for {path} escapes the worktree"),
        ));
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err(refusal(
                "path-symlink-refused",
                format!("target path is a symlink: {path}"),
            ));
        }
    }
    Ok(path.to_string())
}

fn canonical_worktree(path: &Path) -> Result<PathBuf, BridgeRefusal> {
    if !path.is_absolute() {
        return Err(refusal(
            "worktree-not-absolute",
            "worktree must be an absolute path",
        ));
    }
    let canonical = path.canonicalize().map_err(|e| {
        refusal(
            "worktree-canonicalize-failed",
            format!("could not canonicalize worktree {}: {e}", path.display()),
        )
    })?;
    let inside = Command::new("git")
        .arg("-C")
        .arg(&canonical)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .map_err(|e| refusal("git-launch-failed", format!("{e}")))?;
    if inside.status.success() && String::from_utf8_lossy(&inside.stdout).trim() == "true" {
        Ok(canonical)
    } else {
        Err(refusal(
            "worktree-not-git",
            format!("{} is not a git worktree", canonical.display()),
        ))
    }
}

fn canonical_broker_url(input: Option<&str>) -> Result<String, BridgeRefusal> {
    let raw = input.unwrap_or(DEFAULT_BROKER_URL).trim();
    let forbidden = ["114", "34"].concat();
    if raw.contains(&forbidden) {
        return Err(refusal(
            "broker-route-raw-upstream-refused",
            "broker_url must route through the broker, not the raw upstream port",
        ));
    }
    if raw == DEFAULT_BROKER_URL || raw == "http://localhost:11435" {
        return Ok(DEFAULT_BROKER_URL.to_string());
    }
    Err(refusal(
        "broker-route-unsupported",
        format!("broker_url must be {DEFAULT_BROKER_URL}"),
    ))
}

fn validate_task_id(task_id: &str) -> Result<(), BridgeRefusal> {
    if task_id.is_empty() || task_id.len() > 128 || matches!(task_id, "." | "..") {
        return Err(refusal(
            "task-id-invalid",
            "task_id must be 1..128 chars and not . or ..",
        ));
    }
    if task_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        Ok(())
    } else {
        Err(refusal(
            "task-id-invalid",
            "task_id must contain only ASCII alphanumeric, underscore, dash, or dot",
        ))
    }
}

fn reject_secret_like_candidate(candidate: &str) -> Result<(), BridgeRefusal> {
    let forbidden = ["114", "34"].concat();
    if candidate.contains(&forbidden) {
        return Err(refusal(
            "planner-output-raw-upstream-refused",
            "planner output referenced the raw upstream port",
        ));
    }
    if candidate.contains("-----BEGIN ") && candidate.contains("KEY-----") {
        return Err(refusal(
            "planner-output-secret-like-refused",
            "planner output contained key-block shaped text",
        ));
    }
    for marker in ["sk-", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if candidate.contains(marker) {
            return Err(refusal(
                "planner-output-secret-like-refused",
                "planner output contained token-shaped text",
            ));
        }
    }
    Ok(())
}

fn write_context_summary(request: &PlannerRequest) -> Result<PathBuf, BridgeRefusal> {
    let path = std::env::temp_dir().join(format!(
        "stack-code-task-bridge-context-{}-{}.json",
        std::process::id(),
        request.task_id
    ));
    let payload = json!({
        "schema_version": "stack-code-runnable-task-context.v1",
        "task_id": request.task_id,
        "workspace_root": request.worktree,
        "base_sha": request.base_sha,
        "allowed_paths": request.allowed_paths,
        "target_path": request.target_path,
        "operator_after_bytes_owned": true,
        "planner_authority": "advisory_only",
    });
    write_json_pretty_new(&path, &payload)?;
    Ok(path)
}

fn git_stdout(worktree: &Path, args: &[&str]) -> Result<String, BridgeRefusal> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(args)
        .output()
        .map_err(|e| refusal("git-launch-failed", format!("{e}")))?;
    if !output.status.success() {
        return Err(refusal(
            "git-command-failed",
            format!(
                "git -C {} {} failed: {}{}",
                worktree.display(),
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn create_dir_0700(path: &Path) -> Result<(), BridgeRefusal> {
    fs::create_dir_all(path).map_err(|e| {
        refusal(
            "receipt-dir-create-failed",
            format!("could not create {}: {e}", path.display()),
        )
    })
}

fn write_receipt(dir: &Path, name: &str, value: &Value) -> Result<(), BridgeRefusal> {
    write_json_pretty_new(&dir.join(name), value)
}

fn write_json_pretty_new(path: &Path, value: &Value) -> Result<(), BridgeRefusal> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        refusal(
            "receipt-json-serialize-failed",
            format!("could not serialize receipt: {e}"),
        )
    })?;
    write_file_new(path, &bytes)
}

fn write_file_new(path: &Path, bytes: &[u8]) -> Result<(), BridgeRefusal> {
    let Some(parent) = path.parent() else {
        return Err(refusal("write-path-parent-missing", "path has no parent"));
    };
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|e| {
            refusal(
                "write-path-parent-create-failed",
                format!("could not create {}: {e}", parent.display()),
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| {
            refusal(
                "receipt-write-failed",
                format!("could not create {}: {e}", path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|e| {
        refusal(
            "receipt-write-failed",
            format!("could not write {}: {e}", path.display()),
        )
    })?;
    file.flush().map_err(|e| {
        refusal(
            "receipt-write-failed",
            format!("could not flush {}: {e}", path.display()),
        )
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn sha256_file_hex(path: &Path) -> io::Result<String> {
    fs::read(path).map(|bytes| sha256_hex(&bytes))
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix-{}.{:09}Z", now.as_secs(), now.subsec_nanos())
}

fn refusal(kind: &'static str, reason: impl Into<String>) -> BridgeRefusal {
    BridgeRefusal {
        kind,
        reason: reason.into(),
        exit_code: EXIT_REFUSED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug)]
    struct StubPlanner {
        output: String,
    }

    #[derive(Debug)]
    struct InlineValidator;

    impl PlannerClient for StubPlanner {
        fn request_plan(
            &self,
            request: &PlannerRequest,
        ) -> Result<PlannerCandidate, BridgeRefusal> {
            assert_eq!(request.broker_url, DEFAULT_BROKER_URL);
            Ok(PlannerCandidate {
                planner_output_json: self.output.clone(),
                broker_route: request.broker_url.clone(),
                resolved_model: request.model.clone(),
                response_sha256: sha256_hex(self.output.as_bytes()),
            })
        }
    }

    impl PlannerValidator for InlineValidator {
        fn validate(
            &self,
            task: &NormalizedTask,
            candidate_json: &str,
        ) -> Result<PathBuf, BridgeRefusal> {
            let _: Value = serde_json::from_str(candidate_json)
                .map_err(|e| refusal("planner-output-validation-refused", format!("{e}")))?;
            let path = task.receipt_dir.join("planner-output.json");
            write_file_new(&path, candidate_json.as_bytes())?;
            Ok(path)
        }
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock sane")
            .as_nanos();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "stack-code-task-bridge-{label}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir.canonicalize().expect("canonicalize temp dir")
    }

    fn git(worktree: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(worktree)
            .args(args)
            .output()
            .expect("git launches");
        assert!(
            output.status.success(),
            "git {:?} failed: stdout={} stderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_repo(label: &str) -> PathBuf {
        git_repo_at(unique_temp_dir(label))
    }

    fn git_repo_at(dir: PathBuf) -> PathBuf {
        git(&dir, &["init", "-b", "cert"]);
        git(&dir, &["config", "user.email", "test@example.invalid"]);
        git(&dir, &["config", "user.name", "Stack Code Test"]);
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("README.md"), "baseline\n").unwrap();
        git(&dir, &["add", "README.md"]);
        git(&dir, &["commit", "-m", "baseline"]);
        dir
    }

    fn valid_planner_output(path: &str) -> String {
        serde_json::to_string(&json!({
            "schema_version": "a2-l4-planner-output.v1",
            "task_id": "cert-task",
            "workspace_root": ".",
            "task_summary": "create certification document",
            "plan_steps": [{"step_id": "step-1", "description": "write the allowed file"}],
            "risk_notes": [],
            "operator_next_steps": ["review A2 evidence"],
            "candidate_files": [path],
            "patch_intent": {"summary": "create file", "notes": ["operator after_text supplies bytes"]}
        }))
        .unwrap()
    }

    fn spec(worktree: &Path, allowed: Vec<&str>, target: Option<&str>) -> RunnableTaskSpec {
        RunnableTaskSpec {
            schema_version: TASK_SCHEMA_V1.to_string(),
            task_id: "cert-task".to_string(),
            objective: "Create a certification document".to_string(),
            worktree: worktree.to_path_buf(),
            allowed_paths: allowed.into_iter().map(str::to_string).collect(),
            target_path: target.map(str::to_string),
            validation_profile: VALIDATION_DOCS_ONLY.to_string(),
            caller_id: "stack-code".to_string(),
            task_type: "code".to_string(),
            broker_url: None,
            model: None,
            operator_approval: true,
            after_text: "# Purpose\n\nCertification.\n".to_string(),
        }
    }

    #[test]
    fn valid_single_file_mutation_applies_and_emits_package_handoff() {
        let repo = git_repo("valid");
        let planner = StubPlanner {
            output: valid_planner_output("docs/cert.md"),
        };
        let result = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &planner,
            &InlineValidator,
        )
        .expect("task succeeds");
        assert_eq!(result["ok"], true);
        assert_eq!(result["actual_changed_paths"], json!(["docs/cert.md"]));
        assert!(repo.join("docs/cert.md").is_file());
        assert!(result["approved_lane_path"]
            .as_str()
            .unwrap()
            .ends_with("approved-lane.json"));
    }

    #[test]
    fn approved_lane_is_accepted_by_tier4_package_plan() {
        let disposable_root = unique_temp_dir("package-plan-root");
        let repo_dir = disposable_root.join("package-plan-worktree");
        fs::create_dir_all(&repo_dir).expect("create disposable worktree fixture");
        let repo = git_repo_at(repo_dir.canonicalize().expect("canonicalize fixture"));
        let planner = StubPlanner {
            output: valid_planner_output("docs/cert.md"),
        };
        let result = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &planner,
            &InlineValidator,
        )
        .expect("task succeeds");
        let lane = PathBuf::from(result["approved_lane_path"].as_str().unwrap());
        let control = git_repo("package-plan-control");
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = crate_dir
            .ancestors()
            .nth(3)
            .expect("crate dir is under repo root");
        let script = repo_root.join("scripts/a2-tier3-write-orchestrator.sh");
        let output = Command::new("bash")
            .env("A2_CONTROL_CHECKOUT", &control)
            .env("A2_DISPOSABLE_WORKTREE_ROOT", &disposable_root)
            .arg(script)
            .arg("package-plan")
            .arg("--worktree")
            .arg(&repo)
            .arg("--approved-lane")
            .arg(&lane)
            .output()
            .expect("package-plan launches");
        assert!(
            output.status.success(),
            "package-plan refused bridge lane: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("\"schema\": \"a2-tier4-package-plan.v0\""));
        assert!(stdout.contains("\"would_open_pr\": false"));
        assert!(stdout.contains("\"would_push\": false"));
    }

    #[test]
    fn multiple_allowed_files_are_accepted_when_target_is_explicit() {
        let repo = git_repo("multiple-allowed");
        let planner = StubPlanner {
            output: valid_planner_output("docs/cert.md"),
        };
        let result = run_task(
            spec(
                &repo,
                vec!["docs/cert.md", "docs/notes.md"],
                Some("docs/cert.md"),
            ),
            &planner,
            &InlineValidator,
        )
        .expect("task succeeds");
        assert_eq!(result["actual_changed_paths"], json!(["docs/cert.md"]));
    }

    #[test]
    fn unauthorized_candidate_file_is_rejected_before_apply() {
        let repo = git_repo("unauthorized");
        let planner = StubPlanner {
            output: valid_planner_output("docs/other.md"),
        };
        let err = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &planner,
            &InlineValidator,
        )
        .expect_err("unauthorized candidate must fail");
        assert_eq!(err.kind, "planner-output-unauthorized-path");
        assert!(!repo.join("docs/cert.md").exists());
    }

    #[test]
    fn absolute_path_is_rejected() {
        let repo = git_repo("absolute");
        let err = run_task(
            spec(&repo, vec!["/tmp/out.md"], None),
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("absolute path must fail");
        assert_eq!(err.kind, "path-absolute-refused");
    }

    #[test]
    fn traversal_path_is_rejected() {
        let repo = git_repo("traversal");
        let err = run_task(
            spec(&repo, vec!["../out.md"], None),
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("traversal must fail");
        assert_eq!(err.kind, "path-traversal-refused");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let repo = git_repo("symlink");
        let outside = unique_temp_dir("outside");
        symlink(outside.join("cert.md"), repo.join("docs/cert.md")).unwrap();
        let err = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("symlink target must fail");
        assert_eq!(err.kind, "path-symlink-refused");
    }

    #[test]
    fn dirty_starting_worktree_is_rejected() {
        let repo = git_repo("dirty");
        fs::write(repo.join("docs/dirty.md"), "dirty\n").unwrap();
        let err = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("dirty worktree must fail");
        assert_eq!(err.kind, "worktree-dirty");
    }

    #[test]
    fn malformed_planner_output_is_rejected() {
        let repo = git_repo("malformed");
        let err = run_task(
            spec(&repo, vec!["docs/cert.md"], None),
            &StubPlanner {
                output: "{not json".to_string(),
            },
            &InlineValidator,
        )
        .expect_err("malformed planner output must fail");
        assert_eq!(err.kind, "planner-output-validation-refused");
        assert!(!repo.join("docs/cert.md").exists());
    }

    #[test]
    fn raw_upstream_broker_route_is_rejected() {
        let repo = git_repo("broker-route");
        let mut task = spec(&repo, vec!["docs/cert.md"], None);
        task.broker_url = Some("http://127.0.0.1:11434".to_string());
        let err = run_task(
            task,
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("raw upstream route must fail");
        assert_eq!(err.kind, "broker-route-raw-upstream-refused");
    }

    #[test]
    fn missing_operator_approval_is_rejected() {
        let repo = git_repo("approval");
        let mut task = spec(&repo, vec!["docs/cert.md"], None);
        task.operator_approval = false;
        let err = run_task(
            task,
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("missing approval must fail");
        assert_eq!(err.kind, "a2-authorization-invalid");
    }

    #[test]
    fn validation_failure_blocks_package_readiness() {
        let repo = git_repo("validation");
        let mut task = spec(&repo, vec!["docs/cert.md"], None);
        task.after_text = "# Purpose\n\n<<<<<<< HEAD\n".to_string();
        let err = run_task(
            task,
            &StubPlanner {
                output: valid_planner_output("docs/cert.md"),
            },
            &InlineValidator,
        )
        .expect_err("diff check must fail");
        assert_eq!(err.kind, "validation-failed");
    }

    #[test]
    fn completed_task_id_is_idempotent() {
        let repo = git_repo("idempotent");
        let planner = StubPlanner {
            output: valid_planner_output("docs/cert.md"),
        };
        let task = spec(&repo, vec!["docs/cert.md"], None);
        let first = run_task(task, &planner, &InlineValidator).expect("first run succeeds");
        assert_eq!(first["status"], "applied");
        let second_task = spec(&repo, vec!["docs/cert.md"], None);
        let second =
            run_task(second_task, &planner, &InlineValidator).expect("second run succeeds");
        assert_eq!(second["status"], "idempotent_complete");
    }
}
