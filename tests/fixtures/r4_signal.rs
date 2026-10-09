// Offline witness: exact CLI signal/input code is included by the shell suite.
use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, HookAbortSignal, PermissionMode,
    PermissionPolicy, RuntimeError, Session, ToolError, ToolExecutor,
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};
include!("monitor.rs");
mod input;

#[derive(Clone)]
struct Probe {
    root: PathBuf,
    stage: String,
}
impl Probe {
    fn event(&self, event: &str) {
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.root.join("events"))
                .unwrap(),
            "{event}"
        )
        .unwrap();
    }
    fn gate(&self, stage: &str) {
        if self.stage == stage {
            self.event(&format!("ready:{stage}"));
            // The parent owns the signal and the release, with a bounded fail-open
            // timeout so a swallowed signal is observable as forbidden activity.
            for _ in 0..1000 {
                if self.root.join("release").exists() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}
struct FakeApi {
    n: usize,
    probe: Probe,
}
impl ApiClient for FakeApi {
    fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        self.n += 1;
        self.probe.event("provider");
        if self.n == 1 {
            self.probe.gate("during-provider");
            self.probe.gate("before-retry");
            if self.probe.stage == "before-retry" {
                self.probe.event("retry");
                self.probe.event("provider");
            }
            let response = vec![
                AssistantEvent::ToolUse {
                    id: "one".into(),
                    name: "write_file".into(),
                    input: "{}".into(),
                },
                AssistantEvent::MessageStop,
            ];
            self.probe.gate("after-response");
            Ok(response)
        } else {
            Ok(vec![
                AssistantEvent::TextDelta("done".into()),
                AssistantEvent::MessageStop,
            ])
        }
    }
}
struct FakeTool(Probe);
impl ToolExecutor for FakeTool {
    fn execute(&mut self, _: &str, _: &str) -> Result<String, ToolError> {
        self.0.event("tool");
        self.0.gate("before-write");
        self.0.event("command"); // harmless stand-in; never runs a host command
        std::fs::write(self.0.root.join("marker"), "permitted fake write").unwrap();
        self.0.event("write");
        self.0.gate("after-write");
        Ok("fake write".into())
    }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let probe = Probe {
        root: PathBuf::from(&args[1]),
        stage: args[2].clone(),
    };
    let confinement =
        tools::WorkspaceConfinement::new(&probe.root, &["marker"], &[], 1000).unwrap();
    tools::set_workspace_confinement(confinement).unwrap();
    let abort = HookAbortSignal::new();
    let monitor = HookAbortMonitor::spawn(abort.clone());
    thread::sleep(Duration::from_millis(150));
    if probe.stage == "idle" {
        let mut editor = input::LineEditor::new("> ", vec![]);
        probe.event("idle-ready");
        let outcome = editor.read_line().unwrap();
        probe.event(&format!("idle-result:{outcome:?}"));
        monitor.stop();
        return;
    }
    probe.gate("before-provider");
    let mut rt = ConversationRuntime::new(
        Session::new(),
        FakeApi {
            n: 0,
            probe: probe.clone(),
        },
        FakeTool(probe.clone()),
        PermissionPolicy::new(PermissionMode::WorkspaceWrite)
            .with_tool_requirement("write_file", PermissionMode::WorkspaceWrite),
        vec![],
    )
    .with_hook_abort_signal(abort);
    rt.run_turn("offline", None).unwrap();
    probe.event("continued");
    monitor.stop();
    if probe.stage == "next-turn" {
        let monitor = HookAbortMonitor::spawn(HookAbortSignal::new());
        thread::sleep(Duration::from_millis(150));
        probe.gate("next-turn");
        rt.run_turn("next", None).unwrap();
        probe.event("continued");
        monitor.stop();
    }
}
