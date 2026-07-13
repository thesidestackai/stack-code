#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::unneeded_struct_pattern,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]
mod init;
mod input;
mod render;

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::net::TcpListener;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, UNIX_EPOCH};

use api::{
    detect_provider_kind, resolve_startup_auth_source, AnthropicClient, AuthSource,
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, MessageResponse,
    OutputContentBlock, PromptCache, ProviderClient as ApiProviderClient, ProviderKind,
    StreamEvent as ApiStreamEvent, ToolChoice, ToolDefinition, ToolResultContentBlock,
};

use commands::{
    classify_skills_slash_command, handle_agents_slash_command, handle_agents_slash_command_json,
    handle_mcp_slash_command, handle_mcp_slash_command_json, handle_plugins_slash_command,
    handle_skills_slash_command, handle_skills_slash_command_json, render_slash_command_help,
    render_slash_command_help_filtered, resolve_skill_invocation, resume_supported_slash_commands,
    slash_command_specs, validate_slash_command_input, SkillSlashDispatch, SlashCommand,
};
use compat_harness::{extract_manifest, UpstreamPaths};
use init::initialize_repo;
use plugins::{PluginHooks, PluginManager, PluginManagerConfig, PluginRegistry};
use render::{MarkdownStreamState, Spinner, TerminalRenderer};
use runtime::mcp_tool_name;
use runtime::{
    check_base_commit, format_stale_base_warning, format_usd, load_oauth_credentials,
    load_system_prompt, pricing_for_model, resolve_expected_base, resolve_sandbox_status,
    ApiClient, ApiRequest, AssistantEvent, CompactionConfig, ConfigLoader, ConfigSource,
    ContentBlock, ConversationMessage, ConversationRuntime, McpServer, McpServerManager,
    McpServerSpec, McpTool, MessageRole, ModelPricing, PermissionMode, PermissionPolicy,
    ProjectContext, PromptCacheEvent, ResolvedPermissionMode, RuntimeError, Session, TokenUsage,
    ToolError, ToolExecutor, UsageTracker,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tools::{
    execute_tool, mvp_tool_specs, GlobalToolRegistry, RuntimeToolDefinition, ToolSearchOutput,
};

const DEFAULT_MODEL: &str = "claude-opus-4-6";

/// #148: Model provenance for `claw status` JSON/text output. Records where
/// the resolved model string came from so claws don't have to re-read argv
/// to audit whether their `--model` flag was honored vs falling back to env
/// or config or default.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelSource {
    /// Explicit `--model` / `--model=` CLI flag.
    Flag,
    /// ANTHROPIC_MODEL environment variable (when no flag was passed).
    Env,
    /// `model` key in `.claw.json` / `.claw/settings.json` (when neither
    /// flag nor env set it).
    Config,
    /// Compiled-in DEFAULT_MODEL fallback.
    Default,
}

impl ModelSource {
    fn as_str(&self) -> &'static str {
        match self {
            ModelSource::Flag => "flag",
            ModelSource::Env => "env",
            ModelSource::Config => "config",
            ModelSource::Default => "default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelProvenance {
    /// Resolved model string (after alias expansion).
    resolved: String,
    /// Raw user input before alias resolution. None when source is Default.
    raw: Option<String>,
    /// Where the resolved model string originated.
    source: ModelSource,
}

impl ModelProvenance {
    fn default_fallback() -> Self {
        Self {
            resolved: DEFAULT_MODEL.to_string(),
            raw: None,
            source: ModelSource::Default,
        }
    }

    fn from_flag(raw: &str) -> Self {
        Self {
            resolved: resolve_model_alias_with_config(raw),
            raw: Some(raw.to_string()),
            source: ModelSource::Flag,
        }
    }

    fn from_env_or_config_or_default(cli_model: &str) -> Self {
        // Only called when no --model flag was passed. Probe env first,
        // then config, else fall back to default. Mirrors the logic in
        // resolve_repl_model() but captures the source.
        if cli_model != DEFAULT_MODEL {
            // Already resolved from some prior path; treat as flag.
            return Self {
                resolved: cli_model.to_string(),
                raw: Some(cli_model.to_string()),
                source: ModelSource::Flag,
            };
        }
        if let Some(env_model) = env::var("ANTHROPIC_MODEL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Self {
                resolved: resolve_model_alias_with_config(&env_model),
                raw: Some(env_model),
                source: ModelSource::Env,
            };
        }
        if let Some(config_model) = config_model_for_current_dir() {
            return Self {
                resolved: resolve_model_alias_with_config(&config_model),
                raw: Some(config_model),
                source: ModelSource::Config,
            };
        }
        Self::default_fallback()
    }
}

fn max_tokens_for_model(model: &str) -> u32 {
    if model.contains("opus") {
        32_000
    } else {
        64_000
    }
}
// Build-time constants injected by build.rs (fall back to static values when
// build.rs hasn't run, e.g. in doc-test or unusual toolchain environments).
const DEFAULT_DATE: &str = match option_env!("BUILD_DATE") {
    Some(d) => d,
    None => "unknown",
};
const DEFAULT_OAUTH_CALLBACK_PORT: u16 = 4545;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TARGET: Option<&str> = option_env!("TARGET");
const GIT_SHA: Option<&str> = option_env!("GIT_SHA");
const INTERNAL_PROGRESS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);
const POST_TOOL_STALL_TIMEOUT: Duration = Duration::from_secs(10);
const PRIMARY_SESSION_EXTENSION: &str = "jsonl";
const LEGACY_SESSION_EXTENSION: &str = "json";
const OFFICIAL_REPO_URL: &str = "https://github.com/ultraworkers/claw-code";
const OFFICIAL_REPO_SLUG: &str = "ultraworkers/claw-code";
const DEPRECATED_INSTALL_COMMAND: &str = "cargo install claw-code";
const LATEST_SESSION_REFERENCE: &str = "latest";
const SESSION_REFERENCE_ALIASES: &[&str] = &[LATEST_SESSION_REFERENCE, "last", "recent"];
const CLI_OPTION_SUGGESTIONS: &[&str] = &[
    "--help",
    "-h",
    "--version",
    "-V",
    "--model",
    "--output-format",
    "--permission-mode",
    "--dangerously-skip-permissions",
    "--allowedTools",
    "--allowed-tools",
    "--resume",
    "--acp",
    "-acp",
    "--print",
    "--compact",
    "--base-commit",
    "-p",
];

type AllowedToolSet = BTreeSet<String>;
type RuntimePluginStateBuildOutput = (
    Option<Arc<Mutex<RuntimeMcpState>>>,
    Vec<RuntimeToolDefinition>,
);

fn main() {
    if let Err(error) = run() {
        let message = error.to_string();
        // When --output-format json is active, emit errors as JSON so downstream
        // tools can parse failures the same way they parse successes (ROADMAP #42).
        let argv: Vec<String> = std::env::args().collect();
        let json_output = argv
            .windows(2)
            .any(|w| w[0] == "--output-format" && w[1] == "json")
            || argv.iter().any(|a| a == "--output-format=json");
        if json_output {
            // #77: classify error by prefix so downstream claws can route without
            // regex-scraping the prose. Split short-reason from hint-runbook.
            let kind = classify_error_kind(&message);
            let (short_reason, hint) = split_error_hint(&message);
            eprintln!(
                "{}",
                serde_json::json!({
                    "type": "error",
                    "error": short_reason,
                    "kind": kind,
                    "hint": hint,
                })
            );
        } else {
            // #156: Add machine-readable error kind to text output so stderr observers
            // don't need to regex-scrape the prose.
            let kind = classify_error_kind(&message);
            if message.contains("`claw --help`") {
                eprintln!(
                    "[error-kind: {kind}]
error: {message}"
                );
            } else {
                eprintln!(
                    "[error-kind: {kind}]
error: {message}

Run `claw --help` for usage."
                );
            }
        }
        std::process::exit(1);
    }
}

/// #77: Classify a stringified error message into a machine-readable kind.
///
/// Returns a snake_case token that downstream consumers can switch on instead
/// of regex-scraping the prose. The classification is best-effort prefix/keyword
/// matching against the error messages produced throughout the CLI surface.
fn classify_error_kind(message: &str) -> &'static str {
    // Check specific patterns first (more specific before generic)
    if message.contains("missing Anthropic credentials") {
        "missing_credentials"
    } else if message.contains("Manifest source files are missing") {
        "missing_manifests"
    } else if message.contains("no worker state file found") {
        "missing_worker_state"
    } else if message.contains("session not found") {
        "session_not_found"
    } else if message.contains("failed to restore session") {
        "session_load_failed"
    } else if message.contains("no managed sessions found") {
        "no_managed_sessions"
    } else if message.contains("unrecognized argument") || message.contains("unknown option") {
        "cli_parse"
    } else if message.contains("invalid model syntax") {
        "invalid_model_syntax"
    } else if message.contains("is not yet implemented") {
        "unsupported_command"
    } else if message.contains("unsupported resumed command") {
        "unsupported_resumed_command"
    } else if message.contains("confirmation required") {
        "confirmation_required"
    } else if message.contains("api failed") || message.contains("api returned") {
        "api_http_error"
    } else {
        "unknown"
    }
}

/// #77: Split a multi-line error message into (short_reason, optional_hint).
///
/// The short_reason is the first line (up to the first newline), and the hint
/// is the remaining text or `None` if there's no newline. This prevents the
/// runbook prose from being stuffed into the `error` field that downstream
/// parsers expect to be the short reason alone.
fn split_error_hint(message: &str) -> (String, Option<String>) {
    match message.split_once('\n') {
        Some((short, hint)) => (short.to_string(), Some(hint.trim().to_string())),
        None => (message.to_string(), None),
    }
}

/// Read piped stdin content when stdin is not a terminal.
///
/// Returns `None` when stdin is attached to a terminal (interactive REPL use),
/// when reading fails, or when the piped content is empty after trimming.
/// Returns `Some(raw_content)` when a pipe delivered non-empty content.
fn read_piped_stdin() -> Option<String> {
    if io::stdin().is_terminal() {
        return None;
    }
    let mut buffer = String::new();
    if io::stdin().read_to_string(&mut buffer).is_err() {
        return None;
    }
    if buffer.trim().is_empty() {
        return None;
    }
    Some(buffer)
}

/// Merge a piped stdin payload into a prompt argument.
///
/// When `stdin_content` is `None` or empty after trimming, the prompt is
/// returned unchanged. Otherwise the trimmed stdin content is appended to the
/// prompt separated by a blank line so the model sees the prompt first and the
/// piped context immediately after it.
fn merge_prompt_with_stdin(prompt: &str, stdin_content: Option<&str>) -> String {
    let Some(raw) = stdin_content else {
        return prompt.to_string();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return prompt.to_string();
    }
    if prompt.is_empty() {
        return trimmed.to_string();
    }
    format!("{prompt}\n\n{trimmed}")
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args)? {
        CliAction::DumpManifests {
            output_format,
            manifests_dir,
        } => dump_manifests(manifests_dir.as_deref(), output_format)?,
        CliAction::BootstrapPlan { output_format } => print_bootstrap_plan(output_format)?,
        CliAction::Agents {
            args,
            output_format,
        } => LiveCli::print_agents(args.as_deref(), output_format)?,
        CliAction::Mcp {
            args,
            output_format,
        } => LiveCli::print_mcp(args.as_deref(), output_format)?,
        CliAction::Skills {
            args,
            output_format,
        } => LiveCli::print_skills(args.as_deref(), output_format)?,
        CliAction::Plugins {
            action,
            target,
            output_format,
        } => LiveCli::print_plugins(action.as_deref(), target.as_deref(), output_format)?,
        CliAction::PrintSystemPrompt {
            cwd,
            date,
            output_format,
        } => print_system_prompt(cwd, date, output_format)?,
        CliAction::Version { output_format } => print_version(output_format)?,
        CliAction::ResumeSession {
            session_path,
            commands,
            output_format,
        } => resume_session(&session_path, &commands, output_format),
        CliAction::Status {
            model,
            model_flag_raw,
            permission_mode,
            output_format,
        } => print_status_snapshot(
            &model,
            model_flag_raw.as_deref(),
            permission_mode,
            output_format,
        )?,
        CliAction::Sandbox { output_format } => print_sandbox_status_snapshot(output_format)?,
        CliAction::Prompt {
            prompt,
            model,
            output_format,
            allowed_tools,
            permission_mode,
            compact,
            base_commit,
            reasoning_effort,
            allow_broad_cwd,
        } => {
            enforce_broad_cwd_policy(allow_broad_cwd, output_format)?;
            run_stale_base_preflight(base_commit.as_deref());
            // Only consume piped stdin as prompt context when the permission
            // mode is fully unattended. In modes where the permission
            // prompter may invoke CliPermissionPrompter::decide(), stdin
            // must remain available for interactive approval; otherwise the
            // prompter's read_line() would hit EOF and deny every request.
            let stdin_context = if matches!(permission_mode, PermissionMode::DangerFullAccess) {
                read_piped_stdin()
            } else {
                None
            };
            let effective_prompt = merge_prompt_with_stdin(&prompt, stdin_context.as_deref());
            let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode)?;
            cli.set_reasoning_effort(reasoning_effort);
            cli.run_turn_with_output(&effective_prompt, output_format, compact)?;
        }
        CliAction::Doctor { output_format } => run_doctor(output_format)?,
        CliAction::Acp { output_format } => print_acp_status(output_format)?,
        CliAction::State { output_format } => run_worker_state(output_format)?,
        CliAction::Init { output_format } => run_init(output_format)?,
        // #146: dispatch pure-local introspection. Text mode uses existing
        // render_config_report/render_diff_report; JSON mode uses the
        // corresponding _json helpers already exposed for resume sessions.
        CliAction::Config {
            section,
            output_format,
        } => match output_format {
            CliOutputFormat::Text => {
                println!("{}", render_config_report(section.as_deref())?);
            }
            CliOutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&render_config_json(section.as_deref())?)?
                );
            }
        },
        CliAction::Diff { output_format } => match output_format {
            CliOutputFormat::Text => {
                println!("{}", render_diff_report()?);
            }
            CliOutputFormat::Json => {
                let cwd = env::current_dir()?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&render_diff_json_for(&cwd)?)?
                );
            }
        },
        CliAction::Export {
            session_reference,
            output_path,
            output_format,
        } => run_export(&session_reference, output_path.as_deref(), output_format)?,
        CliAction::Repl {
            model,
            allowed_tools,
            permission_mode,
            base_commit,
            reasoning_effort,
            allow_broad_cwd,
        } => run_repl(
            model,
            allowed_tools,
            permission_mode,
            base_commit,
            reasoning_effort,
            allow_broad_cwd,
        )?,
        CliAction::HelpTopic(topic) => print_help_topic(topic),
        CliAction::Help { output_format } => print_help(output_format)?,
        // A2-L1b: additive dispatch. Entire body lives in
        // `run_plan_subcommand`; the only thing this arm does is hand off.
        CliAction::Plan {
            file,
            dry_run,
            report_format,
            substrate_url,
            fast_model,
            wrapper,
            step_timeout,
            workspace_write_preview,
            workspace_root,
        } => {
            let code = run_plan_subcommand(
                &file,
                dry_run,
                report_format,
                substrate_url.as_deref(),
                fast_model.as_deref(),
                wrapper.as_deref(),
                step_timeout,
                workspace_write_preview,
                workspace_root.as_deref(),
            );
            // Exit codes 0–5 form the operator-facing contract for `claw
            // plan run`. With --workspace-write-preview the CLI may also
            // emit 7 (preview-ready, halted pre-apply). Bypass the
            // surrounding `Ok(())` flow because the exit code must
            // surface verbatim.
            std::process::exit(code);
        }
        CliAction::PlanApprove {
            bundle_path,
            approval_result_output,
        } => {
            // A2-L2b Slice 3d: read bundle, render prompt to stderr,
            // read approval line from stdin, emit approval-result JSON
            // on stdout, exit with operator-facing code (0/5/7). The
            // command never writes target files; cf. `run_plan_approve`.
            // Option C: when --approval-result-output <path> is given,
            // also persist the emitted approval-result JSON to that path
            // (only on a successful approval); cf. `run_plan_approve_with_output`.
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let stderr = std::io::stderr();
            let stdin_is_tty = stdin.is_terminal();
            let mut stdin_lock = stdin.lock();
            let mut stdout_lock = stdout.lock();
            let mut stderr_lock = stderr.lock();
            let code = run_plan_approve_with_output(
                &bundle_path,
                approval_result_output.as_deref(),
                stdin_is_tty,
                &mut stdin_lock,
                &mut stdout_lock,
                &mut stderr_lock,
            );
            std::process::exit(code);
        }
        CliAction::PlanApply { bundle_path } => {
            // A2-L2b Slice L2b-CLI-Apply: read apply bundle, validate
            // authority chain, resolve target, invoke library executor,
            // emit apply-result JSON on stdout. The command never reads
            // stdin, never spawns subprocesses, never calls the broker.
            // cf. `run_plan_apply`.
            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let code = run_plan_apply(&bundle_path, &mut stdout_lock);
            std::process::exit(code);
        }
        CliAction::PlanPreviewBundle {
            workspace_root,
            target_relative_path,
            after_file,
        } => {
            // A2-L2b Slice L2b-CLI-Preview-Bundle: resolve target via
            // Slice-1, capture Slice-2 checkpoint, copy after-file bytes
            // into runner-owned payload storage, build Slice-3a preview,
            // write preview-bundle.json. NEVER mutates target. NEVER
            // approves. NEVER applies. cf. `run_plan_preview_bundle`.
            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let code = run_plan_preview_bundle(
                &workspace_root,
                &target_relative_path,
                &after_file,
                &mut stdout_lock,
            );
            std::process::exit(code);
        }
        CliAction::PlanApplyBundle {
            preview_result_path,
            approval_result_path,
        } => {
            // A2-L2b Slice L2b-CLI-Apply-Bundle-Generator: read the
            // preview-generator result + the approval-result, validate
            // the full authority chain, and write an apply-bundle.json
            // artifact consumable by `claw plan apply`. NEVER executes
            // apply. NEVER mutates the target file. NEVER calls
            // `execute_write` or `bind_after_bytes`. cf.
            // `run_plan_apply_bundle`.
            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let code = run_plan_apply_bundle(
                &preview_result_path,
                &approval_result_path,
                &mut stdout_lock,
            );
            std::process::exit(code);
        }
        CliAction::PlanStatus {
            workspace_root,
            approval_result_path,
        } => {
            // A2-L2d Read-Only Artifact Inspector: aggregate state from
            // existing `<workspace>/.claw/l2b-*` artifacts and emit an
            // a2-l2d-status.v1 envelope. NEVER mutates state. NEVER
            // calls the broker, model, or Ollama. cf. `run_plan_status`.
            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let code = run_plan_status(
                &workspace_root,
                approval_result_path.as_deref(),
                &mut stdout_lock,
            );
            std::process::exit(code);
        }
    }
    Ok(())
}

/// A2-L1b dispatcher. Wraps the `a2-plan-runner` crate exclusively — does
/// NOT construct claw args, does NOT call the broker directly, does NOT
/// spawn any subprocess of its own. Returns the CLI exit code (0–7).
///
/// When `workspace_write_preview` is `true`, the dispatcher routes to
/// [`a2_plan_runner::run_plan_with_write_preview`] which permits exactly
/// one workspace-write step to produce preview-only artifacts and halt
/// before approval. Without the flag, the existing L1b read-only-only
/// path is unchanged.
#[allow(clippy::too_many_arguments)]
fn run_plan_subcommand(
    file: &Path,
    dry_run: bool,
    report_format: PlanReportFormat,
    substrate_url: Option<&str>,
    fast_model: Option<&str>,
    wrapper: Option<&Path>,
    step_timeout: Option<std::time::Duration>,
    workspace_write_preview: bool,
    workspace_root: Option<&Path>,
) -> i32 {
    use a2_plan_runner::{
        exit_code_for, refused_precheck_report, substrate_unavailable_report, write_json,
        write_markers, DEFAULT_STEP_TIMEOUT, EXIT_PARSE_ERROR,
    };
    use a2_plan_schema::{parse_plan, validate_plan};

    // Operator-supplied --step-timeout overrides DEFAULT_STEP_TIMEOUT; both
    // are bounded by `a2_plan_runner::parse_step_timeout_seconds`.
    let effective_step_timeout = step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT);

    // Default substrate: the canonical SideStackAI broker on :11435 with
    // qwen3:14b as the FAST model. Operator can override per invocation.
    const DEFAULT_SUBSTRATE_URL: &str = "http://127.0.0.1:11435/v1";
    const DEFAULT_FAST_MODEL: &str = "qwen3:14b";
    // Wrapper default: relative to CWD. Operator runs `claw plan run` from
    // the repo root; passing `--wrapper` overrides for non-standard layouts.
    let default_wrapper = PathBuf::from("scripts/claw-sidestack-local");

    // -- 1. YAML parse (exit 5 on failure; surfaces BEFORE any runner code).
    let yaml = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("claw plan run: cannot read {}: {}", file.display(), e);
            return EXIT_PARSE_ERROR;
        }
    };
    let plan = match parse_plan(&yaml) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "claw plan run: yaml parse error in {}: {}",
                file.display(),
                e
            );
            return EXIT_PARSE_ERROR;
        }
    };

    // -- 2a. L2b write-preview branch. Live-only (dry-run + preview is a
    //         contradiction: the artifacts are the deliverable). Producing
    //         the preview requires real filesystem reads.
    if workspace_write_preview {
        if dry_run {
            eprintln!(
                "claw plan run: --dry-run is not supported with --workspace-write-preview. \
                 The preview artifacts are the deliverable; a dry run would have nothing to emit."
            );
            return EXIT_PARSE_ERROR;
        }
        // NOTE: no wrapper-existence preflight here. The runner's
        // write-preview path only spawns the wrapper for read-only steps
        // BEFORE the lone workspace-write step. For a plan whose sole
        // step is the workspace-write step, the wrapper is irrelevant
        // and a missing-wrapper preflight would incorrectly refuse a
        // valid request. When read-only steps DO exist, the runner
        // surfaces missing-wrapper via its normal failure path
        // (`StepFailure::SpawnError`).
        let wrapper_path = wrapper.unwrap_or(&default_wrapper);
        // Workspace root resolution: operator may pass --workspace-root;
        // otherwise CWD is used. The runner canonicalizes either before
        // touching the filesystem.
        let workspace_root_owned: PathBuf = match workspace_root {
            Some(p) => p.to_path_buf(),
            None => match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("claw plan run: cannot read CWD as workspace root: {e}");
                    return a2_plan_runner::EXIT_RUN_PLAN_WRITE_PREVIEW_REFUSED;
                }
            },
        };
        let substrate = Some((
            substrate_url.unwrap_or(DEFAULT_SUBSTRATE_URL),
            fast_model.unwrap_or(DEFAULT_FAST_MODEL),
        ));
        let report = a2_plan_runner::run_plan_with_write_preview(
            &plan,
            wrapper_path,
            substrate,
            effective_step_timeout,
            &workspace_root_owned,
        );
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        let write_result = match report_format {
            PlanReportFormat::Markers => write_write_preview_markers(&report, &mut handle),
            PlanReportFormat::Json => write_write_preview_json(&report, &mut handle),
        };
        if let Err(e) = write_result {
            eprintln!("claw plan run: failed to write report: {e}");
            return 1;
        }
        return report.exit_code_hint;
    }

    // -- 2b. Build the PlanReport. Two paths depending on --dry-run.
    let report = if dry_run {
        // Dry-run: validator + precheck only. No subprocess, no substrate
        // probe, no live broker. Exits 0/2/3 only.
        let validator_report = validate_plan(&plan);
        match a2_plan_runner::preflight::precheck(&plan, &validator_report) {
            Ok(()) => {
                // L1a-valid + tool-allowlist-clean → dry-run treats as PASS.
                // Build an empty-step pass report so the operator gets the
                // standard marker stream.
                use a2_plan_runner::runner::aggregate_plan_report;
                aggregate_plan_report(&plan.name, Vec::new())
            }
            Err(refusal) => refused_precheck_report(&plan.name, &refusal),
        }
    } else {
        // Live path: validator → precheck → substrate probe → per-step
        // run_step (via build_claw_command + wrapper subprocess). All of
        // this is owned by a2_plan_runner::run_plan — the CLI never
        // touches ClawCommand construction or the subprocess boundary.
        let wrapper_path = wrapper.unwrap_or(&default_wrapper);
        let substrate = Some((
            substrate_url.unwrap_or(DEFAULT_SUBSTRATE_URL),
            fast_model.unwrap_or(DEFAULT_FAST_MODEL),
        ));
        // Resolve workspace root for step executor CWD. Operator may pass
        // --workspace-root; otherwise fall back to CWD. Mirrors the
        // resolution in the --workspace-write-preview branch above.
        let workspace_root_live: PathBuf = match workspace_root {
            Some(p) => p.to_path_buf(),
            None => match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("claw plan run: cannot read CWD as workspace root: {e}");
                    return EXIT_PARSE_ERROR;
                }
            },
        };
        // Pre-check wrapper existence so we emit a clean
        // substrate-unavailable instead of letting Command::new spawn fail
        // with an obscure errno message.
        if !wrapper_path.exists() {
            eprintln!(
                "claw plan run: wrapper not found at {}. Pass --wrapper PATH or run from repo root.",
                wrapper_path.display()
            );
            substrate_unavailable_report(&plan.name)
        } else {
            a2_plan_runner::run_plan(
                &plan,
                wrapper_path,
                substrate,
                effective_step_timeout,
                &workspace_root_live,
            )
        }
    };

    // -- 3. Emit the report in the requested format. Stdout only — never
    // writes to disk.
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let write_result = match report_format {
        PlanReportFormat::Markers => write_markers(&report, &mut handle),
        PlanReportFormat::Json => write_json(&report, &mut handle),
    };
    if let Err(e) = write_result {
        eprintln!("claw plan run: failed to write report: {e}");
        return 1;
    }

    exit_code_for(&report)
}

/// L2b write-preview report writer (markers). Emits the
/// runner-supplied marker stream followed by an operator-facing summary
/// of the artifact paths (only on `WritePreviewReady`). Pure stdout — no
/// file writes here; the artifacts are already on disk under runner-owned
/// `.claw/l2b-*` directories.
fn write_write_preview_markers<W: std::io::Write>(
    report: &a2_plan_runner::WritePreviewRunReport,
    mut writer: W,
) -> std::io::Result<()> {
    use std::io::Write as _;
    writeln!(writer, "# plan: {}", report.plan_name)?;
    writeln!(
        writer,
        "# write_preview_status: {}",
        write_preview_status_label(&report.status)
    )?;
    for marker in &report.markers {
        writeln!(writer, "{marker}")?;
    }
    for sr in &report.step_reports {
        writeln!(writer, "# step: {}", sr.step_id)?;
        for marker in &sr.markers {
            writeln!(writer, "{marker}")?;
        }
    }
    if let Some(artifacts) = &report.preview_artifacts {
        writeln!(writer, "# run_id: {}", artifacts.run_id)?;
        writeln!(writer, "# pending_step_id: {}", artifacts.pending_step_id)?;
        writeln!(
            writer,
            "# preview_bundle_path: {}",
            artifacts.preview_bundle_path.display()
        )?;
        writeln!(
            writer,
            "# preview_generator_result_path: {}",
            artifacts.preview_generator_result_path.display()
        )?;
        writeln!(
            writer,
            "# checkpoint_manifest_path: {}",
            artifacts.checkpoint_manifest_path.display()
        )?;
        writeln!(
            writer,
            "# payload_path: {}",
            artifacts.payload_path.display()
        )?;
        writeln!(writer, "# payload_sha256: {}", artifacts.payload_sha256)?;
        writeln!(
            writer,
            "# run_manifest_path: {}",
            artifacts.run_manifest_path.display()
        )?;
        writeln!(
            writer,
            "# next_operator_command: {}",
            artifacts.next_operator_command
        )?;
    }
    if let Some(refusal) = &report.refusal {
        writeln!(
            writer,
            "# refusal: {}",
            write_preview_refusal_label(refusal)
        )?;
    }
    Ok(())
}

fn write_write_preview_json<W: std::io::Write>(
    report: &a2_plan_runner::WritePreviewRunReport,
    mut writer: W,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let steps: Vec<_> = report
        .step_reports
        .iter()
        .map(|sr| {
            let outcome = match &sr.outcome {
                Ok(()) => "passed",
                Err(_) => "failed",
            };
            serde_json::json!({
                "step_id": sr.step_id,
                "outcome": outcome,
                "markers": sr.markers,
            })
        })
        .collect();
    let artifacts_json = report.preview_artifacts.as_ref().map(|a| {
        serde_json::json!({
            "run_id": a.run_id,
            "pending_step_id": a.pending_step_id,
            "preview_id": a.preview_id,
            "workspace_root": a.workspace_root,
            "target_relative_path": a.target_relative_path,
            "preview_bundle_path": a.preview_bundle_path,
            "preview_generator_result_path": a.preview_generator_result_path,
            "checkpoint_manifest_path": a.checkpoint_manifest_path,
            "payload_path": a.payload_path,
            "payload_sha256_path": a.payload_sha256_path,
            "payload_sha256": a.payload_sha256,
            "payload_size_bytes": a.payload_size_bytes,
            "run_manifest_path": a.run_manifest_path,
            "run_status_path": a.run_status_path,
            "is_binary": a.is_binary,
            "is_redacted": a.is_redacted,
            "is_truncated": a.is_truncated,
            "next_operator_command": a.next_operator_command,
        })
    });
    let refusal_json = report
        .refusal
        .as_ref()
        .map(|r| serde_json::json!({"label": write_preview_refusal_label(r)}));
    let doc = serde_json::json!({
        "schema_version": "a2-l2b-run-plan-write-preview-report.v1",
        "plan_name": report.plan_name,
        "status": write_preview_status_label(&report.status),
        "write_step_count": report.write_step_count,
        "markers": report.markers,
        "steps": steps,
        "preview_artifacts": artifacts_json,
        "refusal": refusal_json,
        "exit_code_hint": report.exit_code_hint,
        "next_operator_command": report.next_operator_command,
    });
    writeln!(writer, "{doc}")
}

fn write_preview_status_label(status: &a2_plan_runner::WritePreviewPlanStatus) -> &'static str {
    use a2_plan_runner::WritePreviewPlanStatus;
    match status {
        WritePreviewPlanStatus::ReadOnlyComplete => "read_only_complete",
        WritePreviewPlanStatus::WritePreviewReady => "write_preview_ready",
        WritePreviewPlanStatus::Refused => "refused",
        WritePreviewPlanStatus::ReadOnlyFailedBeforeWrite => "read_only_failed_before_write",
    }
}

fn write_preview_refusal_label(
    refusal: &a2_plan_runner::runner::WritePreviewPlanRefusal,
) -> String {
    use a2_plan_runner::runner::WritePreviewPlanRefusal;
    match refusal {
        WritePreviewPlanRefusal::ValidatorRefused => "validator-refused".to_string(),
        WritePreviewPlanRefusal::ToolDisallowed { step_id, tool } => {
            format!("tool-disallowed: step={step_id} tool={tool}")
        }
        WritePreviewPlanRefusal::MultipleWorkspaceWriteSteps { count } => {
            format!("multi-write-refused: count={count}")
        }
        WritePreviewPlanRefusal::PreviewProduction(refusal) => refusal.reason(),
    }
}

// =========================================================================
// A2-L2b Slice 3c — CLI-local approval UX plumbing (hidden test-only seam)
// =========================================================================
//
// This block wires the Slice-3b operator-facing renderers
// (`render_approval_prompt`, `render_non_approvable_summary`) and the
// Slice-3a strict approval parser (`evaluate_operator_input`) to an
// injected `Read` / `Write` pair so the CLI can be unit-tested against
// the approval contract without a live TTY.
//
// Hard scope (Slice 3c):
//
// * Helper is private to this binary. There is no public CLI subcommand
//   that invokes it; the production `claw plan run` dispatch above is
//   unchanged.
// * The helper performs no filesystem writes, no subprocess invocations,
//   no broker/network calls, and never wires into the `run_plan`
//   workspace-write execution path (workspace-write remains entirely
//   out of `run_plan` in this slice).
// * `a2-plan-runner` is consumed via its existing crate-root re-exports
//   only. No new exports are added there, and no `stdin`/`stdout`
//   side effects are introduced into that crate.

// Mirror of [`a2_plan_runner::EXIT_APPROVAL_DENIED`], pinned here so the
// CLI owns its exit-code namespace. The value is taken from the runner
// constant directly so the two cannot drift.
const EXIT_APPROVAL_DENIED: i32 = a2_plan_runner::EXIT_APPROVAL_DENIED;

/// Structured outcome of one CLI-local approval interaction.
///
/// `decision` is the authoritative result from the Slice-3a parser.
/// `audit_markers` carry the Slice-3b renderer's audit-only tokens plus
/// a single decision-tier marker; they are *never* authority for the
/// outcome. `exit_code_hint` is the CLI's standard approval-denial code
/// (`7`) on any non-approval, and `0` on approval; callers MAY use it
/// to drive `std::process::exit`, but the slice does not itself wire
/// this into any process-exit path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CliApprovalInteractionResult {
    decision: a2_plan_runner::ApprovalDecision,
    exit_code_hint: i32,
    audit_markers: Vec<&'static str>,
}

/// Render the operator prompt (or non-approvable summary) for
/// `preview_record` to `output`, optionally read one operator
/// submission from `input`, and return the structured approval
/// decision.
///
/// Contract:
///
/// 1. Renders via the Slice-3b helpers
///    [`a2_plan_runner::render_approval_prompt`] /
///    [`a2_plan_runner::render_non_approvable_summary`].
/// 2. Writes the rendered text to `output` exactly once.
/// 3. Flushes `output` BEFORE any read from `input`.
/// 4. Non-approvable previews (`is_binary`, `is_redacted`,
///    `is_truncated`) short-circuit: **no bytes are read from `input`**.
///    The returned decision mirrors the refusal the Slice-3a parser
///    would have produced on the matching preview-state branch.
/// 5. Approvable previews read ONE line from `input` so an interactive
///    operator who types the approval line and presses Enter gets a
///    verdict without having to signal EOF (Ctrl-D). When the underlying
///    read delivered more than that first line (a paste or piped
///    payload), the already-buffered remainder is appended so the full
///    content reaches the parser — those extra bytes are consumed here
///    rather than left for the shell. The residue then has *exactly one*
///    trailing `\n` or `\r\n` stripped (priority: `\r\n` first, then
///    `\n`) and is passed verbatim to
///    [`a2_plan_runner::evaluate_operator_input`]. Embedded newlines,
///    pasted approval markers, batch syntax, etc. are all refused by the
///    underlying strict parser exactly as before.
/// 6. EOF (operator supplies zero bytes) flows through the parser path
///    and is refused via [`a2_plan_runner::ApprovalRefusal::ArgCount`].
fn run_approval_interaction<R: std::io::Read, W: std::io::Write>(
    preview_record: &a2_plan_runner::PreviewRecord,
    preview_display: &a2_plan_runner::PreviewDisplay,
    checkpoint_baseline_unchanged: bool,
    input: R,
    mut output: W,
) -> std::io::Result<CliApprovalInteractionResult> {
    use a2_plan_runner::markers::{L2B_APPROVAL_REFUSED, L2B_APPROVED};
    use a2_plan_runner::{
        evaluate_operator_input, render_approval_prompt, render_non_approvable_summary,
        ApprovalDecision, ApprovalRefusal,
    };
    use std::io::BufRead;

    if !preview_record.is_approvable() {
        let render = render_non_approvable_summary(preview_record);
        write!(output, "{}", render.text)?;
        output.flush()?;

        // Mirror the precedence used by `evaluate_approval` so callers
        // see the same refusal variant the parser would have produced
        // if the operator had typed a syntactically valid approval.
        let refusal = if preview_record.is_binary {
            ApprovalRefusal::PreviewBinary
        } else if preview_record.is_redacted {
            ApprovalRefusal::PreviewRedacted
        } else if preview_record.is_truncated {
            ApprovalRefusal::PreviewTruncated
        } else {
            // Defensive: `is_approvable()` returned false but no flag
            // was set. Surface a refusal consistent with the
            // "no approval command accepted" contract.
            ApprovalRefusal::ArgCount
        };

        let mut audit_markers = render.audit_markers;
        audit_markers.push(L2B_APPROVAL_REFUSED);

        return Ok(CliApprovalInteractionResult {
            decision: ApprovalDecision::Refused(refusal),
            exit_code_hint: EXIT_APPROVAL_DENIED,
            audit_markers,
        });
    }

    let render = render_approval_prompt(preview_record, preview_display);
    write!(output, "{}", render.text)?;
    output.flush()?;

    // Interactive Enter-to-approve: read ONE line so a TTY operator who types
    // the exact `apply <step-id> <preview_sha256>` line and presses Enter gets
    // a verdict immediately, without having to signal EOF (Ctrl-D).
    //
    // Safety is preserved by draining any bytes that arrived *together with*
    // the first line. `BufReader` pulls a full chunk in one underlying read;
    // for a clean single-line submission that chunk is exactly the line and
    // `buffer()` is empty afterward, so no second (blocking) read occurs. For
    // a multi-line paste / piped payload the remainder is already buffered, so
    // we append it and hand the full content to the strict parser — which
    // refuses embedded newlines exactly as the prior read-to-EOF path did, and
    // those extra bytes are consumed here rather than leaking back to the
    // shell.
    let mut buffered = std::io::BufReader::new(input);
    let mut raw = String::new();
    buffered.read_line(&mut raw)?;
    if !buffered.buffer().is_empty() {
        buffered.read_to_string(&mut raw)?;
    }
    let stripped = strip_one_trailing_newline(&raw);
    let decision = evaluate_operator_input(preview_record, stripped, checkpoint_baseline_unchanged);

    let (exit_code_hint, decision_marker) = match &decision {
        ApprovalDecision::Approved { .. } => (0, L2B_APPROVED),
        ApprovalDecision::Refused(_) => (EXIT_APPROVAL_DENIED, L2B_APPROVAL_REFUSED),
    };
    let mut audit_markers = render.audit_markers;
    audit_markers.push(decision_marker);

    Ok(CliApprovalInteractionResult {
        decision,
        exit_code_hint,
        audit_markers,
    })
}

/// Strip *exactly one* trailing line terminator from `s`.
///
/// Recognized terminators in priority order: `"\r\n"`, then `"\n"`. Any
/// earlier newline (including a second trailing one) is left in place
/// so the Slice-3a parser surfaces it as
/// [`a2_plan_runner::ApprovalRefusal::ControlChars`].
fn strip_one_trailing_newline(s: &str) -> &str {
    if let Some(rest) = s.strip_suffix("\r\n") {
        return rest;
    }
    if let Some(rest) = s.strip_suffix('\n') {
        return rest;
    }
    s
}

// =========================================================================
// A2-L2b Slice 3d — `claw plan approve <preview-bundle.json>` command
// =========================================================================
//
// Scope (Slice 3d):
//
// * Reads ONE preview bundle JSON file from disk.
// * Validates the bundle schema (`a2-l2b-preview-bundle.v1`, no extra
//   fields).
// * Verifies the binding between the embedded `PreviewRecord` and
//   `PreviewDisplay` by re-deriving `preview_sha256` from the canonical
//   record subset plus the display's `rendered` bytes — mismatch is a
//   bundle integrity error (exit 5), not a refusal (exit 7).
// * Delegates render-prompt + read-line + evaluate to the Slice-3c
//   helper `run_approval_interaction`.
// * Emits canonical approval-result JSON (`a2-l2b-approval-result.v1`)
//   on stdout.
// * Exit codes: 0 approved, 7 refused / non-approvable / EOF / drift /
//   non-TTY, 5 bundle read / parse / schema / integrity error.
//
// Hard contract (Slice 3d):
//
// * Never opens a write-capable handle to any target file.
// * Never mutates the checkpoint store.
// * Never wires workspace-write into `run_plan`.
// * Never calls the broker, model, or any network.
// * Never spawns a subprocess.
// * Never trusts an embedded approval decision in the bundle;
//   `serde(deny_unknown_fields)` rejects any such field outright.

/// Pinned bundle schema version. Bumped only on a wire-incompatible
/// change to the on-disk layout.
const PREVIEW_BUNDLE_SCHEMA_V1: &str = "a2-l2b-preview-bundle.v1";

/// Pinned approval-result schema version emitted on stdout.
const APPROVAL_RESULT_SCHEMA_V1: &str = "a2-l2b-approval-result.v1";

/// Operator-facing audit marker tokens used in the refusal JSON when
/// the command rejects a bundle before any approval interaction. Pinned
/// here (not as runner markers) because these surface only on the CLI
/// approval-result wire, not in the runner's marker stream.
const APPROVAL_RESULT_AUDIT_BUNDLE_REJECTED: &str = "a2-l2b-approval-result-bundle-rejected";
const APPROVAL_RESULT_AUDIT_NON_TTY: &str = "a2-l2b-approval-result-non-tty";

/// On-disk preview bundle. `deny_unknown_fields` is load-bearing: it
/// rejects any bundle that smuggles an `approval_decision`, `markers`,
/// `signature`, or other field the command might mistake for authority.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBundleV1 {
    schema_version: String,
    preview_record: a2_plan_runner::PreviewRecord,
    preview_display: a2_plan_runner::PreviewDisplay,
    checkpoint_baseline_unchanged: bool,
}

/// Bundle-read / parse / schema / integrity failure causes. Each
/// variant carries a stable short string for the refusal JSON `reason`
/// field; the operator never sees the internal enum.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BundleLoadError {
    /// `fs::read` failed (file missing, permission denied, etc.).
    Io(String),
    /// `serde_json::from_slice` failed.
    Json(String),
    /// `schema_version` field did not match `PREVIEW_BUNDLE_SCHEMA_V1`.
    SchemaVersionMismatch { actual: String },
    /// Re-derived `preview_sha256` did not match
    /// `preview_record.preview_sha256` — the record/display binding is
    /// broken.
    BindingMismatch,
}

impl BundleLoadError {
    fn reason(&self) -> String {
        match self {
            Self::Io(msg) => format!("bundle-io-error: {msg}"),
            Self::Json(msg) => format!("bundle-json-parse-error: {msg}"),
            Self::SchemaVersionMismatch { actual } => {
                format!("bundle-schema-version-mismatch: got {actual}")
            }
            Self::BindingMismatch => "bundle-record-display-binding-mismatch".to_string(),
        }
    }

    fn short(&self) -> &'static str {
        match self {
            Self::Io(_) => "bundle-io-error",
            Self::Json(_) => "bundle-json-parse-error",
            Self::SchemaVersionMismatch { .. } => "bundle-schema-version-mismatch",
            Self::BindingMismatch => "bundle-record-display-binding-mismatch",
        }
    }
}

/// CLI exit code returned when the bundle cannot be loaded, parsed,
/// schema-validated, or its record/display binding fails. Mirrors the
/// `claw plan run` parse-error code so the operator-facing
/// "5 = parse error" contract stays consistent across plan
/// subcommands.
const EXIT_BUNDLE_PARSE_ERROR: i32 = 5;

/// Load and validate a preview bundle from disk.
///
/// Performs read → JSON parse → schema-version check → binding check.
/// All four steps fail with [`BundleLoadError`] mapped to exit code
/// [`EXIT_BUNDLE_PARSE_ERROR`]. The returned bundle is safe to feed
/// into `run_approval_interaction` without further validation.
fn load_preview_bundle(bundle_path: &Path) -> Result<PreviewBundleV1, BundleLoadError> {
    let bytes = std::fs::read(bundle_path).map_err(|e| BundleLoadError::Io(e.to_string()))?;
    let bundle: PreviewBundleV1 =
        serde_json::from_slice(&bytes).map_err(|e| BundleLoadError::Json(e.to_string()))?;
    if bundle.schema_version != PREVIEW_BUNDLE_SCHEMA_V1 {
        return Err(BundleLoadError::SchemaVersionMismatch {
            actual: bundle.schema_version,
        });
    }
    verify_record_display_binding(&bundle.preview_record, &bundle.preview_display)?;
    Ok(bundle)
}

/// Re-derive `preview_sha256` from the canonical record subset plus
/// `display.rendered` and compare against `record.preview_sha256`.
///
/// This is the cryptographic binding that prevents an operator from
/// approving a display they never saw: the hash on the wire MUST be
/// reproducible from the record fields plus the exact rendered bytes
/// the prompt will surface.
fn verify_record_display_binding(
    record: &a2_plan_runner::PreviewRecord,
    display: &a2_plan_runner::PreviewDisplay,
) -> Result<(), BundleLoadError> {
    let subset = a2_plan_runner::CanonicalSubset {
        preview_id: &record.preview_id,
        step_id: &record.step_id,
        target_relative_path_sanitized: &record.target_relative_path_sanitized,
        before_sha256: &record.before_sha256,
        after_sha256: &record.after_sha256,
        checkpoint_run_id: &record.checkpoint_run_id,
        checkpoint_step_id: &record.checkpoint_step_id,
        is_binary: record.is_binary,
        is_redacted: record.is_redacted,
        is_truncated: record.is_truncated,
        preview_format_version: record.preview_format_version,
    };
    let canonical = a2_plan_runner::canonical_preview_record_for_approval(&subset);
    let derived = a2_plan_runner::preview_hash_from_parts(&canonical, &display.rendered);
    if derived != record.preview_sha256 {
        return Err(BundleLoadError::BindingMismatch);
    }
    Ok(())
}

/// Emit the bundle-load refusal JSON envelope to `stdout`. Exit code is
/// [`EXIT_BUNDLE_PARSE_ERROR`]; the JSON `exit_code_hint` mirrors it.
fn emit_bundle_load_failure(err: &BundleLoadError, stdout: &mut dyn Write) -> i32 {
    let payload = serde_json::json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": "bundle_rejected",
        "reason": err.reason(),
        "exit_code_hint": EXIT_BUNDLE_PARSE_ERROR,
        "audit_markers": [APPROVAL_RESULT_AUDIT_BUNDLE_REJECTED, err.short()],
    });
    // Best-effort emit: a broken stdout pipe at this point cannot
    // change the operator's verdict (we have nothing useful to do
    // beyond returning the exit code).
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    EXIT_BUNDLE_PARSE_ERROR
}

/// Emit the non-TTY refusal JSON envelope to `stdout`. Exit code is
/// [`EXIT_APPROVAL_DENIED`]; the JSON `exit_code_hint` mirrors it.
/// `checkpoint_baseline_unchanged` is the bundle's reported value
/// (passed through, never re-asserted by this command).
fn emit_non_tty_refusal(
    record: &a2_plan_runner::PreviewRecord,
    checkpoint_baseline_unchanged: bool,
    reason: &str,
    stdout: &mut dyn Write,
) -> i32 {
    let payload = serde_json::json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": "refused",
        "reason": reason,
        "preview_id": record.preview_id,
        "step_id": record.step_id,
        "preview_sha256": record.preview_sha256,
        "checkpoint_baseline_unchanged": checkpoint_baseline_unchanged,
        "exit_code_hint": EXIT_APPROVAL_DENIED,
        "audit_markers": [
            APPROVAL_RESULT_AUDIT_NON_TTY,
            a2_plan_runner::markers::L2B_APPROVAL_REFUSED,
        ],
    });
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    EXIT_APPROVAL_DENIED
}

/// Emit the approval-result JSON envelope to `stdout` for an
/// `ApprovalDecision` produced by the Slice-3c interaction helper.
/// Returns the exit code mirrored in `exit_code_hint`.
fn emit_approval_result(
    record: &a2_plan_runner::PreviewRecord,
    interaction: &CliApprovalInteractionResult,
    checkpoint_baseline_unchanged: bool,
    stdout: &mut dyn Write,
) -> i32 {
    let (decision_label, reason): (&str, Option<String>) = match &interaction.decision {
        a2_plan_runner::ApprovalDecision::Approved { .. } => ("approved", None),
        a2_plan_runner::ApprovalDecision::Refused(refusal) => {
            ("refused", Some(refusal.describe().to_string()))
        }
    };
    let mut payload = serde_json::json!({
        "schema_version": APPROVAL_RESULT_SCHEMA_V1,
        "decision": decision_label,
        "preview_id": record.preview_id,
        "step_id": record.step_id,
        "preview_sha256": record.preview_sha256,
        "checkpoint_baseline_unchanged": checkpoint_baseline_unchanged,
        "exit_code_hint": interaction.exit_code_hint,
        "audit_markers": interaction.audit_markers,
    });
    if let Some(r) = reason {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("reason".to_string(), serde_json::Value::String(r));
        }
    }
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    interaction.exit_code_hint
}

/// Operator entry point for `claw plan approve <bundle-path>`.
///
/// `stdin_is_tty` is the production runtime's `IsTerminal` probe. Tests
/// inject `true` so the production TTY guard is exercisable through the
/// helper without requiring a real terminal; the production dispatcher
/// always passes the live probe result.
fn run_plan_approve<R: Read, W1: Write, W2: Write>(
    bundle_path: &Path,
    stdin_is_tty: bool,
    stdin: &mut R,
    stdout: &mut W1,
    stderr: &mut W2,
) -> i32 {
    let bundle = match load_preview_bundle(bundle_path) {
        Ok(b) => b,
        Err(e) => {
            return emit_bundle_load_failure(&e, stdout);
        }
    };

    // Fail-closed non-TTY guard: only triggers for approvable previews,
    // because a non-approvable preview never reads stdin in the first
    // place (Slice-3c short-circuits). Honoring the TTY check on the
    // non-approvable path would surface an unrelated refusal reason.
    if bundle.preview_record.is_approvable() && !stdin_is_tty {
        return emit_non_tty_refusal(
            &bundle.preview_record,
            bundle.checkpoint_baseline_unchanged,
            "approval-stdin-not-tty",
            stdout,
        );
    }

    let interaction = match run_approval_interaction(
        &bundle.preview_record,
        &bundle.preview_display,
        bundle.checkpoint_baseline_unchanged,
        stdin,
        &mut *stderr,
    ) {
        Ok(r) => r,
        Err(_e) => {
            // A stderr write failure here is unusual and not the
            // operator's fault. Surface as a refusal so the operator
            // never sees a silent approval; the exit code stays in the
            // operator-facing namespace.
            return emit_non_tty_refusal(
                &bundle.preview_record,
                bundle.checkpoint_baseline_unchanged,
                "approval-prompt-write-failed",
                stdout,
            );
        }
    };

    emit_approval_result(
        &bundle.preview_record,
        &interaction,
        bundle.checkpoint_baseline_unchanged,
        stdout,
    )
}

// =========================================================================
// END A2-L2b Slice 3d — scope sentinel
// =========================================================================
//
// The source-grep tests in `plan_approve_tests` use this sentinel to bound
// the implementation region they scan for forbidden APIs. Do not move or
// rename it without updating those tests.

// =========================================================================
// Option C — approval-result persistence (guard-preserving)
// =========================================================================
//
// `--approval-result-output <path>` on `claw plan approve` persists the EXACT
// approval-result JSON the command already emits on stdout to an
// operator-specified file, but ONLY after a successful approved decision.
//
// This wrapper lives OUTSIDE the Slice-3d scope sentinel region on purpose:
// the file-write APIs it uses are forbidden INSIDE that region by the
// `plan_approve_tests` source-grep guards. It changes none of the approval
// semantics — it calls `run_plan_approve` unchanged into a capture buffer,
// relays that buffer verbatim to the real stdout, and writes the same bytes
// to the output file only when the approval exit code is 0 (approved).
//
// It NEVER relaxes the TTY guard, NEVER accepts pre-approval flags, NEVER
// applies, and NEVER writes the workspace target file.

/// Exit code when an approval succeeded but persisting the approval-result to
/// `--approval-result-output <path>` failed (or the path already exists).
/// Distinct from the approval namespace (0 approved / 5 parse / 7 denied) so
/// the operator can tell "approved but not persisted" from "not approved".
const EXIT_APPROVAL_OUTPUT_IO: i32 = 12;

/// Persist `bytes` to `path`, refusing to overwrite an existing file.
///
/// Uses `create_new(true)` so an existing file is an error (never clobber
/// prior approval evidence) and requires the parent directory to already
/// exist (no broad directory creation). The approval-result is a single small
/// JSON line, so a direct create + write + flush is sufficient.
fn persist_approval_result_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

/// Guard-preserving wrapper around [`run_plan_approve`] that optionally
/// persists the emitted approval-result JSON to `approval_result_output`.
///
/// * If an output path is supplied and already exists, refuse up front (exit
///   [`EXIT_APPROVAL_OUTPUT_IO`]) WITHOUT running the approval — never clobber
///   existing evidence, and never make the operator approve only to fail
///   persistence afterward.
/// * Otherwise run [`run_plan_approve`] unchanged into a capture buffer and
///   relay that buffer verbatim to `real_stdout` (stdout behavior unchanged).
/// * Only when the approval exit code is `0` (approved) AND an output path was
///   supplied, write the SAME bytes to the file. A write failure surfaces on
///   `stderr` and returns [`EXIT_APPROVAL_OUTPUT_IO`]; the stdout JSON still
///   shows the approved decision.
/// * On any non-approved exit code, no file is written.
fn run_plan_approve_with_output<R: Read, W1: Write, W2: Write>(
    bundle_path: &Path,
    approval_result_output: Option<&Path>,
    stdin_is_tty: bool,
    stdin: &mut R,
    real_stdout: &mut W1,
    stderr: &mut W2,
) -> i32 {
    if let Some(path) = approval_result_output {
        if path.exists() {
            let _ = writeln!(
                stderr,
                "claw plan approve: --approval-result-output path already exists, \
                 refusing to overwrite: {}",
                path.display()
            );
            return EXIT_APPROVAL_OUTPUT_IO;
        }
    }

    let mut captured: Vec<u8> = Vec::new();
    let code = run_plan_approve(bundle_path, stdin_is_tty, stdin, &mut captured, stderr);

    let _ = real_stdout.write_all(&captured);
    let _ = real_stdout.flush();

    if code == 0 {
        if let Some(path) = approval_result_output {
            if let Err(e) = persist_approval_result_file(path, &captured) {
                let _ = writeln!(
                    stderr,
                    "claw plan approve: approved, but failed to write \
                     --approval-result-output {}: {e}",
                    path.display()
                );
                return EXIT_APPROVAL_OUTPUT_IO;
            }
        }
    }

    code
}

// =========================================================================
// A2-L2b Slice L2b-CLI-Apply — `claw plan apply <apply-bundle.json>` command
// =========================================================================
//
// Scope:
//
// * Reads ONE apply bundle JSON file from disk.
// * Validates the bundle schema (`a2-l2b-apply-bundle.v1`, no extra fields
//   anywhere via `serde(deny_unknown_fields)`).
// * Loads the payload file bytes from disk and verifies size + sha256.
// * Re-binds the payload bytes to the embedded `PreviewRecord` via
//   [`a2_plan_runner::bind_after_bytes`].
// * Re-validates the embedded approval result is `decision == "approved"`
//   and that its `step_id` / `preview_sha256` bind to the embedded
//   `PreviewRecord` — refusal otherwise (exit 7).
// * Loads the checkpoint `Manifest` from disk and reconstructs a
//   `CheckpointHandle` for it (manifest + optional `before.bin`).
// * Canonicalizes the operator-supplied `workspace_root` and resolves the
//   write target fresh through Slice-1 `resolve_write_target`.
// * Builds a `WriteExecutionRequest` and invokes
//   [`a2_plan_runner::execute_write`].
// * Emits exactly one JSON envelope (`a2-l2b-apply-result.v1`) on stdout.
// * Exit codes mirror the executor: 0 applied, 5 invalid bundle / authority
//   mismatch, 7 approval refused / mismatched, 8 rollback failed, 9
//   baseline drift, 10 atomic write IO failed, 11 validation failed +
//   rolled back.
//
// Hard contract:
//
// * Never opens a write-capable handle to any path other than the executor's
//   atomic temp + rename inside the resolved target's parent directory.
// * Never spawns a subprocess.
// * Never calls the broker, model, or any network.
// * Never reads stdin.
// * Never accepts pre-approval flags (`--yes`, `--auto`, `--force`,
//   `--allow-write`, `--preapproved`, `--batch`).
// * Never writes more than one target file per invocation.
// * Never reads or executes payload bytes from stdin or base64.
// * Never trusts markers in the bundle as authority.

/// Pinned apply-bundle schema version. Bumped only on a wire-incompatible
/// change to the on-disk layout.
const APPLY_BUNDLE_SCHEMA_V1: &str = "a2-l2b-apply-bundle.v1";

/// Pinned apply-result schema version emitted on stdout.
const APPLY_RESULT_SCHEMA_V1: &str = "a2-l2b-apply-result.v1";

/// Operator-facing audit marker emitted on the apply-result JSON when the
/// command rejects a bundle before any executor invocation. Pinned at the
/// CLI layer because the executor never sees these refusals.
const APPLY_RESULT_AUDIT_BUNDLE_REJECTED: &str = "a2-l2b-apply-result-bundle-rejected";

/// Operator-facing audit marker emitted when the command rejects the
/// embedded approval result before any executor invocation (e.g.,
/// `decision != "approved"`, `step_id` / `preview_sha256` mismatch).
const APPLY_RESULT_AUDIT_APPROVAL_REFUSED: &str = "a2-l2b-apply-result-approval-refused";

/// Embedded approval result inside an apply bundle. Mirrors the
/// `a2-l2b-approval-result.v1` shape `claw plan approve` emits on stdout.
/// `deny_unknown_fields` is load-bearing: it rejects any approval result
/// that smuggles a marker field the command might mistake for authority.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBundleApprovalResult {
    schema_version: String,
    decision: String,
    preview_id: String,
    step_id: String,
    preview_sha256: String,
    #[serde(default)]
    checkpoint_baseline_unchanged: Option<bool>,
    #[serde(default)]
    exit_code_hint: Option<i32>,
    #[serde(default)]
    audit_markers: Option<Vec<String>>,
    #[serde(default)]
    reason: Option<String>,
}

/// Embedded checkpoint reference. v1 carries only the manifest file path;
/// the `before.bin` (when present) is reconstructed as
/// `manifest_path.parent()/before.bin`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBundleCheckpoint {
    manifest_path: PathBuf,
}

/// Embedded payload reference. v1 only supports `kind == "file"` with a
/// path on disk. No inline base64, no stdin.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBundlePayload {
    kind: String,
    path: PathBuf,
    after_sha256: String,
    after_size_bytes: u64,
}

/// On-disk apply bundle. `deny_unknown_fields` everywhere prevents an
/// operator from smuggling unrecognized authority data.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBundleV1 {
    schema_version: String,
    workspace_root: PathBuf,
    target_relative_path: String,
    preview_record: a2_plan_runner::PreviewRecord,
    approval_result: ApplyBundleApprovalResult,
    checkpoint: ApplyBundleCheckpoint,
    payload: ApplyBundlePayload,
}

/// Apply-bundle load / validation failure causes. Each variant carries a
/// stable short string for the refusal JSON `reason` field; the operator
/// never sees the internal enum.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApplyBundleLoadError {
    /// `fs::read` on the bundle file failed.
    BundleIo(String),
    /// `serde_json::from_slice` on the bundle failed (including unknown
    /// fields anywhere in the nested tree).
    BundleJson(String),
    /// `schema_version` did not match `APPLY_BUNDLE_SCHEMA_V1`.
    SchemaVersionMismatch { actual: String },
    /// `workspace_root` canonicalization failed (missing / not a dir / I/O
    /// error). Catches operator-supplied root that doesn't exist before
    /// the resolver is touched.
    WorkspaceRootInvalid(String),
    /// Operator-supplied `target_relative_path` ≠
    /// `preview_record.target_relative_path_sanitized`.
    TargetPathMismatch,
    /// `payload.kind` is not the supported `"file"`.
    UnsupportedPayloadKind { actual: String },
    /// Payload file could not be read from `payload.path`.
    PayloadIo(String),
    /// Disk size of payload file ≠ declared `payload.after_size_bytes`.
    PayloadSizeMismatch { declared: u64, actual: u64 },
    /// `payload.after_sha256` ≠ `preview_record.after_sha256`. Catches
    /// a tampered bundle declaration before any file read.
    PayloadPreviewAfterShaMismatch,
    /// Manifest file could not be read.
    ManifestIo(String),
    /// Manifest JSON parse failed.
    ManifestJson(String),
    /// Manifest claimed `pre_existed = true`, but the colocated
    /// `before.bin` is missing on disk.
    ManifestBeforeBinMissing,
    /// `bind_after_bytes` refused. Each [`a2_plan_runner::BindError`]
    /// variant maps to a distinct short token so operators can
    /// distinguish hash mismatch (the most common error) from size cap,
    /// non-approvable preview, or lexical target-path refusal.
    PayloadBindRefused(a2_plan_runner::BindError),
}

impl ApplyBundleLoadError {
    fn reason(&self) -> String {
        match self {
            Self::BundleIo(m) => format!("bundle-io-error: {m}"),
            Self::BundleJson(m) => format!("bundle-json-parse-error: {m}"),
            Self::SchemaVersionMismatch { actual } => {
                format!("bundle-schema-version-mismatch: got {actual}")
            }
            Self::WorkspaceRootInvalid(m) => format!("bundle-workspace-root-invalid: {m}"),
            Self::TargetPathMismatch => "bundle-target-path-mismatch".to_string(),
            Self::UnsupportedPayloadKind { actual } => {
                format!("bundle-unsupported-payload-kind: got {actual}")
            }
            Self::PayloadIo(m) => format!("payload-io-error: {m}"),
            Self::PayloadSizeMismatch { declared, actual } => {
                format!("payload-size-mismatch: declared={declared} actual={actual}")
            }
            Self::PayloadPreviewAfterShaMismatch => {
                "payload-preview-after-sha-mismatch".to_string()
            }
            Self::ManifestIo(m) => format!("checkpoint-manifest-io-error: {m}"),
            Self::ManifestJson(m) => format!("checkpoint-manifest-parse-error: {m}"),
            Self::ManifestBeforeBinMissing => "checkpoint-before-bin-missing".to_string(),
            Self::PayloadBindRefused(e) => format!("{}: {e}", bind_error_short(e)),
        }
    }

    fn short(&self) -> &'static str {
        match self {
            Self::BundleIo(_) => "bundle-io-error",
            Self::BundleJson(_) => "bundle-json-parse-error",
            Self::SchemaVersionMismatch { .. } => "bundle-schema-version-mismatch",
            Self::WorkspaceRootInvalid(_) => "bundle-workspace-root-invalid",
            Self::TargetPathMismatch => "bundle-target-path-mismatch",
            Self::UnsupportedPayloadKind { .. } => "bundle-unsupported-payload-kind",
            Self::PayloadIo(_) => "payload-io-error",
            Self::PayloadSizeMismatch { .. } => "payload-size-mismatch",
            Self::PayloadPreviewAfterShaMismatch => "payload-preview-after-sha-mismatch",
            Self::ManifestIo(_) => "checkpoint-manifest-io-error",
            Self::ManifestJson(_) => "checkpoint-manifest-parse-error",
            Self::ManifestBeforeBinMissing => "checkpoint-before-bin-missing",
            Self::PayloadBindRefused(e) => bind_error_short(e),
        }
    }
}

/// Short stable token for each [`a2_plan_runner::BindError`] variant. The
/// payload-hash-mismatch arm is the operator-facing common case; the
/// others are defense-in-depth and surface only when the bundle is
/// internally inconsistent.
fn bind_error_short(e: &a2_plan_runner::BindError) -> &'static str {
    match e {
        a2_plan_runner::BindError::PayloadHashMismatch { .. } => "payload-hash-mismatch",
        a2_plan_runner::BindError::PreviewNotApprovable => "preview-not-approvable",
        a2_plan_runner::BindError::TargetPathMismatch => "payload-target-path-mismatch",
        a2_plan_runner::BindError::InvalidTargetPath => "payload-invalid-target-path",
        a2_plan_runner::BindError::PayloadTooLarge { .. } => "payload-too-large",
    }
}

/// Why the command refused the embedded approval result before any
/// executor invocation. Returns exit code 7 on every arm.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApplyApprovalRefusal {
    /// `approval_result.decision` was not the literal string `"approved"`.
    DecisionNotApproved { actual: String },
    /// `approval_result.step_id` ≠ `preview_record.step_id`.
    StepIdMismatch,
    /// `approval_result.preview_sha256` ≠ `preview_record.preview_sha256`.
    PreviewShaMismatch,
}

impl ApplyApprovalRefusal {
    fn reason(&self) -> String {
        match self {
            Self::DecisionNotApproved { actual } => {
                format!("approval-decision-not-approved: got {actual:?}")
            }
            Self::StepIdMismatch => "approval-step-id-mismatch".to_string(),
            Self::PreviewShaMismatch => "approval-preview-sha-mismatch".to_string(),
        }
    }

    fn short(&self) -> &'static str {
        match self {
            Self::DecisionNotApproved { .. } => "approval-decision-not-approved",
            Self::StepIdMismatch => "approval-step-id-mismatch",
            Self::PreviewShaMismatch => "approval-preview-sha-mismatch",
        }
    }
}

/// CLI exit code returned when the bundle cannot be loaded, parsed,
/// schema-validated, or fails any pre-executor authority check that is
/// NOT specifically an approval refusal. Mirrors
/// `a2_plan_runner::EXIT_INVALID_REQUEST` to keep the operator-facing
/// "5 = parse / invalid request" contract consistent across plan
/// subcommands.
const EXIT_APPLY_BUNDLE_REJECTED: i32 = 5;

/// Resolved load + validated authority chain. Owned values only; the
/// executor borrows from this at call time.
struct LoadedApplyBundle {
    bundle: ApplyBundleV1,
    workspace_root: PathBuf,
    handle: a2_plan_runner::CheckpointHandle,
    payload: a2_plan_runner::ApprovedWritePayload,
}

/// Read and statically validate an apply bundle from disk.
///
/// On success the returned [`LoadedApplyBundle`] carries the parsed bundle
/// plus a canonicalized `workspace_root`, a reconstructed
/// [`a2_plan_runner::CheckpointHandle`], and an
/// [`a2_plan_runner::ApprovedWritePayload`] minted by `bind_after_bytes`.
/// Side effects: read-only filesystem syscalls on the bundle file,
/// payload file, and manifest file; canonicalization of `workspace_root`
/// and `manifest_path`. No writes.
fn load_apply_bundle(bundle_path: &Path) -> Result<LoadedApplyBundle, ApplyBundleLoadError> {
    // 1. Read the bundle JSON.
    let bytes =
        std::fs::read(bundle_path).map_err(|e| ApplyBundleLoadError::BundleIo(e.to_string()))?;
    let bundle: ApplyBundleV1 = serde_json::from_slice(&bytes)
        .map_err(|e| ApplyBundleLoadError::BundleJson(e.to_string()))?;

    if bundle.schema_version != APPLY_BUNDLE_SCHEMA_V1 {
        return Err(ApplyBundleLoadError::SchemaVersionMismatch {
            actual: bundle.schema_version,
        });
    }

    // 2. Workspace root must canonicalize to an existing directory.
    let workspace_root = bundle
        .workspace_root
        .canonicalize()
        .map_err(|e| ApplyBundleLoadError::WorkspaceRootInvalid(e.to_string()))?;
    let ws_meta = std::fs::symlink_metadata(&workspace_root)
        .map_err(|e| ApplyBundleLoadError::WorkspaceRootInvalid(e.to_string()))?;
    if !ws_meta.is_dir() {
        return Err(ApplyBundleLoadError::WorkspaceRootInvalid(
            "workspace_root is not a directory".to_string(),
        ));
    }

    // 3. Operator-supplied target path must match the preview's.
    if bundle.target_relative_path != bundle.preview_record.target_relative_path_sanitized {
        return Err(ApplyBundleLoadError::TargetPathMismatch);
    }

    // 4. Payload kind must be "file" in v1.
    if bundle.payload.kind != "file" {
        return Err(ApplyBundleLoadError::UnsupportedPayloadKind {
            actual: bundle.payload.kind.clone(),
        });
    }

    // 5. Payload-preview hash cross-check (catches a tampered bundle
    //    declaration without touching the payload file).
    if bundle.payload.after_sha256 != bundle.preview_record.after_sha256 {
        return Err(ApplyBundleLoadError::PayloadPreviewAfterShaMismatch);
    }

    // 6. Payload file: read + size cross-check. `bind_after_bytes` does
    //    the authoritative sha256 + size-cap + target-path checks below.
    let payload_bytes = std::fs::read(&bundle.payload.path)
        .map_err(|e| ApplyBundleLoadError::PayloadIo(e.to_string()))?;
    let payload_bytes_len = payload_bytes.len() as u64;
    if payload_bytes_len != bundle.payload.after_size_bytes {
        return Err(ApplyBundleLoadError::PayloadSizeMismatch {
            declared: bundle.payload.after_size_bytes,
            actual: payload_bytes_len,
        });
    }

    // 7. Manifest: read, parse, reconstruct CheckpointHandle.
    let manifest_bytes = std::fs::read(&bundle.checkpoint.manifest_path)
        .map_err(|e| ApplyBundleLoadError::ManifestIo(e.to_string()))?;
    let manifest: a2_plan_runner::Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| ApplyBundleLoadError::ManifestJson(e.to_string()))?;

    let manifest_path = bundle.checkpoint.manifest_path.clone();
    let step_dir = manifest_path
        .parent()
        .ok_or_else(|| ApplyBundleLoadError::ManifestIo("manifest_path has no parent".to_string()))?
        .to_path_buf();
    let before_bin_path = if manifest.pre_existed {
        let candidate = step_dir.join("before.bin");
        if !candidate.is_file() {
            return Err(ApplyBundleLoadError::ManifestBeforeBinMissing);
        }
        Some(candidate)
    } else {
        None
    };

    let handle = a2_plan_runner::CheckpointHandle {
        step_dir,
        manifest_path,
        before_bin_path,
        manifest,
    };

    // 8. Bind payload bytes to the preview record via Slice-4a. This is
    //    the authoritative hash + size + target-path check.
    let target_rel_path = PathBuf::from(&bundle.target_relative_path);
    let payload =
        a2_plan_runner::bind_after_bytes(&bundle.preview_record, target_rel_path, payload_bytes)
            .map_err(ApplyBundleLoadError::PayloadBindRefused)?;

    Ok(LoadedApplyBundle {
        bundle,
        workspace_root,
        handle,
        payload,
    })
}

/// Validate the embedded approval result against the embedded preview
/// record. Returns the structured `ApprovalDecision::Approved` on success.
fn validate_apply_approval(
    approval: &ApplyBundleApprovalResult,
    preview: &a2_plan_runner::PreviewRecord,
) -> Result<a2_plan_runner::ApprovalDecision, ApplyApprovalRefusal> {
    if approval.decision != "approved" {
        return Err(ApplyApprovalRefusal::DecisionNotApproved {
            actual: approval.decision.clone(),
        });
    }
    if approval.step_id != preview.step_id {
        return Err(ApplyApprovalRefusal::StepIdMismatch);
    }
    if approval.preview_sha256 != preview.preview_sha256 {
        return Err(ApplyApprovalRefusal::PreviewShaMismatch);
    }
    Ok(a2_plan_runner::ApprovalDecision::Approved {
        step_id: approval.step_id.clone(),
        preview_sha256: approval.preview_sha256.clone(),
    })
}

/// Emit the bundle-load refusal JSON envelope to `stdout`. Exit code is
/// [`EXIT_APPLY_BUNDLE_REJECTED`]; the JSON `exit_code` mirrors it.
fn emit_apply_bundle_load_failure(err: &ApplyBundleLoadError, stdout: &mut dyn Write) -> i32 {
    let payload = serde_json::json!({
        "schema_version": APPLY_RESULT_SCHEMA_V1,
        "outcome": "bundle_rejected",
        "exit_code": EXIT_APPLY_BUNDLE_REJECTED,
        "reason": err.reason(),
        "markers": [APPLY_RESULT_AUDIT_BUNDLE_REJECTED, err.short()],
    });
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    EXIT_APPLY_BUNDLE_REJECTED
}

/// Emit the embedded-approval refusal JSON envelope to `stdout`. Exit
/// code is `EXIT_APPROVAL_DENIED` (`7`); the JSON `exit_code` mirrors it.
fn emit_apply_approval_refusal(
    refusal: &ApplyApprovalRefusal,
    preview: &a2_plan_runner::PreviewRecord,
    stdout: &mut dyn Write,
) -> i32 {
    let exit_code = a2_plan_runner::EXIT_APPROVAL_DENIED;
    let payload = serde_json::json!({
        "schema_version": APPLY_RESULT_SCHEMA_V1,
        "outcome": "refused",
        "exit_code": exit_code,
        "reason": refusal.reason(),
        "step_id": preview.step_id,
        "preview_id": preview.preview_id,
        "preview_sha256": preview.preview_sha256,
        "target_relative_path": preview.target_relative_path_sanitized,
        "markers": [APPLY_RESULT_AUDIT_APPROVAL_REFUSED, refusal.short()],
    });
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    exit_code
}

/// Emit the path-resolution refusal JSON envelope to `stdout`. The
/// resolver's exit code is `6` (write-path-refused); we keep that
/// verbatim so the operator gets a distinct code from the executor's `9`
/// (baseline drift).
fn emit_apply_resolver_refusal(
    refusal: &a2_plan_runner::write_runtime::WriteTargetRefusal,
    preview: &a2_plan_runner::PreviewRecord,
    stdout: &mut dyn Write,
) -> i32 {
    let exit_code = refusal.exit_code();
    let payload = serde_json::json!({
        "schema_version": APPLY_RESULT_SCHEMA_V1,
        "outcome": "refused",
        "exit_code": exit_code,
        "reason": format!("resolver-refused: {:?}", refusal),
        "step_id": preview.step_id,
        "preview_id": preview.preview_id,
        "preview_sha256": preview.preview_sha256,
        "target_relative_path": preview.target_relative_path_sanitized,
        "markers": [refusal.marker()],
    });
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    exit_code
}

/// Emit the executor result JSON envelope to `stdout` and return its
/// exit code. The `outcome` label is the only thing this function adds
/// over the executor's [`a2_plan_runner::WriteExecutionResult`] — the
/// markers + exit code come straight from the executor.
fn emit_apply_executor_result(
    result: &a2_plan_runner::WriteExecutionResult,
    preview: &a2_plan_runner::PreviewRecord,
    stdout: &mut dyn Write,
) -> i32 {
    let (label, reason) = match &result.outcome {
        a2_plan_runner::WriteExecutionOutcome::Applied { .. } => ("applied", None),
        a2_plan_runner::WriteExecutionOutcome::RefusedAuthorityMismatch { cause } => {
            ("refused", Some(format!("authority-mismatch: {cause:?}")))
        }
        a2_plan_runner::WriteExecutionOutcome::RefusedApproval { cause } => {
            ("refused", Some(format!("approval-refused: {cause:?}")))
        }
        a2_plan_runner::WriteExecutionOutcome::RefusedBaselineDrift { cause } => {
            ("refused", Some(format!("baseline-drift: {cause:?}")))
        }
        a2_plan_runner::WriteExecutionOutcome::AtomicWriteIoFailed { stage, message } => (
            "io_failed",
            Some(format!("atomic-write-io: {stage:?}: {message}")),
        ),
        a2_plan_runner::WriteExecutionOutcome::ValidationFailedRolledBack { message } => (
            "validation_failed_rolled_back",
            Some(format!("post-write-validation-failed: {message}")),
        ),
        a2_plan_runner::WriteExecutionOutcome::RollbackFailed { cause } => (
            "rollback_failed",
            Some(format!("rollback-failed: {cause:?}")),
        ),
    };
    let mut payload = serde_json::json!({
        "schema_version": APPLY_RESULT_SCHEMA_V1,
        "outcome": label,
        "exit_code": result.exit_code,
        "step_id": preview.step_id,
        "preview_id": preview.preview_id,
        "preview_sha256": preview.preview_sha256,
        "target_relative_path": preview.target_relative_path_sanitized,
        "markers": result.markers,
    });
    if let Some(r) = reason {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("reason".to_string(), serde_json::Value::String(r));
        }
    }
    let _ = writeln!(stdout, "{payload}");
    let _ = stdout.flush();
    result.exit_code
}

/// Operator entry point for `claw plan apply <apply-bundle.json>`.
///
/// Single-file real-write command. Reads the bundle from disk, validates
/// every authority object, resolves the target through Slice-1, and
/// invokes the library-level [`a2_plan_runner::execute_write`]. Emits
/// exactly one JSON line on stdout and returns the executor's exit code
/// (or a pre-executor refusal code).
fn run_plan_apply<W: Write>(bundle_path: &Path, stdout: &mut W) -> i32 {
    let loaded = match load_apply_bundle(bundle_path) {
        Ok(l) => l,
        Err(e) => return emit_apply_bundle_load_failure(&e, stdout),
    };

    let approval = match validate_apply_approval(
        &loaded.bundle.approval_result,
        &loaded.bundle.preview_record,
    ) {
        Ok(a) => a,
        Err(refusal) => {
            return emit_apply_approval_refusal(&refusal, &loaded.bundle.preview_record, stdout);
        }
    };

    // Resolve the target fresh through Slice-1. `WriteTarget` carries the
    // operator-relative path; the resolver re-proves every lexical /
    // symlink / parent invariant against the live filesystem.
    let write_target = a2_plan_schema::WriteTarget {
        path: loaded.bundle.target_relative_path.clone(),
        // `create_if_absent` is advisory for the schema layer; the runtime
        // resolver only inspects the path. Mirror the manifest's pre-write
        // state so the bundle's intent is preserved in the resolver call.
        create_if_absent: !loaded.handle.manifest.pre_existed,
    };
    let resolved = match a2_plan_runner::write_runtime::resolve_write_target(
        &loaded.workspace_root,
        &write_target,
    ) {
        Ok(r) => r,
        Err(refusal) => {
            return emit_apply_resolver_refusal(&refusal, &loaded.bundle.preview_record, stdout);
        }
    };

    let request = a2_plan_runner::WriteExecutionRequest {
        workspace_root: &loaded.workspace_root,
        resolved: &resolved,
        checkpoint: &loaded.handle,
        preview: &loaded.bundle.preview_record,
        approval: &approval,
        payload: &loaded.payload,
    };

    let result = a2_plan_runner::execute_write(&request);
    emit_apply_executor_result(&result, &loaded.bundle.preview_record, stdout)
}

// =========================================================================
// END A2-L2b Slice L2b-CLI-Apply — scope sentinel
// =========================================================================
//
// The source-grep tests in `plan_apply_tests` use this sentinel to bound
// the implementation region they scan for forbidden APIs. Do not move or
// rename it without updating those tests.

// =========================================================================
// A2-L2b Slice L2b-CLI-Preview-Bundle — `claw plan preview-bundle` command
// =========================================================================
//
// Scope:
//
// * Resolves an operator-supplied target through Slice-1
//   [`a2_plan_runner::write_runtime::resolve_write_target`].
// * Captures a Slice-2 pre-write checkpoint via
//   [`a2_plan_runner::CheckpointStore::create_checkpoint`].
// * Reads bytes from an operator-supplied after-file and copies them
//   into runner-owned payload storage
//   (`<workspace-root>/.claw/l2b-payloads/<run-id>/<step-id>/after.bin`).
// * Builds a Slice-3a [`a2_plan_runner::PreviewRecord`] +
//   [`a2_plan_runner::PreviewDisplay`] via
//   [`a2_plan_runner::build_preview`].
// * Writes a `preview-bundle.json` compatible with
//   [`PreviewBundleV1`] / `claw plan approve` under
//   `<workspace-root>/.claw/l2b-preview-bundles/<run-id>/<step-id>/`.
// * Emits exactly one JSON envelope
//   (`a2-l2b-preview-bundle-generator-result.v1`) on stdout.
//
// Hard contract:
//
// * NEVER mutates the operator-supplied target file.
// * NEVER calls `claw plan approve` or `claw plan apply` programmatically.
// * NEVER wires into [`a2_plan_runner::run_plan`] workspace-write.
// * NEVER spawns a subprocess.
// * NEVER reads stdin.
// * NEVER calls the broker, model, or any network.
// * NEVER accepts pre-approval flags (`--yes`, `--auto`, `--force`,
//   `--allow-write`, `--preapproved`, `--batch`).
// * NEVER prints raw payload bytes to stdout or stderr.
// * NEVER accepts inline / base64 / stdin payloads — only an on-disk
//   after-file path.
// * NEVER follows a symlinked after-file.
// * NEVER admits an after-file larger than
//   [`a2_plan_runner::MAX_APPROVED_PAYLOAD_BYTES`].

/// Pinned generator-result schema version emitted on stdout. Bumped
/// only on a wire-incompatible change to the envelope's shape.
const PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1: &str = "a2-l2b-preview-bundle-generator-result.v1";

/// Pinned step-id used by the single per-invocation preview produced by
/// this generator. The checkpoint store's `run_id` is generated fresh
/// on every call, so colocating a fixed step-id underneath that ULID
/// directory is safe: each invocation lives in its own run dir.
const PREVIEW_BUNDLE_GENERATOR_STEP_ID: &str = "preview-bundle-step";

/// Audit markers emitted on the generator-result JSON. These are
/// **audit-only**; they MUST NOT be interpreted as authority by any
/// downstream consumer. The cryptographic authority lives in
/// `preview_record.preview_sha256` and the file artifacts the
/// envelope points at.
const PREVIEW_BUNDLE_MARKER_CREATED: &str = "a2-l2b-preview-bundle-created";
const PREVIEW_BUNDLE_MARKER_PAYLOAD_CAPTURED: &str = "a2-l2b-payload-captured";
const PREVIEW_BUNDLE_MARKER_CHECKPOINT_WRITTEN: &str = "a2-l2b-checkpoint-written";

/// Runner-owned payload artifact root, joined with `run-id` /
/// `step-id` / `after.bin` (and a sibling `after.sha256`).
const PREVIEW_BUNDLE_PAYLOAD_ROOT_REL: &str = ".claw/l2b-payloads";

/// Runner-owned preview-bundle artifact root, joined with `run-id` /
/// `step-id` / `preview-bundle.json`.
const PREVIEW_BUNDLE_BUNDLE_ROOT_REL: &str = ".claw/l2b-preview-bundles";

/// CLI exit code for generator refusals. Mirrors the `EXIT_PARSE_ERROR`
/// family used by `claw plan run`, `approve`, and `apply` so the
/// operator-facing "5 = refusal" contract stays consistent across plan
/// subcommands. Granularity is exposed via the `audit_markers` and
/// `reason` fields of the result envelope, not via the exit code.
const EXIT_PREVIEW_BUNDLE_REFUSED: i32 = 5;

/// On-disk preview-bundle shape produced by the generator.
///
/// Field order + names mirror the existing [`PreviewBundleV1`]
/// (`Deserialize`-only) so the bundle this generator writes round-trips
/// into `claw plan approve` without any schema widening.
#[derive(Debug, serde::Serialize)]
struct PreviewBundleV1Output<'a> {
    schema_version: &'a str,
    preview_record: &'a a2_plan_runner::PreviewRecord,
    preview_display: &'a a2_plan_runner::PreviewDisplay,
    checkpoint_baseline_unchanged: bool,
}

/// Generator refusal causes. Each variant maps to a stable short string
/// in the result envelope; the operator never sees the internal enum.
#[derive(Debug)]
enum PreviewBundleRefusal {
    WorkspaceRootInvalid(String),
    AfterFileMissing(String),
    AfterFileNotRegular,
    AfterFileSymlink,
    AfterFileTooLarge {
        actual: u64,
        cap: u64,
    },
    AfterFileIo(String),
    TargetResolveRefused {
        marker: &'static str,
        kind: &'static str,
    },
    BeforeReadIo(String),
    CheckpointFailed(String),
    PreviewBuildFailed(String),
    PayloadIo(String),
    PayloadVerifyMismatch {
        expected: String,
        actual: String,
    },
    PreviewBundleIo(String),
}

impl PreviewBundleRefusal {
    fn short(&self) -> &'static str {
        match self {
            Self::WorkspaceRootInvalid(_) => "workspace-root-invalid",
            Self::AfterFileMissing(_) => "after-file-missing",
            Self::AfterFileNotRegular => "after-file-not-regular",
            Self::AfterFileSymlink => "after-file-symlink",
            Self::AfterFileTooLarge { .. } => "after-file-too-large",
            Self::AfterFileIo(_) => "after-file-io-error",
            Self::TargetResolveRefused { kind, .. } => kind,
            Self::BeforeReadIo(_) => "before-read-io-error",
            Self::CheckpointFailed(_) => "checkpoint-failed",
            Self::PreviewBuildFailed(_) => "preview-build-failed",
            Self::PayloadIo(_) => "payload-io-error",
            Self::PayloadVerifyMismatch { .. } => "payload-verify-mismatch",
            Self::PreviewBundleIo(_) => "preview-bundle-io-error",
        }
    }

    fn reason(&self) -> String {
        match self {
            Self::WorkspaceRootInvalid(m) => format!("workspace-root-invalid: {m}"),
            Self::AfterFileMissing(m) => format!("after-file-missing: {m}"),
            Self::AfterFileNotRegular => "after-file-not-regular: not a regular file".to_string(),
            Self::AfterFileSymlink => "after-file-symlink: refusing to follow symlink".to_string(),
            Self::AfterFileTooLarge { actual, cap } => {
                format!("after-file-too-large: {actual} bytes exceeds cap {cap}")
            }
            Self::AfterFileIo(m) => format!("after-file-io-error: {m}"),
            Self::TargetResolveRefused { marker, kind } => {
                format!("{kind}: resolver refused with marker {marker}")
            }
            Self::BeforeReadIo(m) => format!("before-read-io-error: {m}"),
            Self::CheckpointFailed(m) => format!("checkpoint-failed: {m}"),
            Self::PreviewBuildFailed(m) => format!("preview-build-failed: {m}"),
            Self::PayloadIo(m) => format!("payload-io-error: {m}"),
            Self::PayloadVerifyMismatch { expected, actual } => {
                format!("payload-verify-mismatch: expected after_sha256={expected} actual={actual}")
            }
            Self::PreviewBundleIo(m) => format!("preview-bundle-io-error: {m}"),
        }
    }
}

/// Lowercase hex SHA-256 of `bytes`. Local to the CLI because the
/// runner's `sha256_hex` is crate-private; this implementation MUST
/// produce byte-identical output for every input so the verify
/// round-trip below catches any drift between the operator-supplied
/// after-bytes and the on-disk payload.
fn cli_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
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

/// Format a UTC timestamp as `YYYY-MM-DDTHH:MM:SS.<nanos>Z` without
/// pulling in chrono. Caller-owned format per
/// [`a2_plan_runner::PreviewInputs::created_at_utc`] — the canonical
/// preview hash treats this as an opaque string. Uses the existing
/// `civil_from_days` Hinnant implementation already defined below to
/// avoid a duplicate-symbol clash.
fn now_utc_rfc3339_nanos() -> String {
    let now = std::time::SystemTime::now()
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

/// Inspect the after-file path with `symlink_metadata` and refuse on
/// any non-regular kind (directory, symlink, socket, FIFO, char/block
/// device). Returns the byte length on success.
fn inspect_after_file(path: &Path) -> Result<u64, PreviewBundleRefusal> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PreviewBundleRefusal::AfterFileMissing(format!(
                "{}: not found",
                path.display()
            )));
        }
        Err(e) => {
            return Err(PreviewBundleRefusal::AfterFileIo(format!(
                "{}: {e}",
                path.display()
            )));
        }
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        return Err(PreviewBundleRefusal::AfterFileSymlink);
    }
    if !ft.is_file() {
        return Err(PreviewBundleRefusal::AfterFileNotRegular);
    }
    Ok(meta.len())
}

/// Create a directory with best-effort 0700 permissions on Unix. The
/// parent chain is created idempotently. Mirrors the runner's
/// `create_dir_recursive_0700` semantics without re-exporting it.
fn create_dir_0700(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        // Best-effort; ignore "operation not permitted" so cross-FS
        // tempdir tests on platforms that reject chmod still pass.
        let _ = std::fs::set_permissions(path, perms);
    }
    Ok(())
}

/// Write `bytes` to `path` atomically (tmp + rename) with best-effort
/// 0600 permissions on Unix.
fn write_file_0600_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
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
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Operator entry point for `claw plan preview-bundle <workspace-root>
/// <target-relative-path> <after-file>`.
///
/// Performs:
///
/// 1. Workspace-root canonicalization (must be a directory).
/// 2. After-file inspection (regular, non-symlink, ≤ payload cap).
/// 3. After-file read into memory.
/// 4. Target resolution through Slice-1
///    [`a2_plan_runner::write_runtime::resolve_write_target`].
/// 5. Before-bytes read iff the resolved target already exists.
/// 6. Checkpoint creation through Slice-2
///    [`a2_plan_runner::CheckpointStore::create_checkpoint`] under a
///    freshly generated run-id ULID.
/// 7. Preview construction through Slice-3a
///    [`a2_plan_runner::build_preview`].
/// 8. Payload artifact write (atomic `after.bin` + sidecar
///    `after.sha256`) under
///    `<workspace-root>/.claw/l2b-payloads/<run-id>/<step-id>/`.
/// 9. Payload verify (re-read `after.bin`, recompute sha256, compare
///    against `preview_record.after_sha256`).
/// 10. Preview-bundle write (atomic) under
///     `<workspace-root>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`.
/// 11. Single JSON envelope on `stdout`.
///
/// Returns:
///
/// * `0` on success.
/// * [`EXIT_PREVIEW_BUNDLE_REFUSED`] on any refusal arm; the structured
///   envelope on stdout carries the specific refusal token.
fn run_plan_preview_bundle(
    workspace_root: &Path,
    target_relative_path: &str,
    after_file: &Path,
    stdout: &mut dyn Write,
) -> i32 {
    match try_run_plan_preview_bundle(workspace_root, target_relative_path, after_file) {
        Ok(envelope) => {
            if let Err(e) = serde_json::to_writer(&mut *stdout, &envelope) {
                eprintln!("claw plan preview-bundle: failed to write envelope: {e}");
                return EXIT_PREVIEW_BUNDLE_REFUSED;
            }
            let _ = stdout.write_all(b"\n");
            0
        }
        Err(refusal) => emit_preview_bundle_refusal(&refusal, stdout),
    }
}

/// Successful generator envelope. Mirrors the
/// `a2-l2b-preview-bundle-generator-result.v1` contract documented in
/// the lane plan.
///
/// `is_binary`/`is_redacted`/`is_truncated` come from the embedded
/// [`a2_plan_runner::PreviewRecord`] flags; they MUST stay surfaced as
/// separate booleans so operators can refuse non-approvable previews
/// without re-parsing the embedded bundle.
#[derive(Debug, serde::Serialize)]
#[allow(clippy::struct_excessive_bools)]
struct PreviewBundleGeneratorResultV1 {
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

/// Refusal envelope. Same `schema_version`, `ok = false`, structured
/// reason fields. No leakage of internal stack traces.
#[derive(Debug, serde::Serialize)]
struct PreviewBundleGeneratorRefusalV1<'a> {
    schema_version: &'a str,
    ok: bool,
    refusal: &'a str,
    reason: String,
    audit_markers: Vec<&'static str>,
}

fn emit_preview_bundle_refusal(refusal: &PreviewBundleRefusal, stdout: &mut dyn Write) -> i32 {
    let envelope = PreviewBundleGeneratorRefusalV1 {
        schema_version: PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1,
        ok: false,
        refusal: refusal.short(),
        reason: refusal.reason(),
        audit_markers: vec!["a2-l2b-preview-bundle-refused"],
    };
    if let Err(e) = serde_json::to_writer(&mut *stdout, &envelope) {
        eprintln!("claw plan preview-bundle: failed to write refusal envelope: {e}");
    }
    let _ = stdout.write_all(b"\n");
    EXIT_PREVIEW_BUNDLE_REFUSED
}

#[allow(clippy::too_many_lines)]
fn try_run_plan_preview_bundle(
    workspace_root: &Path,
    target_relative_path: &str,
    after_file: &Path,
) -> Result<PreviewBundleGeneratorResultV1, PreviewBundleRefusal> {
    // 1. Workspace-root canonicalize + must-be-directory.
    let workspace_root_canonical = workspace_root
        .canonicalize()
        .map_err(|e| PreviewBundleRefusal::WorkspaceRootInvalid(format!("{e}")))?;
    let workspace_meta = std::fs::symlink_metadata(&workspace_root_canonical)
        .map_err(|e| PreviewBundleRefusal::WorkspaceRootInvalid(format!("{e}")))?;
    if !workspace_meta.is_dir() {
        return Err(PreviewBundleRefusal::WorkspaceRootInvalid(
            "not a directory".to_string(),
        ));
    }

    // 2. After-file inspection (regular, not symlink, not too big).
    let after_size = inspect_after_file(after_file)?;
    if after_size > a2_plan_runner::MAX_APPROVED_PAYLOAD_BYTES {
        return Err(PreviewBundleRefusal::AfterFileTooLarge {
            actual: after_size,
            cap: a2_plan_runner::MAX_APPROVED_PAYLOAD_BYTES,
        });
    }

    // 3. Read after-file bytes.
    let after_bytes = std::fs::read(after_file)
        .map_err(|e| PreviewBundleRefusal::AfterFileIo(format!("{}: {e}", after_file.display())))?;
    // Defense-in-depth: re-check size after the read (race-free
    // boundary against TOCTOU on the inspect step).
    let after_len_u64 = after_bytes.len() as u64;
    if after_len_u64 > a2_plan_runner::MAX_APPROVED_PAYLOAD_BYTES {
        return Err(PreviewBundleRefusal::AfterFileTooLarge {
            actual: after_len_u64,
            cap: a2_plan_runner::MAX_APPROVED_PAYLOAD_BYTES,
        });
    }

    // 4. Resolve target through Slice-1.
    let write_target = a2_plan_schema::WriteTarget {
        path: target_relative_path.to_string(),
        create_if_absent: true,
    };
    let resolved = a2_plan_runner::write_runtime::resolve_write_target(
        &workspace_root_canonical,
        &write_target,
    )
    .map_err(|refusal| PreviewBundleRefusal::TargetResolveRefused {
        marker: refusal.marker(),
        kind: write_target_refusal_kind(&refusal),
    })?;

    // 5. Read before-bytes only if the target already exists.
    let before_bytes: Option<Vec<u8>> = if resolved.already_exists {
        Some(std::fs::read(&resolved.absolute).map_err(|e| {
            PreviewBundleRefusal::BeforeReadIo(format!("{}: {e}", resolved.absolute.display()))
        })?)
    } else {
        None
    };

    // 6. Capture a Slice-2 checkpoint under a freshly-generated run-id.
    let store = a2_plan_runner::CheckpointStore::new_with_generated_run_id(
        workspace_root_canonical.clone(),
    );
    let run_id_str = store.run_id().to_string();
    let step_id = PREVIEW_BUNDLE_GENERATOR_STEP_ID;
    let target_relative = Path::new(target_relative_path);
    let handle = store
        .create_checkpoint(step_id, &resolved.absolute, target_relative)
        .map_err(|e| PreviewBundleRefusal::CheckpointFailed(format!("{e}")))?;

    // 7. Build the Slice-3a preview.
    let inputs = a2_plan_runner::PreviewInputs {
        step_id,
        target_relative_path: target_relative,
        target_absolute_path: &resolved.absolute,
        before: before_bytes.as_deref(),
        after: &after_bytes,
        checkpoint_run_id: store.run_id(),
        checkpoint_step_id: step_id,
        created_at_utc: &now_utc_rfc3339_nanos(),
    };
    let (record, display) = a2_plan_runner::build_preview(&inputs)
        .map_err(|e| PreviewBundleRefusal::PreviewBuildFailed(format!("{e}")))?;

    // 8. Write the payload artifact (atomic) + sha256 sidecar.
    let payload_dir = workspace_root_canonical
        .join(PREVIEW_BUNDLE_PAYLOAD_ROOT_REL)
        .join(&run_id_str)
        .join(step_id);
    create_dir_0700(&payload_dir)
        .map_err(|e| PreviewBundleRefusal::PayloadIo(format!("{}: {e}", payload_dir.display())))?;
    let payload_path = payload_dir.join("after.bin");
    write_file_0600_atomic(&payload_path, &after_bytes)
        .map_err(|e| PreviewBundleRefusal::PayloadIo(format!("{}: {e}", payload_path.display())))?;
    let payload_sha256_path = payload_dir.join("after.sha256");
    let payload_sha256_content = format!("{}\n", record.after_sha256);
    write_file_0600_atomic(&payload_sha256_path, payload_sha256_content.as_bytes()).map_err(
        |e| PreviewBundleRefusal::PayloadIo(format!("{}: {e}", payload_sha256_path.display())),
    )?;

    // 9. Verify the on-disk payload's bytes still hash to
    //    `record.after_sha256`. This is the round-trip that catches
    //    any drift between the operator's after-file and the runner-
    //    owned copy.
    let payload_bytes_redux = std::fs::read(&payload_path)
        .map_err(|e| PreviewBundleRefusal::PayloadIo(format!("{}: {e}", payload_path.display())))?;
    let actual_sha = cli_sha256_hex(&payload_bytes_redux);
    if actual_sha != record.after_sha256 {
        return Err(PreviewBundleRefusal::PayloadVerifyMismatch {
            expected: record.after_sha256.clone(),
            actual: actual_sha,
        });
    }

    // 10. Write the preview-bundle.json (atomic) under runner-owned
    //     storage. Shape mirrors `PreviewBundleV1` so this bundle is
    //     consumable by `claw plan approve` without any schema
    //     widening.
    let bundle_dir = workspace_root_canonical
        .join(PREVIEW_BUNDLE_BUNDLE_ROOT_REL)
        .join(&run_id_str)
        .join(step_id);
    create_dir_0700(&bundle_dir).map_err(|e| {
        PreviewBundleRefusal::PreviewBundleIo(format!("{}: {e}", bundle_dir.display()))
    })?;
    let preview_bundle_path = bundle_dir.join("preview-bundle.json");
    let bundle_out = PreviewBundleV1Output {
        schema_version: PREVIEW_BUNDLE_SCHEMA_V1,
        preview_record: &record,
        preview_display: &display,
        checkpoint_baseline_unchanged: true,
    };
    let bundle_bytes = serde_json::to_vec_pretty(&bundle_out)
        .map_err(|e| PreviewBundleRefusal::PreviewBundleIo(format!("serde_json error: {e}")))?;
    write_file_0600_atomic(&preview_bundle_path, &bundle_bytes).map_err(|e| {
        PreviewBundleRefusal::PreviewBundleIo(format!("{}: {e}", preview_bundle_path.display()))
    })?;

    Ok(PreviewBundleGeneratorResultV1 {
        schema_version: PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1,
        ok: true,
        run_id: run_id_str,
        step_id: step_id.to_string(),
        preview_id: record.preview_id.clone(),
        target_relative_path: target_relative_path.to_string(),
        preview_bundle_path,
        payload_path,
        payload_sha256_path,
        payload_sha256: record.after_sha256.clone(),
        payload_size_bytes: after_len_u64,
        checkpoint_manifest_path: handle.manifest_path.clone(),
        is_binary: record.is_binary,
        is_redacted: record.is_redacted,
        is_truncated: record.is_truncated,
        audit_markers: vec![
            PREVIEW_BUNDLE_MARKER_CHECKPOINT_WRITTEN,
            PREVIEW_BUNDLE_MARKER_PAYLOAD_CAPTURED,
            PREVIEW_BUNDLE_MARKER_CREATED,
        ],
    })
}

fn write_target_refusal_kind(
    refusal: &a2_plan_runner::write_runtime::WriteTargetRefusal,
) -> &'static str {
    use a2_plan_runner::write_runtime::WriteTargetRefusal;
    match refusal {
        WriteTargetRefusal::PathEscape => "target-path-escape",
        WriteTargetRefusal::ParentMissing => "target-parent-missing",
        WriteTargetRefusal::DenyComponent => "target-deny-component",
        WriteTargetRefusal::DenyGlobFilename => "target-deny-glob-filename",
        WriteTargetRefusal::SymlinkTarget => "target-symlink-target",
        WriteTargetRefusal::SymlinkParent => "target-symlink-parent",
    }
}

// =========================================================================
// END A2-L2b Slice L2b-CLI-Preview-Bundle — scope sentinel
// =========================================================================
//
// The source-grep tests in `plan_preview_bundle_tests` use this sentinel
// to bound the implementation region they scan for forbidden APIs. Do not
// move or rename it without updating those tests.

// =========================================================================
// A2-L2b Slice L2b-CLI-Apply-Bundle-Generator — `claw plan apply-bundle`
// =========================================================================
//
// Scope:
//
// * Reads ONE preview-generator result JSON file from disk
//   (`a2-l2b-preview-bundle-generator-result.v1`).
// * Reads ONE approval-result JSON file from disk
//   (`a2-l2b-approval-result.v1`).
// * Loads the preview bundle JSON referenced by the preview-generator
//   result.
// * Loads + verifies the payload artifact + its sidecar against both the
//   preview-generator result and the preview bundle's `PreviewRecord`.
// * Loads + verifies the checkpoint manifest against the preview result
//   and the preview record's `before_sha256`.
// * Validates the full authority chain — schema versions, identity bindings
//   (`step_id`, `preview_sha256`, `preview_id`, `target_relative_path`,
//   `after_sha256`, sizes), and the approvability of the preview.
// * Writes ONE `apply-bundle.json` artifact adjacent to the preview bundle
//   (`<workspace-root>/.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json`).
// * Emits exactly ONE JSON envelope on stdout
//   (`a2-l2b-apply-bundle-generator-result.v1`).
//
// Hard contract:
//
// * NEVER executes `claw plan apply`.
// * NEVER calls `a2_plan_runner::execute_write`.
// * NEVER calls `a2_plan_runner::bind_after_bytes`.
// * NEVER wires `a2_plan_runner::run_plan` workspace-write.
// * NEVER mutates the target file.
// * NEVER mutates any artifact under `.claw/l2b-payloads` or
//   `.claw/l2b-checkpoints` — read-only by construction.
// * NEVER calls the broker, model, or any network.
// * NEVER spawns a subprocess.
// * NEVER reads stdin.
// * NEVER accepts pre-approval / batch flags (`--yes`, `--auto`,
//   `--force`, `--allow-write`, `--preapproved`, `--batch`).
// * NEVER prints raw payload bytes to stdout or stderr.
// * NEVER writes outside the runner-owned preview-bundle leaf directory.

/// Pinned generator-result schema version emitted on stdout.
const APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1: &str = "a2-l2b-apply-bundle-generator-result.v1";

/// Audit markers emitted on the success envelope.
const APPLY_BUNDLE_GEN_MARKER_APPROVAL_VALIDATED: &str = "a2-l2b-approval-result-validated";
const APPLY_BUNDLE_GEN_MARKER_CREATED: &str = "a2-l2b-apply-bundle-created";

/// Audit marker emitted on the refusal envelope.
const APPLY_BUNDLE_GEN_MARKER_REFUSED: &str = "a2-l2b-apply-bundle-refused";

/// CLI exit code for generator refusals.
const EXIT_APPLY_BUNDLE_GEN_REFUSED: i32 = 5;

/// Runner-owned checkpoint artifact root, joined with `run-id` /
/// `step-id` / `manifest.json`. Mirrors the checkpoint store layout.
const APPLY_BUNDLE_GEN_CHECKPOINT_ROOT_REL: &str = ".claw/l2b-checkpoints";

/// On-disk shape of the preview-generator result. Mirrors
/// [`PreviewBundleGeneratorResultV1`] (`Serialize`-only) so this
/// generator round-trips the file `claw plan preview-bundle` emits
/// without any schema widening.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
struct PreviewGenResultRead {
    schema_version: String,
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
    #[allow(dead_code)]
    is_binary: bool,
    #[allow(dead_code)]
    is_redacted: bool,
    #[allow(dead_code)]
    is_truncated: bool,
    #[allow(dead_code)]
    audit_markers: Vec<String>,
}

/// On-disk shape of the approval result. Mirrors what `claw plan approve`
/// emits on stdout (the `Approved` and `Refused` arms — `bundle_rejected`
/// is rejected at the decision-check step). Re-serializing this exact
/// struct into the apply bundle's `approval_result` field guarantees the
/// downstream `claw plan apply` parser accepts it under its
/// `deny_unknown_fields` constraint.
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ApprovalResultRead {
    schema_version: String,
    decision: String,
    preview_id: String,
    step_id: String,
    preview_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint_baseline_unchanged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exit_code_hint: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audit_markers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// Output shape of the apply-bundle artifact. Field order + names mirror
/// the `Deserialize`-only [`ApplyBundleV1`] consumed by `claw plan apply`
/// so the artifact this generator writes round-trips into apply without
/// any schema widening.
#[derive(Debug, serde::Serialize)]
struct ApplyBundleV1Output<'a> {
    schema_version: &'static str,
    workspace_root: &'a Path,
    target_relative_path: &'a str,
    preview_record: &'a a2_plan_runner::PreviewRecord,
    approval_result: &'a ApprovalResultRead,
    checkpoint: ApplyBundleCheckpointOut<'a>,
    payload: ApplyBundlePayloadOut<'a>,
}

#[derive(Debug, serde::Serialize)]
struct ApplyBundleCheckpointOut<'a> {
    manifest_path: &'a Path,
}

#[derive(Debug, serde::Serialize)]
struct ApplyBundlePayloadOut<'a> {
    kind: &'a str,
    path: &'a Path,
    after_sha256: &'a str,
    after_size_bytes: u64,
}

/// Success envelope. Single JSON line on stdout.
#[derive(Debug, serde::Serialize)]
struct ApplyBundleGenResultV1 {
    schema_version: &'static str,
    ok: bool,
    run_id: String,
    step_id: String,
    preview_id: String,
    target_relative_path: String,
    apply_bundle_path: PathBuf,
    preview_bundle_path: PathBuf,
    approval_result_path: PathBuf,
    payload_path: PathBuf,
    payload_sha256: String,
    payload_size_bytes: u64,
    checkpoint_manifest_path: PathBuf,
    audit_markers: Vec<&'static str>,
}

/// Refusal envelope. Same `schema_version`, `ok = false`, structured
/// reason fields, single audit marker.
#[derive(Debug, serde::Serialize)]
struct ApplyBundleGenRefusalV1<'a> {
    schema_version: &'a str,
    ok: bool,
    refusal: &'a str,
    reason: String,
    audit_markers: Vec<&'static str>,
}

/// Generator refusal causes. Each variant maps to a stable short string
/// in the result envelope.
#[derive(Debug)]
enum ApplyBundleGenRefusal {
    PreviewResultIo(String),
    PreviewResultJson(String),
    PreviewResultSchemaMismatch { actual: String },
    PreviewResultNotOk,
    ApprovalResultIo(String),
    ApprovalResultJson(String),
    ApprovalResultSchemaMismatch { actual: String },
    ApprovalDecisionNotApproved { actual: String },
    ApprovalStepIdMismatch,
    ApprovalPreviewIdMismatch,
    PreviewBundleIo(String),
    PreviewBundleJson(String),
    PreviewBundleSchemaMismatch { actual: String },
    PreviewBundlePathLayoutInvalid(String),
    PreviewRecordPreviewIdMismatch,
    PreviewRecordStepIdMismatch,
    PreviewRecordTargetPathMismatch,
    PreviewRecordAfterShaMismatch,
    ApprovalPreviewShaMismatch,
    PreviewNonApprovable { kind: &'static str },
    PayloadPathLayoutInvalid(String),
    PayloadSidecarPathLayoutInvalid(String),
    CheckpointManifestPathLayoutInvalid(String),
    PayloadIo(String),
    PayloadSidecarIo(String),
    PayloadSidecarFormatInvalid,
    PayloadSidecarHashMismatch,
    PayloadSizeMismatch { declared: u64, actual: u64 },
    PayloadHashMismatchPreviewResult { expected: String, actual: String },
    CheckpointManifestIo(String),
    CheckpointManifestJson(String),
    CheckpointManifestStepIdMismatch,
    CheckpointManifestTargetPathMismatch,
    CheckpointManifestPreShaMismatch,
    ApplyBundleIo(String),
    ApplyBundleExistsDivergent,
}

impl ApplyBundleGenRefusal {
    fn short(&self) -> &'static str {
        match self {
            Self::PreviewResultIo(_) => "preview-result-io-error",
            Self::PreviewResultJson(_) => "preview-result-json-parse-error",
            Self::PreviewResultSchemaMismatch { .. } => "preview-result-schema-version-mismatch",
            Self::PreviewResultNotOk => "preview-result-not-ok",
            Self::ApprovalResultIo(_) => "approval-result-io-error",
            Self::ApprovalResultJson(_) => "approval-result-json-parse-error",
            Self::ApprovalResultSchemaMismatch { .. } => "approval-result-schema-version-mismatch",
            Self::ApprovalDecisionNotApproved { .. } => "approval-decision-not-approved",
            Self::ApprovalStepIdMismatch => "approval-step-id-mismatch",
            Self::ApprovalPreviewIdMismatch => "approval-preview-id-mismatch",
            Self::PreviewBundleIo(_) => "preview-bundle-io-error",
            Self::PreviewBundleJson(_) => "preview-bundle-json-parse-error",
            Self::PreviewBundleSchemaMismatch { .. } => "preview-bundle-schema-version-mismatch",
            Self::PreviewBundlePathLayoutInvalid(_) => "preview-bundle-path-layout-invalid",
            Self::PreviewRecordPreviewIdMismatch => "preview-record-preview-id-mismatch",
            Self::PreviewRecordStepIdMismatch => "preview-record-step-id-mismatch",
            Self::PreviewRecordTargetPathMismatch => "preview-record-target-path-mismatch",
            Self::PreviewRecordAfterShaMismatch => "preview-record-after-sha-mismatch",
            Self::ApprovalPreviewShaMismatch => "approval-preview-sha-mismatch",
            Self::PreviewNonApprovable { kind } => kind,
            Self::PayloadPathLayoutInvalid(_) => "payload-path-layout-invalid",
            Self::PayloadSidecarPathLayoutInvalid(_) => "payload-sidecar-path-layout-invalid",
            Self::CheckpointManifestPathLayoutInvalid(_) => {
                "checkpoint-manifest-path-layout-invalid"
            }
            Self::PayloadIo(_) => "payload-io-error",
            Self::PayloadSidecarIo(_) => "payload-sidecar-io-error",
            Self::PayloadSidecarFormatInvalid => "payload-sidecar-format-invalid",
            Self::PayloadSidecarHashMismatch => "payload-sidecar-hash-mismatch",
            Self::PayloadSizeMismatch { .. } => "payload-size-mismatch",
            Self::PayloadHashMismatchPreviewResult { .. } => "payload-hash-mismatch",
            Self::CheckpointManifestIo(_) => "checkpoint-manifest-io-error",
            Self::CheckpointManifestJson(_) => "checkpoint-manifest-parse-error",
            Self::CheckpointManifestStepIdMismatch => "checkpoint-manifest-step-id-mismatch",
            Self::CheckpointManifestTargetPathMismatch => {
                "checkpoint-manifest-target-path-mismatch"
            }
            Self::CheckpointManifestPreShaMismatch => "checkpoint-manifest-pre-sha-mismatch",
            Self::ApplyBundleIo(_) => "apply-bundle-io-error",
            Self::ApplyBundleExistsDivergent => "apply-bundle-exists-divergent",
        }
    }

    fn reason(&self) -> String {
        match self {
            Self::PreviewResultIo(m) => format!("preview-result-io-error: {m}"),
            Self::PreviewResultJson(m) => format!("preview-result-json-parse-error: {m}"),
            Self::PreviewResultSchemaMismatch { actual } => {
                format!("preview-result-schema-version-mismatch: got {actual}")
            }
            Self::PreviewResultNotOk => "preview-result-not-ok: ok=false".to_string(),
            Self::ApprovalResultIo(m) => format!("approval-result-io-error: {m}"),
            Self::ApprovalResultJson(m) => format!("approval-result-json-parse-error: {m}"),
            Self::ApprovalResultSchemaMismatch { actual } => {
                format!("approval-result-schema-version-mismatch: got {actual}")
            }
            Self::ApprovalDecisionNotApproved { actual } => {
                format!("approval-decision-not-approved: got {actual:?}")
            }
            Self::ApprovalStepIdMismatch => "approval-step-id-mismatch".to_string(),
            Self::ApprovalPreviewIdMismatch => "approval-preview-id-mismatch".to_string(),
            Self::PreviewBundleIo(m) => format!("preview-bundle-io-error: {m}"),
            Self::PreviewBundleJson(m) => format!("preview-bundle-json-parse-error: {m}"),
            Self::PreviewBundleSchemaMismatch { actual } => {
                format!("preview-bundle-schema-version-mismatch: got {actual}")
            }
            Self::PreviewBundlePathLayoutInvalid(m) => {
                format!("preview-bundle-path-layout-invalid: {m}")
            }
            Self::PreviewRecordPreviewIdMismatch => {
                "preview-record-preview-id-mismatch".to_string()
            }
            Self::PreviewRecordStepIdMismatch => "preview-record-step-id-mismatch".to_string(),
            Self::PreviewRecordTargetPathMismatch => {
                "preview-record-target-path-mismatch".to_string()
            }
            Self::PreviewRecordAfterShaMismatch => "preview-record-after-sha-mismatch".to_string(),
            Self::ApprovalPreviewShaMismatch => "approval-preview-sha-mismatch".to_string(),
            Self::PreviewNonApprovable { kind } => format!("preview-non-approvable: {kind}"),
            Self::PayloadPathLayoutInvalid(m) => format!("payload-path-layout-invalid: {m}"),
            Self::PayloadSidecarPathLayoutInvalid(m) => {
                format!("payload-sidecar-path-layout-invalid: {m}")
            }
            Self::CheckpointManifestPathLayoutInvalid(m) => {
                format!("checkpoint-manifest-path-layout-invalid: {m}")
            }
            Self::PayloadIo(m) => format!("payload-io-error: {m}"),
            Self::PayloadSidecarIo(m) => format!("payload-sidecar-io-error: {m}"),
            Self::PayloadSidecarFormatInvalid => "payload-sidecar-format-invalid".to_string(),
            Self::PayloadSidecarHashMismatch => "payload-sidecar-hash-mismatch".to_string(),
            Self::PayloadSizeMismatch { declared, actual } => {
                format!("payload-size-mismatch: declared={declared} actual={actual}")
            }
            Self::PayloadHashMismatchPreviewResult { expected, actual } => {
                format!("payload-hash-mismatch: expected={expected} actual={actual}")
            }
            Self::CheckpointManifestIo(m) => format!("checkpoint-manifest-io-error: {m}"),
            Self::CheckpointManifestJson(m) => format!("checkpoint-manifest-parse-error: {m}"),
            Self::CheckpointManifestStepIdMismatch => {
                "checkpoint-manifest-step-id-mismatch".to_string()
            }
            Self::CheckpointManifestTargetPathMismatch => {
                "checkpoint-manifest-target-path-mismatch".to_string()
            }
            Self::CheckpointManifestPreShaMismatch => {
                "checkpoint-manifest-pre-sha-mismatch".to_string()
            }
            Self::ApplyBundleIo(m) => format!("apply-bundle-io-error: {m}"),
            Self::ApplyBundleExistsDivergent => {
                "apply-bundle-exists-divergent: existing apply-bundle.json differs from \
                 the canonical serialization for the same authority chain"
                    .to_string()
            }
        }
    }
}

/// Walk `path` upward, confirming each component matches `expected`
/// from the leaf up. Returns the ancestor directory above the last
/// matched component on success. Used to validate the four runner-owned
/// artifact paths share the same workspace-root parent.
fn strip_expected_suffix(path: &Path, expected: &[&str]) -> Result<PathBuf, String> {
    let mut cur = path.to_path_buf();
    for segment in expected {
        let name = cur
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("path has no file_name at {}", cur.display()))?
            .to_string();
        if name != *segment {
            return Err(format!(
                "expected segment {segment:?} at {}, got {name:?}",
                cur.display()
            ));
        }
        let parent = cur
            .parent()
            .ok_or_else(|| format!("path has no parent at {}", cur.display()))?
            .to_path_buf();
        cur = parent;
    }
    Ok(cur)
}

/// Read a JSON file from disk into the requested type, returning the
/// raw read error mapped via `io_err` and parse error mapped via
/// `json_err` so callers can surface a stable refusal token.
fn read_json_file<T, IoErr, JsonErr>(
    path: &Path,
    io_err: IoErr,
    json_err: JsonErr,
) -> Result<T, ApplyBundleGenRefusal>
where
    T: for<'de> serde::Deserialize<'de>,
    IoErr: FnOnce(String) -> ApplyBundleGenRefusal,
    JsonErr: FnOnce(String) -> ApplyBundleGenRefusal,
{
    let bytes = std::fs::read(path).map_err(|e| io_err(format!("{}: {e}", path.display())))?;
    serde_json::from_slice::<T>(&bytes).map_err(|e| json_err(format!("{}: {e}", path.display())))
}

/// Stream the file at `path` through SHA-256 and return the lowercase
/// hex digest. Streaming avoids loading the full payload into memory
/// just for the hash check.
fn sha256_file_hex(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let out = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in &out {
        write!(hex, "{byte:02x}").expect("writing to String never fails");
    }
    Ok(hex)
}

/// Parse a `<hex>\n` (or `<hex>`) sidecar file, returning just the hex
/// portion. Refuses anything else so an operator who tampered with the
/// sidecar can't slip a different hash past the cross-check.
fn parse_payload_sidecar(content: &str) -> Result<String, ApplyBundleGenRefusal> {
    let mut lines = content.split('\n');
    let first = lines
        .next()
        .ok_or(ApplyBundleGenRefusal::PayloadSidecarFormatInvalid)?;
    let trailing = lines.next().unwrap_or("");
    let rest: String = lines.collect();
    if !trailing.is_empty() || !rest.is_empty() {
        return Err(ApplyBundleGenRefusal::PayloadSidecarFormatInvalid);
    }
    if first.len() != 64 {
        return Err(ApplyBundleGenRefusal::PayloadSidecarFormatInvalid);
    }
    if !first
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(ApplyBundleGenRefusal::PayloadSidecarFormatInvalid);
    }
    Ok(first.to_string())
}

fn run_plan_apply_bundle(
    preview_result_path: &Path,
    approval_result_path: &Path,
    stdout: &mut dyn Write,
) -> i32 {
    match try_run_plan_apply_bundle(preview_result_path, approval_result_path) {
        Ok(envelope) => {
            if let Err(e) = serde_json::to_writer(&mut *stdout, &envelope) {
                eprintln!("claw plan apply-bundle: failed to write envelope: {e}");
                return EXIT_APPLY_BUNDLE_GEN_REFUSED;
            }
            let _ = stdout.write_all(b"\n");
            0
        }
        Err(refusal) => emit_apply_bundle_gen_refusal(&refusal, stdout),
    }
}

fn emit_apply_bundle_gen_refusal(refusal: &ApplyBundleGenRefusal, stdout: &mut dyn Write) -> i32 {
    let envelope = ApplyBundleGenRefusalV1 {
        schema_version: APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1,
        ok: false,
        refusal: refusal.short(),
        reason: refusal.reason(),
        audit_markers: vec![APPLY_BUNDLE_GEN_MARKER_REFUSED],
    };
    if let Err(e) = serde_json::to_writer(&mut *stdout, &envelope) {
        eprintln!("claw plan apply-bundle: failed to write refusal envelope: {e}");
    }
    let _ = stdout.write_all(b"\n");
    EXIT_APPLY_BUNDLE_GEN_REFUSED
}

#[allow(clippy::too_many_lines)]
fn try_run_plan_apply_bundle(
    preview_result_path: &Path,
    approval_result_path: &Path,
) -> Result<ApplyBundleGenResultV1, ApplyBundleGenRefusal> {
    // 1. Read + validate the preview-generator result.
    let preview_result: PreviewGenResultRead = read_json_file(
        preview_result_path,
        ApplyBundleGenRefusal::PreviewResultIo,
        ApplyBundleGenRefusal::PreviewResultJson,
    )?;
    if preview_result.schema_version != PREVIEW_BUNDLE_GENERATOR_RESULT_SCHEMA_V1 {
        return Err(ApplyBundleGenRefusal::PreviewResultSchemaMismatch {
            actual: preview_result.schema_version,
        });
    }
    if !preview_result.ok {
        return Err(ApplyBundleGenRefusal::PreviewResultNotOk);
    }

    // 2. Read + validate the approval result.
    let approval: ApprovalResultRead = read_json_file(
        approval_result_path,
        ApplyBundleGenRefusal::ApprovalResultIo,
        ApplyBundleGenRefusal::ApprovalResultJson,
    )?;
    if approval.schema_version != APPROVAL_RESULT_SCHEMA_V1 {
        return Err(ApplyBundleGenRefusal::ApprovalResultSchemaMismatch {
            actual: approval.schema_version,
        });
    }
    if approval.decision != "approved" {
        return Err(ApplyBundleGenRefusal::ApprovalDecisionNotApproved {
            actual: approval.decision,
        });
    }
    if approval.step_id != preview_result.step_id {
        return Err(ApplyBundleGenRefusal::ApprovalStepIdMismatch);
    }
    if approval.preview_id != preview_result.preview_id {
        return Err(ApplyBundleGenRefusal::ApprovalPreviewIdMismatch);
    }

    // 3. Read + validate the preview bundle. Use the same `PreviewBundleV1`
    //    parser `claw plan approve` uses, so any tampered binding fails
    //    the same way it would there.
    let preview_bundle: PreviewBundleV1 = read_json_file(
        &preview_result.preview_bundle_path,
        ApplyBundleGenRefusal::PreviewBundleIo,
        ApplyBundleGenRefusal::PreviewBundleJson,
    )?;
    if preview_bundle.schema_version != PREVIEW_BUNDLE_SCHEMA_V1 {
        return Err(ApplyBundleGenRefusal::PreviewBundleSchemaMismatch {
            actual: preview_bundle.schema_version,
        });
    }

    // 3a. PreviewRecord identity bindings against the preview-generator
    //     result.
    let record = &preview_bundle.preview_record;
    if record.preview_id != preview_result.preview_id {
        return Err(ApplyBundleGenRefusal::PreviewRecordPreviewIdMismatch);
    }
    if record.step_id != preview_result.step_id {
        return Err(ApplyBundleGenRefusal::PreviewRecordStepIdMismatch);
    }
    if record.target_relative_path_sanitized != preview_result.target_relative_path {
        return Err(ApplyBundleGenRefusal::PreviewRecordTargetPathMismatch);
    }
    if record.after_sha256 != preview_result.payload_sha256 {
        return Err(ApplyBundleGenRefusal::PreviewRecordAfterShaMismatch);
    }

    // 3b. Approval ↔ record `preview_sha256` binding.
    if approval.preview_sha256 != record.preview_sha256 {
        return Err(ApplyBundleGenRefusal::ApprovalPreviewShaMismatch);
    }

    // 3c. Refuse non-approvable previews — apply must never be reachable
    //     for binary / redacted / truncated payloads.
    if record.is_binary {
        return Err(ApplyBundleGenRefusal::PreviewNonApprovable {
            kind: "preview-binary",
        });
    }
    if record.is_redacted {
        return Err(ApplyBundleGenRefusal::PreviewNonApprovable {
            kind: "preview-redacted",
        });
    }
    if record.is_truncated {
        return Err(ApplyBundleGenRefusal::PreviewNonApprovable {
            kind: "preview-truncated",
        });
    }

    // 4. Derive the canonical workspace root from the preview bundle's
    //    canonical path. The preview bundle path is the anchor because the
    //    `claw plan preview-bundle` generator writes it under
    //    `<workspace-root>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`.
    let preview_bundle_canonical =
        preview_result
            .preview_bundle_path
            .canonicalize()
            .map_err(|e| {
                ApplyBundleGenRefusal::PreviewBundlePathLayoutInvalid(format!(
                    "{}: {e}",
                    preview_result.preview_bundle_path.display()
                ))
            })?;
    let workspace_root_canonical = strip_expected_suffix(
        &preview_bundle_canonical,
        &[
            "preview-bundle.json",
            &preview_result.step_id,
            &preview_result.run_id,
            "l2b-preview-bundles",
            ".claw",
        ],
    )
    .map_err(ApplyBundleGenRefusal::PreviewBundlePathLayoutInvalid)?;

    // 5. Validate each runner-owned artifact path canonicalizes to the
    //    expected location under the derived workspace root. Any operator-
    //    supplied path that escapes this layout is refused.
    let expected_payload_path = workspace_root_canonical
        .join(PREVIEW_BUNDLE_PAYLOAD_ROOT_REL)
        .join(&preview_result.run_id)
        .join(&preview_result.step_id)
        .join("after.bin");
    let actual_payload_canonical = preview_result.payload_path.canonicalize().map_err(|e| {
        ApplyBundleGenRefusal::PayloadPathLayoutInvalid(format!(
            "{}: canonicalize failed: {e}",
            preview_result.payload_path.display()
        ))
    })?;
    let expected_payload_canonical = expected_payload_path.canonicalize().map_err(|e| {
        ApplyBundleGenRefusal::PayloadPathLayoutInvalid(format!(
            "expected payload canonicalize failed: {}: {e}",
            expected_payload_path.display()
        ))
    })?;
    if actual_payload_canonical != expected_payload_canonical {
        return Err(ApplyBundleGenRefusal::PayloadPathLayoutInvalid(format!(
            "{} != {}",
            actual_payload_canonical.display(),
            expected_payload_canonical.display()
        )));
    }

    let expected_sidecar_path = workspace_root_canonical
        .join(PREVIEW_BUNDLE_PAYLOAD_ROOT_REL)
        .join(&preview_result.run_id)
        .join(&preview_result.step_id)
        .join("after.sha256");
    let actual_sidecar_canonical =
        preview_result
            .payload_sha256_path
            .canonicalize()
            .map_err(|e| {
                ApplyBundleGenRefusal::PayloadSidecarPathLayoutInvalid(format!(
                    "{}: canonicalize failed: {e}",
                    preview_result.payload_sha256_path.display()
                ))
            })?;
    let expected_sidecar_canonical = expected_sidecar_path.canonicalize().map_err(|e| {
        ApplyBundleGenRefusal::PayloadSidecarPathLayoutInvalid(format!(
            "expected sidecar canonicalize failed: {}: {e}",
            expected_sidecar_path.display()
        ))
    })?;
    if actual_sidecar_canonical != expected_sidecar_canonical {
        return Err(ApplyBundleGenRefusal::PayloadSidecarPathLayoutInvalid(
            format!(
                "{} != {}",
                actual_sidecar_canonical.display(),
                expected_sidecar_canonical.display()
            ),
        ));
    }

    let expected_manifest_path = workspace_root_canonical
        .join(APPLY_BUNDLE_GEN_CHECKPOINT_ROOT_REL)
        .join(&preview_result.run_id)
        .join(&preview_result.step_id)
        .join("manifest.json");
    let actual_manifest_canonical = preview_result
        .checkpoint_manifest_path
        .canonicalize()
        .map_err(|e| {
            ApplyBundleGenRefusal::CheckpointManifestPathLayoutInvalid(format!(
                "{}: canonicalize failed: {e}",
                preview_result.checkpoint_manifest_path.display()
            ))
        })?;
    let expected_manifest_canonical = expected_manifest_path.canonicalize().map_err(|e| {
        ApplyBundleGenRefusal::CheckpointManifestPathLayoutInvalid(format!(
            "expected manifest canonicalize failed: {}: {e}",
            expected_manifest_path.display()
        ))
    })?;
    if actual_manifest_canonical != expected_manifest_canonical {
        return Err(ApplyBundleGenRefusal::CheckpointManifestPathLayoutInvalid(
            format!(
                "{} != {}",
                actual_manifest_canonical.display(),
                expected_manifest_canonical.display()
            ),
        ));
    }

    // 6. Verify the payload file: size matches the declared size, then
    //    streaming sha256 matches both the declared hash AND the embedded
    //    PreviewRecord's `after_sha256` (already cross-checked at 3a).
    let payload_meta = std::fs::symlink_metadata(&actual_payload_canonical).map_err(|e| {
        ApplyBundleGenRefusal::PayloadIo(format!("{}: {e}", actual_payload_canonical.display()))
    })?;
    if !payload_meta.is_file() {
        return Err(ApplyBundleGenRefusal::PayloadIo(format!(
            "{}: not a regular file",
            actual_payload_canonical.display()
        )));
    }
    let actual_size = payload_meta.len();
    if actual_size != preview_result.payload_size_bytes {
        return Err(ApplyBundleGenRefusal::PayloadSizeMismatch {
            declared: preview_result.payload_size_bytes,
            actual: actual_size,
        });
    }
    let payload_hex = sha256_file_hex(&actual_payload_canonical).map_err(|e| {
        ApplyBundleGenRefusal::PayloadIo(format!("{}: {e}", actual_payload_canonical.display()))
    })?;
    if payload_hex != preview_result.payload_sha256 {
        return Err(ApplyBundleGenRefusal::PayloadHashMismatchPreviewResult {
            expected: preview_result.payload_sha256.clone(),
            actual: payload_hex,
        });
    }

    // 7. Verify the payload sidecar agrees with the disk hash.
    let sidecar_text = std::fs::read_to_string(&actual_sidecar_canonical).map_err(|e| {
        ApplyBundleGenRefusal::PayloadSidecarIo(format!(
            "{}: {e}",
            actual_sidecar_canonical.display()
        ))
    })?;
    let sidecar_hex = parse_payload_sidecar(&sidecar_text)?;
    if sidecar_hex != payload_hex {
        return Err(ApplyBundleGenRefusal::PayloadSidecarHashMismatch);
    }

    // 8. Read + validate the checkpoint manifest against the preview
    //    record's `before_sha256` plus the run-id / step-id bindings.
    let manifest: a2_plan_runner::Manifest = read_json_file(
        &actual_manifest_canonical,
        ApplyBundleGenRefusal::CheckpointManifestIo,
        ApplyBundleGenRefusal::CheckpointManifestJson,
    )?;
    if manifest.step_id != preview_result.step_id {
        return Err(ApplyBundleGenRefusal::CheckpointManifestStepIdMismatch);
    }
    if manifest.target_relative_path != preview_result.target_relative_path {
        return Err(ApplyBundleGenRefusal::CheckpointManifestTargetPathMismatch);
    }
    if manifest.pre_sha256 != record.before_sha256 {
        return Err(ApplyBundleGenRefusal::CheckpointManifestPreShaMismatch);
    }

    // 9. Serialize the apply-bundle artifact under canonical form.
    let apply_bundle_out = ApplyBundleV1Output {
        schema_version: APPLY_BUNDLE_SCHEMA_V1,
        workspace_root: &workspace_root_canonical,
        target_relative_path: &preview_result.target_relative_path,
        preview_record: record,
        approval_result: &approval,
        checkpoint: ApplyBundleCheckpointOut {
            manifest_path: &actual_manifest_canonical,
        },
        payload: ApplyBundlePayloadOut {
            kind: "file",
            path: &actual_payload_canonical,
            after_sha256: &record.after_sha256,
            after_size_bytes: actual_size,
        },
    };
    let apply_bundle_bytes = serde_json::to_vec_pretty(&apply_bundle_out)
        .map_err(|e| ApplyBundleGenRefusal::ApplyBundleIo(format!("serde_json error: {e}")))?;

    // 10. Write the apply-bundle.json adjacent to the preview bundle.
    //     If a previous run already produced a byte-identical artifact,
    //     accept it; otherwise refuse with `apply-bundle-exists-divergent`
    //     so the operator regenerates from scratch instead of silently
    //     overwriting an existing bundle.
    let apply_bundle_dir = workspace_root_canonical
        .join(PREVIEW_BUNDLE_BUNDLE_ROOT_REL)
        .join(&preview_result.run_id)
        .join(&preview_result.step_id);
    let apply_bundle_path = apply_bundle_dir.join("apply-bundle.json");

    if apply_bundle_path.exists() {
        let existing = std::fs::read(&apply_bundle_path).map_err(|e| {
            ApplyBundleGenRefusal::ApplyBundleIo(format!("{}: {e}", apply_bundle_path.display()))
        })?;
        if existing != apply_bundle_bytes {
            return Err(ApplyBundleGenRefusal::ApplyBundleExistsDivergent);
        }
    } else {
        create_dir_0700(&apply_bundle_dir).map_err(|e| {
            ApplyBundleGenRefusal::ApplyBundleIo(format!("{}: {e}", apply_bundle_dir.display()))
        })?;
        write_file_0600_atomic(&apply_bundle_path, &apply_bundle_bytes).map_err(|e| {
            ApplyBundleGenRefusal::ApplyBundleIo(format!("{}: {e}", apply_bundle_path.display()))
        })?;
    }

    Ok(ApplyBundleGenResultV1 {
        schema_version: APPLY_BUNDLE_GEN_RESULT_SCHEMA_V1,
        ok: true,
        run_id: preview_result.run_id.clone(),
        step_id: preview_result.step_id.clone(),
        preview_id: preview_result.preview_id.clone(),
        target_relative_path: preview_result.target_relative_path.clone(),
        apply_bundle_path,
        preview_bundle_path: preview_bundle_canonical,
        approval_result_path: approval_result_path.to_path_buf(),
        payload_path: actual_payload_canonical,
        payload_sha256: payload_hex,
        payload_size_bytes: actual_size,
        checkpoint_manifest_path: actual_manifest_canonical,
        audit_markers: vec![
            APPLY_BUNDLE_GEN_MARKER_APPROVAL_VALIDATED,
            APPLY_BUNDLE_GEN_MARKER_CREATED,
        ],
    })
}

// =========================================================================
// END A2-L2b Slice L2b-CLI-Apply-Bundle-Generator — scope sentinel
// =========================================================================
//
// The source-grep tests in `plan_apply_bundle_tests` use this sentinel to
// bound the implementation region they scan for forbidden APIs. Do not move
// or rename it without updating those tests.

// =========================================================================
// BEGIN A2-L2d Read-Only Artifact Inspector / Status Contract
// =========================================================================
//
// `claw plan status <workspace> [<approval-result.json>]` — read-only
// state inspector for the A2-L2b preview-to-apply chain. Emits an
// `a2-l2d-status.v1` JSON envelope on stdout. NEVER mutates state.
// NEVER calls broker, model, or Ollama. NEVER approves or applies.

/// CLI-level emitter for the A2-L2d status command. Calls the library
/// `read_status` (which performs all reads and the entire phase / STOP
/// derivation) and serializes the envelope to stdout with stable key
/// order and trailing newline. Returns the recommended process exit
/// code (0 on success, `EXIT_STATUS_REFUSED` on read-time refusal).
fn run_plan_status<W: Write>(
    workspace_root: &Path,
    approval_result_path: Option<&Path>,
    stdout: &mut W,
) -> i32 {
    let result = a2_plan_runner::read_status(workspace_root, approval_result_path);
    let json = serde_json::to_string_pretty(&result.envelope).unwrap_or_else(|_| "{}".to_string());
    let _ = writeln!(stdout, "{json}");
    result.exit_code
}

// =========================================================================
// END A2-L2d Read-Only Artifact Inspector — scope sentinel
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    DumpManifests {
        output_format: CliOutputFormat,
        manifests_dir: Option<PathBuf>,
    },
    BootstrapPlan {
        output_format: CliOutputFormat,
    },
    Agents {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Mcp {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Skills {
        args: Option<String>,
        output_format: CliOutputFormat,
    },
    Plugins {
        action: Option<String>,
        target: Option<String>,
        output_format: CliOutputFormat,
    },
    PrintSystemPrompt {
        cwd: PathBuf,
        date: String,
        output_format: CliOutputFormat,
    },
    Version {
        output_format: CliOutputFormat,
    },
    ResumeSession {
        session_path: PathBuf,
        commands: Vec<String>,
        output_format: CliOutputFormat,
    },
    Status {
        model: String,
        // #148: raw `--model` flag input (pre-alias-resolution), if any.
        // None means no flag was supplied; env/config/default fallback is
        // resolved inside `print_status_snapshot`.
        model_flag_raw: Option<String>,
        permission_mode: PermissionMode,
        output_format: CliOutputFormat,
    },
    Sandbox {
        output_format: CliOutputFormat,
    },
    Prompt {
        prompt: String,
        model: String,
        output_format: CliOutputFormat,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        compact: bool,
        base_commit: Option<String>,
        reasoning_effort: Option<String>,
        allow_broad_cwd: bool,
    },
    Doctor {
        output_format: CliOutputFormat,
    },
    Acp {
        output_format: CliOutputFormat,
    },
    State {
        output_format: CliOutputFormat,
    },
    Init {
        output_format: CliOutputFormat,
    },
    // #146: `claw config` and `claw diff` are pure-local read-only
    // introspection commands; wire them as standalone CLI subcommands.
    Config {
        section: Option<String>,
        output_format: CliOutputFormat,
    },
    Diff {
        output_format: CliOutputFormat,
    },
    Export {
        session_reference: String,
        output_path: Option<PathBuf>,
        output_format: CliOutputFormat,
    },
    Repl {
        model: String,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        base_commit: Option<String>,
        reasoning_effort: Option<String>,
        allow_broad_cwd: bool,
    },
    HelpTopic(LocalHelpTopic),
    // prompt-mode formatting is only supported for non-interactive runs
    Help {
        output_format: CliOutputFormat,
    },
    // A2-L1b: additive read-only plan runner subcommand. Implementation
    // lives entirely in the `a2-plan-runner` crate; this variant only
    // carries parsed flags. No existing CliAction variant is modified.
    //
    // A2-L2b run-plan write-preview extension: when
    // `workspace_write_preview` is set, the CLI dispatches to
    // [`a2_plan_runner::run_plan_with_write_preview`] which permits exactly
    // ONE workspace-write step to produce preview-only artifacts and halt
    // before approval. Without the flag, the existing read-only-only
    // `run_plan` contract is unchanged (workspace-write plans still
    // refuse via precheck).
    Plan {
        file: PathBuf,
        dry_run: bool,
        report_format: PlanReportFormat,
        substrate_url: Option<String>,
        fast_model: Option<String>,
        wrapper: Option<PathBuf>,
        /// Operator-supplied per-step wall-clock cap. `None` = use the
        /// runner's `DEFAULT_STEP_TIMEOUT`. Bounds enforced by
        /// [`a2_plan_runner::parse_step_timeout_seconds`].
        step_timeout: Option<std::time::Duration>,
        /// L2b opt-in: enable workspace-write preview-only handling for a
        /// plan that contains exactly one `mode: workspace-write` step.
        /// Default `false` preserves L1b read-only-only behavior.
        workspace_write_preview: bool,
        /// L2b opt-in: optional workspace root override. Defaults to the
        /// CLI's current working directory. The runner canonicalizes the
        /// path internally before any filesystem read.
        workspace_root: Option<PathBuf>,
    },
    // A2-L2b Slice 3d: operator-facing approval command. Reads ONE
    // preview bundle file from disk, renders the Slice-3b operator
    // prompt to stderr, reads exactly one approval line from TTY stdin,
    // and emits a structured approval-result JSON on stdout. The
    // command NEVER writes target files, NEVER mutates the checkpoint
    // store, NEVER calls the broker, NEVER spawns subprocesses. It is
    // purely a renderer + input reader + Slice-3a evaluator wrapper.
    PlanApprove {
        bundle_path: PathBuf,
        /// Optional guard-preserving sink for the emitted approval-result
        /// JSON (`--approval-result-output <path>`). `None` = stdout only
        /// (unchanged behavior). Written only on a successful approval.
        approval_result_output: Option<PathBuf>,
    },
    // A2-L2b Slice L2b-CLI-Apply: operator-facing single-file write
    // command. Reads ONE apply bundle file from disk, validates every
    // authority object (preview / approval / checkpoint / payload),
    // resolves the target fresh through Slice-1, and invokes the
    // library-level write executor. Emits a structured apply-result JSON
    // on stdout. NEVER reads stdin, NEVER spawns subprocesses, NEVER
    // calls the broker, NEVER accepts pre-approval flags, NEVER touches
    // more than the single resolved target file.
    PlanApply {
        bundle_path: PathBuf,
    },
    // A2-L2b Slice L2b-CLI-Preview-Bundle: operator-facing upstream
    // artifact producer. Resolves the target through Slice-1, captures
    // a Slice-2 checkpoint, copies the operator-provided after-file
    // into runner-owned payload storage, builds a Slice-3a preview, and
    // writes a `preview-bundle.json` compatible with `claw plan
    // approve`. NEVER mutates the target file. NEVER approves. NEVER
    // applies. NEVER spawns subprocesses. NEVER calls the broker.
    PlanPreviewBundle {
        workspace_root: PathBuf,
        target_relative_path: String,
        after_file: PathBuf,
    },
    // A2-L2b Slice L2b-CLI-Apply-Bundle-Generator: operator-facing
    // apply-bundle artifact producer. Reads the result of `claw plan
    // preview-bundle` and the result of `claw plan approve`, validates
    // the full authority chain, and writes an `apply-bundle.json`
    // consumable by `claw plan apply`. NEVER executes apply. NEVER
    // mutates the target file. NEVER calls `execute_write` or
    // `bind_after_bytes`. NEVER wires `run_plan` workspace-write.
    PlanApplyBundle {
        preview_result_path: PathBuf,
        approval_result_path: PathBuf,
    },
    // A2-L2d Read-Only Artifact Inspector / Status Contract:
    // read-only state inspector. Aggregates state from existing
    // `<workspace>/.claw/l2b-*` artifacts and emits an
    // `a2-l2d-status.v1` JSON envelope on stdout. NEVER mutates the
    // filesystem. NEVER calls the broker, model, or Ollama. NEVER
    // approves or applies. The optional `approval_result_path` is the
    // ONLY permitted read outside `<workspace>/.claw/**`; it is on a
    // distinct code branch from automatic artifact discovery.
    PlanStatus {
        workspace_root: PathBuf,
        approval_result_path: Option<PathBuf>,
    },
}

/// A2-L1b CLI report-format selector. Separate enum from `CliOutputFormat`
/// because the report markers form a stable operator-facing contract that
/// is independent of the existing `--output-format` text/json toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanReportFormat {
    Markers,
    Json,
}

impl PlanReportFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "markers" => Ok(Self::Markers),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unsupported value for --report-format: {other} (expected markers or json)"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalHelpTopic {
    Status,
    Sandbox,
    Doctor,
    Acp,
    // #141: extend the local-help pattern to every subcommand so
    // `claw <subcommand> --help` has one consistent contract.
    Init,
    State,
    Export,
    Version,
    SystemPrompt,
    DumpManifests,
    BootstrapPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliOutputFormat {
    Text,
    Json,
}

impl CliOutputFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unsupported value for --output-format: {other} (expected text or json)"
            )),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut model = DEFAULT_MODEL.to_string();
    // #148: when user passes --model/--model=, capture the raw input so we
    // can attribute source: "flag" later. None means no flag was supplied.
    let mut model_flag_raw: Option<String> = None;
    let mut output_format = CliOutputFormat::Text;
    let mut permission_mode_override = None;
    let mut wants_help = false;
    let mut wants_version = false;
    let mut allowed_tool_values = Vec::new();
    let mut compact = false;
    let mut base_commit: Option<String> = None;
    let mut reasoning_effort: Option<String> = None;
    let mut allow_broad_cwd = false;
    let mut rest: Vec<String> = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" if rest.is_empty() => {
                wants_help = true;
                index += 1;
            }
            "--help" | "-h"
                if !rest.is_empty()
                    && matches!(rest[0].as_str(), "prompt" | "commit" | "pr" | "issue") =>
            {
                // `--help` following a subcommand that would otherwise forward
                // the arg to the API (e.g. `claw prompt --help`) should show
                // top-level help instead. Subcommands that consume their own
                // args (agents, mcp, plugins, skills) and local help-topic
                // subcommands (status, sandbox, doctor, init, state, export,
                // version, system-prompt, dump-manifests, bootstrap-plan) must
                // NOT be intercepted here — they handle --help in their own
                // dispatch paths via parse_local_help_action(). See #141.
                wants_help = true;
                index += 1;
            }
            "--version" | "-V" => {
                wants_version = true;
                index += 1;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --model".to_string())?;
                validate_model_syntax(value)?;
                model = resolve_model_alias_with_config(value);
                model_flag_raw = Some(value.clone()); // #148
                index += 2;
            }
            flag if flag.starts_with("--model=") => {
                let value = &flag[8..];
                validate_model_syntax(value)?;
                model = resolve_model_alias_with_config(value);
                model_flag_raw = Some(value.to_string()); // #148
                index += 1;
            }
            "--output-format" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --output-format".to_string())?;
                output_format = CliOutputFormat::parse(value)?;
                index += 2;
            }
            "--permission-mode" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --permission-mode".to_string())?;
                permission_mode_override = Some(parse_permission_mode_arg(value)?);
                index += 2;
            }
            flag if flag.starts_with("--output-format=") => {
                output_format = CliOutputFormat::parse(&flag[16..])?;
                index += 1;
            }
            flag if flag.starts_with("--permission-mode=") => {
                permission_mode_override = Some(parse_permission_mode_arg(&flag[18..])?);
                index += 1;
            }
            "--dangerously-skip-permissions" => {
                permission_mode_override = Some(PermissionMode::DangerFullAccess);
                index += 1;
            }
            "--compact" => {
                compact = true;
                index += 1;
            }
            "--base-commit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --base-commit".to_string())?;
                base_commit = Some(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--base-commit=") => {
                base_commit = Some(flag[14..].to_string());
                index += 1;
            }
            "--reasoning-effort" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --reasoning-effort".to_string())?;
                if !matches!(value.as_str(), "low" | "medium" | "high") {
                    return Err(format!(
                        "invalid value for --reasoning-effort: '{value}'; must be low, medium, or high"
                    ));
                }
                reasoning_effort = Some(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--reasoning-effort=") => {
                let value = &flag[19..];
                if !matches!(value, "low" | "medium" | "high") {
                    return Err(format!(
                        "invalid value for --reasoning-effort: '{value}'; must be low, medium, or high"
                    ));
                }
                reasoning_effort = Some(value.to_string());
                index += 1;
            }
            "--allow-broad-cwd" => {
                allow_broad_cwd = true;
                index += 1;
            }
            "-p" => {
                // Claw Code compat: -p "prompt" = one-shot prompt
                let prompt = args[index + 1..].join(" ");
                if prompt.trim().is_empty() {
                    return Err("-p requires a prompt string".to_string());
                }
                return Ok(CliAction::Prompt {
                    prompt,
                    model: resolve_model_alias_with_config(&model),
                    output_format,
                    allowed_tools: normalize_allowed_tools(&allowed_tool_values)?,
                    permission_mode: permission_mode_override
                        .unwrap_or_else(default_permission_mode),
                    compact,
                    base_commit: base_commit.clone(),
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                });
            }
            "--print" => {
                // Claw Code compat: --print makes output non-interactive
                output_format = CliOutputFormat::Text;
                index += 1;
            }
            "--resume" if rest.is_empty() => {
                rest.push("--resume".to_string());
                index += 1;
            }
            flag if rest.is_empty() && flag.starts_with("--resume=") => {
                rest.push("--resume".to_string());
                rest.push(flag[9..].to_string());
                index += 1;
            }
            "--acp" | "-acp" => {
                rest.push("acp".to_string());
                index += 1;
            }
            "--allowedTools" | "--allowed-tools" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --allowedTools".to_string())?;
                allowed_tool_values.push(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--allowedTools=") => {
                allowed_tool_values.push(flag[15..].to_string());
                index += 1;
            }
            flag if flag.starts_with("--allowed-tools=") => {
                allowed_tool_values.push(flag[16..].to_string());
                index += 1;
            }
            other if rest.is_empty() && other.starts_with('-') => {
                return Err(format_unknown_option(other))
            }
            other => {
                rest.push(other.to_string());
                index += 1;
            }
        }
    }

    if wants_help {
        return Ok(CliAction::Help { output_format });
    }

    if wants_version {
        return Ok(CliAction::Version { output_format });
    }

    let allowed_tools = normalize_allowed_tools(&allowed_tool_values)?;

    if rest.is_empty() {
        let permission_mode = permission_mode_override.unwrap_or_else(default_permission_mode);
        // When stdin is not a terminal (pipe/redirect) and no prompt is given on the
        // command line, read stdin as the prompt and dispatch as a one-shot Prompt
        // rather than starting the interactive REPL (which would consume the pipe and
        // print the startup banner, then exit without sending anything to the API).
        if !std::io::stdin().is_terminal() {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            let piped = buf.trim().to_string();
            if !piped.is_empty() {
                return Ok(CliAction::Prompt {
                    model,
                    prompt: piped,
                    allowed_tools,
                    permission_mode,
                    output_format,
                    compact: false,
                    base_commit,
                    reasoning_effort,
                    allow_broad_cwd,
                });
            }
        }
        return Ok(CliAction::Repl {
            model,
            allowed_tools,
            permission_mode,
            base_commit,
            reasoning_effort: reasoning_effort.clone(),
            allow_broad_cwd,
        });
    }
    if rest.first().map(String::as_str) == Some("--resume") {
        return parse_resume_args(&rest[1..], output_format);
    }
    if let Some(action) = parse_local_help_action(&rest) {
        return action;
    }
    if let Some(action) = parse_single_word_command_alias(
        &rest,
        &model,
        model_flag_raw.as_deref(),
        permission_mode_override,
        output_format,
    ) {
        return action;
    }

    // A2-L1b additive guard: reject plan-incompatible global flags BEFORE
    // they are consumed into the per-subcommand binding. The runner pins
    // these by construction (build_claw_command), so accepting them at the
    // CLI would create a path that bypasses the pinning. Non-plan
    // subcommands are completely unaffected by this check.
    if rest.first().map(String::as_str) == Some("plan") {
        if permission_mode_override.is_some() {
            return Err(
                "--permission-mode is not supported by `claw plan run`. The runner \
                 pins read-only execution by construction."
                    .to_string(),
            );
        }
        if !allowed_tool_values.is_empty() {
            return Err(
                "--allowed-tools / --allowedTools is not supported by `claw plan run`. \
                 The runner pins the static read-only tool allowlist by construction."
                    .to_string(),
            );
        }
    }

    let permission_mode = permission_mode_override.unwrap_or_else(default_permission_mode);

    match rest[0].as_str() {
        "dump-manifests" => parse_dump_manifests_args(&rest[1..], output_format),
        "bootstrap-plan" => Ok(CliAction::BootstrapPlan { output_format }),
        "agents" => Ok(CliAction::Agents {
            args: join_optional_args(&rest[1..]),
            output_format,
        }),
        "mcp" => Ok(CliAction::Mcp {
            args: join_optional_args(&rest[1..]),
            output_format,
        }),
        // #145: `plugins` was routed through the prompt fallback because no
        // top-level parser arm produced CliAction::Plugins. That made `claw
        // plugins` (and `claw plugins --help`, `claw plugins list`, ...)
        // attempt an Anthropic network call, surfacing the misleading error
        // `missing Anthropic credentials` even though the command is purely
        // local introspection. Mirror `agents`/`mcp`/`skills`: action is the
        // first positional arg, target is the second.
        "plugins" => {
            let tail = &rest[1..];
            let action = tail.first().cloned();
            let target = tail.get(1).cloned();
            if tail.len() > 2 {
                return Err(format!(
                    "unexpected extra arguments after `claw plugins {}`: {}",
                    tail[..2].join(" "),
                    tail[2..].join(" ")
                ));
            }
            Ok(CliAction::Plugins {
                action,
                target,
                output_format,
            })
        }
        // #146: `config` is pure-local read-only introspection (merges
        // `.claw.json` + `.claw/settings.json` from disk, no network, no
        // state mutation). Previously callers had to spin up a session with
        // `claw --resume SESSION.jsonl /config` to see their own config,
        // which is synthetic friction. Accepts an optional section name
        // (env|hooks|model|plugins) matching the slash command shape.
        "config" => {
            let tail = &rest[1..];
            let section = tail.first().cloned();
            if tail.len() > 1 {
                return Err(format!(
                    "unexpected extra arguments after `claw config {}`: {}",
                    tail[0],
                    tail[1..].join(" ")
                ));
            }
            Ok(CliAction::Config {
                section,
                output_format,
            })
        }
        // #146: `diff` is pure-local (shells out to `git diff --cached` +
        // `git diff`). No session needed to inspect the working tree.
        "diff" => {
            if rest.len() > 1 {
                return Err(format!(
                    "unexpected extra arguments after `claw diff`: {}",
                    rest[1..].join(" ")
                ));
            }
            Ok(CliAction::Diff { output_format })
        }
        "skills" => {
            let args = join_optional_args(&rest[1..]);
            match classify_skills_slash_command(args.as_deref()) {
                SkillSlashDispatch::Invoke(prompt) => Ok(CliAction::Prompt {
                    prompt,
                    model,
                    output_format,
                    allowed_tools,
                    permission_mode,
                    compact,
                    base_commit,
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                }),
                SkillSlashDispatch::Local => Ok(CliAction::Skills {
                    args,
                    output_format,
                }),
            }
        }
        "system-prompt" => parse_system_prompt_args(&rest[1..], output_format),
        "acp" => parse_acp_args(&rest[1..], output_format),
        "login" | "logout" => Err(removed_auth_surface_error(rest[0].as_str())),
        "init" => Ok(CliAction::Init { output_format }),
        "export" => parse_export_args(&rest[1..], output_format),
        // A2-L1b: `claw plan run <file>` read-only plan runner. Purely
        // additive — no other CliAction match arm is altered.
        "plan" => parse_plan_subcommand_args(&rest[1..]),
        "prompt" => {
            let prompt = rest[1..].join(" ");
            if prompt.trim().is_empty() {
                return Err("prompt subcommand requires a prompt string".to_string());
            }
            Ok(CliAction::Prompt {
                prompt,
                model,
                output_format,
                allowed_tools,
                permission_mode,
                compact,
                base_commit: base_commit.clone(),
                reasoning_effort: reasoning_effort.clone(),
                allow_broad_cwd,
            })
        }
        other if other.starts_with('/') => parse_direct_slash_cli_action(
            &rest,
            model,
            output_format,
            allowed_tools,
            permission_mode,
            compact,
            base_commit,
            reasoning_effort,
            allow_broad_cwd,
        ),
        other => {
            if rest.len() == 1 && looks_like_subcommand_typo(other) {
                if let Some(suggestions) = suggest_similar_subcommand(other) {
                    let mut message = format!("unknown subcommand: {other}.");
                    if let Some(line) = render_suggestion_line("Did you mean", &suggestions) {
                        message.push('\n');
                        message.push_str(&line);
                    }
                    message.push_str(
                        "\nRun `claw --help` for the full list. If you meant to send a prompt literally, use `claw prompt <text>`.",
                    );
                    return Err(message);
                }
            }
            // #147: guard empty/whitespace-only prompts at the fallthrough
            // path the same way `"prompt"` arm above does. Without this,
            // `claw ""`, `claw "   "`, and `claw "" ""` silently route to
            // the Anthropic call and surface a misleading
            // `missing Anthropic credentials` error (or burn API tokens on
            // an empty prompt when credentials are present).
            let joined = rest.join(" ");
            if joined.trim().is_empty() {
                return Err(
                    "empty prompt: provide a subcommand (run `claw --help`) or a non-empty prompt string"
                        .to_string(),
                );
            }
            Ok(CliAction::Prompt {
                prompt: joined,
                model,
                output_format,
                allowed_tools,
                permission_mode,
                compact,
                base_commit,
                reasoning_effort: reasoning_effort.clone(),
                allow_broad_cwd,
            })
        }
    }
}

fn parse_local_help_action(rest: &[String]) -> Option<Result<CliAction, String>> {
    if rest.len() != 2 || !is_help_flag(&rest[1]) {
        return None;
    }

    let topic = match rest[0].as_str() {
        "status" => LocalHelpTopic::Status,
        "sandbox" => LocalHelpTopic::Sandbox,
        "doctor" => LocalHelpTopic::Doctor,
        "acp" => LocalHelpTopic::Acp,
        // #141: add the subcommands that were previously falling back
        // to global help (init/state/export/version) or erroring out
        // (system-prompt/dump-manifests) or printing their primary
        // output instead of help text (bootstrap-plan).
        "init" => LocalHelpTopic::Init,
        "state" => LocalHelpTopic::State,
        "export" => LocalHelpTopic::Export,
        "version" => LocalHelpTopic::Version,
        "system-prompt" => LocalHelpTopic::SystemPrompt,
        "dump-manifests" => LocalHelpTopic::DumpManifests,
        "bootstrap-plan" => LocalHelpTopic::BootstrapPlan,
        _ => return None,
    };
    Some(Ok(CliAction::HelpTopic(topic)))
}

fn is_help_flag(value: &str) -> bool {
    matches!(value, "--help" | "-h")
}

fn parse_single_word_command_alias(
    rest: &[String],
    model: &str,
    // #148: raw --model flag input for status provenance. None = no flag.
    model_flag_raw: Option<&str>,
    permission_mode_override: Option<PermissionMode>,
    output_format: CliOutputFormat,
) -> Option<Result<CliAction, String>> {
    if rest.is_empty() {
        return None;
    }

    // Diagnostic verbs (help, version, status, sandbox, doctor, state) accept only the verb itself
    // or --help / -h as a suffix. Any other suffix args are unrecognized.
    let verb = &rest[0];
    let is_diagnostic = matches!(
        verb.as_str(),
        "help" | "version" | "status" | "sandbox" | "doctor" | "state"
    );

    if is_diagnostic && rest.len() > 1 {
        // Diagnostic verb with trailing args: reject unrecognized suffix
        if is_help_flag(&rest[1]) && rest.len() == 2 {
            // "doctor --help" is valid, routed to parse_local_help_action() instead
            return None;
        }
        // Unrecognized suffix like "--json"
        let mut msg = format!(
            "unrecognized argument `{}` for subcommand `{}`",
            rest[1], verb
        );
        // #152: common mistake — users type `--json` expecting JSON output.
        // Hint at the correct flag so they don't have to re-read --help.
        if rest[1] == "--json" {
            msg.push_str("\nDid you mean `--output-format json`?");
        }
        return Some(Err(msg));
    }

    if rest.len() != 1 {
        return None;
    }

    match rest[0].as_str() {
        "help" => Some(Ok(CliAction::Help { output_format })),
        "version" => Some(Ok(CliAction::Version { output_format })),
        "status" => Some(Ok(CliAction::Status {
            model: model.to_string(),
            model_flag_raw: model_flag_raw.map(str::to_string), // #148
            permission_mode: permission_mode_override.unwrap_or_else(default_permission_mode),
            output_format,
        })),
        "sandbox" => Some(Ok(CliAction::Sandbox { output_format })),
        "doctor" => Some(Ok(CliAction::Doctor { output_format })),
        "state" => Some(Ok(CliAction::State { output_format })),
        // #146: let `config` and `diff` fall through to parse_subcommand
        // where they are wired as pure-local introspection, instead of
        // producing the "is a slash command" guidance. Zero-arg cases
        // reach parse_subcommand too via this None.
        "config" | "diff" => None,
        other => bare_slash_command_guidance(other).map(Err),
    }
}

fn bare_slash_command_guidance(command_name: &str) -> Option<String> {
    if matches!(
        command_name,
        "dump-manifests"
            | "bootstrap-plan"
            | "agents"
            | "mcp"
            | "skills"
            | "system-prompt"
            | "init"
            | "prompt"
            | "export"
    ) {
        return None;
    }
    let slash_command = slash_command_specs()
        .iter()
        .find(|spec| spec.name == command_name)?;
    let guidance = if slash_command.resume_supported {
        format!(
            "`claw {command_name}` is a slash command. Use `claw --resume SESSION.jsonl /{command_name}` or start `claw` and run `/{command_name}`."
        )
    } else {
        format!(
            "`claw {command_name}` is a slash command. Start `claw` and run `/{command_name}` inside the REPL."
        )
    };
    Some(guidance)
}

fn removed_auth_surface_error(command_name: &str) -> String {
    format!(
        "`claw {command_name}` has been removed. Set ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN instead."
    )
}

fn parse_acp_args(args: &[String], output_format: CliOutputFormat) -> Result<CliAction, String> {
    match args {
        [] => Ok(CliAction::Acp { output_format }),
        [subcommand] if subcommand == "serve" => Ok(CliAction::Acp { output_format }),
        _ => Err(String::from(
            "unsupported ACP invocation. Use `claw acp`, `claw acp serve`, `claw --acp`, or `claw -acp`.",
        )),
    }
}

/// A2-L1b: parse `claw plan run <file> [--dry-run] [--report-format
/// markers|json] [--substrate-url URL] [--fast-model NAME]
/// [--wrapper PATH]`.
///
/// `--wrapper PATH` is the one deviation from the saved Phase 4 flag list:
/// the runner subprocess MUST be launched through the read-only wrapper at
/// `scripts/claw-sidestack-local`, so the CLI needs a way to locate it.
/// When omitted, defaults to `scripts/claw-sidestack-local` resolved
/// against the current working directory.
///
/// Explicitly rejected flags (operator hard rules 6+7): `--allow-write`,
/// `--force`, `--permission-mode`, `--model`, `--allowed-tools`. The plan
/// runner pins these by construction via [`a2_plan_runner::run_plan`] →
/// [`a2_plan_runner::runner::build_claw_command`]; surfacing them on the
/// CLI would create a path that bypasses that builder.
fn parse_plan_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    match args.first().map(String::as_str) {
        Some("run") => {}
        Some("approve") => return parse_plan_approve_subcommand_args(&args[1..]),
        Some("apply") => return parse_plan_apply_subcommand_args(&args[1..]),
        Some("preview-bundle") => {
            return parse_plan_preview_bundle_subcommand_args(&args[1..]);
        }
        Some("apply-bundle") => {
            return parse_plan_apply_bundle_subcommand_args(&args[1..]);
        }
        Some("status") => return parse_plan_status_subcommand_args(&args[1..]),
        Some(other) => {
            return Err(format!(
                "unsupported `claw plan` subcommand: {other}. \
                 Use `claw plan run <file>`, `claw plan approve <preview-bundle.json>`, \
                 `claw plan apply <apply-bundle.json>`, \
                 `claw plan preview-bundle <workspace-root> <target-relative-path> <after-file>`, \
                 `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`, \
                 or `claw plan status <workspace> [<approval-result.json>]`."
            ));
        }
        None => {
            return Err("missing `claw plan` subcommand. \
                 Use `claw plan run <file>`, `claw plan approve <preview-bundle.json>`, \
                 `claw plan apply <apply-bundle.json>`, \
                 `claw plan preview-bundle <workspace-root> <target-relative-path> <after-file>`, \
                 `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`, \
                 or `claw plan status <workspace> [<approval-result.json>]`."
                .to_string());
        }
    }
    let tail = &args[1..];

    let mut file: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut report_format = PlanReportFormat::Markers;
    let mut substrate_url: Option<String> = None;
    let mut fast_model: Option<String> = None;
    let mut wrapper: Option<PathBuf> = None;
    let mut step_timeout: Option<std::time::Duration> = None;
    let mut workspace_write_preview = false;
    let mut workspace_root: Option<PathBuf> = None;

    let mut i = 0;
    while i < tail.len() {
        let arg = tail[i].as_str();
        match arg {
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--report-format" => {
                let value = tail.get(i + 1).ok_or_else(|| {
                    "missing value for --report-format (expected markers|json)".to_string()
                })?;
                report_format = PlanReportFormat::parse(value)?;
                i += 2;
            }
            v if v.starts_with("--report-format=") => {
                report_format = PlanReportFormat::parse(&v["--report-format=".len()..])?;
                i += 1;
            }
            "--substrate-url" => {
                let value = tail
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --substrate-url".to_string())?;
                substrate_url = Some(value.clone());
                i += 2;
            }
            v if v.starts_with("--substrate-url=") => {
                substrate_url = Some(v["--substrate-url=".len()..].to_string());
                i += 1;
            }
            "--fast-model" => {
                let value = tail
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --fast-model".to_string())?;
                fast_model = Some(value.clone());
                i += 2;
            }
            v if v.starts_with("--fast-model=") => {
                fast_model = Some(v["--fast-model=".len()..].to_string());
                i += 1;
            }
            "--wrapper" => {
                let value = tail
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --wrapper".to_string())?;
                wrapper = Some(PathBuf::from(value));
                i += 2;
            }
            v if v.starts_with("--wrapper=") => {
                wrapper = Some(PathBuf::from(&v["--wrapper=".len()..]));
                i += 1;
            }
            // Phase 5 Fix A: bounded per-step wall-clock cap.
            // Parser validates and rejects out-of-range / non-integer
            // values via a2_plan_runner::parse_step_timeout_seconds.
            "--step-timeout" => {
                let value = tail
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --step-timeout".to_string())?;
                step_timeout = Some(a2_plan_runner::parse_step_timeout_seconds(value)?);
                i += 2;
            }
            v if v.starts_with("--step-timeout=") => {
                step_timeout = Some(a2_plan_runner::parse_step_timeout_seconds(
                    &v["--step-timeout=".len()..],
                )?);
                i += 1;
            }
            // A2-L2b run-plan write-preview opt-in. Without this flag,
            // workspace-write plans still refuse at precheck. With it,
            // the runner permits exactly one workspace-write step to
            // produce preview-only artifacts and halt.
            "--workspace-write-preview" => {
                workspace_write_preview = true;
                i += 1;
            }
            "--workspace-root" => {
                let value = tail
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --workspace-root".to_string())?;
                workspace_root = Some(PathBuf::from(value));
                i += 2;
            }
            v if v.starts_with("--workspace-root=") => {
                workspace_root = Some(PathBuf::from(&v["--workspace-root=".len()..]));
                i += 1;
            }
            // Explicitly reject flags that would bypass the runner's
            // construction guarantees. Spelled out so users get a clear
            // error instead of silent acceptance.
            "--allow-write" | "--force" | "--permission-mode" | "--model" | "--allowed-tools"
            | "--yes" | "--auto" | "--preapproved" | "--batch" => {
                return Err(format!(
                    "{arg} is not supported by `claw plan run`. The runner pins read-only \
                     execution, FAST tier, and the static tool allowlist by construction. \
                     Approval / apply must go through `claw plan approve` and \
                     `claw plan apply-bundle` + `claw plan apply` — never bypassed."
                ));
            }
            v if v.starts_with("--") => {
                return Err(format!("unsupported flag for `claw plan run`: {v}"));
            }
            v => {
                if file.is_some() {
                    return Err(format!(
                        "unexpected positional argument after plan file: {v}"
                    ));
                }
                file = Some(PathBuf::from(v));
                i += 1;
            }
        }
    }

    let file = file.ok_or_else(|| {
        "missing plan file. Usage: `claw plan run <file> [--dry-run] \
         [--report-format markers|json] [--substrate-url URL] \
         [--fast-model NAME] [--wrapper PATH] [--step-timeout SECONDS] \
         [--workspace-write-preview] [--workspace-root PATH]`"
            .to_string()
    })?;

    if workspace_write_preview && dry_run {
        return Err(
            "--workspace-write-preview cannot be combined with --dry-run. \
             The preview artifacts are the deliverable; a dry run would have nothing to emit."
                .to_string(),
        );
    }

    if workspace_root.is_some() && !workspace_write_preview {
        return Err(
            "--workspace-root requires --workspace-write-preview. The L1b read-only \
             path runs from the current working directory and ignores any workspace \
             root override."
                .to_string(),
        );
    }

    Ok(CliAction::Plan {
        file,
        dry_run,
        report_format,
        substrate_url,
        fast_model,
        wrapper,
        step_timeout,
        workspace_write_preview,
        workspace_root,
    })
}

/// A2-L2b Slice 3d: parse `claw plan approve <preview-bundle.json>`.
///
/// Exactly one positional argument (the preview bundle). The only accepted
/// flag is the OPT-IN `--approval-result-output <path>` (or
/// `--approval-result-output=<path>`), which is NOT a pre-approval flag and
/// does not relax any approval guard. Any OTHER `--*` flag — including
/// pre-approval flags like `--yes`, `--auto`, `--force`, `--allow-write`
/// — is refused outright before any filesystem touch.
fn parse_plan_approve_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    const USAGE: &str =
        "Usage: `claw plan approve <preview-bundle.json> [--approval-result-output <path>]`.";
    let mut bundle: Option<PathBuf> = None;
    let mut approval_result_output: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let v = arg.as_str();
        if let Some(rest) = v.strip_prefix("--approval-result-output") {
            let path = if let Some(inline) = rest.strip_prefix('=') {
                inline.to_string()
            } else if rest.is_empty() {
                match iter.next() {
                    Some(p) => p.clone(),
                    None => {
                        return Err(format!(
                            "`--approval-result-output` requires a path argument. {USAGE}"
                        ))
                    }
                }
            } else {
                // e.g. `--approval-result-outputX` — an unknown flag, not ours.
                return Err(format!(
                    "unsupported flag for `claw plan approve`: {v}. {USAGE}"
                ));
            };
            if approval_result_output.is_some() {
                return Err(format!(
                    "`--approval-result-output` specified more than once. {USAGE}"
                ));
            }
            if path.is_empty() {
                return Err(format!(
                    "`--approval-result-output` requires a non-empty path. {USAGE}"
                ));
            }
            approval_result_output = Some(PathBuf::from(path));
            continue;
        }
        if v.starts_with("--") {
            return Err(format!(
                "unsupported flag for `claw plan approve`: {v}. {USAGE}"
            ));
        }
        if bundle.is_some() {
            return Err(format!(
                "unexpected positional argument after preview bundle: {v}. {USAGE}"
            ));
        }
        bundle = Some(PathBuf::from(v));
    }
    let bundle_path = bundle.ok_or_else(|| format!("missing preview bundle. {USAGE}"))?;
    Ok(CliAction::PlanApprove {
        bundle_path,
        approval_result_output,
    })
}

/// A2-L2b Slice L2b-CLI-Apply: parse `claw plan apply <apply-bundle.json>`.
///
/// Exactly one positional argument, no flags. Every pre-approval flag
/// (`--yes`, `--auto`, `--force`, `--allow-write`, `--preapproved`,
/// `--batch`) is refused outright before any filesystem touch.
fn parse_plan_apply_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    let mut bundle: Option<PathBuf> = None;
    for arg in args {
        let v = arg.as_str();
        if v.starts_with("--") {
            return Err(format!(
                "unsupported flag for `claw plan apply`: {v}. \
                 Usage: `claw plan apply <apply-bundle.json>`."
            ));
        }
        if bundle.is_some() {
            return Err(format!(
                "unexpected positional argument after apply bundle: {v}. \
                 Usage: `claw plan apply <apply-bundle.json>`."
            ));
        }
        bundle = Some(PathBuf::from(v));
    }
    let bundle_path = bundle.ok_or_else(|| {
        "missing apply bundle. Usage: `claw plan apply <apply-bundle.json>`.".to_string()
    })?;
    Ok(CliAction::PlanApply { bundle_path })
}

/// A2-L2b Slice L2b-CLI-Preview-Bundle: parse
/// `claw plan preview-bundle <workspace-root> <target-relative-path> <after-file>`.
///
/// Exactly three positional arguments, no flags. Every pre-approval /
/// batch flag (`--yes`, `--auto`, `--force`, `--allow-write`,
/// `--preapproved`, `--batch`) is refused outright before any
/// filesystem touch. The generator never reads stdin and never accepts
/// inline payload bytes; the after-file path is the only supported
/// payload source.
fn parse_plan_preview_bundle_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    let usage = "Usage: `claw plan preview-bundle <workspace-root> \
                 <target-relative-path> <after-file>`.";
    let mut positionals: Vec<&str> = Vec::with_capacity(3);
    for arg in args {
        let v = arg.as_str();
        if v.starts_with("--") {
            return Err(format!(
                "unsupported flag for `claw plan preview-bundle`: {v}. {usage}"
            ));
        }
        if positionals.len() >= 3 {
            return Err(format!(
                "unexpected positional argument after after-file: {v}. {usage}"
            ));
        }
        positionals.push(v);
    }
    if positionals.len() != 3 {
        return Err(format!(
            "expected 3 positional arguments (got {}). {usage}",
            positionals.len()
        ));
    }
    let workspace_root = PathBuf::from(positionals[0]);
    let target_relative_path = positionals[1].to_string();
    let after_file = PathBuf::from(positionals[2]);
    Ok(CliAction::PlanPreviewBundle {
        workspace_root,
        target_relative_path,
        after_file,
    })
}

/// A2-L2b Slice L2b-CLI-Apply-Bundle-Generator: parse
/// `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`.
///
/// Exactly two positional arguments, no flags. Every pre-approval / batch
/// flag (`--yes`, `--auto`, `--force`, `--allow-write`, `--preapproved`,
/// `--batch`) is refused outright before any filesystem touch. The
/// generator never reads stdin and never accepts inline JSON; both inputs
/// must be on-disk file paths.
fn parse_plan_apply_bundle_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    let usage = "Usage: `claw plan apply-bundle <preview-generator-result.json> \
                 <approval-result.json>`.";
    let mut positionals: Vec<&str> = Vec::with_capacity(2);
    for arg in args {
        let v = arg.as_str();
        if v.starts_with("--") {
            return Err(format!(
                "unsupported flag for `claw plan apply-bundle`: {v}. {usage}"
            ));
        }
        if positionals.len() >= 2 {
            return Err(format!(
                "unexpected positional argument after approval-result: {v}. {usage}"
            ));
        }
        positionals.push(v);
    }
    if positionals.len() != 2 {
        return Err(format!(
            "expected 2 positional arguments (got {}). {usage}",
            positionals.len()
        ));
    }
    let preview_result_path = PathBuf::from(positionals[0]);
    let approval_result_path = PathBuf::from(positionals[1]);
    Ok(CliAction::PlanApplyBundle {
        preview_result_path,
        approval_result_path,
    })
}

/// A2-L2d Read-Only Artifact Inspector: parse
/// `claw plan status <workspace> [<approval-result.json>]`.
///
/// One required and one optional positional argument, no flags. Every
/// write-adjacent flag (`--apply`, `--approve`, `--yes`, `--auto`,
/// `--clean`, `--rollback`, `--mutate`, `--all-runs`, `--no-prompt`,
/// `--skip-approval`, `--cache`) is refused outright. The command is
/// read-only by construction.
fn parse_plan_status_subcommand_args(args: &[String]) -> Result<CliAction, String> {
    let usage = "Usage: `claw plan status <workspace> [<approval-result.json>]`.";
    let mut positionals: Vec<&str> = Vec::with_capacity(2);
    for arg in args {
        let v = arg.as_str();
        if v.starts_with("--") {
            return Err(format!(
                "unsupported flag for `claw plan status`: {v}. \
                 The status command is read-only and accepts no flags. \
                 {usage}"
            ));
        }
        if positionals.len() >= 2 {
            return Err(format!(
                "unexpected positional argument after approval-result: {v}. {usage}"
            ));
        }
        positionals.push(v);
    }
    if positionals.is_empty() {
        return Err(format!("missing workspace argument. {usage}"));
    }
    let workspace_root = PathBuf::from(positionals[0]);
    let approval_result_path = positionals.get(1).map(|s| PathBuf::from(*s));
    Ok(CliAction::PlanStatus {
        workspace_root,
        approval_result_path,
    })
}

fn try_resolve_bare_skill_prompt(cwd: &Path, trimmed: &str) -> Option<String> {
    let bare_first_token = trimmed.split_whitespace().next().unwrap_or_default();
    let looks_like_skill_name = !bare_first_token.is_empty()
        && !bare_first_token.starts_with('/')
        && bare_first_token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    if !looks_like_skill_name {
        return None;
    }
    match resolve_skill_invocation(cwd, Some(trimmed)) {
        Ok(SkillSlashDispatch::Invoke(prompt)) => Some(prompt),
        _ => None,
    }
}

fn join_optional_args(args: &[String]) -> Option<String> {
    let joined = args.join(" ");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn parse_direct_slash_cli_action(
    rest: &[String],
    model: String,
    output_format: CliOutputFormat,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    compact: bool,
    base_commit: Option<String>,
    reasoning_effort: Option<String>,
    allow_broad_cwd: bool,
) -> Result<CliAction, String> {
    let raw = rest.join(" ");
    match SlashCommand::parse(&raw) {
        Ok(Some(SlashCommand::Help)) => Ok(CliAction::Help { output_format }),
        Ok(Some(SlashCommand::Agents { args })) => Ok(CliAction::Agents {
            args,
            output_format,
        }),
        Ok(Some(SlashCommand::Mcp { action, target })) => Ok(CliAction::Mcp {
            args: match (action, target) {
                (None, None) => None,
                (Some(action), None) => Some(action),
                (Some(action), Some(target)) => Some(format!("{action} {target}")),
                (None, Some(target)) => Some(target),
            },
            output_format,
        }),
        Ok(Some(SlashCommand::Skills { args })) => {
            match classify_skills_slash_command(args.as_deref()) {
                SkillSlashDispatch::Invoke(prompt) => Ok(CliAction::Prompt {
                    prompt,
                    model,
                    output_format,
                    allowed_tools,
                    permission_mode,
                    compact,
                    base_commit,
                    reasoning_effort: reasoning_effort.clone(),
                    allow_broad_cwd,
                }),
                SkillSlashDispatch::Local => Ok(CliAction::Skills {
                    args,
                    output_format,
                }),
            }
        }
        Ok(Some(SlashCommand::Unknown(name))) => Err(format_unknown_direct_slash_command(&name)),
        Ok(Some(command)) => Err({
            let _ = command;
            format!(
                "slash command {command_name} is interactive-only. Start `claw` and run it there, or use `claw --resume SESSION.jsonl {command_name}` / `claw --resume {latest} {command_name}` when the command is marked [resume] in /help.",
                command_name = rest[0],
                latest = LATEST_SESSION_REFERENCE,
            )
        }),
        Ok(None) => Err(format!("unknown subcommand: {}", rest[0])),
        Err(error) => Err(error.to_string()),
    }
}

fn format_unknown_option(option: &str) -> String {
    let mut message = format!("unknown option: {option}");
    if let Some(suggestion) = suggest_closest_term(option, CLI_OPTION_SUGGESTIONS) {
        message.push_str("\nDid you mean ");
        message.push_str(suggestion);
        message.push('?');
    }
    message.push_str("\nRun `claw --help` for usage.");
    message
}

fn format_unknown_direct_slash_command(name: &str) -> String {
    let mut message = format!("unknown slash command outside the REPL: /{name}");
    if let Some(suggestions) = render_suggestion_line("Did you mean", &suggest_slash_commands(name))
    {
        message.push('\n');
        message.push_str(&suggestions);
    }
    if let Some(note) = omc_compatibility_note_for_unknown_slash_command(name) {
        message.push('\n');
        message.push_str(note);
    }
    message.push_str("\nRun `claw --help` for CLI usage, or start `claw` and use /help.");
    message
}

fn format_unknown_slash_command(name: &str) -> String {
    let mut message = format!("Unknown slash command: /{name}");
    if let Some(suggestions) = render_suggestion_line("Did you mean", &suggest_slash_commands(name))
    {
        message.push('\n');
        message.push_str(&suggestions);
    }
    if let Some(note) = omc_compatibility_note_for_unknown_slash_command(name) {
        message.push('\n');
        message.push_str(note);
    }
    message.push_str("\n  Help             /help lists available slash commands");
    message
}

fn omc_compatibility_note_for_unknown_slash_command(name: &str) -> Option<&'static str> {
    name.starts_with("oh-my-claudecode:")
        .then_some(
            "Compatibility note: `/oh-my-claudecode:*` is a Claude Code/OMC plugin command. `claw` does not yet load plugin slash commands, Claude statusline stdin, or OMC session hooks.",
        )
}

fn render_suggestion_line(label: &str, suggestions: &[String]) -> Option<String> {
    (!suggestions.is_empty()).then(|| format!("  {label:<16} {}", suggestions.join(", "),))
}

fn suggest_slash_commands(input: &str) -> Vec<String> {
    let mut candidates = slash_command_specs()
        .iter()
        .flat_map(|spec| {
            std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(|name| format!("/{name}"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    let candidate_refs = candidates.iter().map(String::as_str).collect::<Vec<_>>();
    ranked_suggestions(input.trim_start_matches('/'), &candidate_refs)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn suggest_closest_term<'a>(input: &str, candidates: &'a [&'a str]) -> Option<&'a str> {
    ranked_suggestions(input, candidates).into_iter().next()
}

fn suggest_similar_subcommand(input: &str) -> Option<Vec<String>> {
    const KNOWN_SUBCOMMANDS: &[&str] = &[
        "help",
        "version",
        "status",
        "sandbox",
        "doctor",
        "state",
        "dump-manifests",
        "bootstrap-plan",
        "agents",
        "mcp",
        "skills",
        "system-prompt",
        "acp",
        "init",
        "export",
        "prompt",
    ];

    let normalized_input = input.to_ascii_lowercase();
    let mut ranked = KNOWN_SUBCOMMANDS
        .iter()
        .filter_map(|candidate| {
            let normalized_candidate = candidate.to_ascii_lowercase();
            let distance = levenshtein_distance(&normalized_input, &normalized_candidate);
            let prefix_match = common_prefix_len(&normalized_input, &normalized_candidate) >= 4;
            let substring_match = normalized_candidate.contains(&normalized_input)
                || normalized_input.contains(&normalized_candidate);
            ((distance <= 2) || prefix_match || substring_match).then_some((distance, *candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.cmp(right).then_with(|| left.1.cmp(right.1)));
    ranked.dedup_by(|left, right| left.1 == right.1);
    let suggestions = ranked
        .into_iter()
        .map(|(_, candidate)| candidate.to_string())
        .take(3)
        .collect::<Vec<_>>();
    (!suggestions.is_empty()).then_some(suggestions)
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(l, r)| l == r)
        .count()
}

fn looks_like_subcommand_typo(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
}

fn ranked_suggestions<'a>(input: &str, candidates: &'a [&'a str]) -> Vec<&'a str> {
    let normalized_input = input.trim_start_matches('/').to_ascii_lowercase();
    let mut ranked = candidates
        .iter()
        .filter_map(|candidate| {
            let normalized_candidate = candidate.trim_start_matches('/').to_ascii_lowercase();
            let distance = levenshtein_distance(&normalized_input, &normalized_candidate);
            let prefix_bonus = usize::from(
                !(normalized_candidate.starts_with(&normalized_input)
                    || normalized_input.starts_with(&normalized_candidate)),
            );
            let score = distance + prefix_bonus;
            (score <= 4).then_some((score, *candidate))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.cmp(right).then_with(|| left.1.cmp(right.1)));
    ranked
        .into_iter()
        .map(|(_, candidate)| candidate)
        .take(3)
        .collect()
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let substitution_cost = usize::from(left_char != *right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        previous.clone_from(&current);
    }

    previous[right_chars.len()]
}

fn resolve_model_alias(model: &str) -> &str {
    match model {
        "opus" => "claude-opus-4-6",
        "sonnet" => "claude-sonnet-4-6",
        "haiku" => "claude-haiku-4-5-20251213",
        _ => model,
    }
}

/// Resolve a model name through user-defined config aliases first, then fall
/// back to the built-in alias table. This is the entry point used wherever a
/// user-supplied model string is about to be dispatched to a provider.
fn resolve_model_alias_with_config(model: &str) -> String {
    let trimmed = model.trim();
    // Env-level aliases (e.g. `RUSTY_CLAUDE_MODEL_ALIAS__FAST=qwen3:14b` from
    // an opt-in profile like `examples/sidestack-local.env`) win over both the
    // repo-level config aliases and the built-in table: an operator who set an
    // env var in their current shell expects that override to be authoritative.
    if let Some(resolved) = resolve_model_env_alias(trimmed) {
        return resolve_model_alias(&resolved).to_string();
    }
    if let Some(resolved) = config_alias_for_current_dir(trimmed) {
        return resolve_model_alias(&resolved).to_string();
    }
    resolve_model_alias(trimmed).to_string()
}

/// Look up an opt-in env model alias for `model`.
///
/// Mirrors the provider-layer logic in `api::providers::openai_compat`:
/// only bare alias names (ASCII alphanumeric + `_`) trigger a lookup, the
/// key is `RUSTY_CLAUDE_MODEL_ALIAS__` + uppercased input, blank values are
/// treated as absent. Keeping the two implementations in lockstep is what
/// lets `claw --model fast` reach the provider with the same effective
/// model the provider would have computed on its own.
fn resolve_model_env_alias(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let key = format!("RUSTY_CLAUDE_MODEL_ALIAS__{}", trimmed.to_ascii_uppercase());
    let value = std::env::var(&key).ok()?;
    let trimmed_value = value.trim();
    if trimmed_value.is_empty() {
        None
    } else {
        Some(trimmed_value.to_string())
    }
}

/// Validate model syntax at parse time.
/// Accepts: known aliases (opus, sonnet, haiku), env-defined aliases
/// (`RUSTY_CLAUDE_MODEL_ALIAS__*`), or `provider/model` patterns.
/// Rejects: empty, whitespace-only, strings with spaces, or invalid chars.
fn validate_model_syntax(model: &str) -> Result<(), String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err("model string cannot be empty".to_string());
    }
    // Known aliases are always valid
    match trimmed {
        "opus" | "sonnet" | "haiku" => return Ok(()),
        _ => {}
    }
    // Opt-in env aliases unlock bare names like `--model fast` so the CLI
    // accepts what the operator's broker profile exports. If the env value
    // is blank or absent, validation falls through to the strict
    // provider/model checks below — typos like `--model fast` without a
    // profile loaded still surface the helpful `invalid_model_syntax`
    // error.
    if resolve_model_env_alias(trimmed).is_some() {
        return Ok(());
    }
    // Check for spaces (malformed)
    if trimmed.contains(' ') {
        return Err(format!(
            "invalid model syntax: '{}' contains spaces. Use provider/model format or known alias",
            trimmed
        ));
    }
    // Check provider/model format: provider_id/model_id
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        // #154: hint if the model looks like it belongs to a different provider
        let mut err_msg = format!(
            "invalid model syntax: '{}'. Expected provider/model (e.g., anthropic/claude-opus-4-6) or known alias (opus, sonnet, haiku)",
            trimmed
        );
        if trimmed.starts_with("gpt-") || trimmed.starts_with("gpt_") {
            err_msg.push_str("\nDid you mean `openai/");
            err_msg.push_str(trimmed);
            err_msg.push_str("`? (Requires OPENAI_API_KEY env var)");
        } else if trimmed.starts_with("qwen") {
            err_msg.push_str("\nDid you mean `qwen/");
            err_msg.push_str(trimmed);
            err_msg.push_str("`? (Requires DASHSCOPE_API_KEY env var)");
        } else if trimmed.starts_with("grok") {
            err_msg.push_str("\nDid you mean `xai/");
            err_msg.push_str(trimmed);
            err_msg.push_str("`? (Requires XAI_API_KEY env var)");
        }
        return Err(err_msg);
    }
    Ok(())
}

fn config_alias_for_current_dir(alias: &str) -> Option<String> {
    if alias.is_empty() {
        return None;
    }
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().ok()?;
    config.aliases().get(alias).cloned()
}

fn normalize_allowed_tools(values: &[String]) -> Result<Option<AllowedToolSet>, String> {
    if values.is_empty() {
        return Ok(None);
    }
    current_tool_registry()?.normalize_allowed_tools(values)
}

fn current_tool_registry() -> Result<GlobalToolRegistry, String> {
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load().map_err(|error| error.to_string())?;
    let state = build_runtime_plugin_state_with_loader(&cwd, &loader, &runtime_config)
        .map_err(|error| error.to_string())?;
    let registry = state.tool_registry.clone();
    if let Some(mcp_state) = state.mcp_state {
        mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown()
            .map_err(|error| error.to_string())?;
    }
    Ok(registry)
}

fn parse_permission_mode_arg(value: &str) -> Result<PermissionMode, String> {
    normalize_permission_mode(value)
        .ok_or_else(|| {
            format!(
                "unsupported permission mode '{value}'. Use read-only, workspace-write, or danger-full-access."
            )
        })
        .map(permission_mode_from_label)
}

fn permission_mode_from_label(mode: &str) -> PermissionMode {
    match mode {
        "read-only" => PermissionMode::ReadOnly,
        "workspace-write" => PermissionMode::WorkspaceWrite,
        "danger-full-access" => PermissionMode::DangerFullAccess,
        other => panic!("unsupported permission mode label: {other}"),
    }
}

fn permission_mode_from_resolved(mode: ResolvedPermissionMode) -> PermissionMode {
    match mode {
        ResolvedPermissionMode::ReadOnly => PermissionMode::ReadOnly,
        ResolvedPermissionMode::WorkspaceWrite => PermissionMode::WorkspaceWrite,
        ResolvedPermissionMode::DangerFullAccess => PermissionMode::DangerFullAccess,
    }
}

fn default_permission_mode() -> PermissionMode {
    // Safe-by-default: when neither RUSTY_CLAUDE_PERMISSION_MODE nor the
    // project config specifies a permission mode, fall back to ReadOnly.
    // Callers that need elevated access must opt in via the env var, the
    // project config, --permission-mode, or --dangerously-skip-permissions.
    env::var("RUSTY_CLAUDE_PERMISSION_MODE")
        .ok()
        .as_deref()
        .and_then(normalize_permission_mode)
        .map(permission_mode_from_label)
        .or_else(config_permission_mode_for_current_dir)
        .unwrap_or(PermissionMode::ReadOnly)
}

fn config_permission_mode_for_current_dir() -> Option<PermissionMode> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    loader
        .load()
        .ok()?
        .permission_mode()
        .map(permission_mode_from_resolved)
}

fn config_model_for_current_dir() -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    loader.load().ok()?.model().map(ToOwned::to_owned)
}

fn resolve_repl_model(cli_model: String) -> String {
    if cli_model != DEFAULT_MODEL {
        return cli_model;
    }
    if let Some(env_model) = env::var("ANTHROPIC_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return resolve_model_alias_with_config(&env_model);
    }
    if let Some(config_model) = config_model_for_current_dir() {
        return resolve_model_alias_with_config(&config_model);
    }
    cli_model
}

fn provider_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Xai => "xai",
        ProviderKind::OpenAi => "openai",
    }
}

fn format_connected_line(model: &str) -> String {
    let provider = provider_label(detect_provider_kind(model));
    format!("Connected: {model} via {provider}")
}

fn filter_tool_specs(
    tool_registry: &GlobalToolRegistry,
    allowed_tools: Option<&AllowedToolSet>,
) -> Vec<ToolDefinition> {
    tool_registry.definitions(allowed_tools)
}

fn parse_system_prompt_args(
    args: &[String],
    output_format: CliOutputFormat,
) -> Result<CliAction, String> {
    let mut cwd = env::current_dir().map_err(|error| error.to_string())?;
    let mut date = DEFAULT_DATE.to_string();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--cwd" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --cwd".to_string())?;
                cwd = PathBuf::from(value);
                index += 2;
            }
            "--date" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --date".to_string())?;
                date.clone_from(value);
                index += 2;
            }
            other => {
                // #152: hint `--output-format json` when user types `--json`.
                let mut msg = format!("unknown system-prompt option: {other}");
                if other == "--json" {
                    msg.push_str("\nDid you mean `--output-format json`?");
                }
                return Err(msg);
            }
        }
    }

    Ok(CliAction::PrintSystemPrompt {
        cwd,
        date,
        output_format,
    })
}

fn parse_export_args(args: &[String], output_format: CliOutputFormat) -> Result<CliAction, String> {
    let mut session_reference = LATEST_SESSION_REFERENCE.to_string();
    let mut output_path: Option<PathBuf> = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--session" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --session".to_string())?;
                session_reference.clone_from(value);
                index += 2;
            }
            flag if flag.starts_with("--session=") => {
                session_reference = flag[10..].to_string();
                index += 1;
            }
            "--output" | "-o" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {}", args[index]))?;
                output_path = Some(PathBuf::from(value));
                index += 2;
            }
            flag if flag.starts_with("--output=") => {
                output_path = Some(PathBuf::from(&flag[9..]));
                index += 1;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown export option: {other}"));
            }
            other if output_path.is_none() => {
                output_path = Some(PathBuf::from(other));
                index += 1;
            }
            other => {
                return Err(format!("unexpected export argument: {other}"));
            }
        }
    }

    Ok(CliAction::Export {
        session_reference,
        output_path,
        output_format,
    })
}

fn parse_dump_manifests_args(
    args: &[String],
    output_format: CliOutputFormat,
) -> Result<CliAction, String> {
    let mut manifests_dir: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--manifests-dir" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| String::from("--manifests-dir requires a path"))?;
            manifests_dir = Some(PathBuf::from(value));
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--manifests-dir=") {
            if value.is_empty() {
                return Err(String::from("--manifests-dir requires a path"));
            }
            manifests_dir = Some(PathBuf::from(value));
            index += 1;
            continue;
        }
        return Err(format!("unknown dump-manifests option: {arg}"));
    }

    Ok(CliAction::DumpManifests {
        output_format,
        manifests_dir,
    })
}

fn parse_resume_args(args: &[String], output_format: CliOutputFormat) -> Result<CliAction, String> {
    let (session_path, command_tokens): (PathBuf, &[String]) = match args.first() {
        None => (PathBuf::from(LATEST_SESSION_REFERENCE), &[]),
        Some(first) if looks_like_slash_command_token(first) => {
            (PathBuf::from(LATEST_SESSION_REFERENCE), args)
        }
        Some(first) => (PathBuf::from(first), &args[1..]),
    };
    let mut commands = Vec::new();
    let mut current_command = String::new();

    for token in command_tokens {
        if token.trim_start().starts_with('/') {
            if resume_command_can_absorb_token(&current_command, token) {
                current_command.push(' ');
                current_command.push_str(token);
                continue;
            }
            if !current_command.is_empty() {
                commands.push(current_command);
            }
            current_command = String::from(token.as_str());
            continue;
        }

        if current_command.is_empty() {
            return Err("--resume trailing arguments must be slash commands".to_string());
        }

        current_command.push(' ');
        current_command.push_str(token);
    }

    if !current_command.is_empty() {
        commands.push(current_command);
    }

    Ok(CliAction::ResumeSession {
        session_path,
        commands,
        output_format,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticLevel {
    Ok,
    Warn,
    Fail,
}

impl DiagnosticLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    fn is_failure(self) -> bool {
        matches!(self, Self::Fail)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagnosticCheck {
    name: &'static str,
    level: DiagnosticLevel,
    summary: String,
    details: Vec<String>,
    data: Map<String, Value>,
}

impl DiagnosticCheck {
    fn new(name: &'static str, level: DiagnosticLevel, summary: impl Into<String>) -> Self {
        Self {
            name,
            level,
            summary: summary.into(),
            details: Vec::new(),
            data: Map::new(),
        }
    }

    fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }

    fn with_data(mut self, data: Map<String, Value>) -> Self {
        self.data = data;
        self
    }

    fn json_value(&self) -> Value {
        let mut value = Map::from_iter([
            (
                "name".to_string(),
                Value::String(self.name.to_ascii_lowercase()),
            ),
            (
                "status".to_string(),
                Value::String(self.level.label().to_string()),
            ),
            ("summary".to_string(), Value::String(self.summary.clone())),
            (
                "details".to_string(),
                Value::Array(
                    self.details
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>(),
                ),
            ),
        ]);
        value.extend(self.data.clone());
        Value::Object(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorReport {
    checks: Vec<DiagnosticCheck>,
}

impl DoctorReport {
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.checks
                .iter()
                .filter(|check| check.level == DiagnosticLevel::Ok)
                .count(),
            self.checks
                .iter()
                .filter(|check| check.level == DiagnosticLevel::Warn)
                .count(),
            self.checks
                .iter()
                .filter(|check| check.level == DiagnosticLevel::Fail)
                .count(),
        )
    }

    fn has_failures(&self) -> bool {
        self.checks.iter().any(|check| check.level.is_failure())
    }

    fn render(&self) -> String {
        let (ok_count, warn_count, fail_count) = self.counts();
        let mut lines = vec![
            "Doctor".to_string(),
            format!(
                "Summary\n  OK               {ok_count}\n  Warnings         {warn_count}\n  Failures         {fail_count}"
            ),
        ];
        lines.extend(self.checks.iter().map(render_diagnostic_check));
        lines.join("\n\n")
    }

    fn json_value(&self) -> Value {
        let report = self.render();
        let (ok_count, warn_count, fail_count) = self.counts();
        json!({
            "kind": "doctor",
            "message": report,
            "report": report,
            "has_failures": self.has_failures(),
            "summary": {
                "total": self.checks.len(),
                "ok": ok_count,
                "warnings": warn_count,
                "failures": fail_count,
            },
            "checks": self
                .checks
                .iter()
                .map(DiagnosticCheck::json_value)
                .collect::<Vec<_>>(),
        })
    }
}

fn render_diagnostic_check(check: &DiagnosticCheck) -> String {
    let mut lines = vec![format!(
        "{}\n  Status           {}\n  Summary          {}",
        check.name,
        check.level.label(),
        check.summary
    )];
    if !check.details.is_empty() {
        lines.push("  Details".to_string());
        lines.extend(check.details.iter().map(|detail| format!("    - {detail}")));
    }
    lines.join("\n")
}

fn render_doctor_report() -> Result<DoctorReport, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let config_loader = ConfigLoader::default_for(&cwd);
    let config = config_loader.load();
    let discovered_config = config_loader.discover();
    let project_context = ProjectContext::discover_with_git(&cwd, DEFAULT_DATE)?;
    let (project_root, git_branch) =
        parse_git_status_metadata(project_context.git_status.as_deref());
    let git_summary = parse_git_workspace_summary(project_context.git_status.as_deref());
    let empty_config = runtime::RuntimeConfig::empty();
    let sandbox_config = config.as_ref().ok().unwrap_or(&empty_config);
    let context = StatusContext {
        cwd: cwd.clone(),
        session_path: None,
        loaded_config_files: config
            .as_ref()
            .ok()
            .map_or(0, |runtime_config| runtime_config.loaded_entries().len()),
        discovered_config_files: discovered_config.len(),
        memory_file_count: project_context.instruction_files.len(),
        project_root,
        git_branch,
        git_summary,
        sandbox_status: resolve_sandbox_status(sandbox_config.sandbox(), &cwd),
        // Doctor path has its own config check; StatusContext here is only
        // fed into health renderers that don't read config_load_error.
        config_load_error: config.as_ref().err().map(ToString::to_string),
    };
    Ok(DoctorReport {
        checks: vec![
            check_auth_health(),
            check_config_health(&config_loader, config.as_ref()),
            check_install_source_health(),
            check_workspace_health(&context),
            check_sandbox_health(&context.sandbox_status),
            check_system_health(&cwd, config.as_ref().ok()),
        ],
    })
}

fn run_doctor(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let report = render_doctor_report()?;
    let message = report.render();
    match output_format {
        CliOutputFormat::Text => println!("{message}"),
        CliOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report.json_value())?);
        }
    }
    if report.has_failures() {
        return Err("doctor found failing checks".into());
    }
    Ok(())
}

/// Starts a minimal Model Context Protocol server that exposes claw's
/// built-in tools over stdio.
///
/// Tool descriptors come from [`tools::mvp_tool_specs`] and calls are
/// dispatched through [`tools::execute_tool`], so this server exposes exactly
/// Read `.claw/worker-state.json` from the current working directory and print it.
/// This is the file-based worker observability surface: `push_event()` in `worker_boot.rs`
/// atomically writes state transitions here so external observers (clawhip, orchestrators)
/// can poll current `WorkerStatus` without needing an HTTP route on the opencode binary.
fn run_worker_state(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let state_path = cwd.join(".claw").join("worker-state.json");
    if !state_path.exists() {
        // #139: this error used to say "run a worker first" without telling
        // callers how to run one. "worker" is an internal concept (there is
        // no `claw worker` subcommand), so claws/CI had no discoverable path
        // from the error to a fix. Emit an actionable, structured error that
        // names the two concrete commands that produce worker state.
        //
        // Format in both text and JSON modes is stable so scripts can match:
        //   error: no worker state file found at <path>
        //     Hint: worker state is written by the interactive REPL or a non-interactive prompt.
        //     Run:   claw               # start the REPL (writes state on first turn)
        //     Or:    claw prompt <text> # run one non-interactive turn
        //     Then rerun: claw state [--output-format json]
        return Err(format!(
            "no worker state file found at {path}\n  Hint: worker state is written by the interactive REPL or a non-interactive prompt.\n  Run:   claw               # start the REPL (writes state on first turn)\n  Or:    claw prompt <text> # run one non-interactive turn\n  Then rerun: claw state [--output-format json]",
            path = state_path.display()
        )
        .into());
    }
    let raw = std::fs::read_to_string(&state_path)?;
    match output_format {
        CliOutputFormat::Text => println!("{raw}"),
        CliOutputFormat::Json => {
            // Validate it parses as JSON before re-emitting
            let _: serde_json::Value = serde_json::from_str(&raw)?;
            println!("{raw}");
        }
    }
    Ok(())
}

/// the same surface the in-process agent loop uses.
fn run_mcp_serve() -> Result<(), Box<dyn std::error::Error>> {
    let tools = mvp_tool_specs()
        .into_iter()
        .map(|spec| McpTool {
            name: spec.name.to_string(),
            description: Some(spec.description.to_string()),
            input_schema: Some(spec.input_schema),
            annotations: None,
            meta: None,
        })
        .collect();

    let spec = McpServerSpec {
        server_name: "claw".to_string(),
        server_version: VERSION.to_string(),
        tools,
        tool_handler: Box::new(execute_tool),
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut server = McpServer::new(spec);
        server.run().await
    })?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_auth_health() -> DiagnosticCheck {
    let api_key_present = env::var("ANTHROPIC_API_KEY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let auth_token_present = env::var("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let env_details = format!(
        "Environment       api_key={} auth_token={}",
        if api_key_present { "present" } else { "absent" },
        if auth_token_present {
            "present"
        } else {
            "absent"
        }
    );

    match load_oauth_credentials() {
        Ok(Some(token_set)) => DiagnosticCheck::new(
            "Auth",
            if api_key_present || auth_token_present {
                DiagnosticLevel::Ok
            } else {
                DiagnosticLevel::Warn
            },
            if api_key_present || auth_token_present {
                "supported auth env vars are configured; legacy saved OAuth is ignored"
            } else {
                "legacy saved OAuth credentials are present but unsupported"
            },
        )
        .with_details(vec![
            env_details,
            format!(
                "Legacy OAuth      expires_at={} refresh_token={} scopes={}",
                token_set
                    .expires_at
                    .map_or_else(|| "<none>".to_string(), |value| value.to_string()),
                if token_set.refresh_token.is_some() {
                    "present"
                } else {
                    "absent"
                },
                if token_set.scopes.is_empty() {
                    "<none>".to_string()
                } else {
                    token_set.scopes.join(",")
                }
            ),
            "Suggested action  set ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN; `claw login` is removed"
                .to_string(),
        ])
        .with_data(Map::from_iter([
            ("api_key_present".to_string(), json!(api_key_present)),
            ("auth_token_present".to_string(), json!(auth_token_present)),
            ("legacy_saved_oauth_present".to_string(), json!(true)),
            (
                "legacy_saved_oauth_expires_at".to_string(),
                json!(token_set.expires_at),
            ),
            (
                "legacy_refresh_token_present".to_string(),
                json!(token_set.refresh_token.is_some()),
            ),
            ("legacy_scopes".to_string(), json!(token_set.scopes)),
        ])),
        Ok(None) => DiagnosticCheck::new(
            "Auth",
            if api_key_present || auth_token_present {
                DiagnosticLevel::Ok
            } else {
                DiagnosticLevel::Warn
            },
            if api_key_present || auth_token_present {
                "supported auth env vars are configured"
            } else {
                "no supported auth env vars were found"
            },
        )
        .with_details(vec![env_details])
        .with_data(Map::from_iter([
            ("api_key_present".to_string(), json!(api_key_present)),
            ("auth_token_present".to_string(), json!(auth_token_present)),
            ("legacy_saved_oauth_present".to_string(), json!(false)),
            ("legacy_saved_oauth_expires_at".to_string(), Value::Null),
            ("legacy_refresh_token_present".to_string(), json!(false)),
            ("legacy_scopes".to_string(), json!(Vec::<String>::new())),
        ])),
        Err(error) => DiagnosticCheck::new(
            "Auth",
            DiagnosticLevel::Fail,
            format!("failed to inspect legacy saved credentials: {error}"),
        )
        .with_data(Map::from_iter([
            ("api_key_present".to_string(), json!(api_key_present)),
            ("auth_token_present".to_string(), json!(auth_token_present)),
            ("legacy_saved_oauth_present".to_string(), Value::Null),
            ("legacy_saved_oauth_expires_at".to_string(), Value::Null),
            ("legacy_refresh_token_present".to_string(), Value::Null),
            ("legacy_scopes".to_string(), Value::Null),
            ("legacy_saved_oauth_error".to_string(), json!(error.to_string())),
        ])),
    }
}

fn check_config_health(
    config_loader: &ConfigLoader,
    config: Result<&runtime::RuntimeConfig, &runtime::ConfigError>,
) -> DiagnosticCheck {
    let discovered = config_loader.discover();
    let discovered_count = discovered.len();
    // Separate candidate paths that actually exist from those that don't.
    // Showing non-existent paths as "Discovered file" implies they loaded
    // but something went wrong, which is confusing. We only surface paths
    // that exist on disk as discovered; non-existent ones are silently
    // omitted from the display (they are just the standard search locations).
    let present_paths: Vec<String> = discovered
        .iter()
        .filter(|e| e.path.exists())
        .map(|e| e.path.display().to_string())
        .collect();
    let discovered_paths = discovered
        .iter()
        .map(|entry| entry.path.display().to_string())
        .collect::<Vec<_>>();
    match config {
        Ok(runtime_config) => {
            let loaded_entries = runtime_config.loaded_entries();
            let loaded_count = loaded_entries.len();
            let present_count = present_paths.len();
            let mut details = vec![format!(
                "Config files      loaded {}/{}",
                loaded_count, present_count
            )];
            if let Some(model) = runtime_config.model() {
                details.push(format!("Resolved model    {model}"));
            }
            details.push(format!(
                "MCP servers       {}",
                runtime_config.mcp().servers().len()
            ));
            if present_paths.is_empty() {
                details.push("Discovered files  <none> (defaults active)".to_string());
            } else {
                details.extend(
                    present_paths
                        .iter()
                        .map(|path| format!("Discovered file   {path}")),
                );
            }
            DiagnosticCheck::new(
                "Config",
                DiagnosticLevel::Ok,
                if present_count == 0 {
                    "no config files present; defaults are active"
                } else {
                    "runtime config loaded successfully"
                },
            )
            .with_details(details)
            .with_data(Map::from_iter([
                ("discovered_files".to_string(), json!(present_paths)),
                ("discovered_files_count".to_string(), json!(present_count)),
                ("loaded_config_files".to_string(), json!(loaded_count)),
                ("resolved_model".to_string(), json!(runtime_config.model())),
                (
                    "mcp_servers".to_string(),
                    json!(runtime_config.mcp().servers().len()),
                ),
            ]))
        }
        Err(error) => DiagnosticCheck::new(
            "Config",
            DiagnosticLevel::Fail,
            format!("runtime config failed to load: {error}"),
        )
        .with_details(if discovered_paths.is_empty() {
            vec!["Discovered files  <none>".to_string()]
        } else {
            discovered_paths
                .iter()
                .map(|path| format!("Discovered file   {path}"))
                .collect()
        })
        .with_data(Map::from_iter([
            ("discovered_files".to_string(), json!(discovered_paths)),
            (
                "discovered_files_count".to_string(),
                json!(discovered_count),
            ),
            ("loaded_config_files".to_string(), json!(0)),
            ("resolved_model".to_string(), Value::Null),
            ("mcp_servers".to_string(), Value::Null),
            ("load_error".to_string(), json!(error.to_string())),
        ])),
    }
}

fn check_install_source_health() -> DiagnosticCheck {
    DiagnosticCheck::new(
        "Install source",
        DiagnosticLevel::Ok,
        format!(
            "official source of truth is {OFFICIAL_REPO_SLUG}; avoid `{DEPRECATED_INSTALL_COMMAND}`"
        ),
    )
    .with_details(vec![
        format!("Official repo     {OFFICIAL_REPO_URL}"),
        "Recommended path  build from this repo or use the upstream binary documented in README.md"
            .to_string(),
        format!(
            "Deprecated crate  `{DEPRECATED_INSTALL_COMMAND}` installs a deprecated stub and does not provide the `claw` binary"
        )
            .to_string(),
    ])
    .with_data(Map::from_iter([
        ("official_repo".to_string(), json!(OFFICIAL_REPO_URL)),
        (
            "deprecated_install".to_string(),
            json!(DEPRECATED_INSTALL_COMMAND),
        ),
        (
            "recommended_install".to_string(),
            json!("build from source or follow the upstream binary instructions in README.md"),
        ),
    ]))
}

fn check_workspace_health(context: &StatusContext) -> DiagnosticCheck {
    let in_repo = context.project_root.is_some();
    DiagnosticCheck::new(
        "Workspace",
        if in_repo {
            DiagnosticLevel::Ok
        } else {
            DiagnosticLevel::Warn
        },
        if in_repo {
            format!(
                "project root detected on branch {}",
                context.git_branch.as_deref().unwrap_or("unknown")
            )
        } else {
            "current directory is not inside a git project".to_string()
        },
    )
    .with_details(vec![
        format!("Cwd              {}", context.cwd.display()),
        format!(
            "Project root     {}",
            context
                .project_root
                .as_ref()
                .map_or_else(|| "<none>".to_string(), |path| path.display().to_string())
        ),
        format!(
            "Git branch       {}",
            context.git_branch.as_deref().unwrap_or("unknown")
        ),
        format!("Git state        {}", context.git_summary.headline()),
        format!("Changed files    {}", context.git_summary.changed_files),
        format!(
            "Memory files     {} · config files loaded {}/{}",
            context.memory_file_count, context.loaded_config_files, context.discovered_config_files
        ),
    ])
    .with_data(Map::from_iter([
        ("cwd".to_string(), json!(context.cwd.display().to_string())),
        (
            "project_root".to_string(),
            json!(context
                .project_root
                .as_ref()
                .map(|path| path.display().to_string())),
        ),
        ("in_git_repo".to_string(), json!(in_repo)),
        ("git_branch".to_string(), json!(context.git_branch)),
        (
            "git_state".to_string(),
            json!(context.git_summary.headline()),
        ),
        (
            "changed_files".to_string(),
            json!(context.git_summary.changed_files),
        ),
        (
            "memory_file_count".to_string(),
            json!(context.memory_file_count),
        ),
        (
            "loaded_config_files".to_string(),
            json!(context.loaded_config_files),
        ),
        (
            "discovered_config_files".to_string(),
            json!(context.discovered_config_files),
        ),
    ]))
}

fn check_sandbox_health(status: &runtime::SandboxStatus) -> DiagnosticCheck {
    let degraded = status.enabled && !status.active;
    let mut details = vec![
        format!("Enabled          {}", status.enabled),
        format!("Active           {}", status.active),
        format!("Supported        {}", status.supported),
        format!("Filesystem mode  {}", status.filesystem_mode.as_str()),
        format!("Filesystem live  {}", status.filesystem_active),
    ];
    if let Some(reason) = &status.fallback_reason {
        details.push(format!("Fallback reason  {reason}"));
    }
    DiagnosticCheck::new(
        "Sandbox",
        if degraded {
            DiagnosticLevel::Warn
        } else {
            DiagnosticLevel::Ok
        },
        if degraded {
            "sandbox was requested but is not currently active"
        } else if status.active {
            "sandbox protections are active"
        } else {
            "sandbox is not active for this session"
        },
    )
    .with_details(details)
    .with_data(Map::from_iter([
        ("enabled".to_string(), json!(status.enabled)),
        ("active".to_string(), json!(status.active)),
        ("supported".to_string(), json!(status.supported)),
        (
            "namespace_supported".to_string(),
            json!(status.namespace_supported),
        ),
        (
            "namespace_active".to_string(),
            json!(status.namespace_active),
        ),
        (
            "network_supported".to_string(),
            json!(status.network_supported),
        ),
        ("network_active".to_string(), json!(status.network_active)),
        (
            "filesystem_mode".to_string(),
            json!(status.filesystem_mode.as_str()),
        ),
        (
            "filesystem_active".to_string(),
            json!(status.filesystem_active),
        ),
        ("allowed_mounts".to_string(), json!(status.allowed_mounts)),
        ("in_container".to_string(), json!(status.in_container)),
        (
            "container_markers".to_string(),
            json!(status.container_markers),
        ),
        ("fallback_reason".to_string(), json!(status.fallback_reason)),
    ]))
}

fn check_system_health(cwd: &Path, config: Option<&runtime::RuntimeConfig>) -> DiagnosticCheck {
    let default_model = config.and_then(runtime::RuntimeConfig::model);
    let mut details = vec![
        format!("OS               {} {}", env::consts::OS, env::consts::ARCH),
        format!("Working dir      {}", cwd.display()),
        format!("Version          {}", VERSION),
        format!("Build target     {}", BUILD_TARGET.unwrap_or("<unknown>")),
        format!("Git SHA          {}", GIT_SHA.unwrap_or("<unknown>")),
    ];
    if let Some(model) = default_model {
        details.push(format!("Default model    {model}"));
    }
    DiagnosticCheck::new(
        "System",
        DiagnosticLevel::Ok,
        "captured local runtime metadata",
    )
    .with_details(details)
    .with_data(Map::from_iter([
        ("os".to_string(), json!(env::consts::OS)),
        ("arch".to_string(), json!(env::consts::ARCH)),
        ("working_dir".to_string(), json!(cwd.display().to_string())),
        ("version".to_string(), json!(VERSION)),
        ("build_target".to_string(), json!(BUILD_TARGET)),
        ("git_sha".to_string(), json!(GIT_SHA)),
        ("default_model".to_string(), json!(default_model)),
    ]))
}

fn resume_command_can_absorb_token(current_command: &str, token: &str) -> bool {
    matches!(
        SlashCommand::parse(current_command),
        Ok(Some(SlashCommand::Export { path: None }))
    ) && !looks_like_slash_command_token(token)
}

fn looks_like_slash_command_token(token: &str) -> bool {
    let trimmed = token.trim_start();
    let Some(name) = trimmed.strip_prefix('/').and_then(|value| {
        value
            .split_whitespace()
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) else {
        return false;
    };

    slash_command_specs()
        .iter()
        .any(|spec| spec.name == name || spec.aliases.contains(&name))
}

fn dump_manifests(
    manifests_dir: Option<&Path>,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    dump_manifests_at_path(&workspace_dir, manifests_dir, output_format)
}

const DUMP_MANIFESTS_OVERRIDE_HINT: &str =
    "Hint: set CLAUDE_CODE_UPSTREAM=/path/to/upstream or pass `claw dump-manifests --manifests-dir /path/to/upstream`.";

// Internal function for testing that accepts a workspace directory path.
fn dump_manifests_at_path(
    workspace_dir: &std::path::Path,
    manifests_dir: Option<&Path>,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = if let Some(dir) = manifests_dir {
        let resolved = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        UpstreamPaths::from_repo_root(resolved)
    } else {
        // Surface the resolved path in the error so users can diagnose missing
        // manifest files without guessing what path the binary expected.
        let resolved = workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| workspace_dir.to_path_buf());
        UpstreamPaths::from_workspace_dir(&resolved)
    };

    let source_root = paths.repo_root();
    if !source_root.exists() {
        return Err(format!(
            "Manifest source directory does not exist.\n  looked in: {}\n  {DUMP_MANIFESTS_OVERRIDE_HINT}",
            source_root.display(),
        )
        .into());
    }

    let required_paths = [
        ("src/commands.ts", paths.commands_path()),
        ("src/tools.ts", paths.tools_path()),
        ("src/entrypoints/cli.tsx", paths.cli_path()),
    ];
    let missing = required_paths
        .iter()
        .filter_map(|(label, path)| (!path.is_file()).then_some(*label))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Manifest source files are missing.\n  repo root: {}\n  missing: {}\n  {DUMP_MANIFESTS_OVERRIDE_HINT}",
            source_root.display(),
            missing.join(", "),
        )
        .into());
    }

    match extract_manifest(&paths) {
        Ok(manifest) => {
            match output_format {
                CliOutputFormat::Text => {
                    println!("commands: {}", manifest.commands.entries().len());
                    println!("tools: {}", manifest.tools.entries().len());
                    println!("bootstrap phases: {}", manifest.bootstrap.phases().len());
                }
                CliOutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "kind": "dump-manifests",
                        "commands": manifest.commands.entries().len(),
                        "tools": manifest.tools.entries().len(),
                        "bootstrap_phases": manifest.bootstrap.phases().len(),
                    }))?
                ),
            }
            Ok(())
        }
        Err(error) => Err(format!(
            "failed to extract manifests: {error}\n  looked in: {path}\n  {DUMP_MANIFESTS_OVERRIDE_HINT}",
            path = paths.repo_root().display()
        )
        .into()),
    }
}

fn print_bootstrap_plan(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let phases = runtime::BootstrapPlan::claude_code_default()
        .phases()
        .iter()
        .map(|phase| format!("{phase:?}"))
        .collect::<Vec<_>>();
    match output_format {
        CliOutputFormat::Text => {
            for phase in &phases {
                println!("- {phase}");
            }
        }
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "bootstrap-plan",
                "phases": phases,
            }))?
        ),
    }
    Ok(())
}

fn print_system_prompt(
    cwd: PathBuf,
    date: String,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let sections = load_system_prompt(cwd, date, env::consts::OS, "unknown")?;
    let message = sections.join(
        "

",
    );
    match output_format {
        CliOutputFormat::Text => println!("{message}"),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "system-prompt",
                "message": message,
                "sections": sections,
            }))?
        ),
    }
    Ok(())
}

fn print_version(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    match output_format {
        CliOutputFormat::Text => println!("{}", render_version_report()),
        CliOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&version_json_value())?);
        }
    }
    Ok(())
}

fn version_json_value() -> serde_json::Value {
    json!({
        "kind": "version",
        "message": render_version_report(),
        "version": VERSION,
        "git_sha": GIT_SHA,
        "target": BUILD_TARGET,
    })
}

#[allow(clippy::too_many_lines)]
fn resume_session(session_path: &Path, commands: &[String], output_format: CliOutputFormat) {
    let session_reference = session_path.display().to_string();
    let (handle, session) = match load_session_reference(&session_reference) {
        Ok(loaded) => loaded,
        Err(error) => {
            if output_format == CliOutputFormat::Json {
                // #77: classify session load errors for downstream consumers
                let full_message = format!("failed to restore session: {error}");
                let kind = classify_error_kind(&full_message);
                let (short_reason, hint) = split_error_hint(&full_message);
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "type": "error",
                        "error": short_reason,
                        "kind": kind,
                        "hint": hint,
                    })
                );
            } else {
                eprintln!("failed to restore session: {error}");
            }
            std::process::exit(1);
        }
    };
    let resolved_path = handle.path.clone();

    if commands.is_empty() {
        if output_format == CliOutputFormat::Json {
            println!(
                "{}",
                serde_json::json!({
                    "kind": "restored",
                    "session_id": session.session_id,
                    "path": handle.path.display().to_string(),
                    "message_count": session.messages.len(),
                })
            );
        } else {
            println!(
                "Restored session from {} ({} messages).",
                handle.path.display(),
                session.messages.len()
            );
        }
        return;
    }

    let mut session = session;
    for raw_command in commands {
        // Intercept spec commands that have no parse arm before calling
        // SlashCommand::parse — they return Err(SlashCommandParseError) which
        // formats as the confusing circular "Did you mean /X?" message.
        // STUB_COMMANDS covers both completions-filtered stubs and parse-less
        // spec entries; treat both as unsupported in resume mode.
        {
            let cmd_root = raw_command
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if STUB_COMMANDS.contains(&cmd_root) {
                if output_format == CliOutputFormat::Json {
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "type": "error",
                            "error": format!("/{cmd_root} is not yet implemented in this build"),
                            "kind": "unsupported_command",
                            "command": raw_command,
                        })
                    );
                } else {
                    eprintln!("/{cmd_root} is not yet implemented in this build");
                }
                std::process::exit(2);
            }
        }
        let command = match SlashCommand::parse(raw_command) {
            Ok(Some(command)) => command,
            Ok(None) => {
                if output_format == CliOutputFormat::Json {
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "type": "error",
                            "error": format!("unsupported resumed command: {raw_command}"),
                            "kind": "unsupported_resumed_command",
                            "command": raw_command,
                        })
                    );
                } else {
                    eprintln!("unsupported resumed command: {raw_command}");
                }
                std::process::exit(2);
            }
            Err(error) => {
                if output_format == CliOutputFormat::Json {
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "type": "error",
                            "error": error.to_string(),
                            "command": raw_command,
                        })
                    );
                } else {
                    eprintln!("{error}");
                }
                std::process::exit(2);
            }
        };
        match run_resume_command(&resolved_path, &session, &command) {
            Ok(ResumeCommandOutcome {
                session: next_session,
                message,
                json,
            }) => {
                session = next_session;
                if output_format == CliOutputFormat::Json {
                    if let Some(value) = json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&value)
                                .expect("resume command json output")
                        );
                    } else if let Some(message) = message {
                        println!("{message}");
                    }
                } else if let Some(message) = message {
                    println!("{message}");
                }
            }
            Err(error) => {
                if output_format == CliOutputFormat::Json {
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "type": "error",
                            "error": error.to_string(),
                            "command": raw_command,
                        })
                    );
                } else {
                    eprintln!("{error}");
                }
                std::process::exit(2);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ResumeCommandOutcome {
    session: Session,
    message: Option<String>,
    json: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct StatusContext {
    cwd: PathBuf,
    session_path: Option<PathBuf>,
    loaded_config_files: usize,
    discovered_config_files: usize,
    memory_file_count: usize,
    project_root: Option<PathBuf>,
    git_branch: Option<String>,
    git_summary: GitWorkspaceSummary,
    sandbox_status: runtime::SandboxStatus,
    /// #143: when `.claw.json` (or another loaded config file) fails to parse,
    /// we capture the parse error here and still populate every field that
    /// doesn't depend on runtime config (workspace, git, sandbox defaults,
    /// discovery counts). Top-level JSON output then reports
    /// `status: "degraded"` so claws can distinguish "status ran but config
    /// is broken" from "status ran cleanly".
    config_load_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct StatusUsage {
    message_count: usize,
    turns: u32,
    latest: TokenUsage,
    cumulative: TokenUsage,
    estimated_tokens: usize,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GitWorkspaceSummary {
    changed_files: usize,
    staged_files: usize,
    unstaged_files: usize,
    untracked_files: usize,
    conflicted_files: usize,
}

impl GitWorkspaceSummary {
    fn is_clean(self) -> bool {
        self.changed_files == 0
    }

    fn headline(self) -> String {
        if self.is_clean() {
            "clean".to_string()
        } else {
            let mut details = Vec::new();
            if self.staged_files > 0 {
                details.push(format!("{} staged", self.staged_files));
            }
            if self.unstaged_files > 0 {
                details.push(format!("{} unstaged", self.unstaged_files));
            }
            if self.untracked_files > 0 {
                details.push(format!("{} untracked", self.untracked_files));
            }
            if self.conflicted_files > 0 {
                details.push(format!("{} conflicted", self.conflicted_files));
            }
            format!(
                "dirty · {} files · {}",
                self.changed_files,
                details.join(", ")
            )
        }
    }
}

#[cfg(test)]
fn format_unknown_slash_command_message(name: &str) -> String {
    let suggestions = suggest_slash_commands(name);
    let mut message = format!("unknown slash command: /{name}.");
    if !suggestions.is_empty() {
        message.push_str(" Did you mean ");
        message.push_str(&suggestions.join(", "));
        message.push('?');
    }
    if let Some(note) = omc_compatibility_note_for_unknown_slash_command(name) {
        message.push(' ');
        message.push_str(note);
    }
    message.push_str(" Use /help to list available commands.");
    message
}

fn format_model_report(model: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Model
  Current model    {model}
  Session messages {message_count}
  Session turns    {turns}

Usage
  Inspect current model with /model
  Switch models with /model <name>"
    )
}

fn format_model_switch_report(previous: &str, next: &str, message_count: usize) -> String {
    format!(
        "Model updated
  Previous         {previous}
  Current          {next}
  Preserved msgs   {message_count}"
    )
}

fn format_permissions_report(mode: &str) -> String {
    let modes = [
        ("read-only", "Read/search tools only", mode == "read-only"),
        (
            "workspace-write",
            "Edit files inside the workspace",
            mode == "workspace-write",
        ),
        (
            "danger-full-access",
            "Unrestricted tool access",
            mode == "danger-full-access",
        ),
    ]
    .into_iter()
    .map(|(name, description, is_current)| {
        let marker = if is_current {
            "● current"
        } else {
            "○ available"
        };
        format!("  {name:<18} {marker:<11} {description}")
    })
    .collect::<Vec<_>>()
    .join(
        "
",
    );

    format!(
        "Permissions
  Active mode      {mode}
  Mode status      live session default

Modes
{modes}

Usage
  Inspect current mode with /permissions
  Switch modes with /permissions <mode>"
    )
}

fn format_permissions_switch_report(previous: &str, next: &str) -> String {
    format!(
        "Permissions updated
  Result           mode switched
  Previous mode    {previous}
  Active mode      {next}
  Applies to       subsequent tool calls
  Usage            /permissions to inspect current mode"
    )
}

fn format_cost_report(usage: TokenUsage) -> String {
    format!(
        "Cost
  Input tokens     {}
  Output tokens    {}
  Cache create     {}
  Cache read       {}
  Total tokens     {}",
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_creation_input_tokens,
        usage.cache_read_input_tokens,
        usage.total_tokens(),
    )
}

fn format_resume_report(session_path: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Session resumed
  Session file     {session_path}
  Messages         {message_count}
  Turns            {turns}"
    )
}

fn render_resume_usage() -> String {
    format!(
        "Resume
  Usage            /resume <session-path|session-id|{LATEST_SESSION_REFERENCE}>
  Auto-save        .claw/sessions/<session-id>.{PRIMARY_SESSION_EXTENSION}
  Tip              use /session list to inspect saved sessions"
    )
}

fn format_compact_report(removed: usize, resulting_messages: usize, skipped: bool) -> String {
    if skipped {
        format!(
            "Compact
  Result           skipped
  Reason           session below compaction threshold
  Messages kept    {resulting_messages}"
        )
    } else {
        format!(
            "Compact
  Result           compacted
  Messages removed {removed}
  Messages kept    {resulting_messages}"
        )
    }
}

fn format_auto_compaction_notice(removed: usize) -> String {
    format!("[auto-compacted: removed {removed} messages]")
}

fn parse_git_status_metadata(status: Option<&str>) -> (Option<PathBuf>, Option<String>) {
    parse_git_status_metadata_for(
        &env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        status,
    )
}

fn parse_git_status_branch(status: Option<&str>) -> Option<String> {
    let status = status?;
    let first_line = status.lines().next()?;
    let line = first_line.strip_prefix("## ")?;
    if line.starts_with("HEAD") {
        return Some("detached HEAD".to_string());
    }
    let branch = line.split(['.', ' ']).next().unwrap_or_default().trim();
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

fn parse_git_workspace_summary(status: Option<&str>) -> GitWorkspaceSummary {
    let mut summary = GitWorkspaceSummary::default();
    let Some(status) = status else {
        return summary;
    };

    for line in status.lines() {
        if line.starts_with("## ") || line.trim().is_empty() {
            continue;
        }

        summary.changed_files += 1;
        let mut chars = line.chars();
        let index_status = chars.next().unwrap_or(' ');
        let worktree_status = chars.next().unwrap_or(' ');

        if index_status == '?' && worktree_status == '?' {
            summary.untracked_files += 1;
            continue;
        }

        if index_status != ' ' {
            summary.staged_files += 1;
        }
        if worktree_status != ' ' {
            summary.unstaged_files += 1;
        }
        if (matches!(index_status, 'U' | 'A') && matches!(worktree_status, 'U' | 'A'))
            || index_status == 'U'
            || worktree_status == 'U'
        {
            summary.conflicted_files += 1;
        }
    }

    summary
}

fn resolve_git_branch_for(cwd: &Path) -> Option<String> {
    let branch = run_git_capture_in(cwd, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if !branch.is_empty() {
        return Some(branch.to_string());
    }

    let fallback = run_git_capture_in(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let fallback = fallback.trim();
    if fallback.is_empty() {
        None
    } else if fallback == "HEAD" {
        Some("detached HEAD".to_string())
    } else {
        Some(fallback.to_string())
    }
}

fn run_git_capture_in(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn find_git_root_in(cwd: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        return Err("not a git repository".into());
    }
    let path = String::from_utf8(output.stdout)?.trim().to_string();
    if path.is_empty() {
        return Err("empty git root".into());
    }
    Ok(PathBuf::from(path))
}

fn parse_git_status_metadata_for(
    cwd: &Path,
    status: Option<&str>,
) -> (Option<PathBuf>, Option<String>) {
    let branch = resolve_git_branch_for(cwd).or_else(|| parse_git_status_branch(status));
    let project_root = find_git_root_in(cwd).ok();
    (project_root, branch)
}

#[allow(clippy::too_many_lines)]
fn run_resume_command(
    session_path: &Path,
    session: &Session,
    command: &SlashCommand,
) -> Result<ResumeCommandOutcome, Box<dyn std::error::Error>> {
    match command {
        SlashCommand::Help => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_repl_help()),
            json: Some(serde_json::json!({ "kind": "help", "text": render_repl_help() })),
        }),
        SlashCommand::Compact => {
            let result = runtime::compact_session(
                session,
                CompactionConfig {
                    max_estimated_tokens: 0,
                    ..CompactionConfig::default()
                },
            );
            let removed = result.removed_message_count;
            let kept = result.compacted_session.messages.len();
            let skipped = removed == 0;
            result.compacted_session.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: result.compacted_session,
                message: Some(format_compact_report(removed, kept, skipped)),
                json: Some(serde_json::json!({
                    "kind": "compact",
                    "skipped": skipped,
                    "removed_messages": removed,
                    "kept_messages": kept,
                })),
            })
        }
        SlashCommand::Clear { confirm } => {
            if !confirm {
                return Ok(ResumeCommandOutcome {
                    session: session.clone(),
                    message: Some(
                        "clear: confirmation required; rerun with /clear --confirm".to_string(),
                    ),
                    json: Some(serde_json::json!({
                        "kind": "error",
                        "error": "confirmation required",
                        "hint": "rerun with /clear --confirm",
                    })),
                });
            }
            let backup_path = write_session_clear_backup(session, session_path)?;
            let previous_session_id = session.session_id.clone();
            let cleared = new_cli_session()?;
            let new_session_id = cleared.session_id.clone();
            cleared.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: cleared,
                message: Some(format!(
                    "Session cleared\n  Mode             resumed session reset\n  Previous session {previous_session_id}\n  Backup           {}\n  Resume previous  claw --resume {}\n  New session      {new_session_id}\n  Session file     {}",
                    backup_path.display(),
                    backup_path.display(),
                    session_path.display()
                )),
                json: Some(serde_json::json!({
                    "kind": "clear",
                    "previous_session_id": previous_session_id,
                    "new_session_id": new_session_id,
                    "backup": backup_path.display().to_string(),
                    "session_file": session_path.display().to_string(),
                })),
            })
        }
        SlashCommand::Status => {
            let tracker = UsageTracker::from_session(session);
            let usage = tracker.cumulative_usage();
            let context = status_context(Some(session_path))?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_status_report(
                    session.model.as_deref().unwrap_or("restored-session"),
                    StatusUsage {
                        message_count: session.messages.len(),
                        turns: tracker.turns(),
                        latest: tracker.current_turn_usage(),
                        cumulative: usage,
                        estimated_tokens: 0,
                    },
                    default_permission_mode().as_str(),
                    &context,
                    None, // #148: resumed sessions don't have flag provenance
                )),
                json: Some(status_json_value(
                    session.model.as_deref(),
                    StatusUsage {
                        message_count: session.messages.len(),
                        turns: tracker.turns(),
                        latest: tracker.current_turn_usage(),
                        cumulative: usage,
                        estimated_tokens: 0,
                    },
                    default_permission_mode().as_str(),
                    &context,
                    None, // #148: resumed sessions don't have flag provenance
                )),
            })
        }
        SlashCommand::Sandbox => {
            let cwd = env::current_dir()?;
            let loader = ConfigLoader::default_for(&cwd);
            let runtime_config = loader.load()?;
            let status = resolve_sandbox_status(runtime_config.sandbox(), &cwd);
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_sandbox_report(&status)),
                json: Some(sandbox_json_value(&status)),
            })
        }
        SlashCommand::Cost => {
            let usage = UsageTracker::from_session(session).cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_cost_report(usage)),
                json: Some(serde_json::json!({
                    "kind": "cost",
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "cache_creation_input_tokens": usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": usage.cache_read_input_tokens,
                    "total_tokens": usage.total_tokens(),
                })),
            })
        }
        SlashCommand::Config { section } => {
            let message = render_config_report(section.as_deref())?;
            let json = render_config_json(section.as_deref())?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(message),
                json: Some(json),
            })
        }
        SlashCommand::Mcp { action, target } => {
            let cwd = env::current_dir()?;
            let args = match (action.as_deref(), target.as_deref()) {
                (None, None) => None,
                (Some(action), None) => Some(action.to_string()),
                (Some(action), Some(target)) => Some(format!("{action} {target}")),
                (None, Some(target)) => Some(target.to_string()),
            };
            let runtime_inventory = runtime_mcp_inventory_json(&cwd, args.as_deref())?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(match runtime_inventory.as_ref() {
                    Some(inventory) => render_runtime_mcp_inventory_text(inventory),
                    None => handle_mcp_slash_command(args.as_deref(), &cwd)?,
                }),
                json: Some(match runtime_inventory {
                    Some(inventory) => inventory,
                    None => handle_mcp_slash_command_json(args.as_deref(), &cwd)?,
                }),
            })
        }
        SlashCommand::Memory => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_memory_report()?),
            json: Some(render_memory_json()?),
        }),
        SlashCommand::Init => {
            // #142: run the init once, then render both text + structured JSON
            // from the same InitReport so both surfaces stay in sync.
            let cwd = env::current_dir()?;
            let report = crate::init::initialize_repo(&cwd)?;
            let message = report.render();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(message.clone()),
                json: Some(init_json_value(&report, &message)),
            })
        }
        SlashCommand::Diff => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let message = render_diff_report_for(&cwd)?;
            let json = render_diff_json_for(&cwd)?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(message),
                json: Some(json),
            })
        }
        SlashCommand::Version => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_version_report()),
            json: Some(version_json_value()),
        }),
        SlashCommand::Export { path } => {
            let export_path = resolve_export_path(path.as_deref(), session)?;
            fs::write(&export_path, render_export_text(session))?;
            let msg_count = session.messages.len();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format!(
                    "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
                    export_path.display(),
                    msg_count,
                )),
                json: Some(serde_json::json!({
                    "kind": "export",
                    "file": export_path.display().to_string(),
                    "message_count": msg_count,
                })),
            })
        }
        SlashCommand::Agents { args } => {
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_agents_slash_command(args.as_deref(), &cwd)?),
                json: Some(serde_json::json!({
                    "kind": "agents",
                    "text": handle_agents_slash_command(args.as_deref(), &cwd)?,
                })),
            })
        }
        SlashCommand::Skills { args } => {
            if let SkillSlashDispatch::Invoke(_) = classify_skills_slash_command(args.as_deref()) {
                return Err(
                    "resumed /skills invocations are interactive-only; start `claw` and run `/skills <skill>` in the REPL".into(),
                );
            }
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_skills_slash_command(args.as_deref(), &cwd)?),
                json: Some(handle_skills_slash_command_json(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Doctor => {
            let report = render_doctor_report()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(report.render()),
                json: Some(report.json_value()),
            })
        }
        SlashCommand::Stats => {
            let usage = UsageTracker::from_session(session).cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_cost_report(usage)),
                json: Some(serde_json::json!({
                    "kind": "stats",
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "cache_creation_input_tokens": usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": usage.cache_read_input_tokens,
                    "total_tokens": usage.total_tokens(),
                })),
            })
        }
        SlashCommand::History { count } => {
            let limit = parse_history_count(count.as_deref())
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            let entries = collect_session_prompt_history(session);
            let shown: Vec<_> = entries.iter().rev().take(limit).rev().collect();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(render_prompt_history_report(&entries, limit)),
                json: Some(serde_json::json!({
                    "kind": "history",
                    "total": entries.len(),
                    "showing": shown.len(),
                    "entries": shown.iter().map(|e| serde_json::json!({
                        "timestamp_ms": e.timestamp_ms,
                        "text": e.text,
                    })).collect::<Vec<_>>(),
                })),
            })
        }
        SlashCommand::Unknown(name) => Err(format_unknown_slash_command(name).into()),
        // /session list can be served from the sessions directory without a live session.
        SlashCommand::Session {
            action: Some(ref act),
            ..
        } if act == "list" => {
            let sessions = list_managed_sessions().unwrap_or_default();
            let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
            let active_id = session.session_id.clone();
            let text = render_session_list(&active_id).unwrap_or_else(|e| format!("error: {e}"));
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(text),
                json: Some(serde_json::json!({
                    "kind": "session_list",
                    "sessions": session_ids,
                    "active": active_id,
                })),
            })
        }
        SlashCommand::Bughunter { .. }
        | SlashCommand::Commit { .. }
        | SlashCommand::Pr { .. }
        | SlashCommand::Issue { .. }
        | SlashCommand::Ultraplan { .. }
        | SlashCommand::Teleport { .. }
        | SlashCommand::DebugToolCall { .. }
        | SlashCommand::Resume { .. }
        | SlashCommand::Model { .. }
        | SlashCommand::Permissions { .. }
        | SlashCommand::Session { .. }
        | SlashCommand::Plugins { .. }
        | SlashCommand::Login
        | SlashCommand::Logout
        | SlashCommand::Vim
        | SlashCommand::Upgrade
        | SlashCommand::Share
        | SlashCommand::Feedback
        | SlashCommand::Files
        | SlashCommand::Fast
        | SlashCommand::Exit
        | SlashCommand::Summary
        | SlashCommand::Desktop
        | SlashCommand::Brief
        | SlashCommand::Advisor
        | SlashCommand::Stickers
        | SlashCommand::Insights
        | SlashCommand::Thinkback
        | SlashCommand::ReleaseNotes
        | SlashCommand::SecurityReview
        | SlashCommand::Keybindings
        | SlashCommand::PrivacySettings
        | SlashCommand::Plan { .. }
        | SlashCommand::Review { .. }
        | SlashCommand::Tasks { .. }
        | SlashCommand::Theme { .. }
        | SlashCommand::Voice { .. }
        | SlashCommand::Usage { .. }
        | SlashCommand::Rename { .. }
        | SlashCommand::Copy { .. }
        | SlashCommand::Hooks { .. }
        | SlashCommand::Context { .. }
        | SlashCommand::Color { .. }
        | SlashCommand::Effort { .. }
        | SlashCommand::Branch { .. }
        | SlashCommand::Rewind { .. }
        | SlashCommand::Ide { .. }
        | SlashCommand::Tag { .. }
        | SlashCommand::OutputStyle { .. }
        | SlashCommand::AddDir { .. } => Err("unsupported resumed slash command".into()),
    }
}

/// Detect if the current working directory is "broad" (home directory or
/// filesystem root). Returns the cwd path if broad, None otherwise.
fn detect_broad_cwd() -> Option<PathBuf> {
    let Ok(cwd) = env::current_dir() else {
        return None;
    };
    let is_home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .is_some_and(|h| Path::new(&h) == cwd);
    let is_root = cwd.parent().is_none();
    if is_home || is_root {
        Some(cwd)
    } else {
        None
    }
}

/// Enforce the broad-CWD policy: when running from home or root, either
/// require the --allow-broad-cwd flag, or prompt for confirmation (interactive),
/// or exit with an error (non-interactive).
fn enforce_broad_cwd_policy(
    allow_broad_cwd: bool,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if allow_broad_cwd {
        return Ok(());
    }
    let Some(cwd) = detect_broad_cwd() else {
        return Ok(());
    };

    let is_interactive = io::stdin().is_terminal();

    if is_interactive {
        // Interactive mode: print warning and ask for confirmation
        eprintln!(
            "Warning: claw is running from a very broad directory ({}).\n\
             The agent can read and search everything under this path.\n\
             Consider running from inside your project: cd /path/to/project && claw",
            cwd.display()
        );
        eprint!("Continue anyway? [y/N]: ");
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            eprintln!("Aborted.");
            std::process::exit(0);
        }
        Ok(())
    } else {
        // Non-interactive mode: exit with error (JSON or text)
        let message = format!(
            "claw is running from a very broad directory ({}). \
             The agent can read and search everything under this path. \
             Use --allow-broad-cwd to proceed anyway, \
             or run from inside your project: cd /path/to/project && claw",
            cwd.display()
        );
        match output_format {
            CliOutputFormat::Json => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "type": "error",
                        "error": message,
                    })
                );
            }
            CliOutputFormat::Text => {
                eprintln!("error: {message}");
            }
        }
        std::process::exit(1);
    }
}

fn run_stale_base_preflight(flag_value: Option<&str>) {
    let Ok(cwd) = env::current_dir() else {
        return;
    };
    let source = resolve_expected_base(flag_value, &cwd);
    let state = check_base_commit(&cwd, source.as_ref());
    if let Some(warning) = format_stale_base_warning(&state) {
        eprintln!("{warning}");
    }
}

#[allow(clippy::needless_pass_by_value)]
fn run_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    base_commit: Option<String>,
    reasoning_effort: Option<String>,
    allow_broad_cwd: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    enforce_broad_cwd_policy(allow_broad_cwd, CliOutputFormat::Text)?;
    run_stale_base_preflight(base_commit.as_deref());
    let resolved_model = resolve_repl_model(model);
    let mut cli = LiveCli::new(resolved_model, true, allowed_tools, permission_mode)?;
    cli.set_reasoning_effort(reasoning_effort);
    let mut editor =
        input::LineEditor::new("> ", cli.repl_completion_candidates().unwrap_or_default());
    println!("{}", cli.startup_banner());
    println!("{}", format_connected_line(&cli.model));

    loop {
        editor.set_completions(cli.repl_completion_candidates().unwrap_or_default());
        match editor.read_line()? {
            input::ReadOutcome::Submit(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed.as_str(), "/exit" | "/quit") {
                    cli.persist_session()?;
                    break;
                }
                match SlashCommand::parse(&trimmed) {
                    Ok(Some(command)) => {
                        if cli.handle_repl_command(command)? {
                            cli.persist_session()?;
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("{error}");
                        continue;
                    }
                }
                // Bare-word skill dispatch: if the first token of the input
                // matches a known skill name, invoke it as `/skills <input>`
                // rather than forwarding raw text to the LLM (ROADMAP #36).
                let cwd = std::env::current_dir().unwrap_or_default();
                if let Some(prompt) = try_resolve_bare_skill_prompt(&cwd, &trimmed) {
                    editor.push_history(input);
                    cli.record_prompt_history(&trimmed);
                    cli.run_turn(&prompt)?;
                    continue;
                }
                editor.push_history(input);
                cli.record_prompt_history(&trimmed);
                cli.run_turn(&trimmed)?;
            }
            input::ReadOutcome::Cancel => {}
            input::ReadOutcome::Exit => {
                cli.persist_session()?;
                break;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct SessionHandle {
    id: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ManagedSessionSummary {
    id: String,
    path: PathBuf,
    updated_at_ms: u64,
    modified_epoch_millis: u128,
    message_count: usize,
    parent_session_id: Option<String>,
    branch_name: Option<String>,
}

struct LiveCli {
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    system_prompt: Vec<String>,
    runtime: BuiltRuntime,
    session: SessionHandle,
    prompt_history: Vec<PromptHistoryEntry>,
}

#[derive(Debug, Clone)]
struct PromptHistoryEntry {
    timestamp_ms: u64,
    text: String,
}

struct RuntimePluginState {
    feature_config: runtime::RuntimeFeatureConfig,
    tool_registry: GlobalToolRegistry,
    plugin_registry: PluginRegistry,
    mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
}

struct RuntimeMcpState {
    runtime: tokio::runtime::Runtime,
    manager: McpServerManager,
    pending_servers: Vec<String>,
    degraded_report: Option<runtime::McpDegradedReport>,
}

struct BuiltRuntime {
    runtime: Option<ConversationRuntime<AnthropicRuntimeClient, CliToolExecutor>>,
    plugin_registry: PluginRegistry,
    plugins_active: bool,
    mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    mcp_active: bool,
}

impl BuiltRuntime {
    fn new(
        runtime: ConversationRuntime<AnthropicRuntimeClient, CliToolExecutor>,
        plugin_registry: PluginRegistry,
        mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            plugin_registry,
            plugins_active: true,
            mcp_state,
            mcp_active: true,
        }
    }

    fn with_hook_abort_signal(mut self, hook_abort_signal: runtime::HookAbortSignal) -> Self {
        let runtime = self
            .runtime
            .take()
            .expect("runtime should exist before installing hook abort signal");
        self.runtime = Some(runtime.with_hook_abort_signal(hook_abort_signal));
        self
    }

    fn shutdown_plugins(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.plugins_active {
            self.plugin_registry.shutdown()?;
            self.plugins_active = false;
        }
        Ok(())
    }

    fn shutdown_mcp(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.mcp_active {
            if let Some(mcp_state) = &self.mcp_state {
                mcp_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .shutdown()?;
            }
            self.mcp_active = false;
        }
        Ok(())
    }
}

impl Deref for BuiltRuntime {
    type Target = ConversationRuntime<AnthropicRuntimeClient, CliToolExecutor>;

    fn deref(&self) -> &Self::Target {
        self.runtime
            .as_ref()
            .expect("runtime should exist while built runtime is alive")
    }
}

impl DerefMut for BuiltRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.runtime
            .as_mut()
            .expect("runtime should exist while built runtime is alive")
    }
}

impl Drop for BuiltRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown_mcp();
        let _ = self.shutdown_plugins();
    }
}

#[derive(Debug, Deserialize)]
struct ToolSearchRequest {
    query: String,
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct McpToolRequest {
    server: Option<String>,
    #[serde(rename = "qualifiedName")]
    qualified_name: Option<String>,
    tool: Option<String>,
    arguments: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ListMcpResourcesRequest {
    server: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadMcpResourceRequest {
    server: String,
    uri: String,
}

#[derive(Debug, Deserialize)]
struct McpAuthRequest {
    server: String,
}

impl RuntimeMcpState {
    fn new(
        runtime_config: &runtime::RuntimeConfig,
    ) -> Result<Option<(Self, runtime::McpToolDiscoveryReport)>, Box<dyn std::error::Error>> {
        let mut manager = McpServerManager::from_runtime_config(runtime_config);
        if manager.server_names().is_empty() && manager.unsupported_servers().is_empty() {
            return Ok(None);
        }

        let runtime = tokio::runtime::Runtime::new()?;
        let discovery = runtime.block_on(manager.discover_tools_best_effort());
        let pending_servers = discovery
            .failed_servers
            .iter()
            .map(|failure| failure.server_name.clone())
            .chain(
                discovery
                    .unsupported_servers
                    .iter()
                    .map(|server| server.server_name.clone()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let available_tools = discovery
            .tools
            .iter()
            .map(|tool| tool.qualified_name.clone())
            .collect::<Vec<_>>();
        let failed_server_names = pending_servers.iter().cloned().collect::<BTreeSet<_>>();
        let working_servers = manager
            .server_names()
            .into_iter()
            .filter(|server_name| !failed_server_names.contains(server_name))
            .collect::<Vec<_>>();
        let failed_servers =
            discovery
                .failed_servers
                .iter()
                .map(|failure| runtime::McpFailedServer {
                    server_name: failure.server_name.clone(),
                    phase: failure.phase,
                    error: runtime::McpErrorSurface::new(
                        failure.phase,
                        Some(failure.server_name.clone()),
                        failure.error.clone(),
                        failure.context.clone(),
                        failure.recoverable,
                    ),
                })
                .chain(discovery.unsupported_servers.iter().map(|server| {
                    runtime::McpFailedServer {
                        server_name: server.server_name.clone(),
                        phase: runtime::McpLifecyclePhase::ServerRegistration,
                        error: runtime::McpErrorSurface::new(
                            runtime::McpLifecyclePhase::ServerRegistration,
                            Some(server.server_name.clone()),
                            server.reason.clone(),
                            std::collections::BTreeMap::from([(
                                "transport".to_string(),
                                format!("{:?}", server.transport).to_ascii_lowercase(),
                            )]),
                            false,
                        ),
                    }
                }))
                .collect::<Vec<_>>();
        let degraded_report = (!failed_servers.is_empty()).then(|| {
            runtime::McpDegradedReport::new(
                working_servers,
                failed_servers,
                available_tools.clone(),
                available_tools,
            )
        });

        Ok(Some((
            Self {
                runtime,
                manager,
                pending_servers,
                degraded_report,
            },
            discovery,
        )))
    }

    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.block_on(self.manager.shutdown())?;
        Ok(())
    }

    fn pending_servers(&self) -> Option<Vec<String>> {
        (!self.pending_servers.is_empty()).then(|| self.pending_servers.clone())
    }

    fn degraded_report(&self) -> Option<runtime::McpDegradedReport> {
        self.degraded_report.clone()
    }

    fn lifecycle_status_for_server(&self, server_name: &str) -> serde_json::Value {
        let failed = self.degraded_report.as_ref().and_then(|report| {
            report
                .failed_servers
                .iter()
                .find(|server| server.server_name == server_name)
        });
        if let Some(failed) = failed {
            let managed = self.server_names().iter().any(|name| name == server_name);
            let status = if managed { "error" } else { "unsupported" };
            return json!({
                "server": server_name,
                "status": status,
                "phase": failed.phase,
                "message": failed.error.message,
                "context": failed.error.context,
                "recoverable": failed.error.recoverable,
                "failure": {
                    "server_name": failed.server_name.clone(),
                    "phase": failed.phase,
                    "message": failed.error.message.clone(),
                    "context": failed.error.context.clone(),
                    "recoverable": failed.error.recoverable,
                },
            });
        }

        if self.server_names().iter().any(|name| name == server_name) {
            return json!({
                "server": server_name,
                "status": "connected",
                "phase": runtime::McpLifecyclePhase::Ready,
                "message": Value::Null,
                "context": {},
                "recoverable": Value::Null,
                "failure": Value::Null,
            });
        }

        json!({
            "server": server_name,
            "status": "disconnected",
            "phase": runtime::McpLifecyclePhase::ServerRegistration,
            "message": format!("server `{server_name}` is not configured"),
            "context": {},
            "recoverable": false,
            "failure": Value::Null,
        })
    }

    fn lifecycle_statuses(&self) -> Vec<serde_json::Value> {
        let mut server_names = self.server_names().into_iter().collect::<BTreeSet<_>>();
        if let Some(report) = &self.degraded_report {
            server_names.extend(
                report
                    .failed_servers
                    .iter()
                    .map(|server| server.server_name.clone()),
            );
        }
        server_names
            .into_iter()
            .map(|server_name| self.lifecycle_status_for_server(&server_name))
            .collect()
    }

    fn inventory_json(&self) -> serde_json::Value {
        json!({
            "servers": self.lifecycle_statuses(),
            "pending_servers": self.pending_servers(),
            "mcp_degraded": self.degraded_report(),
        })
    }

    fn server_names(&self) -> Vec<String> {
        self.manager.server_names()
    }

    fn call_tool(
        &mut self,
        qualified_tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<String, ToolError> {
        let response = self
            .runtime
            .block_on(self.manager.call_tool(qualified_tool_name, arguments))
            .map_err(|error| ToolError::new(error.to_string()))?;
        if let Some(error) = response.error {
            return Err(ToolError::new(format!(
                "MCP tool `{qualified_tool_name}` returned JSON-RPC error: {} ({})",
                error.message, error.code
            )));
        }

        let result = response.result.ok_or_else(|| {
            ToolError::new(format!(
                "MCP tool `{qualified_tool_name}` returned no result payload"
            ))
        })?;
        serde_json::to_string_pretty(&result).map_err(|error| ToolError::new(error.to_string()))
    }

    fn call_legacy_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<String, ToolError> {
        let result = self.call_tool(&mcp_tool_name(server_name, tool_name), arguments)?;
        let parsed = serde_json::from_str::<serde_json::Value>(&result)
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "tool": tool_name,
            "result": parsed,
            "status": "success"
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn list_resources_for_server(&mut self, server_name: &str) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.list_resources(server_name))
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "resources": result.resources,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn list_resources_for_server_legacy(&mut self, server_name: &str) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.list_resources(server_name))
            .map_err(|error| ToolError::new(error.to_string()))?;
        let count = result.resources.len();
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "resources": result.resources,
            "count": count,
            "source": "runtime"
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn list_resources_for_all_servers(&mut self) -> Result<String, ToolError> {
        let mut resources = Vec::new();
        let mut failures = Vec::new();

        for server_name in self.server_names() {
            match self
                .runtime
                .block_on(self.manager.list_resources(&server_name))
            {
                Ok(result) => resources.push(json!({
                    "server": server_name,
                    "resources": result.resources,
                })),
                Err(error) => failures.push(json!({
                    "server": server_name,
                    "error": error.to_string(),
                })),
            }
        }

        if resources.is_empty() && !failures.is_empty() {
            let message = failures
                .iter()
                .filter_map(|failure| failure.get("error").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ToolError::new(message));
        }

        serde_json::to_string_pretty(&json!({
            "resources": resources,
            "failures": failures,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn list_resources_for_all_servers_legacy(&mut self) -> Result<String, ToolError> {
        let mut resources = Vec::new();
        let mut failures = Vec::new();
        let mut count = 0;

        for server_name in self.server_names() {
            match self
                .runtime
                .block_on(self.manager.list_resources(&server_name))
            {
                Ok(result) => {
                    count += result.resources.len();
                    resources.push(json!({
                        "server": server_name,
                        "resources": result.resources,
                    }));
                }
                Err(error) => failures.push(json!({
                    "server": server_name,
                    "error": error.to_string(),
                })),
            }
        }

        if resources.is_empty() && !failures.is_empty() {
            let message = failures
                .iter()
                .filter_map(|failure| failure.get("error").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ToolError::new(message));
        }

        serde_json::to_string_pretty(&json!({
            "resources": resources,
            "failures": failures,
            "count": count,
            "source": "runtime"
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn read_resource(&mut self, server_name: &str, uri: &str) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.read_resource(server_name, uri))
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "contents": result.contents,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn read_resource_legacy(&mut self, server_name: &str, uri: &str) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.read_resource(server_name, uri))
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "uri": uri,
            "contents": result.contents,
            "source": "runtime"
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn auth_status(&self, server_name: &str) -> Result<String, ToolError> {
        let failed = self.degraded_report.as_ref().and_then(|report| {
            report
                .failed_servers
                .iter()
                .find(|server| server.server_name == server_name)
        });
        let status = if failed.is_some() {
            "error"
        } else if self.server_names().iter().any(|name| name == server_name) {
            "connected"
        } else {
            "disconnected"
        };

        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "status": status,
            "source": "runtime",
            "lifecycle_status": self.lifecycle_status_for_server(server_name),
            "failure": failed,
            "mcp_degraded": self.degraded_report(),
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }
}

enum RuntimeMcpInventoryAction<'a> {
    List,
    Show(&'a str),
}

fn parse_runtime_mcp_inventory_action(args: Option<&str>) -> Option<RuntimeMcpInventoryAction<'_>> {
    let args = args.map(str::trim).filter(|value| !value.is_empty());
    match args {
        None | Some("list") => Some(RuntimeMcpInventoryAction::List),
        Some(args) if matches!(args, "help" | "-h" | "--help") => None,
        Some(args) if args.split_whitespace().next() == Some("show") => {
            let mut parts = args.split_whitespace();
            let _ = parts.next();
            let server_name = parts.next()?;
            parts
                .next()
                .is_none()
                .then_some(RuntimeMcpInventoryAction::Show(server_name))
        }
        _ => None,
    }
}

fn runtime_mcp_inventory_json_for_loader(
    loader: &ConfigLoader,
    cwd: &Path,
    args: Option<&str>,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let Some(action) = parse_runtime_mcp_inventory_action(args) else {
        return Ok(None);
    };
    let runtime_config = loader.load()?;
    let configured = handle_mcp_slash_command_json(args, cwd)?;
    let lifecycle = match RuntimeMcpState::new(&runtime_config)? {
        Some((mut state, _discovery)) => {
            let lifecycle = match action {
                RuntimeMcpInventoryAction::List => state.inventory_json(),
                RuntimeMcpInventoryAction::Show(server_name) => json!({
                    "servers": [state.lifecycle_status_for_server(server_name)],
                    "pending_servers": state.pending_servers(),
                    "mcp_degraded": state.degraded_report(),
                }),
            };
            state.shutdown()?;
            lifecycle
        }
        None => match action {
            RuntimeMcpInventoryAction::List => json!({
                "servers": [],
                "pending_servers": Value::Null,
                "mcp_degraded": Value::Null,
            }),
            RuntimeMcpInventoryAction::Show(server_name) => json!({
                "servers": [{
                    "server": server_name,
                    "status": "disconnected",
                    "phase": runtime::McpLifecyclePhase::ServerRegistration,
                    "message": format!("server `{server_name}` is not configured"),
                    "context": {},
                    "recoverable": false,
                    "failure": Value::Null,
                }],
                "pending_servers": Value::Null,
                "mcp_degraded": Value::Null,
            }),
        },
    };

    let lifecycle_servers = lifecycle
        .get("servers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok(Some(json!({
        "kind": "mcp",
        "action": configured.get("action").cloned().unwrap_or_else(|| json!("list")),
        "working_directory": cwd.display().to_string(),
        "servers": lifecycle_servers,
        "configured": configured,
        "lifecycle": lifecycle,
    })))
}

fn runtime_mcp_inventory_json(
    cwd: &Path,
    args: Option<&str>,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let loader = ConfigLoader::default_for(cwd);
    runtime_mcp_inventory_json_for_loader(&loader, cwd, args)
}

fn render_runtime_mcp_inventory_text(inventory: &Value) -> String {
    let mut lines = vec![
        "MCP".to_string(),
        format!(
            "  Working directory {}",
            inventory
                .get("working_directory")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
        ),
    ];
    let statuses = inventory
        .get("lifecycle")
        .and_then(|lifecycle| lifecycle.get("servers"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    lines.push(format!("  Lifecycle servers {}", statuses.len()));
    if statuses.is_empty() {
        lines.push("  No MCP servers configured.".to_string());
        return lines.join("\n");
    }

    lines.push(String::new());
    for status in statuses {
        let server = status
            .get("server")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let state = status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let phase = status
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let recoverable = status.get("recoverable").map_or_else(
            || "<unknown>".to_string(),
            |value| {
                value
                    .as_bool()
                    .map_or_else(|| "n/a".to_string(), |flag| flag.to_string())
            },
        );
        lines.push(format!(
            "  {server:<16} {state:<13} phase={phase:<20} recoverable={recoverable}"
        ));
        if let Some(message) = status.get("message").and_then(Value::as_str) {
            lines.push(format!("    {message}"));
        }
    }
    lines.join("\n")
}

fn build_runtime_mcp_state(
    runtime_config: &runtime::RuntimeConfig,
) -> Result<RuntimePluginStateBuildOutput, Box<dyn std::error::Error>> {
    let Some((mcp_state, discovery)) = RuntimeMcpState::new(runtime_config)? else {
        return Ok((None, Vec::new()));
    };

    let mut runtime_tools = discovery
        .tools
        .iter()
        .map(mcp_runtime_tool_definition)
        .collect::<Vec<_>>();
    if !mcp_state.server_names().is_empty() {
        runtime_tools.extend(mcp_wrapper_tool_definitions());
    }

    Ok((Some(Arc::new(Mutex::new(mcp_state))), runtime_tools))
}

fn mcp_runtime_tool_definition(tool: &runtime::ManagedMcpTool) -> RuntimeToolDefinition {
    RuntimeToolDefinition {
        name: tool.qualified_name.clone(),
        description: Some(
            tool.tool
                .description
                .clone()
                .unwrap_or_else(|| format!("Invoke MCP tool `{}`.", tool.qualified_name)),
        ),
        input_schema: tool
            .tool
            .input_schema
            .clone()
            .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true })),
        required_permission: permission_mode_for_mcp_tool(&tool.tool),
    }
}

fn mcp_wrapper_tool_definitions() -> Vec<RuntimeToolDefinition> {
    vec![
        RuntimeToolDefinition {
            name: "MCPTool".to_string(),
            description: Some(
                "Call a configured MCP tool by its qualified name and JSON arguments.".to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "qualifiedName": { "type": "string" },
                    "arguments": {}
                },
                "required": ["qualifiedName"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        RuntimeToolDefinition {
            name: "ListMcpResourcesTool".to_string(),
            description: Some(
                "List MCP resources from one configured server or from every connected server."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" }
                },
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        RuntimeToolDefinition {
            name: "ReadMcpResourceTool".to_string(),
            description: Some("Read a specific MCP resource from a configured server.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" },
                    "uri": { "type": "string" }
                },
                "required": ["server", "uri"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
    ]
}

fn permission_mode_for_mcp_tool(tool: &McpTool) -> PermissionMode {
    let read_only = mcp_annotation_flag(tool, "readOnlyHint");
    let destructive = mcp_annotation_flag(tool, "destructiveHint");
    let open_world = mcp_annotation_flag(tool, "openWorldHint");

    if read_only && !destructive && !open_world {
        PermissionMode::ReadOnly
    } else if destructive || open_world {
        PermissionMode::DangerFullAccess
    } else {
        PermissionMode::WorkspaceWrite
    }
}

fn mcp_annotation_flag(tool: &McpTool, key: &str) -> bool {
    tool.annotations
        .as_ref()
        .and_then(|annotations| annotations.get(key))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

struct HookAbortMonitor {
    stop_tx: Option<Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl HookAbortMonitor {
    fn spawn(abort_signal: runtime::HookAbortSignal) -> Self {
        Self::spawn_with_waiter(abort_signal, move |stop_rx, abort_signal| {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            runtime.block_on(async move {
                let wait_for_stop = tokio::task::spawn_blocking(move || {
                    let _ = stop_rx.recv();
                });

                tokio::select! {
                    result = tokio::signal::ctrl_c() => {
                        if result.is_ok() {
                            abort_signal.abort();
                        }
                    }
                    _ = wait_for_stop => {}
                }
            });
        })
    }

    fn spawn_with_waiter<F>(abort_signal: runtime::HookAbortSignal, wait_for_interrupt: F) -> Self
    where
        F: FnOnce(Receiver<()>, runtime::HookAbortSignal) + Send + 'static,
    {
        let (stop_tx, stop_rx) = mpsc::channel();
        let join_handle = thread::spawn(move || wait_for_interrupt(stop_rx, abort_signal));

        Self {
            stop_tx: Some(stop_tx),
            join_handle: Some(join_handle),
        }
    }

    fn stop(mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl LiveCli {
    fn new(
        model: String,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let system_prompt = build_system_prompt()?;
        let session_state = new_cli_session()?;
        let session = create_managed_session_handle(&session_state.session_id)?;
        let runtime = build_runtime(
            session_state.with_persistence_path(session.path.clone()),
            &session.id,
            model.clone(),
            system_prompt.clone(),
            enable_tools,
            true,
            allowed_tools.clone(),
            permission_mode,
            None,
        )?;
        let cli = Self {
            model,
            allowed_tools,
            permission_mode,
            system_prompt,
            runtime,
            session,
            prompt_history: Vec::new(),
        };
        cli.persist_session()?;
        Ok(cli)
    }

    fn set_reasoning_effort(&mut self, effort: Option<String>) {
        if let Some(rt) = self.runtime.runtime.as_mut() {
            rt.api_client_mut().set_reasoning_effort(effort);
        }
    }

    fn startup_banner(&self) -> String {
        let cwd = env::current_dir().map_or_else(
            |_| "<unknown>".to_string(),
            |path| path.display().to_string(),
        );
        let status = status_context(None).ok();
        let git_branch = status
            .as_ref()
            .and_then(|context| context.git_branch.as_deref())
            .unwrap_or("unknown");
        let workspace = status.as_ref().map_or_else(
            || "unknown".to_string(),
            |context| context.git_summary.headline(),
        );
        let session_path = self.session.path.strip_prefix(Path::new(&cwd)).map_or_else(
            |_| self.session.path.display().to_string(),
            |path| path.display().to_string(),
        );
        format!(
            "\x1b[38;5;196m\
 ██████╗██╗      █████╗ ██╗    ██╗\n\
██╔════╝██║     ██╔══██╗██║    ██║\n\
██║     ██║     ███████║██║ █╗ ██║\n\
██║     ██║     ██╔══██║██║███╗██║\n\
╚██████╗███████╗██║  ██║╚███╔███╔╝\n\
 ╚═════╝╚══════╝╚═╝  ╚═╝ ╚══╝╚══╝\x1b[0m \x1b[38;5;208mCode\x1b[0m 🦞\n\n\
  \x1b[2mModel\x1b[0m            {}\n\
  \x1b[2mPermissions\x1b[0m      {}\n\
  \x1b[2mBranch\x1b[0m           {}\n\
  \x1b[2mWorkspace\x1b[0m        {}\n\
  \x1b[2mDirectory\x1b[0m        {}\n\
  \x1b[2mSession\x1b[0m          {}\n\
  \x1b[2mAuto-save\x1b[0m        {}\n\n\
  Type \x1b[1m/help\x1b[0m for commands · \x1b[1m/status\x1b[0m for live context · \x1b[2m/resume latest\x1b[0m jumps back to the newest session · \x1b[1m/diff\x1b[0m then \x1b[1m/commit\x1b[0m to ship · \x1b[2mTab\x1b[0m for workflow completions · \x1b[2mShift+Enter\x1b[0m for newline",
            self.model,
            self.permission_mode.as_str(),
            git_branch,
            workspace,
            cwd,
            self.session.id,
            session_path,
        )
    }

    fn repl_completion_candidates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(slash_command_completion_candidates_with_sessions(
            &self.model,
            Some(&self.session.id),
            list_managed_sessions()?
                .into_iter()
                .map(|session| session.id)
                .collect(),
        ))
    }

    fn prepare_turn_runtime(
        &self,
        emit_output: bool,
    ) -> Result<(BuiltRuntime, HookAbortMonitor), Box<dyn std::error::Error>> {
        let hook_abort_signal = runtime::HookAbortSignal::new();
        let runtime = build_runtime(
            self.runtime.session().clone(),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            emit_output,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?
        .with_hook_abort_signal(hook_abort_signal.clone());
        let hook_abort_monitor = HookAbortMonitor::spawn(hook_abort_signal);

        Ok((runtime, hook_abort_monitor))
    }

    fn replace_runtime(&mut self, runtime: BuiltRuntime) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.shutdown_plugins()?;
        self.runtime = runtime;
        Ok(())
    }

    fn run_turn(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(true)?;
        let mut spinner = Spinner::new();
        let mut stdout = io::stdout();
        spinner.tick(
            "🦀 Thinking...",
            TerminalRenderer::new().color_theme(),
            &mut stdout,
        )?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();
        match result {
            Ok(summary) => {
                self.replace_runtime(runtime)?;
                spinner.finish(
                    "✨ Done",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                println!();
                if let Some(event) = summary.auto_compaction {
                    println!(
                        "{}",
                        format_auto_compaction_notice(event.removed_message_count)
                    );
                }
                self.persist_session()?;
                Ok(())
            }
            Err(error) => {
                runtime.shutdown_plugins()?;
                spinner.fail(
                    "❌ Request failed",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                Err(Box::new(error))
            }
        }
    }

    fn run_turn_with_output(
        &mut self,
        input: &str,
        output_format: CliOutputFormat,
        compact: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match output_format {
            CliOutputFormat::Json if compact => self.run_prompt_compact_json(input),
            CliOutputFormat::Text if compact => self.run_prompt_compact(input),
            CliOutputFormat::Text => self.run_turn(input),
            CliOutputFormat::Json => self.run_prompt_json(input),
        }
    }

    fn run_prompt_compact(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(false)?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();
        let summary = result?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        let final_text = final_assistant_text(&summary);
        println!("{final_text}");
        Ok(())
    }

    fn run_prompt_compact_json(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(false)?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();
        let summary = result?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!(
            "{}",
            json!({
                "message": final_assistant_text(&summary),
                "compact": true,
                "model": self.model,
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                },
            })
        );
        Ok(())
    }

    fn run_prompt_json(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(false)?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();
        let summary = result?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!(
            "{}",
            json!({
                "message": final_assistant_text(&summary),
                "model": self.model,
                "iterations": summary.iterations,
                "auto_compaction": summary.auto_compaction.map(|event| json!({
                    "removed_messages": event.removed_message_count,
                    "kept_messages": event.kept_message_count,
                    "compaction_count": event.compaction_count,
                    "notice": format_auto_compaction_notice(event.removed_message_count),
                })),
                "tool_uses": collect_tool_uses(&summary),
                "tool_results": collect_tool_results(&summary),
                "prompt_cache_events": collect_prompt_cache_events(&summary),
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                },
                "estimated_cost": format_usd(
                    summary.usage.estimate_cost_usd_with_pricing(
                        pricing_for_model(&self.model)
                            .unwrap_or_else(runtime::ModelPricing::default_sonnet_tier)
                    ).total_cost_usd()
                )
            })
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn handle_repl_command(
        &mut self,
        command: SlashCommand,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(match command {
            SlashCommand::Help => {
                println!("{}", render_repl_help());
                false
            }
            SlashCommand::Status => {
                self.print_status();
                false
            }
            SlashCommand::Bughunter { scope } => {
                self.run_bughunter(scope.as_deref())?;
                false
            }
            SlashCommand::Commit => {
                self.run_commit(None)?;
                false
            }
            SlashCommand::Pr { context } => {
                self.run_pr(context.as_deref())?;
                false
            }
            SlashCommand::Issue { context } => {
                self.run_issue(context.as_deref())?;
                false
            }
            SlashCommand::Ultraplan { task } => {
                self.run_ultraplan(task.as_deref())?;
                false
            }
            SlashCommand::Teleport { target } => {
                Self::run_teleport(target.as_deref())?;
                false
            }
            SlashCommand::DebugToolCall => {
                self.run_debug_tool_call(None)?;
                false
            }
            SlashCommand::Sandbox => {
                Self::print_sandbox_status();
                false
            }
            SlashCommand::Compact => {
                self.compact()?;
                false
            }
            SlashCommand::Model { model } => self.set_model(model)?,
            SlashCommand::Permissions { mode } => self.set_permissions(mode)?,
            SlashCommand::Clear { confirm } => self.clear_session(confirm)?,
            SlashCommand::Cost => {
                self.print_cost();
                false
            }
            SlashCommand::Resume { session_path } => self.resume_session(session_path)?,
            SlashCommand::Config { section } => {
                Self::print_config(section.as_deref())?;
                false
            }
            SlashCommand::Mcp { action, target } => {
                let args = match (action.as_deref(), target.as_deref()) {
                    (None, None) => None,
                    (Some(action), None) => Some(action.to_string()),
                    (Some(action), Some(target)) => Some(format!("{action} {target}")),
                    (None, Some(target)) => Some(target.to_string()),
                };
                Self::print_mcp(args.as_deref(), CliOutputFormat::Text)?;
                false
            }
            SlashCommand::Memory => {
                Self::print_memory()?;
                false
            }
            SlashCommand::Init => {
                run_init(CliOutputFormat::Text)?;
                false
            }
            SlashCommand::Diff => {
                Self::print_diff()?;
                false
            }
            SlashCommand::Version => {
                Self::print_version(CliOutputFormat::Text);
                false
            }
            SlashCommand::Export { path } => {
                self.export_session(path.as_deref())?;
                false
            }
            SlashCommand::Session { action, target } => {
                self.handle_session_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Plugins { action, target } => {
                self.handle_plugins_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Agents { args } => {
                Self::print_agents(args.as_deref(), CliOutputFormat::Text)?;
                false
            }
            SlashCommand::Skills { args } => {
                match classify_skills_slash_command(args.as_deref()) {
                    SkillSlashDispatch::Invoke(prompt) => self.run_turn(&prompt)?,
                    SkillSlashDispatch::Local => {
                        Self::print_skills(args.as_deref(), CliOutputFormat::Text)?;
                    }
                }
                false
            }
            SlashCommand::Doctor => {
                println!("{}", render_doctor_report()?.render());
                false
            }
            SlashCommand::History { count } => {
                self.print_prompt_history(count.as_deref());
                false
            }
            SlashCommand::Stats => {
                let usage = UsageTracker::from_session(self.runtime.session()).cumulative_usage();
                println!("{}", format_cost_report(usage));
                false
            }
            SlashCommand::Login
            | SlashCommand::Logout
            | SlashCommand::Vim
            | SlashCommand::Upgrade
            | SlashCommand::Share
            | SlashCommand::Feedback
            | SlashCommand::Files
            | SlashCommand::Fast
            | SlashCommand::Exit
            | SlashCommand::Summary
            | SlashCommand::Desktop
            | SlashCommand::Brief
            | SlashCommand::Advisor
            | SlashCommand::Stickers
            | SlashCommand::Insights
            | SlashCommand::Thinkback
            | SlashCommand::ReleaseNotes
            | SlashCommand::SecurityReview
            | SlashCommand::Keybindings
            | SlashCommand::PrivacySettings
            | SlashCommand::Plan { .. }
            | SlashCommand::Review { .. }
            | SlashCommand::Tasks { .. }
            | SlashCommand::Theme { .. }
            | SlashCommand::Voice { .. }
            | SlashCommand::Usage { .. }
            | SlashCommand::Rename { .. }
            | SlashCommand::Copy { .. }
            | SlashCommand::Hooks { .. }
            | SlashCommand::Context { .. }
            | SlashCommand::Color { .. }
            | SlashCommand::Effort { .. }
            | SlashCommand::Branch { .. }
            | SlashCommand::Rewind { .. }
            | SlashCommand::Ide { .. }
            | SlashCommand::Tag { .. }
            | SlashCommand::OutputStyle { .. }
            | SlashCommand::AddDir { .. } => {
                let cmd_name = command.slash_name();
                eprintln!("{cmd_name} is not yet implemented in this build.");
                false
            }
            SlashCommand::Unknown(name) => {
                eprintln!("{}", format_unknown_slash_command(&name));
                false
            }
        })
    }

    fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session.path)?;
        Ok(())
    }

    fn print_status(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        let latest = self.runtime.usage().current_turn_usage();
        println!(
            "{}",
            format_status_report(
                &self.model,
                StatusUsage {
                    message_count: self.runtime.session().messages.len(),
                    turns: self.runtime.usage().turns(),
                    latest,
                    cumulative,
                    estimated_tokens: self.runtime.estimated_tokens(),
                },
                self.permission_mode.as_str(),
                &status_context(Some(&self.session.path)).expect("status context should load"),
                None, // #148: REPL /status doesn't carry flag provenance
            )
        );
    }

    fn record_prompt_history(&mut self, prompt: &str) {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map_or(self.runtime.session().updated_at_ms, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            });
        let entry = PromptHistoryEntry {
            timestamp_ms,
            text: prompt.to_string(),
        };
        self.prompt_history.push(entry);
        if let Err(error) = self.runtime.session_mut().push_prompt_entry(prompt) {
            eprintln!("warning: failed to persist prompt history: {error}");
        }
    }

    fn print_prompt_history(&self, count: Option<&str>) {
        let limit = match parse_history_count(count) {
            Ok(limit) => limit,
            Err(message) => {
                eprintln!("{message}");
                return;
            }
        };
        let session_entries = &self.runtime.session().prompt_history;
        let entries = if session_entries.is_empty() {
            if self.prompt_history.is_empty() {
                collect_session_prompt_history(self.runtime.session())
            } else {
                self.prompt_history
                    .iter()
                    .map(|entry| PromptHistoryEntry {
                        timestamp_ms: entry.timestamp_ms,
                        text: entry.text.clone(),
                    })
                    .collect()
            }
        } else {
            session_entries
                .iter()
                .map(|entry| PromptHistoryEntry {
                    timestamp_ms: entry.timestamp_ms,
                    text: entry.text.clone(),
                })
                .collect()
        };
        println!("{}", render_prompt_history_report(&entries, limit));
    }

    fn print_sandbox_status() {
        let cwd = env::current_dir().expect("current dir");
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader
            .load()
            .unwrap_or_else(|_| runtime::RuntimeConfig::empty());
        println!(
            "{}",
            format_sandbox_report(&resolve_sandbox_status(runtime_config.sandbox(), &cwd))
        );
    }

    fn set_model(&mut self, model: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(model) = model else {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        };

        let model = resolve_model_alias_with_config(&model);

        if model == self.model {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        }

        let previous = self.model.clone();
        let session = self.runtime.session().clone();
        let message_count = session.messages.len();
        let runtime = build_runtime(
            session,
            &self.session.id,
            model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.model.clone_from(&model);
        println!(
            "{}",
            format_model_switch_report(&previous, &model, message_count)
        );
        Ok(true)
    }

    fn set_permissions(
        &mut self,
        mode: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(mode) = mode else {
            println!(
                "{}",
                format_permissions_report(self.permission_mode.as_str())
            );
            return Ok(false);
        };

        let normalized = normalize_permission_mode(&mode).ok_or_else(|| {
            format!(
                "unsupported permission mode '{mode}'. Use read-only, workspace-write, or danger-full-access."
            )
        })?;

        if normalized == self.permission_mode.as_str() {
            println!("{}", format_permissions_report(normalized));
            return Ok(false);
        }

        let previous = self.permission_mode.as_str().to_string();
        let session = self.runtime.session().clone();
        self.permission_mode = permission_mode_from_label(normalized);
        let runtime = build_runtime(
            session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        println!(
            "{}",
            format_permissions_switch_report(&previous, normalized)
        );
        Ok(true)
    }

    fn clear_session(&mut self, confirm: bool) -> Result<bool, Box<dyn std::error::Error>> {
        if !confirm {
            println!(
                "clear: confirmation required; run /clear --confirm to start a fresh session."
            );
            return Ok(false);
        }

        let previous_session = self.session.clone();
        let session_state = new_cli_session()?;
        self.session = create_managed_session_handle(&session_state.session_id)?;
        let runtime = build_runtime(
            session_state.with_persistence_path(self.session.path.clone()),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        println!(
            "Session cleared\n  Mode             fresh session\n  Previous session {}\n  Resume previous  /resume {}\n  Preserved model  {}\n  Permission mode  {}\n  New session      {}\n  Session file     {}",
            previous_session.id,
            previous_session.id,
            self.model,
            self.permission_mode.as_str(),
            self.session.id,
            self.session.path.display(),
        );
        Ok(true)
    }

    fn print_cost(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        println!("{}", format_cost_report(cumulative));
    }

    fn resume_session(
        &mut self,
        session_path: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(session_ref) = session_path else {
            println!("{}", render_resume_usage());
            return Ok(false);
        };

        let (handle, session) = load_session_reference(&session_ref)?;
        let message_count = session.messages.len();
        let session_id = session.session_id.clone();
        let runtime = build_runtime(
            session,
            &handle.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.session = SessionHandle {
            id: session_id,
            path: handle.path,
        };
        println!(
            "{}",
            format_resume_report(
                &self.session.path.display().to_string(),
                message_count,
                self.runtime.usage().turns(),
            )
        );
        Ok(true)
    }

    fn print_config(section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_config_report(section)?);
        Ok(())
    }

    fn print_memory() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_memory_report()?);
        Ok(())
    }

    fn print_agents(
        args: Option<&str>,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        match output_format {
            CliOutputFormat::Text => println!("{}", handle_agents_slash_command(args, &cwd)?),
            CliOutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&handle_agents_slash_command_json(args, &cwd)?)?
            ),
        }
        Ok(())
    }

    fn print_mcp(
        args: Option<&str>,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // `claw mcp serve` starts a stdio MCP server exposing claw's built-in
        // tools. All other `mcp` subcommands fall through to the existing
        // configured-server reporter (`list`, `status`, ...).
        if matches!(args.map(str::trim), Some("serve")) {
            return run_mcp_serve();
        }
        let cwd = env::current_dir()?;
        if let Some(inventory) = runtime_mcp_inventory_json(&cwd, args)? {
            match output_format {
                CliOutputFormat::Text => {
                    println!("{}", render_runtime_mcp_inventory_text(&inventory))
                }
                CliOutputFormat::Json => println!("{}", serde_json::to_string_pretty(&inventory)?),
            }
            return Ok(());
        }
        match output_format {
            CliOutputFormat::Text => println!("{}", handle_mcp_slash_command(args, &cwd)?),
            CliOutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&handle_mcp_slash_command_json(args, &cwd)?)?
            ),
        }
        Ok(())
    }

    fn print_skills(
        args: Option<&str>,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        match output_format {
            CliOutputFormat::Text => println!("{}", handle_skills_slash_command(args, &cwd)?),
            CliOutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&handle_skills_slash_command_json(args, &cwd)?)?
            ),
        }
        Ok(())
    }

    fn print_plugins(
        action: Option<&str>,
        target: Option<&str>,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
        let result = handle_plugins_slash_command(action, target, &mut manager)?;
        match output_format {
            CliOutputFormat::Text => println!("{}", result.message),
            CliOutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "kind": "plugin",
                    "action": action.unwrap_or("list"),
                    "target": target,
                    "message": result.message,
                    "reload_runtime": result.reload_runtime,
                }))?
            ),
        }
        Ok(())
    }

    fn print_diff() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_diff_report()?);
        Ok(())
    }

    fn print_version(output_format: CliOutputFormat) {
        let _ = crate::print_version(output_format);
    }

    fn export_session(
        &self,
        requested_path: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let export_path = resolve_export_path(requested_path, self.runtime.session())?;
        fs::write(&export_path, render_export_text(self.runtime.session()))?;
        println!(
            "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
            export_path.display(),
            self.runtime.session().messages.len(),
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn handle_session_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match action {
            None | Some("list") => {
                println!("{}", render_session_list(&self.session.id)?);
                Ok(false)
            }
            Some("switch") => {
                let Some(target) = target else {
                    println!("Usage: /session switch <session-id>");
                    return Ok(false);
                };
                let (handle, session) = load_session_reference(target)?;
                let message_count = session.messages.len();
                let session_id = session.session_id.clone();
                let runtime = build_runtime(
                    session,
                    &handle.id,
                    self.model.clone(),
                    self.system_prompt.clone(),
                    true,
                    true,
                    self.allowed_tools.clone(),
                    self.permission_mode,
                    None,
                )?;
                self.replace_runtime(runtime)?;
                self.session = SessionHandle {
                    id: session_id,
                    path: handle.path,
                };
                println!(
                    "Session switched\n  Active session   {}\n  File             {}\n  Messages         {}",
                    self.session.id,
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some("fork") => {
                let forked = self.runtime.fork_session(target.map(ToOwned::to_owned));
                let parent_session_id = self.session.id.clone();
                let handle = create_managed_session_handle(&forked.session_id)?;
                let branch_name = forked
                    .fork
                    .as_ref()
                    .and_then(|fork| fork.branch_name.clone());
                let forked = forked.with_persistence_path(handle.path.clone());
                let message_count = forked.messages.len();
                forked.save_to_path(&handle.path)?;
                let runtime = build_runtime(
                    forked,
                    &handle.id,
                    self.model.clone(),
                    self.system_prompt.clone(),
                    true,
                    true,
                    self.allowed_tools.clone(),
                    self.permission_mode,
                    None,
                )?;
                self.replace_runtime(runtime)?;
                self.session = handle;
                println!(
                    "Session forked\n  Parent session   {}\n  Active session   {}\n  Branch           {}\n  File             {}\n  Messages         {}",
                    parent_session_id,
                    self.session.id,
                    branch_name.as_deref().unwrap_or("(unnamed)"),
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some("delete") => {
                let Some(target) = target else {
                    println!("Usage: /session delete <session-id> [--force]");
                    return Ok(false);
                };
                let handle = resolve_session_reference(target)?;
                if handle.id == self.session.id {
                    println!(
                        "delete: refusing to delete the active session '{}'.\nSwitch to another session first with /session switch <session-id>.",
                        handle.id
                    );
                    return Ok(false);
                }
                if !confirm_session_deletion(&handle.id) {
                    println!("delete: cancelled.");
                    return Ok(false);
                }
                delete_managed_session(&handle.path)?;
                println!(
                    "Session deleted\n  Deleted session  {}\n  File             {}",
                    handle.id,
                    handle.path.display(),
                );
                Ok(false)
            }
            Some("delete-force") => {
                let Some(target) = target else {
                    println!("Usage: /session delete <session-id> [--force]");
                    return Ok(false);
                };
                let handle = resolve_session_reference(target)?;
                if handle.id == self.session.id {
                    println!(
                        "delete: refusing to delete the active session '{}'.\nSwitch to another session first with /session switch <session-id>.",
                        handle.id
                    );
                    return Ok(false);
                }
                delete_managed_session(&handle.path)?;
                println!(
                    "Session deleted\n  Deleted session  {}\n  File             {}",
                    handle.id,
                    handle.path.display(),
                );
                Ok(false)
            }
            Some(other) => {
                println!(
                    "Unknown /session action '{other}'. Use /session list, /session switch <session-id>, /session fork [branch-name], or /session delete <session-id> [--force]."
                );
                Ok(false)
            }
        }
    }

    fn handle_plugins_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
        let result = handle_plugins_slash_command(action, target, &mut manager)?;
        println!("{}", result.message);
        if result.reload_runtime {
            self.reload_runtime_features()?;
        }
        Ok(false)
    }

    fn reload_runtime_features(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let runtime = build_runtime(
            self.runtime.session().clone(),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.persist_session()
    }

    fn compact(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.runtime.compact(CompactionConfig::default());
        let removed = result.removed_message_count;
        let kept = result.compacted_session.messages.len();
        let skipped = removed == 0;
        let runtime = build_runtime(
            result.compacted_session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!("{}", format_compact_report(removed, kept, skipped));
        Ok(())
    }

    fn run_internal_prompt_text_with_progress(
        &self,
        prompt: &str,
        enable_tools: bool,
        progress: Option<InternalPromptProgressReporter>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = build_runtime(
            session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            enable_tools,
            false,
            self.allowed_tools.clone(),
            self.permission_mode,
            progress,
        )?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(prompt, Some(&mut permission_prompter))?;
        let text = final_assistant_text(&summary).trim().to_string();
        runtime.shutdown_plugins()?;
        Ok(text)
    }

    fn run_internal_prompt_text(
        &self,
        prompt: &str,
        enable_tools: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.run_internal_prompt_text_with_progress(prompt, enable_tools, None)
    }

    fn run_bughunter(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_bughunter_report(scope));
        Ok(())
    }

    fn run_ultraplan(&self, task: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_ultraplan_report(task));
        Ok(())
    }

    fn run_teleport(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
            println!("Usage: /teleport <symbol-or-path>");
            return Ok(());
        };

        println!("{}", render_teleport_report(target)?);
        Ok(())
    }

    fn run_debug_tool_call(&self, args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        validate_no_args("/debug-tool-call", args)?;
        println!("{}", render_last_tool_debug_report(self.runtime.session())?);
        Ok(())
    }

    fn run_commit(&mut self, args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        validate_no_args("/commit", args)?;
        let status = git_output(&["status", "--short", "--branch"])?;
        let summary = parse_git_workspace_summary(Some(&status));
        let branch = parse_git_status_branch(Some(&status));
        if summary.is_clean() {
            println!("{}", format_commit_skipped_report());
            return Ok(());
        }

        println!(
            "{}",
            format_commit_preflight_report(branch.as_deref(), summary)
        );
        Ok(())
    }

    fn run_pr(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let branch =
            resolve_git_branch_for(&env::current_dir()?).unwrap_or_else(|| "unknown".to_string());
        println!("{}", format_pr_report(&branch, context));
        Ok(())
    }

    fn run_issue(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_issue_report(context));
        Ok(())
    }
}

fn sessions_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(current_session_store()?.sessions_dir().to_path_buf())
}

fn current_session_store() -> Result<runtime::SessionStore, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    runtime::SessionStore::from_cwd(&cwd).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

fn new_cli_session() -> Result<Session, Box<dyn std::error::Error>> {
    Ok(Session::new().with_workspace_root(env::current_dir()?))
}

fn create_managed_session_handle(
    session_id: &str,
) -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let handle = current_session_store()?.create_handle(session_id);
    Ok(SessionHandle {
        id: handle.id,
        path: handle.path,
    })
}

fn resolve_session_reference(reference: &str) -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let handle = current_session_store()?
        .resolve_reference(reference)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    Ok(SessionHandle {
        id: handle.id,
        path: handle.path,
    })
}

fn resolve_managed_session_path(session_id: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    current_session_store()?
        .resolve_managed_path(session_id)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

fn list_managed_sessions() -> Result<Vec<ManagedSessionSummary>, Box<dyn std::error::Error>> {
    Ok(current_session_store()?
        .list_sessions()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
        .into_iter()
        .map(|session| ManagedSessionSummary {
            id: session.id,
            path: session.path,
            updated_at_ms: session.updated_at_ms,
            modified_epoch_millis: session.modified_epoch_millis,
            message_count: session.message_count,
            parent_session_id: session.parent_session_id,
            branch_name: session.branch_name,
        })
        .collect())
}

fn latest_managed_session() -> Result<ManagedSessionSummary, Box<dyn std::error::Error>> {
    let session = current_session_store()?
        .latest_session()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    Ok(ManagedSessionSummary {
        id: session.id,
        path: session.path,
        updated_at_ms: session.updated_at_ms,
        modified_epoch_millis: session.modified_epoch_millis,
        message_count: session.message_count,
        parent_session_id: session.parent_session_id,
        branch_name: session.branch_name,
    })
}

fn load_session_reference(
    reference: &str,
) -> Result<(SessionHandle, Session), Box<dyn std::error::Error>> {
    let loaded = current_session_store()?
        .load_session(reference)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    Ok((
        SessionHandle {
            id: loaded.handle.id,
            path: loaded.handle.path,
        },
        loaded.session,
    ))
}

fn delete_managed_session(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Err(format!("session file does not exist: {}", path.display()).into());
    }
    fs::remove_file(path)?;
    Ok(())
}

fn confirm_session_deletion(session_id: &str) -> bool {
    print!("Delete session '{session_id}'? This cannot be undone. [y/N]: ");
    io::stdout().flush().unwrap_or(());
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES")
}

fn render_session_list(active_session_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let sessions = list_managed_sessions()?;
    let mut lines = vec![
        "Sessions".to_string(),
        format!("  Directory         {}", sessions_dir()?.display()),
    ];
    if sessions.is_empty() {
        lines.push("  No managed sessions saved yet.".to_string());
        return Ok(lines.join("\n"));
    }
    for session in sessions {
        let marker = if session.id == active_session_id {
            "● current"
        } else {
            "○ saved"
        };
        let lineage = match (
            session.branch_name.as_deref(),
            session.parent_session_id.as_deref(),
        ) {
            (Some(branch_name), Some(parent_session_id)) => {
                format!(" branch={branch_name} from={parent_session_id}")
            }
            (None, Some(parent_session_id)) => format!(" from={parent_session_id}"),
            (Some(branch_name), None) => format!(" branch={branch_name}"),
            (None, None) => String::new(),
        };
        lines.push(format!(
            "  {id:<20} {marker:<10} msgs={msgs:<4} modified={modified}{lineage} path={path}",
            id = session.id,
            msgs = session.message_count,
            modified = format_session_modified_age(session.modified_epoch_millis),
            lineage = lineage,
            path = session.path.display(),
        ));
    }
    Ok(lines.join("\n"))
}

fn format_session_modified_age(modified_epoch_millis: u128) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(modified_epoch_millis, |duration| duration.as_millis());
    let delta_seconds = now
        .saturating_sub(modified_epoch_millis)
        .checked_div(1_000)
        .unwrap_or_default();
    match delta_seconds {
        0..=4 => "just-now".to_string(),
        5..=59 => format!("{delta_seconds}s-ago"),
        60..=3_599 => format!("{}m-ago", delta_seconds / 60),
        3_600..=86_399 => format!("{}h-ago", delta_seconds / 3_600),
        _ => format!("{}d-ago", delta_seconds / 86_400),
    }
}

fn write_session_clear_backup(
    session: &Session,
    session_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let backup_path = session_clear_backup_path(session_path);
    session.save_to_path(&backup_path)?;
    Ok(backup_path)
}

fn session_clear_backup_path(session_path: &Path) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(0, |duration| duration.as_millis());
    let file_name = session_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session.jsonl");
    session_path.with_file_name(format!("{file_name}.before-clear-{timestamp}.bak"))
}

fn render_repl_help() -> String {
    [
        "REPL".to_string(),
        "  /exit                Quit the REPL".to_string(),
        "  /quit                Quit the REPL".to_string(),
        "  Up/Down              Navigate prompt history".to_string(),
        "  Ctrl-R               Reverse-search prompt history".to_string(),
        "  Tab                  Complete commands, modes, and recent sessions".to_string(),
        "  Ctrl-C               Clear input (or exit on empty prompt)".to_string(),
        "  Shift+Enter/Ctrl+J   Insert a newline".to_string(),
        "  Auto-save            .claw/sessions/<session-id>.jsonl".to_string(),
        "  Resume latest        /resume latest".to_string(),
        "  Browse sessions      /session list".to_string(),
        "  Show prompt history  /history [count]".to_string(),
        String::new(),
        render_slash_command_help_filtered(STUB_COMMANDS),
    ]
    .join(
        "
",
    )
}

fn print_status_snapshot(
    model: &str,
    model_flag_raw: Option<&str>,
    permission_mode: PermissionMode,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let usage = StatusUsage {
        message_count: 0,
        turns: 0,
        latest: TokenUsage::default(),
        cumulative: TokenUsage::default(),
        estimated_tokens: 0,
    };
    let context = status_context(None)?;
    // #148: resolve model provenance. If user passed --model, source is
    // "flag" with the raw input preserved. Otherwise probe env -> config
    // -> default and record the winning source.
    let provenance = match model_flag_raw {
        Some(raw) => ModelProvenance {
            resolved: model.to_string(),
            raw: Some(raw.to_string()),
            source: ModelSource::Flag,
        },
        None => ModelProvenance::from_env_or_config_or_default(model),
    };
    match output_format {
        CliOutputFormat::Text => println!(
            "{}",
            format_status_report(
                &provenance.resolved,
                usage,
                permission_mode.as_str(),
                &context,
                Some(&provenance)
            )
        ),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&status_json_value(
                Some(&provenance.resolved),
                usage,
                permission_mode.as_str(),
                &context,
                Some(&provenance),
            ))?
        ),
    }
    Ok(())
}

fn status_json_value(
    model: Option<&str>,
    usage: StatusUsage,
    permission_mode: &str,
    context: &StatusContext,
    // #148: optional provenance for `model` field. Surfaces `model_source`
    // ("flag" | "env" | "config" | "default") and `model_raw` (user input
    // before alias resolution, or null when source is "default"). Callers
    // that don't have provenance (legacy resume paths) pass None, in which
    // case both new fields are omitted.
    provenance: Option<&ModelProvenance>,
) -> serde_json::Value {
    // #143: top-level `status` marker so claws can distinguish
    // a clean run from a degraded run (config parse failed but other fields
    // are still populated). `config_load_error` carries the parse-error string
    // when present; it's a string rather than a typed object in Phase 1 and
    // will join the typed-error taxonomy in Phase 2 (ROADMAP §4.44).
    let degraded = context.config_load_error.is_some();
    let model_source = provenance.map(|p| p.source.as_str());
    let model_raw = provenance.and_then(|p| p.raw.clone());
    json!({
        "kind": "status",
        "status": if degraded { "degraded" } else { "ok" },
        "config_load_error": context.config_load_error,
        "model": model,
        "model_source": model_source,
        "model_raw": model_raw,
        "permission_mode": permission_mode,
        "usage": {
            "messages": usage.message_count,
            "turns": usage.turns,
            "latest_total": usage.latest.total_tokens(),
            "cumulative_input": usage.cumulative.input_tokens,
            "cumulative_output": usage.cumulative.output_tokens,
            "cumulative_total": usage.cumulative.total_tokens(),
            "estimated_tokens": usage.estimated_tokens,
        },
        "workspace": {
            "cwd": context.cwd,
            "project_root": context.project_root,
            "git_branch": context.git_branch,
            "git_state": context.git_summary.headline(),
            "changed_files": context.git_summary.changed_files,
            "staged_files": context.git_summary.staged_files,
            "unstaged_files": context.git_summary.unstaged_files,
            "untracked_files": context.git_summary.untracked_files,
            "session": context.session_path.as_ref().map_or_else(|| "live-repl".to_string(), |path| path.display().to_string()),
            "session_id": context.session_path.as_ref().and_then(|path| {
                // Session files are named <session-id>.jsonl directly under
                // .claw/sessions/. Extract the stem (drop the .jsonl extension).
                path.file_stem().map(|n| n.to_string_lossy().into_owned())
            }),
            "loaded_config_files": context.loaded_config_files,
            "discovered_config_files": context.discovered_config_files,
            "memory_file_count": context.memory_file_count,
        },
        "sandbox": {
            "enabled": context.sandbox_status.enabled,
            "active": context.sandbox_status.active,
            "supported": context.sandbox_status.supported,
            "in_container": context.sandbox_status.in_container,
            "requested_namespace": context.sandbox_status.requested.namespace_restrictions,
            "active_namespace": context.sandbox_status.namespace_active,
            "requested_network": context.sandbox_status.requested.network_isolation,
            "active_network": context.sandbox_status.network_active,
            "filesystem_mode": context.sandbox_status.filesystem_mode.as_str(),
            "filesystem_active": context.sandbox_status.filesystem_active,
            "allowed_mounts": context.sandbox_status.allowed_mounts,
            "markers": context.sandbox_status.container_markers,
            "fallback_reason": context.sandbox_status.fallback_reason,
        }
    })
}

fn status_context(
    session_path: Option<&Path>,
) -> Result<StatusContext, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered_config_files = loader.discover().len();
    // #143: degrade gracefully on config parse failure rather than hard-fail.
    // `claw doctor` already does this; `claw status` now matches that contract
    // so that one malformed `mcpServers.*` entry doesn't take down the whole
    // health surface (workspace, git, model, permission, sandbox can still be
    // reported independently).
    let (loaded_config_files, sandbox_status, config_load_error) = match loader.load() {
        Ok(runtime_config) => (
            runtime_config.loaded_entries().len(),
            resolve_sandbox_status(runtime_config.sandbox(), &cwd),
            None,
        ),
        Err(err) => (
            0,
            // Fall back to defaults for sandbox resolution so claws still see
            // a populated sandbox section instead of a missing field. Defaults
            // produce the same output as a runtime config with no sandbox
            // overrides, which is the right degraded-mode shape: we cannot
            // report what the user *intended*, only what is actually in effect.
            resolve_sandbox_status(&runtime::SandboxConfig::default(), &cwd),
            Some(err.to_string()),
        ),
    };
    let project_context = ProjectContext::discover_with_git(&cwd, DEFAULT_DATE)?;
    let (project_root, git_branch) =
        parse_git_status_metadata(project_context.git_status.as_deref());
    let git_summary = parse_git_workspace_summary(project_context.git_status.as_deref());
    Ok(StatusContext {
        cwd,
        session_path: session_path.map(Path::to_path_buf),
        loaded_config_files,
        discovered_config_files,
        memory_file_count: project_context.instruction_files.len(),
        project_root,
        git_branch,
        git_summary,
        sandbox_status,
        config_load_error,
    })
}

fn format_status_report(
    model: &str,
    usage: StatusUsage,
    permission_mode: &str,
    context: &StatusContext,
    // #148: optional model provenance to surface in a `Model source` line.
    // Callers without provenance (legacy resume paths) pass None and the
    // source line is omitted for backward compat.
    provenance: Option<&ModelProvenance>,
) -> String {
    // #143: if config failed to parse, surface a degraded banner at the top
    // of the text report so humans see the parse error before the body, while
    // the body below still reports everything that could be resolved without
    // config (workspace, git, sandbox defaults, etc.).
    let status_line = if context.config_load_error.is_some() {
        "Status (degraded)"
    } else {
        "Status"
    };
    let mut blocks: Vec<String> = Vec::new();
    if let Some(err) = context.config_load_error.as_deref() {
        blocks.push(format!(
            "Config load error\n  Status           fail\n  Summary          runtime config failed to load; reporting partial status\n  Details          {err}\n  Hint             `claw doctor` classifies config parse errors; fix the listed field and rerun"
        ));
    }
    // #148: render Model source line after Model, showing where the string
    // came from (flag / env / config / default) and the raw input if any.
    let model_source_line = provenance
        .map(|p| match &p.raw {
            Some(raw) if raw != model => {
                format!("\n  Model source     {} (raw: {raw})", p.source.as_str())
            }
            Some(_) => format!("\n  Model source     {}", p.source.as_str()),
            None => format!("\n  Model source     {}", p.source.as_str()),
        })
        .unwrap_or_default();
    blocks.extend([
        format!(
            "{status_line}
  Model            {model}{model_source_line}
  Permission mode  {permission_mode}
  Messages         {}
  Turns            {}
  Estimated tokens {}",
            usage.message_count, usage.turns, usage.estimated_tokens,
        ),
        format!(
            "Usage
  Latest total     {}
  Cumulative input {}
  Cumulative output {}
  Cumulative total {}",
            usage.latest.total_tokens(),
            usage.cumulative.input_tokens,
            usage.cumulative.output_tokens,
            usage.cumulative.total_tokens(),
        ),
        format!(
            "Workspace
  Cwd              {}
  Project root     {}
  Git branch       {}
  Git state        {}
  Changed files    {}
  Staged           {}
  Unstaged         {}
  Untracked        {}
  Session          {}
  Config files     loaded {}/{}
  Memory files     {}
  Suggested flow   /status → /diff → /commit",
            context.cwd.display(),
            context
                .project_root
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |path| path.display().to_string()),
            context.git_branch.as_deref().unwrap_or("unknown"),
            context.git_summary.headline(),
            context.git_summary.changed_files,
            context.git_summary.staged_files,
            context.git_summary.unstaged_files,
            context.git_summary.untracked_files,
            context.session_path.as_ref().map_or_else(
                || "live-repl".to_string(),
                |path| path.display().to_string()
            ),
            context.loaded_config_files,
            context.discovered_config_files,
            context.memory_file_count,
        ),
        format_sandbox_report(&context.sandbox_status),
    ]);
    blocks.join("\n\n")
}

fn format_sandbox_report(status: &runtime::SandboxStatus) -> String {
    format!(
        "Sandbox
  Enabled           {}
  Active            {}
  Supported         {}
  In container      {}
  Requested ns      {}
  Active ns         {}
  Requested net     {}
  Active net        {}
  Filesystem mode   {}
  Filesystem active {}
  Allowed mounts    {}
  Markers           {}
  Fallback reason   {}",
        status.enabled,
        status.active,
        status.supported,
        status.in_container,
        status.requested.namespace_restrictions,
        status.namespace_active,
        status.requested.network_isolation,
        status.network_active,
        status.filesystem_mode.as_str(),
        status.filesystem_active,
        if status.allowed_mounts.is_empty() {
            "<none>".to_string()
        } else {
            status.allowed_mounts.join(", ")
        },
        if status.container_markers.is_empty() {
            "<none>".to_string()
        } else {
            status.container_markers.join(", ")
        },
        status
            .fallback_reason
            .clone()
            .unwrap_or_else(|| "<none>".to_string()),
    )
}

fn format_commit_preflight_report(branch: Option<&str>, summary: GitWorkspaceSummary) -> String {
    format!(
        "Commit
  Result           ready
  Branch           {}
  Workspace        {}
  Changed files    {}
  Action           create a git commit from the current workspace changes",
        branch.unwrap_or("unknown"),
        summary.headline(),
        summary.changed_files,
    )
}

fn format_commit_skipped_report() -> String {
    "Commit
  Result           skipped
  Reason           no workspace changes
  Action           create a git commit from the current workspace changes
  Next             /status to inspect context · /diff to inspect repo changes"
        .to_string()
}

fn print_sandbox_status_snapshot(
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader
        .load()
        .unwrap_or_else(|_| runtime::RuntimeConfig::empty());
    let status = resolve_sandbox_status(runtime_config.sandbox(), &cwd);
    match output_format {
        CliOutputFormat::Text => println!("{}", format_sandbox_report(&status)),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&sandbox_json_value(&status))?
        ),
    }
    Ok(())
}

fn sandbox_json_value(status: &runtime::SandboxStatus) -> serde_json::Value {
    json!({
        "kind": "sandbox",
        "enabled": status.enabled,
        "active": status.active,
        "supported": status.supported,
        "in_container": status.in_container,
        "requested_namespace": status.requested.namespace_restrictions,
        "active_namespace": status.namespace_active,
        "requested_network": status.requested.network_isolation,
        "active_network": status.network_active,
        "filesystem_mode": status.filesystem_mode.as_str(),
        "filesystem_active": status.filesystem_active,
        "allowed_mounts": status.allowed_mounts,
        "markers": status.container_markers,
        "fallback_reason": status.fallback_reason,
    })
}

fn render_help_topic(topic: LocalHelpTopic) -> String {
    match topic {
        LocalHelpTopic::Status => "Status
  Usage            claw status [--output-format <format>]
  Purpose          show the local workspace snapshot without entering the REPL
  Output           model, permissions, git state, config files, and sandbox status
  Formats          text (default), json
  Related          /status · claw --resume latest /status"
            .to_string(),
        LocalHelpTopic::Sandbox => "Sandbox
  Usage            claw sandbox [--output-format <format>]
  Purpose          inspect the resolved sandbox and isolation state for the current directory
  Output           namespace, network, filesystem, and fallback details
  Formats          text (default), json
  Related          /sandbox · claw status"
            .to_string(),
        LocalHelpTopic::Doctor => "Doctor
  Usage            claw doctor [--output-format <format>]
  Purpose          diagnose local auth, config, workspace, sandbox, and build metadata
  Output           local-only health report; no provider request or session resume required
  Formats          text (default), json
  Related          /doctor · claw --resume latest /doctor"
            .to_string(),
        LocalHelpTopic::Acp => "ACP / Zed
  Usage            claw acp [serve] [--output-format <format>]
  Aliases          claw --acp · claw -acp
  Purpose          explain the current editor-facing ACP/Zed launch contract without starting the runtime
  Status           discoverability only; `serve` is a status alias and does not launch a daemon yet
  Formats          text (default), json
  Related          ROADMAP #64a (discoverability) · ROADMAP #76 (real ACP support) · claw --help"
            .to_string(),
        LocalHelpTopic::Init => "Init
  Usage            claw init [--output-format <format>]
  Purpose          create .claw/, .claw.json, .gitignore, and CLAUDE.md in the current project
  Output           list of created vs. skipped files (idempotent: safe to re-run)
  Formats          text (default), json
  Related          claw status · claw doctor"
            .to_string(),
        LocalHelpTopic::State => "State
  Usage            claw state [--output-format <format>]
  Purpose          read .claw/worker-state.json written by the interactive REPL or a one-shot prompt
  Output           worker id, model, permissions, session reference (text or json)
  Formats          text (default), json
  Produces state   `claw` (interactive REPL) or `claw prompt <text>` (one non-interactive turn)
  Observes state   `claw state` reads; clawhip/CI may poll this file without HTTP
  Exit codes       0 if state file exists and parses; 1 with actionable hint otherwise
  Related          claw status · ROADMAP #139 (this worker-concept contract)"
            .to_string(),
        LocalHelpTopic::Export => "Export
  Usage            claw export [--session <id|latest>] [--output <path>] [--output-format <format>]
  Purpose          serialize a managed session to JSON for review, transfer, or archival
  Defaults         --session latest (most recent managed session in .claw/sessions/)
  Formats          text (default), json
  Related          /session list · claw --resume latest"
            .to_string(),
        LocalHelpTopic::Version => "Version
  Usage            claw version [--output-format <format>]
  Aliases          claw --version · claw -V
  Purpose          print the claw CLI version and build metadata
  Formats          text (default), json
  Related          claw doctor (full build/auth/config diagnostic)"
            .to_string(),
        LocalHelpTopic::SystemPrompt => "System Prompt
  Usage            claw system-prompt [--cwd <path>] [--date YYYY-MM-DD] [--output-format <format>]
  Purpose          render the resolved system prompt that `claw` would send for the given cwd + date
  Options          --cwd overrides the workspace dir · --date injects a deterministic date stamp
  Formats          text (default), json
  Related          claw doctor · claw dump-manifests"
            .to_string(),
        LocalHelpTopic::DumpManifests => "Dump Manifests
  Usage            claw dump-manifests [--manifests-dir <path>] [--output-format <format>]
  Purpose          emit every skill/agent/tool manifest the resolver would load for the current cwd
  Options          --manifests-dir scopes discovery to a specific directory
  Formats          text (default), json
  Related          claw skills · claw agents · claw doctor"
            .to_string(),
        LocalHelpTopic::BootstrapPlan => "Bootstrap Plan
  Usage            claw bootstrap-plan [--output-format <format>]
  Purpose          list the ordered startup phases the CLI would execute before dispatch
  Output           phase names (text) or structured phase list (json) — primary output is the plan itself
  Formats          text (default), json
  Related          claw doctor · claw status"
            .to_string(),
    }
}

fn print_help_topic(topic: LocalHelpTopic) {
    println!("{}", render_help_topic(topic));
}

fn print_acp_status(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let message = "ACP/Zed editor integration is not implemented in claw-code yet. `claw acp serve` is only a discoverability alias today; it does not launch a daemon or Zed-specific protocol endpoint. Use the normal terminal surfaces for now and track ROADMAP #76 for real ACP support.";
    match output_format {
        CliOutputFormat::Text => {
            println!(
                "ACP / Zed\n  Status           discoverability only\n  Launch           `claw acp serve` / `claw --acp` / `claw -acp` report status only; no editor daemon is available yet\n  Today            use `claw prompt`, the REPL, or `claw doctor` for local verification\n  Tracking         ROADMAP #76\n  Message          {message}"
            );
        }
        CliOutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "kind": "acp",
                    "status": "discoverability_only",
                    "supported": false,
                    "serve_alias_only": true,
                    "message": message,
                    "launch_command": serde_json::Value::Null,
                    "aliases": ["acp", "--acp", "-acp"],
                    "discoverability_tracking": "ROADMAP #64a",
                    "tracking": "ROADMAP #76",
                    "recommended_workflows": [
                        "claw prompt TEXT",
                        "claw",
                        "claw doctor"
                    ],
                }))?
            );
        }
    }
    Ok(())
}

fn render_config_report(section: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered = loader.discover();
    let runtime_config = loader.load()?;

    let mut lines = vec![
        format!(
            "Config
  Working directory {}
  Loaded files      {}
  Merged keys       {}",
            cwd.display(),
            runtime_config.loaded_entries().len(),
            runtime_config.merged().len()
        ),
        "Discovered files".to_string(),
    ];
    for entry in discovered {
        let source = match entry.source {
            ConfigSource::User => "user",
            ConfigSource::Project => "project",
            ConfigSource::Local => "local",
        };
        let status = if runtime_config
            .loaded_entries()
            .iter()
            .any(|loaded_entry| loaded_entry.path == entry.path)
        {
            "loaded"
        } else {
            "missing"
        };
        lines.push(format!(
            "  {source:<7} {status:<7} {}",
            entry.path.display()
        ));
    }

    if let Some(section) = section {
        lines.push(format!("Merged section: {section}"));
        let value = match section {
            "env" => runtime_config.get("env"),
            "hooks" => runtime_config.get("hooks"),
            "model" => runtime_config.get("model"),
            "plugins" => runtime_config
                .get("plugins")
                .or_else(|| runtime_config.get("enabledPlugins")),
            other => {
                lines.push(format!(
                    "  Unsupported config section '{other}'. Use env, hooks, model, or plugins."
                ));
                return Ok(lines.join(
                    "
",
                ));
            }
        };
        lines.push(format!(
            "  {}",
            match value {
                Some(value) => value.render(),
                None => "<unset>".to_string(),
            }
        ));
        return Ok(lines.join(
            "
",
        ));
    }

    lines.push("Merged JSON".to_string());
    lines.push(format!("  {}", runtime_config.as_json().render()));
    Ok(lines.join(
        "
",
    ))
}

fn render_config_json(
    _section: Option<&str>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered = loader.discover();
    let runtime_config = loader.load()?;

    let loaded_paths: Vec<_> = runtime_config
        .loaded_entries()
        .iter()
        .map(|e| e.path.display().to_string())
        .collect();

    let files: Vec<_> = discovered
        .iter()
        .map(|e| {
            let source = match e.source {
                ConfigSource::User => "user",
                ConfigSource::Project => "project",
                ConfigSource::Local => "local",
            };
            let is_loaded = runtime_config
                .loaded_entries()
                .iter()
                .any(|le| le.path == e.path);
            serde_json::json!({
                "path": e.path.display().to_string(),
                "source": source,
                "loaded": is_loaded,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "kind": "config",
        "cwd": cwd.display().to_string(),
        "loaded_files": loaded_paths.len(),
        "merged_keys": runtime_config.merged().len(),
        "files": files,
    }))
}

fn render_memory_report() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let project_context = ProjectContext::discover(&cwd, DEFAULT_DATE)?;
    let mut lines = vec![format!(
        "Memory
  Working directory {}
  Instruction files {}",
        cwd.display(),
        project_context.instruction_files.len()
    )];
    if project_context.instruction_files.is_empty() {
        lines.push("Discovered files".to_string());
        lines.push(
            "  No CLAUDE instruction files discovered in the current directory ancestry."
                .to_string(),
        );
    } else {
        lines.push("Discovered files".to_string());
        for (index, file) in project_context.instruction_files.iter().enumerate() {
            let preview = file.content.lines().next().unwrap_or("").trim();
            let preview = if preview.is_empty() {
                "<empty>"
            } else {
                preview
            };
            lines.push(format!("  {}. {}", index + 1, file.path.display(),));
            lines.push(format!(
                "     lines={} preview={}",
                file.content.lines().count(),
                preview
            ));
        }
    }
    Ok(lines.join(
        "
",
    ))
}

fn render_memory_json() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let project_context = ProjectContext::discover(&cwd, DEFAULT_DATE)?;
    let files: Vec<_> = project_context
        .instruction_files
        .iter()
        .map(|f| {
            json!({
                "path": f.path.display().to_string(),
                "lines": f.content.lines().count(),
                "preview": f.content.lines().next().unwrap_or("").trim(),
            })
        })
        .collect();
    Ok(json!({
        "kind": "memory",
        "cwd": cwd.display().to_string(),
        "instruction_files": files.len(),
        "files": files,
    }))
}

fn init_claude_md() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    Ok(initialize_repo(&cwd)?.render())
}

fn run_init(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let report = initialize_repo(&cwd)?;
    let message = report.render();
    match output_format {
        CliOutputFormat::Text => println!("{message}"),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&init_json_value(&report, &message))?
        ),
    }
    Ok(())
}

/// #142: emit first-class structured fields alongside the legacy `message`
/// string so claws can detect per-artifact state without substring matching.
fn init_json_value(report: &crate::init::InitReport, message: &str) -> serde_json::Value {
    use crate::init::InitStatus;
    json!({
        "kind": "init",
        "project_path": report.project_root.display().to_string(),
        "created": report.artifacts_with_status(InitStatus::Created),
        "updated": report.artifacts_with_status(InitStatus::Updated),
        "skipped": report.artifacts_with_status(InitStatus::Skipped),
        "artifacts": report.artifact_json_entries(),
        "next_step": crate::init::InitReport::NEXT_STEP,
        "message": message,
    })
}

fn normalize_permission_mode(mode: &str) -> Option<&'static str> {
    match mode.trim() {
        "read-only" => Some("read-only"),
        "workspace-write" => Some("workspace-write"),
        "danger-full-access" => Some("danger-full-access"),
        _ => None,
    }
}

fn render_diff_report() -> Result<String, Box<dyn std::error::Error>> {
    render_diff_report_for(&env::current_dir()?)
}

fn render_diff_report_for(cwd: &Path) -> Result<String, Box<dyn std::error::Error>> {
    // Verify we are inside a git repository before calling `git diff`.
    // Running `git diff --cached` outside a git tree produces a misleading
    // "unknown option `cached`" error because git falls back to --no-index mode.
    let in_git_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !in_git_repo {
        return Ok(format!(
            "Diff\n  Result           no git repository\n  Detail           {} is not inside a git project",
            cwd.display()
        ));
    }
    let staged = run_git_diff_command_in(cwd, &["diff", "--cached"])?;
    let unstaged = run_git_diff_command_in(cwd, &["diff"])?;
    if staged.trim().is_empty() && unstaged.trim().is_empty() {
        return Ok(
            "Diff\n  Result           clean working tree\n  Detail           no current changes"
                .to_string(),
        );
    }

    let mut sections = Vec::new();
    if !staged.trim().is_empty() {
        sections.push(format!("Staged changes:\n{}", staged.trim_end()));
    }
    if !unstaged.trim().is_empty() {
        sections.push(format!("Unstaged changes:\n{}", unstaged.trim_end()));
    }

    Ok(format!("Diff\n\n{}", sections.join("\n\n")))
}

fn render_diff_json_for(cwd: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let in_git_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !in_git_repo {
        return Ok(serde_json::json!({
            "kind": "diff",
            "result": "no_git_repo",
            "detail": format!("{} is not inside a git project", cwd.display()),
        }));
    }
    let staged = run_git_diff_command_in(cwd, &["diff", "--cached"])?;
    let unstaged = run_git_diff_command_in(cwd, &["diff"])?;
    Ok(serde_json::json!({
        "kind": "diff",
        "result": if staged.trim().is_empty() && unstaged.trim().is_empty() { "clean" } else { "changes" },
        "staged": staged.trim(),
        "unstaged": unstaged.trim(),
    }))
}

fn run_git_diff_command_in(
    cwd: &Path,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn render_teleport_report(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;

    let file_list = Command::new("rg")
        .args(["--files"])
        .current_dir(&cwd)
        .output()?;
    let file_matches = if file_list.status.success() {
        String::from_utf8(file_list.stdout)?
            .lines()
            .filter(|line| line.contains(target))
            .take(10)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let content_output = Command::new("rg")
        .args(["-n", "-S", "--color", "never", target, "."])
        .current_dir(&cwd)
        .output()?;

    let mut lines = vec![
        "Teleport".to_string(),
        format!("  Target           {target}"),
        "  Action           search workspace files and content for the target".to_string(),
    ];
    if !file_matches.is_empty() {
        lines.push(String::new());
        lines.push("File matches".to_string());
        lines.extend(file_matches.into_iter().map(|path| format!("  {path}")));
    }

    if content_output.status.success() {
        let matches = String::from_utf8(content_output.stdout)?;
        if !matches.trim().is_empty() {
            lines.push(String::new());
            lines.push("Content matches".to_string());
            lines.push(truncate_for_prompt(&matches, 4_000));
        }
    }

    if lines.len() == 1 {
        lines.push("  Result           no matches found".to_string());
    }

    Ok(lines.join("\n"))
}

fn render_last_tool_debug_report(session: &Session) -> Result<String, Box<dyn std::error::Error>> {
    let last_tool_use = session
        .messages
        .iter()
        .rev()
        .find_map(|message| {
            message.blocks.iter().rev().find_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
        })
        .ok_or_else(|| "no prior tool call found in session".to_string())?;

    let tool_result = session.messages.iter().rev().find_map(|message| {
        message.blocks.iter().rev().find_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } if tool_use_id == &last_tool_use.0 => {
                Some((tool_name.clone(), output.clone(), *is_error))
            }
            _ => None,
        })
    });

    let mut lines = vec![
        "Debug tool call".to_string(),
        "  Action           inspect the last recorded tool call and its result".to_string(),
        format!("  Tool id          {}", last_tool_use.0),
        format!("  Tool name        {}", last_tool_use.1),
        "  Input".to_string(),
        indent_block(&last_tool_use.2, 4),
    ];

    match tool_result {
        Some((tool_name, output, is_error)) => {
            lines.push("  Result".to_string());
            lines.push(format!("    name           {tool_name}"));
            lines.push(format!(
                "    status         {}",
                if is_error { "error" } else { "ok" }
            ));
            lines.push(indent_block(&output, 4));
        }
        None => lines.push("  Result           missing tool result".to_string()),
    }

    Ok(lines.join("\n"))
}

fn indent_block(value: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_no_args(
    command_name: &str,
    args: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(args) = args.map(str::trim).filter(|value| !value.is_empty()) {
        return Err(format!(
            "{command_name} does not accept arguments. Received: {args}\nUsage: {command_name}"
        )
        .into());
    }
    Ok(())
}

fn format_bughunter_report(scope: Option<&str>) -> String {
    format!(
        "Bughunter
  Scope            {}
  Action           inspect the selected code for likely bugs and correctness issues
  Output           findings should include file paths, severity, and suggested fixes",
        scope.unwrap_or("the current repository")
    )
}

fn format_ultraplan_report(task: Option<&str>) -> String {
    format!(
        "Ultraplan
  Task             {}
  Action           break work into a multi-step execution plan
  Output           plan should cover goals, risks, sequencing, verification, and rollback",
        task.unwrap_or("the current repo work")
    )
}

fn format_pr_report(branch: &str, context: Option<&str>) -> String {
    format!(
        "PR
  Branch           {branch}
  Context          {}
  Action           draft or create a pull request for the current branch
  Output           title and markdown body suitable for GitHub",
        context.unwrap_or("none")
    )
}

fn format_issue_report(context: Option<&str>) -> String {
    format!(
        "Issue
  Context          {}
  Action           draft or create a GitHub issue from the current context
  Output           title and markdown body suitable for GitHub",
        context.unwrap_or("none")
    )
}

fn git_output(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn git_status_ok(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(())
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn write_temp_text_file(
    filename: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = env::temp_dir().join(filename);
    fs::write(&path, contents)?;
    Ok(path)
}

const DEFAULT_HISTORY_LIMIT: usize = 20;

fn parse_history_count(raw: Option<&str>) -> Result<usize, String> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_HISTORY_LIMIT);
    };
    let parsed: usize = raw
        .parse()
        .map_err(|_| format!("history: invalid count '{raw}'. Expected a positive integer."))?;
    if parsed == 0 {
        return Err("history: count must be greater than 0.".to_string());
    }
    Ok(parsed)
}

fn format_history_timestamp(timestamp_ms: u64) -> String {
    let secs = timestamp_ms / 1_000;
    let subsec_ms = timestamp_ms % 1_000;
    let days_since_epoch = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let hours = seconds_of_day / 3_600;
    let minutes = (seconds_of_day % 3_600) / 60;
    let seconds = seconds_of_day % 60;

    let (year, month, day) = civil_from_days(i64::try_from(days_since_epoch).unwrap_or(0));
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{subsec_ms:03}Z")
}

// Computes civil (Gregorian) year/month/day from days since the Unix epoch
// (1970-01-01) using Howard Hinnant's `civil_from_days` algorithm.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64; // [0, 146_096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = y + i64::from(m <= 2);
    (y as i32, m as u32, d as u32)
}

fn render_prompt_history_report(entries: &[PromptHistoryEntry], limit: usize) -> String {
    if entries.is_empty() {
        return "Prompt history\n  Result           no prompts recorded yet".to_string();
    }

    let total = entries.len();
    let start = total.saturating_sub(limit);
    let shown = &entries[start..];
    let mut lines = vec![
        "Prompt history".to_string(),
        format!("  Total            {total}"),
        format!("  Showing          {} most recent", shown.len()),
        format!("  Reverse search   Ctrl-R in the REPL"),
        String::new(),
    ];
    for (offset, entry) in shown.iter().enumerate() {
        let absolute_index = start + offset + 1;
        let timestamp = format_history_timestamp(entry.timestamp_ms);
        let first_line = entry.text.lines().next().unwrap_or("").trim();
        let display = if first_line.chars().count() > 80 {
            let truncated: String = first_line.chars().take(77).collect();
            format!("{truncated}...")
        } else {
            first_line.to_string()
        };
        lines.push(format!("  {absolute_index:>3}. [{timestamp}] {display}"));
    }
    lines.join("\n")
}

fn collect_session_prompt_history(session: &Session) -> Vec<PromptHistoryEntry> {
    if !session.prompt_history.is_empty() {
        return session
            .prompt_history
            .iter()
            .map(|entry| PromptHistoryEntry {
                timestamp_ms: entry.timestamp_ms,
                text: entry.text.clone(),
            })
            .collect();
    }
    let timestamp_ms = session.updated_at_ms;
    session
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .filter_map(|message| {
            message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(PromptHistoryEntry {
                    timestamp_ms,
                    text: text.clone(),
                }),
                _ => None,
            })
        })
        .collect()
}

fn recent_user_context(session: &Session, limit: usize) -> String {
    let requests = session
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .filter_map(|message| {
            message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.trim().to_string()),
                _ => None,
            })
        })
        .rev()
        .take(limit)
        .collect::<Vec<_>>();

    if requests.is_empty() {
        "<no prior user messages>".to_string()
    } else {
        requests
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, text)| format!("{}. {}", index + 1, text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn truncate_for_prompt(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.trim().to_string()
    } else {
        let truncated = value.chars().take(limit).collect::<String>();
        format!("{}\n…[truncated]", truncated.trim_end())
    }
}

fn sanitize_generated_message(value: &str) -> String {
    value.trim().trim_matches('`').trim().replace("\r\n", "\n")
}

fn parse_titled_body(value: &str) -> Option<(String, String)> {
    let normalized = sanitize_generated_message(value);
    let title = normalized
        .lines()
        .find_map(|line| line.strip_prefix("TITLE:").map(str::trim))?;
    let body_start = normalized.find("BODY:")?;
    let body = normalized[body_start + "BODY:".len()..].trim();
    Some((title.to_string(), body.to_string()))
}

fn render_version_report() -> String {
    let git_sha = GIT_SHA.unwrap_or("unknown");
    let target = BUILD_TARGET.unwrap_or("unknown");
    format!(
        "Claw Code\n  Version          {VERSION}\n  Git SHA          {git_sha}\n  Target           {target}\n  Build date       {DEFAULT_DATE}"
    )
}

fn render_export_text(session: &Session) -> String {
    let mut lines = vec!["# Conversation Export".to_string(), String::new()];
    for (index, message) in session.messages.iter().enumerate() {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        lines.push(format!("## {}. {role}", index + 1));
        for block in &message.blocks {
            match block {
                ContentBlock::Text { text } => lines.push(text.clone()),
                ContentBlock::ToolUse { id, name, input } => {
                    lines.push(format!("[tool_use id={id} name={name}] {input}"));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    tool_name,
                    output,
                    is_error,
                } => {
                    lines.push(format!(
                        "[tool_result id={tool_use_id} name={tool_name} error={is_error}] {output}"
                    ));
                }
            }
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

fn default_export_filename(session: &Session) -> String {
    let stem = session
        .messages
        .iter()
        .find_map(|message| match message.role {
            MessageRole::User => message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .map_or("conversation", |text| {
            text.lines().next().unwrap_or("conversation")
        })
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    let fallback = if stem.is_empty() {
        "conversation"
    } else {
        &stem
    };
    format!("{fallback}.txt")
}

fn resolve_export_path(
    requested_path: Option<&str>,
    session: &Session,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let file_name =
        requested_path.map_or_else(|| default_export_filename(session), ToOwned::to_owned);
    let final_name = if Path::new(&file_name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
    {
        file_name
    } else {
        format!("{file_name}.txt")
    };
    Ok(cwd.join(final_name))
}

const SESSION_MARKDOWN_TOOL_SUMMARY_LIMIT: usize = 280;

fn summarize_tool_payload_for_markdown(payload: &str) -> String {
    let compact = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value.to_string(),
        Err(_) => payload.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    if compact.is_empty() {
        return String::new();
    }
    truncate_for_summary(&compact, SESSION_MARKDOWN_TOOL_SUMMARY_LIMIT)
}

fn run_export(
    session_reference: &str,
    output_path: Option<&Path>,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let (handle, session) = load_session_reference(session_reference)?;
    let markdown = render_session_markdown(&session, &handle.id, &handle.path);

    if let Some(path) = output_path {
        fs::write(path, &markdown)?;
        let report = format!(
            "Export\n  Result           wrote markdown transcript\n  File             {}\n  Session          {}\n  Messages         {}",
            path.display(),
            handle.id,
            session.messages.len(),
        );
        match output_format {
            CliOutputFormat::Text => println!("{report}"),
            CliOutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "kind": "export",
                    "message": report,
                    "session_id": handle.id,
                    "file": path.display().to_string(),
                    "messages": session.messages.len(),
                }))?
            ),
        }
        return Ok(());
    }

    match output_format {
        CliOutputFormat::Text => {
            print!("{markdown}");
            if !markdown.ends_with('\n') {
                println!();
            }
        }
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "export",
                "session_id": handle.id,
                "file": handle.path.display().to_string(),
                "messages": session.messages.len(),
                "markdown": markdown,
            }))?
        ),
    }
    Ok(())
}

fn render_session_markdown(session: &Session, session_id: &str, session_path: &Path) -> String {
    let mut lines = vec![
        "# Conversation Export".to_string(),
        String::new(),
        format!("- **Session**: `{session_id}`"),
        format!("- **File**: `{}`", session_path.display()),
        format!("- **Messages**: {}", session.messages.len()),
    ];
    if let Some(workspace_root) = session.workspace_root() {
        lines.push(format!("- **Workspace**: `{}`", workspace_root.display()));
    }
    if let Some(fork) = &session.fork {
        let branch = fork.branch_name.as_deref().unwrap_or("(unnamed)");
        lines.push(format!(
            "- **Forked from**: `{}` (branch `{branch}`)",
            fork.parent_session_id
        ));
    }
    if let Some(compaction) = &session.compaction {
        lines.push(format!(
            "- **Compactions**: {} (last removed {} messages)",
            compaction.count, compaction.removed_message_count
        ));
    }
    lines.push(String::new());
    lines.push("---".to_string());
    lines.push(String::new());

    for (index, message) in session.messages.iter().enumerate() {
        let role = match message.role {
            MessageRole::System => "System",
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::Tool => "Tool",
        };
        lines.push(format!("## {}. {role}", index + 1));
        lines.push(String::new());
        for block in &message.blocks {
            match block {
                ContentBlock::Text { text } => {
                    let trimmed = text.trim_end();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                        lines.push(String::new());
                    }
                }
                ContentBlock::ToolUse { id, name, input } => {
                    lines.push(format!(
                        "**Tool call** `{name}` _(id `{}`)_",
                        short_tool_id(id)
                    ));
                    let summary = summarize_tool_payload_for_markdown(input);
                    if !summary.is_empty() {
                        lines.push(format!("> {summary}"));
                    }
                    lines.push(String::new());
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    tool_name,
                    output,
                    is_error,
                } => {
                    let status = if *is_error { "error" } else { "ok" };
                    lines.push(format!(
                        "**Tool result** `{tool_name}` _(id `{}`, {status})_",
                        short_tool_id(tool_use_id)
                    ));
                    let summary = summarize_tool_payload_for_markdown(output);
                    if !summary.is_empty() {
                        lines.push(format!("> {summary}"));
                    }
                    lines.push(String::new());
                }
            }
        }
        if let Some(usage) = message.usage {
            lines.push(format!(
                "_tokens: in={} out={} cache_create={} cache_read={}_",
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
            ));
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

fn short_tool_id(id: &str) -> String {
    let char_count = id.chars().count();
    if char_count <= 12 {
        return id.to_string();
    }
    let prefix: String = id.chars().take(12).collect();
    format!("{prefix}…")
}

fn build_system_prompt() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(load_system_prompt(
        env::current_dir()?,
        DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )?)
}

fn build_runtime_plugin_state() -> Result<RuntimePluginState, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    build_runtime_plugin_state_with_loader(&cwd, &loader, &runtime_config)
}

fn build_runtime_plugin_state_with_loader(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> Result<RuntimePluginState, Box<dyn std::error::Error>> {
    let plugin_manager = build_plugin_manager(cwd, loader, runtime_config);
    let plugin_registry = plugin_manager.plugin_registry()?;
    let plugin_hook_config =
        runtime_hook_config_from_plugin_hooks(plugin_registry.aggregated_hooks()?);
    let feature_config = runtime_config
        .feature_config()
        .clone()
        .with_hooks(runtime_config.hooks().merged(&plugin_hook_config));
    let (mcp_state, runtime_tools) = build_runtime_mcp_state(runtime_config)?;
    let tool_registry = GlobalToolRegistry::with_plugin_tools(plugin_registry.aggregated_tools()?)?
        .with_runtime_tools(runtime_tools)?;
    Ok(RuntimePluginState {
        feature_config,
        tool_registry,
        plugin_registry,
        mcp_state,
    })
}

fn build_plugin_manager(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

fn resolve_plugin_path(cwd: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

fn runtime_hook_config_from_plugin_hooks(hooks: PluginHooks) -> runtime::RuntimeHookConfig {
    runtime::RuntimeHookConfig::new(
        hooks.pre_tool_use,
        hooks.post_tool_use,
        hooks.post_tool_use_failure,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalPromptProgressState {
    command_label: &'static str,
    task_label: String,
    step: usize,
    phase: String,
    detail: Option<String>,
    saw_final_text: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalPromptProgressEvent {
    Started,
    Update,
    Heartbeat,
    Complete,
    Failed,
}

#[derive(Debug)]
struct InternalPromptProgressShared {
    state: Mutex<InternalPromptProgressState>,
    output_lock: Mutex<()>,
    started_at: Instant,
}

#[derive(Debug, Clone)]
struct InternalPromptProgressReporter {
    shared: Arc<InternalPromptProgressShared>,
}

#[derive(Debug)]
struct InternalPromptProgressRun {
    reporter: InternalPromptProgressReporter,
    heartbeat_stop: Option<mpsc::Sender<()>>,
    heartbeat_handle: Option<thread::JoinHandle<()>>,
}

impl InternalPromptProgressReporter {
    fn ultraplan(task: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Ultraplan",
                    task_label: task.to_string(),
                    step: 0,
                    phase: "planning started".to_string(),
                    detail: Some(format!("task: {task}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    fn emit(&self, event: InternalPromptProgressEvent, error: Option<&str>) {
        let snapshot = self.snapshot();
        let line = format_internal_prompt_progress_line(event, &snapshot, self.elapsed(), error);
        self.write_line(&line);
    }

    fn mark_model_phase(&self) {
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = if state.step == 1 {
                "analyzing request".to_string()
            } else {
                "reviewing findings".to_string()
            };
            state.detail = Some(format!("task: {}", state.task_label));
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn mark_tool_phase(&self, name: &str, input: &str) {
        let detail = describe_tool_progress(name, input);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = format!("running {name}");
            state.detail = Some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn mark_text_phase(&self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let detail = truncate_for_summary(first_visible_line(trimmed), 120);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            if state.saw_final_text {
                return;
            }
            state.saw_final_text = true;
            state.step += 1;
            state.phase = "drafting final plan".to_string();
            state.detail = (!detail.is_empty()).then_some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn emit_heartbeat(&self) {
        let snapshot = self.snapshot();
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Heartbeat,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn snapshot(&self) -> InternalPromptProgressState {
        self.shared
            .state
            .lock()
            .expect("internal prompt progress state poisoned")
            .clone()
    }

    fn elapsed(&self) -> Duration {
        self.shared.started_at.elapsed()
    }

    fn write_line(&self, line: &str) {
        let _guard = self
            .shared
            .output_lock
            .lock()
            .expect("internal prompt progress output lock poisoned");
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}

impl InternalPromptProgressRun {
    fn start_ultraplan(task: &str) -> Self {
        let reporter = InternalPromptProgressReporter::ultraplan(task);
        reporter.emit(InternalPromptProgressEvent::Started, None);

        let (heartbeat_stop, heartbeat_rx) = mpsc::channel();
        let heartbeat_reporter = reporter.clone();
        let heartbeat_handle = thread::spawn(move || loop {
            match heartbeat_rx.recv_timeout(INTERNAL_PROGRESS_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => heartbeat_reporter.emit_heartbeat(),
            }
        });

        Self {
            reporter,
            heartbeat_stop: Some(heartbeat_stop),
            heartbeat_handle: Some(heartbeat_handle),
        }
    }

    fn reporter(&self) -> InternalPromptProgressReporter {
        self.reporter.clone()
    }

    fn finish_success(&mut self) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Complete, None);
    }

    fn finish_failure(&mut self, error: &str) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Failed, Some(error));
    }

    fn stop_heartbeat(&mut self) {
        if let Some(sender) = self.heartbeat_stop.take() {
            let _ = sender.send(());
        }
        if let Some(handle) = self.heartbeat_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for InternalPromptProgressRun {
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

fn format_internal_prompt_progress_line(
    event: InternalPromptProgressEvent,
    snapshot: &InternalPromptProgressState,
    elapsed: Duration,
    error: Option<&str>,
) -> String {
    let elapsed_seconds = elapsed.as_secs();
    let step_label = if snapshot.step == 0 {
        "current step pending".to_string()
    } else {
        format!("current step {}", snapshot.step)
    };
    let mut status_bits = vec![step_label, format!("phase {}", snapshot.phase)];
    if let Some(detail) = snapshot
        .detail
        .as_deref()
        .filter(|detail| !detail.is_empty())
    {
        status_bits.push(detail.to_string());
    }
    let status = status_bits.join(" · ");
    match event {
        InternalPromptProgressEvent::Started => {
            format!(
                "🧭 {} status · planning started · {status}",
                snapshot.command_label
            )
        }
        InternalPromptProgressEvent::Update => {
            format!("… {} status · {status}", snapshot.command_label)
        }
        InternalPromptProgressEvent::Heartbeat => format!(
            "… {} heartbeat · {elapsed_seconds}s elapsed · {status}",
            snapshot.command_label
        ),
        InternalPromptProgressEvent::Complete => format!(
            "✔ {} status · completed · {elapsed_seconds}s elapsed · {} steps total",
            snapshot.command_label, snapshot.step
        ),
        InternalPromptProgressEvent::Failed => format!(
            "✘ {} status · failed · {elapsed_seconds}s elapsed · {}",
            snapshot.command_label,
            error.unwrap_or("unknown error")
        ),
    }
}

fn describe_tool_progress(name: &str, input: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()));
    match name {
        "bash" | "Bash" => {
            let command = parsed
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if command.is_empty() {
                "running shell command".to_string()
            } else {
                format!("command {}", truncate_for_summary(command.trim(), 100))
            }
        }
        "read_file" | "Read" => format!("reading {}", extract_tool_path(&parsed)),
        "write_file" | "Write" => format!("writing {}", extract_tool_path(&parsed)),
        "edit_file" | "Edit" => format!("editing {}", extract_tool_path(&parsed)),
        "glob_search" | "Glob" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("glob `{pattern}` in {scope}")
        }
        "grep_search" | "Grep" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("grep `{pattern}` in {scope}")
        }
        "web_search" | "WebSearch" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .map_or_else(
                || "running web search".to_string(),
                |query| format!("query {}", truncate_for_summary(query, 100)),
            ),
        _ => {
            let summary = summarize_tool_payload(input);
            if summary.is_empty() {
                format!("running {name}")
            } else {
                format!("{name}: {summary}")
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
fn build_runtime(
    session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    let runtime_plugin_state = build_runtime_plugin_state()?;
    build_runtime_with_plugin_state(
        session,
        session_id,
        model,
        system_prompt,
        enable_tools,
        emit_output,
        allowed_tools,
        permission_mode,
        progress_reporter,
        runtime_plugin_state,
    )
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
fn build_runtime_with_plugin_state(
    mut session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
    runtime_plugin_state: RuntimePluginState,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    // Persist the model in session metadata so resumed sessions can report it.
    if session.model.is_none() {
        session.model = Some(model.clone());
    }
    let RuntimePluginState {
        feature_config,
        tool_registry,
        plugin_registry,
        mcp_state,
    } = runtime_plugin_state;
    plugin_registry.initialize()?;
    let policy = permission_policy(permission_mode, &feature_config, &tool_registry)
        .map_err(std::io::Error::other)?;
    let mut runtime = ConversationRuntime::new_with_features(
        session,
        AnthropicRuntimeClient::new(
            session_id,
            model,
            enable_tools,
            emit_output,
            allowed_tools.clone(),
            tool_registry.clone(),
            progress_reporter,
        )?,
        CliToolExecutor::new(
            allowed_tools.clone(),
            emit_output,
            tool_registry.clone(),
            mcp_state.clone(),
        ),
        policy,
        system_prompt,
        &feature_config,
    );
    if emit_output {
        runtime = runtime.with_hook_progress_reporter(Box::new(CliHookProgressReporter));
    }
    Ok(BuiltRuntime::new(runtime, plugin_registry, mcp_state))
}

struct CliHookProgressReporter;

impl runtime::HookProgressReporter for CliHookProgressReporter {
    fn on_event(&mut self, event: &runtime::HookProgressEvent) {
        match event {
            runtime::HookProgressEvent::Started {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
            runtime::HookProgressEvent::Completed {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook done {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
            runtime::HookProgressEvent::Cancelled {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook cancelled {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
        }
    }
}

struct CliPermissionPrompter {
    current_mode: PermissionMode,
}

impl CliPermissionPrompter {
    fn new(current_mode: PermissionMode) -> Self {
        Self { current_mode }
    }
}

impl runtime::PermissionPrompter for CliPermissionPrompter {
    fn decide(
        &mut self,
        request: &runtime::PermissionRequest,
    ) -> runtime::PermissionPromptDecision {
        println!();
        println!("Permission approval required");
        println!("  Tool             {}", request.tool_name);
        println!("  Current mode     {}", self.current_mode.as_str());
        println!("  Required mode    {}", request.required_mode.as_str());
        if let Some(reason) = &request.reason {
            println!("  Reason           {reason}");
        }
        println!("  Input            {}", request.input);
        print!("Approve this tool call? [y/N]: ");
        let _ = io::stdout().flush();

        let mut response = String::new();
        match io::stdin().read_line(&mut response) {
            Ok(_) => {
                let normalized = response.trim().to_ascii_lowercase();
                if matches!(normalized.as_str(), "y" | "yes") {
                    runtime::PermissionPromptDecision::Allow
                } else {
                    runtime::PermissionPromptDecision::Deny {
                        reason: format!(
                            "tool '{}' denied by user approval prompt",
                            request.tool_name
                        ),
                    }
                }
            }
            Err(error) => runtime::PermissionPromptDecision::Deny {
                reason: format!("permission approval failed: {error}"),
            },
        }
    }
}

// NOTE: Despite the historical name `AnthropicRuntimeClient`, this struct
// now holds an `ApiProviderClient` which dispatches to Anthropic, xAI,
// OpenAI, or DashScope at construction time based on
// `detect_provider_kind(&model)`. The struct name is kept to avoid
// churning `BuiltRuntime` and every Deref/DerefMut site that references
// it. See ROADMAP #29 for the provider-dispatch routing fix.
struct AnthropicRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ApiProviderClient,
    session_id: String,
    model: String,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    progress_reporter: Option<InternalPromptProgressReporter>,
    reasoning_effort: Option<String>,
}

impl AnthropicRuntimeClient {
    fn new(
        session_id: &str,
        model: String,
        enable_tools: bool,
        emit_output: bool,
        allowed_tools: Option<AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        progress_reporter: Option<InternalPromptProgressReporter>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Dispatch to the correct provider at construction time.
        // `ApiProviderClient` (exposed by the api crate as
        // `ProviderClient`) is an enum over Anthropic / xAI / OpenAI
        // variants, where xAI and OpenAI both use the OpenAI-compat
        // wire format under the hood. We consult
        // `detect_provider_kind(&resolved_model)` so model-name prefix
        // routing (`openai/`, `gpt-`, `grok`, `qwen/`) wins over
        // env-var presence.
        //
        // For Anthropic we build the client directly instead of going
        // through `ApiProviderClient::from_model_with_anthropic_auth`
        // so we can explicitly apply `api::read_base_url()` — that
        // reads `ANTHROPIC_BASE_URL` and is required for the local
        // mock-server test harness
        // (`crates/rusty-claude-cli/tests/compact_output.rs`) to point
        // claw at its fake Anthropic endpoint. We also attach a
        // session-scoped prompt cache on the Anthropic path; the
        // prompt cache is Anthropic-only so non-Anthropic variants
        // skip it.
        let resolved_model = api::resolve_model_alias(&model);
        let client = match detect_provider_kind(&resolved_model) {
            ProviderKind::Anthropic => {
                let auth = resolve_cli_auth_source()?;
                let inner = AnthropicClient::from_auth(auth)
                    .with_base_url(api::read_base_url())
                    .with_prompt_cache(PromptCache::new(session_id));
                ApiProviderClient::Anthropic(inner)
            }
            ProviderKind::Xai | ProviderKind::OpenAi => {
                // The api crate's `ProviderClient::from_model_with_anthropic_auth`
                // with `None` for the anthropic auth routes via
                // `detect_provider_kind` and builds an
                // `OpenAiCompatClient::from_env` with the matching
                // `OpenAiCompatConfig` (openai / xai / dashscope).
                // That reads the correct API-key env var and BASE_URL
                // override internally, so this one call covers OpenAI,
                // OpenRouter, xAI, DashScope, Ollama, and any other
                // OpenAI-compat endpoint users configure via
                // `OPENAI_BASE_URL` / `XAI_BASE_URL` / `DASHSCOPE_BASE_URL`.
                ApiProviderClient::from_model_with_anthropic_auth(&resolved_model, None)?
            }
        };
        Ok(Self {
            runtime: tokio::runtime::Runtime::new()?,
            client,
            session_id: session_id.to_string(),
            model,
            enable_tools,
            emit_output,
            allowed_tools,
            tool_registry,
            progress_reporter,
            reasoning_effort: None,
        })
    }

    fn set_reasoning_effort(&mut self, effort: Option<String>) {
        self.reasoning_effort = effort;
    }
}

fn resolve_cli_auth_source() -> Result<AuthSource, Box<dyn std::error::Error>> {
    Ok(resolve_cli_auth_source_for_cwd()?)
}

fn resolve_cli_auth_source_for_cwd() -> Result<AuthSource, api::ApiError> {
    resolve_startup_auth_source(|| Ok(None))
}

impl ApiClient for AnthropicRuntimeClient {
    #[allow(clippy::too_many_lines)]
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if let Some(progress_reporter) = &self.progress_reporter {
            progress_reporter.mark_model_phase();
        }
        let is_post_tool = request_ends_with_tool_result(&request);
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: self
                .enable_tools
                .then(|| filter_tool_specs(&self.tool_registry, self.allowed_tools.as_ref())),
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            stream: true,
            reasoning_effort: self.reasoning_effort.clone(),
            ..Default::default()
        };

        self.runtime.block_on(async {
            // When resuming after tool execution, apply a stall timeout on the
            // first stream event.  If the model does not respond within the
            // deadline we drop the stalled connection and re-send the request as
            // a continuation nudge (one retry only).
            let max_attempts: usize = if is_post_tool { 2 } else { 1 };

            for attempt in 1..=max_attempts {
                let result = self
                    .consume_stream(&message_request, is_post_tool && attempt == 1)
                    .await;
                match result {
                    Ok(events) => return Ok(events),
                    Err(error)
                        if error.to_string().contains("post-tool stall")
                            && attempt < max_attempts =>
                    {
                        // Stalled after tool completion — nudge the model by
                        // re-sending the same request.
                    }
                    Err(error) => return Err(error),
                }
            }

            Err(RuntimeError::new("post-tool continuation nudge exhausted"))
        })
    }
}

impl AnthropicRuntimeClient {
    /// Consume a single streaming response, optionally applying a stall
    /// timeout on the first event for post-tool continuations.
    #[allow(clippy::too_many_lines)]
    async fn consume_stream(
        &self,
        message_request: &MessageRequest,
        apply_stall_timeout: bool,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let mut stream = self
            .client
            .stream_message(message_request)
            .await
            .map_err(|error| {
                RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
            })?;
        let mut stdout = io::stdout();
        let mut sink = io::sink();
        let out: &mut dyn Write = if self.emit_output {
            &mut stdout
        } else {
            &mut sink
        };
        let renderer = TerminalRenderer::new();
        let mut markdown_stream = MarkdownStreamState::default();
        let mut events = Vec::new();
        let mut pending_tool: Option<(String, String, String)> = None;
        let mut block_has_thinking_summary = false;
        let mut saw_stop = false;
        let mut received_any_event = false;

        loop {
            let next = if apply_stall_timeout && !received_any_event {
                match tokio::time::timeout(POST_TOOL_STALL_TIMEOUT, stream.next_event()).await {
                    Ok(inner) => inner.map_err(|error| {
                        RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
                    })?,
                    Err(_elapsed) => {
                        return Err(RuntimeError::new(
                            "post-tool stall: model did not respond within timeout",
                        ));
                    }
                }
            } else {
                stream.next_event().await.map_err(|error| {
                    RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
                })?
            };

            let Some(event) = next else {
                break;
            };
            received_any_event = true;

            match event {
                ApiStreamEvent::MessageStart(start) => {
                    for block in start.message.content {
                        push_output_block(
                            block,
                            out,
                            &mut events,
                            &mut pending_tool,
                            true,
                            &mut block_has_thinking_summary,
                        )?;
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    push_output_block(
                        start.content_block,
                        out,
                        &mut events,
                        &mut pending_tool,
                        true,
                        &mut block_has_thinking_summary,
                    )?;
                }
                ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if !text.is_empty() {
                            if let Some(progress_reporter) = &self.progress_reporter {
                                progress_reporter.mark_text_phase(&text);
                            }
                            if let Some(rendered) = markdown_stream.push(&renderer, &text) {
                                write!(out, "{rendered}")
                                    .and_then(|()| out.flush())
                                    .map_err(|error| RuntimeError::new(error.to_string()))?;
                            }
                            events.push(AssistantEvent::TextDelta(text));
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, _, input)) = &mut pending_tool {
                            input.push_str(&partial_json);
                        }
                    }
                    ContentBlockDelta::ThinkingDelta { .. } => {
                        if !block_has_thinking_summary {
                            render_thinking_block_summary(out, None, false)?;
                            block_has_thinking_summary = true;
                        }
                    }
                    ContentBlockDelta::SignatureDelta { .. } => {}
                },
                ApiStreamEvent::ContentBlockStop(_) => {
                    block_has_thinking_summary = false;
                    if let Some(rendered) = markdown_stream.flush(&renderer) {
                        write!(out, "{rendered}")
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                    }
                    if let Some((id, name, input)) = pending_tool.take() {
                        if let Some(progress_reporter) = &self.progress_reporter {
                            progress_reporter.mark_tool_phase(&name, &input);
                        }
                        // Display tool call now that input is fully accumulated
                        writeln!(out, "\n{}", format_tool_call_start(&name, &input))
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                        events.push(AssistantEvent::ToolUse { id, name, input });
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    events.push(AssistantEvent::Usage(delta.usage.token_usage()));
                }
                ApiStreamEvent::MessageStop(_) => {
                    saw_stop = true;
                    if let Some(rendered) = markdown_stream.flush(&renderer) {
                        write!(out, "{rendered}")
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                    }
                    events.push(AssistantEvent::MessageStop);
                }
            }
        }

        push_prompt_cache_record(&self.client, &mut events);

        if !saw_stop
            && events.iter().any(|event| {
                matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                    || matches!(event, AssistantEvent::ToolUse { .. })
            })
        {
            events.push(AssistantEvent::MessageStop);
        }

        if events
            .iter()
            .any(|event| matches!(event, AssistantEvent::MessageStop))
        {
            return Ok(events);
        }

        let response = self
            .client
            .send_message(&MessageRequest {
                stream: false,
                ..message_request.clone()
            })
            .await
            .map_err(|error| {
                RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
            })?;
        let mut events = response_to_events(response, out)?;
        push_prompt_cache_record(&self.client, &mut events);
        Ok(events)
    }
}

/// Returns `true` when the conversation ends with a tool-result message,
/// meaning the model is expected to continue after tool execution.
fn request_ends_with_tool_result(request: &ApiRequest) -> bool {
    request
        .messages
        .last()
        .is_some_and(|message| message.role == MessageRole::Tool)
}

fn format_user_visible_api_error(session_id: &str, error: &api::ApiError) -> String {
    if error.is_context_window_failure() {
        format_context_window_blocked_error(session_id, error)
    } else if error.is_generic_fatal_wrapper() {
        let mut qualifiers = vec![format!("session {session_id}")];
        if let Some(request_id) = error.request_id() {
            qualifiers.push(format!("trace {request_id}"));
        }
        format!(
            "{} ({}): {}",
            error.safe_failure_class(),
            qualifiers.join(", "),
            error
        )
    } else {
        error.to_string()
    }
}

fn format_context_window_blocked_error(session_id: &str, error: &api::ApiError) -> String {
    let mut lines = vec![
        "Context window blocked".to_string(),
        "  Failure class    context_window_blocked".to_string(),
        format!("  Session          {session_id}"),
    ];

    if let Some(request_id) = error.request_id() {
        lines.push(format!("  Trace            {request_id}"));
    }

    match error {
        api::ApiError::ContextWindowExceeded {
            model,
            estimated_input_tokens,
            requested_output_tokens,
            estimated_total_tokens,
            context_window_tokens,
        } => {
            lines.push(format!("  Model            {model}"));
            lines.push(format!(
                "  Input estimate   ~{estimated_input_tokens} tokens (heuristic)"
            ));
            lines.push(format!(
                "  Requested output {requested_output_tokens} tokens"
            ));
            lines.push(format!(
                "  Total estimate   ~{estimated_total_tokens} tokens (heuristic)"
            ));
            lines.push(format!("  Context window   {context_window_tokens} tokens"));
        }
        api::ApiError::Api { message, body, .. } => {
            let detail = message.as_deref().unwrap_or(body).trim();
            if !detail.is_empty() {
                lines.push(format!(
                    "  Detail           {}",
                    truncate_for_summary(detail, 120)
                ));
            }
        }
        api::ApiError::RetriesExhausted { last_error, .. } => {
            let detail = match last_error.as_ref() {
                api::ApiError::Api { message, body, .. } => message.as_deref().unwrap_or(body),
                other => return format_context_window_blocked_error(session_id, other),
            }
            .trim();
            if !detail.is_empty() {
                lines.push(format!(
                    "  Detail           {}",
                    truncate_for_summary(detail, 120)
                ));
            }
        }
        _ => {}
    }

    lines.push(String::new());
    lines.push("Recovery".to_string());
    lines.push("  Compact          /compact".to_string());
    lines.push(format!(
        "  Resume compact   claw --resume {session_id} /compact"
    ));
    lines.push("  Fresh session    /clear --confirm".to_string());
    lines.push(
        "  Reduce scope     remove large pasted context/files or ask for a smaller slice"
            .to_string(),
    );
    lines.push("  Retry            rerun after compacting or reducing the request".to_string());

    lines.join("\n")
}

fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn collect_tool_uses(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .assistant_messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some(json!({
                "id": id,
                "name": name,
                "input": input,
            })),
            _ => None,
        })
        .collect()
}

fn collect_tool_results(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .tool_results
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Some(json!({
                "tool_use_id": tool_use_id,
                "tool_name": tool_name,
                "output": output,
                "is_error": is_error,
            })),
            _ => None,
        })
        .collect()
}

fn collect_prompt_cache_events(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .prompt_cache_events
        .iter()
        .map(|event| {
            json!({
                "unexpected": event.unexpected,
                "reason": event.reason,
                "previous_cache_read_input_tokens": event.previous_cache_read_input_tokens,
                "current_cache_read_input_tokens": event.current_cache_read_input_tokens,
                "token_drop": event.token_drop,
            })
        })
        .collect()
}

/// Slash commands that are registered in the spec list but not yet implemented
/// in this build. Used to filter both REPL completions and help output so the
/// discovery surface only shows commands that actually work (ROADMAP #39).
const STUB_COMMANDS: &[&str] = &[
    "login",
    "logout",
    "vim",
    "upgrade",
    "share",
    "feedback",
    "files",
    "fast",
    "exit",
    "summary",
    "desktop",
    "brief",
    "advisor",
    "stickers",
    "insights",
    "thinkback",
    "release-notes",
    "security-review",
    "keybindings",
    "privacy-settings",
    "plan",
    "review",
    "tasks",
    "theme",
    "voice",
    "usage",
    "rename",
    "copy",
    "hooks",
    "context",
    "color",
    "effort",
    "branch",
    "rewind",
    "ide",
    "tag",
    "output-style",
    "add-dir",
    // Spec entries with no parse arm — produce circular "Did you mean" error
    // without this guard. Adding here routes them to the proper unsupported
    // message and excludes them from REPL completions / help.
    // NOTE: do NOT add "stats", "tokens", "cache" — they are implemented.
    "allowed-tools",
    "bookmarks",
    "workspace",
    "reasoning",
    "budget",
    "rate-limit",
    "changelog",
    "diagnostics",
    "metrics",
    "tool-details",
    "focus",
    "unfocus",
    "pin",
    "unpin",
    "language",
    "profile",
    "max-tokens",
    "temperature",
    "system-prompt",
    "notifications",
    "telemetry",
    "env",
    "project",
    "terminal-setup",
    "api-key",
    "reset",
    "undo",
    "stop",
    "retry",
    "paste",
    "screenshot",
    "image",
    "search",
    "listen",
    "speak",
    "format",
    "test",
    "lint",
    "build",
    "run",
    "git",
    "stash",
    "blame",
    "log",
    "cron",
    "team",
    "benchmark",
    "migrate",
    "templates",
    "explain",
    "refactor",
    "docs",
    "fix",
    "perf",
    "chat",
    "web",
    "map",
    "symbols",
    "references",
    "definition",
    "hover",
    "autofix",
    "multi",
    "macro",
    "alias",
    "parallel",
    "subagent",
    "agent",
];

fn slash_command_completion_candidates_with_sessions(
    model: &str,
    active_session_id: Option<&str>,
    recent_session_ids: Vec<String>,
) -> Vec<String> {
    let mut completions = BTreeSet::new();

    for spec in slash_command_specs() {
        if STUB_COMMANDS.contains(&spec.name) {
            continue;
        }
        completions.insert(format!("/{}", spec.name));
        for alias in spec.aliases {
            if !STUB_COMMANDS.contains(alias) {
                completions.insert(format!("/{alias}"));
            }
        }
    }

    for candidate in [
        "/bughunter ",
        "/clear --confirm",
        "/config ",
        "/config env",
        "/config hooks",
        "/config model",
        "/config plugins",
        "/mcp ",
        "/mcp list",
        "/mcp show ",
        "/export ",
        "/issue ",
        "/model ",
        "/model opus",
        "/model sonnet",
        "/model haiku",
        "/permissions ",
        "/permissions read-only",
        "/permissions workspace-write",
        "/permissions danger-full-access",
        "/plugin list",
        "/plugin install ",
        "/plugin enable ",
        "/plugin disable ",
        "/plugin uninstall ",
        "/plugin update ",
        "/plugins list",
        "/pr ",
        "/resume ",
        "/session list",
        "/session switch ",
        "/session fork ",
        "/teleport ",
        "/ultraplan ",
        "/agents help",
        "/mcp help",
        "/skills help",
    ] {
        completions.insert(candidate.to_string());
    }

    if !model.trim().is_empty() {
        completions.insert(format!("/model {}", resolve_model_alias(model)));
        completions.insert(format!("/model {model}"));
    }

    if let Some(active_session_id) = active_session_id.filter(|value| !value.trim().is_empty()) {
        completions.insert(format!("/resume {active_session_id}"));
        completions.insert(format!("/session switch {active_session_id}"));
    }

    for session_id in recent_session_ids
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .take(10)
    {
        completions.insert(format!("/resume {session_id}"));
        completions.insert(format!("/session switch {session_id}"));
    }

    completions.into_iter().collect()
}

fn format_tool_call_start(name: &str, input: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()));

    let detail = match name {
        "bash" | "Bash" => format_bash_call(&parsed),
        "read_file" | "Read" => {
            let path = extract_tool_path(&parsed);
            format!("\x1b[2m📄 Reading {path}…\x1b[0m")
        }
        "write_file" | "Write" => {
            let path = extract_tool_path(&parsed);
            let lines = parsed
                .get("content")
                .and_then(|value| value.as_str())
                .map_or(0, |content| content.lines().count());
            format!("\x1b[1;32m✏️ Writing {path}\x1b[0m \x1b[2m({lines} lines)\x1b[0m")
        }
        "edit_file" | "Edit" => {
            let path = extract_tool_path(&parsed);
            let old_value = parsed
                .get("old_string")
                .or_else(|| parsed.get("oldString"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let new_value = parsed
                .get("new_string")
                .or_else(|| parsed.get("newString"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            format!(
                "\x1b[1;33m📝 Editing {path}\x1b[0m{}",
                format_patch_preview(old_value, new_value)
                    .map(|preview| format!("\n{preview}"))
                    .unwrap_or_default()
            )
        }
        "glob_search" | "Glob" => format_search_start("🔎 Glob", &parsed),
        "grep_search" | "Grep" => format_search_start("🔎 Grep", &parsed),
        "web_search" | "WebSearch" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("?")
            .to_string(),
        _ => summarize_tool_payload(input),
    };

    let border = "─".repeat(name.len() + 8);
    format!(
        "\x1b[38;5;245m╭─ \x1b[1;36m{name}\x1b[0;38;5;245m ─╮\x1b[0m\n\x1b[38;5;245m│\x1b[0m {detail}\n\x1b[38;5;245m╰{border}╯\x1b[0m"
    )
}

fn format_tool_result(name: &str, output: &str, is_error: bool) -> String {
    let icon = if is_error {
        "\x1b[1;31m✗\x1b[0m"
    } else {
        "\x1b[1;32m✓\x1b[0m"
    };
    if is_error {
        let summary = truncate_for_summary(output.trim(), 160);
        return if summary.is_empty() {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
        } else {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m\n\x1b[38;5;203m{summary}\x1b[0m")
        };
    }

    let parsed: serde_json::Value =
        serde_json::from_str(output).unwrap_or(serde_json::Value::String(output.to_string()));
    match name {
        "bash" | "Bash" => format_bash_result(icon, &parsed),
        "read_file" | "Read" => format_read_result(icon, &parsed),
        "write_file" | "Write" => format_write_result(icon, &parsed),
        "edit_file" | "Edit" => format_edit_result(icon, &parsed),
        "glob_search" | "Glob" => format_glob_result(icon, &parsed),
        "grep_search" | "Grep" => format_grep_result(icon, &parsed),
        _ => format_generic_tool_result(icon, name, &parsed),
    }
}

const DISPLAY_TRUNCATION_NOTICE: &str =
    "\x1b[2m… output truncated for display; full result preserved in session.\x1b[0m";
const READ_DISPLAY_MAX_LINES: usize = 80;
const READ_DISPLAY_MAX_CHARS: usize = 6_000;
const TOOL_OUTPUT_DISPLAY_MAX_LINES: usize = 60;
const TOOL_OUTPUT_DISPLAY_MAX_CHARS: usize = 4_000;

fn extract_tool_path(parsed: &serde_json::Value) -> String {
    parsed
        .get("file_path")
        .or_else(|| parsed.get("filePath"))
        .or_else(|| parsed.get("path"))
        .and_then(|value| value.as_str())
        .unwrap_or("?")
        .to_string()
}

fn format_search_start(label: &str, parsed: &serde_json::Value) -> String {
    let pattern = parsed
        .get("pattern")
        .and_then(|value| value.as_str())
        .unwrap_or("?");
    let scope = parsed
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or(".");
    format!("{label} {pattern}\n\x1b[2min {scope}\x1b[0m")
}

fn format_patch_preview(old_value: &str, new_value: &str) -> Option<String> {
    if old_value.is_empty() && new_value.is_empty() {
        return None;
    }
    Some(format!(
        "\x1b[38;5;203m- {}\x1b[0m\n\x1b[38;5;70m+ {}\x1b[0m",
        truncate_for_summary(first_visible_line(old_value), 72),
        truncate_for_summary(first_visible_line(new_value), 72)
    ))
}

fn format_bash_call(parsed: &serde_json::Value) -> String {
    let command = parsed
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if command.is_empty() {
        String::new()
    } else {
        format!(
            "\x1b[48;5;236;38;5;255m $ {} \x1b[0m",
            truncate_for_summary(command, 160)
        )
    }
}

fn first_visible_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text)
}

fn format_bash_result(icon: &str, parsed: &serde_json::Value) -> String {
    use std::fmt::Write as _;

    let mut lines = vec![format!("{icon} \x1b[38;5;245mbash\x1b[0m")];
    if let Some(task_id) = parsed
        .get("backgroundTaskId")
        .and_then(|value| value.as_str())
    {
        write!(&mut lines[0], " backgrounded ({task_id})").expect("write to string");
    } else if let Some(status) = parsed
        .get("returnCodeInterpretation")
        .and_then(|value| value.as_str())
        .filter(|status| !status.is_empty())
    {
        write!(&mut lines[0], " {status}").expect("write to string");
    }

    if let Some(stdout) = parsed.get("stdout").and_then(|value| value.as_str()) {
        if !stdout.trim().is_empty() {
            lines.push(truncate_output_for_display(
                stdout,
                TOOL_OUTPUT_DISPLAY_MAX_LINES,
                TOOL_OUTPUT_DISPLAY_MAX_CHARS,
            ));
        }
    }
    if let Some(stderr) = parsed.get("stderr").and_then(|value| value.as_str()) {
        if !stderr.trim().is_empty() {
            lines.push(format!(
                "\x1b[38;5;203m{}\x1b[0m",
                truncate_output_for_display(
                    stderr,
                    TOOL_OUTPUT_DISPLAY_MAX_LINES,
                    TOOL_OUTPUT_DISPLAY_MAX_CHARS,
                )
            ));
        }
    }

    lines.join("\n\n")
}

fn format_read_result(icon: &str, parsed: &serde_json::Value) -> String {
    let file = parsed.get("file").unwrap_or(parsed);
    let path = extract_tool_path(file);
    let start_line = file
        .get("startLine")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    let num_lines = file
        .get("numLines")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let total_lines = file
        .get("totalLines")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(num_lines);
    let content = file
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let end_line = start_line.saturating_add(num_lines.saturating_sub(1));

    format!(
        "{icon} \x1b[2m📄 Read {path} (lines {}-{} of {})\x1b[0m\n{}",
        start_line,
        end_line.max(start_line),
        total_lines,
        truncate_output_for_display(content, READ_DISPLAY_MAX_LINES, READ_DISPLAY_MAX_CHARS)
    )
}

fn format_write_result(icon: &str, parsed: &serde_json::Value) -> String {
    let path = extract_tool_path(parsed);
    let kind = parsed
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("write");
    let line_count = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .map_or(0, |content| content.lines().count());
    format!(
        "{icon} \x1b[1;32m✏️ {} {path}\x1b[0m \x1b[2m({line_count} lines)\x1b[0m",
        if kind == "create" { "Wrote" } else { "Updated" },
    )
}

fn format_structured_patch_preview(parsed: &serde_json::Value) -> Option<String> {
    let hunks = parsed.get("structuredPatch")?.as_array()?;
    let mut preview = Vec::new();
    for hunk in hunks.iter().take(2) {
        let lines = hunk.get("lines")?.as_array()?;
        for line in lines.iter().filter_map(|value| value.as_str()).take(6) {
            match line.chars().next() {
                Some('+') => preview.push(format!("\x1b[38;5;70m{line}\x1b[0m")),
                Some('-') => preview.push(format!("\x1b[38;5;203m{line}\x1b[0m")),
                _ => preview.push(line.to_string()),
            }
        }
    }
    if preview.is_empty() {
        None
    } else {
        Some(preview.join("\n"))
    }
}

fn format_edit_result(icon: &str, parsed: &serde_json::Value) -> String {
    let path = extract_tool_path(parsed);
    let suffix = if parsed
        .get("replaceAll")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        " (replace all)"
    } else {
        ""
    };
    let preview = format_structured_patch_preview(parsed).or_else(|| {
        let old_value = parsed
            .get("oldString")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let new_value = parsed
            .get("newString")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        format_patch_preview(old_value, new_value)
    });

    match preview {
        Some(preview) => format!("{icon} \x1b[1;33m📝 Edited {path}{suffix}\x1b[0m\n{preview}"),
        None => format!("{icon} \x1b[1;33m📝 Edited {path}{suffix}\x1b[0m"),
    }
}

fn format_glob_result(icon: &str, parsed: &serde_json::Value) -> String {
    let num_files = parsed
        .get("numFiles")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let filenames = parsed
        .get("filenames")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str())
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if filenames.is_empty() {
        format!("{icon} \x1b[38;5;245mglob_search\x1b[0m matched {num_files} files")
    } else {
        format!("{icon} \x1b[38;5;245mglob_search\x1b[0m matched {num_files} files\n{filenames}")
    }
}

fn format_grep_result(icon: &str, parsed: &serde_json::Value) -> String {
    let num_matches = parsed
        .get("numMatches")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let num_files = parsed
        .get("numFiles")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let content = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let filenames = parsed
        .get("filenames")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str())
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let summary = format!(
        "{icon} \x1b[38;5;245mgrep_search\x1b[0m {num_matches} matches across {num_files} files"
    );
    if !content.trim().is_empty() {
        format!(
            "{summary}\n{}",
            truncate_output_for_display(
                content,
                TOOL_OUTPUT_DISPLAY_MAX_LINES,
                TOOL_OUTPUT_DISPLAY_MAX_CHARS,
            )
        )
    } else if !filenames.is_empty() {
        format!("{summary}\n{filenames}")
    } else {
        summary
    }
}

fn format_generic_tool_result(icon: &str, name: &str, parsed: &serde_json::Value) -> String {
    let rendered_output = match parsed {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            serde_json::to_string_pretty(parsed).unwrap_or_else(|_| parsed.to_string())
        }
        _ => parsed.to_string(),
    };
    let preview = truncate_output_for_display(
        &rendered_output,
        TOOL_OUTPUT_DISPLAY_MAX_LINES,
        TOOL_OUTPUT_DISPLAY_MAX_CHARS,
    );

    if preview.is_empty() {
        format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
    } else if preview.contains('\n') {
        format!("{icon} \x1b[38;5;245m{name}\x1b[0m\n{preview}")
    } else {
        format!("{icon} \x1b[38;5;245m{name}:\x1b[0m {preview}")
    }
}

fn summarize_tool_payload(payload: &str) -> String {
    let compact = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value.to_string(),
        Err(_) => payload.trim().to_string(),
    };
    truncate_for_summary(&compact, 96)
}

fn truncate_for_summary(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn truncate_output_for_display(content: &str, max_lines: usize, max_chars: usize) -> String {
    let original = content.trim_end_matches('\n');
    if original.is_empty() {
        return String::new();
    }

    let mut preview_lines = Vec::new();
    let mut used_chars = 0usize;
    let mut truncated = false;

    for (index, line) in original.lines().enumerate() {
        if index >= max_lines {
            truncated = true;
            break;
        }

        let newline_cost = usize::from(!preview_lines.is_empty());
        let available = max_chars.saturating_sub(used_chars + newline_cost);
        if available == 0 {
            truncated = true;
            break;
        }

        let line_chars = line.chars().count();
        if line_chars > available {
            preview_lines.push(line.chars().take(available).collect::<String>());
            truncated = true;
            break;
        }

        preview_lines.push(line.to_string());
        used_chars += newline_cost + line_chars;
    }

    let mut preview = preview_lines.join("\n");
    if truncated {
        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(DISPLAY_TRUNCATION_NOTICE);
    }
    preview
}

fn render_thinking_block_summary(
    out: &mut (impl Write + ?Sized),
    char_count: Option<usize>,
    redacted: bool,
) -> Result<(), RuntimeError> {
    let summary = if redacted {
        "\n▶ Thinking block hidden by provider\n".to_string()
    } else if let Some(char_count) = char_count {
        format!("\n▶ Thinking ({char_count} chars hidden)\n")
    } else {
        "\n▶ Thinking hidden\n".to_string()
    };
    write!(out, "{summary}")
        .and_then(|()| out.flush())
        .map_err(|error| RuntimeError::new(error.to_string()))
}

fn push_output_block(
    block: OutputContentBlock,
    out: &mut (impl Write + ?Sized),
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    streaming_tool_input: bool,
    block_has_thinking_summary: &mut bool,
) -> Result<(), RuntimeError> {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let rendered = TerminalRenderer::new().markdown_to_ansi(&text);
                write!(out, "{rendered}")
                    .and_then(|()| out.flush())
                    .map_err(|error| RuntimeError::new(error.to_string()))?;
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            // During streaming, the initial content_block_start has an empty input ({}).
            // The real input arrives via input_json_delta events. In
            // non-streaming responses, preserve a legitimate empty object.
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { thinking, .. } => {
            render_thinking_block_summary(out, Some(thinking.chars().count()), false)?;
            *block_has_thinking_summary = true;
        }
        OutputContentBlock::RedactedThinking { .. } => {
            render_thinking_block_summary(out, None, true)?;
            *block_has_thinking_summary = true;
        }
    }
    Ok(())
}

fn response_to_events(
    response: MessageResponse,
    out: &mut (impl Write + ?Sized),
) -> Result<Vec<AssistantEvent>, RuntimeError> {
    let mut events = Vec::new();
    let mut pending_tool = None;

    for block in response.content {
        let mut block_has_thinking_summary = false;
        push_output_block(
            block,
            out,
            &mut events,
            &mut pending_tool,
            false,
            &mut block_has_thinking_summary,
        )?;
        if let Some((id, name, input)) = pending_tool.take() {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    events.push(AssistantEvent::Usage(response.usage.token_usage()));
    events.push(AssistantEvent::MessageStop);
    Ok(events)
}

fn push_prompt_cache_record(client: &ApiProviderClient, events: &mut Vec<AssistantEvent>) {
    // `ApiProviderClient::take_last_prompt_cache_record` is a pass-through
    // to the Anthropic variant and returns `None` for OpenAI-compat /
    // xAI variants, which do not have a prompt cache. So this helper
    // remains a no-op on non-Anthropic providers without any extra
    // branching here.
    if let Some(record) = client.take_last_prompt_cache_record() {
        if let Some(event) = prompt_cache_record_to_runtime_event(record) {
            events.push(AssistantEvent::PromptCache(event));
        }
    }
}

fn prompt_cache_record_to_runtime_event(
    record: api::PromptCacheRecord,
) -> Option<PromptCacheEvent> {
    let cache_break = record.cache_break?;
    Some(PromptCacheEvent {
        unexpected: cache_break.unexpected,
        reason: cache_break.reason,
        previous_cache_read_input_tokens: cache_break.previous_cache_read_input_tokens,
        current_cache_read_input_tokens: cache_break.current_cache_read_input_tokens,
        token_drop: cache_break.token_drop,
    })
}

struct CliToolExecutor {
    renderer: TerminalRenderer,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
}

impl CliToolExecutor {
    fn new(
        allowed_tools: Option<AllowedToolSet>,
        emit_output: bool,
        tool_registry: GlobalToolRegistry,
        mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    ) -> Self {
        Self {
            renderer: TerminalRenderer::new(),
            emit_output,
            allowed_tools,
            tool_registry,
            mcp_state,
        }
    }

    fn execute_search_tool(&self, value: serde_json::Value) -> Result<String, ToolError> {
        let input: ToolSearchRequest = serde_json::from_value(value)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let (pending_mcp_servers, mcp_degraded) =
            self.mcp_state.as_ref().map_or((None, None), |state| {
                let state = state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (state.pending_servers(), state.degraded_report())
            });
        serde_json::to_string_pretty(&self.tool_registry.search(
            &input.query,
            input.max_results.unwrap_or(5),
            pending_mcp_servers,
            mcp_degraded,
        ))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn execute_runtime_tool(
        &self,
        tool_name: &str,
        value: serde_json::Value,
    ) -> Result<String, ToolError> {
        let Some(mcp_state) = &self.mcp_state else {
            return Err(ToolError::new(format!(
                "runtime tool `{tool_name}` is unavailable without configured MCP servers"
            )));
        };
        let mut mcp_state = mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match tool_name {
            "MCPTool" => {
                let input: McpToolRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                let qualified_name = input
                    .qualified_name
                    .or(input.tool)
                    .ok_or_else(|| ToolError::new("missing required field `qualifiedName`"))?;
                mcp_state.call_tool(&qualified_name, input.arguments)
            }
            "ListMcpResourcesTool" => {
                let input: ListMcpResourcesRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                match input.server {
                    Some(server_name) => mcp_state.list_resources_for_server(&server_name),
                    None => mcp_state.list_resources_for_all_servers(),
                }
            }
            "ReadMcpResourceTool" => {
                let input: ReadMcpResourceRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                mcp_state.read_resource(&input.server, &input.uri)
            }
            _ => mcp_state.call_tool(tool_name, Some(value)),
        }
    }

    fn execute_legacy_mcp_builtin(
        &self,
        tool_name: &str,
        value: serde_json::Value,
    ) -> Option<Result<String, ToolError>> {
        if !matches!(
            tool_name,
            "ListMcpResources" | "ReadMcpResource" | "McpAuth" | "MCP"
        ) {
            return None;
        }
        let mcp_state = self.mcp_state.as_ref()?.clone();
        let mut mcp_state = mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        Some(match tool_name {
            "ListMcpResources" => {
                let input = match serde_json::from_value::<ListMcpResourcesRequest>(value) {
                    Ok(input) => input,
                    Err(error) => {
                        return Some(Err(ToolError::new(format!(
                            "invalid tool input JSON: {error}"
                        ))));
                    }
                };
                match input.server {
                    Some(server_name) => mcp_state.list_resources_for_server_legacy(&server_name),
                    None => mcp_state.list_resources_for_all_servers_legacy(),
                }
            }
            "ReadMcpResource" => serde_json::from_value::<ReadMcpResourceRequest>(value)
                .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))
                .and_then(|input| mcp_state.read_resource_legacy(&input.server, &input.uri)),
            "McpAuth" => serde_json::from_value::<McpAuthRequest>(value)
                .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))
                .and_then(|input| mcp_state.auth_status(&input.server)),
            "MCP" => serde_json::from_value::<McpToolRequest>(value)
                .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))
                .and_then(|input| {
                    if let Some(qualified_name) = input.qualified_name {
                        return mcp_state.call_tool(&qualified_name, input.arguments);
                    }
                    let server = input.server.ok_or_else(|| {
                        ToolError::new("missing required field `server` or `qualifiedName`")
                    })?;
                    let tool = input
                        .tool
                        .ok_or_else(|| ToolError::new("missing required field `tool`"))?;
                    mcp_state.call_legacy_tool(&server, &tool, input.arguments)
                }),
            _ => unreachable!("legacy MCP built-in names are matched above"),
        })
    }
}

impl ToolExecutor for CliToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let result = if tool_name == "ToolSearch" {
            self.execute_search_tool(value)
        } else if self.tool_registry.has_runtime_tool(tool_name) {
            self.execute_runtime_tool(tool_name, value)
        } else if let Some(result) = self.execute_legacy_mcp_builtin(tool_name, value.clone()) {
            result
        } else {
            self.tool_registry
                .execute(tool_name, &value)
                .map_err(ToolError::new)
        };
        match result {
            Ok(output) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &output, false);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|error| ToolError::new(error.to_string()))?;
                }
                Ok(output)
            }
            Err(error) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &error.to_string(), true);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|stream_error| ToolError::new(stream_error.to_string()))?;
                }
                Err(error)
            }
        }
    }
}

fn permission_policy(
    mode: PermissionMode,
    feature_config: &runtime::RuntimeFeatureConfig,
    tool_registry: &GlobalToolRegistry,
) -> Result<PermissionPolicy, String> {
    Ok(tool_registry.permission_specs(None)?.into_iter().fold(
        PermissionPolicy::new(mode).with_permission_rules(feature_config.permission_rules()),
        |policy, (name, required_permission)| {
            policy.with_tool_requirement(name, required_permission)
        },
    ))
}

/// Reclassify a pre-stringified tool-result `output` into a parsed JSON
/// envelope when it matches a shape the OpenAI-compatible flattener
/// (`api::providers::openai_compat::flatten_tool_result_content`) knows how
/// to extract. Returns `None` for plain text and any unrecognized JSON shape
/// so the caller falls back to a verbatim `ToolResultContentBlock::Text`
/// emission. Without this reclassification, the `OpenAI` flattener's `Text`
/// arm passes the stringified envelope through verbatim and Ollama-routed
/// models reject it with `"Value looks like object, but can't find closing
/// '}' symbol"`.
fn maybe_parse_tool_result_json_envelope(output: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(output).ok()?;
    let obj = value.as_object()?;
    if let Some(file) = obj.get("file").and_then(Value::as_object) {
        if file.get("content").and_then(Value::as_str).is_some()
            || file.get("text").and_then(Value::as_str).is_some()
        {
            return Some(value);
        }
    }
    for key in ["text", "content", "output", "result", "message", "error"] {
        if obj.get(key).and_then(Value::as_str).is_some() {
            return Some(value);
        }
    }
    None
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => {
                        let content_block =
                            if let Some(value) = maybe_parse_tool_result_json_envelope(output) {
                                ToolResultContentBlock::Json { value }
                            } else {
                                ToolResultContentBlock::Text {
                                    text: output.clone(),
                                }
                            };
                        InputContentBlock::ToolResult {
                            tool_use_id: tool_use_id.clone(),
                            content: vec![content_block],
                            is_error: *is_error,
                        }
                    }
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn print_help_to(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "claw v{VERSION}")?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(
        out,
        "  claw [--model MODEL] [--allowedTools TOOL[,TOOL...]]"
    )?;
    writeln!(out, "      Start the interactive REPL")?;
    writeln!(
        out,
        "  claw [--model MODEL] [--output-format text|json] prompt TEXT"
    )?;
    writeln!(out, "      Send one prompt and exit")?;
    writeln!(
        out,
        "  claw [--model MODEL] [--output-format text|json] TEXT"
    )?;
    writeln!(out, "      Shorthand non-interactive prompt mode")?;
    writeln!(
        out,
        "  claw --resume [SESSION.jsonl|session-id|latest] [/status] [/compact] [...]"
    )?;
    writeln!(
        out,
        "      Inspect or maintain a saved session without entering the REPL"
    )?;
    writeln!(out, "  claw help")?;
    writeln!(out, "      Alias for --help")?;
    writeln!(out, "  claw version")?;
    writeln!(out, "      Alias for --version")?;
    writeln!(out, "  claw status")?;
    writeln!(
        out,
        "      Show the current local workspace status snapshot"
    )?;
    writeln!(out, "  claw sandbox")?;
    writeln!(out, "      Show the current sandbox isolation snapshot")?;
    writeln!(out, "  claw doctor")?;
    writeln!(
        out,
        "      Diagnose local auth, config, workspace, and sandbox health"
    )?;
    writeln!(out, "  claw acp [serve]")?;
    writeln!(
        out,
        "      Show ACP/Zed editor integration status (currently unsupported; aliases: --acp, -acp)"
    )?;
    writeln!(out, "      Source of truth: {OFFICIAL_REPO_SLUG}")?;
    writeln!(
        out,
        "      Warning: do not `{DEPRECATED_INSTALL_COMMAND}` (deprecated stub)"
    )?;
    writeln!(out, "  claw dump-manifests [--manifests-dir PATH]")?;
    writeln!(out, "  claw bootstrap-plan")?;
    writeln!(out, "  claw agents")?;
    writeln!(out, "  claw mcp")?;
    writeln!(out, "  claw skills")?;
    writeln!(out, "  claw system-prompt [--cwd PATH] [--date YYYY-MM-DD]")?;
    writeln!(out, "  claw init")?;
    writeln!(
        out,
        "  claw export [PATH] [--session SESSION] [--output PATH]"
    )?;
    writeln!(
        out,
        "      Dump the latest (or named) session as markdown; writes to PATH or stdout"
    )?;
    writeln!(out)?;
    writeln!(out, "Flags:")?;
    writeln!(
        out,
        "  --model MODEL              Override the active model"
    )?;
    writeln!(
        out,
        "  --output-format FORMAT     Non-interactive output format: text or json"
    )?;
    writeln!(
        out,
        "  --compact                  Strip tool call details; print only the final assistant text (text mode only; useful for piping)"
    )?;
    writeln!(
        out,
        "  --permission-mode MODE     Set read-only, workspace-write, or danger-full-access"
    )?;
    writeln!(
        out,
        "  --dangerously-skip-permissions  Skip all permission checks"
    )?;
    writeln!(out, "  --allowedTools TOOLS       Restrict enabled tools (repeatable; comma-separated aliases supported)")?;
    writeln!(
        out,
        "  --version, -V              Print version and build information locally"
    )?;
    writeln!(out)?;
    writeln!(out, "Interactive slash commands:")?;
    writeln!(out, "{}", render_slash_command_help_filtered(STUB_COMMANDS))?;
    writeln!(out)?;
    let resume_commands = resume_supported_slash_commands()
        .into_iter()
        .map(|spec| match spec.argument_hint {
            Some(argument_hint) => format!("/{} {}", spec.name, argument_hint),
            None => format!("/{}", spec.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "Resume-safe commands: {resume_commands}")?;
    writeln!(out)?;
    writeln!(out, "Session shortcuts:")?;
    writeln!(
        out,
        "  REPL turns auto-save to .claw/sessions/<session-id>.{PRIMARY_SESSION_EXTENSION}"
    )?;
    writeln!(
        out,
        "  Use `{LATEST_SESSION_REFERENCE}` with --resume, /resume, or /session switch to target the newest saved session"
    )?;
    writeln!(
        out,
        "  Use /session list in the REPL to browse managed sessions"
    )?;
    writeln!(out, "Examples:")?;
    writeln!(out, "  claw --model claude-opus \"summarize this repo\"")?;
    writeln!(
        out,
        "  claw --output-format json prompt \"explain src/main.rs\""
    )?;
    writeln!(out, "  claw --compact \"summarize Cargo.toml\" | wc -l")?;
    writeln!(
        out,
        "  claw --allowedTools read,glob \"summarize Cargo.toml\""
    )?;
    writeln!(out, "  claw --resume {LATEST_SESSION_REFERENCE}")?;
    writeln!(
        out,
        "  claw --resume {LATEST_SESSION_REFERENCE} /status /diff /export notes.txt"
    )?;
    writeln!(out, "  claw agents")?;
    writeln!(out, "  claw mcp show my-server")?;
    writeln!(out, "  claw /skills")?;
    writeln!(out, "  claw doctor")?;
    writeln!(out, "  source of truth: {OFFICIAL_REPO_URL}")?;
    writeln!(
        out,
        "  do not run `{DEPRECATED_INSTALL_COMMAND}` — it installs a deprecated stub"
    )?;
    writeln!(out, "  claw init")?;
    writeln!(out, "  claw export")?;
    writeln!(out, "  claw export conversation.md")?;
    Ok(())
}

fn print_help(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    print_help_to(&mut buffer)?;
    let message = String::from_utf8(buffer)?;
    match output_format {
        CliOutputFormat::Text => print!("{message}"),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "help",
                "message": message,
            }))?
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_runtime_plugin_state_with_loader, build_runtime_with_plugin_state,
        classify_error_kind, collect_session_prompt_history, create_managed_session_handle,
        describe_tool_progress, filter_tool_specs, format_bughunter_report,
        format_commit_preflight_report, format_commit_skipped_report, format_compact_report,
        format_connected_line, format_cost_report, format_history_timestamp,
        format_internal_prompt_progress_line, format_issue_report, format_model_report,
        format_model_switch_report, format_permissions_report, format_permissions_switch_report,
        format_pr_report, format_resume_report, format_status_report, format_tool_call_start,
        format_tool_result, format_ultraplan_report, format_unknown_slash_command,
        format_unknown_slash_command_message, format_user_visible_api_error,
        maybe_parse_tool_result_json_envelope, merge_prompt_with_stdin, normalize_permission_mode,
        parse_args, parse_export_args, parse_git_status_branch, parse_git_status_metadata_for,
        parse_git_workspace_summary, parse_history_count, permission_policy, print_help_to,
        push_output_block, render_config_report, render_diff_report, render_diff_report_for,
        render_help_topic, render_memory_report, render_prompt_history_report, render_repl_help,
        render_resume_usage, render_session_markdown, resolve_model_alias,
        resolve_model_alias_with_config, resolve_model_env_alias, resolve_repl_model,
        resolve_session_reference, response_to_events, resume_supported_slash_commands,
        run_resume_command, runtime_mcp_inventory_json_for_loader, short_tool_id,
        slash_command_completion_candidates_with_sessions, split_error_hint, status_context,
        summarize_tool_payload_for_markdown, try_resolve_bare_skill_prompt, validate_model_syntax,
        validate_no_args, write_mcp_server_fixture, write_mcp_tools_list_disconnect_fixture,
        CliAction, CliOutputFormat, CliToolExecutor, GitWorkspaceSummary,
        InternalPromptProgressEvent, InternalPromptProgressState, LiveCli, LocalHelpTopic,
        PromptHistoryEntry, RuntimePluginState, SlashCommand, StatusUsage, DEFAULT_MODEL,
        LATEST_SESSION_REFERENCE, STUB_COMMANDS,
    };
    use api::{
        ApiError, InputContentBlock, MessageResponse, OutputContentBlock, ToolResultContentBlock,
        Usage,
    };
    use plugins::{
        PluginManager, PluginManagerConfig, PluginTool, PluginToolDefinition, PluginToolPermission,
    };
    use runtime::{
        load_oauth_credentials, save_oauth_credentials, AssistantEvent, ConfigLoader, ContentBlock,
        ConversationMessage, MessageRole, OAuthConfig, PermissionMode, Session, ToolExecutor,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tools::GlobalToolRegistry;

    fn registry_with_plugin_tool() -> GlobalToolRegistry {
        GlobalToolRegistry::with_plugin_tools(vec![PluginTool::new(
            "plugin-demo@external",
            "plugin-demo",
            PluginToolDefinition {
                name: "plugin_echo".to_string(),
                description: Some("Echo plugin payload".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }),
            },
            "echo".to_string(),
            Vec::new(),
            PluginToolPermission::WorkspaceWrite,
            None,
        )])
        .expect("plugin tool registry should build")
    }

    #[test]
    fn opaque_provider_wrapper_surfaces_failure_class_session_and_trace() {
        let error = ApiError::Api {
            status: "500".parse().expect("status"),
            error_type: Some("api_error".to_string()),
            message: Some(
                "Something went wrong while processing your request. Please try again, or use /new to start a fresh session."
                    .to_string(),
            ),
            request_id: Some("req_jobdori_789".to_string()),
            body: String::new(),
            retryable: true,
            suggested_action: None,
        };

        let rendered = format_user_visible_api_error("session-issue-22", &error);
        assert!(rendered.contains("provider_internal"));
        assert!(rendered.contains("session session-issue-22"));
        assert!(rendered.contains("trace req_jobdori_789"));
    }

    #[test]
    fn retry_exhaustion_uses_retry_failure_class_for_generic_provider_wrapper() {
        let error = ApiError::RetriesExhausted {
            attempts: 3,
            last_error: Box::new(ApiError::Api {
                status: "502".parse().expect("status"),
                error_type: Some("api_error".to_string()),
                message: Some(
                    "Something went wrong while processing your request. Please try again, or use /new to start a fresh session."
                        .to_string(),
                ),
                request_id: Some("req_jobdori_790".to_string()),
                body: String::new(),
                retryable: true,
                suggested_action: None,
            }),
        };

        let rendered = format_user_visible_api_error("session-issue-22", &error);
        assert!(rendered.contains("provider_retry_exhausted"), "{rendered}");
        assert!(rendered.contains("session session-issue-22"));
        assert!(rendered.contains("trace req_jobdori_790"));
    }

    #[test]
    fn context_window_preflight_errors_render_recovery_steps() {
        let error = ApiError::ContextWindowExceeded {
            model: "claude-sonnet-4-6".to_string(),
            estimated_input_tokens: 182_000,
            requested_output_tokens: 64_000,
            estimated_total_tokens: 246_000,
            context_window_tokens: 200_000,
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("Context window blocked"), "{rendered}");
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Session          session-issue-32"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Model            claude-sonnet-4-6"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Input estimate   ~182000 tokens (heuristic)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Total estimate   ~246000 tokens (heuristic)"),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Resume compact   claw --resume session-issue-32 /compact"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Fresh session    /clear --confirm"),
            "{rendered}"
        );
        assert!(rendered.contains("Reduce scope"), "{rendered}");
        assert!(rendered.contains("Retry            rerun"), "{rendered}");
    }

    #[test]
    fn provider_context_window_errors_are_reframed_with_same_guidance() {
        let error = ApiError::Api {
            status: "400".parse().expect("status"),
            error_type: Some("invalid_request_error".to_string()),
            message: Some(
                "This model's maximum context length is 200000 tokens, but your request used 230000 tokens."
                    .to_string(),
            ),
            request_id: Some("req_ctx_456".to_string()),
            body: String::new(),
            retryable: false,
            suggested_action: None,
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Trace            req_ctx_456"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("Detail           This model's maximum context length is 200000 tokens"),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Fresh session    /clear --confirm"),
            "{rendered}"
        );
    }

    #[test]
    fn retry_wrapped_context_window_errors_keep_recovery_guidance() {
        let error = ApiError::RetriesExhausted {
            attempts: 2,
            last_error: Box::new(ApiError::Api {
                status: "413".parse().expect("status"),
                error_type: Some("invalid_request_error".to_string()),
                message: Some("Request is too large for this model's context window.".to_string()),
                request_id: Some("req_ctx_retry_789".to_string()),
                body: String::new(),
                retryable: false,
                suggested_action: None,
            }),
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("Context window blocked"), "{rendered}");
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Trace            req_ctx_retry_789"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("Detail           Request is too large for this model's context window."),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Resume compact   claw --resume session-issue-32 /compact"),
            "{rendered}"
        );
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("rusty-claude-cli-{nanos}-{unique}"))
    }

    fn git(args: &[&str], cwd: &Path) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git command should run");
        assert!(
            status.success(),
            "git command failed: git {}",
            args.join(" ")
        );
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn with_current_dir<T>(cwd: &Path, f: impl FnOnce() -> T) -> T {
        let _guard = cwd_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::current_dir().expect("cwd should load");
        std::env::set_current_dir(cwd).expect("cwd should change");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        std::env::set_current_dir(previous).expect("cwd should restore");
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn write_skill_fixture(root: &Path, name: &str, description: &str) {
        let skill_dir = root.join(name);
        fs::create_dir_all(&skill_dir).expect("skill dir should exist");
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .expect("skill file should write");
    }

    fn write_plugin_fixture(root: &Path, name: &str, include_hooks: bool, include_lifecycle: bool) {
        fs::create_dir_all(root.join(".claude-plugin")).expect("manifest dir");
        if include_hooks {
            fs::create_dir_all(root.join("hooks")).expect("hooks dir");
            fs::write(
                root.join("hooks").join("pre.sh"),
                "#!/bin/sh\nprintf 'plugin pre hook'\n",
            )
            .expect("write hook");
        }
        if include_lifecycle {
            fs::create_dir_all(root.join("lifecycle")).expect("lifecycle dir");
            fs::write(
                root.join("lifecycle").join("init.sh"),
                "#!/bin/sh\nprintf 'init\\n' >> lifecycle.log\n",
            )
            .expect("write init lifecycle");
            fs::write(
                root.join("lifecycle").join("shutdown.sh"),
                "#!/bin/sh\nprintf 'shutdown\\n' >> lifecycle.log\n",
            )
            .expect("write shutdown lifecycle");
        }

        let hooks = if include_hooks {
            ",\n  \"hooks\": {\n    \"PreToolUse\": [\"./hooks/pre.sh\"]\n  }"
        } else {
            ""
        };
        let lifecycle = if include_lifecycle {
            ",\n  \"lifecycle\": {\n    \"Init\": [\"./lifecycle/init.sh\"],\n    \"Shutdown\": [\"./lifecycle/shutdown.sh\"]\n  }"
        } else {
            ""
        };
        fs::write(
            root.join(".claude-plugin").join("plugin.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"1.0.0\",\n  \"description\": \"runtime plugin fixture\"{hooks}{lifecycle}\n}}"
            ),
        )
        .expect("write plugin manifest");
    }
    #[test]
    fn defaults_to_repl_when_no_args() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&[]).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn default_permission_mode_uses_project_config_when_env_is_unset() {
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"permissionMode":"acceptEdits"}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_permission_mode = std::env::var("RUSTY_CLAUDE_PERMISSION_MODE").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        let resolved = with_current_dir(&cwd, super::default_permission_mode);

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_permission_mode {
            Some(value) => std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", value),
            None => std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        assert_eq!(resolved, PermissionMode::WorkspaceWrite);
    }

    #[test]
    fn env_permission_mode_overrides_project_config_default() {
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"permissionMode":"acceptEdits"}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_permission_mode = std::env::var("RUSTY_CLAUDE_PERMISSION_MODE").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");

        let resolved = with_current_dir(&cwd, super::default_permission_mode);

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_permission_mode {
            Some(value) => std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", value),
            None => std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        assert_eq!(resolved, PermissionMode::ReadOnly);
    }

    #[test]
    fn default_permission_mode_falls_back_to_read_only_when_env_unset_and_no_project_config() {
        // Safe-by-default regression guard: with no RUSTY_CLAUDE_PERMISSION_MODE
        // env var and no project-level .claw/settings.json, the resolved
        // default must be ReadOnly (not DangerFullAccess). Callers that need
        // elevated access must opt in explicitly.
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(&cwd).expect("project dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_permission_mode = std::env::var("RUSTY_CLAUDE_PERMISSION_MODE").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        let resolved = with_current_dir(&cwd, super::default_permission_mode);

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_permission_mode {
            Some(value) => std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", value),
            None => std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        assert_eq!(resolved, PermissionMode::ReadOnly);
    }

    #[test]
    fn resolve_cli_auth_source_ignores_saved_oauth_credentials() {
        let _guard = env_lock();
        let config_home = temp_dir();
        std::fs::create_dir_all(&config_home).expect("config home should exist");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let original_auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");

        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "expired-access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(0),
            scopes: vec!["org:create_api_key".to_string(), "user:profile".to_string()],
        })
        .expect("save expired oauth credentials");

        let error = super::resolve_cli_auth_source_for_cwd()
            .expect_err("saved oauth should be ignored without env auth");

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_api_key {
            Some(value) => std::env::set_var("ANTHROPIC_API_KEY", value),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
        match original_auth_token {
            Some(value) => std::env::set_var("ANTHROPIC_AUTH_TOKEN", value),
            None => std::env::remove_var("ANTHROPIC_AUTH_TOKEN"),
        }
        std::fs::remove_dir_all(config_home).expect("temp config home should clean up");

        assert!(error.to_string().contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn parses_prompt_subcommand() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "prompt".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "hello world".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn merge_prompt_with_stdin_returns_prompt_unchanged_when_no_pipe() {
        // given
        let prompt = "Review this";

        // when
        let merged = merge_prompt_with_stdin(prompt, None);

        // then
        assert_eq!(merged, "Review this");
    }

    #[test]
    fn merge_prompt_with_stdin_ignores_whitespace_only_pipe() {
        // given
        let prompt = "Review this";
        let piped = "   \n\t\n  ";

        // when
        let merged = merge_prompt_with_stdin(prompt, Some(piped));

        // then
        assert_eq!(merged, "Review this");
    }

    #[test]
    fn merge_prompt_with_stdin_appends_piped_content_as_context() {
        // given
        let prompt = "Review this";
        let piped = "fn main() { println!(\"hi\"); }\n";

        // when
        let merged = merge_prompt_with_stdin(prompt, Some(piped));

        // then
        assert_eq!(merged, "Review this\n\nfn main() { println!(\"hi\"); }");
    }

    #[test]
    fn merge_prompt_with_stdin_trims_surrounding_whitespace_on_pipe() {
        // given
        let prompt = "Summarize";
        let piped = "\n\n  some notes  \n\n";

        // when
        let merged = merge_prompt_with_stdin(prompt, Some(piped));

        // then
        assert_eq!(merged, "Summarize\n\nsome notes");
    }

    #[test]
    fn merge_prompt_with_stdin_returns_pipe_when_prompt_is_empty() {
        // given
        let prompt = "";
        let piped = "standalone body";

        // when
        let merged = merge_prompt_with_stdin(prompt, Some(piped));

        // then
        assert_eq!(merged, "standalone body");
    }

    #[test]
    fn parses_bare_prompt_and_json_output_flag() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--output-format=json".to_string(),
            "--model".to_string(),
            "opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: "claude-opus-4-6".to_string(),
                output_format: CliOutputFormat::Json,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_compact_flag_for_prompt_mode() {
        // given a bare prompt invocation that includes the --compact flag
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--compact".to_string(),
            "summarize".to_string(),
            "this".to_string(),
        ];

        // when parse_args interprets the flag
        let parsed = parse_args(&args).expect("args should parse");

        // then compact mode is propagated and other defaults stay unchanged
        assert_eq!(
            parsed,
            CliAction::Prompt {
                prompt: "summarize this".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: true,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn prompt_subcommand_defaults_compact_to_false() {
        // given a `prompt` subcommand invocation without --compact
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec!["prompt".to_string(), "hello".to_string()];

        // when parse_args runs
        let parsed = parse_args(&args).expect("args should parse");

        // then compact stays false (opt-in flag)
        match parsed {
            CliAction::Prompt { compact, .. } => assert!(!compact),
            other => panic!("expected Prompt action, got {other:?}"),
        }
    }

    #[test]
    fn resolves_model_aliases_in_args() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--model".to_string(),
            "opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: "claude-opus-4-6".to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn resolves_known_model_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("sonnet"), "claude-sonnet-4-6");
        assert_eq!(resolve_model_alias("haiku"), "claude-haiku-4-5-20251213");
        assert_eq!(resolve_model_alias("claude-opus"), "claude-opus");
    }

    #[test]
    fn user_defined_aliases_resolve_before_provider_dispatch() {
        // given
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        let config_home = root.join("config-home");
        std::fs::create_dir_all(cwd.join(".claw")).expect("project config dir should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");
        std::fs::write(
            cwd.join(".claw").join("settings.json"),
            r#"{"aliases":{"fast":"claude-haiku-4-5-20251213","smart":"opus","cheap":"grok-3-mini"}}"#,
        )
        .expect("project config should write");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);

        // when
        let direct = with_current_dir(&cwd, || resolve_model_alias_with_config("fast"));
        let chained = with_current_dir(&cwd, || resolve_model_alias_with_config("smart"));
        let cross_provider = with_current_dir(&cwd, || resolve_model_alias_with_config("cheap"));
        let unknown = with_current_dir(&cwd, || resolve_model_alias_with_config("unknown-model"));
        let builtin = with_current_dir(&cwd, || resolve_model_alias_with_config("haiku"));

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(root).expect("temp config root should clean up");

        // then
        assert_eq!(direct, "claude-haiku-4-5-20251213");
        assert_eq!(chained, "claude-opus-4-6");
        assert_eq!(cross_provider, "grok-3-mini");
        assert_eq!(unknown, "unknown-model");
        assert_eq!(builtin, "claude-haiku-4-5-20251213");
    }

    #[test]
    fn env_model_alias_substitutes_in_args() {
        // given an operator profile that exported a bare env model alias
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        std::env::set_var("RUSTY_CLAUDE_MODEL_ALIAS__FAST", "qwen3:14b");
        let args = vec![
            "--model".to_string(),
            "fast".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];

        // when the CLI parses `--model fast`
        let parsed = parse_args(&args);

        // cleanup before assertions so an assertion failure does not leak env state
        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__FAST");

        // then the alias is accepted and its env value flows to the provider layer
        assert_eq!(
            parsed.expect("env-aliased `--model fast` should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: "qwen3:14b".to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn env_model_alias_blank_falls_back_to_strict_validation() {
        // given an env alias key set to a blank value
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        std::env::set_var("RUSTY_CLAUDE_MODEL_ALIAS__FAST", "   ");
        let args = vec![
            "prompt".to_string(),
            "test".to_string(),
            "--model".to_string(),
            "fast".to_string(),
        ];

        // when the CLI parses `--model fast`
        let parsed = parse_args(&args);

        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__FAST");

        // then blank env values are treated as absent and strict validation fires
        let err = parsed.expect_err("blank env alias should not satisfy validation");
        assert!(
            err.contains("invalid model syntax: 'fast'"),
            "blank env alias should fall through to strict invalid_model_syntax: {err}"
        );
    }

    #[test]
    fn env_model_alias_absent_yields_strict_invalid_model_syntax() {
        // given no env alias key for `fast`
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__FAST");
        let args = vec![
            "prompt".to_string(),
            "test".to_string(),
            "--model".to_string(),
            "fast".to_string(),
        ];

        // when the CLI parses `--model fast`
        let parsed = parse_args(&args);

        // then the existing strict-validation error still triggers
        let err = parsed.expect_err("absent env alias should fail strict validation");
        assert!(
            err.contains("invalid model syntax: 'fast'"),
            "absent env alias should fall through to strict invalid_model_syntax: {err}"
        );
    }

    #[test]
    fn env_model_alias_lookup_only_triggers_for_bare_alias_names() {
        // given an env key whose name happens to match a provider prefix
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        std::env::set_var("RUSTY_CLAUDE_MODEL_ALIAS__OPENAI", "should-never-resolve");
        let args = vec![
            "--model".to_string(),
            "openai/gpt-4".to_string(),
            "ping".to_string(),
        ];

        // when the CLI parses a provider/model string
        let parsed = parse_args(&args);

        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__OPENAI");

        // then the env-alias lookup is NOT consulted (`/` disqualifies the key)
        let action = parsed.expect("provider/model input should still parse");
        match action {
            CliAction::Prompt { model, .. } => assert_eq!(model, "openai/gpt-4"),
            other => panic!("expected Prompt action, got {other:?}"),
        }
    }

    #[test]
    fn resolve_model_alias_with_config_honors_env_alias() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_MODEL_ALIAS__DEEP", "qwen3.5:27b");
        let resolved = resolve_model_alias_with_config("deep");
        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__DEEP");
        assert_eq!(resolved, "qwen3.5:27b");
    }

    #[test]
    fn resolve_model_alias_with_config_treats_blank_env_alias_as_absent() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_MODEL_ALIAS__DEEP", "");
        let resolved = resolve_model_alias_with_config("deep");
        std::env::remove_var("RUSTY_CLAUDE_MODEL_ALIAS__DEEP");
        assert_eq!(resolved, "deep");
    }

    #[test]
    fn parses_version_flags_without_initializing_prompt_mode() {
        assert_eq!(
            parse_args(&["--version".to_string()]).expect("args should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["-V".to_string()]).expect("args should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_permission_mode_flag() {
        let args = vec!["--permission-mode=read-only".to_string()];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn dangerously_skip_permissions_flag_forces_danger_full_access_in_repl() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");
        let args = vec!["--dangerously-skip-permissions".to_string()];
        let parsed = parse_args(&args).expect("args should parse");
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        assert_eq!(
            parsed,
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn dangerously_skip_permissions_flag_applies_to_prompt_subcommand() {
        let _guard = env_lock();
        std::env::set_var("RUSTY_CLAUDE_PERMISSION_MODE", "read-only");
        let args = vec![
            "--dangerously-skip-permissions".to_string(),
            "prompt".to_string(),
            "do".to_string(),
            "the".to_string(),
            "thing".to_string(),
        ];
        let parsed = parse_args(&args).expect("args should parse");
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");

        assert_eq!(
            parsed,
            CliAction::Prompt {
                prompt: "do the thing".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_allowed_tools_flags_with_aliases_and_lists() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec![
            "--allowedTools".to_string(),
            "read,glob".to_string(),
            "--allowed-tools=write_file".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: DEFAULT_MODEL.to_string(),
                allowed_tools: Some(
                    ["glob_search", "read_file", "write_file"]
                        .into_iter()
                        .map(str::to_string)
                        .collect()
                ),
                permission_mode: PermissionMode::ReadOnly,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn rejects_unknown_allowed_tools() {
        let error = parse_args(&["--allowedTools".to_string(), "teleport".to_string()])
            .expect_err("tool should be rejected");
        assert!(error.contains("unsupported tool in --allowedTools: teleport"));
    }

    #[test]
    fn parses_system_prompt_options() {
        let args = vec![
            "system-prompt".to_string(),
            "--cwd".to_string(),
            "/tmp/project".to_string(),
            "--date".to_string(),
            "2026-04-01".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::PrintSystemPrompt {
                cwd: PathBuf::from("/tmp/project"),
                date: "2026-04-01".to_string(),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn removed_login_and_logout_subcommands_error_helpfully() {
        let login = parse_args(&["login".to_string()]).expect_err("login should be removed");
        assert!(login.contains("ANTHROPIC_API_KEY"));
        let logout = parse_args(&["logout".to_string()]).expect_err("logout should be removed");
        assert!(logout.contains("ANTHROPIC_AUTH_TOKEN"));
        assert_eq!(
            parse_args(&["doctor".to_string()]).expect("doctor should parse"),
            CliAction::Doctor {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["state".to_string()]).expect("state should parse"),
            CliAction::State {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "state".to_string(),
                "--output-format".to_string(),
                "json".to_string()
            ])
            .expect("state --output-format json should parse"),
            CliAction::State {
                output_format: CliOutputFormat::Json,
            }
        );
        assert_eq!(
            parse_args(&["init".to_string()]).expect("init should parse"),
            CliAction::Init {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["agents".to_string()]).expect("agents should parse"),
            CliAction::Agents {
                args: None,
                output_format: CliOutputFormat::Text
            }
        );
        assert_eq!(
            parse_args(&["mcp".to_string()]).expect("mcp should parse"),
            CliAction::Mcp {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["skills".to_string()]).expect("skills should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "skills".to_string(),
                "help".to_string(),
                "overview".to_string()
            ])
            .expect("skills help overview should invoke"),
            CliAction::Prompt {
                prompt: "$help overview".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        assert_eq!(
            parse_args(&["agents".to_string(), "--help".to_string()])
                .expect("agents help should parse"),
            CliAction::Agents {
                args: Some("--help".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        // #145: `plugins` must parse as CliAction::Plugins (not fall through
        // to the prompt path, which would hit the Anthropic API for a purely
        // local introspection command).
        assert_eq!(
            parse_args(&["plugins".to_string()]).expect("plugins should parse"),
            CliAction::Plugins {
                action: None,
                target: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["plugins".to_string(), "list".to_string()])
                .expect("plugins list should parse"),
            CliAction::Plugins {
                action: Some("list".to_string()),
                target: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "plugins".to_string(),
                "enable".to_string(),
                "example-bundled".to_string(),
            ])
            .expect("plugins enable <target> should parse"),
            CliAction::Plugins {
                action: Some("enable".to_string()),
                target: Some("example-bundled".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "plugins".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ])
            .expect("plugins --output-format json should parse"),
            CliAction::Plugins {
                action: None,
                target: None,
                output_format: CliOutputFormat::Json,
            }
        );
        // #146: `config` and `diff` must parse as standalone CLI actions,
        // not fall through to the "is a slash command" error. Both are
        // pure-local read-only introspection.
        assert_eq!(
            parse_args(&["config".to_string()]).expect("config should parse"),
            CliAction::Config {
                section: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["config".to_string(), "env".to_string()])
                .expect("config env should parse"),
            CliAction::Config {
                section: Some("env".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "config".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ])
            .expect("config --output-format json should parse"),
            CliAction::Config {
                section: None,
                output_format: CliOutputFormat::Json,
            }
        );
        assert_eq!(
            parse_args(&["diff".to_string()]).expect("diff should parse"),
            CliAction::Diff {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "diff".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ])
            .expect("diff --output-format json should parse"),
            CliAction::Diff {
                output_format: CliOutputFormat::Json,
            }
        );
        // #147: empty / whitespace-only positional args must be rejected
        // with a specific error instead of falling through to the prompt
        // path (where they surface a misleading "missing Anthropic
        // credentials" error or burn API tokens on an empty prompt).
        let empty_err =
            parse_args(&["".to_string()]).expect_err("empty positional arg should be rejected");
        assert!(
            empty_err.starts_with("empty prompt:"),
            "empty-arg error should be specific, got: {empty_err}"
        );
        let whitespace_err = parse_args(&["   ".to_string()])
            .expect_err("whitespace-only positional arg should be rejected");
        assert!(
            whitespace_err.starts_with("empty prompt:"),
            "whitespace-only error should be specific, got: {whitespace_err}"
        );
        let multi_empty_err = parse_args(&["".to_string(), "".to_string()])
            .expect_err("multiple empty positional args should be rejected");
        assert!(
            multi_empty_err.starts_with("empty prompt:"),
            "multi-empty error should be specific, got: {multi_empty_err}"
        );
        // Typo guard from #108 must still take precedence for non-empty
        // single-word non-prompt-looking inputs.
        let typo_err = parse_args(&["sttaus".to_string()])
            .expect_err("typo'd subcommand should be caught by #108 guard");
        assert!(
            typo_err.starts_with("unknown subcommand:"),
            "typo guard should fire for 'sttaus', got: {typo_err}"
        );
        // #148: `--model` flag must be captured as model_flag_raw so status
        // JSON can report provenance (source: flag, raw: <user-input>).
        match parse_args(&[
            "--model".to_string(),
            "sonnet".to_string(),
            "status".to_string(),
        ])
        .expect("--model sonnet status should parse")
        {
            CliAction::Status {
                model,
                model_flag_raw,
                ..
            } => {
                assert_eq!(model, "claude-sonnet-4-6", "sonnet alias should resolve");
                assert_eq!(
                    model_flag_raw.as_deref(),
                    Some("sonnet"),
                    "raw flag input should be preserved"
                );
            }
            other => panic!("expected CliAction::Status, got: {other:?}"),
        }
        // --model= form should also capture raw.
        match parse_args(&[
            "--model=anthropic/claude-opus-4-6".to_string(),
            "status".to_string(),
        ])
        .expect("--model=... status should parse")
        {
            CliAction::Status {
                model,
                model_flag_raw,
                ..
            } => {
                assert_eq!(model, "anthropic/claude-opus-4-6");
                assert_eq!(
                    model_flag_raw.as_deref(),
                    Some("anthropic/claude-opus-4-6"),
                    "--model= form should also preserve raw input"
                );
            }
            other => panic!("expected CliAction::Status, got: {other:?}"),
        }
    }

    #[test]
    fn dump_manifests_subcommand_accepts_explicit_manifest_dir() {
        assert_eq!(
            parse_args(&[
                "dump-manifests".to_string(),
                "--manifests-dir".to_string(),
                "/tmp/upstream".to_string(),
            ])
            .expect("dump-manifests should parse"),
            CliAction::DumpManifests {
                output_format: CliOutputFormat::Text,
                manifests_dir: Some(PathBuf::from("/tmp/upstream")),
            }
        );
        assert_eq!(
            parse_args(&[
                "dump-manifests".to_string(),
                "--manifests-dir=/tmp/upstream".to_string()
            ])
            .expect("inline dump-manifests flag should parse"),
            CliAction::DumpManifests {
                output_format: CliOutputFormat::Text,
                manifests_dir: Some(PathBuf::from("/tmp/upstream")),
            }
        );
    }

    #[test]
    fn parses_acp_command_surfaces() {
        assert_eq!(
            parse_args(&["acp".to_string()]).expect("acp should parse"),
            CliAction::Acp {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["acp".to_string(), "serve".to_string()]).expect("acp serve should parse"),
            CliAction::Acp {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["--acp".to_string()]).expect("--acp should parse"),
            CliAction::Acp {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["-acp".to_string()]).expect("-acp should parse"),
            CliAction::Acp {
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn local_command_help_flags_stay_on_the_local_parser_path() {
        assert_eq!(
            parse_args(&["status".to_string(), "--help".to_string()])
                .expect("status help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Status)
        );
        assert_eq!(
            parse_args(&["sandbox".to_string(), "-h".to_string()])
                .expect("sandbox help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Sandbox)
        );
        assert_eq!(
            parse_args(&["doctor".to_string(), "--help".to_string()])
                .expect("doctor help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Doctor)
        );
        assert_eq!(
            parse_args(&["acp".to_string(), "--help".to_string()]).expect("acp help should parse"),
            CliAction::HelpTopic(LocalHelpTopic::Acp)
        );
    }

    #[test]
    fn subcommand_help_flag_has_one_contract_across_all_subcommands_141() {
        // #141: every documented subcommand must resolve `<subcommand> --help`
        // to a subcommand-specific help topic, never to global help, never to
        // an "unknown option" error, never to the subcommand's primary output.
        let cases: &[(&str, LocalHelpTopic)] = &[
            ("status", LocalHelpTopic::Status),
            ("sandbox", LocalHelpTopic::Sandbox),
            ("doctor", LocalHelpTopic::Doctor),
            ("acp", LocalHelpTopic::Acp),
            ("init", LocalHelpTopic::Init),
            ("state", LocalHelpTopic::State),
            ("export", LocalHelpTopic::Export),
            ("version", LocalHelpTopic::Version),
            ("system-prompt", LocalHelpTopic::SystemPrompt),
            ("dump-manifests", LocalHelpTopic::DumpManifests),
            ("bootstrap-plan", LocalHelpTopic::BootstrapPlan),
        ];
        for (subcommand, expected_topic) in cases {
            for flag in ["--help", "-h"] {
                let parsed = parse_args(&[subcommand.to_string(), flag.to_string()])
                    .unwrap_or_else(|error| {
                        panic!("`{subcommand} {flag}` should parse as help but errored: {error}")
                    });
                assert_eq!(
                    parsed,
                    CliAction::HelpTopic(*expected_topic),
                    "`{subcommand} {flag}` should resolve to HelpTopic({expected_topic:?})"
                );
            }
            // And the rendered help must actually mention the subcommand name
            // (or its canonical title) so users know they got the right help.
            let rendered = render_help_topic(*expected_topic);
            assert!(
                !rendered.is_empty(),
                "{subcommand} help text should not be empty"
            );
            assert!(
                rendered.contains("Usage"),
                "{subcommand} help text should contain a Usage line"
            );
        }
    }

    #[test]
    fn status_degrades_gracefully_on_malformed_mcp_config_143() {
        // #143: previously `claw status` hard-failed on any config parse error,
        // taking down the entire health surface for one malformed MCP entry.
        // `claw doctor` already degrades gracefully; this test locks `status`
        // to the same contract.
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project-with-malformed-mcp");
        std::fs::create_dir_all(&cwd).expect("project dir should exist");
        // One valid server + one malformed entry missing `command`.
        std::fs::write(
            cwd.join(".claw.json"),
            r#"{
  "mcpServers": {
    "everything": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-everything"]},
    "missing-command": {"args": ["arg-only-no-command"]}
  }
}
"#,
        )
        .expect("write malformed .claw.json");

        let context = with_current_dir(&cwd, || {
            super::status_context(None)
                .expect("status_context should not hard-fail on config parse errors (#143)")
        });

        // Phase 1 contract: config_load_error is populated with the parse error.
        let err = context
            .config_load_error
            .as_ref()
            .expect("config_load_error should be Some when config parse fails");
        assert!(
            err.contains("mcpServers.missing-command"),
            "config_load_error should name the malformed field path: {err}"
        );
        assert!(
            err.contains("missing string field command"),
            "config_load_error should carry the underlying parse error: {err}"
        );

        // Phase 1 contract: workspace/git/sandbox fields are still populated
        // (independent of config parse). Sandbox falls back to defaults.
        assert_eq!(context.cwd, cwd.canonicalize().unwrap_or(cwd.clone()));
        assert_eq!(
            context.loaded_config_files, 0,
            "loaded_config_files should be 0 when config parse fails"
        );
        assert!(
            context.discovered_config_files > 0,
            "discovered_config_files should still count the file that failed to parse"
        );

        // JSON output contract: top-level `status: "degraded"` + config_load_error field.
        let usage = super::StatusUsage {
            message_count: 0,
            turns: 0,
            latest: runtime::TokenUsage::default(),
            cumulative: runtime::TokenUsage::default(),
            estimated_tokens: 0,
        };
        let json =
            super::status_json_value(Some("test-model"), usage, "workspace-write", &context, None);
        assert_eq!(
            json.get("status").and_then(|v| v.as_str()),
            Some("degraded"),
            "top-level status marker should be 'degraded' when config parse failed: {json}"
        );
        assert!(
            json.get("config_load_error")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("mcpServers.missing-command")),
            "config_load_error should surface in JSON output: {json}"
        );
        // Independent fields still populated.
        assert_eq!(
            json.get("model").and_then(|v| v.as_str()),
            Some("test-model")
        );
        assert!(
            json.get("workspace").is_some(),
            "workspace field still reported"
        );
        assert!(
            json.get("sandbox").is_some(),
            "sandbox field still reported"
        );

        // Clean path: no config error → status: "ok", config_load_error: null.
        let clean_cwd = root.join("project-with-clean-config");
        std::fs::create_dir_all(&clean_cwd).expect("clean project dir");
        let clean_context = with_current_dir(&clean_cwd, || {
            super::status_context(None).expect("clean status_context should succeed")
        });
        assert!(clean_context.config_load_error.is_none());
        let clean_json = super::status_json_value(
            Some("test-model"),
            usage,
            "workspace-write",
            &clean_context,
            None,
        );
        assert_eq!(
            clean_json.get("status").and_then(|v| v.as_str()),
            Some("ok"),
            "clean run should report status: 'ok'"
        );
    }

    #[test]
    fn state_error_surfaces_actionable_worker_commands_139() {
        // #139: the error for missing `.claw/worker-state.json` must name
        // the concrete commands that produce worker state, otherwise claws
        // have no discoverable path from the error to a fix.
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project-with-no-state");
        std::fs::create_dir_all(&cwd).expect("project dir should exist");

        let error = with_current_dir(&cwd, || {
            super::run_worker_state(CliOutputFormat::Text).expect_err("missing state should error")
        });
        let message = error.to_string();

        // Keep the original locator so scripts grepping for it still work.
        assert!(
            message.contains("no worker state file found at"),
            "error should keep the canonical prefix: {message}"
        );
        // New actionable hints — this is what #139 is fixing.
        assert!(
            message.contains("claw prompt"),
            "error should name `claw prompt <text>` as a producer: {message}"
        );
        assert!(
            message.contains("REPL"),
            "error should mention the interactive REPL as a producer: {message}"
        );
        assert!(
            message.contains("claw state"),
            "error should tell the user what to rerun once state exists: {message}"
        );
        // And the State --help topic must document the worker relationship
        // so claws can discover the contract without hitting the error first.
        let state_help = render_help_topic(LocalHelpTopic::State);
        assert!(
            state_help.contains("Produces state"),
            "state help must document how state is produced: {state_help}"
        );
        assert!(
            state_help.contains("claw prompt"),
            "state help must name `claw prompt <text>` as a producer: {state_help}"
        );
    }

    #[test]
    fn parses_single_word_command_aliases_without_falling_back_to_prompt_mode() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        assert_eq!(
            parse_args(&["help".to_string()]).expect("help should parse"),
            CliAction::Help {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["version".to_string()]).expect("version should parse"),
            CliAction::Version {
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["status".to_string()]).expect("status should parse"),
            CliAction::Status {
                model: DEFAULT_MODEL.to_string(),
                model_flag_raw: None, // #148: no --model flag passed
                permission_mode: PermissionMode::ReadOnly,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["sandbox".to_string()]).expect("sandbox should parse"),
            CliAction::Sandbox {
                output_format: CliOutputFormat::Text,
            }
        );
        // #152: `--json` on diagnostic verbs should hint the correct flag.
        let err = parse_args(&["doctor".to_string(), "--json".to_string()])
            .expect_err("`doctor --json` should fail with hint");
        assert!(
            err.contains("unrecognized argument `--json` for subcommand `doctor`"),
            "error should name the verb: {err}"
        );
        assert!(
            err.contains("Did you mean `--output-format json`?"),
            "error should hint the correct flag: {err}"
        );
        // Other unrecognized args should NOT trigger the --json hint.
        let err_other = parse_args(&["doctor".to_string(), "garbage".to_string()])
            .expect_err("`doctor garbage` should fail without --json hint");
        assert!(
            !err_other.contains("--output-format json"),
            "unrelated args should not trigger --json hint: {err_other}"
        );
        // #154: model syntax error should hint at provider prefix when applicable
        let err_gpt = parse_args(&[
            "prompt".to_string(),
            "test".to_string(),
            "--model".to_string(),
            "gpt-4".to_string(),
        ])
        .expect_err("`--model gpt-4` should fail with OpenAI hint");
        assert!(
            err_gpt.contains("Did you mean `openai/gpt-4`?"),
            "GPT model error should hint openai/ prefix: {err_gpt}"
        );
        assert!(
            err_gpt.contains("OPENAI_API_KEY"),
            "GPT model error should mention env var: {err_gpt}"
        );
        let err_qwen = parse_args(&[
            "prompt".to_string(),
            "test".to_string(),
            "--model".to_string(),
            "qwen-plus".to_string(),
        ])
        .expect_err("`--model qwen-plus` should fail with DashScope hint");
        assert!(
            err_qwen.contains("Did you mean `qwen/qwen-plus`?"),
            "Qwen model error should hint qwen/ prefix: {err_qwen}"
        );
        assert!(
            err_qwen.contains("DASHSCOPE_API_KEY"),
            "Qwen model error should mention env var: {err_qwen}"
        );
        // Unrelated invalid model should NOT get a hint
        let err_garbage = parse_args(&[
            "prompt".to_string(),
            "test".to_string(),
            "--model".to_string(),
            "asdfgh".to_string(),
        ])
        .expect_err("`--model asdfgh` should fail");
        assert!(
            !err_garbage.contains("Did you mean"),
            "Unrelated model errors should not get a hint: {err_garbage}"
        );
    }

    #[test]
    fn classify_error_kind_returns_correct_discriminants() {
        // #77: error kind classification for JSON error payloads
        assert_eq!(
            classify_error_kind("missing Anthropic credentials; export ..."),
            "missing_credentials"
        );
        assert_eq!(
            classify_error_kind("no worker state file found at /tmp/..."),
            "missing_worker_state"
        );
        assert_eq!(
            classify_error_kind("session not found: abc123"),
            "session_not_found"
        );
        assert_eq!(
            classify_error_kind("failed to restore session: no managed sessions found"),
            "session_load_failed"
        );
        assert_eq!(
            classify_error_kind("unrecognized argument `--foo` for subcommand `doctor`"),
            "cli_parse"
        );
        assert_eq!(
            classify_error_kind("invalid model syntax: 'gpt-4'. Expected ..."),
            "invalid_model_syntax"
        );
        assert_eq!(
            classify_error_kind("unsupported resumed command: /blargh"),
            "unsupported_resumed_command"
        );
        assert_eq!(
            classify_error_kind("api failed after 3 attempts: ..."),
            "api_http_error"
        );
        assert_eq!(
            classify_error_kind("something completely unknown"),
            "unknown"
        );
    }

    #[test]
    fn split_error_hint_separates_reason_from_runbook() {
        // #77: short reason / hint separation for JSON error payloads
        let (short, hint) = split_error_hint("missing credentials\nHint: export ANTHROPIC_API_KEY");
        assert_eq!(short, "missing credentials");
        assert_eq!(hint, Some("Hint: export ANTHROPIC_API_KEY".to_string()));

        let (short, hint) = split_error_hint("simple error with no hint");
        assert_eq!(short, "simple error with no hint");
        assert_eq!(hint, None);
    }

    #[test]
    fn parses_bare_export_subcommand_targeting_latest_session() {
        // given
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        let args = vec!["export".to_string()];

        // when
        let parsed = parse_args(&args).expect("bare export should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: None,
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_positional_output_path() {
        // given
        let args = vec!["export".to_string(), "conversation.md".to_string()];

        // when
        let parsed = parse_args(&args).expect("export with path should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: Some(PathBuf::from("conversation.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_session_and_output_flags() {
        // given
        let args = vec![
            "export".to_string(),
            "--session".to_string(),
            "session-alpha".to_string(),
            "--output".to_string(),
            "/tmp/share.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("export flags should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: "session-alpha".to_string(),
                output_path: Some(PathBuf::from("/tmp/share.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_inline_flag_values() {
        // given
        let args = vec![
            "export".to_string(),
            "--session=session-beta".to_string(),
            "--output=/tmp/beta.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("export inline flags should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: "session-beta".to_string(),
                output_path: Some(PathBuf::from("/tmp/beta.md")),
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_export_subcommand_with_json_output_format() {
        // given
        let args = vec![
            "--output-format=json".to_string(),
            "export".to_string(),
            "/tmp/notes.md".to_string(),
        ];

        // when
        let parsed = parse_args(&args).expect("json export should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: Some(PathBuf::from("/tmp/notes.md")),
                output_format: CliOutputFormat::Json,
            }
        );
    }

    #[test]
    fn rejects_unknown_export_options_with_helpful_message() {
        // given
        let args = vec!["export".to_string(), "--bogus".to_string()];

        // when
        let error = parse_args(&args).expect_err("unknown export option should fail");

        // then
        assert!(error.contains("unknown export option: --bogus"));
    }

    #[test]
    fn rejects_export_with_extra_positional_after_path() {
        // given
        let args = vec![
            "export".to_string(),
            "first.md".to_string(),
            "second.md".to_string(),
        ];

        // when
        let error = parse_args(&args).expect_err("multiple positionals should fail");

        // then
        assert!(error.contains("unexpected export argument: second.md"));
    }

    #[test]
    fn parse_export_args_helper_defaults_to_latest_reference_and_no_output() {
        // given
        let args: Vec<String> = vec![];

        // when
        let parsed = parse_export_args(&args, CliOutputFormat::Text)
            .expect("empty export args should parse");

        // then
        assert_eq!(
            parsed,
            CliAction::Export {
                session_reference: LATEST_SESSION_REFERENCE.to_string(),
                output_path: None,
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn render_session_markdown_includes_header_and_summarized_tool_calls() {
        // given
        let mut session = Session::new();
        session.session_id = "session-export-test".to_string();
        session.messages = vec![
            ConversationMessage::user_text("How do I list files?"),
            ConversationMessage::assistant(vec![
                ContentBlock::Text {
                    text: "I'll run a tool.".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "toolu_abcdefghijklmnop".to_string(),
                    name: "bash".to_string(),
                    input: r#"{"command":"ls -la"}"#.to_string(),
                },
            ]),
            ConversationMessage {
                role: MessageRole::Tool,
                blocks: vec![ContentBlock::ToolResult {
                    tool_use_id: "toolu_abcdefghijklmnop".to_string(),
                    tool_name: "bash".to_string(),
                    output: "total 8\ndrwxr-xr-x  2 user staff   64 Apr  7 12:00 .".to_string(),
                    is_error: false,
                }],
                usage: None,
            },
        ];

        // when
        let markdown = render_session_markdown(
            &session,
            "session-export-test",
            std::path::Path::new("/tmp/sessions/session-export-test.jsonl"),
        );

        // then
        assert!(markdown.starts_with("# Conversation Export"));
        assert!(markdown.contains("- **Session**: `session-export-test`"));
        assert!(markdown.contains("- **Messages**: 3"));
        assert!(markdown.contains("## 1. User"));
        assert!(markdown.contains("How do I list files?"));
        assert!(markdown.contains("## 2. Assistant"));
        assert!(markdown.contains("**Tool call** `bash`"));
        assert!(markdown.contains("toolu_abcdef…"));
        assert!(markdown.contains("ls -la"));
        assert!(markdown.contains("## 3. Tool"));
        assert!(markdown.contains("**Tool result** `bash`"));
        assert!(markdown.contains("ok"));
        assert!(markdown.contains("total 8"));
    }

    #[test]
    fn render_session_markdown_marks_tool_errors_and_skips_empty_summaries() {
        // given
        let mut session = Session::new();
        session.session_id = "errs".to_string();
        session.messages = vec![ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: "short".to_string(),
                tool_name: "read_file".to_string(),
                output: "   ".to_string(),
                is_error: true,
            }],
            usage: None,
        }];

        // when
        let markdown =
            render_session_markdown(&session, "errs", std::path::Path::new("errs.jsonl"));

        // then
        assert!(markdown.contains("**Tool result** `read_file` _(id `short`, error)_"));
        // an empty summary should not produce a stray blockquote line
        assert!(!markdown.contains("> \n"));
    }

    #[test]
    fn summarize_tool_payload_for_markdown_compacts_json_and_truncates_overflow() {
        // given
        let json_payload = r#"{
            "command":   "ls -la",
            "cwd": "/tmp"
        }"#;
        let long_payload = "a".repeat(600);

        // when
        let compacted = summarize_tool_payload_for_markdown(json_payload);
        let truncated = summarize_tool_payload_for_markdown(&long_payload);

        // then
        assert_eq!(compacted, r#"{"command":"ls -la","cwd":"/tmp"}"#);
        assert!(truncated.ends_with('…'));
        assert!(truncated.chars().count() <= 281);
    }

    #[test]
    fn short_tool_id_truncates_long_identifiers_with_ellipsis() {
        // given
        let long = "toolu_01ABCDEFGHIJKLMN";
        let short = "tool_1";

        // when
        let trimmed_long = short_tool_id(long);
        let trimmed_short = short_tool_id(short);

        // then
        assert_eq!(trimmed_long, "toolu_01ABCD…");
        assert_eq!(trimmed_short, "tool_1");
    }

    #[test]
    fn parses_json_output_for_mcp_and_skills_commands() {
        assert_eq!(
            parse_args(&["--output-format=json".to_string(), "mcp".to_string()])
                .expect("json mcp should parse"),
            CliAction::Mcp {
                args: None,
                output_format: CliOutputFormat::Json,
            }
        );
        assert_eq!(
            parse_args(&[
                "--output-format=json".to_string(),
                "/skills".to_string(),
                "help".to_string(),
            ])
            .expect("json /skills help should parse"),
            CliAction::Skills {
                args: Some("help".to_string()),
                output_format: CliOutputFormat::Json,
            }
        );
    }

    #[test]
    fn single_word_slash_command_names_return_guidance_instead_of_hitting_prompt_mode() {
        let error = parse_args(&["cost".to_string()]).expect_err("cost should return guidance");
        assert!(error.contains("slash command"));
        assert!(error.contains("/cost"));
    }

    #[test]
    fn multi_word_prompt_still_uses_shorthand_prompt_mode() {
        let _guard = env_lock();
        std::env::remove_var("RUSTY_CLAUDE_PERMISSION_MODE");
        // Input is ["--model", "opus", "please", "debug", "this"] so the joined
        // prompt shorthand must stay a normal multi-word prompt while still
        // honoring alias validation at parse time.
        assert_eq!(
            parse_args(&[
                "--model".to_string(),
                "opus".to_string(),
                "please".to_string(),
                "debug".to_string(),
                "this".to_string(),
            ])
            .expect("prompt shorthand should still work"),
            CliAction::Prompt {
                prompt: "please debug this".to_string(),
                model: "claude-opus-4-6".to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn parses_direct_agents_mcp_and_skills_slash_commands() {
        assert_eq!(
            parse_args(&["/agents".to_string()]).expect("/agents should parse"),
            CliAction::Agents {
                args: None,
                output_format: CliOutputFormat::Text
            }
        );
        assert_eq!(
            parse_args(&["/mcp".to_string(), "show".to_string(), "demo".to_string()])
                .expect("/mcp show demo should parse"),
            CliAction::Mcp {
                args: Some("show demo".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string()]).expect("/skills should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skill".to_string()]).expect("/skill should parse"),
            CliAction::Skills {
                args: None,
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string(), "help".to_string()])
                .expect("/skills help should parse"),
            CliAction::Skills {
                args: Some("help".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skill".to_string(), "list".to_string()])
                .expect("/skill list should parse"),
            CliAction::Skills {
                args: Some("list".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&[
                "/skills".to_string(),
                "help".to_string(),
                "overview".to_string()
            ])
            .expect("/skills help overview should invoke"),
            CliAction::Prompt {
                prompt: "$help overview".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        assert_eq!(
            parse_args(&[
                "/skills".to_string(),
                "install".to_string(),
                "./fixtures/help-skill".to_string(),
            ])
            .expect("/skills install should parse"),
            CliAction::Skills {
                args: Some("install ./fixtures/help-skill".to_string()),
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["/skills".to_string(), "/test".to_string()])
                .expect("/skills /test should normalize to a single skill prompt prefix"),
            CliAction::Prompt {
                prompt: "$test".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
        let error = parse_args(&["/status".to_string()])
            .expect_err("/status should remain REPL-only when invoked directly");
        assert!(error.contains("interactive-only"));
        assert!(error.contains("claw --resume SESSION.jsonl /status"));
    }

    #[test]
    fn direct_slash_commands_surface_shared_validation_errors() {
        let compact_error = parse_args(&["/compact".to_string(), "now".to_string()])
            .expect_err("invalid /compact shape should be rejected");
        assert!(compact_error.contains("Unexpected arguments for /compact."));
        assert!(compact_error.contains("Usage            /compact"));

        let plugins_error = parse_args(&[
            "/plugins".to_string(),
            "list".to_string(),
            "extra".to_string(),
        ])
        .expect_err("invalid /plugins list shape should be rejected");
        assert!(plugins_error.contains("Usage: /plugin list"));
        assert!(plugins_error.contains("Aliases          /plugins, /marketplace"));
    }

    #[test]
    fn formats_unknown_slash_command_with_suggestions() {
        let report = format_unknown_slash_command_message("statsu");
        assert!(report.contains("unknown slash command: /statsu"));
        assert!(report.contains("Did you mean"));
        assert!(report.contains("Use /help"));
    }

    #[test]
    fn typoed_doctor_subcommand_returns_did_you_mean_error() {
        let error = parse_args(&["doctorr".to_string()]).expect_err("doctorr should error");
        assert!(error.contains("unknown subcommand: doctorr."));
        assert!(error.contains("Did you mean"));
        assert!(error.contains("doctor"));
    }

    #[test]
    fn typoed_skills_subcommand_returns_did_you_mean_error() {
        let error = parse_args(&["skilsl".to_string()]).expect_err("skilsl should error");
        assert!(error.contains("unknown subcommand: skilsl."));
        assert!(error.contains("skills"));
    }

    #[test]
    fn typoed_status_subcommand_returns_did_you_mean_error() {
        let error = parse_args(&["statuss".to_string()]).expect_err("statuss should error");
        assert!(error.contains("unknown subcommand: statuss."));
        assert!(error.contains("status"));
    }

    #[test]
    fn typoed_export_subcommand_returns_did_you_mean_error() {
        let error = parse_args(&["exporrt".to_string()]).expect_err("exporrt should error");
        assert!(error.contains("unknown subcommand: exporrt."));
        assert!(error.contains("Did you mean"));
        assert!(error.contains("export"));
    }

    #[test]
    fn typoed_mcp_subcommand_returns_did_you_mean_error() {
        let error = parse_args(&["mcpp".to_string()]).expect_err("mcpp should error");
        assert!(error.contains("unknown subcommand: mcpp."));
        assert!(error.contains("mcp"));
    }

    #[test]
    fn multi_word_prompt_still_bypasses_subcommand_typo_guard() {
        assert_eq!(
            parse_args(&[
                "hello".to_string(),
                "world".to_string(),
                "this".to_string(),
                "is".to_string(),
                "a".to_string(),
                "prompt".to_string(),
            ])
            .expect("multi-word prompt should still parse"),
            CliAction::Prompt {
                prompt: "hello world this is a prompt".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: crate::default_permission_mode(),
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn prompt_subcommand_allows_literal_typo_word() {
        assert_eq!(
            parse_args(&["prompt".to_string(), "doctorr".to_string()])
                .expect("explicit prompt subcommand should allow literal typo word"),
            CliAction::Prompt {
                prompt: "doctorr".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn punctuation_bearing_single_token_still_dispatches_to_prompt() {
        // #140: Guard against test pollution — isolate cwd + env so this test
        // doesn't pick up a stale .claw/settings.json from other tests that
        // may have set `permissionMode: acceptEdits` in a shared cwd.
        let _guard = env_lock();
        let root = temp_dir();
        let cwd = root.join("project");
        std::fs::create_dir_all(&cwd).expect("project dir should exist");
        let result = with_current_dir(&cwd, || {
            parse_args(&["PARITY_SCENARIO:bash_permission_prompt_approved".to_string()])
                .expect("scenario token should still dispatch to prompt")
        });
        assert_eq!(
            result,
            CliAction::Prompt {
                prompt: "PARITY_SCENARIO:bash_permission_prompt_approved".to_string(),
                model: DEFAULT_MODEL.to_string(),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
                compact: false,
                base_commit: None,
                reasoning_effort: None,
                allow_broad_cwd: false,
            }
        );
    }

    #[test]
    fn formats_namespaced_omc_slash_command_with_contract_guidance() {
        let report = format_unknown_slash_command_message("oh-my-claudecode:hud");
        assert!(report.contains("unknown slash command: /oh-my-claudecode:hud"));
        assert!(report.contains("Claude Code/OMC plugin command"));
        assert!(report.contains("plugin slash commands"));
        assert!(report.contains("statusline"));
        assert!(report.contains("session hooks"));
    }

    #[test]
    fn parses_resume_flag_with_slash_command() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/compact".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec!["/compact".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_without_path_as_latest_session() {
        assert_eq!(
            parse_args(&["--resume".to_string()]).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("latest"),
                commands: vec![],
                output_format: CliOutputFormat::Text,
            }
        );
        assert_eq!(
            parse_args(&["--resume".to_string(), "/status".to_string()])
                .expect("resume shortcut should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("latest"),
                commands: vec!["/status".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_multiple_slash_commands() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/status".to_string(),
            "/compact".to_string(),
            "/cost".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec![
                    "/status".to_string(),
                    "/compact".to_string(),
                    "/cost".to_string(),
                ],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn rejects_unknown_options_with_helpful_guidance() {
        let error = parse_args(&["--resum".to_string()]).expect_err("unknown option should fail");
        assert!(error.contains("unknown option: --resum"));
        assert!(error.contains("Did you mean --resume?"));
        assert!(error.contains("claw --help"));
    }

    #[test]
    fn parses_resume_flag_with_slash_command_arguments() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/export".to_string(),
            "notes.txt".to_string(),
            "/clear".to_string(),
            "--confirm".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec![
                    "/export notes.txt".to_string(),
                    "/clear --confirm".to_string(),
                ],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_absolute_export_path() {
        let args = vec![
            "--resume".to_string(),
            "session.jsonl".to_string(),
            "/export".to_string(),
            "/tmp/notes.txt".to_string(),
            "/status".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.jsonl"),
                commands: vec!["/export /tmp/notes.txt".to_string(), "/status".to_string()],
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn filtered_tool_specs_respect_allowlist() {
        let allowed = ["read_file", "grep_search"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let filtered = filter_tool_specs(&GlobalToolRegistry::builtin(), Some(&allowed));
        let names = filtered
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["read_file", "grep_search"]);
    }

    #[test]
    fn filtered_tool_specs_include_plugin_tools() {
        let filtered = filter_tool_specs(&registry_with_plugin_tool(), None);
        let names = filtered
            .into_iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"plugin_echo".to_string()));
    }

    #[test]
    fn permission_policy_uses_plugin_tool_permissions() {
        let feature_config = runtime::RuntimeFeatureConfig::default();
        let policy = permission_policy(
            PermissionMode::ReadOnly,
            &feature_config,
            &registry_with_plugin_tool(),
        )
        .expect("permission policy should build");
        let required = policy.required_mode_for("plugin_echo");
        assert_eq!(required, PermissionMode::WorkspaceWrite);
    }

    #[test]
    fn shared_help_uses_resume_annotation_copy() {
        let help = commands::render_slash_command_help();
        assert!(help.contains("Slash commands"));
        assert!(help.contains("works with --resume SESSION.jsonl"));
    }

    #[test]
    fn bare_skill_dispatch_resolves_known_project_skill_to_prompt() {
        let _guard = env_lock();
        let workspace = temp_dir();
        write_skill_fixture(
            &workspace.join(".codex").join("skills"),
            "caveman",
            "Project skill fixture",
        );

        let prompt = try_resolve_bare_skill_prompt(&workspace, "caveman sharpen club")
            .expect("known bare skill should dispatch");
        assert_eq!(prompt, "$caveman sharpen club");

        fs::remove_dir_all(workspace).expect("workspace should clean up");
    }

    #[test]
    fn bare_skill_dispatch_ignores_unknown_or_non_skill_input() {
        let _guard = env_lock();
        let workspace = temp_dir();
        fs::create_dir_all(&workspace).expect("workspace should exist");

        assert_eq!(
            try_resolve_bare_skill_prompt(&workspace, "not-a-known-skill do thing"),
            None
        );
        assert_eq!(try_resolve_bare_skill_prompt(&workspace, "/status"), None);

        fs::remove_dir_all(workspace).expect("workspace should clean up");
    }

    #[test]
    fn repl_help_includes_shared_commands_and_exit() {
        let help = render_repl_help();
        assert!(help.contains("REPL"));
        assert!(help.contains("/help"));
        assert!(help.contains("Complete commands, modes, and recent sessions"));
        assert!(help.contains("/status"));
        assert!(help.contains("/sandbox"));
        assert!(help.contains("/model [model]"));
        assert!(help.contains("/permissions [read-only|workspace-write|danger-full-access]"));
        assert!(help.contains("/clear [--confirm]"));
        assert!(help.contains("/cost"));
        assert!(help.contains("/resume <session-path>"));
        assert!(help.contains("/config [env|hooks|model|plugins]"));
        assert!(help.contains("/mcp [list|show <server>|help]"));
        assert!(help.contains("/memory"));
        assert!(help.contains("/init"));
        assert!(help.contains("/diff"));
        assert!(help.contains("/version"));
        assert!(help.contains("/export [file]"));
        // Batch 5 added `/session delete`; match on the stable core rather than
        // the trailing bracket so future additions don't re-break this.
        assert!(help.contains("/session [list|switch <session-id>|fork [branch-name]"));
        assert!(help.contains(
            "/plugin [list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]"
        ));
        assert!(help.contains("aliases: /plugins, /marketplace"));
        assert!(help.contains("/agents"));
        assert!(help.contains("/skills"));
        assert!(help.contains("/exit"));
        assert!(help.contains("Auto-save            .claw/sessions/<session-id>.jsonl"));
        assert!(help.contains("Resume latest        /resume latest"));
    }

    #[test]
    fn completion_candidates_include_workflow_shortcuts_and_dynamic_sessions() {
        let completions = slash_command_completion_candidates_with_sessions(
            "sonnet",
            Some("session-current"),
            vec!["session-old".to_string()],
        );

        assert!(completions.contains(&"/model claude-sonnet-4-6".to_string()));
        assert!(completions.contains(&"/permissions workspace-write".to_string()));
        assert!(completions.contains(&"/session list".to_string()));
        assert!(completions.contains(&"/session switch session-current".to_string()));
        assert!(completions.contains(&"/resume session-old".to_string()));
        assert!(completions.contains(&"/mcp list".to_string()));
        assert!(completions.contains(&"/ultraplan ".to_string()));
    }

    #[test]
    fn startup_banner_mentions_workflow_completions() {
        let _guard = env_lock();
        // Inject dummy credentials so LiveCli can construct without real Anthropic key
        std::env::set_var("ANTHROPIC_API_KEY", "test-dummy-key-for-banner-test");
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");

        let banner = with_current_dir(&root, || {
            LiveCli::new(
                "claude-sonnet-4-6".to_string(),
                true,
                None,
                PermissionMode::DangerFullAccess,
            )
            .expect("cli should initialize")
            .startup_banner()
        });

        assert!(banner.contains("Tab"));
        assert!(banner.contains("workflow completions"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn format_connected_line_renders_anthropic_provider_for_claude_model() {
        let model = "claude-sonnet-4-6";

        let line = format_connected_line(model);

        assert_eq!(line, "Connected: claude-sonnet-4-6 via anthropic");
    }

    #[test]
    fn format_connected_line_renders_xai_provider_for_grok_model() {
        let model = "grok-3";

        let line = format_connected_line(model);

        assert_eq!(line, "Connected: grok-3 via xai");
    }

    #[test]
    fn resolve_repl_model_returns_user_supplied_model_unchanged_when_explicit() {
        let user_model = "claude-sonnet-4-6".to_string();

        let resolved = resolve_repl_model(user_model);

        assert_eq!(resolved, "claude-sonnet-4-6");
    }

    #[test]
    fn resolve_repl_model_falls_back_to_anthropic_model_env_when_default() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        let config_home = root.join("config");
        fs::create_dir_all(&config_home).expect("config home dir");
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_MODEL");
        std::env::set_var("ANTHROPIC_MODEL", "sonnet");

        let resolved = with_current_dir(&root, || resolve_repl_model(DEFAULT_MODEL.to_string()));

        assert_eq!(resolved, "claude-sonnet-4-6");

        std::env::remove_var("ANTHROPIC_MODEL");
        std::env::remove_var("CLAW_CONFIG_HOME");
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn resolve_repl_model_returns_default_when_env_unset_and_no_config() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        let config_home = root.join("config");
        fs::create_dir_all(&config_home).expect("config home dir");
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_MODEL");

        let resolved = with_current_dir(&root, || resolve_repl_model(DEFAULT_MODEL.to_string()));

        assert_eq!(resolved, DEFAULT_MODEL);

        std::env::remove_var("CLAW_CONFIG_HOME");
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn resume_supported_command_list_matches_expected_surface() {
        let names = resume_supported_slash_commands()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        // Now with 135+ slash commands, verify minimum resume support
        assert!(
            names.len() >= 39,
            "expected at least 39 resume-supported commands, got {}",
            names.len()
        );
        // Verify key resume commands still exist
        assert!(names.contains(&"help"));
        assert!(names.contains(&"status"));
        assert!(names.contains(&"compact"));
    }

    #[test]
    fn resume_report_uses_sectioned_layout() {
        let report = format_resume_report("session.jsonl", 14, 6);
        assert!(report.contains("Session resumed"));
        assert!(report.contains("Session file     session.jsonl"));
        assert!(report.contains("Messages         14"));
        assert!(report.contains("Turns            6"));
    }

    #[test]
    fn compact_report_uses_structured_output() {
        let compacted = format_compact_report(8, 5, false);
        assert!(compacted.contains("Compact"));
        assert!(compacted.contains("Result           compacted"));
        assert!(compacted.contains("Messages removed 8"));
        let skipped = format_compact_report(0, 3, true);
        assert!(skipped.contains("Result           skipped"));
    }

    #[test]
    fn cost_report_uses_sectioned_layout() {
        let report = format_cost_report(runtime::TokenUsage {
            input_tokens: 20,
            output_tokens: 8,
            cache_creation_input_tokens: 3,
            cache_read_input_tokens: 1,
        });
        assert!(report.contains("Cost"));
        assert!(report.contains("Input tokens     20"));
        assert!(report.contains("Output tokens    8"));
        assert!(report.contains("Cache create     3"));
        assert!(report.contains("Cache read       1"));
        assert!(report.contains("Total tokens     32"));
    }

    #[test]
    fn permissions_report_uses_sectioned_layout() {
        let report = format_permissions_report("workspace-write");
        assert!(report.contains("Permissions"));
        assert!(report.contains("Active mode      workspace-write"));
        assert!(report.contains("Modes"));
        assert!(report.contains("read-only          ○ available Read/search tools only"));
        assert!(report.contains("workspace-write    ● current   Edit files inside the workspace"));
        assert!(report.contains("danger-full-access ○ available Unrestricted tool access"));
    }

    #[test]
    fn permissions_switch_report_is_structured() {
        let report = format_permissions_switch_report("read-only", "workspace-write");
        assert!(report.contains("Permissions updated"));
        assert!(report.contains("Result           mode switched"));
        assert!(report.contains("Previous mode    read-only"));
        assert!(report.contains("Active mode      workspace-write"));
        assert!(report.contains("Applies to       subsequent tool calls"));
    }

    #[test]
    fn init_help_mentions_direct_subcommand() {
        let mut help = Vec::new();
        print_help_to(&mut help).expect("help should render");
        let help = String::from_utf8(help).expect("help should be utf8");
        assert!(help.contains("claw help"));
        assert!(help.contains("claw version"));
        assert!(help.contains("claw status"));
        assert!(help.contains("claw sandbox"));
        assert!(help.contains("claw init"));
        assert!(help.contains("claw acp [serve]"));
        assert!(help.contains("claw agents"));
        assert!(help.contains("claw mcp"));
        assert!(help.contains("claw skills"));
        assert!(help.contains("claw /skills"));
        assert!(help.contains("ultraworkers/claw-code"));
        assert!(help.contains("cargo install claw-code"));
        assert!(!help.contains("claw login"));
        assert!(!help.contains("claw logout"));
    }

    #[test]
    fn model_report_uses_sectioned_layout() {
        let report = format_model_report("claude-sonnet", 12, 4);
        assert!(report.contains("Model"));
        assert!(report.contains("Current model    claude-sonnet"));
        assert!(report.contains("Session messages 12"));
        assert!(report.contains("Switch models with /model <name>"));
    }

    #[test]
    fn model_switch_report_preserves_context_summary() {
        let report = format_model_switch_report("claude-sonnet", "claude-opus", 9);
        assert!(report.contains("Model updated"));
        assert!(report.contains("Previous         claude-sonnet"));
        assert!(report.contains("Current          claude-opus"));
        assert!(report.contains("Preserved msgs   9"));
    }

    #[test]
    fn status_line_reports_model_and_token_totals() {
        let status = format_status_report(
            "claude-sonnet",
            StatusUsage {
                message_count: 7,
                turns: 3,
                latest: runtime::TokenUsage {
                    input_tokens: 5,
                    output_tokens: 4,
                    cache_creation_input_tokens: 1,
                    cache_read_input_tokens: 0,
                },
                cumulative: runtime::TokenUsage {
                    input_tokens: 20,
                    output_tokens: 8,
                    cache_creation_input_tokens: 2,
                    cache_read_input_tokens: 1,
                },
                estimated_tokens: 128,
            },
            "workspace-write",
            &super::StatusContext {
                cwd: PathBuf::from("/tmp/project"),
                session_path: Some(PathBuf::from("session.jsonl")),
                loaded_config_files: 2,
                discovered_config_files: 3,
                memory_file_count: 4,
                project_root: Some(PathBuf::from("/tmp")),
                git_branch: Some("main".to_string()),
                git_summary: GitWorkspaceSummary {
                    changed_files: 3,
                    staged_files: 1,
                    unstaged_files: 1,
                    untracked_files: 1,
                    conflicted_files: 0,
                },
                sandbox_status: runtime::SandboxStatus::default(),
                config_load_error: None,
            },
            None, // #148
        );
        assert!(status.contains("Status"));
        assert!(status.contains("Model            claude-sonnet"));
        assert!(status.contains("Permission mode  workspace-write"));
        assert!(status.contains("Messages         7"));
        assert!(status.contains("Latest total     10"));
        assert!(status.contains("Cumulative total 31"));
        assert!(status.contains("Cwd              /tmp/project"));
        assert!(status.contains("Project root     /tmp"));
        assert!(status.contains("Git branch       main"));
        assert!(
            status.contains("Git state        dirty · 3 files · 1 staged, 1 unstaged, 1 untracked")
        );
        assert!(status.contains("Changed files    3"));
        assert!(status.contains("Staged           1"));
        assert!(status.contains("Unstaged         1"));
        assert!(status.contains("Untracked        1"));
        assert!(status.contains("Session          session.jsonl"));
        assert!(status.contains("Config files     loaded 2/3"));
        assert!(status.contains("Memory files     4"));
        assert!(status.contains("Suggested flow   /status → /diff → /commit"));
    }

    #[test]
    fn commit_reports_surface_workspace_context() {
        let summary = GitWorkspaceSummary {
            changed_files: 2,
            staged_files: 1,
            unstaged_files: 1,
            untracked_files: 0,
            conflicted_files: 0,
        };

        let preflight = format_commit_preflight_report(Some("feature/ux"), summary);
        assert!(preflight.contains("Result           ready"));
        assert!(preflight.contains("Branch           feature/ux"));
        assert!(preflight.contains("Workspace        dirty · 2 files · 1 staged, 1 unstaged"));
        assert!(preflight
            .contains("Action           create a git commit from the current workspace changes"));
    }

    #[test]
    fn commit_skipped_report_points_to_next_steps() {
        let report = format_commit_skipped_report();
        assert!(report.contains("Reason           no workspace changes"));
        assert!(report
            .contains("Action           create a git commit from the current workspace changes"));
        assert!(report.contains("/status to inspect context"));
        assert!(report.contains("/diff to inspect repo changes"));
    }

    #[test]
    fn runtime_slash_reports_describe_command_behavior() {
        let bughunter = format_bughunter_report(Some("runtime"));
        assert!(bughunter.contains("Scope            runtime"));
        assert!(bughunter.contains("inspect the selected code for likely bugs"));

        let ultraplan = format_ultraplan_report(Some("ship the release"));
        assert!(ultraplan.contains("Task             ship the release"));
        assert!(ultraplan.contains("break work into a multi-step execution plan"));

        let pr = format_pr_report("feature/ux", Some("ready for review"));
        assert!(pr.contains("Branch           feature/ux"));
        assert!(pr.contains("draft or create a pull request"));

        let issue = format_issue_report(Some("flaky test"));
        assert!(issue.contains("Context          flaky test"));
        assert!(issue.contains("draft or create a GitHub issue"));
    }

    #[test]
    fn no_arg_commands_reject_unexpected_arguments() {
        assert!(validate_no_args("/commit", None).is_ok());

        let error = validate_no_args("/commit", Some("now"))
            .expect_err("unexpected arguments should fail")
            .to_string();
        assert!(error.contains("/commit does not accept arguments"));
        assert!(error.contains("Received: now"));
    }

    #[test]
    fn config_report_supports_section_views() {
        let report = render_config_report(Some("env")).expect("config report should render");
        assert!(report.contains("Merged section: env"));
        let plugins_report =
            render_config_report(Some("plugins")).expect("plugins config report should render");
        assert!(plugins_report.contains("Merged section: plugins"));
    }

    #[test]
    fn memory_report_uses_sectioned_layout() {
        let report = render_memory_report().expect("memory report should render");
        assert!(report.contains("Memory"));
        assert!(report.contains("Working directory"));
        assert!(report.contains("Instruction files"));
        assert!(report.contains("Discovered files"));
    }

    #[test]
    fn config_report_uses_sectioned_layout() {
        let report = render_config_report(None).expect("config report should render");
        assert!(report.contains("Config"));
        assert!(report.contains("Discovered files"));
        assert!(report.contains("Merged JSON"));
    }

    #[test]
    fn parses_git_status_metadata() {
        let _guard = env_lock();
        let temp_root = temp_dir();
        fs::create_dir_all(&temp_root).expect("root dir");
        let (project_root, branch) = parse_git_status_metadata_for(
            &temp_root,
            Some(
                "## rcc/cli...origin/rcc/cli
 M src/main.rs",
            ),
        );
        assert_eq!(branch.as_deref(), Some("rcc/cli"));
        assert!(project_root.is_none());
        fs::remove_dir_all(temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_detached_head_from_status_snapshot() {
        let _guard = env_lock();
        assert_eq!(
            parse_git_status_branch(Some(
                "## HEAD (no branch)
 M src/main.rs"
            )),
            Some("detached HEAD".to_string())
        );
    }

    #[test]
    fn parses_git_workspace_summary_counts() {
        let summary = parse_git_workspace_summary(Some(
            "## feature/ux
M  src/main.rs
 M README.md
?? notes.md
UU conflicted.rs",
        ));

        assert_eq!(
            summary,
            GitWorkspaceSummary {
                changed_files: 4,
                staged_files: 2,
                unstaged_files: 2,
                untracked_files: 1,
                conflicted_files: 1,
            }
        );
        assert_eq!(
            summary.headline(),
            "dirty · 4 files · 2 staged, 2 unstaged, 1 untracked, 1 conflicted"
        );
    }

    #[test]
    fn render_diff_report_shows_clean_tree_for_committed_repo() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        git(&["init", "--quiet"], &root);
        git(&["config", "user.email", "tests@example.com"], &root);
        git(&["config", "user.name", "Rusty Claude Tests"], &root);
        fs::write(root.join("tracked.txt"), "hello\n").expect("write file");
        git(&["add", "tracked.txt"], &root);
        git(&["commit", "-m", "init", "--quiet"], &root);

        let report = render_diff_report_for(&root).expect("diff report should render");
        assert!(report.contains("clean working tree"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn render_diff_report_includes_staged_and_unstaged_sections() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        git(&["init", "--quiet"], &root);
        git(&["config", "user.email", "tests@example.com"], &root);
        git(&["config", "user.name", "Rusty Claude Tests"], &root);
        fs::write(root.join("tracked.txt"), "hello\n").expect("write file");
        git(&["add", "tracked.txt"], &root);
        git(&["commit", "-m", "init", "--quiet"], &root);

        fs::write(root.join("tracked.txt"), "hello\nstaged\n").expect("update file");
        git(&["add", "tracked.txt"], &root);
        fs::write(root.join("tracked.txt"), "hello\nstaged\nunstaged\n")
            .expect("update file twice");

        let report = render_diff_report_for(&root).expect("diff report should render");
        assert!(report.contains("Staged changes:"));
        assert!(report.contains("Unstaged changes:"));
        assert!(report.contains("tracked.txt"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn render_diff_report_omits_ignored_files() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        git(&["init", "--quiet"], &root);
        git(&["config", "user.email", "tests@example.com"], &root);
        git(&["config", "user.name", "Rusty Claude Tests"], &root);
        fs::write(root.join(".gitignore"), ".omx/\nignored.txt\n").expect("write gitignore");
        fs::write(root.join("tracked.txt"), "hello\n").expect("write tracked");
        git(&["add", ".gitignore", "tracked.txt"], &root);
        git(&["commit", "-m", "init", "--quiet"], &root);
        fs::create_dir_all(root.join(".omx")).expect("write omx dir");
        fs::write(root.join(".omx").join("state.json"), "{}").expect("write ignored omx");
        fs::write(root.join("ignored.txt"), "secret\n").expect("write ignored file");
        fs::write(root.join("tracked.txt"), "hello\nworld\n").expect("write tracked change");

        let report = render_diff_report_for(&root).expect("diff report should render");
        assert!(report.contains("tracked.txt"));
        assert!(!report.contains("+++ b/ignored.txt"));
        assert!(!report.contains("+++ b/.omx/state.json"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn resume_diff_command_renders_report_for_saved_session() {
        let _guard = env_lock();
        let root = temp_dir();
        fs::create_dir_all(&root).expect("root dir");
        git(&["init", "--quiet"], &root);
        git(&["config", "user.email", "tests@example.com"], &root);
        git(&["config", "user.name", "Rusty Claude Tests"], &root);
        fs::write(root.join("tracked.txt"), "hello\n").expect("write tracked");
        git(&["add", "tracked.txt"], &root);
        git(&["commit", "-m", "init", "--quiet"], &root);
        fs::write(root.join("tracked.txt"), "hello\nworld\n").expect("modify tracked");
        let session_path = root.join("session.json");
        Session::new()
            .save_to_path(&session_path)
            .expect("session should save");

        let session = Session::load_from_path(&session_path).expect("session should load");
        let outcome = with_current_dir(&root, || {
            run_resume_command(&session_path, &session, &SlashCommand::Diff)
                .expect("resume diff should work")
        });
        let message = outcome.message.expect("diff message should exist");
        assert!(message.contains("Unstaged changes:"));
        assert!(message.contains("tracked.txt"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn status_context_reads_real_workspace_metadata() {
        let context = status_context(None).expect("status context should load");
        assert!(context.cwd.is_absolute());
        assert!(context.discovered_config_files >= context.loaded_config_files);
        assert!(context.loaded_config_files <= context.discovered_config_files);
    }

    #[test]
    fn normalizes_supported_permission_modes() {
        assert_eq!(normalize_permission_mode("read-only"), Some("read-only"));
        assert_eq!(
            normalize_permission_mode("workspace-write"),
            Some("workspace-write")
        );
        assert_eq!(
            normalize_permission_mode("danger-full-access"),
            Some("danger-full-access")
        );
        assert_eq!(normalize_permission_mode("unknown"), None);
    }

    #[test]
    fn clear_command_requires_explicit_confirmation_flag() {
        assert_eq!(
            SlashCommand::parse("/clear"),
            Ok(Some(SlashCommand::Clear { confirm: false }))
        );
        assert_eq!(
            SlashCommand::parse("/clear --confirm"),
            Ok(Some(SlashCommand::Clear { confirm: true }))
        );
    }

    #[test]
    fn parses_resume_and_config_slash_commands() {
        assert_eq!(
            SlashCommand::parse("/resume saved-session.jsonl"),
            Ok(Some(SlashCommand::Resume {
                session_path: Some("saved-session.jsonl".to_string())
            }))
        );
        assert_eq!(
            SlashCommand::parse("/clear --confirm"),
            Ok(Some(SlashCommand::Clear { confirm: true }))
        );
        assert_eq!(
            SlashCommand::parse("/config"),
            Ok(Some(SlashCommand::Config { section: None }))
        );
        assert_eq!(
            SlashCommand::parse("/config env"),
            Ok(Some(SlashCommand::Config {
                section: Some("env".to_string())
            }))
        );
        assert_eq!(
            SlashCommand::parse("/memory"),
            Ok(Some(SlashCommand::Memory))
        );
        assert_eq!(SlashCommand::parse("/init"), Ok(Some(SlashCommand::Init)));
        assert_eq!(
            SlashCommand::parse("/session fork incident-review"),
            Ok(Some(SlashCommand::Session {
                action: Some("fork".to_string()),
                target: Some("incident-review".to_string())
            }))
        );
    }

    #[test]
    fn help_mentions_jsonl_resume_examples() {
        let mut help = Vec::new();
        print_help_to(&mut help).expect("help should render");
        let help = String::from_utf8(help).expect("help should be utf8");
        assert!(help.contains("claw --resume [SESSION.jsonl|session-id|latest]"));
        assert!(help.contains("Use `latest` with --resume, /resume, or /session switch"));
        assert!(help.contains("claw --resume latest"));
        assert!(help.contains("claw --resume latest /status /diff /export notes.txt"));
    }

    #[test]
    fn managed_sessions_default_to_jsonl_and_resolve_legacy_json() {
        let _guard = cwd_guard();
        let workspace = temp_workspace("session-resolution");
        std::fs::create_dir_all(&workspace).expect("workspace should create");
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&workspace).expect("switch cwd");

        let handle = create_managed_session_handle("session-alpha").expect("jsonl handle");
        assert!(handle.path.ends_with("session-alpha.jsonl"));

        let legacy_path = workspace.join(".claw/sessions/legacy.json");
        std::fs::create_dir_all(
            legacy_path
                .parent()
                .expect("legacy path should have parent directory"),
        )
        .expect("session dir should exist");
        Session::new()
            .with_workspace_root(workspace.clone())
            .with_persistence_path(legacy_path.clone())
            .save_to_path(&legacy_path)
            .expect("legacy session should save");

        let resolved = resolve_session_reference("legacy").expect("legacy session should resolve");
        assert_eq!(
            resolved
                .path
                .canonicalize()
                .expect("resolved path should exist"),
            legacy_path
                .canonicalize()
                .expect("legacy path should exist")
        );

        std::env::set_current_dir(previous).expect("restore cwd");
        std::fs::remove_dir_all(workspace).expect("workspace should clean up");
    }

    #[test]
    fn latest_session_alias_resolves_most_recent_managed_session() {
        let _guard = cwd_guard();
        let workspace = temp_workspace("latest-session-alias");
        std::fs::create_dir_all(&workspace).expect("workspace should create");
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&workspace).expect("switch cwd");

        let older = create_managed_session_handle("session-older").expect("older handle");
        Session::new()
            .with_persistence_path(older.path.clone())
            .save_to_path(&older.path)
            .expect("older session should save");
        std::thread::sleep(Duration::from_millis(20));
        let newer = create_managed_session_handle("session-newer").expect("newer handle");
        Session::new()
            .with_persistence_path(newer.path.clone())
            .save_to_path(&newer.path)
            .expect("newer session should save");

        let resolved = resolve_session_reference("latest").expect("latest session should resolve");
        assert_eq!(
            resolved
                .path
                .canonicalize()
                .expect("resolved path should exist"),
            newer.path.canonicalize().expect("newer path should exist")
        );

        std::env::set_current_dir(previous).expect("restore cwd");
        std::fs::remove_dir_all(workspace).expect("workspace should clean up");
    }

    #[test]
    fn load_session_reference_rejects_workspace_mismatch() {
        let _guard = cwd_guard();
        let workspace_a = temp_workspace("session-mismatch-a");
        let workspace_b = temp_workspace("session-mismatch-b");
        std::fs::create_dir_all(&workspace_a).expect("workspace a should create");
        std::fs::create_dir_all(&workspace_b).expect("workspace b should create");
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&workspace_b).expect("switch cwd");

        let session_path = workspace_a.join(".claw/sessions/legacy-cross.jsonl");
        std::fs::create_dir_all(
            session_path
                .parent()
                .expect("session path should have parent directory"),
        )
        .expect("session dir should exist");
        Session::new()
            .with_workspace_root(workspace_a.clone())
            .with_persistence_path(session_path.clone())
            .save_to_path(&session_path)
            .expect("session should save");

        let error = crate::load_session_reference(&session_path.display().to_string())
            .expect_err("mismatched workspace should fail");
        assert!(
            error.to_string().contains("session workspace mismatch"),
            "unexpected error: {error}"
        );
        assert!(
            error
                .to_string()
                .contains(&workspace_b.display().to_string()),
            "expected current workspace in error: {error}"
        );
        assert!(
            error
                .to_string()
                .contains(&workspace_a.display().to_string()),
            "expected originating workspace in error: {error}"
        );

        std::env::set_current_dir(previous).expect("restore cwd");
        std::fs::remove_dir_all(workspace_a).expect("workspace a should clean up");
        std::fs::remove_dir_all(workspace_b).expect("workspace b should clean up");
    }

    #[test]
    fn unknown_slash_command_guidance_suggests_nearby_commands() {
        let message = format_unknown_slash_command("stats");
        assert!(message.contains("Unknown slash command: /stats"));
        assert!(message.contains("/status"));
        assert!(message.contains("/help"));
    }

    #[test]
    fn unknown_omc_slash_command_guidance_explains_runtime_gap() {
        let message = format_unknown_slash_command("oh-my-claudecode:hud");
        assert!(message.contains("Unknown slash command: /oh-my-claudecode:hud"));
        assert!(message.contains("Claude Code/OMC plugin command"));
        assert!(message.contains("does not yet load plugin slash commands"));
    }

    #[test]
    fn resume_usage_mentions_latest_shortcut() {
        let usage = render_resume_usage();
        assert!(usage.contains("/resume <session-path|session-id|latest>"));
        assert!(usage.contains(".claw/sessions/<session-id>.jsonl"));
        assert!(usage.contains("/session list"));
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn cwd_guard() -> MutexGuard<'static, ()> {
        cwd_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn cwd_guard_recovers_after_poisoning() {
        let poisoned = std::thread::spawn(|| {
            let _guard = cwd_guard();
            panic!("poison cwd lock");
        })
        .join();
        assert!(poisoned.is_err(), "poisoning thread should panic");

        let _guard = cwd_guard();
    }

    fn temp_workspace(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("claw-cli-{label}-{nanos}"))
    }

    #[test]
    fn init_template_mentions_detected_rust_workspace() {
        let _guard = cwd_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rendered = crate::init::render_init_claude_md(&workspace_root);
        assert!(rendered.contains("# CLAUDE.md"));
        assert!(rendered.contains("cargo clippy --workspace --all-targets -- -D warnings"));
    }

    #[test]
    fn converts_tool_roundtrip_messages() {
        let messages = vec![
            ConversationMessage::user_text("hello"),
            ConversationMessage::assistant(vec![ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "bash".to_string(),
                input: "{\"command\":\"pwd\"}".to_string(),
            }]),
            ConversationMessage {
                role: MessageRole::Tool,
                blocks: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    tool_name: "bash".to_string(),
                    output: "ok".to_string(),
                    is_error: false,
                }],
                usage: None,
            },
        ];

        let converted = super::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "user");
    }

    // ========================================================================
    // CLI tool-result envelope reclassification (post-PR16 RCA 2026-05-20)
    //
    // Pre-fix the CLI emit path always built `ToolResultContentBlock::Text`
    // even when `output` was a pre-stringified JSON envelope. PR #16 only
    // flattens the `Json` arm, so on Ollama-routed models the embedded `{}`
    // caused turn-3 to fail with
    //   "Value looks like object, but can't find closing '}' symbol".
    // These tests exercise the live `convert_messages` path so the
    // PR #16 helper actually becomes reachable.
    // ========================================================================

    const SMOKE_FILE_READ_ENVELOPE: &str = concat!(
        "{\"type\":\"text\",",
        "\"file\":{",
        "\"filePath\":\"smoke_target.txt\",",
        "\"content\":\"a1-smoke-ok\",",
        "\"numLines\":1,",
        "\"startLine\":1,",
        "\"totalLines\":1",
        "}}",
    );

    #[test]
    fn envelope_helper_extracts_file_read_envelope() {
        let parsed = maybe_parse_tool_result_json_envelope(SMOKE_FILE_READ_ENVELOPE)
            .expect("file-read envelope must be recognized");
        let inner = parsed
            .pointer("/file/content")
            .and_then(Value::as_str)
            .expect("inner file.content must remain accessible");
        assert_eq!(inner, "a1-smoke-ok");
    }

    #[test]
    fn envelope_helper_returns_none_for_plain_text() {
        assert!(maybe_parse_tool_result_json_envelope("a1-smoke-ok").is_none());
        assert!(maybe_parse_tool_result_json_envelope("").is_none());
        assert!(maybe_parse_tool_result_json_envelope("ok\n").is_none());
    }

    #[test]
    fn envelope_helper_returns_none_for_unrecognized_object() {
        let arbitrary = r#"{"unexpected":{"nested":true}}"#;
        assert!(maybe_parse_tool_result_json_envelope(arbitrary).is_none());
        let array = r#"[1, 2, 3]"#;
        assert!(maybe_parse_tool_result_json_envelope(array).is_none());
        let primitive = r#"42"#;
        assert!(maybe_parse_tool_result_json_envelope(primitive).is_none());
    }

    #[test]
    fn envelope_helper_extracts_error_envelope() {
        let err = r#"{"error":"command failed"}"#;
        let parsed =
            maybe_parse_tool_result_json_envelope(err).expect("error envelope must be recognized");
        assert_eq!(
            parsed.get("error").and_then(Value::as_str),
            Some("command failed"),
        );
    }

    #[test]
    fn envelope_helper_extracts_other_string_keys() {
        for key in ["text", "content", "output", "result", "message"] {
            let payload = format!(r#"{{"{key}":"payload-{key}"}}"#);
            let parsed = maybe_parse_tool_result_json_envelope(&payload)
                .unwrap_or_else(|| panic!("key {key} must be recognized"));
            assert_eq!(
                parsed.get(key).and_then(Value::as_str),
                Some(format!("payload-{key}").as_str()),
            );
        }
    }

    #[test]
    fn convert_messages_emits_json_block_for_file_read_envelope() {
        let messages = vec![ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: "call_smoke".to_string(),
                tool_name: "read_file".to_string(),
                output: SMOKE_FILE_READ_ENVELOPE.to_string(),
                is_error: false,
            }],
            usage: None,
        }];

        let converted = super::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        let tool_result = match &converted[0].content[0] {
            InputContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "call_smoke");
                assert!(!is_error);
                content
            }
            other => panic!("expected ToolResult, got {other:?}"),
        };
        assert_eq!(tool_result.len(), 1);
        match &tool_result[0] {
            ToolResultContentBlock::Json { value } => {
                assert_eq!(
                    value.pointer("/file/content").and_then(Value::as_str),
                    Some("a1-smoke-ok"),
                );
            }
            ToolResultContentBlock::Text { text } => panic!(
                "envelope-shaped output must become Json, got Text({:?})",
                text
            ),
        }
    }

    #[test]
    fn convert_messages_preserves_text_block_for_plain_output() {
        let messages = vec![ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: "call_plain".to_string(),
                tool_name: "bash".to_string(),
                output: "a1-smoke-ok".to_string(),
                is_error: false,
            }],
            usage: None,
        }];

        let converted = super::convert_messages(&messages);
        let content = match &converted[0].content[0] {
            InputContentBlock::ToolResult { content, .. } => content,
            other => panic!("expected ToolResult, got {other:?}"),
        };
        match &content[0] {
            ToolResultContentBlock::Text { text } => assert_eq!(text, "a1-smoke-ok"),
            ToolResultContentBlock::Json { value } => {
                panic!("plain text must stay Text, got Json({value:?})")
            }
        }
    }

    #[test]
    fn convert_messages_preserves_text_block_for_unrecognized_json() {
        let arbitrary = r#"{"unexpected":{"nested":true}}"#;
        let messages = vec![ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: "call_arb".to_string(),
                tool_name: "bash".to_string(),
                output: arbitrary.to_string(),
                is_error: false,
            }],
            usage: None,
        }];

        let converted = super::convert_messages(&messages);
        let content = match &converted[0].content[0] {
            InputContentBlock::ToolResult { content, .. } => content,
            other => panic!("expected ToolResult, got {other:?}"),
        };
        match &content[0] {
            ToolResultContentBlock::Text { text } => assert_eq!(text, arbitrary),
            ToolResultContentBlock::Json { value } => {
                panic!("unrecognized JSON must stay Text, got Json({value:?})")
            }
        }
    }

    #[test]
    fn convert_messages_emits_flat_string_through_openai_flattener() {
        // End-to-end live-path check: after `convert_messages` the
        // file-read envelope must flatten to a bare string when run through
        // `api::flatten_tool_result_content`, the same helper the
        // OpenAI-compatible provider invokes when translating to
        // `role:"tool".content`. This is the contract that broke turn-3 in
        // the post-PR16 RCA wire capture.
        let messages = vec![ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: "call_smoke".to_string(),
                tool_name: "read_file".to_string(),
                output: SMOKE_FILE_READ_ENVELOPE.to_string(),
                is_error: false,
            }],
            usage: None,
        }];
        let converted = super::convert_messages(&messages);
        let content = match &converted[0].content[0] {
            InputContentBlock::ToolResult { content, .. } => content,
            other => panic!("expected ToolResult, got {other:?}"),
        };
        let flat = api::flatten_tool_result_content(content);
        assert_eq!(flat, "a1-smoke-ok");
        assert!(
            !flat.contains('{') && !flat.contains('}'),
            "flat content must not contain JSON braces; got {flat:?}"
        );
    }

    #[test]
    fn repl_help_mentions_history_completion_and_multiline() {
        let help = render_repl_help();
        assert!(help.contains("Up/Down"));
        assert!(help.contains("Tab"));
        assert!(help.contains("Shift+Enter/Ctrl+J"));
        assert!(help.contains("Ctrl-R"));
        assert!(help.contains("Reverse-search prompt history"));
        assert!(help.contains("/history [count]"));
    }

    #[test]
    fn parse_history_count_defaults_to_twenty_when_missing() {
        // given
        let raw: Option<&str> = None;

        // when
        let parsed = parse_history_count(raw);

        // then
        assert_eq!(parsed, Ok(20));
    }

    #[test]
    fn parse_history_count_accepts_positive_integers() {
        // given
        let raw = Some("25");

        // when
        let parsed = parse_history_count(raw);

        // then
        assert_eq!(parsed, Ok(25));
    }

    #[test]
    fn parse_history_count_rejects_zero() {
        // given
        let raw = Some("0");

        // when
        let parsed = parse_history_count(raw);

        // then
        assert!(parsed.is_err());
        assert!(parsed.unwrap_err().contains("greater than 0"));
    }

    #[test]
    fn parse_history_count_rejects_non_numeric() {
        // given
        let raw = Some("abc");

        // when
        let parsed = parse_history_count(raw);

        // then
        assert!(parsed.is_err());
        assert!(parsed.unwrap_err().contains("invalid count 'abc'"));
    }

    #[test]
    fn format_history_timestamp_renders_iso8601_utc() {
        // given
        // 2023-01-15T12:34:56.789Z -> 1673786096789 ms
        let timestamp_ms: u64 = 1_673_786_096_789;

        // when
        let formatted = format_history_timestamp(timestamp_ms);

        // then
        assert_eq!(formatted, "2023-01-15T12:34:56.789Z");
    }

    #[test]
    fn format_history_timestamp_renders_unix_epoch_origin() {
        // given
        let timestamp_ms: u64 = 0;

        // when
        let formatted = format_history_timestamp(timestamp_ms);

        // then
        assert_eq!(formatted, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn render_prompt_history_report_lists_entries_with_timestamps() {
        // given
        let entries = vec![
            PromptHistoryEntry {
                timestamp_ms: 1_673_786_096_000,
                text: "first prompt".to_string(),
            },
            PromptHistoryEntry {
                timestamp_ms: 1_673_786_100_000,
                text: "second prompt".to_string(),
            },
        ];

        // when
        let rendered = render_prompt_history_report(&entries, 10);

        // then
        assert!(rendered.contains("Prompt history"));
        assert!(rendered.contains("Total            2"));
        assert!(rendered.contains("Showing          2 most recent"));
        assert!(rendered.contains("Reverse search   Ctrl-R in the REPL"));
        assert!(rendered.contains("2023-01-15T12:34:56.000Z"));
        assert!(rendered.contains("first prompt"));
        assert!(rendered.contains("second prompt"));
    }

    #[test]
    fn render_prompt_history_report_truncates_to_limit_from_the_tail() {
        // given
        let entries = vec![
            PromptHistoryEntry {
                timestamp_ms: 1_000,
                text: "older".to_string(),
            },
            PromptHistoryEntry {
                timestamp_ms: 2_000,
                text: "middle".to_string(),
            },
            PromptHistoryEntry {
                timestamp_ms: 3_000,
                text: "latest".to_string(),
            },
        ];

        // when
        let rendered = render_prompt_history_report(&entries, 2);

        // then
        assert!(rendered.contains("Total            3"));
        assert!(rendered.contains("Showing          2 most recent"));
        assert!(!rendered.contains("older"));
        assert!(rendered.contains("middle"));
        assert!(rendered.contains("latest"));
    }

    #[test]
    fn render_prompt_history_report_handles_empty_history() {
        // given
        let entries: Vec<PromptHistoryEntry> = Vec::new();

        // when
        let rendered = render_prompt_history_report(&entries, 10);

        // then
        assert!(rendered.contains("no prompts recorded yet"));
    }

    #[test]
    fn collect_session_prompt_history_extracts_user_text_blocks() {
        // given
        let mut session = Session::new();
        session.push_user_text("hello").unwrap();
        session.push_user_text("world").unwrap();

        // when
        let entries = collect_session_prompt_history(&session);

        // then
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "hello");
        assert_eq!(entries[1].text, "world");
    }

    #[test]
    fn tool_rendering_helpers_compact_output() {
        let start = format_tool_call_start("read_file", r#"{"path":"src/main.rs"}"#);
        assert!(start.contains("read_file"));
        assert!(start.contains("src/main.rs"));

        let done = format_tool_result(
            "read_file",
            r#"{"file":{"filePath":"src/main.rs","content":"hello","numLines":1,"startLine":1,"totalLines":1}}"#,
            false,
        );
        assert!(done.contains("📄 Read src/main.rs"));
        assert!(done.contains("hello"));
    }

    #[test]
    fn tool_rendering_truncates_large_read_output_for_display_only() {
        let content = (0..200)
            .map(|index| format!("line {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = json!({
            "file": {
                "filePath": "src/main.rs",
                "content": content,
                "numLines": 200,
                "startLine": 1,
                "totalLines": 200
            }
        })
        .to_string();

        let rendered = format_tool_result("read_file", &output, false);

        assert!(rendered.contains("line 000"));
        assert!(rendered.contains("line 079"));
        assert!(!rendered.contains("line 199"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("line 199"));
    }

    #[test]
    fn tool_rendering_truncates_large_bash_output_for_display_only() {
        let stdout = (0..120)
            .map(|index| format!("stdout {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = json!({
            "stdout": stdout,
            "stderr": "",
            "returnCodeInterpretation": "completed successfully"
        })
        .to_string();

        let rendered = format_tool_result("bash", &output, false);

        assert!(rendered.contains("stdout 000"));
        assert!(rendered.contains("stdout 059"));
        assert!(!rendered.contains("stdout 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("stdout 119"));
    }

    #[test]
    fn tool_rendering_truncates_generic_long_output_for_display_only() {
        let items = (0..120)
            .map(|index| format!("payload {index:03}"))
            .collect::<Vec<_>>();
        let output = json!({
            "summary": "plugin payload",
            "items": items,
        })
        .to_string();

        let rendered = format_tool_result("plugin_echo", &output, false);

        assert!(rendered.contains("plugin_echo"));
        assert!(rendered.contains("payload 000"));
        assert!(rendered.contains("payload 040"));
        assert!(!rendered.contains("payload 080"));
        assert!(!rendered.contains("payload 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("payload 119"));
    }

    #[test]
    fn tool_rendering_truncates_raw_generic_output_for_display_only() {
        let output = (0..120)
            .map(|index| format!("raw {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");

        let rendered = format_tool_result("plugin_echo", &output, false);

        assert!(rendered.contains("plugin_echo"));
        assert!(rendered.contains("raw 000"));
        assert!(rendered.contains("raw 059"));
        assert!(!rendered.contains("raw 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("raw 119"));
    }

    #[test]
    fn ultraplan_progress_lines_include_phase_step_and_elapsed_status() {
        let snapshot = InternalPromptProgressState {
            command_label: "Ultraplan",
            task_label: "ship plugin progress".to_string(),
            step: 3,
            phase: "running read_file".to_string(),
            detail: Some("reading rust/crates/rusty-claude-cli/src/main.rs".to_string()),
            saw_final_text: false,
        };

        let started = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Started,
            &snapshot,
            Duration::from_secs(0),
            None,
        );
        let heartbeat = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Heartbeat,
            &snapshot,
            Duration::from_secs(9),
            None,
        );
        let completed = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Complete,
            &snapshot,
            Duration::from_secs(12),
            None,
        );
        let failed = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Failed,
            &snapshot,
            Duration::from_secs(12),
            Some("network timeout"),
        );

        assert!(started.contains("planning started"));
        assert!(started.contains("current step 3"));
        assert!(heartbeat.contains("heartbeat"));
        assert!(heartbeat.contains("9s elapsed"));
        assert!(heartbeat.contains("phase running read_file"));
        assert!(completed.contains("completed"));
        assert!(completed.contains("3 steps total"));
        assert!(failed.contains("failed"));
        assert!(failed.contains("network timeout"));
    }

    #[test]
    fn describe_tool_progress_summarizes_known_tools() {
        assert_eq!(
            describe_tool_progress("read_file", r#"{"path":"src/main.rs"}"#),
            "reading src/main.rs"
        );
        assert!(
            describe_tool_progress("bash", r#"{"command":"cargo test -p rusty-claude-cli"}"#)
                .contains("cargo test -p rusty-claude-cli")
        );
        assert_eq!(
            describe_tool_progress("grep_search", r#"{"pattern":"ultraplan","path":"rust"}"#),
            "grep `ultraplan` in rust"
        );
    }

    #[test]
    fn push_output_block_renders_markdown_text() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;
        let mut block_has_thinking_summary = false;

        push_output_block(
            OutputContentBlock::Text {
                text: "# Heading".to_string(),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            false,
            &mut block_has_thinking_summary,
        )
        .expect("text block should render");

        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("Heading"));
        assert!(rendered.contains('\u{1b}'));
    }

    #[test]
    fn push_output_block_skips_empty_object_prefix_for_tool_streams() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;
        let mut block_has_thinking_summary = false;

        push_output_block(
            OutputContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: json!({}),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            true,
            &mut block_has_thinking_summary,
        )
        .expect("tool block should accumulate");

        assert!(events.is_empty());
        assert_eq!(
            pending_tool,
            Some(("tool-1".to_string(), "read_file".to_string(), String::new(),))
        );
    }

    #[test]
    fn response_to_events_preserves_empty_object_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-1".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{}"
        ));
    }

    #[test]
    fn response_to_events_preserves_non_empty_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-2".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-2".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "rust/Cargo.toml" }),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{\"path\":\"rust/Cargo.toml\"}"
        ));
    }

    #[test]
    fn response_to_events_renders_collapsed_thinking_summary() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-3".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![
                    OutputContentBlock::Thinking {
                        thinking: "step 1".to_string(),
                        signature: Some("sig_123".to_string()),
                    },
                    OutputContentBlock::Text {
                        text: "Final answer".to_string(),
                    },
                ],
                stop_reason: Some("end_turn".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::TextDelta(text) if text == "Final answer"
        ));
        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("▶ Thinking (6 chars hidden)"));
        assert!(!rendered.contains("step 1"));
    }

    #[test]
    fn build_runtime_plugin_state_merges_plugin_hooks_into_runtime_features() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        let source_root = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&source_root).expect("source root");
        write_plugin_fixture(&source_root, "hook-runtime-demo", true, false);

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager
            .install(source_root.to_str().expect("utf8 source path"))
            .expect("plugin install should succeed");
        let loader = ConfigLoader::new(&workspace, &config_home);
        let runtime_config = loader.load().expect("runtime config should load");
        let state = build_runtime_plugin_state_with_loader(&workspace, &loader, &runtime_config)
            .expect("plugin state should load");
        let pre_hooks = state.feature_config.hooks().pre_tool_use();
        assert_eq!(pre_hooks.len(), 1);
        assert!(
            pre_hooks[0].ends_with("hooks/pre.sh"),
            "expected installed plugin hook path, got {pre_hooks:?}"
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn build_runtime_plugin_state_discovers_mcp_tools_and_surfaces_pending_servers() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        let script_path = workspace.join("fixture-mcp.py");
        write_mcp_server_fixture(&script_path);
        fs::write(
            config_home.join("settings.json"),
            format!(
                r#"{{
                  "mcpServers": {{
                    "alpha": {{
                      "command": "python3",
                      "args": ["{}"]
                    }},
                    "broken": {{
                      "command": "python3",
                      "args": ["-c", "import sys; sys.exit(0)"]
                    }}
                  }}
                }}"#,
                script_path.to_string_lossy()
            ),
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let runtime_config = loader.load().expect("runtime config should load");
        let state = build_runtime_plugin_state_with_loader(&workspace, &loader, &runtime_config)
            .expect("runtime plugin state should load");

        let allowed = state
            .tool_registry
            .normalize_allowed_tools(&["mcp__alpha__echo".to_string(), "MCPTool".to_string()])
            .expect("mcp tools should be allow-listable")
            .expect("allow-list should exist");
        assert!(allowed.contains("mcp__alpha__echo"));
        assert!(allowed.contains("MCPTool"));

        let mut executor = CliToolExecutor::new(
            None,
            false,
            state.tool_registry.clone(),
            state.mcp_state.clone(),
        );

        let tool_output = executor
            .execute("mcp__alpha__echo", r#"{"text":"hello"}"#)
            .expect("discovered mcp tool should execute");
        let tool_json: serde_json::Value =
            serde_json::from_str(&tool_output).expect("tool output should be json");
        assert_eq!(tool_json["structuredContent"]["echoed"], "hello");

        let wrapped_output = executor
            .execute(
                "MCPTool",
                r#"{"qualifiedName":"mcp__alpha__echo","arguments":{"text":"wrapped"}}"#,
            )
            .expect("generic mcp wrapper should execute");
        let wrapped_json: serde_json::Value =
            serde_json::from_str(&wrapped_output).expect("wrapped output should be json");
        assert_eq!(wrapped_json["structuredContent"]["echoed"], "wrapped");

        let search_output = executor
            .execute("ToolSearch", r#"{"query":"alpha echo","max_results":5}"#)
            .expect("tool search should execute");
        let search_json: serde_json::Value =
            serde_json::from_str(&search_output).expect("search output should be json");
        assert_eq!(search_json["matches"][0], "mcp__alpha__echo");
        assert_eq!(search_json["pending_mcp_servers"][0], "broken");
        assert_eq!(
            search_json["mcp_degraded"]["failed_servers"][0]["server_name"],
            "broken"
        );
        assert_eq!(
            search_json["mcp_degraded"]["failed_servers"][0]["phase"],
            "initialize_handshake"
        );
        assert_eq!(
            search_json["mcp_degraded"]["available_tools"][0],
            "mcp__alpha__echo"
        );
        let inventory_json = state
            .mcp_state
            .as_ref()
            .expect("mcp state should exist")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .inventory_json();
        assert_eq!(
            inventory_json["mcp_degraded"],
            search_json["mcp_degraded"],
            "direct runtime MCP inventory and model-facing search should report the same degraded state"
        );

        let listed = executor
            .execute("ListMcpResourcesTool", r#"{"server":"alpha"}"#)
            .expect("resources should list");
        let listed_json: serde_json::Value =
            serde_json::from_str(&listed).expect("resource output should be json");
        assert_eq!(listed_json["resources"][0]["uri"], "file://guide.txt");
        let legacy_listed = executor
            .execute("ListMcpResources", r#"{"server":"alpha"}"#)
            .expect("legacy resources tool should use live runtime state");
        let legacy_listed_json: serde_json::Value =
            serde_json::from_str(&legacy_listed).expect("legacy resource output should be json");
        assert_eq!(legacy_listed_json["source"], "runtime");
        assert_eq!(legacy_listed_json["count"], 1);
        assert_eq!(
            legacy_listed_json["resources"][0]["uri"],
            listed_json["resources"][0]["uri"]
        );

        let read = executor
            .execute(
                "ReadMcpResourceTool",
                r#"{"server":"alpha","uri":"file://guide.txt"}"#,
            )
            .expect("resource should read");
        let read_json: serde_json::Value =
            serde_json::from_str(&read).expect("resource read output should be json");
        assert_eq!(
            read_json["contents"][0]["text"],
            "contents for file://guide.txt"
        );
        let legacy_read = executor
            .execute(
                "ReadMcpResource",
                r#"{"server":"alpha","uri":"file://guide.txt"}"#,
            )
            .expect("legacy resource read should use live runtime state");
        let legacy_read_json: serde_json::Value =
            serde_json::from_str(&legacy_read).expect("legacy read output should be json");
        assert_eq!(legacy_read_json["source"], "runtime");
        assert_eq!(
            legacy_read_json["contents"][0]["text"],
            read_json["contents"][0]["text"]
        );

        let legacy_mcp = executor
            .execute(
                "MCP",
                r#"{"server":"alpha","tool":"echo","arguments":{"text":"legacy"}}"#,
            )
            .expect("legacy MCP tool should use live runtime state");
        let legacy_mcp_json: serde_json::Value =
            serde_json::from_str(&legacy_mcp).expect("legacy mcp output should be json");
        assert_eq!(legacy_mcp_json["status"], "success");
        assert_eq!(
            legacy_mcp_json["result"]["structuredContent"]["echoed"],
            "legacy"
        );

        let legacy_auth = executor
            .execute("McpAuth", r#"{"server":"broken"}"#)
            .expect("legacy MCP auth/status should use live runtime state");
        let legacy_auth_json: serde_json::Value =
            serde_json::from_str(&legacy_auth).expect("legacy mcp auth output should be json");
        assert_eq!(legacy_auth_json["source"], "runtime");
        assert_eq!(legacy_auth_json["status"], "error");
        assert_eq!(
            legacy_auth_json["mcp_degraded"], search_json["mcp_degraded"],
            "legacy model-facing MCP status should preserve the direct degraded report"
        );

        if let Some(mcp_state) = state.mcp_state {
            mcp_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shutdown()
                .expect("mcp shutdown should succeed");
        }

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn build_runtime_plugin_state_surfaces_unsupported_mcp_servers_structurally() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            config_home.join("settings.json"),
            r#"{
              "mcpServers": {
                "remote": {
                  "url": "https://example.test/mcp"
                }
              }
            }"#,
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let runtime_config = loader.load().expect("runtime config should load");
        let state = build_runtime_plugin_state_with_loader(&workspace, &loader, &runtime_config)
            .expect("runtime plugin state should load");
        let mut executor = CliToolExecutor::new(
            None,
            false,
            state.tool_registry.clone(),
            state.mcp_state.clone(),
        );

        let search_output = executor
            .execute("ToolSearch", r#"{"query":"remote","max_results":5}"#)
            .expect("tool search should execute");
        let search_json: serde_json::Value =
            serde_json::from_str(&search_output).expect("search output should be json");
        assert_eq!(search_json["pending_mcp_servers"][0], "remote");
        assert_eq!(
            search_json["mcp_degraded"]["failed_servers"][0]["server_name"],
            "remote"
        );
        assert_eq!(
            search_json["mcp_degraded"]["failed_servers"][0]["phase"],
            "server_registration"
        );
        assert_eq!(
            search_json["mcp_degraded"]["failed_servers"][0]["error"]["context"]["transport"],
            "http"
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    fn lifecycle_status_for_server(inventory: &serde_json::Value, server_name: &str) -> Value {
        inventory["lifecycle"]["servers"]
            .as_array()
            .expect("lifecycle servers should be an array")
            .iter()
            .find(|status| status["server"] == server_name)
            .cloned()
            .unwrap_or_else(|| panic!("missing lifecycle status for {server_name}"))
    }

    fn model_facing_mcp_auth_status(
        state: &RuntimePluginState,
        server_name: &str,
    ) -> serde_json::Value {
        let mut executor = CliToolExecutor::new(
            None,
            false,
            state.tool_registry.clone(),
            state.mcp_state.clone(),
        );
        let output = executor
            .execute("McpAuth", &format!(r#"{{"server":"{server_name}"}}"#))
            .expect("McpAuth status should execute");
        serde_json::from_str::<serde_json::Value>(&output).expect("McpAuth output should be JSON")
            ["lifecycle_status"]
            .clone()
    }

    fn assert_inventory_and_tool_status_match(
        loader: &ConfigLoader,
        workspace: &Path,
        server_name: &str,
    ) -> serde_json::Value {
        let inventory = runtime_mcp_inventory_json_for_loader(loader, workspace, None)
            .expect("runtime MCP inventory should render")
            .expect("runtime MCP inventory should handle list");
        let direct_status = lifecycle_status_for_server(&inventory, server_name);

        let runtime_config = loader.load().expect("runtime config should load");
        let state = build_runtime_plugin_state_with_loader(workspace, loader, &runtime_config)
            .expect("runtime plugin state should load");
        let tool_status = model_facing_mcp_auth_status(&state, server_name);
        assert_eq!(direct_status, tool_status);

        if let Some(mcp_state) = state.mcp_state {
            mcp_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shutdown()
                .expect("mcp shutdown should succeed");
        }

        direct_status
    }

    #[test]
    fn mcp_inventory_and_tool_status_match_for_healthy_server() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        let script_path = workspace.join("fixture-mcp.py");
        write_mcp_server_fixture(&script_path);
        fs::write(
            config_home.join("settings.json"),
            format!(
                r#"{{
                  "mcpServers": {{
                    "alpha": {{
                      "command": "python3",
                      "args": ["{}"]
                    }}
                  }}
                }}"#,
                script_path.to_string_lossy()
            ),
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let status = assert_inventory_and_tool_status_match(&loader, &workspace, "alpha");
        assert_eq!(status["status"], "connected");
        assert_eq!(status["phase"], "ready");
        assert_eq!(status["recoverable"], Value::Null);

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn mcp_inventory_and_tool_status_match_for_unsupported_transport() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            config_home.join("settings.json"),
            r#"{
              "mcpServers": {
                "remote": {
                  "url": "https://example.test/mcp"
                }
              }
            }"#,
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let status = assert_inventory_and_tool_status_match(&loader, &workspace, "remote");
        assert_eq!(status["status"], "unsupported");
        assert_eq!(status["phase"], "server_registration");
        assert_eq!(status["context"]["transport"], "http");
        assert_eq!(status["recoverable"], false);

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn mcp_inventory_and_tool_status_preserve_initialize_failure_phase() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            config_home.join("settings.json"),
            r#"{
              "mcpServers": {
                "broken": {
                  "command": "python3",
                  "args": ["-c", "import sys; sys.exit(0)"]
                }
              }
            }"#,
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let status = assert_inventory_and_tool_status_match(&loader, &workspace, "broken");
        assert_eq!(status["status"], "error");
        assert_eq!(status["phase"], "initialize_handshake");
        assert_eq!(status["recoverable"], false);
        assert!(status["message"]
            .as_str()
            .expect("message should be string")
            .contains("initialize"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn mcp_inventory_and_tool_status_match_for_degraded_startup() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        let script_path = workspace.join("fixture-mcp.py");
        write_mcp_server_fixture(&script_path);
        fs::write(
            config_home.join("settings.json"),
            format!(
                r#"{{
                  "mcpServers": {{
                    "alpha": {{
                      "command": "python3",
                      "args": ["{}"]
                    }},
                    "broken": {{
                      "command": "python3",
                      "args": ["-c", "import sys; sys.exit(0)"]
                    }}
                  }}
                }}"#,
                script_path.to_string_lossy()
            ),
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let alpha = assert_inventory_and_tool_status_match(&loader, &workspace, "alpha");
        assert_eq!(alpha["status"], "connected");
        let broken = assert_inventory_and_tool_status_match(&loader, &workspace, "broken");
        assert_eq!(broken["status"], "error");
        assert_eq!(broken["phase"], "initialize_handshake");

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn mcp_lifecycle_status_preserves_context_and_recoverability() {
        let config_home = temp_dir();
        let workspace = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        let healthy_path = workspace.join("fixture-mcp.py");
        let slow_path = workspace.join("disconnect-tools-list-mcp.py");
        write_mcp_server_fixture(&healthy_path);
        write_mcp_tools_list_disconnect_fixture(&slow_path);
        fs::write(
            config_home.join("settings.json"),
            format!(
                r#"{{
                  "mcpServers": {{
                    "alpha": {{
                      "command": "python3",
                      "args": ["{}"]
                    }},
                    "slow": {{
                      "command": "python3",
                      "args": ["{}"]
                    }}
                  }}
                }}"#,
                healthy_path.to_string_lossy(),
                slow_path.to_string_lossy()
            ),
        )
        .expect("write mcp settings");

        let loader = ConfigLoader::new(&workspace, &config_home);
        let status = assert_inventory_and_tool_status_match(&loader, &workspace, "slow");
        assert_eq!(status["status"], "error");
        assert_eq!(status["phase"], "tool_discovery");
        assert_eq!(status["recoverable"], true);
        assert_eq!(status["context"]["method"], "tools/list");
        assert!(status["context"].get("io_kind").is_some());
        assert!(status["message"]
            .as_str()
            .expect("message should be string")
            .contains("transport failed"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn build_runtime_runs_plugin_lifecycle_init_and_shutdown() {
        // Serialize access to process-wide env vars so parallel tests that
        // set/remove ANTHROPIC_API_KEY do not race with this test.
        let _guard = env_lock();
        let config_home = temp_dir();
        // Inject a dummy API key so runtime construction succeeds without real credentials.
        // This test only exercises plugin lifecycle (init/shutdown), never calls the API.
        std::env::set_var("ANTHROPIC_API_KEY", "test-dummy-key-for-plugin-lifecycle");
        let workspace = temp_dir();
        let source_root = temp_dir();
        fs::create_dir_all(&config_home).expect("config home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&source_root).expect("source root");
        write_plugin_fixture(&source_root, "lifecycle-runtime-demo", false, true);

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let install = manager
            .install(source_root.to_str().expect("utf8 source path"))
            .expect("plugin install should succeed");
        let log_path = install.install_path.join("lifecycle.log");
        let loader = ConfigLoader::new(&workspace, &config_home);
        let runtime_config = loader.load().expect("runtime config should load");
        let runtime_plugin_state =
            build_runtime_plugin_state_with_loader(&workspace, &loader, &runtime_config)
                .expect("plugin state should load");
        let mut runtime = build_runtime_with_plugin_state(
            Session::new(),
            "runtime-plugin-lifecycle",
            DEFAULT_MODEL.to_string(),
            vec!["test system prompt".to_string()],
            true,
            false,
            None,
            PermissionMode::DangerFullAccess,
            None,
            runtime_plugin_state,
        )
        .expect("runtime should build");

        assert_eq!(
            fs::read_to_string(&log_path).expect("init log should exist"),
            "init\n"
        );

        runtime
            .shutdown_plugins()
            .expect("plugin shutdown should succeed");

        assert_eq!(
            fs::read_to_string(&log_path).expect("shutdown log should exist"),
            "init\nshutdown\n"
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(source_root);
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn rejects_invalid_reasoning_effort_value() {
        let err = parse_args(&[
            "--reasoning-effort".to_string(),
            "turbo".to_string(),
            "prompt".to_string(),
            "hello".to_string(),
        ])
        .unwrap_err();
        assert!(
            err.contains("invalid value for --reasoning-effort"),
            "unexpected error: {err}"
        );
        assert!(err.contains("turbo"), "unexpected error: {err}");
    }

    #[test]
    fn accepts_valid_reasoning_effort_values() {
        for value in ["low", "medium", "high"] {
            let result = parse_args(&[
                "--reasoning-effort".to_string(),
                value.to_string(),
                "prompt".to_string(),
                "hello".to_string(),
            ]);
            assert!(
                result.is_ok(),
                "--reasoning-effort {value} should be accepted, got: {result:?}"
            );
            if let Ok(CliAction::Prompt {
                reasoning_effort, ..
            }) = result
            {
                assert_eq!(reasoning_effort.as_deref(), Some(value));
            }
        }
    }

    #[test]
    fn stub_commands_absent_from_repl_completions() {
        let candidates =
            slash_command_completion_candidates_with_sessions("claude-3-5-sonnet", None, vec![]);
        for stub in STUB_COMMANDS {
            let with_slash = format!("/{stub}");
            assert!(
                !candidates.contains(&with_slash),
                "stub command {with_slash} should not appear in REPL completions"
            );
        }
    }
}

fn write_mcp_server_fixture(script_path: &Path) {
    let script = [
            "#!/usr/bin/env python3",
            "import json, sys",
            "",
            "def read_message():",
            "    header = b''",
            r"    while not header.endswith(b'\r\n\r\n'):",
            "        chunk = sys.stdin.buffer.read(1)",
            "        if not chunk:",
            "            return None",
            "        header += chunk",
            "    length = 0",
            r"    for line in header.decode().split('\r\n'):",
            r"        if line.lower().startswith('content-length:'):",
            "            length = int(line.split(':', 1)[1].strip())",
            "    payload = sys.stdin.buffer.read(length)",
            "    return json.loads(payload.decode())",
            "",
            "def send_message(message):",
            "    payload = json.dumps(message).encode()",
            r"    sys.stdout.buffer.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode() + payload)",
            "    sys.stdout.buffer.flush()",
            "",
            "while True:",
            "    request = read_message()",
            "    if request is None:",
            "        break",
            "    method = request['method']",
            "    if method == 'initialize':",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'protocolVersion': request['params']['protocolVersion'],",
            "                'capabilities': {'tools': {}, 'resources': {}},",
            "                'serverInfo': {'name': 'fixture', 'version': '1.0.0'}",
            "            }",
            "        })",
            "    elif method == 'tools/list':",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'tools': [",
            "                    {",
            "                        'name': 'echo',",
            "                        'description': 'Echo from MCP fixture',",
            "                        'inputSchema': {",
            "                            'type': 'object',",
            "                            'properties': {'text': {'type': 'string'}},",
            "                            'required': ['text'],",
            "                            'additionalProperties': False",
            "                        },",
            "                        'annotations': {'readOnlyHint': True}",
            "                    }",
            "                ]",
            "            }",
            "        })",
            "    elif method == 'tools/call':",
            "        args = request['params'].get('arguments') or {}",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'content': [{'type': 'text', 'text': f\"echo:{args.get('text', '')}\"}],",
            "                'structuredContent': {'echoed': args.get('text', '')},",
            "                'isError': False",
            "            }",
            "        })",
            "    elif method == 'resources/list':",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'resources': [{'uri': 'file://guide.txt', 'name': 'guide', 'mimeType': 'text/plain'}]",
            "            }",
            "        })",
            "    elif method == 'resources/read':",
            "        uri = request['params']['uri']",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'contents': [{'uri': uri, 'mimeType': 'text/plain', 'text': f'contents for {uri}'}]",
            "            }",
            "        })",
            "    else:",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'error': {'code': -32601, 'message': method}",
            "        })",
            "",
        ]
        .join("\n");
    fs::write(script_path, script).expect("mcp fixture script should write");
}

fn write_mcp_tools_list_disconnect_fixture(script_path: &Path) {
    let script = [
            "#!/usr/bin/env python3",
            "import json, sys",
            "",
            "def read_message():",
            "    header = b''",
            r"    while not header.endswith(b'\r\n\r\n'):",
            "        chunk = sys.stdin.buffer.read(1)",
            "        if not chunk:",
            "            return None",
            "        header += chunk",
            "    length = 0",
            r"    for line in header.decode().split('\r\n'):",
            r"        if line.lower().startswith('content-length:'):",
            "            length = int(line.split(':', 1)[1].strip())",
            "    payload = sys.stdin.buffer.read(length)",
            "    return json.loads(payload.decode())",
            "",
            "def send_message(message):",
            "    payload = json.dumps(message).encode()",
            r"    sys.stdout.buffer.write(f'Content-Length: {len(payload)}\r\n\r\n'.encode() + payload)",
            "    sys.stdout.buffer.flush()",
            "",
            "while True:",
            "    request = read_message()",
            "    if request is None:",
            "        break",
            "    method = request['method']",
            "    if method == 'initialize':",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'result': {",
            "                'protocolVersion': request['params']['protocolVersion'],",
            "                'capabilities': {'tools': {}, 'resources': {}},",
            "                'serverInfo': {'name': 'slow-tools-list', 'version': '1.0.0'}",
            "            }",
            "        })",
            "    elif method == 'tools/list':",
            "        sys.exit(0)",
            "    else:",
            "        send_message({",
            "            'jsonrpc': '2.0',",
            "            'id': request['id'],",
            "            'error': {'code': -32601, 'message': method}",
            "        })",
            "",
        ]
        .join("\n");
    fs::write(script_path, script).expect("disconnecting mcp fixture script should write");
}

#[cfg(test)]
mod sandbox_report_tests {
    use super::{format_sandbox_report, HookAbortMonitor};
    use runtime::HookAbortSignal;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn sandbox_report_renders_expected_fields() {
        let report = format_sandbox_report(&runtime::SandboxStatus::default());
        assert!(report.contains("Sandbox"));
        assert!(report.contains("Enabled"));
        assert!(report.contains("Filesystem mode"));
        assert!(report.contains("Fallback reason"));
    }

    #[test]
    fn hook_abort_monitor_stops_without_aborting() {
        let abort_signal = HookAbortSignal::new();
        let (ready_tx, ready_rx) = mpsc::channel();
        let monitor = HookAbortMonitor::spawn_with_waiter(
            abort_signal.clone(),
            move |stop_rx, abort_signal| {
                ready_tx.send(()).expect("ready signal");
                let _ = stop_rx.recv();
                assert!(!abort_signal.is_aborted());
            },
        );

        ready_rx.recv().expect("waiter should be ready");
        monitor.stop();

        assert!(!abort_signal.is_aborted());
    }

    #[test]
    fn hook_abort_monitor_propagates_interrupt() {
        let abort_signal = HookAbortSignal::new();
        let (done_tx, done_rx) = mpsc::channel();
        let monitor = HookAbortMonitor::spawn_with_waiter(
            abort_signal.clone(),
            move |_stop_rx, abort_signal| {
                abort_signal.abort();
                done_tx.send(()).expect("done signal");
            },
        );

        done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("interrupt should complete");
        monitor.stop();

        assert!(abort_signal.is_aborted());
    }
}

#[cfg(test)]
mod dump_manifests_tests {
    use super::{dump_manifests_at_path, CliOutputFormat};
    use std::fs;

    #[test]
    fn dump_manifests_shows_helpful_error_when_manifests_missing() {
        let root = std::env::temp_dir().join(format!(
            "claw_test_missing_manifests_{}",
            std::process::id()
        ));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).expect("failed to create temp workspace");

        let result = dump_manifests_at_path(&workspace, None, CliOutputFormat::Text);
        assert!(
            result.is_err(),
            "expected an error when manifests are missing"
        );

        let error_msg = result.unwrap_err().to_string();

        assert!(
            error_msg.contains("Manifest source files are missing"),
            "error message should mention missing manifest sources: {error_msg}"
        );
        assert!(
            error_msg.contains(&root.display().to_string()),
            "error message should contain the resolved repo root path: {error_msg}"
        );
        assert!(
            error_msg.contains("src/commands.ts"),
            "error message should mention missing commands.ts: {error_msg}"
        );
        assert!(
            error_msg.contains("CLAUDE_CODE_UPSTREAM"),
            "error message should explain how to supply the upstream path: {error_msg}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dump_manifests_uses_explicit_manifest_dir() {
        let root = std::env::temp_dir().join(format!(
            "claw_test_explicit_manifest_dir_{}",
            std::process::id()
        ));
        let workspace = root.join("workspace");
        let upstream = root.join("upstream");
        fs::create_dir_all(workspace.join("nested")).expect("workspace should exist");
        fs::create_dir_all(upstream.join("src/entrypoints"))
            .expect("upstream fixture should exist");
        fs::write(
            upstream.join("src/commands.ts"),
            "import FooCommand from './commands/foo'\n",
        )
        .expect("commands fixture should write");
        fs::write(
            upstream.join("src/tools.ts"),
            "import ReadTool from './tools/read'\n",
        )
        .expect("tools fixture should write");
        fs::write(
            upstream.join("src/entrypoints/cli.tsx"),
            "startupProfiler()\n",
        )
        .expect("cli fixture should write");

        let result = dump_manifests_at_path(&workspace, Some(&upstream), CliOutputFormat::Text);
        assert!(
            result.is_ok(),
            "explicit manifest dir should succeed: {result:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }
}

// =============================================================================
// A2-L1b — `claw plan run` CLI parse + dispatch tests.
//
// Operator-required (Phase 4):
//   - `claw plan run <file>` exists
//   - valid plan path reaches a2-plan-runner
//   - refused plan exits non-zero
//   - no live broker call in CLI tests
//
// All tests stay in dry-run mode (no wrapper subprocess) OR point `--wrapper`
// at a non-existent path (graceful substrate-unavailable, still no live call).
// =============================================================================

#[cfg(test)]
mod plan_run_cli_tests {
    use super::*;
    use std::fs;
    use std::io::Write as _;

    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| (*s).to_string()).collect()
    }

    fn write_temp_plan(name: &str, body: &str) -> std::path::PathBuf {
        // Per-test sub-directory keyed by process + thread id avoids filename
        // collisions when cargo runs tests in parallel.
        let dir = std::env::temp_dir().join(format!(
            "a2_l1b_cli_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).expect("tempdir create");
        let path = dir.join(name);
        let mut f = fs::File::create(&path).expect("tempfile create");
        f.write_all(body.as_bytes()).expect("tempfile write");
        f.sync_all().expect("tempfile sync");
        path
    }

    // --- Required claim 1: `claw plan run <file>` exists --------------------

    #[test]
    fn plan_run_parses_with_only_a_file_arg() {
        let plan = write_temp_plan(
            "ok.yaml",
            "name: x\nsteps:\n  - id: s\n    description: d\n    tools: [Read]\n",
        );
        let action = parse_args(&args(&["plan", "run", plan.to_str().unwrap()])).unwrap();
        match action {
            CliAction::Plan {
                file,
                dry_run,
                report_format,
                ..
            } => {
                assert_eq!(file, plan);
                assert!(!dry_run);
                assert_eq!(report_format, PlanReportFormat::Markers);
            }
            other => panic!("expected CliAction::Plan, got {other:?}"),
        }
    }

    #[test]
    fn plan_run_parses_all_documented_flags() {
        let plan = write_temp_plan("flags.yaml", "name: x\nsteps: []\n");
        let action = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--dry-run",
            "--report-format",
            "json",
            "--substrate-url",
            "http://127.0.0.1:11435/v1",
            "--fast-model",
            "qwen3:14b",
            "--wrapper",
            "/some/path/claw-sidestack-local",
        ]))
        .unwrap();
        match action {
            CliAction::Plan {
                file,
                dry_run,
                report_format,
                substrate_url,
                fast_model,
                wrapper,
                step_timeout,
                workspace_write_preview,
                workspace_root,
            } => {
                assert_eq!(file, plan);
                assert!(dry_run);
                assert_eq!(report_format, PlanReportFormat::Json);
                assert_eq!(substrate_url.as_deref(), Some("http://127.0.0.1:11435/v1"));
                assert_eq!(fast_model.as_deref(), Some("qwen3:14b"));
                assert_eq!(
                    wrapper.unwrap(),
                    std::path::PathBuf::from("/some/path/claw-sidestack-local")
                );
                // No --step-timeout in this invocation → None;
                // run_plan_subcommand falls back to DEFAULT_STEP_TIMEOUT.
                assert_eq!(step_timeout, None);
                // No L2b opt-in flags in this invocation → defaults.
                assert!(!workspace_write_preview);
                assert!(workspace_root.is_none());
            }
            other => panic!("expected CliAction::Plan, got {other:?}"),
        }
    }

    #[test]
    fn plan_run_missing_file_errors() {
        let err = parse_args(&args(&["plan", "run"])).unwrap_err();
        assert!(err.contains("missing plan file"));
    }

    #[test]
    fn plan_run_rejects_unknown_subcommand() {
        let err = parse_args(&args(&["plan", "exec", "x.yaml"])).unwrap_err();
        assert!(err.contains("unsupported"));
    }

    // --- Required claim 2: refused plan exits non-zero ----------------------
    //   (Tested via run_plan_subcommand in dry-run mode — no subprocess, no
    //   broker. The dispatch arm in run() calls std::process::exit(code), so
    //   we test the returned exit code directly.)

    #[test]
    fn plan_run_dry_run_workspace_write_plan_exits_two_no_subprocess() {
        let plan = write_temp_plan(
            "refused-write.yaml",
            "name: w\nsteps:\n  - id: s\n    description: d\n    mode: workspace-write\n    tools: [Write]\n",
        );
        let code = run_plan_subcommand(
            &plan,
            true, // dry_run
            PlanReportFormat::Markers,
            None,
            None,
            None,  // wrapper unused in dry-run
            None,  // step_timeout unused in dry-run
            false, // workspace_write_preview: existing L1b path
            None,  // workspace_root
        );
        assert_eq!(
            code, 2,
            "workspace-write plan must exit 2 (PLAN_REFUSED_PRECHECK)"
        );
    }

    #[test]
    fn plan_run_dry_run_deep_plan_exits_two_no_subprocess() {
        let plan = write_temp_plan(
            "refused-deep.yaml",
            "name: d\nsteps:\n  - id: s\n    description: d\n    model_tier: DEEP\n    tools: [Read]\n",
        );
        let code = run_plan_subcommand(
            &plan,
            true,
            PlanReportFormat::Markers,
            None,
            None,
            None,
            None,
            false,
            None,
        );
        assert_eq!(code, 2, "DEEP plan must exit 2 (PLAN_REFUSED_PRECHECK)");
    }

    #[test]
    fn plan_run_dry_run_disallowed_tool_plan_exits_three_no_subprocess() {
        let plan = write_temp_plan(
            "refused-tool.yaml",
            "name: t\nsteps:\n  - id: s\n    description: d\n    tools: [Edit]\n",
        );
        let code = run_plan_subcommand(
            &plan,
            true,
            PlanReportFormat::Markers,
            None,
            None,
            None,
            None,
            false,
            None,
        );
        assert_eq!(code, 3, "Edit tool must exit 3 (TOOL_DISALLOWED)");
    }

    // --- Required claim 3: valid plan path reaches a2-plan-runner -----------
    //   Dry-run path proves the CLI hands off to a2_plan_runner::preflight
    //   and a2_plan_runner::runner::aggregate_plan_report (which emits the
    //   a2-l1b-* marker stream). Exit 0 = aggregate_plan_report returned Pass.

    #[test]
    fn plan_run_dry_run_valid_plan_exits_zero_via_runner_crate() {
        let plan = write_temp_plan(
            "valid.yaml",
            "name: ok\nsteps:\n  - id: s1\n    description: read\n    tools: [Read]\n",
        );
        let code = run_plan_subcommand(
            &plan,
            true,
            PlanReportFormat::Markers,
            None,
            None,
            None,
            None,
            false,
            None,
        );
        assert_eq!(code, 0, "valid read-only plan must exit 0 in dry-run");
    }

    // --- Required claim 4: no live broker call in CLI tests ----------------
    //   Substrate-unavailable path: --wrapper points at a non-existent file.
    //   run_plan_subcommand pre-checks wrapper existence and returns 4
    //   without spawning anything. The default substrate URL is included in
    //   the dispatch call but probe_substrate is never reached because the
    //   wrapper check short-circuits.

    #[test]
    fn plan_run_missing_wrapper_exits_four_without_subprocess_or_broker() {
        let plan = write_temp_plan(
            "valid.yaml",
            "name: ok\nsteps:\n  - id: s1\n    description: read\n    tools: [Read]\n",
        );
        let missing_wrapper = std::path::PathBuf::from("/does/not/exist/claw-sidestack-local");
        let code = run_plan_subcommand(
            &plan,
            false, // live path
            PlanReportFormat::Markers,
            None,
            None,
            Some(&missing_wrapper),
            None,  // step_timeout — irrelevant; wrapper check short-circuits first
            false, // workspace_write_preview
            None,  // workspace_root
        );
        assert_eq!(
            code, 4,
            "missing wrapper must exit 4 (SUBSTRATE_UNAVAILABLE)"
        );
    }

    // --- Parse-error path: exit 5 ------------------------------------------

    #[test]
    fn plan_run_yaml_parse_error_exits_five() {
        let plan = write_temp_plan("broken.yaml", "this is not valid yaml: {[\n");
        let code = run_plan_subcommand(
            &plan,
            true,
            PlanReportFormat::Markers,
            None,
            None,
            None,
            None,
            false,
            None,
        );
        assert_eq!(code, 5, "yaml parse error must exit 5 (EXIT_PARSE_ERROR)");
    }

    #[test]
    fn plan_run_missing_file_exits_five() {
        let missing = std::path::PathBuf::from("/does/not/exist/plan.yaml");
        let code = run_plan_subcommand(
            &missing,
            true,
            PlanReportFormat::Markers,
            None,
            None,
            None,
            None,
            false,
            None,
        );
        assert_eq!(code, 5, "missing plan file must exit 5 (EXIT_PARSE_ERROR)");
    }

    // --- Hard rules 6 + 7: elevation flags rejected at parse ---------------

    #[test]
    fn plan_run_rejects_allow_write_flag() {
        let plan = write_temp_plan("x.yaml", "name: x\nsteps: []\n");
        let err = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--allow-write",
        ]))
        .unwrap_err();
        assert!(err.contains("--allow-write"));
    }

    #[test]
    fn plan_run_rejects_force_flag() {
        let plan = write_temp_plan("x.yaml", "name: x\nsteps: []\n");
        let err =
            parse_args(&args(&["plan", "run", plan.to_str().unwrap(), "--force"])).unwrap_err();
        assert!(err.contains("--force"));
    }

    #[test]
    fn plan_run_subprocess_pins_read_only_via_runner_builder() {
        // The CLI's global `--permission-mode` flag is consumed by
        // `parse_args` BEFORE subcommand dispatch (it's a top-level flag,
        // like --output-format), so a subcommand-tail rejection isn't
        // architecturally reachable here. What matters operationally is
        // that the subprocess invocation a2-plan-runner produces ALWAYS
        // pins `--permission-mode read-only`, regardless of any user flag
        // plumbing. Defense-in-depth proof from the CLI's vantage point:
        use a2_plan_runner::runner::build_claw_command;
        use a2_plan_schema::{ModelTier, PlanMode, PlanStep};
        let step = PlanStep {
            id: "s".into(),
            description: "do".into(),
            mode: Some(PlanMode::ReadOnly),
            model_tier: Some(ModelTier::Fast),
            tools: vec!["Read".into()],
            expected_output: None,
            write_target: None,
            expected_post_write: None,
            after_file: None,
        };
        let cmd = build_claw_command(
            std::path::Path::new("/tmp/wrapper"),
            &step,
            std::path::Path::new("/tmp/workspace"),
        );
        assert!(
            cmd.args
                .windows(2)
                .any(|w| w[0] == "--permission-mode" && w[1] == "read-only"),
            "subprocess path must pin --permission-mode read-only; args were {:?}",
            cmd.args
        );
    }

    // --- Phase 5 Fix A: --step-timeout flag --------------------------------

    #[test]
    fn plan_run_parses_step_timeout_with_space_separator() {
        let plan = write_temp_plan("st-space.yaml", "name: x\nsteps: []\n");
        let action = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout",
            "300",
        ]))
        .unwrap();
        match action {
            CliAction::Plan { step_timeout, .. } => {
                assert_eq!(step_timeout, Some(std::time::Duration::from_secs(300)));
            }
            other => panic!("expected CliAction::Plan, got {other:?}"),
        }
    }

    #[test]
    fn plan_run_parses_step_timeout_with_equals_separator() {
        let plan = write_temp_plan("st-eq.yaml", "name: x\nsteps: []\n");
        let action = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout=420",
        ]))
        .unwrap();
        match action {
            CliAction::Plan { step_timeout, .. } => {
                assert_eq!(step_timeout, Some(std::time::Duration::from_secs(420)));
            }
            other => panic!("expected CliAction::Plan, got {other:?}"),
        }
    }

    #[test]
    fn plan_run_rejects_zero_step_timeout() {
        let plan = write_temp_plan("st-zero.yaml", "name: x\nsteps: []\n");
        let err = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout",
            "0",
        ]))
        .unwrap_err();
        assert!(err.contains("--step-timeout"));
        assert!(err.contains("minimum") || err.contains("below"));
    }

    #[test]
    fn plan_run_rejects_above_max_step_timeout() {
        let plan = write_temp_plan("st-huge.yaml", "name: x\nsteps: []\n");
        let err = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout",
            "999999",
        ]))
        .unwrap_err();
        assert!(err.contains("--step-timeout"));
        assert!(err.contains("maximum") || err.contains("exceeds"));
    }

    #[test]
    fn plan_run_rejects_non_integer_step_timeout() {
        let plan = write_temp_plan("st-bad.yaml", "name: x\nsteps: []\n");
        let err = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout",
            "abc",
        ]))
        .unwrap_err();
        assert!(err.contains("--step-timeout"));
        assert!(err.contains("invalid"));
    }

    #[test]
    fn plan_run_step_timeout_missing_value_errors() {
        let plan = write_temp_plan("st-miss.yaml", "name: x\nsteps: []\n");
        let err = parse_args(&args(&[
            "plan",
            "run",
            plan.to_str().unwrap(),
            "--step-timeout",
        ]))
        .unwrap_err();
        assert!(err.contains("missing value for --step-timeout"));
    }

    #[test]
    fn plan_run_step_timeout_is_applied_when_passed_through() {
        // End-to-end: --step-timeout 5 flows through parse_args →
        // CliAction::Plan → run_plan_subcommand → run_plan. We verify the
        // dry-run path (no subprocess) accepts the parsed value cleanly.
        let plan = write_temp_plan(
            "st-applied.yaml",
            "name: ok\nsteps:\n  - id: s\n    description: d\n    tools: [Read]\n",
        );
        let code = run_plan_subcommand(
            &plan,
            true, // dry-run: timeout value isn't exercised, but the parser must accept it cleanly
            PlanReportFormat::Markers,
            None,
            None,
            None,
            Some(std::time::Duration::from_secs(5)),
            false,
            None,
        );
        assert_eq!(code, 0, "valid dry-run plan must exit 0");
    }

    // --- Existing CLI behavior MUST not regress ----------------------------

    #[test]
    fn existing_doctor_subcommand_still_parses_unchanged() {
        let action = parse_args(&args(&["doctor"])).unwrap();
        assert!(matches!(action, CliAction::Doctor { .. }));
    }

    #[test]
    fn existing_prompt_subcommand_still_parses_unchanged() {
        let action = parse_args(&args(&["prompt", "hello world"])).unwrap();
        assert!(matches!(action, CliAction::Prompt { .. }));
    }

    #[test]
    fn existing_state_subcommand_still_parses_unchanged() {
        let action = parse_args(&args(&["state"])).unwrap();
        assert!(matches!(action, CliAction::State { .. }));
    }
}

// =========================================================================
// A2-L2b Slice 3c — CLI-local approval UX plumbing tests
// =========================================================================

#[cfg(test)]
mod approval_interaction_tests {
    use super::{
        run_approval_interaction, strip_one_trailing_newline, CliApprovalInteractionResult,
        EXIT_APPROVAL_DENIED,
    };
    use a2_plan_runner::{
        evaluate_operator_input, ApprovalDecision, ApprovalRefusal, PreviewDisplay, PreviewRecord,
    };
    use std::io::{self, Cursor, Read, Write};
    use std::sync::{Arc, Mutex};

    // -- Fixtures ----------------------------------------------------------

    const STEP: &str = "step-1";
    const HASH: &str = "c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4";

    fn mk_record(binary: bool, redacted: bool, truncated: bool) -> PreviewRecord {
        PreviewRecord {
            preview_id: "01HZZZZZZZZZZZZZZZZZZZZZZ0".to_string(),
            step_id: STEP.to_string(),
            target_relative_path_sanitized: "src/lib.rs".to_string(),
            target_absolute_path_sanitized: "/ws/src/lib.rs".to_string(),
            before_sha256: "a".repeat(64),
            after_sha256: "b".repeat(64),
            preview_sha256: HASH.to_string(),
            checkpoint_run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            checkpoint_step_id: STEP.to_string(),
            is_binary: binary,
            is_redacted: redacted,
            is_truncated: truncated,
            created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
            preview_format_version: 1,
        }
    }

    fn mk_display() -> PreviewDisplay {
        PreviewDisplay {
            rendered: "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@\n-old\n+new\n".to_string(),
        }
    }

    fn approvable_input() -> String {
        format!("apply {STEP} {HASH}")
    }

    // -- I/O helpers ------------------------------------------------------

    /// Reader that records, on its first `read` call, the value of a
    /// shared `flushed_count` so a test can prove the helper flushed
    /// `output` BEFORE reading any byte from `input`.
    struct FlushOrderReader {
        log: Arc<Mutex<IoLog>>,
        data: Cursor<Vec<u8>>,
    }

    impl Read for FlushOrderReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            {
                let mut log = self.log.lock().unwrap();
                if log.read_started_with_flush_count.is_none() {
                    log.read_started_with_flush_count = Some(log.flushed_count);
                }
            }
            self.data.read(buf)
        }
    }

    /// Companion writer that bumps `flushed_count` on every `flush` call
    /// and never marks itself flushed on `write`.
    struct FlushOrderWriter {
        log: Arc<Mutex<IoLog>>,
    }

    impl Write for FlushOrderWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut log = self.log.lock().unwrap();
            log.output.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.log.lock().unwrap().flushed_count += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    struct IoLog {
        output: Vec<u8>,
        flushed_count: usize,
        read_started_with_flush_count: Option<usize>,
    }

    /// Reader that panics on any read. Used to prove non-approvable
    /// previews short-circuit without ever touching the input stream.
    struct PanicOnReadReader;

    impl Read for PanicOnReadReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            panic!("run_approval_interaction read input on a non-approvable preview");
        }
    }

    fn run_with_buffers(
        rec: &PreviewRecord,
        display: &PreviewDisplay,
        baseline_ok: bool,
        input: &[u8],
    ) -> (CliApprovalInteractionResult, Vec<u8>) {
        let mut output = Vec::<u8>::new();
        let result = run_approval_interaction(
            rec,
            display,
            baseline_ok,
            Cursor::new(input.to_vec()),
            &mut output,
        )
        .expect("run_approval_interaction must not error on in-memory streams");
        (result, output)
    }

    // -- 1. prompt text is written to output stream -----------------------

    #[test]
    fn approvable_preview_writes_prompt_to_output() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (_result, output) =
            run_with_buffers(&rec, &display, true, approvable_input().as_bytes());
        let text = String::from_utf8(output).unwrap();
        assert!(
            text.contains("A2-L2b approval required"),
            "prompt header missing from output: {text}"
        );
        assert!(
            text.contains("To approve, type exactly:"),
            "approval instruction line missing from output: {text}"
        );
        assert!(
            text.contains(&format!("apply {STEP} {HASH}")),
            "approval command line missing from output: {text}"
        );
        assert!(
            text.contains("--- Diff Preview ---"),
            "diff preview separator missing from output: {text}"
        );
    }

    #[test]
    fn non_approvable_preview_writes_summary_to_output() {
        let rec = mk_record(true, false, false);
        let display = mk_display();
        let mut output = Vec::<u8>::new();
        let result =
            run_approval_interaction(&rec, &display, true, PanicOnReadReader, &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(
            text.contains("A2-L2b preview is not approvable"),
            "non-approvable summary header missing: {text}"
        );
        assert!(
            text.contains("No approval command is accepted for this preview."),
            "non-approvable closing line missing: {text}"
        );
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::PreviewBinary)
        );
    }

    // -- 2. output is flushed before read --------------------------------

    #[test]
    fn output_is_flushed_before_input_is_read() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let log = Arc::new(Mutex::new(IoLog::default()));
        let reader = FlushOrderReader {
            log: log.clone(),
            data: Cursor::new(approvable_input().into_bytes()),
        };
        let writer = FlushOrderWriter { log: log.clone() };
        let _result = run_approval_interaction(&rec, &display, true, reader, writer).unwrap();
        let log = log.lock().unwrap();
        let flush_count_at_first_read = log
            .read_started_with_flush_count
            .expect("first read should have observed the flushed counter");
        assert!(
            flush_count_at_first_read >= 1,
            "helper must flush output BEFORE reading input; \
             flushed_count at first read = {flush_count_at_first_read}"
        );
        assert!(
            !log.output.is_empty(),
            "helper must have written prompt bytes before reading input"
        );
    }

    // -- 3. exactly one operator submission is handled -------------------

    #[test]
    fn approves_on_exact_command_without_trailing_newline() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (result, _) = run_with_buffers(&rec, &display, true, approvable_input().as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    // -- 4. one terminal newline is stripped -----------------------------

    #[test]
    fn approves_on_exact_command_with_single_lf() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let input = format!("{}\n", approvable_input());
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    // -- 5. one CRLF is stripped -----------------------------------------

    #[test]
    fn approves_on_exact_command_with_single_crlf() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let input = format!("{}\r\n", approvable_input());
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    #[test]
    fn strip_one_trailing_newline_only_strips_one_terminator() {
        assert_eq!(strip_one_trailing_newline("apply x y"), "apply x y");
        assert_eq!(strip_one_trailing_newline("apply x y\n"), "apply x y");
        assert_eq!(strip_one_trailing_newline("apply x y\r\n"), "apply x y");
        // Embedded earlier newlines are NOT stripped.
        assert_eq!(strip_one_trailing_newline("a\nb\n"), "a\nb");
        // CRLF takes priority over bare LF when both could match.
        assert_eq!(strip_one_trailing_newline("a\r\n"), "a");
    }

    // -- 6. EOF returns refusal ------------------------------------------

    #[test]
    fn empty_input_refuses_via_arg_count() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (result, _) = run_with_buffers(&rec, &display, true, b"");
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 7. correct hash approves (already covered above; baseline=true) -

    // -- 8. wrong hash refuses -------------------------------------------

    #[test]
    fn wrong_hash_refuses_via_preview_hash_mismatch() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let wrong_hash = "d".repeat(64);
        let input = format!("apply {STEP} {wrong_hash}");
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::PreviewHashMismatch)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 9. baseline changed refuses -------------------------------------

    #[test]
    fn baseline_changed_refuses_via_checkpoint_drift() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (result, _) = run_with_buffers(&rec, &display, false, approvable_input().as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::CheckpointDrift)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 10. multi-line input refuses ------------------------------------

    #[test]
    fn multi_line_input_refuses_via_control_chars() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        // Valid command followed by an embedded newline + extra payload.
        let input = format!("{}\nextra-line", approvable_input());
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert!(
            matches!(
                result.decision,
                ApprovalDecision::Refused(ApprovalRefusal::ControlChars)
            ),
            "expected ControlChars refusal, got {:?}",
            result.decision
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn double_trailing_newline_collapses_through_layered_strip() {
        // Contract pin: rule 6 says the helper strips exactly one
        // trailing terminator, and rule 7 hands the residue to
        // `evaluate_operator_input`, whose internal
        // `strip_single_trailing_newline` independently strips one
        // more. Net effect for a syntactically valid command with two
        // trailing newlines is approval — embedded (non-terminal)
        // newlines are still refused, as the `multi_line_input_*`
        // case proves.
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let input = format!("{}\n\n", approvable_input());
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    // -- 11. pasted `a2-l2b-approved` marker text refuses ---------------

    #[test]
    fn pasted_approval_marker_text_alone_refuses() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (result, _) = run_with_buffers(&rec, &display, true, b"a2-l2b-approved");
        // Single token → ArgCount refusal.
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 12. marker plus valid command pasted as junk refuses ----------

    #[test]
    fn pasted_marker_plus_command_refuses_as_junk() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let input = format!("a2-l2b-approved apply {STEP} {HASH}");
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        // 4 tokens (marker + apply + step + hash) → ArgCount refusal.
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 13-15. binary / redacted / truncated previews skip input read --

    #[test]
    fn binary_preview_refuses_without_reading_input() {
        let rec = mk_record(true, false, false);
        let display = mk_display();
        let mut output = Vec::<u8>::new();
        let result =
            run_approval_interaction(&rec, &display, true, PanicOnReadReader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::PreviewBinary)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn redacted_preview_refuses_without_reading_input() {
        let rec = mk_record(false, true, false);
        let display = mk_display();
        let mut output = Vec::<u8>::new();
        let result =
            run_approval_interaction(&rec, &display, true, PanicOnReadReader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::PreviewRedacted)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn truncated_preview_refuses_without_reading_input() {
        let rec = mk_record(false, false, true);
        let display = mk_display();
        let mut output = Vec::<u8>::new();
        let result =
            run_approval_interaction(&rec, &display, true, PanicOnReadReader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::PreviewTruncated)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 16. audit markers are emitted but not authoritative -----------

    #[test]
    fn audit_markers_are_emitted_alongside_decision() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let (result, _) = run_with_buffers(&rec, &display, true, approvable_input().as_bytes());
        // Approval markers from the renderer plus the decision-tier
        // marker are present.
        assert!(result.audit_markers.contains(&"a2-l2b-diff-preview-ready"));
        assert!(result.audit_markers.contains(&"a2-l2b-approval-prompt"));
        assert!(result.audit_markers.contains(&"a2-l2b-approved"));
    }

    #[test]
    fn audit_markers_are_not_authority_for_decision() {
        // The decision returned by the helper is the *exact* decision
        // the underlying strict parser would have returned given the
        // same residue: markers are emitted in parallel and never
        // determine the verdict.
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let raw = approvable_input();
        let (result, _) = run_with_buffers(&rec, &display, true, raw.as_bytes());
        let direct = evaluate_operator_input(&rec, &raw, true);
        assert_eq!(result.decision, direct);

        let wrong_hash = "d".repeat(64);
        let bad_raw = format!("apply {STEP} {wrong_hash}");
        let (bad_result, _) = run_with_buffers(&rec, &display, true, bad_raw.as_bytes());
        let bad_direct = evaluate_operator_input(&rec, &bad_raw, true);
        assert_eq!(bad_result.decision, bad_direct);
        // Markers still emitted on refusal, but `decision` carries the
        // authoritative refusal variant.
        assert!(bad_result
            .audit_markers
            .contains(&"a2-l2b-approval-refused"));
    }

    // -- Enter-to-approve: interactive single-line submission --------------
    //
    // The interactive operator types the exact `apply <step-id>
    // <preview_sha256>` line and presses Enter ONCE. The helper must reach a
    // verdict on that single line WITHOUT waiting for a second read (which on
    // a real TTY means waiting for EOF / Ctrl-D). These tests use readers that
    // panic if read a second time, so they fail loudly if the helper ever
    // falls back to read-to-EOF on a clean single-line submission.

    /// Reader that serves a fixed payload on its first `read` call and then
    /// panics on any subsequent `read`. Proves the helper consumes a clean
    /// single-line submission without a second (EOF-seeking) read.
    struct SingleServeThenPanicReader {
        payload: Vec<u8>,
        served: bool,
    }

    impl SingleServeThenPanicReader {
        fn new(payload: &[u8]) -> Self {
            Self {
                payload: payload.to_vec(),
                served: false,
            }
        }
    }

    impl Read for SingleServeThenPanicReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            assert!(
                !self.served,
                "run_approval_interaction read a second time on a single-line \
                 submission — Enter alone must suffice (no Ctrl-D / read-to-EOF)"
            );
            self.served = true;
            let n = self.payload.len().min(buf.len());
            // Test payloads are far smaller than any BufReader fill, so a
            // single serve always delivers the whole line.
            assert_eq!(n, self.payload.len(), "test payload exceeded one read");
            buf[..n].copy_from_slice(&self.payload[..n]);
            Ok(n)
        }
    }

    #[test]
    fn enter_alone_approves_without_waiting_for_eof() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let line = format!("{}\n", approvable_input());
        let reader = SingleServeThenPanicReader::new(line.as_bytes());
        let mut output = Vec::<u8>::new();
        let result = run_approval_interaction(&rec, &display, true, reader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    #[test]
    fn enter_with_crlf_approves_without_waiting_for_eof() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let line = format!("{}\r\n", approvable_input());
        let reader = SingleServeThenPanicReader::new(line.as_bytes());
        let mut output = Vec::<u8>::new();
        let result = run_approval_interaction(&rec, &display, true, reader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Approved {
                step_id: STEP.to_string(),
                preview_sha256: HASH.to_string(),
            }
        );
        assert_eq!(result.exit_code_hint, 0);
    }

    /// Reader that returns EOF (`Ok(0)`) on its first read and panics if read
    /// again. Proves the empty-input refusal path also stops after one read.
    struct EofThenPanicReader {
        served: bool,
    }

    impl Read for EofThenPanicReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            assert!(
                !self.served,
                "run_approval_interaction read again after EOF on empty input"
            );
            self.served = true;
            Ok(0)
        }
    }

    #[test]
    fn eof_refuses_via_arg_count_without_extra_read() {
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let reader = EofThenPanicReader { served: false };
        let mut output = Vec::<u8>::new();
        let result = run_approval_interaction(&rec, &display, true, reader, &mut output).unwrap();
        assert_eq!(
            result.decision,
            ApprovalDecision::Refused(ApprovalRefusal::ArgCount)
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn bad_approval_line_refuses_on_single_enter() {
        // A malformed-but-single-line submission (Enter pressed once) must
        // refuse on that line alone, without waiting for EOF.
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let reader = SingleServeThenPanicReader::new(b"approve now please\n");
        let mut output = Vec::<u8>::new();
        let result = run_approval_interaction(&rec, &display, true, reader, &mut output).unwrap();
        assert!(
            matches!(result.decision, ApprovalDecision::Refused(_)),
            "expected refusal, got {:?}",
            result.decision
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn multiline_paste_still_refused_no_enter_bypass() {
        // SAFETY: a pasted blob whose first line is a valid approval but which
        // carries an extra command line MUST still be refused (the embedded
        // newline trips the strict parser), and the extra bytes must be
        // consumed by the helper rather than left for the shell. Reading only
        // the first line would approve here and leak the trailing command line
        // (here a harmless `id`) to the shell — this test pins that the
        // fallback drains the remainder and refuses.
        let rec = mk_record(false, false, false);
        let display = mk_display();
        let input = format!("{}\nid\n", approvable_input());
        let (result, _) = run_with_buffers(&rec, &display, true, input.as_bytes());
        assert!(
            matches!(
                result.decision,
                ApprovalDecision::Refused(ApprovalRefusal::ControlChars)
            ),
            "expected ControlChars refusal for multi-line paste, got {:?}",
            result.decision
        );
        assert_eq!(result.exit_code_hint, EXIT_APPROVAL_DENIED);
    }

    // -- 17. no target write APIs introduced (slice-3c helper section) -

    /// Source-grep test scoped to the slice-3c helper region of this
    /// binary. Mirrors the Phase-8 audit grep on the live diff so future
    /// edits can't silently introduce target-write APIs into the helper.
    #[test]
    fn slice_3c_helper_introduces_no_target_write_apis() {
        const SELF_SRC: &str = include_str!("main.rs");
        let section = slice_3c_helper_section(SELF_SRC);
        for needle in [
            "File::create",
            "OpenOptions",
            "std::fs::write",
            "::rename(",
            "remove_file",
            "remove_dir",
            "create_dir",
            "create_dir_all",
            "write_all",
        ] {
            assert!(
                !section.contains(needle),
                "slice-3c helper unexpectedly contains target-write token {needle:?}"
            );
        }
    }

    // -- 18. no run_plan workspace-write wiring introduced -------------

    #[test]
    fn slice_3c_helper_does_not_wire_run_plan_workspace_write() {
        // Phase-8 audit form: scoped to the slice-3c helper region of
        // this binary, assert no code-level workspace-write call sites
        // appear. The exact Phase-8 grep is `run_plan.*WorkspaceWrite`;
        // we replicate that as a per-line conjunction so descriptive
        // doc-comments (which reference the concepts in prose) don't
        // false-positive.
        const SELF_SRC: &str = include_str!("main.rs");
        let section = slice_3c_helper_section(SELF_SRC);
        for line in section.lines() {
            assert!(
                !(line.contains("run_plan") && line.contains("WorkspaceWrite")),
                "slice-3c helper unexpectedly contains a run_plan+WorkspaceWrite \
                 reference on line {line:?}; workspace-write wiring is out of scope"
            );
        }
        // The PascalCase Rust type name is a strong code-only signal —
        // it never appears in slice-3c prose, so any occurrence here is
        // a regression on its own.
        assert!(
            !section.contains("WorkspaceWrite"),
            "slice-3c helper unexpectedly contains `WorkspaceWrite` type reference"
        );
        // Direct invocation of the runner's plan executor would be a
        // function call, identifiable by the open paren. Doc-comments
        // refer to `run_plan` (no paren), so the paren-bound check is
        // comment-safe.
        for line in section.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }
            assert!(
                !line.contains("run_plan("),
                "slice-3c helper unexpectedly invokes `run_plan(` on line {line:?}"
            );
        }
    }

    // -- 19. no direct stdin/stdout added to a2-plan-runner ------------

    /// Embed every `a2-plan-runner/src/*.rs` source file at compile time
    /// and assert none of them gain stdin/stdout side effects. Slice 3c
    /// is forbidden from editing `a2-plan-runner`; if a future change
    /// adds I/O there, this test fails.
    #[test]
    fn a2_plan_runner_remains_free_of_stdin_stdout_side_effects() {
        const SRCS: &[(&str, &str)] = &[
            ("lib.rs", include_str!("../../a2-plan-runner/src/lib.rs")),
            (
                "approval.rs",
                include_str!("../../a2-plan-runner/src/approval.rs"),
            ),
            (
                "approval_ux.rs",
                include_str!("../../a2-plan-runner/src/approval_ux.rs"),
            ),
            (
                "checkpoint.rs",
                include_str!("../../a2-plan-runner/src/checkpoint.rs"),
            ),
            (
                "diff_preview.rs",
                include_str!("../../a2-plan-runner/src/diff_preview.rs"),
            ),
            (
                "markers.rs",
                include_str!("../../a2-plan-runner/src/markers.rs"),
            ),
            (
                "preflight.rs",
                include_str!("../../a2-plan-runner/src/preflight.rs"),
            ),
            (
                "report.rs",
                include_str!("../../a2-plan-runner/src/report.rs"),
            ),
            (
                "runner.rs",
                include_str!("../../a2-plan-runner/src/runner.rs"),
            ),
            (
                "write_runtime.rs",
                include_str!("../../a2-plan-runner/src/write_runtime.rs"),
            ),
        ];
        for (name, src) in SRCS {
            for needle in ["io::stdin", "io::stdout", "println!", "eprintln!"] {
                assert!(
                    !src.contains(needle),
                    "a2-plan-runner/src/{name} unexpectedly contains {needle:?}; \
                     slice 3c forbids adding stdin/stdout side effects there"
                );
            }
        }
    }

    // -- helpers ---------------------------------------------------------

    /// Extract the slice-3c helper region from this binary's source so
    /// the source-grep tests above scope their assertions to the region
    /// this lane is allowed to edit.
    ///
    /// The end marker is the next slice's section header (Slice 3d).
    /// When Slice 3c was the only A2-L2b CLI block, this scoping was
    /// bounded by the `CliAction` enum; later slices (3d, L2b-CLI-Apply,
    /// L2b-CLI-Preview-Bundle) ship between Slice 3c and that enum, so
    /// the old enum-anchored end marker incorrectly captured all of
    /// them. Anchoring on the next Slice header keeps the scope tight.
    fn slice_3c_helper_section(src: &str) -> &str {
        let start_marker =
            "// A2-L2b Slice 3c — CLI-local approval UX plumbing (hidden test-only seam)";
        let end_marker = "// A2-L2b Slice 3d — `claw plan approve <preview-bundle.json>` command";
        let start = src
            .find(start_marker)
            .expect("slice-3c helper section start marker should be present in main.rs");
        let rel_end = src[start..]
            .find(end_marker)
            .expect("slice-3c helper section end marker should be present after start");
        &src[start..start + rel_end]
    }
}

// =========================================================================
// A2-L2b Slice 3d — `claw plan approve` command tests
// =========================================================================

#[cfg(test)]
mod plan_approve_tests {
    use super::{
        emit_approval_result, emit_bundle_load_failure, emit_non_tty_refusal, load_preview_bundle,
        parse_plan_subcommand_args, run_approval_interaction, run_plan_approve,
        run_plan_approve_with_output, verify_record_display_binding, BundleLoadError, CliAction,
        CliApprovalInteractionResult, APPROVAL_RESULT_AUDIT_BUNDLE_REJECTED,
        APPROVAL_RESULT_AUDIT_NON_TTY, APPROVAL_RESULT_SCHEMA_V1, EXIT_APPROVAL_DENIED,
        EXIT_APPROVAL_OUTPUT_IO, EXIT_BUNDLE_PARSE_ERROR, PREVIEW_BUNDLE_SCHEMA_V1,
    };
    use a2_plan_runner::{
        canonical_preview_record_for_approval, preview_hash_from_parts, CanonicalSubset,
        PreviewDisplay, PreviewRecord, PREVIEW_FORMAT_VERSION,
    };
    use serde_json::Value;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    // -- Fixtures --------------------------------------------------------

    const STEP: &str = "step-1";
    const PREVIEW_ID: &str = "01HZZZZZZZZZZZZZZZZZZZZZZ0";
    const RUN_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock sane")
            .as_nanos();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("a2-l2b-approve-{nanos}-{seq}"));
        fs::create_dir_all(&dir).expect("tempdir create");
        dir
    }

    /// Build an approvable record + display, computing `preview_sha256`
    /// from the canonical subset so the resulting pair has a valid
    /// binding under [`verify_record_display_binding`].
    fn build_bound_pair() -> (PreviewRecord, PreviewDisplay) {
        build_pair(false, false, false, "--- a/src/lib.rs\n+++ b/src/lib.rs\n")
    }

    fn build_pair(
        is_binary: bool,
        is_redacted: bool,
        is_truncated: bool,
        rendered: &str,
    ) -> (PreviewRecord, PreviewDisplay) {
        let before_sha = "a".repeat(64);
        let after_sha = "b".repeat(64);
        let rel = "src/lib.rs".to_string();
        let subset = CanonicalSubset {
            preview_id: PREVIEW_ID,
            step_id: STEP,
            target_relative_path_sanitized: &rel,
            before_sha256: &before_sha,
            after_sha256: &after_sha,
            checkpoint_run_id: RUN_ID,
            checkpoint_step_id: STEP,
            is_binary,
            is_redacted,
            is_truncated,
            preview_format_version: PREVIEW_FORMAT_VERSION,
        };
        let canonical = canonical_preview_record_for_approval(&subset);
        let hash = preview_hash_from_parts(&canonical, rendered);
        let record = PreviewRecord {
            preview_id: PREVIEW_ID.to_string(),
            step_id: STEP.to_string(),
            target_relative_path_sanitized: rel,
            target_absolute_path_sanitized: "/ws/src/lib.rs".to_string(),
            before_sha256: before_sha,
            after_sha256: after_sha,
            preview_sha256: hash,
            checkpoint_run_id: RUN_ID.to_string(),
            checkpoint_step_id: STEP.to_string(),
            is_binary,
            is_redacted,
            is_truncated,
            created_at_utc: "2026-05-21T00:00:00.000000000Z".to_string(),
            preview_format_version: PREVIEW_FORMAT_VERSION,
        };
        let display = PreviewDisplay {
            rendered: rendered.to_string(),
        };
        (record, display)
    }

    fn write_bundle_json(
        dir: &std::path::Path,
        record: &PreviewRecord,
        display: &PreviewDisplay,
        baseline_unchanged: bool,
    ) -> PathBuf {
        let bundle = serde_json::json!({
            "schema_version": PREVIEW_BUNDLE_SCHEMA_V1,
            "preview_record": record,
            "preview_display": display,
            "checkpoint_baseline_unchanged": baseline_unchanged,
        });
        let path = dir.join("bundle.json");
        fs::write(
            &path,
            serde_json::to_vec(&bundle).expect("serialize bundle"),
        )
        .expect("write bundle");
        path
    }

    fn parse_stdout_json(stdout: &[u8]) -> Value {
        let text = std::str::from_utf8(stdout).expect("stdout utf8");
        serde_json::from_str(text.trim_end()).expect("stdout is one JSON value")
    }

    // -- CLI parse tests -------------------------------------------------

    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn plan_approve_subcommand_parses_with_bundle_path() {
        let action = parse_plan_subcommand_args(&args(&["approve", "/tmp/x.json"])).unwrap();
        match action {
            CliAction::PlanApprove {
                bundle_path,
                approval_result_output,
            } => {
                assert_eq!(bundle_path, PathBuf::from("/tmp/x.json"));
                assert_eq!(approval_result_output, None);
            }
            other => panic!("expected PlanApprove, got {other:?}"),
        }
    }

    #[test]
    fn plan_approve_subcommand_rejects_missing_bundle_path() {
        let err = parse_plan_subcommand_args(&args(&["approve"])).unwrap_err();
        assert!(err.contains("missing preview bundle"), "got: {err}");
    }

    #[test]
    fn plan_approve_subcommand_rejects_extra_positional() {
        let err = parse_plan_subcommand_args(&args(&["approve", "a.json", "b.json"])).unwrap_err();
        assert!(err.contains("unexpected positional"), "got: {err}");
    }

    #[test]
    fn plan_approve_subcommand_rejects_yes_flag() {
        let err = parse_plan_subcommand_args(&args(&["approve", "--yes", "a.json"])).unwrap_err();
        assert!(err.contains("unsupported flag"), "got: {err}");
    }

    #[test]
    fn plan_approve_subcommand_rejects_auto_flag() {
        let err = parse_plan_subcommand_args(&args(&["approve", "--auto", "a.json"])).unwrap_err();
        assert!(err.contains("unsupported flag"), "got: {err}");
    }

    #[test]
    fn plan_approve_subcommand_rejects_force_flag() {
        let err = parse_plan_subcommand_args(&args(&["approve", "--force", "a.json"])).unwrap_err();
        assert!(err.contains("unsupported flag"), "got: {err}");
    }

    // -- Bundle load tests -----------------------------------------------

    #[test]
    fn load_preview_bundle_round_trips_valid_bundle() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let bundle = load_preview_bundle(&path).expect("valid bundle");
        assert_eq!(bundle.schema_version, PREVIEW_BUNDLE_SCHEMA_V1);
        assert_eq!(bundle.preview_record.preview_sha256, rec.preview_sha256);
        assert!(bundle.checkpoint_baseline_unchanged);
    }

    #[test]
    fn load_preview_bundle_rejects_missing_file() {
        let dir = unique_temp_dir();
        let missing = dir.join("does-not-exist.json");
        match load_preview_bundle(&missing).unwrap_err() {
            BundleLoadError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn load_preview_bundle_rejects_invalid_json() {
        let dir = unique_temp_dir();
        let path = dir.join("broken.json");
        fs::write(&path, "{not json").unwrap();
        match load_preview_bundle(&path).unwrap_err() {
            BundleLoadError::Json(_) => {}
            other => panic!("expected Json, got {other:?}"),
        }
    }

    #[test]
    fn load_preview_bundle_rejects_wrong_schema_version() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let bundle = serde_json::json!({
            "schema_version": "a2-l2b-preview-bundle.v999",
            "preview_record": rec,
            "preview_display": disp,
            "checkpoint_baseline_unchanged": true,
        });
        let path = dir.join("bundle.json");
        fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        match load_preview_bundle(&path).unwrap_err() {
            BundleLoadError::SchemaVersionMismatch { actual } => {
                assert_eq!(actual, "a2-l2b-preview-bundle.v999");
            }
            other => panic!("expected SchemaVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn load_preview_bundle_rejects_unknown_fields() {
        // `serde(deny_unknown_fields)` is load-bearing: an attacker MUST
        // NOT be able to smuggle a trusted approval decision through.
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let bundle = serde_json::json!({
            "schema_version": PREVIEW_BUNDLE_SCHEMA_V1,
            "preview_record": rec,
            "preview_display": disp,
            "checkpoint_baseline_unchanged": true,
            "approval_decision": "approved",
        });
        let path = dir.join("bundle.json");
        fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        match load_preview_bundle(&path).unwrap_err() {
            BundleLoadError::Json(msg) => {
                assert!(
                    msg.contains("unknown field") || msg.contains("approval_decision"),
                    "got: {msg}"
                );
            }
            other => panic!("expected Json (deny_unknown_fields), got {other:?}"),
        }
    }

    #[test]
    fn load_preview_bundle_rejects_record_display_mismatch() {
        let dir = unique_temp_dir();
        let (rec, _disp) = build_bound_pair();
        // Tamper with the display by changing rendered bytes — the
        // re-derived hash no longer matches the record's preview_sha256.
        let tampered = PreviewDisplay {
            rendered: "completely different rendered bytes\n".to_string(),
        };
        let path = write_bundle_json(&dir, &rec, &tampered, true);
        match load_preview_bundle(&path).unwrap_err() {
            BundleLoadError::BindingMismatch => {}
            other => panic!("expected BindingMismatch, got {other:?}"),
        }
    }

    #[test]
    fn verify_record_display_binding_accepts_valid_pair() {
        let (rec, disp) = build_bound_pair();
        verify_record_display_binding(&rec, &disp).expect("valid binding");
    }

    #[test]
    fn verify_record_display_binding_rejects_tampered_record() {
        let (mut rec, disp) = build_bound_pair();
        // Flip one hex digit in preview_sha256.
        let mut hash: Vec<char> = rec.preview_sha256.chars().collect();
        hash[0] = if hash[0] == 'a' { 'b' } else { 'a' };
        rec.preview_sha256 = hash.into_iter().collect();
        match verify_record_display_binding(&rec, &disp).unwrap_err() {
            BundleLoadError::BindingMismatch => {}
            other => panic!("expected BindingMismatch, got {other:?}"),
        }
    }

    // -- run_plan_approve happy + refusal paths --------------------------

    fn run_with(
        bundle_path: &std::path::Path,
        stdin_is_tty: bool,
        input_bytes: &[u8],
    ) -> (i32, Vec<u8>, Vec<u8>) {
        let mut stdin = Cursor::new(input_bytes.to_vec());
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let code = run_plan_approve(
            bundle_path,
            stdin_is_tty,
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );
        (code, stdout, stderr)
    }

    // -- Option C: --approval-result-output persistence ------------------

    /// Drive `run_plan_approve_with_output` into in-memory streams with an
    /// optional persistence path. Mirrors `run_with` plus the output sink.
    fn run_with_output(
        bundle_path: &std::path::Path,
        approval_result_output: Option<&std::path::Path>,
        stdin_is_tty: bool,
        input_bytes: &[u8],
    ) -> (i32, Vec<u8>, Vec<u8>) {
        let mut stdin = Cursor::new(input_bytes.to_vec());
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let code = run_plan_approve_with_output(
            bundle_path,
            approval_result_output,
            stdin_is_tty,
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );
        (code, stdout, stderr)
    }

    #[test]
    fn approve_output_flag_persists_approval_result_on_success() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let out_path = dir.join("approval-result.json");
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) =
            run_with_output(&path, Some(out_path.as_path()), true, approval.as_bytes());
        assert_eq!(code, 0, "approved decision exits 0");
        // The persisted file equals the stdout bytes exactly.
        let file_bytes = std::fs::read(&out_path).expect("approval-result file written");
        assert_eq!(file_bytes, stdout, "persisted file must equal stdout bytes");
        let json = parse_stdout_json(&file_bytes);
        assert_eq!(json["schema_version"], APPROVAL_RESULT_SCHEMA_V1);
        assert_eq!(json["decision"], "approved");
        assert_eq!(json["step_id"], rec.step_id);
        assert_eq!(json["preview_sha256"], rec.preview_sha256);
    }

    #[test]
    fn approve_without_output_flag_writes_no_file_and_preserves_stdout() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) = run_with_output(&path, None, true, approval.as_bytes());
        assert_eq!(code, 0);
        // Stdout still carries the approval-result JSON (behavior unchanged).
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "approved");
        assert!(
            !dir.join("approval-result.json").exists(),
            "no file should be created without the flag"
        );
    }

    #[test]
    fn approve_output_flag_writes_no_file_on_refusal() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let out_path = dir.join("approval-result.json");
        let bad_hash = "f".repeat(64);
        let approval = format!("apply {} {bad_hash}\n", rec.step_id);
        let (code, stdout, _stderr) =
            run_with_output(&path, Some(out_path.as_path()), true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert!(!out_path.exists(), "no file written on a refused approval");
    }

    #[test]
    fn approve_output_flag_writes_no_file_on_non_tty() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let out_path = dir.join("approval-result.json");
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        // stdin_is_tty = false → approvable bundle refuses (exit 7); no file.
        let (code, stdout, _stderr) =
            run_with_output(&path, Some(out_path.as_path()), false, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["reason"].as_str().unwrap(), "approval-stdin-not-tty");
        assert!(
            !out_path.exists(),
            "no file written when the TTY guard fails"
        );
    }

    #[test]
    fn approve_output_flag_refuses_to_overwrite_existing_file() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let out_path = dir.join("approval-result.json");
        std::fs::write(&out_path, b"PREEXISTING").expect("seed existing output file");
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) =
            run_with_output(&path, Some(out_path.as_path()), true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_OUTPUT_IO, "refuse-overwrite exit code");
        // Existing file is untouched and the approval never ran (no stdout JSON).
        let existing = std::fs::read(&out_path).expect("existing file still present");
        assert_eq!(existing, b"PREEXISTING");
        assert!(
            stdout.is_empty(),
            "approval must not run when the output path already exists"
        );
    }

    // -- Option C parser tests -------------------------------------------

    #[test]
    fn plan_approve_subcommand_parses_approval_result_output_separate_arg() {
        let action = parse_plan_subcommand_args(&args(&[
            "approve",
            "/tmp/x.json",
            "--approval-result-output",
            "/tmp/out.json",
        ]))
        .unwrap();
        match action {
            CliAction::PlanApprove {
                bundle_path,
                approval_result_output,
            } => {
                assert_eq!(bundle_path, PathBuf::from("/tmp/x.json"));
                assert_eq!(approval_result_output, Some(PathBuf::from("/tmp/out.json")));
            }
            other => panic!("expected PlanApprove, got {other:?}"),
        }
    }

    #[test]
    fn plan_approve_subcommand_parses_approval_result_output_equals_form() {
        let action = parse_plan_subcommand_args(&args(&[
            "approve",
            "--approval-result-output=/tmp/out.json",
            "/tmp/x.json",
        ]))
        .unwrap();
        match action {
            CliAction::PlanApprove {
                bundle_path,
                approval_result_output,
            } => {
                assert_eq!(bundle_path, PathBuf::from("/tmp/x.json"));
                assert_eq!(approval_result_output, Some(PathBuf::from("/tmp/out.json")));
            }
            other => panic!("expected PlanApprove, got {other:?}"),
        }
    }

    #[test]
    fn plan_approve_subcommand_rejects_output_flag_without_path() {
        let err = parse_plan_subcommand_args(&args(&[
            "approve",
            "/tmp/x.json",
            "--approval-result-output",
        ]))
        .unwrap_err();
        assert!(err.contains("requires a path"), "err was {err:?}");
    }

    #[test]
    fn plan_approve_subcommand_still_rejects_yes_flag_with_output_support() {
        let err =
            parse_plan_subcommand_args(&args(&["approve", "--yes", "/tmp/x.json"])).unwrap_err();
        assert!(err.contains("unsupported flag"), "err was {err:?}");
    }

    #[test]
    fn run_plan_approve_happy_path_emits_approved_json_exit_zero() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, 0);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["schema_version"], APPROVAL_RESULT_SCHEMA_V1);
        assert_eq!(json["decision"], "approved");
        assert_eq!(json["preview_id"], rec.preview_id);
        assert_eq!(json["step_id"], rec.step_id);
        assert_eq!(json["preview_sha256"], rec.preview_sha256);
        assert_eq!(json["checkpoint_baseline_unchanged"], true);
        assert_eq!(json["exit_code_hint"], 0);
        let markers = json["audit_markers"].as_array().unwrap();
        assert!(markers.iter().any(|m| m == "a2-l2b-approved"));
    }

    #[test]
    fn run_plan_approve_wrong_hash_emits_refused_exit_seven() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let bad_hash = "f".repeat(64);
        let approval = format!("apply {} {bad_hash}\n", rec.step_id);
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["exit_code_hint"], EXIT_APPROVAL_DENIED);
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "approval-preview-hash-mismatch"
        );
    }

    #[test]
    fn run_plan_approve_eof_input_refuses_exit_seven() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let (code, stdout, _stderr) = run_with(&path, true, b"");
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        // Slice-3a routes EOF through ArgCount.
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "approval-arg-count-invalid"
        );
    }

    #[test]
    fn run_plan_approve_checkpoint_drift_refuses_exit_seven() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, false);
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["checkpoint_baseline_unchanged"], false);
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "checkpoint-baseline-changed"
        );
    }

    /// Reader that panics on any read. Proves a non-approvable bundle
    /// never touches `stdin` before refusing.
    struct PanicOnReadReader;
    impl std::io::Read for PanicOnReadReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            panic!("run_plan_approve must not read stdin for a non-approvable bundle");
        }
    }

    fn run_with_panic_reader(bundle_path: &std::path::Path) -> (i32, Vec<u8>) {
        let mut stdin = PanicOnReadReader;
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let code = run_plan_approve(bundle_path, true, &mut stdin, &mut stdout, &mut stderr);
        (code, stdout)
    }

    #[test]
    fn run_plan_approve_binary_preview_refuses_without_reading_input() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_pair(true, false, false, "binary preview placeholder\n");
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let (code, stdout) = run_with_panic_reader(&path);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "preview-binary-non-approvable"
        );
    }

    #[test]
    fn run_plan_approve_redacted_preview_refuses_without_reading_input() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_pair(false, true, false, "redacted preview placeholder\n");
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let (code, stdout) = run_with_panic_reader(&path);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "preview-redacted-non-approvable"
        );
    }

    #[test]
    fn run_plan_approve_truncated_preview_refuses_without_reading_input() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_pair(false, false, true, "truncated preview placeholder\n");
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let (code, stdout) = run_with_panic_reader(&path);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "preview-truncated-non-approvable"
        );
    }

    #[test]
    fn run_plan_approve_invalid_bundle_json_exits_five() {
        let dir = unique_temp_dir();
        let path = dir.join("broken.json");
        fs::write(&path, "{not json").unwrap();
        let (code, stdout, _stderr) = run_with(&path, true, b"");
        assert_eq!(code, EXIT_BUNDLE_PARSE_ERROR);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "bundle_rejected");
        assert_eq!(json["exit_code_hint"], EXIT_BUNDLE_PARSE_ERROR);
        assert!(json["reason"]
            .as_str()
            .unwrap()
            .starts_with("bundle-json-parse-error"));
    }

    #[test]
    fn run_plan_approve_missing_bundle_file_exits_five() {
        let dir = unique_temp_dir();
        let missing = dir.join("nope.json");
        let (code, stdout, _stderr) = run_with(&missing, true, b"");
        assert_eq!(code, EXIT_BUNDLE_PARSE_ERROR);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "bundle_rejected");
        assert!(json["reason"]
            .as_str()
            .unwrap()
            .starts_with("bundle-io-error"));
    }

    #[test]
    fn run_plan_approve_wrong_schema_version_exits_five() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let bundle = serde_json::json!({
            "schema_version": "a2-l2b-preview-bundle.v999",
            "preview_record": rec,
            "preview_display": disp,
            "checkpoint_baseline_unchanged": true,
        });
        let path = dir.join("bundle.json");
        fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        let (code, stdout, _stderr) = run_with(&path, true, b"");
        assert_eq!(code, EXIT_BUNDLE_PARSE_ERROR);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "bundle_rejected");
        assert!(json["reason"]
            .as_str()
            .unwrap()
            .starts_with("bundle-schema-version-mismatch"));
    }

    #[test]
    fn run_plan_approve_record_display_mismatch_exits_five() {
        let dir = unique_temp_dir();
        let (rec, _disp) = build_bound_pair();
        let tampered = PreviewDisplay {
            rendered: "different bytes that change the hash\n".to_string(),
        };
        let path = write_bundle_json(&dir, &rec, &tampered, true);
        let (code, stdout, _stderr) = run_with(&path, true, b"");
        assert_eq!(code, EXIT_BUNDLE_PARSE_ERROR);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "bundle_rejected");
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "bundle-record-display-binding-mismatch"
        );
    }

    #[test]
    fn run_plan_approve_pasted_marker_text_refuses() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        // An operator pasting the audit marker token instead of the
        // approval line — the Slice-3a parser must refuse. A single
        // token (no space-separated tail) is refused by the parser's
        // arg-count check, BEFORE the keyword check; either way the
        // exit code is 7 and the decision is "refused".
        let approval = b"a2-l2b-approved\n";
        let (code, stdout, _stderr) = run_with(&path, true, approval);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        let reason = json["reason"].as_str().unwrap();
        assert!(
            reason == "approval-arg-count-invalid" || reason == "apply-keyword-invalid",
            "expected pasted-marker text to be refused via arg-count or keyword check, got: {reason}"
        );
    }

    #[test]
    fn run_plan_approve_pasted_three_token_marker_text_refuses_keyword() {
        // Three-space-separated tokens whose first token is not
        // `apply` exercise the keyword-check path explicitly.
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("a2-l2b-approved {} {}\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["reason"].as_str().unwrap(), "apply-keyword-invalid");
    }

    #[test]
    fn run_plan_approve_non_tty_for_approvable_refuses_without_reading() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let mut stdin = PanicOnReadReader;
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        // stdin_is_tty=false on an approvable bundle must short-circuit
        // before any read.
        let code = run_plan_approve(&path, false, &mut stdin, &mut stdout, &mut stderr);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["reason"].as_str().unwrap(), "approval-stdin-not-tty");
        let markers = json["audit_markers"].as_array().unwrap();
        assert!(markers
            .iter()
            .any(|m| m.as_str() == Some(APPROVAL_RESULT_AUDIT_NON_TTY)));
    }

    /// Reader that serves a fixed payload on the first read and panics if
    /// read again — proves `run_plan_approve` reaches a verdict on a single
    /// interactive Enter without a second (EOF-seeking / Ctrl-D) read.
    struct ServeOnceThenPanicReader {
        payload: Vec<u8>,
        served: bool,
    }
    impl std::io::Read for ServeOnceThenPanicReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            assert!(
                !self.served,
                "run_plan_approve read a second time on a single-line submission \
                 — Enter alone must suffice (no Ctrl-D)"
            );
            self.served = true;
            let n = self.payload.len().min(buf.len());
            assert_eq!(n, self.payload.len(), "test payload exceeded one read");
            buf[..n].copy_from_slice(&self.payload[..n]);
            Ok(n)
        }
    }

    #[test]
    fn run_plan_approve_enter_to_approve_single_line_no_ctrl_d() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let line = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let mut stdin = ServeOnceThenPanicReader {
            payload: line.into_bytes(),
            served: false,
        };
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        // tty=true and only ONE line served: must approve without a second read.
        let code = run_plan_approve(&path, true, &mut stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "approved");
        assert_eq!(json["preview_sha256"], rec.preview_sha256);
        assert_eq!(json["exit_code_hint"], 0);
    }

    #[test]
    fn run_plan_approve_tty_multiline_paste_refused_no_bypass() {
        // SAFETY: even on a TTY, a pasted blob whose first line is a valid
        // approval but which carries a trailing command line must be refused
        // (embedded newline → strict parser), so single-line reading can never
        // be abused to approve-and-leak the remainder to the shell.
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("apply {} {}\nid\n", rec.step_id, rec.preview_sha256);
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(
            json["reason"].as_str().unwrap(),
            "control-chars-in-approval"
        );
    }

    #[test]
    fn run_plan_approve_batch_syntax_refuses() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!(
            "apply {} {} ; apply {} {}\n",
            rec.step_id, rec.preview_sha256, rec.step_id, rec.preview_sha256
        );
        let (code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        // Slice-3a refuses batch syntax via `BatchSyntax`.
        assert_eq!(json["reason"].as_str().unwrap(), "batch-syntax-in-approval");
    }

    #[test]
    fn run_plan_approve_preapproval_token_refuses() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        // Operator tries to slip in a preapproval token. The Slice-3a
        // parser must refuse with `Preapproval`.
        let approval = b"apply --yes --auto\n";
        let (code, stdout, _stderr) = run_with(&path, true, approval);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let json = parse_stdout_json(&stdout);
        assert_eq!(json["decision"], "refused");
        assert_eq!(json["reason"].as_str().unwrap(), "preapproval-refused");
    }

    #[test]
    fn run_plan_approve_emits_exactly_one_json_line_on_stdout() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (_code, stdout, _stderr) = run_with(&path, true, approval.as_bytes());
        let text = std::str::from_utf8(&stdout).unwrap();
        // exactly one trailing newline ⇒ exactly one JSON object on stdout
        let trimmed = text.trim_end_matches('\n');
        assert!(
            !trimmed.contains('\n'),
            "stdout should contain a single JSON line, got: {text:?}"
        );
        let _: Value = serde_json::from_str(trimmed).expect("stdout single json value");
    }

    #[test]
    fn run_plan_approve_writes_prompt_to_stderr_not_stdout() {
        let dir = unique_temp_dir();
        let (rec, disp) = build_bound_pair();
        let path = write_bundle_json(&dir, &rec, &disp, true);
        let approval = format!("apply {} {}\n", rec.step_id, rec.preview_sha256);
        let (_code, stdout, stderr) = run_with(&path, true, approval.as_bytes());
        let err = std::str::from_utf8(&stderr).unwrap();
        let out = std::str::from_utf8(&stdout).unwrap();
        // The Slice-3b operator prompt header lives on stderr.
        assert!(
            err.contains("A2-L2b approval required"),
            "approval prompt must surface on stderr; stderr was {err:?}"
        );
        // Stdout carries only the structured JSON.
        assert!(
            !out.contains("A2-L2b approval required"),
            "approval prompt must NOT bleed onto stdout; stdout was {out:?}"
        );
    }

    // -- Source-grep tests: no auto-approve, no broker, no run_plan wiring

    fn approve_section(src: &str) -> &str {
        let start_marker = "// A2-L2b Slice 3d — `claw plan approve <preview-bundle.json>` command";
        let end_marker = "// END A2-L2b Slice 3d — scope sentinel";
        let start = src
            .find(start_marker)
            .expect("approve section start marker");
        let end = src[start..]
            .find(end_marker)
            .expect("approve section end sentinel must follow start marker");
        &src[start..start + end]
    }

    #[test]
    fn no_yes_flag_in_approve_subcommand_parser() {
        let src = include_str!("main.rs");
        // The only references to "--yes" / "--auto" / "auto-apply" in
        // the approve section must be REJECTIONS (the test fixtures
        // above) or rejection error messages. There must be no parsing
        // arm that ACCEPTS them.
        let section = approve_section(src);
        for forbidden in [
            "\"--yes\" =>",
            "\"--auto\" =>",
            "\"--force\" =>",
            "\"--allow-write\" =>",
            "\"--apply\" =>",
            "auto_approve",
            "preapproved",
            "PreApproved",
        ] {
            assert!(
                !section.contains(forbidden),
                "approve section must not contain accept-arm `{forbidden}`"
            );
        }
    }

    #[test]
    fn approve_section_has_no_target_write_apis() {
        let src = include_str!("main.rs");
        let section = approve_section(src);
        for forbidden in [
            "File::create",
            "OpenOptions",
            "std::fs::write",
            "fs::write(",
            ".write_all(",
            "fs::rename",
            "remove_file",
            "remove_dir",
            "fs::create_dir",
            "fs::create_dir_all",
        ] {
            assert!(
                !section.contains(forbidden),
                "approve section must not call `{forbidden}` (workspace-write executor is out of scope)"
            );
        }
    }

    #[test]
    fn approve_section_has_no_broker_or_network_calls() {
        let src = include_str!("main.rs");
        let section = approve_section(src);
        for forbidden in [
            "reqwest",
            "http://",
            "https://",
            "11434",
            "11435",
            "Command::new",
            ".spawn(",
            "Ollama",
            "vram-broker",
            "broker.py",
            "OPENAI_BASE_URL",
            "SideStackAI",
            "sidestackai",
        ] {
            assert!(
                !section.contains(forbidden),
                "approve section must not reference `{forbidden}` (no broker / model / subprocess)"
            );
        }
    }

    #[test]
    fn approve_section_does_not_wire_into_run_plan() {
        let src = include_str!("main.rs");
        let section = approve_section(src);
        // The slice MUST NOT wire workspace-write into `run_plan` or
        // any new write executor entry point. Surface symbols that
        // would indicate such wiring.
        for forbidden in [
            "run_plan(",
            "run_plan_subcommand(",
            "WorkspaceWrite",
            "write_runtime",
            "CheckpointStore",
            "execute_write",
            "apply_diff(",
        ] {
            assert!(
                !section.contains(forbidden),
                "approve section must not wire into `{forbidden}`"
            );
        }
    }

    #[test]
    fn approve_section_does_not_introduce_deep_path() {
        let src = include_str!("main.rs");
        let section = approve_section(src);
        for forbidden in ["DEEP", "model_tier", "claude-opus", "claude-sonnet"] {
            assert!(
                !section.contains(forbidden),
                "approve section must not reference `{forbidden}` (read-only / no model selection)"
            );
        }
    }

    // -- Misc emitter coverage -------------------------------------------

    #[test]
    fn emit_bundle_load_failure_envelope_shape() {
        let mut out: Vec<u8> = Vec::new();
        let code = emit_bundle_load_failure(&BundleLoadError::Io("nope".to_string()), &mut out);
        assert_eq!(code, EXIT_BUNDLE_PARSE_ERROR);
        let v: Value = serde_json::from_str(std::str::from_utf8(&out).unwrap().trim_end()).unwrap();
        assert_eq!(v["schema_version"], APPROVAL_RESULT_SCHEMA_V1);
        assert_eq!(v["decision"], "bundle_rejected");
        assert_eq!(v["exit_code_hint"], EXIT_BUNDLE_PARSE_ERROR);
        let markers = v["audit_markers"].as_array().unwrap();
        assert!(markers
            .iter()
            .any(|m| m.as_str() == Some(APPROVAL_RESULT_AUDIT_BUNDLE_REJECTED)));
    }

    #[test]
    fn emit_non_tty_refusal_envelope_shape() {
        let (rec, _disp) = build_bound_pair();
        let mut out: Vec<u8> = Vec::new();
        let code = emit_non_tty_refusal(&rec, true, "approval-stdin-not-tty", &mut out);
        assert_eq!(code, EXIT_APPROVAL_DENIED);
        let v: Value = serde_json::from_str(std::str::from_utf8(&out).unwrap().trim_end()).unwrap();
        assert_eq!(v["decision"], "refused");
        assert_eq!(v["reason"], "approval-stdin-not-tty");
        assert_eq!(v["preview_id"], rec.preview_id);
        assert_eq!(v["exit_code_hint"], EXIT_APPROVAL_DENIED);
    }

    #[test]
    fn emit_approval_result_approved_envelope() {
        let (rec, disp) = build_bound_pair();
        let mut stdin =
            Cursor::new(format!("apply {} {}\n", rec.step_id, rec.preview_sha256).into_bytes());
        let mut stderr_buf: Vec<u8> = Vec::new();
        let interaction = run_approval_interaction(&rec, &disp, true, &mut stdin, &mut stderr_buf)
            .expect("interaction ok");
        let mut out: Vec<u8> = Vec::new();
        let code = emit_approval_result(&rec, &interaction, true, &mut out);
        assert_eq!(code, 0);
        let v: Value = serde_json::from_str(std::str::from_utf8(&out).unwrap().trim_end()).unwrap();
        assert_eq!(v["decision"], "approved");
        assert!(
            v.get("reason").is_none(),
            "approved envelopes have no reason field"
        );
    }

    #[test]
    fn cli_apply_keyword_in_approve_section_is_only_string_literal() {
        // The Slice-3a parser is the only authority over the `apply`
        // keyword. The approve section MUST NOT introduce a second
        // keyword-acceptance code path.
        let src = include_str!("main.rs");
        let section = approve_section(src);
        // No additional "fn parse_apply" / "fn match_apply" helpers.
        assert!(!section.contains("fn parse_apply"));
        assert!(!section.contains("fn match_apply"));
        assert!(!section.contains("fn accept_apply"));
    }
}
