//! Regression matrix for the default-tool non-termination revision.
//!
//! Descends from the audit reproducer (`default_tool_nontermination_repro.rs`,
//! audit worktree `stack-code-default-tool-loop-audit-20260910_071544`) and
//! from the first repair candidate, which an independent review rejected
//! (`STACK_CODE_DEFAULT_TOOL_REPAIR_INDEPENDENT_REVIEW_DO_NOT_MERGE`).
//!
//! Everything here runs in-process: the provider is a scripted local fake and
//! the tool side uses the REAL `tools::execute_tool` dispatch, so the guard is
//! proven against the actual `SendUserMessage`/`Brief` implementation rather
//! than a restatement of it. No network, broker, Ollama, GPU, or model work.
//!
//! Scope note: this file covers NON-TERMINATION only. `SendUserMessage` still
//! does not render anything to the operator; that defect is deferred under
//! `SENDUSERMESSAGE_DELIVERY_SEMANTICS_UNRESOLVED` and
//! `duplicate_notice_makes_no_delivery_claim` pins the wording so the repair
//! cannot be mistaken for a delivery fix.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationRuntime, HookAbortSignal,
    HookEvent, HookProgressEvent, HookProgressReporter, PermissionMode, PermissionPolicy,
    PermissionPromptDecision, PermissionPrompter, PermissionRequest, RuntimeFeatureConfig,
    RuntimeHookConfig, Session, ToolError, ToolExecutor,
};
use serde_json::{json, Value};

/// Mirrors `DEFAULT_CLI_MAX_ITERATIONS` in `rusty-claude-cli/src/main.rs`.
/// The CLI builder wiring itself is proven by the unit test in that crate;
/// this constant only lets the runtime-level tests run at the same ceiling.
const CLI_EQUIVALENT_MAX_ITERATIONS: usize = 32;

const DUPLICATE_MARKER: &str = "Duplicate SendUserMessage call suppressed";
const HARD_FAIL_MARKER: &str = "repeated SendUserMessage";
const MAX_ITERATIONS_MARKER: &str = "conversation loop exceeded the maximum number of iterations";
/// Tool-result wording for the offending delivery call that ends the turn.
const OFFENDER_RESULT_MARKER: &str =
    "Repeated SendUserMessage delivery call after duplicate suppression";
/// Tool-result wording for a tool call that shared the aborted assistant
/// response and was closed out without ever being reached.
const ABORTED_SIBLING_MARKER: &str =
    "Tool not executed because the current assistant turn was aborted";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Provider that replays a fixed script and records how many times it was
/// asked. Past the end of the script it errors, so an unexpected extra
/// provider round-trip fails the test loudly instead of hanging.
struct ScriptedApi {
    script: Vec<Vec<AssistantEvent>>,
    calls: Rc<RefCell<usize>>,
}

impl ScriptedApi {
    fn new(script: Vec<Vec<AssistantEvent>>) -> (Self, Rc<RefCell<usize>>) {
        let calls = Rc::new(RefCell::new(0));
        (
            Self {
                script,
                calls: Rc::clone(&calls),
            },
            calls,
        )
    }
}

impl ApiClient for ScriptedApi {
    fn stream(
        &mut self,
        _request: ApiRequest,
    ) -> Result<Vec<AssistantEvent>, runtime::RuntimeError> {
        let index = *self.calls.borrow();
        *self.calls.borrow_mut() = index + 1;
        self.script.get(index).cloned().ok_or_else(|| {
            runtime::RuntimeError::new(format!(
                "scripted provider exhausted: unexpected provider call #{}",
                index + 1
            ))
        })
    }
}

/// Provider that checks the history it is handed before answering: every
/// `ToolUse` in the request must be answered by exactly one `ToolResult`.
/// This is what a real provider enforces, so it is what proves a persisted
/// session is genuinely resumable.
struct ProtocolValidatingApi {
    script: Vec<Vec<AssistantEvent>>,
    calls: usize,
    seen: Rc<RefCell<Vec<bool>>>,
}

impl ProtocolValidatingApi {
    fn new(script: Vec<Vec<AssistantEvent>>) -> (Self, Rc<RefCell<Vec<bool>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                script,
                calls: 0,
                seen: Rc::clone(&seen),
            },
            seen,
        )
    }
}

impl ApiClient for ProtocolValidatingApi {
    fn stream(
        &mut self,
        request: ApiRequest,
    ) -> Result<Vec<AssistantEvent>, runtime::RuntimeError> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for block in request
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
        {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    counts.entry(id.clone()).or_insert(0);
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    *counts.entry(tool_use_id.clone()).or_insert(0) += 1;
                }
                ContentBlock::Text { .. } => {}
            }
        }
        let well_formed = counts.values().all(|count| *count == 1);
        self.seen.borrow_mut().push(well_formed);
        if !well_formed {
            return Err(runtime::RuntimeError::new(format!(
                "provider rejected a malformed history: {counts:?}"
            )));
        }
        let index = self.calls;
        self.calls += 1;
        self.script.get(index).cloned().ok_or_else(|| {
            runtime::RuntimeError::new(format!(
                "validating provider exhausted: unexpected provider call #{}",
                index + 1
            ))
        })
    }
}

/// Tool executor that records every call it is handed and routes the delivery
/// family through the REAL `tools::execute_tool` dispatcher. `Ping` stands in
/// for an ordinary, unrelated tool.
struct RealDispatchExecutor {
    log: Rc<RefCell<Vec<(String, String)>>>,
}

impl RealDispatchExecutor {
    fn new() -> (Self, Rc<RefCell<Vec<(String, String)>>>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                log: Rc::clone(&log),
            },
            log,
        )
    }
}

impl ToolExecutor for RealDispatchExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        self.log
            .borrow_mut()
            .push((tool_name.to_string(), input.to_string()));
        match tool_name {
            "SendUserMessage" | "Brief" => {
                let value: Value = serde_json::from_str(input)
                    .map_err(|error| ToolError::new(format!("invalid tool input: {error}")))?;
                tools::execute_tool(tool_name, &value).map_err(ToolError::new)
            }
            "Ping" => Ok(String::from("pong")),
            other => Err(ToolError::new(format!("unknown tool: {other}"))),
        }
    }
}

/// Counts `PreToolUse` hook invocations as observed by the runtime's own
/// progress reporting, without depending on hook stdout formatting.
struct PreToolUseCounter {
    starts: Rc<RefCell<usize>>,
}

impl HookProgressReporter for PreToolUseCounter {
    fn on_event(&mut self, event: &HookProgressEvent) {
        if let HookProgressEvent::Started { event: kind, .. } = event {
            if *kind == HookEvent::PreToolUse {
                *self.starts.borrow_mut() += 1;
            }
        }
    }
}

/// Permission prompt harness that replays a fixed decision list and records
/// every request it was shown.
struct ScriptedPrompter {
    decisions: Vec<PermissionPromptDecision>,
    seen: Vec<PermissionRequest>,
}

impl PermissionPrompter for ScriptedPrompter {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
        self.seen.push(request.clone());
        self.decisions
            .get(self.seen.len() - 1)
            .cloned()
            .unwrap_or(PermissionPromptDecision::Allow)
    }
}

fn tool_use(id: &str, name: &str, input: &str) -> AssistantEvent {
    AssistantEvent::ToolUse {
        id: id.to_string(),
        name: name.to_string(),
        input: input.to_string(),
    }
}

fn delivery(id: &str, input: &str) -> AssistantEvent {
    tool_use(id, "SendUserMessage", input)
}

fn turn(events: Vec<AssistantEvent>) -> Vec<AssistantEvent> {
    let mut all = events;
    all.push(AssistantEvent::MessageStop);
    all
}

fn text_turn() -> Vec<AssistantEvent> {
    turn(vec![AssistantEvent::TextDelta("done".to_string())])
}

fn normal(message: &str) -> String {
    json!({ "message": message, "status": "normal" }).to_string()
}

fn proactive(message: &str) -> String {
    json!({ "message": message, "status": "proactive" }).to_string()
}

type Harness = (
    ConversationRuntime<ScriptedApi, RealDispatchExecutor>,
    Rc<RefCell<usize>>,
    Rc<RefCell<Vec<(String, String)>>>,
);

fn harness_with_policy(script: Vec<Vec<AssistantEvent>>, policy: PermissionPolicy) -> Harness {
    let (api, calls) = ScriptedApi::new(script);
    let (executor, log) = RealDispatchExecutor::new();
    let runtime = ConversationRuntime::new(
        Session::new(),
        api,
        executor,
        policy,
        vec!["system".to_string()],
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);
    (runtime, calls, log)
}

fn harness(script: Vec<Vec<AssistantEvent>>) -> Harness {
    harness_with_policy(
        script,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
    )
}

/// Every tool result recorded on the session, in order, as
/// `(tool_name, output, is_error)`. Read from the session rather than the
/// turn summary so hard-failing turns are still observable.
fn tool_results(session: &Session) -> Vec<(String, String, bool)> {
    session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_name,
                output,
                is_error,
                ..
            } => Some((tool_name.clone(), output.clone(), *is_error)),
            _ => None,
        })
        .collect()
}

fn delivery_executions(log: &Rc<RefCell<Vec<(String, String)>>>) -> usize {
    log.borrow()
        .iter()
        .filter(|(name, _)| name == "SendUserMessage" || name == "Brief")
        .count()
}

fn duplicate_notices(session: &Session) -> usize {
    tool_results(session)
        .iter()
        .filter(|(_, output, is_error)| *is_error && output.contains(DUPLICATE_MARKER))
        .count()
}

/// Unique scratch path for hook-state files. These are a few bytes each and
/// are removed by the test that created them.
fn scratch_path(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "stack_code_delivery_guard_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the unix epoch")
            .as_nanos()
    ));
    path
}

fn hooks_config(pre_tool_use: Vec<String>) -> RuntimeFeatureConfig {
    RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
        pre_tool_use,
        Vec::new(),
        Vec::new(),
    ))
}

/// Every `ToolUse` recorded on the session must have a matching `ToolResult`.
/// A dangling tool use makes the session unresumable, because the provider
/// rejects an assistant tool call that was never answered.
fn assert_no_dangling_tool_use(session: &Session) {
    let mut used = Vec::new();
    let mut answered = Vec::new();
    for block in session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
    {
        match block {
            ContentBlock::ToolUse { id, .. } => used.push(id.clone()),
            ContentBlock::ToolResult { tool_use_id, .. } => answered.push(tool_use_id.clone()),
            ContentBlock::Text { .. } => {}
        }
    }
    for id in &used {
        assert!(
            answered.contains(id),
            "tool use {id:?} has no tool result; session would be unresumable"
        );
    }
}

/// Every `ToolUse` id on the session mapped to the number of `ToolResult`s
/// that answer it. Counting by id, not by tool name, is what makes protocol
/// completeness provable: two calls to the same tool are two obligations.
fn result_counts_by_tool_use_id(session: &Session) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for block in session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
    {
        match block {
            ContentBlock::ToolUse { id, .. } => {
                counts.entry(id.clone()).or_insert(0);
            }
            ContentBlock::ToolResult { tool_use_id, .. } => {
                *counts.entry(tool_use_id.clone()).or_insert(0) += 1;
            }
            ContentBlock::Text { .. } => {}
        }
    }
    counts
}

/// Exactly one result per tool use. `>= 1` is not good enough: a duplicated
/// result is as malformed as a missing one.
fn assert_exactly_one_result_per_tool_use(session: &Session) {
    for (id, count) in result_counts_by_tool_use_id(session) {
        assert_eq!(
            count, 1,
            "tool use {id:?} has {count} tool results; exactly one is required"
        );
    }
}

fn executions_of(log: &Rc<RefCell<Vec<(String, String)>>>, tool_name: &str) -> usize {
    log.borrow()
        .iter()
        .filter(|(name, _)| name == tool_name)
        .count()
}

fn total_executions(log: &Rc<RefCell<Vec<(String, String)>>>) -> usize {
    log.borrow().len()
}

/// The tool result answering one specific tool use id.
fn result_for(session: &Session, tool_use_id: &str) -> (String, bool) {
    session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id: id,
                output,
                is_error,
                ..
            } if id == tool_use_id => Some((output.clone(), *is_error)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("tool use {tool_use_id:?} has no tool result"))
}

/// Counts hook invocations per `HookEvent` through the runtime's own progress
/// reporting, so the assertions do not depend on hook stdout formatting.
#[derive(Clone, Default)]
struct HookCounts {
    pre: Rc<RefCell<usize>>,
    post: Rc<RefCell<usize>>,
    post_failure: Rc<RefCell<usize>>,
}

impl HookCounts {
    fn reporter(&self) -> HookEventCounter {
        HookEventCounter {
            counts: self.clone(),
        }
    }

    fn pre(&self) -> usize {
        *self.pre.borrow()
    }

    fn post(&self) -> usize {
        *self.post.borrow()
    }

    fn post_failure(&self) -> usize {
        *self.post_failure.borrow()
    }
}

struct HookEventCounter {
    counts: HookCounts,
}

impl HookProgressReporter for HookEventCounter {
    fn on_event(&mut self, event: &HookProgressEvent) {
        if let HookProgressEvent::Started { event: kind, .. } = event {
            match kind {
                HookEvent::PreToolUse => *self.counts.pre.borrow_mut() += 1,
                HookEvent::PostToolUse => *self.counts.post.borrow_mut() += 1,
                HookEvent::PostToolUseFailure => *self.counts.post_failure.borrow_mut() += 1,
            }
        }
    }
}

fn all_hooks_config(
    pre_tool_use: Vec<String>,
    post_tool_use: Vec<String>,
    post_tool_use_failure: Vec<String>,
) -> RuntimeFeatureConfig {
    RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
        pre_tool_use,
        post_tool_use,
        post_tool_use_failure,
    ))
}

/// Script that ends with a stubborn repeat: deliver, get the corrective
/// result, then repeat in a later provider iteration. `tail` is appended to
/// that final, fail-closed assistant response.
fn fail_closed_script(tail: Vec<AssistantEvent>) -> Vec<Vec<AssistantEvent>> {
    let mut final_turn = vec![delivery("a3", &normal("hello"))];
    final_turn.extend(tail);
    vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        turn(final_turn),
    ]
}

// ---------------------------------------------------------------------------
// T1 / T2 - historical recovery and stubborn repeats
// ---------------------------------------------------------------------------

#[test]
fn t1_duplicate_in_a_later_iteration_is_suppressed_and_the_turn_recovers() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);

    let summary = runtime
        .run_turn("go", None)
        .expect("the turn should recover after one corrective result");

    assert_eq!(*calls.borrow(), 3, "provider should be asked three times");
    assert_eq!(delivery_executions(&log), 1, "delivery should run once");
    assert_eq!(duplicate_notices(runtime.session()), 1);
    let results = tool_results(runtime.session());
    assert!(results[1].2, "the duplicate result must be an error");
    assert!(summary
        .assistant_messages
        .last()
        .expect("a final assistant message")
        .blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { text } if text == "done")));
}

#[test]
fn t2_stubborn_repeat_in_a_later_iteration_fails_closed() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        turn(vec![delivery("a3", &normal("hello"))]),
    ]);

    let error = runtime
        .run_turn("go", None)
        .expect_err("a repeat after the corrective result must fail closed");

    assert!(
        error.to_string().contains(HARD_FAIL_MARKER),
        "unexpected error: {error}"
    );
    assert!(
        !error.to_string().contains(MAX_ITERATIONS_MARKER),
        "the duplicate hard failure must be distinguishable from budget exhaustion"
    );
    assert_eq!(*calls.borrow(), 3, "there must be no fourth provider call");
    assert_eq!(delivery_executions(&log), 1);
    assert_no_dangling_tool_use(runtime.session());
}

// ---------------------------------------------------------------------------
// T3 / T4 - same-response repetition must still get one corrective turn
// ---------------------------------------------------------------------------

#[test]
fn t3_three_identical_calls_in_one_response_are_suppressed_not_failed() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![
            delivery("a1", &normal("hello")),
            delivery("a2", &normal("hello")),
            delivery("a3", &normal("hello")),
        ]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("same-response repetition must not hard-fail before a corrective turn");

    assert_eq!(*calls.borrow(), 2, "the model must get one corrective turn");
    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(duplicate_notices(runtime.session()), 2);
    for (_, output, is_error) in tool_results(runtime.session()).iter().skip(1) {
        assert!(is_error, "each suppressed duplicate must be an error");
        assert!(output.contains(DUPLICATE_MARKER));
    }
}

#[test]
fn t4_same_response_repetition_then_a_stubborn_next_iteration_fails_closed() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![
            delivery("a1", &normal("hello")),
            delivery("a2", &normal("hello")),
            delivery("a3", &normal("hello")),
        ]),
        turn(vec![delivery("a4", &normal("hello"))]),
    ]);

    let error = runtime
        .run_turn("go", None)
        .expect_err("the corrective turn was consumed, so this must fail closed");

    assert!(error.to_string().contains(HARD_FAIL_MARKER));
    assert_eq!(*calls.borrow(), 2, "there must be no third provider call");
    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(duplicate_notices(runtime.session()), 2);
    assert_no_dangling_tool_use(runtime.session());
}

// ---------------------------------------------------------------------------
// T5 / T6 / T7 - repetition is a sequence, not whole-turn membership
// ---------------------------------------------------------------------------

#[test]
fn t5_intervening_delivery_resets_the_repetition_sequence() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("first"))]),
        turn(vec![delivery("b1", &normal("second"))]),
        turn(vec![delivery("a2", &normal("first"))]),
        text_turn(),
    ]);

    runtime.run_turn("go", None).expect("A/B/A is legitimate");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(delivery_executions(&log), 3, "all three must execute");
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t6_proactive_updates_follow_the_same_sequence_rule() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &proactive("step one"))]),
        turn(vec![delivery("b1", &proactive("step two"))]),
        turn(vec![delivery("a2", &proactive("step one"))]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("distinct proactive updates are legitimate");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(delivery_executions(&log), 3);
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t7_successful_ordinary_tool_progress_resets_a_stale_sequence() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![tool_use("p1", "Ping", "{}")]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("real work between deliveries is material progress");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(delivery_executions(&log), 2, "the later delivery must run");
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

// ---------------------------------------------------------------------------
// T8 / T9 / T10 / T11 - hook and permission ordering
// ---------------------------------------------------------------------------

#[test]
fn t8_pre_tool_use_rewrite_distinguishes_raw_identical_calls() {
    let counter = scratch_path("t8");
    let command = format!(
        "n=$(cat {path} 2>/dev/null || echo 0); n=$((n+1)); printf '%s' \"$n\" > {path}; \
         if [ \"$n\" -eq 2 ]; then \
           printf '%s' '{{\"hookSpecificOutput\":{{\"updatedInput\":{{\"message\":\"rewritten\",\"status\":\"normal\"}}}}}}'; \
         else \
           printf '%s' '{{}}'; \
         fi",
        path = counter.display()
    );

    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &hooks_config(vec![command]),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);

    runtime
        .run_turn("go", None)
        .expect("a hook rewrite makes the second call genuinely different");

    let hook_runs = fs::read_to_string(&counter).unwrap_or_default();
    let _ = fs::remove_file(&counter);

    assert_eq!(hook_runs, "2", "the hook must observe both attempts");
    assert_eq!(*calls.borrow(), 3);
    assert_eq!(delivery_executions(&log), 2, "both effective inputs differ");
    assert_eq!(duplicate_notices(runtime.session()), 0);
    let executed = log.borrow();
    assert!(executed[0].1.contains("hello"));
    assert!(
        executed[1].1.contains("rewritten"),
        "the guard must key on the effective input: {:?}",
        executed[1].1
    );
}

#[test]
fn t9_suppressed_duplicates_still_pass_through_pre_tool_use() {
    let starts = Rc::new(RefCell::new(0usize));
    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &hooks_config(vec!["exit 0".to_string()]),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS)
    .with_hook_progress_reporter(Box::new(PreToolUseCounter {
        starts: Rc::clone(&starts),
    }));

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        *starts.borrow(),
        2,
        "PreToolUse must observe the duplicate attempt too"
    );
    assert_eq!(delivery_executions(&log), 1, "but delivery runs once");
}

#[test]
fn t10_permission_processing_precedes_duplicate_containment() {
    let policy = PermissionPolicy::new(PermissionMode::Prompt)
        .with_tool_requirement("SendUserMessage", PermissionMode::Allow);
    let (mut runtime, calls, log) = harness_with_policy(
        vec![
            turn(vec![delivery("a1", &normal("hello"))]),
            turn(vec![delivery("a2", &normal("hello"))]),
            turn(vec![delivery("a3", &normal("hello"))]),
            text_turn(),
        ],
        policy,
    );
    let mut prompter = ScriptedPrompter {
        decisions: vec![
            PermissionPromptDecision::Allow,
            PermissionPromptDecision::Deny {
                reason: "operator refused this attempt".to_string(),
            },
            PermissionPromptDecision::Allow,
        ],
        seen: Vec::new(),
    };

    runtime
        .run_turn("go", Some(&mut prompter))
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(
        prompter.seen.len(),
        3,
        "every attempt must reach permission processing"
    );
    assert_eq!(delivery_executions(&log), 1);

    let results = tool_results(runtime.session());
    assert!(
        results[1].1.contains("operator refused this attempt"),
        "the denial must be preserved, not replaced by containment: {:?}",
        results[1].1
    );
    assert!(
        !results[1].1.contains(DUPLICATE_MARKER),
        "a denied attempt is not a suppressed duplicate"
    );
    assert!(
        results[2].1.contains(DUPLICATE_MARKER),
        "the denied attempt must not be recorded as a successful delivery: {:?}",
        results[2].1
    );
}

#[test]
fn t11_pre_tool_use_denial_is_not_bypassed_by_containment() {
    let counter = scratch_path("t11");
    let command = format!(
        "n=$(cat {path} 2>/dev/null || echo 0); n=$((n+1)); printf '%s' \"$n\" > {path}; \
         if [ \"$n\" -eq 2 ]; then exit 2; fi",
        path = counter.display()
    );

    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &hooks_config(vec![command]),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");
    let _ = fs::remove_file(&counter);

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(delivery_executions(&log), 1);
    let results = tool_results(runtime.session());
    assert!(results[1].2, "the hook denial is an error result");
    assert!(
        results[1].1.contains("denied"),
        "the hook denial must survive containment: {:?}",
        results[1].1
    );
    assert!(
        !results[1].1.contains(DUPLICATE_MARKER),
        "containment must not replace a hook denial"
    );
}

// ---------------------------------------------------------------------------
// T12 / T13 / T14 - malformed and invalid payloads keep ordinary validation
// ---------------------------------------------------------------------------

#[test]
fn t12_repeated_malformed_json_is_never_suppressed() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", "not json at all")]),
        turn(vec![delivery("a2", "not json at all")]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        delivery_executions(&log),
        2,
        "malformed input must reach ordinary validation each time"
    );
    assert_eq!(duplicate_notices(runtime.session()), 0);
    for (_, _, is_error) in tool_results(runtime.session()) {
        assert!(is_error, "malformed input still errors");
    }
}

#[test]
fn t13_repeated_missing_status_is_never_suppressed() {
    let payload = json!({ "message": "hello" }).to_string();
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &payload), delivery("a2", &payload)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(delivery_executions(&log), 2);
    assert_eq!(duplicate_notices(runtime.session()), 0);
    let results = tool_results(runtime.session());
    assert!(
        results[0].1.contains("status"),
        "the dispatcher's own validation error must surface: {:?}",
        results[0].1
    );
}

#[test]
fn t14_null_numeric_and_unknown_statuses_keep_ordinary_validation() {
    let null_status = json!({ "message": "hello", "status": Value::Null }).to_string();
    let numeric_status = json!({ "message": "hello", "status": 7 }).to_string();
    let unknown_status = json!({ "message": "hello", "status": "urgent" }).to_string();
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![
            delivery("a1", &null_status),
            delivery("a2", &null_status),
            delivery("a3", &numeric_status),
            delivery("a4", &numeric_status),
            delivery("a5", &unknown_status),
            delivery("a6", &unknown_status),
        ]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 2);
    assert_eq!(
        delivery_executions(&log),
        6,
        "no invalid status may be classified as a duplicate"
    );
    assert_eq!(duplicate_notices(runtime.session()), 0);
    for (_, _, is_error) in tool_results(runtime.session()) {
        assert!(is_error);
    }
}

#[test]
fn t14b_repeated_blank_message_keeps_ordinary_validation() {
    let payload = json!({ "message": "   ", "status": "normal" }).to_string();
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &payload), delivery("a2", &payload)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(
        delivery_executions(&log),
        2,
        "a blank message never executes, so it is never a duplicate"
    );
    assert_eq!(duplicate_notices(runtime.session()), 0);
    let results = tool_results(runtime.session());
    assert!(
        results[0].1.contains("message must not be empty"),
        "expected execute_brief's own error: {:?}",
        results[0].1
    );
}

// ---------------------------------------------------------------------------
// T15 / T16 - attachment equivalence must match real dispatch semantics
// ---------------------------------------------------------------------------

#[test]
fn t15_omitted_and_null_attachments_are_the_same_delivery() {
    let omitted = json!({ "message": "hello", "status": "normal" }).to_string();
    let explicit_null =
        json!({ "message": "hello", "status": "normal", "attachments": Value::Null }).to_string();
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &omitted)]),
        turn(vec![delivery("a2", &explicit_null)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(
        delivery_executions(&log),
        1,
        "omitted and null attachments both deserialize to None"
    );
    assert_eq!(duplicate_notices(runtime.session()), 1);
}

#[test]
fn t16_empty_attachments_is_distinct_from_absent_attachments() {
    // First pin the real dispatcher semantics rather than assuming them.
    let omitted_output = tools::execute_tool(
        "SendUserMessage",
        &json!({ "message": "hello", "status": "normal" }),
    )
    .expect("omitted attachments should succeed");
    let empty_output = tools::execute_tool(
        "SendUserMessage",
        &json!({ "message": "hello", "status": "normal", "attachments": [] }),
    )
    .expect("empty attachments should succeed");
    let omitted: Value = serde_json::from_str(&omitted_output).expect("valid json");
    let empty: Value = serde_json::from_str(&empty_output).expect("valid json");
    assert_eq!(omitted.get("attachments"), Some(&Value::Null));
    assert_eq!(empty.get("attachments"), Some(&json!([])));

    // The key must therefore keep them apart.
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery(
            "a1",
            &json!({ "message": "hello", "status": "normal" }).to_string(),
        )]),
        turn(vec![delivery(
            "a2",
            &json!({ "message": "hello", "status": "normal", "attachments": [] }).to_string(),
        )]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(delivery_executions(&log), 2, "these are different calls");
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

// ---------------------------------------------------------------------------
// T17 / T18 / T19 - key representation
// ---------------------------------------------------------------------------

#[test]
fn t17_unit_separator_in_a_message_cannot_forge_a_collision() {
    // Under a delimiter-concatenated fingerprint these two produce the same
    // string: the first is (message="hello", status="normal") and the second
    // smuggles the separator and status into the message while omitting
    // `status` entirely. The second is invalid at dispatch and must not be
    // classified as a duplicate of the first.
    let valid = json!({ "message": "hello", "status": "normal" }).to_string();
    let forged = json!({ "message": "hello\u{001f}normal" }).to_string();
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &valid), delivery("a2", &forged)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(delivery_executions(&log), 2, "no forged collision");
    assert_eq!(duplicate_notices(runtime.session()), 0);
    let results = tool_results(runtime.session());
    assert!(!results[0].2, "the valid call succeeds");
    assert!(results[1].2, "the forged call fails ordinary validation");
}

#[test]
fn t17b_distinct_valid_messages_containing_a_unit_separator_stay_distinct() {
    let first = json!({ "message": "alpha\u{001f}one", "status": "normal" }).to_string();
    let second = json!({ "message": "alpha\u{001f}two", "status": "normal" }).to_string();
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &first), delivery("a2", &second)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(delivery_executions(&log), 2);
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t18_key_order_and_whitespace_do_not_defeat_detection() {
    let first = r#"{"message":"hello","status":"normal"}"#;
    let second = "  {\n  \"status\" : \"normal\" ,\n  \"message\" : \"hello\"\n}  ";
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", first)]),
        turn(vec![delivery("a2", second)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(duplicate_notices(runtime.session()), 1);
}

#[test]
fn t19_brief_alias_shares_the_delivery_family() {
    let payload = normal("hello");
    let (mut runtime, _calls, log) = harness(vec![
        turn(vec![delivery("a1", &payload)]),
        turn(vec![tool_use("a2", "Brief", &payload)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(
        delivery_executions(&log),
        1,
        "`Brief` routes to the same implementation, so it is the same delivery"
    );
    assert_eq!(duplicate_notices(runtime.session()), 1);
}

// ---------------------------------------------------------------------------
// T20 / T21 / T22 / T23 - scoping and unrelated behavior
// ---------------------------------------------------------------------------

#[test]
fn t20_duplicate_state_does_not_leak_across_user_turns() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        text_turn(),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);

    runtime.run_turn("first", None).expect("first turn");
    runtime.run_turn("second", None).expect("second turn");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(
        delivery_executions(&log),
        2,
        "each user turn starts with a clean sequence"
    );
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t21_identical_ordinary_tool_calls_are_unaffected() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![tool_use("p1", "Ping", "{}")]),
        turn(vec![tool_use("p2", "Ping", "{}")]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(log.borrow().len(), 2, "both ordinary calls execute");
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t22_text_plus_ordinary_tool_remains_non_terminal() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![
            AssistantEvent::TextDelta("thinking".to_string()),
            tool_use("p1", "Ping", "{}"),
        ]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 2);
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn t23_text_plus_delivery_keeps_current_non_terminal_semantics() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![
            AssistantEvent::TextDelta("thinking".to_string()),
            delivery("a1", &normal("hello")),
        ]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(
        *calls.borrow(),
        2,
        "SendUserMessage must not become terminal just because text exists"
    );
    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

// ---------------------------------------------------------------------------
// T24 / T25 - finite budget and truthful notice
// ---------------------------------------------------------------------------

#[test]
fn t24_varied_delivery_loop_stops_at_the_configured_bound() {
    let script = (0..CLI_EQUIVALENT_MAX_ITERATIONS)
        .map(|index| {
            turn(vec![delivery(
                &format!("a{index}"),
                &normal(&format!("update {index}")),
            )])
        })
        .collect::<Vec<_>>();
    let (mut runtime, calls, log) = harness(script);

    let error = runtime
        .run_turn("go", None)
        .expect_err("a varied delivery loop must still terminate");

    assert!(
        error.to_string().contains(MAX_ITERATIONS_MARKER),
        "unexpected error: {error}"
    );
    assert!(
        !error.to_string().contains(HARD_FAIL_MARKER),
        "budget exhaustion must be distinguishable from the duplicate hard failure"
    );
    assert_eq!(*calls.borrow(), CLI_EQUIVALENT_MAX_ITERATIONS);
    assert_eq!(delivery_executions(&log), CLI_EQUIVALENT_MAX_ITERATIONS);
    assert_eq!(duplicate_notices(runtime.session()), 0);
}

#[test]
fn t25_duplicate_notice_makes_no_delivery_claim() {
    let (mut runtime, _calls, _log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    let results = tool_results(runtime.session());
    let notice = results[1].1.to_lowercase();
    for claim in [
        "already delivered",
        "successfully delivered",
        "delivered to the user",
        "already sent",
        "sent successfully",
        "message was sent",
    ] {
        assert!(
            !notice.contains(claim),
            "the notice must not claim delivery: found {claim:?} in {notice:?}"
        );
    }
    assert!(
        notice.contains("was not executed again"),
        "the notice must say the tool did not run: {notice:?}"
    );
    assert!(
        notice.contains("ordinary assistant text"),
        "the notice must redirect to plain assistant text: {notice:?}"
    );
}

/// The deferred defect this lane deliberately does NOT fix: a successful
/// `SendUserMessage` result still only echoes its input.
/// Marker: SENDUSERMESSAGE_DELIVERY_SEMANTICS_UNRESOLVED
#[test]
fn send_user_message_result_still_does_not_prove_delivery() {
    let output = tools::execute_tool(
        "SendUserMessage",
        &json!({ "message": "hello", "status": "normal" }),
    )
    .expect("delivery should succeed");
    let parsed: Value = serde_json::from_str(&output).expect("valid json");
    assert_eq!(parsed.get("message"), Some(&json!("hello")));
    assert!(
        parsed.get("deliveredTo").is_none(),
        "no delivery evidence exists yet"
    );
}

// ---------------------------------------------------------------------------
// Revision 2 - the fail-closed abort must leave a protocol-complete session
//
// The rejected revision answered only the offending tool use and returned
// immediately, so any later tool use in the SAME, already persisted assistant
// message was left unanswered and the session became unresumable.
// Marker: HARD_FAIL_PENDING_TOOL_PROTOCOL_GAP_REPRODUCED
// ---------------------------------------------------------------------------

/// R2-T1 - `[fatal A, Ping]`.
#[test]
fn r2_t1_fatal_delivery_closes_out_its_ordinary_sibling() {
    let (mut runtime, calls, log) = harness(fail_closed_script(vec![tool_use("p1", "Ping", "{}")]));

    let error = runtime
        .run_turn("go", None)
        .expect_err("a repeat after the corrective result must fail closed");

    assert!(
        error.to_string().contains(HARD_FAIL_MARKER),
        "unexpected error: {error}"
    );
    assert_eq!(
        *calls.borrow(),
        3,
        "the abort must not request another provider turn"
    );
    assert_eq!(delivery_executions(&log), 1, "only the first delivery ran");
    assert_eq!(
        executions_of(&log, "Ping"),
        0,
        "no tool may execute after the fail-closed decision"
    );

    let counts = result_counts_by_tool_use_id(runtime.session());
    assert_eq!(counts.get("a3"), Some(&1), "counts: {counts:?}");
    assert_eq!(counts.get("p1"), Some(&1), "counts: {counts:?}");
    assert_exactly_one_result_per_tool_use(runtime.session());
    assert_no_dangling_tool_use(runtime.session());

    let (offender, offender_error) = result_for(runtime.session(), "a3");
    assert!(offender_error, "the offending call is an error result");
    assert!(
        offender.contains(OFFENDER_RESULT_MARKER),
        "unexpected offender result: {offender:?}"
    );
    let (sibling, sibling_error) = result_for(runtime.session(), "p1");
    assert!(sibling_error, "the closed-out sibling is an error result");
    assert!(
        sibling.contains(ABORTED_SIBLING_MARKER),
        "unexpected sibling result: {sibling:?}"
    );
    assert!(
        !sibling.contains("pong"),
        "the sibling must not be reported as if it had run: {sibling:?}"
    );
}

/// R2-T2 - `[ordinary prefix, fatal A]`.
///
/// The prefix sibling deliberately FAILS. A prefix tool that succeeded would
/// be material progress and would legitimately reset the repetition sequence
/// (the accepted F3 semantics pinned by `t7`), so the delivery after it would
/// not be fatal at all. A failing ordinary tool still proves the ordering
/// property under test: a sibling processed BEFORE the runtime knows the
/// delivery will fail closed keeps its ordinary sequential behavior, executes
/// exactly once, and is not retroactively undone.
#[test]
fn r2_t2_ordinary_prefix_keeps_its_ordinary_behavior_before_the_fatal_call() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        turn(vec![
            tool_use("u1", "Unsupported", "{}"),
            delivery("a3", &normal("hello")),
        ]),
    ]);

    let error = runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must still fail closed");

    assert!(error.to_string().contains(HARD_FAIL_MARKER));
    assert_eq!(*calls.borrow(), 3, "no further provider call");
    assert_eq!(
        executions_of(&log, "Unsupported"),
        1,
        "the prefix sibling must execute exactly once"
    );
    assert_eq!(delivery_executions(&log), 1, "the repeat never executes");

    let counts = result_counts_by_tool_use_id(runtime.session());
    assert_eq!(counts.get("u1"), Some(&1), "counts: {counts:?}");
    assert_eq!(counts.get("a3"), Some(&1), "counts: {counts:?}");
    assert_exactly_one_result_per_tool_use(runtime.session());
    assert_no_dangling_tool_use(runtime.session());

    let (prefix, prefix_error) = result_for(runtime.session(), "u1");
    assert!(prefix_error, "the prefix tool really did fail");
    assert!(
        !prefix.contains(ABORTED_SIBLING_MARKER),
        "a prefix sibling is not an aborted-tail sibling: {prefix:?}"
    );
    let (offender, _) = result_for(runtime.session(), "a3");
    assert!(offender.contains(OFFENDER_RESULT_MARKER));
}

/// R2-T2b - the companion case: a prefix tool that really SUCCEEDS is
/// material progress, so the delivery after it is legal and the turn
/// continues. Pins that the abort-drain did not make prefix siblings fatal.
#[test]
fn r2_t2b_successful_prefix_progress_keeps_a_later_delivery_legal() {
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        turn(vec![
            tool_use("p1", "Ping", "{}"),
            delivery("a3", &normal("hello")),
        ]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("ordinary progress resets the sequence, so the repeat is legal");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(executions_of(&log, "Ping"), 1);
    assert_eq!(delivery_executions(&log), 2);
    assert_exactly_one_result_per_tool_use(runtime.session());
}

/// R2-T3 - `[fatal A, Ping, delivery B]`.
#[test]
fn r2_t3_every_tail_sibling_is_closed_out_and_none_executes() {
    let (mut runtime, calls, log) = harness(fail_closed_script(vec![
        tool_use("p1", "Ping", "{}"),
        delivery("b1", &normal("a completely different message")),
    ]));

    let error = runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must fail closed");

    assert!(error.to_string().contains(HARD_FAIL_MARKER));
    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        total_executions(&log),
        1,
        "only the very first delivery ever reached the executor: {:?}",
        log.borrow()
    );
    assert_eq!(executions_of(&log, "Ping"), 0);

    let counts = result_counts_by_tool_use_id(runtime.session());
    for id in ["a3", "p1", "b1"] {
        assert_eq!(counts.get(id), Some(&1), "counts: {counts:?}");
    }
    assert_exactly_one_result_per_tool_use(runtime.session());
    assert_no_dangling_tool_use(runtime.session());

    for id in ["p1", "b1"] {
        let (output, is_error) = result_for(runtime.session(), id);
        assert!(is_error, "{id} must be an error result");
        assert!(
            output.contains(ABORTED_SIBLING_MARKER),
            "{id} unexpected result: {output:?}"
        );
    }
}

/// R2-T3b - `[fatal A, A2]`: once the abort is committed, a second copy of the
/// offending call is an ordinary aborted-tail sibling. Duplicate-state logic
/// does not run again, and it gets one result, not two.
#[test]
fn r2_t3b_a_second_copy_of_the_offender_is_just_an_aborted_sibling() {
    let (mut runtime, _calls, log) =
        harness(fail_closed_script(vec![delivery("a4", &normal("hello"))]));

    let error = runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must fail closed");

    assert!(error.to_string().contains(HARD_FAIL_MARKER));
    assert_eq!(delivery_executions(&log), 1);

    let counts = result_counts_by_tool_use_id(runtime.session());
    assert_eq!(counts.get("a3"), Some(&1), "counts: {counts:?}");
    assert_eq!(counts.get("a4"), Some(&1), "counts: {counts:?}");
    assert_exactly_one_result_per_tool_use(runtime.session());

    let (first, _) = result_for(runtime.session(), "a3");
    assert!(first.contains(OFFENDER_RESULT_MARKER));
    let (second, _) = result_for(runtime.session(), "a4");
    assert!(
        second.contains(ABORTED_SIBLING_MARKER),
        "the tail copy must not re-run containment: {second:?}"
    );
}

/// R2-T4 - the persisted session must reload with no dangling tool use, and a
/// fresh turn on the reloaded session must present a protocol-complete
/// history to the provider.
#[test]
fn r2_t4_the_persisted_hard_fail_session_reloads_protocol_complete() {
    let path = scratch_path("r2_t4");
    let (api, _calls) = ScriptedApi::new(fail_closed_script(vec![
        tool_use("p1", "Ping", "{}"),
        delivery("b1", &normal("another message")),
    ]));
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new(
        Session::new().with_persistence_path(path.clone()),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);

    let error = runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must fail closed");
    assert!(error.to_string().contains(HARD_FAIL_MARKER));
    assert_eq!(total_executions(&log), 1);

    let reloaded = Session::load_from_path(&path).expect("the session must reload from disk");

    assert_no_dangling_tool_use(&reloaded);
    assert_exactly_one_result_per_tool_use(&reloaded);
    let counts = result_counts_by_tool_use_id(&reloaded);
    for id in ["a1", "a2", "a3", "p1", "b1"] {
        assert_eq!(
            counts.get(id),
            Some(&1),
            "reloaded session is malformed at {id}: {counts:?}"
        );
    }

    // A fresh turn on the reloaded session: the provider validates the history
    // it is handed before answering, exactly as a real provider would reject
    // an unanswered assistant tool call.
    let (validating, seen) = ProtocolValidatingApi::new(vec![text_turn()]);
    let (executor, _log) = RealDispatchExecutor::new();
    let mut resumed = ConversationRuntime::new(
        reloaded,
        validating,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);

    resumed
        .run_turn("carry on", None)
        .expect("a protocol-complete session must be resumable");
    assert_eq!(
        *seen.borrow(),
        vec![true],
        "the provider must have been handed a protocol-complete history"
    );

    let _ = fs::remove_file(&path);
}

/// R2-T5 - exact cardinality, stated as an explicit map rather than a
/// `>= 1` existence check.
#[test]
fn r2_t5_hard_fail_result_cardinality_is_exactly_one_per_tool_use() {
    let (mut runtime, _calls, _log) = harness(fail_closed_script(vec![
        tool_use("p1", "Ping", "{}"),
        delivery("b1", &normal("another message")),
    ]));

    runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must fail closed");

    let counts = result_counts_by_tool_use_id(runtime.session());
    let expected: BTreeMap<String, usize> = ["a1", "a2", "a3", "p1", "b1"]
        .into_iter()
        .map(|id| (id.to_string(), 1usize))
        .collect();
    assert_eq!(counts, expected, "exact per-id cardinality mismatch");
}

/// R2-T6 - an ordinary suppressed duplicate runs the `PostToolUseFailure`
/// lifecycle exactly once, never the success hook, and never the real tool.
#[test]
fn r2_t6_suppressed_duplicate_runs_the_failure_hook_lifecycle() {
    let counts = HookCounts::default();
    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &all_hooks_config(
            Vec::new(),
            vec!["exit 0".to_string()],
            vec![r#"printf '%s' 'failure hook observed'"#.to_string()],
        ),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS)
    .with_hook_progress_reporter(Box::new(counts.reporter()));

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(delivery_executions(&log), 1, "the duplicate never executes");
    assert_eq!(
        counts.post_failure(),
        1,
        "exactly one PostToolUseFailure invocation, for the suppressed duplicate"
    );
    assert_eq!(
        counts.post(),
        1,
        "only the delivery that really ran gets the success hook"
    );

    let (output, is_error) = result_for(runtime.session(), "a2");
    assert!(is_error, "a suppressed duplicate stays an error result");
    assert!(output.contains(DUPLICATE_MARKER), "output: {output:?}");
    assert!(
        output.contains("failure hook observed"),
        "the failure hook feedback must be merged into the result: {output:?}"
    );
}

/// R2-T7 - the fatal offender takes the SAME failure-hook lifecycle: it
/// reached its `PreToolUse` hook and its permission decision and is reported
/// as a failed tool use, so `PostToolUseFailure` runs for it exactly once.
/// The success hook never runs, because it never executed.
#[test]
fn r2_t7_fatal_offender_runs_the_failure_hook_lifecycle_exactly_once() {
    let counts = HookCounts::default();
    let (api, _calls) = ScriptedApi::new(fail_closed_script(vec![tool_use("p1", "Ping", "{}")]));
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &all_hooks_config(
            Vec::new(),
            vec!["exit 0".to_string()],
            vec![r#"printf '%s' 'failure hook observed'"#.to_string()],
        ),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS)
    .with_hook_progress_reporter(Box::new(counts.reporter()));

    runtime
        .run_turn("go", None)
        .expect_err("the stubborn repeat must fail closed");

    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(
        counts.post_failure(),
        2,
        "one for the suppressed duplicate, one for the fatal offender"
    );
    assert_eq!(counts.post(), 1, "only the delivery that really ran");

    let (offender, is_error) = result_for(runtime.session(), "a3");
    assert!(is_error);
    assert!(offender.contains(OFFENDER_RESULT_MARKER));
    assert!(
        offender.contains("failure hook observed"),
        "the offender's failure-hook feedback must be merged: {offender:?}"
    );
    let (sibling, _) = result_for(runtime.session(), "p1");
    assert!(
        !sibling.contains("failure hook observed"),
        "the aborted tail sibling must not run any hook: {sibling:?}"
    );
}

/// R2-T8 - once the abort is committed the runtime performs NO further work:
/// the tail sibling gets no `PreToolUse` hook, no permission prompt, no
/// executor call, and no post hook, while still receiving exactly one
/// protocol-closing tool result.
#[test]
fn r2_t8_aborted_tail_sibling_triggers_no_hooks_permissions_or_execution() {
    let counts = HookCounts::default();
    let (api, calls) = ScriptedApi::new(fail_closed_script(vec![tool_use("p1", "Ping", "{}")]));
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::Prompt)
            .with_tool_requirement("SendUserMessage", PermissionMode::Allow)
            .with_tool_requirement("Ping", PermissionMode::Allow),
        vec!["system".to_string()],
        &all_hooks_config(
            vec!["exit 0".to_string()],
            vec!["exit 0".to_string()],
            vec!["exit 0".to_string()],
        ),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS)
    .with_hook_progress_reporter(Box::new(counts.reporter()));
    let mut prompter = ScriptedPrompter {
        decisions: Vec::new(),
        seen: Vec::new(),
    };

    runtime
        .run_turn("go", Some(&mut prompter))
        .expect_err("the stubborn repeat must fail closed");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        counts.pre(),
        3,
        "only the three delivery attempts reach PreToolUse; the tail sibling must not"
    );
    assert_eq!(
        prompter.seen.len(),
        3,
        "the tail sibling must never reach permission processing"
    );
    assert!(
        prompter
            .seen
            .iter()
            .all(|request| request.tool_name != "Ping"),
        "Ping must never be shown to the operator: {:?}",
        prompter.seen
    );
    assert_eq!(executions_of(&log, "Ping"), 0);
    assert_eq!(
        counts.post(),
        1,
        "only the delivery that really executed gets a success hook"
    );
    assert_eq!(
        counts.post_failure(),
        2,
        "the suppressed duplicate and the fatal offender only"
    );

    let counts_by_id = result_counts_by_tool_use_id(runtime.session());
    assert_eq!(counts_by_id.get("p1"), Some(&1));
    assert_exactly_one_result_per_tool_use(runtime.session());
}

/// R2-T9 - a hook-cancelled attempt is never overridden by containment. The
/// abort signal is raised before the turn, which is the deterministic
/// cancellation path: every hook run short-circuits as cancelled.
#[test]
fn r2_t9_hook_cancellation_is_never_bypassed_by_containment() {
    let abort = HookAbortSignal::new();
    abort.abort();
    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        turn(vec![delivery("a3", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["system".to_string()],
        &hooks_config(vec!["exit 0".to_string()]),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS)
    .with_hook_abort_signal(abort);

    runtime
        .run_turn("go", None)
        .expect("cancelled attempts deny, they do not fail the turn closed");

    assert_eq!(*calls.borrow(), 4);
    assert_eq!(
        delivery_executions(&log),
        0,
        "a cancelled attempt must never execute"
    );
    assert_eq!(
        duplicate_notices(runtime.session()),
        0,
        "containment must never replace a cancellation"
    );
    for id in ["a1", "a2", "a3"] {
        let (output, is_error) = result_for(runtime.session(), id);
        assert!(is_error, "{id} must be an error result");
        assert!(
            !output.contains(OFFENDER_RESULT_MARKER),
            "a cancelled attempt can never be the fatal offender: {output:?}"
        );
    }
    assert_exactly_one_result_per_tool_use(runtime.session());
}

/// R2-T10 - a `PreToolUse` permission override is honored, and it is applied
/// before containment. The policy would otherwise put every call in front of
/// the operator, and the operator here refuses everything; only the hook
/// override lets the calls through, and the overridden repeat then reaches
/// the guard as an allowed call and is suppressed rather than executed.
#[test]
fn r2_t10_pre_hook_permission_override_is_honored_before_containment() {
    let allow_override = concat!(
        r#"printf '%s' '{"hookSpecificOutput":{"permissionDecision":"allow","#,
        r#""permissionDecisionReason":"hook allowed this tool"}}'"#
    );
    let (api, calls) = ScriptedApi::new(vec![
        turn(vec![delivery("a1", &normal("hello"))]),
        turn(vec![delivery("a2", &normal("hello"))]),
        text_turn(),
    ]);
    let (executor, log) = RealDispatchExecutor::new();
    let mut runtime = ConversationRuntime::new_with_features(
        Session::new(),
        api,
        executor,
        PermissionPolicy::new(PermissionMode::Prompt),
        vec!["system".to_string()],
        &hooks_config(vec![allow_override.to_string()]),
    )
    .with_max_iterations(CLI_EQUIVALENT_MAX_ITERATIONS);
    let mut prompter = ScriptedPrompter {
        decisions: vec![
            PermissionPromptDecision::Deny {
                reason: "operator refuses everything".to_string(),
            },
            PermissionPromptDecision::Deny {
                reason: "operator refuses everything".to_string(),
            },
        ],
        seen: Vec::new(),
    };

    runtime
        .run_turn("go", Some(&mut prompter))
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert!(
        prompter.seen.is_empty(),
        "an allow override must settle the decision without prompting: {:?}",
        prompter.seen
    );
    assert_eq!(
        delivery_executions(&log),
        1,
        "the override lets the first call run"
    );
    assert_eq!(
        duplicate_notices(runtime.session()),
        1,
        "the overridden repeat still reaches containment as an allowed call"
    );
    assert_exactly_one_result_per_tool_use(runtime.session());
}

/// R2-T11 - a missing `message`, and a non-string `message`, keep ordinary
/// dispatcher validation and are never classified as semantic duplicates.
#[test]
fn r2_t11_missing_and_non_string_messages_keep_ordinary_validation() {
    for payload in [
        json!({ "status": "normal" }).to_string(),
        json!({ "message": 42, "status": "normal" }).to_string(),
    ] {
        let (mut runtime, calls, log) = harness(vec![
            turn(vec![delivery("a1", &payload)]),
            turn(vec![delivery("a2", &payload)]),
            text_turn(),
        ]);

        runtime
            .run_turn("go", None)
            .expect("invalid payloads must keep ordinary validation");

        assert_eq!(*calls.borrow(), 3, "payload: {payload}");
        assert_eq!(
            delivery_executions(&log),
            2,
            "both attempts must reach the real dispatcher: {payload}"
        );
        assert_eq!(
            duplicate_notices(runtime.session()),
            0,
            "an unclassifiable payload is never a semantic duplicate: {payload}"
        );
        for (_, output, is_error) in tool_results(runtime.session()) {
            assert!(is_error, "the dispatcher must reject {payload}");
            assert!(!output.contains(DUPLICATE_MARKER));
        }
    }
}

/// R2-T12 - a wrong `attachments` type keeps ordinary dispatcher validation.
#[test]
fn r2_t12_wrong_attachments_type_keeps_ordinary_validation() {
    let payload =
        json!({ "message": "hello", "status": "normal", "attachments": "not-a-list" }).to_string();
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &payload)]),
        turn(vec![delivery("a2", &payload)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("invalid attachments must keep ordinary validation");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(delivery_executions(&log), 2);
    assert_eq!(duplicate_notices(runtime.session()), 0);
    for (_, output, is_error) in tool_results(runtime.session()) {
        assert!(is_error, "the dispatcher must reject a string attachments");
        assert!(!output.contains(DUPLICATE_MARKER));
    }
}

/// R2-T13 - unknown fields: the typed key and the real dispatcher must agree
/// with the current Serde behavior, which is to ignore them. Two calls that
/// differ only in an unknown field are therefore the same delivery.
#[test]
fn r2_t13_unknown_fields_are_ignored_by_both_the_key_and_the_dispatcher() {
    let first = json!({ "message": "hello", "status": "normal", "note": "one" }).to_string();
    let second = json!({ "message": "hello", "status": "normal", "note": "two" }).to_string();

    // The real dispatcher accepts both and ignores the unknown field.
    for payload in [&first, &second] {
        let value: Value = serde_json::from_str(payload).expect("valid json");
        let output = tools::execute_tool("SendUserMessage", &value)
            .expect("unknown fields must not be rejected");
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(parsed.get("message"), Some(&json!("hello")));
        assert!(
            parsed.get("note").is_none(),
            "the unknown field must be discarded: {output}"
        );
    }

    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &first)]),
        turn(vec![delivery("a2", &second)]),
        text_turn(),
    ]);

    runtime
        .run_turn("go", None)
        .expect("the turn should recover");

    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        delivery_executions(&log),
        1,
        "an unknown field cannot make the same delivery look distinct"
    );
    assert_eq!(duplicate_notices(runtime.session()), 1);
}

/// R2-T14 - a unit separator inside an attachment path cannot forge or break
/// a key collision. The key is structural, so there is no delimiter to
/// exploit. Real files are used because `execute_brief` canonicalizes every
/// attachment: an unresolvable path would fail the call, and a failed call
/// never records a delivery, which would silently hollow out the assertion.
#[test]
fn r2_t14_unit_separator_in_attachments_cannot_forge_a_collision() {
    let dir = scratch_path("r2_t14");
    fs::create_dir_all(&dir).expect("scratch dir");
    let split_name = dir.join("f\u{001F}g");
    let first_name = dir.join("f");
    let second_name = dir.join("g");
    for path in [&split_name, &first_name, &second_name] {
        fs::write(path, b"x").expect("scratch attachment");
    }
    let attach = |paths: Vec<&std::path::Path>| {
        json!({
            "message": "hello",
            "status": "normal",
            "attachments": paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        })
        .to_string()
    };
    let split = attach(vec![split_name.as_path()]);
    let joined = attach(vec![first_name.as_path(), second_name.as_path()]);

    // Two different attachment lists must stay different: both attempts run.
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("a1", &split)]),
        turn(vec![delivery("a2", &joined)]),
        text_turn(),
    ]);
    runtime
        .run_turn("go", None)
        .expect("the turn should recover");
    for (_, output, is_error) in tool_results(runtime.session()) {
        assert!(
            !is_error,
            "attachment resolution must succeed for this test to mean anything: {output:?}"
        );
    }
    assert_eq!(*calls.borrow(), 3);
    assert_eq!(
        delivery_executions(&log),
        2,
        "a unit separator must not collapse two different attachment lists"
    );
    assert_eq!(duplicate_notices(runtime.session()), 0);

    // And an identical list containing a unit separator is still recognised.
    let (mut runtime, calls, log) = harness(vec![
        turn(vec![delivery("b1", &split)]),
        turn(vec![delivery("b2", &split)]),
        text_turn(),
    ]);
    runtime
        .run_turn("go", None)
        .expect("the turn should recover");
    assert_eq!(*calls.borrow(), 3);
    assert_eq!(delivery_executions(&log), 1);
    assert_eq!(duplicate_notices(runtime.session()), 1);
    assert_exactly_one_result_per_tool_use(runtime.session());

    let _ = fs::remove_dir_all(&dir);
}
