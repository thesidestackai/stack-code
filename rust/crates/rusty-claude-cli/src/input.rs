use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, IsTerminal, Write};

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers,
};

const INTERACTIVE_TERM_FALLBACK: &str = "xterm-256color";
const RUSTYLINE_UNSUPPORTED_TERMS: [&str; 3] = ["dumb", "cons25", "emacs"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    Submit(String),
    Cancel,
    Exit,
}

struct SlashCommandHelper {
    completions: Vec<String>,
    current_line: RefCell<String>,
}

impl SlashCommandHelper {
    fn new(completions: Vec<String>) -> Self {
        Self {
            completions: normalize_completions(completions),
            current_line: RefCell::new(String::new()),
        }
    }

    fn reset_current_line(&self) {
        self.current_line.borrow_mut().clear();
    }

    fn current_line(&self) -> String {
        self.current_line.borrow().clone()
    }

    fn set_current_line(&self, line: &str) {
        let mut current = self.current_line.borrow_mut();
        current.clear();
        current.push_str(line);
    }

    fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = normalize_completions(completions);
    }
}

impl Completer for SlashCommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let Some(prefix) = slash_command_prefix(line, pos) else {
            return Ok((0, Vec::new()));
        };

        let matches = self
            .completions
            .iter()
            .filter(|candidate| candidate.starts_with(prefix))
            .map(|candidate| Pair {
                display: candidate.clone(),
                replacement: candidate.clone(),
            })
            .collect();

        Ok((0, matches))
    }
}

impl Hinter for SlashCommandHelper {
    type Hint = String;
}

impl Highlighter for SlashCommandHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        self.set_current_line(line);
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, line: &str, _pos: usize, _kind: CmdKind) -> bool {
        self.set_current_line(line);
        false
    }
}

impl Validator for SlashCommandHelper {}
impl Helper for SlashCommandHelper {}

pub struct LineEditor {
    prompt: String,
    editor: Editor<SlashCommandHelper, DefaultHistory>,
}

struct InteractiveTermOverride {
    previous: OsString,
}

impl Drop for InteractiveTermOverride {
    fn drop(&mut self) {
        env::set_var("TERM", &self.previous);
    }
}

fn is_rustyline_unsupported_term(term: &OsStr) -> bool {
    let Some(term) = term.to_str() else {
        return false;
    };

    RUSTYLINE_UNSUPPORTED_TERMS
        .iter()
        .any(|unsupported| term.eq_ignore_ascii_case(unsupported))
}

fn interactive_term_override() -> Option<InteractiveTermOverride> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }

    let previous = env::var_os("TERM")?;
    if !is_rustyline_unsupported_term(&previous) {
        return None;
    }

    env::set_var("TERM", INTERACTIVE_TERM_FALLBACK);
    Some(InteractiveTermOverride { previous })
}

fn newline_key_bindings() -> [(KeyEvent, Cmd); 2] {
    [
        (KeyEvent(KeyCode::Char('J'), Modifiers::CTRL), Cmd::Newline),
        (KeyEvent(KeyCode::Enter, Modifiers::SHIFT), Cmd::Newline),
    ]
}

impl LineEditor {
    #[must_use]
    pub fn new(prompt: impl Into<String>, completions: Vec<String>) -> Self {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .build();
        let _term_override = interactive_term_override();
        let mut editor = Editor::<SlashCommandHelper, DefaultHistory>::with_config(config)
            .expect("rustyline editor should initialize");
        editor.set_helper(Some(SlashCommandHelper::new(completions)));
        for (key, command) in newline_key_bindings() {
            editor.bind_sequence(key, command);
        }

        Self {
            prompt: prompt.into(),
            editor,
        }
    }

    pub fn push_history(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.trim().is_empty() {
            return;
        }

        let _ = self.editor.add_history_entry(entry);
    }

    pub fn set_completions(&mut self, completions: Vec<String>) {
        if let Some(helper) = self.editor.helper_mut() {
            helper.set_completions(completions);
        }
    }

    pub fn read_line(&mut self) -> io::Result<ReadOutcome> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return self.read_line_fallback();
        }

        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }

        match self.editor.readline(&self.prompt) {
            Ok(line) => Ok(ReadOutcome::Submit(line)),
            Err(ReadlineError::Interrupted) => {
                let has_input = !self.current_line().is_empty();
                self.finish_interrupted_read()?;
                if has_input {
                    Ok(ReadOutcome::Cancel)
                } else {
                    Ok(ReadOutcome::Exit)
                }
            }
            Err(ReadlineError::Eof) => {
                self.finish_interrupted_read()?;
                Ok(ReadOutcome::Exit)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn current_line(&self) -> String {
        self.editor
            .helper()
            .map_or_else(String::new, SlashCommandHelper::current_line)
    }

    fn finish_interrupted_read(&mut self) -> io::Result<()> {
        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }
        let mut stdout = io::stdout();
        writeln!(stdout)
    }

    fn read_line_fallback(&self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", self.prompt)?;
        stdout.flush()?;

        let mut buffer = String::new();
        let bytes_read = io::stdin().read_line(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(ReadOutcome::Exit);
        }

        while matches!(buffer.chars().last(), Some('\n' | '\r')) {
            buffer.pop();
        }
        Ok(ReadOutcome::Submit(buffer))
    }
}

fn slash_command_prefix(line: &str, pos: usize) -> Option<&str> {
    if pos != line.len() {
        return None;
    }

    let prefix = &line[..pos];
    if !prefix.starts_with('/') {
        return None;
    }

    Some(prefix)
}

fn normalize_completions(completions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    completions
        .into_iter()
        .filter(|candidate| candidate.starts_with('/'))
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        is_rustyline_unsupported_term, newline_key_bindings, slash_command_prefix, LineEditor,
        SlashCommandHelper,
    };
    use rustyline::completion::Completer;
    use rustyline::highlight::Highlighter;
    use rustyline::history::{DefaultHistory, History};
    use rustyline::Context;
    use rustyline::{Cmd, KeyCode, KeyEvent, Modifiers};

    #[test]
    fn extracts_terminal_slash_command_prefixes_with_arguments() {
        assert_eq!(slash_command_prefix("/he", 3), Some("/he"));
        assert_eq!(slash_command_prefix("/help me", 8), Some("/help me"));
        assert_eq!(
            slash_command_prefix("/session switch ses", 19),
            Some("/session switch ses")
        );
        assert_eq!(slash_command_prefix("hello", 5), None);
        assert_eq!(slash_command_prefix("/help", 2), None);
    }

    #[test]
    fn completes_matching_slash_commands() {
        let helper = SlashCommandHelper::new(vec![
            "/help".to_string(),
            "/hello".to_string(),
            "/status".to_string(),
        ]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/he", 3, &ctx)
            .expect("completion should work");

        assert_eq!(start, 0);
        assert_eq!(
            matches
                .into_iter()
                .map(|candidate| candidate.replacement)
                .collect::<Vec<_>>(),
            vec!["/help".to_string(), "/hello".to_string()]
        );
    }

    #[test]
    fn keeps_core_slash_command_completion_candidates() {
        let helper = SlashCommandHelper::new(vec![
            "/help".to_string(),
            "/status".to_string(),
            "/model".to_string(),
            "/session".to_string(),
        ]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        for command in ["/help", "/status", "/model", "/session"] {
            let (start, matches) = helper.complete(command, command.len(), &ctx).unwrap();

            assert_eq!(start, 0);
            assert!(
                matches
                    .iter()
                    .any(|candidate| candidate.replacement == command),
                "missing completion for {command}"
            );
        }
    }

    #[test]
    fn completes_matching_slash_command_arguments() {
        let helper = SlashCommandHelper::new(vec![
            "/model".to_string(),
            "/model opus".to_string(),
            "/model sonnet".to_string(),
            "/session switch alpha".to_string(),
        ]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/model o", 8, &ctx)
            .expect("completion should work");

        assert_eq!(start, 0);
        assert_eq!(
            matches
                .into_iter()
                .map(|candidate| candidate.replacement)
                .collect::<Vec<_>>(),
            vec!["/model opus".to_string()]
        );
    }

    #[test]
    fn ignores_non_slash_command_completion_requests() {
        let helper = SlashCommandHelper::new(vec!["/help".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_, matches) = helper
            .complete("hello", 5, &ctx)
            .expect("completion should work");

        assert!(matches.is_empty());
    }

    #[test]
    fn tracks_current_buffer_through_highlighter() {
        let helper = SlashCommandHelper::new(Vec::new());
        let _ = helper.highlight("draft", 5);

        assert_eq!(helper.current_line(), "draft");
    }

    #[test]
    fn push_history_ignores_blank_entries() {
        let mut editor = LineEditor::new("> ", vec!["/help".to_string()]);
        editor.push_history("   ");
        editor.push_history("/help");

        assert_eq!(editor.editor.history().len(), 1);
    }

    #[test]
    fn set_completions_replaces_and_normalizes_candidates() {
        let mut editor = LineEditor::new("> ", vec!["/help".to_string()]);
        editor.set_completions(vec![
            "/model opus".to_string(),
            "/model opus".to_string(),
            "status".to_string(),
        ]);

        let helper = editor.editor.helper().expect("helper should exist");
        assert_eq!(helper.completions, vec!["/model opus".to_string()]);
    }

    #[test]
    fn recognizes_rustyline_terms_that_need_interactive_override() {
        assert!(is_rustyline_unsupported_term("dumb".as_ref()));
        assert!(is_rustyline_unsupported_term("DUMB".as_ref()));
        assert!(is_rustyline_unsupported_term("emacs".as_ref()));
        assert!(!is_rustyline_unsupported_term("xterm-256color".as_ref()));
    }

    #[test]
    fn binds_explicit_newline_controls() {
        assert_eq!(
            newline_key_bindings(),
            [
                (KeyEvent(KeyCode::Char('J'), Modifiers::CTRL), Cmd::Newline),
                (KeyEvent(KeyCode::Enter, Modifiers::SHIFT), Cmd::Newline),
            ]
        );
    }

    #[cfg(unix)]
    mod pty_tests {
        use super::LineEditor;
        use crate::input::ReadOutcome;
        use rustyline::history::History;
        use std::process::Command;

        fn run_editor_child(input: &[u8], term: &str) -> String {
            let input_hex = input
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let script = r#"
import binascii
import os
import pty
import select
import subprocess
import sys
import time

exe, input_hex, term = sys.argv[1:4]
input_bytes = binascii.unhexlify(input_hex)
master, slave = pty.openpty()
env = os.environ.copy()
env["STACK_CODE_LINE_EDITOR_PTY_CHILD"] = "1"
env["TERM"] = term
proc = subprocess.Popen(
    [
        exe,
        "input::tests::pty_tests::line_editor_pty_child",
        "--exact",
        "--ignored",
        "--nocapture",
    ],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    close_fds=True,
    env=env,
)
os.close(slave)
output = bytearray()
deadline = time.monotonic() + 5
sent = False
while time.monotonic() < deadline:
    if proc.poll() is not None:
        break
    ready, _, _ = select.select([master], [], [], 0.1)
    if not ready:
        continue
    try:
        chunk = os.read(master, 4096)
    except OSError:
        break
    if not chunk:
        break
    output.extend(chunk)
    if not sent and b"> " in output:
        os.write(master, input_bytes)
        sent = True
        break
if not sent:
    proc.terminate()
    sys.stdout.buffer.write(output)
    raise SystemExit("prompt not observed")
deadline = time.monotonic() + 5
while time.monotonic() < deadline:
    if proc.poll() is not None:
        break
    ready, _, _ = select.select([master], [], [], 0.1)
    if not ready:
        continue
    try:
        chunk = os.read(master, 4096)
    except OSError:
        break
    if not chunk:
        break
    output.extend(chunk)
if proc.poll() is None:
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.terminate()
        sys.stdout.buffer.write(output)
        raise SystemExit("child did not finish")
while True:
    ready, _, _ = select.select([master], [], [], 0)
    if not ready:
        break
    try:
        chunk = os.read(master, 4096)
    except OSError:
        break
    if not chunk:
        break
    output.extend(chunk)
os.close(master)
sys.stdout.buffer.write(output)
raise SystemExit(proc.returncode)
"#;

            let output = Command::new("python3")
                .arg("-c")
                .arg(script)
                .arg(std::env::current_exe().expect("test exe should exist"))
                .arg(input_hex)
                .arg(term)
                .output()
                .expect("python pty harness should run");
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            assert!(
                output.status.success(),
                "pty harness failed: status={:?} stdout={:?} stderr={:?}",
                output.status.code(),
                stdout,
                String::from_utf8_lossy(&output.stderr)
            );

            stdout
        }

        fn assert_single_submit(output: &str, expected: &str) {
            assert_eq!(
                output.matches("SUBMIT=").count(),
                1,
                "expected one submit in output: {output:?}"
            );
            assert!(
                output.contains(&format!("SUBMIT={expected:?}")),
                "missing submitted value {expected:?} in output: {output:?}"
            );
        }

        fn bracketed_paste(content: &str) -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(content.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~\r");
            bytes
        }

        #[test]
        #[ignore]
        fn line_editor_pty_child() {
            if std::env::var_os("STACK_CODE_LINE_EDITOR_PTY_CHILD").is_none() {
                return;
            }

            let mut editor = LineEditor::new(
                "> ",
                vec![
                    "/help".to_string(),
                    "/status".to_string(),
                    "/model".to_string(),
                    "/session".to_string(),
                ],
            );
            match editor.read_line().expect("read should complete") {
                ReadOutcome::Submit(line) => {
                    println!("SUBMIT={line:?}");
                    editor.push_history(line);
                    println!("HISTORY_LEN={}", editor.editor.history().len());
                }
                ReadOutcome::Cancel => println!("CANCEL"),
                ReadOutcome::Exit => println!("EXIT"),
            }
        }

        #[test]
        fn multiline_bracketed_paste_under_dumb_term_is_one_history_entry() {
            let content = "Create the project.\n\nRequirements:\n- one\n- two\n\nDo not install packages.\nDo not commit or push.\nRun the tests before finishing.";
            let output = run_editor_child(&bracketed_paste(content), "dumb");

            assert_single_submit(&output, content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
            assert!(!output.contains("\\u{1b}[200~"), "{output:?}");
            assert!(!output.contains("\\u{1b}[201~"), "{output:?}");
        }

        #[test]
        fn large_bounded_bracketed_paste_stays_one_message() {
            let content = (0..128)
                .map(|index| format!("requirement line {index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let output = run_editor_child(&bracketed_paste(&content), "dumb");

            assert_single_submit(&output, &content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
        }

        #[test]
        fn normal_enter_submits_single_line() {
            let output = run_editor_child(b"hello\r", "dumb");

            assert_single_submit(&output, "hello");
        }

        #[test]
        fn ctrl_j_preserves_embedded_newline_until_enter() {
            let output = run_editor_child(b"hello\nworld\r", "dumb");

            assert_single_submit(&output, "hello\nworld");
        }

        #[test]
        fn single_line_bracketed_paste_submits_normally_on_enter() {
            let output = run_editor_child(&bracketed_paste("hello world"), "dumb");

            assert_single_submit(&output, "hello world");
        }

        #[test]
        fn ctrl_c_on_empty_line_exits() {
            let output = run_editor_child(b"\x03", "dumb");

            assert_eq!(output.matches("SUBMIT=").count(), 0, "{output:?}");
            assert!(output.contains("EXIT"), "{output:?}");
        }
    }
}
