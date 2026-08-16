use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::io::{self, IsTerminal, Read, Write};
use std::mem;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers,
};

const RUSTYLINE_UNSUPPORTED_TERMS: [&str; 3] = ["dumb", "cons25", "emacs"];
const BRACKETED_PASTE_BEGIN: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
/// Deterministic upper bound on one cooked logical line, including verbatim
/// bracketed-paste payload.
const MAX_DIRECT_INPUT_BYTES: usize = 1024 * 1024;

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

/// An accumulator for cooked (non-rustyline) line input.
///
/// Recognizes an exact bracketed-paste begin marker at the *current* input
/// position -- including after literal content already typed on the same
/// line -- then treats every subsequent byte as literal paste content
/// (including control bytes) until the exact end marker is seen. Both
/// markers are matched incrementally so they are recognized correctly even
/// when the underlying reads split them across multiple syscalls.
///
/// This never asks a terminal to *enable* bracketed-paste mode; it only
/// interprets framing bytes that genuinely arrive (from a file, a pipe, or a
/// terminal whose mode some other component already negotiated). Terminals
/// that Stack-Code drives itself are handled by rustyline, which owns that
/// negotiation.
struct PasteAwareBuffer {
    content: Vec<u8>,
    in_paste: bool,
    begin_match: usize,
    end_match: usize,
    strip_floor: usize,
}

impl PasteAwareBuffer {
    fn new() -> Self {
        Self {
            content: Vec::new(),
            in_paste: false,
            begin_match: 0,
            end_match: 0,
            strip_floor: 0,
        }
    }

    fn in_paste(&self) -> bool {
        self.in_paste
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.content.len() + bytes.len() > MAX_DIRECT_INPUT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "direct input exceeds maximum size",
            ));
        }
        self.content.extend_from_slice(bytes);
        Ok(())
    }

    /// Feeds one byte into the accumulator.
    ///
    /// Marker recognition is position-independent: a complete begin marker is
    /// honored wherever it arrives, including immediately after literal text
    /// already typed on the same line, so `prefix: ` followed by a framed
    /// paste yields `prefix: ` plus the payload rather than leaking framing
    /// bytes into the message.
    ///
    /// On a mismatch the partially-matched prefix is flushed as literal input
    /// and matching restarts from the mismatching byte. Because `ESC` occurs
    /// exactly once in each marker -- at index 0 -- checking that single byte
    /// is a complete restart rule; no longer partial overlap is possible.
    fn push(&mut self, byte: u8) -> io::Result<()> {
        if self.in_paste {
            if byte == BRACKETED_PASTE_END[self.end_match] {
                self.end_match += 1;
                if self.end_match == BRACKETED_PASTE_END.len() {
                    self.in_paste = false;
                    self.end_match = 0;
                    // Everything pasted is verbatim payload: a later line
                    // terminator must never strip trailing newlines out of it.
                    self.strip_floor = self.content.len();
                }
                return Ok(());
            }

            let flushed = BRACKETED_PASTE_END[..self.end_match].to_vec();
            self.end_match = 0;
            self.append(&flushed)?;

            if byte == BRACKETED_PASTE_END[0] {
                self.end_match = 1;
                return Ok(());
            }

            return self.append(&[byte]);
        }

        if byte == BRACKETED_PASTE_BEGIN[self.begin_match] {
            self.begin_match += 1;
            if self.begin_match == BRACKETED_PASTE_BEGIN.len() {
                self.in_paste = true;
                self.begin_match = 0;
            }
            return Ok(());
        }

        let flushed = BRACKETED_PASTE_BEGIN[..self.begin_match].to_vec();
        self.begin_match = 0;
        self.append(&flushed)?;

        if byte == BRACKETED_PASTE_BEGIN[0] {
            self.begin_match = 1;
            return Ok(());
        }

        self.append(&[byte])
    }

    /// Moves any partially-matched framing-marker bytes back into content.
    ///
    /// A prefix that never completed a marker is ordinary literal input and
    /// must survive termination or EOF. A *complete* marker can never be
    /// pending here (both counters reset the moment a marker completes), so
    /// this can never leak framing bytes into the payload.
    fn flush_pending_marker(&mut self) -> io::Result<()> {
        let pending = if self.in_paste {
            &BRACKETED_PASTE_END[..self.end_match]
        } else {
            &BRACKETED_PASTE_BEGIN[..self.begin_match]
        };

        if pending.is_empty() {
            return Ok(());
        }

        let pending = pending.to_vec();
        self.begin_match = 0;
        self.end_match = 0;
        self.append(&pending)
    }

    /// Drops a trailing run of CR/LF produced by the line terminator itself,
    /// never reaching into verbatim pasted payload.
    fn strip_trailing_line_endings(&mut self) {
        while self.content.len() > self.strip_floor
            && matches!(self.content.last(), Some(b'\r' | b'\n'))
        {
            self.content.pop();
        }
    }

    /// Finalizes the accumulated bytes into one logical line.
    ///
    /// Rejects invalid UTF-8 with `InvalidData` rather than substituting
    /// replacement characters, matching the prior `BufRead::read_line`
    /// fallback contract.
    fn take_line(&mut self, strip_line_endings: bool) -> io::Result<String> {
        self.flush_pending_marker()?;
        if strip_line_endings {
            self.strip_trailing_line_endings();
        }
        self.in_paste = false;
        self.strip_floor = 0;
        String::from_utf8(mem::take(&mut self.content)).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
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

/// Whether the current process environment describes a terminal rustyline
/// can drive directly. Never mutates the environment: it only reads `TERM`.
/// Unset or empty `TERM` is treated as unsupported so we fall back to the
/// cooked path rather than guessing at capabilities.
fn is_current_term_supported() -> bool {
    match env::var_os("TERM") {
        Some(term) if !term.is_empty() => !is_rustyline_unsupported_term(&term),
        _ => false,
    }
}

/// Selects the native interactive editor (rustyline) only when every
/// requirement for it holds: an interactive input terminal, an interactive
/// output terminal, and a `TERM` rustyline is willing to drive.
///
/// When this is true rustyline owns the terminal completely -- raw mode,
/// bracketed-paste enable/disable negotiation, insertion at the cursor,
/// editing, UTF-8-aware erase, and Ctrl-C. Stack-Code neither duplicates nor
/// intercepts any of it. Every other configuration takes the cooked
/// fallback, which asserts no capabilities at all.
fn should_use_line_editor() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && is_current_term_supported()
}

/// Cooked line reader used for every input source Stack-Code does not drive
/// through rustyline: a rustyline-unsupported terminal, a terminal whose
/// stdout has been redirected, and piped or redirected stdin.
///
/// Stack-Code never enables raw mode here, never emits terminal-capability
/// sequences, and never manufactures replacement echo. When stdin is a
/// terminal it stays in canonical mode, so the OS line discipline owns echo,
/// erase (whole UTF-8 characters where `IUTF8` is set), and the interrupt and
/// end-of-file characters -- exactly as it did before Stack-Code grew a
/// direct interactive reader, and exactly as rustyline's own unsupported-
/// terminal fallback behaves.
///
/// Matches the prior `BufRead::read_line` fallback contract: LF alone
/// terminates a line, a trailing run of CR/LF is stripped (so CRLF is one
/// logical terminator and never yields a spurious empty turn), and a bare CR
/// is ordinary content. Bracketed-paste payload is collected verbatim between
/// markers -- when framing genuinely arrives -- and is never touched by
/// terminator stripping.
///
/// Deliberate limitation: on a terminal that provides no bracketed-paste
/// framing, a pasted newline is an ordinary line terminator. Such a paste
/// therefore submits line by line rather than as one message. Stack-Code does
/// not fake framing to hide this, because it cannot prove such a terminal
/// would understand the sequences required to request it.
fn read_line_cooked(source: &mut impl Read) -> io::Result<ReadOutcome> {
    let mut reader = PasteAwareBuffer::new();
    let mut byte = [0u8; 1];
    let mut read_any = false;

    loop {
        let n = source.read(&mut byte)?;
        if n == 0 {
            if !read_any {
                return Ok(ReadOutcome::Exit);
            }
            return Ok(ReadOutcome::Submit(reader.take_line(true)?));
        }
        read_any = true;
        let b = byte[0];

        if !reader.in_paste() && b == b'\n' {
            return Ok(ReadOutcome::Submit(reader.take_line(true)?));
        }

        reader.push(b)?;
    }
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
        if !should_use_line_editor() {
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

    /// Cooked fallback for every configuration that is not a rustyline-driven
    /// interactive terminal.
    ///
    /// There is no `stdin().is_terminal()` branch here on purpose. A terminal
    /// left in canonical mode buffers typed input until it is read, so there
    /// is no readiness race to close and no reason to treat a tty differently
    /// from a pipe. Critically, this means a TTY stdin with redirected stdout
    /// no longer enters a raw reader: terminal echo stays with the tty, and
    /// redirected output is never contaminated with keystrokes or erase
    /// sequences.
    fn read_line_fallback(&self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", self.prompt)?;
        stdout.flush()?;
        read_line_cooked(&mut io::stdin().lock())
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
    fn recognizes_rustyline_terms_that_need_direct_path_fallback() {
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

    #[test]
    fn production_path_has_no_process_env_mutation() {
        let source = include_str!("input.rs");
        let test_module_start = source
            .find("#[cfg(test)]")
            .expect("test module marker should be present");
        let production_source = &source[..test_module_start];

        assert!(
            !production_source.contains("set_var"),
            "production code must never mutate the process environment"
        );
        assert!(
            !production_source.contains("remove_var"),
            "production code must never mutate the process environment"
        );
    }

    /// Option C invariant: Stack-Code owns no terminal mode of its own.
    ///
    /// Raw mode, capability negotiation and editing belong to rustyline on a
    /// terminal it can drive, and to the OS line discipline everywhere else.
    /// A reintroduced direct raw reader would revive all four review
    /// findings at once, so it is blocked structurally.
    #[test]
    fn production_path_drives_no_terminal_mode_of_its_own() {
        let source = include_str!("input.rs");
        let test_module_start = source
            .find("#[cfg(test)]")
            .expect("test module marker should be present");
        let production_source = &source[..test_module_start];

        for forbidden in [
            "enable_raw_mode",
            "disable_raw_mode",
            "RawModeGuard",
            "crossterm",
        ] {
            assert!(
                !production_source.contains(forbidden),
                "production input path must not use {forbidden}: terminal mode \
                 belongs to rustyline or the line discipline, never to this module"
            );
        }

        // Nor may it ask a terminal to turn bracketed paste on/off itself.
        for sequence in ["?2004h", "?2004l"] {
            assert!(
                !production_source.contains(sequence),
                "production input path must not emit {sequence}"
            );
        }
    }

    /// The limited tier must never be coerced toward xterm behavior.
    #[test]
    fn production_path_never_substitutes_a_capable_term_value() {
        let source = include_str!("input.rs");
        let test_module_start = source
            .find("#[cfg(test)]")
            .expect("test module marker should be present");
        let production_source = &source[..test_module_start];

        assert!(
            !production_source.contains("xterm"),
            "production code must not name a substitute TERM value"
        );
    }

    mod direct_piped_tests {
        use super::super::{read_line_cooked, ReadOutcome};

        fn drain(mut stream: &[u8]) -> Vec<ReadOutcome> {
            let mut outcomes = Vec::new();
            loop {
                let outcome = read_line_cooked(&mut stream).expect("piped read should succeed");
                let done = outcome == ReadOutcome::Exit;
                outcomes.push(outcome);
                if done {
                    return outcomes;
                }
            }
        }

        fn submits(stream: &[u8]) -> Vec<String> {
            drain(stream)
                .into_iter()
                .filter_map(|outcome| match outcome {
                    ReadOutcome::Submit(line) => Some(line),
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn crlf_stream_yields_two_lines_with_no_empty_turn() {
            assert_eq!(submits(b"hello\r\nworld\r\n"), vec!["hello", "world"]);
        }

        #[test]
        fn lf_only_stream_yields_two_lines() {
            assert_eq!(submits(b"hello\nworld\n"), vec!["hello", "world"]);
        }

        #[test]
        fn cr_only_stream_matches_prior_read_line_contract() {
            // The pre-existing fallback used `BufRead::read_line`, which
            // terminates on LF alone; a bare CR is ordinary content and only
            // a trailing run of CR/LF is stripped at EOF.
            assert_eq!(submits(b"hello\rworld\r"), vec!["hello\rworld"]);
        }

        #[test]
        fn crlf_terminator_does_not_strip_pasted_trailing_newlines() {
            let mut stream = Vec::new();
            stream.extend_from_slice(b"\x1b[200~keep\n\n\x1b[201~");
            stream.extend_from_slice(b"\r\n");
            assert_eq!(submits(&stream), vec!["keep\n\n"]);
        }

        #[test]
        fn partial_begin_marker_then_lf_is_not_lost() {
            assert_eq!(submits(b"\x1b[20\n"), vec!["\x1b[20"]);
        }

        #[test]
        fn partial_begin_marker_then_eof_is_not_lost() {
            assert_eq!(submits(b"\x1b["), vec!["\x1b["]);
        }

        #[test]
        fn partial_end_marker_then_eof_is_not_lost() {
            assert_eq!(submits(b"\x1b[200~body\x1b[20"), vec!["body\x1b[20"]);
        }

        #[test]
        fn invalid_utf8_returns_invalid_data_matching_prior_contract() {
            let mut stream: &[u8] = b"he\xffllo\n";
            let error = read_line_cooked(&mut stream)
                .expect_err("invalid UTF-8 must not be silently substituted");
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }

        #[test]
        fn valid_multibyte_utf8_survives_strict_decoding() {
            assert_eq!(submits("héllo → 世界\n".as_bytes()), vec!["héllo → 世界"]);
        }

        #[test]
        fn every_partial_begin_prefix_length_survives_termination() {
            for prefix_len in 1..super::super::BRACKETED_PASTE_BEGIN.len() {
                let prefix = &super::super::BRACKETED_PASTE_BEGIN[..prefix_len];
                let mut stream = prefix.to_vec();
                stream.push(b'\n');
                let expected = String::from_utf8(prefix.to_vec()).expect("prefix is ascii");
                assert_eq!(
                    submits(&stream),
                    vec![expected],
                    "partial begin prefix of length {prefix_len} was lost"
                );
            }
        }

        #[test]
        fn every_partial_begin_prefix_length_survives_eof() {
            for prefix_len in 1..super::super::BRACKETED_PASTE_BEGIN.len() {
                let prefix = &super::super::BRACKETED_PASTE_BEGIN[..prefix_len];
                let expected = String::from_utf8(prefix.to_vec()).expect("prefix is ascii");
                assert_eq!(
                    submits(prefix),
                    vec![expected],
                    "partial begin prefix of length {prefix_len} was lost at EOF"
                );
            }
        }

        #[test]
        fn every_partial_end_prefix_length_survives_eof_inside_paste() {
            for prefix_len in 1..super::super::BRACKETED_PASTE_END.len() {
                let prefix = &super::super::BRACKETED_PASTE_END[..prefix_len];
                let mut stream = b"\x1b[200~body".to_vec();
                stream.extend_from_slice(prefix);
                let expected = format!(
                    "body{}",
                    std::str::from_utf8(prefix).expect("prefix is ascii")
                );
                assert_eq!(
                    submits(&stream),
                    vec![expected],
                    "partial end prefix of length {prefix_len} was lost at EOF"
                );
            }
        }

        #[test]
        fn complete_markers_stay_framing_and_never_become_payload() {
            assert_eq!(submits(b"\x1b[200~payload\x1b[201~\n"), vec!["payload"]);
        }
    }

    mod paste_aware_buffer_tests {
        use super::super::PasteAwareBuffer;

        fn feed_all(buffer: &mut PasteAwareBuffer, bytes: &[u8]) {
            for &byte in bytes {
                buffer.push(byte).expect("push should succeed");
            }
        }

        #[test]
        fn plain_bytes_pass_through_unmodified() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"hello");
            assert_eq!(buffer.take_line(false).expect("valid utf-8"), "hello");
        }

        #[test]
        fn bracketed_paste_strips_markers_and_preserves_newlines() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"\x1b[200~line one\nline two\x1b[201~");
            assert!(!buffer.in_paste());
            assert_eq!(
                buffer.take_line(false).expect("valid utf-8"),
                "line one\nline two"
            );
        }

        #[test]
        fn fragmented_begin_and_end_markers_are_still_recognized() {
            let mut buffer = PasteAwareBuffer::new();
            for &byte in b"\x1b[200~" {
                buffer.push(byte).unwrap();
            }
            assert!(buffer.in_paste());
            for &byte in b"a\nb" {
                buffer.push(byte).unwrap();
            }
            for &byte in b"\x1b[201~" {
                buffer.push(byte).unwrap();
            }
            assert!(!buffer.in_paste());
            assert_eq!(buffer.take_line(false).expect("valid utf-8"), "a\nb");
        }

        #[test]
        fn false_start_on_begin_marker_preserves_literal_bytes() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"\x1b[2XY");
            assert!(!buffer.in_paste());
            assert_eq!(buffer.take_line(false).expect("valid utf-8"), "\x1b[2XY");
        }

        #[test]
        fn over_limit_content_is_rejected_deterministically() {
            let mut buffer = PasteAwareBuffer::new();
            let mut result = Ok(());
            for _ in 0..(1024 * 1024 + 1) {
                result = buffer.push(b'a');
                if result.is_err() {
                    break;
                }
            }
            assert!(
                result.is_err(),
                "push should reject content beyond the deterministic bound"
            );
        }

        #[test]
        fn large_bounded_content_is_accepted() {
            let mut buffer = PasteAwareBuffer::new();
            for _ in 0..1024 {
                buffer.push(b'a').expect("push should succeed within bound");
            }
            assert_eq!(buffer.take_line(false).expect("valid utf-8").len(), 1024);
        }

        #[test]
        fn partial_end_marker_inside_paste_flushes_then_keeps_collecting() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"\x1b[200~body\x1b[20\rmore");
            // An unterminated paste keeps collecting: CR is payload here, and
            // the abandoned end-marker prefix must survive as literal bytes.
            assert!(buffer.in_paste());
            assert_eq!(
                buffer.take_line(false).expect("valid utf-8"),
                "body\x1b[20\rmore"
            );
        }

        // --- position-independent begin-marker recognition (fresh P2) -------

        #[test]
        fn explicit_framing_after_typed_prefix_is_recognized_not_leaked() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"prefix: \x1b[200~line1\nline2\x1b[201~");
            assert!(!buffer.in_paste());
            assert_eq!(
                buffer.take_line(false).expect("valid utf-8"),
                "prefix: line1\nline2"
            );
        }

        #[test]
        fn explicit_framing_mid_content_never_leaks_marker_bytes() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"a\x1b[200~B\x1b[201~c");
            let line = buffer.take_line(false).expect("valid utf-8");
            assert_eq!(line, "aBc");
            assert!(!line.contains('\u{1b}'), "framing leaked into {line:?}");
        }

        #[test]
        fn begin_marker_after_prefix_is_recognized_when_fragmented() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"prefix: ");
            for &byte in b"\x1b[200~" {
                buffer.push(byte).unwrap();
            }
            assert!(buffer.in_paste());
            feed_all(&mut buffer, b"x\ny\x1b[201~");
            assert_eq!(
                buffer.take_line(false).expect("valid utf-8"),
                "prefix: x\ny"
            );
        }

        #[test]
        fn repeated_escape_restarts_marker_matching_instead_of_swallowing_it() {
            // A false start immediately followed by a real marker: the stray
            // ESC must stay literal AND the real marker must still be honored.
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"\x1b\x1b[200~body\x1b[201~");
            assert!(!buffer.in_paste());
            assert_eq!(buffer.take_line(false).expect("valid utf-8"), "\x1bbody");
        }

        #[test]
        fn escape_occurs_once_per_marker_so_single_byte_restart_is_complete() {
            // Documents why `push` needs no full KMP failure function: ESC
            // appears exactly once in each marker, at index 0.
            for marker in [
                super::super::BRACKETED_PASTE_BEGIN,
                super::super::BRACKETED_PASTE_END,
            ] {
                assert_eq!(marker[0], 0x1b, "marker {marker:?} must start with ESC");
                assert_eq!(
                    marker.iter().rposition(|&byte| byte == 0x1b),
                    Some(0),
                    "marker {marker:?} must contain no ESC after index 0"
                );
            }
        }

        #[test]
        fn partial_prefix_after_content_still_survives_termination() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"typed\x1b[20");
            assert_eq!(
                buffer.take_line(false).expect("valid utf-8"),
                "typed\x1b[20"
            );
        }
    }

    /// Terminal-peer PTY tests.
    ///
    /// The peer behaves like a real terminal emulator: it tracks
    /// bracketed-paste mode purely from the bytes Stack-Code writes, and it
    /// frames a paste **only** after observing the application request that
    /// mode with `ESC[?2004h`. It never injects framing the application did
    /// not ask for.
    ///
    /// The previous harness framed unconditionally, which made the
    /// unsupported-terminal path look as though it supported bracketed paste
    /// even though it never enabled it. Any regression back to parsing
    /// framing without enabling it now fails here instead of passing.
    #[cfg(unix)]
    mod pty_tests {
        use super::LineEditor;
        use crate::input::ReadOutcome;
        use rustyline::history::History;
        use std::process::Command;

        const TERM_UNSET_SENTINEL: &str = "__STACK_CODE_TERM_UNSET__";
        const SINGLE_READ_CHILD: &str = "input::tests::pty_tests::line_editor_pty_child";
        const LOOP_CHILD: &str = "input::tests::pty_tests::line_editor_pty_child_loop";

        /// Terminal peer. Argv: exe, child test, TERM, ops, redirect path.
        ///
        /// Ops are `;`-separated; payloads are hex. `paste` is the decisive
        /// one: it frames only while bracketed-paste mode is actually active
        /// according to the application's own output.
        const PEER_SCRIPT: &str = r#"
import binascii
import os
import pty
import select
import subprocess
import sys
import termios
import time

exe, child, term, ops_raw, redirect = sys.argv[1:6]

# Linux <termios.h>. Python's termios module does not expose IUTF8; setting it
# makes the canonical-mode line discipline erase one whole UTF-8 character on
# ERASE, which is what a real UTF-8 terminal does.
IUTF8 = 0o040000
ENABLE = b"\x1b[?2004h"
DISABLE = b"\x1b[?2004l"
PASTE_BEGIN = b"\x1b[200~"
PASTE_END = b"\x1b[201~"
TIMEOUT = 15.0

master, slave = pty.openpty()
attrs = termios.tcgetattr(slave)
attrs[0] |= IUTF8
termios.tcsetattr(slave, termios.TCSANOW, attrs)

env = os.environ.copy()
env["STACK_CODE_LINE_EDITOR_PTY_CHILD"] = "1"
if term == "__STACK_CODE_TERM_UNSET__":
    env.pop("TERM", None)
else:
    env["TERM"] = term

redirect_fh = None
stdout_target = slave
if redirect:
    redirect_fh = open(redirect, "wb")
    stdout_target = redirect_fh.fileno()

proc = subprocess.Popen(
    [exe, child, "--exact", "--ignored", "--nocapture"],
    stdin=slave,
    stdout=stdout_target,
    stderr=stdout_target,
    close_fds=True,
    env=env,
)
os.close(slave)

output = bytearray()
scan_pos = 0
bracketed_active = False
enable_seen = False
disable_seen = False
prompt_seen = False
framing = []


def scan():
    global scan_pos, bracketed_active, enable_seen, disable_seen
    start = max(0, scan_pos - (len(ENABLE) - 1))
    window = bytes(output[start:])
    events = []
    for seq, on in ((ENABLE, True), (DISABLE, False)):
        i = window.find(seq)
        while i != -1:
            events.append((start + i, on))
            i = window.find(seq, i + 1)
    for pos, on in sorted(events):
        if pos < scan_pos:
            continue
        bracketed_active = on
        if on:
            enable_seen = True
        else:
            disable_seen = True
    scan_pos = len(output)


def pump(budget):
    try:
        ready, _, _ = select.select([master], [], [], budget)
    except (OSError, ValueError):
        return False
    if not ready:
        return False
    try:
        chunk = os.read(master, 4096)
    except OSError:
        return False
    if not chunk:
        return False
    output.extend(chunk)
    scan()
    return True


def redirected_bytes():
    if not redirect:
        return b""
    try:
        with open(redirect, "rb") as fh:
            return fh.read()
    except OSError:
        return b""


def app_bytes():
    return bytes(output) + redirected_bytes()


def wait_until(pred, timeout=TIMEOUT):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if pred():
            return True
        if proc.poll() is not None:
            pump(0.05)
            return pred()
        pump(0.05)
    return pred()


for op in [item for item in ops_raw.split(";") if item]:
    if ":" in op:
        name, payload_hex = op.split(":", 1)
        payload = binascii.unhexlify(payload_hex)
    else:
        name, payload = op, b""
    if name == "wait_prompt":
        prompt_seen = wait_until(lambda: b"> " in app_bytes())
    elif name == "wait_enable":
        wait_until(lambda: enable_seen)
    elif name in ("type", "send"):
        os.write(master, payload)
    elif name == "paste":
        if bracketed_active:
            os.write(master, PASTE_BEGIN + payload + PASTE_END)
            framing.append("1")
        else:
            os.write(master, payload.replace(b"\n", b"\r"))
            framing.append("0")
    else:
        raise SystemExit("unknown peer op: " + name)

wait_until(lambda: proc.poll() is not None)
if proc.poll() is None:
    proc.kill()
    proc.wait(timeout=5)
while pump(0.05):
    pass
try:
    os.close(master)
except OSError:
    pass
if redirect_fh:
    redirect_fh.close()

write = sys.stdout.write
write("PEER_PROMPT_OBSERVED=%d\n" % int(prompt_seen))
write("PEER_ENABLE_OBSERVED=%d\n" % int(enable_seen))
write("PEER_DISABLE_OBSERVED=%d\n" % int(disable_seen))
write("PEER_PASTE_FRAMED=%s\n" % ("".join(framing) if framing else "-"))
write("PEER_CHILD_EXIT=%s\n" % proc.returncode)
write("PEER_TTY_BEGIN\n")
sys.stdout.flush()
sys.stdout.buffer.write(bytes(output))
sys.stdout.buffer.flush()
write("\nPEER_TTY_END\n")
write("PEER_REDIRECT_BEGIN\n")
sys.stdout.flush()
sys.stdout.buffer.write(redirected_bytes())
sys.stdout.buffer.flush()
write("\nPEER_REDIRECT_END\n")
sys.stdout.flush()
"#;

        struct PeerRun {
            stdout: String,
        }

        impl PeerRun {
            fn flag(&self, key: &str) -> String {
                let needle = format!("{key}=");
                self.stdout
                    .lines()
                    .find_map(|line| line.strip_prefix(needle.as_str()))
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            }

            fn section(&self, name: &str) -> &str {
                let begin = format!("PEER_{name}_BEGIN\n");
                let end = format!("\nPEER_{name}_END");
                let Some(start) = self.stdout.find(begin.as_str()).map(|at| at + begin.len())
                else {
                    return "";
                };
                match self.stdout[start..].find(end.as_str()) {
                    Some(at) => &self.stdout[start..start + at],
                    None => "",
                }
            }

            fn enable_observed(&self) -> bool {
                self.flag("PEER_ENABLE_OBSERVED") == "1"
            }

            fn disable_observed(&self) -> bool {
                self.flag("PEER_DISABLE_OBSERVED") == "1"
            }

            fn paste_framing(&self) -> String {
                self.flag("PEER_PASTE_FRAMED")
            }

            fn tty(&self) -> &str {
                self.section("TTY")
            }

            fn redirected(&self) -> &str {
                self.section("REDIRECT")
            }

            /// Everything the application produced, wherever it landed.
            fn app_output(&self) -> String {
                format!("{}{}", self.tty(), self.redirected())
            }

            fn submits(&self) -> Vec<String> {
                self.app_output()
                    .lines()
                    .filter_map(|line| {
                        line.split_once("SUBMIT=")
                            .map(|(_, rest)| rest.trim().to_string())
                    })
                    .collect()
            }
        }

        fn hex(bytes: &[u8]) -> String {
            use std::fmt::Write as _;

            bytes.iter().fold(String::new(), |mut encoded, byte| {
                let _ = write!(encoded, "{byte:02x}");
                encoded
            })
        }

        fn op_type(text: &str) -> String {
            format!("type:{}", hex(text.as_bytes()))
        }

        fn op_send(bytes: &[u8]) -> String {
            format!("send:{}", hex(bytes))
        }

        fn op_paste(text: &str) -> String {
            format!("paste:{}", hex(text.as_bytes()))
        }

        fn run_peer_child(child: &str, term: &str, ops: &[String], redirect: &str) -> PeerRun {
            let output = Command::new("python3")
                .arg("-c")
                .arg(PEER_SCRIPT)
                .arg(std::env::current_exe().expect("test exe should exist"))
                .arg(child)
                .arg(term)
                .arg(ops.join(";"))
                .arg(redirect)
                .output()
                .expect("terminal peer harness should run");
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            assert!(
                output.status.success(),
                "peer harness failed: status={:?} stdout={stdout:?} stderr={:?}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            );
            PeerRun { stdout }
        }

        fn run_peer(term: &str, ops: &[String]) -> PeerRun {
            run_peer_child(SINGLE_READ_CHILD, term, ops, "")
        }

        fn assert_single_submit(run: &PeerRun, expected: &str) {
            let output = run.app_output();
            assert_eq!(
                output.matches("SUBMIT=").count(),
                1,
                "expected exactly one submit in output: {output:?}"
            );
            assert!(
                output.contains(&format!("SUBMIT={expected:?}")),
                "missing submitted value {expected:?} in output: {output:?}"
            );
        }

        fn assert_no_marker_leakage(run: &PeerRun) {
            let output = run.app_output();
            assert!(!output.contains("\\u{1b}[200~"), "{output:?}");
            assert!(!output.contains("\\u{1b}[201~"), "{output:?}");
        }

        /// Ops for a genuine user paste on a terminal Stack-Code drives
        /// natively: wait for the prompt, wait until the application has
        /// actually asked the terminal for bracketed paste, then paste, then
        /// press Enter.
        fn real_paste_ops(prefix: Option<&str>, content: &str) -> Vec<String> {
            let mut ops = vec!["wait_prompt".to_string(), "wait_enable".to_string()];
            if let Some(prefix) = prefix {
                ops.push(op_type(prefix));
            }
            ops.push(op_paste(content));
            ops.push(op_send(b"\r"));
            ops
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

        /// Multi-turn child, used to observe how many logical messages a
        /// single physical paste becomes.
        #[test]
        #[ignore]
        fn line_editor_pty_child_loop() {
            if std::env::var_os("STACK_CODE_LINE_EDITOR_PTY_CHILD").is_none() {
                return;
            }

            let mut editor = LineEditor::new("> ", Vec::new());
            loop {
                match editor.read_line().expect("read should complete") {
                    ReadOutcome::Submit(line) => {
                        println!("SUBMIT={line:?}");
                        editor.push_history(line);
                    }
                    ReadOutcome::Cancel => println!("CANCEL"),
                    ReadOutcome::Exit => {
                        println!("EXIT");
                        break;
                    }
                }
            }
        }

        // ---------------------------------------------------------------
        // Tier A: terminals Stack-Code drives through rustyline.
        //
        // These are the tests that close the P1 finding. The peer refuses to
        // frame until it has seen `ESC[?2004h`, so they can only pass if the
        // application really negotiated the mode.
        // ---------------------------------------------------------------

        #[test]
        fn real_paste_under_xterm_requires_enable_and_yields_one_message() {
            let content = "Create the project.\n\nRequirements:\n- one\n- two";
            let run = run_peer("xterm", &real_paste_ops(None, content));

            assert!(
                run.enable_observed(),
                "application never enabled bracketed paste: {:?}",
                run.tty()
            );
            assert_eq!(
                run.paste_framing(),
                "1",
                "peer refused to frame because the mode was not active"
            );
            assert_single_submit(&run, content);
            assert!(run.app_output().contains("HISTORY_LEN=1"));
            assert_no_marker_leakage(&run);
        }

        #[test]
        fn real_paste_under_xterm_256color_requires_enable_and_yields_one_message() {
            let content = "Create the project.\n\nRequirements:\n- one\n- two";
            let run = run_peer("xterm-256color", &real_paste_ops(None, content));

            assert!(run.enable_observed(), "{:?}", run.tty());
            assert_eq!(run.paste_framing(), "1");
            assert_single_submit(&run, content);
            assert!(run.app_output().contains("HISTORY_LEN=1"));
            assert_no_marker_leakage(&run);
        }

        #[test]
        fn real_paste_after_typed_prefix_is_one_message() {
            let run = run_peer(
                "xterm-256color",
                &real_paste_ops(Some("prefix: "), "line1\nline2"),
            );

            assert!(run.enable_observed(), "{:?}", run.tty());
            assert_eq!(run.paste_framing(), "1");
            assert_single_submit(&run, "prefix: line1\nline2");
            assert_no_marker_leakage(&run);
        }

        #[test]
        fn large_bounded_real_paste_stays_one_message() {
            let content = (0..128)
                .map(|index| format!("requirement line {index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let run = run_peer("xterm-256color", &real_paste_ops(None, &content));

            assert!(run.enable_observed());
            assert_eq!(run.paste_framing(), "1");
            assert_single_submit(&run, &content);
            assert!(run.app_output().contains("HISTORY_LEN=1"));
        }

        #[test]
        fn single_line_real_paste_submits_normally_on_enter() {
            let run = run_peer("xterm-256color", &real_paste_ops(None, "hello world"));

            assert!(run.enable_observed());
            assert_single_submit(&run, "hello world");
        }

        #[test]
        fn native_editor_pairs_bracketed_paste_disable_with_enable() {
            let run = run_peer("xterm-256color", &real_paste_ops(None, "one\ntwo"));

            assert!(run.enable_observed(), "enable never observed");
            assert!(
                run.disable_observed(),
                "bracketed paste was enabled but never disabled: {:?}",
                run.tty()
            );
        }

        #[test]
        fn native_enter_submits_single_line() {
            let ops = vec![
                "wait_prompt".to_string(),
                "wait_enable".to_string(),
                op_type("hello"),
                op_send(b"\r"),
            ];
            let run = run_peer("xterm-256color", &ops);

            assert_single_submit(&run, "hello");
        }

        #[test]
        fn native_ctrl_j_preserves_embedded_newline_until_enter() {
            let ops = vec![
                "wait_prompt".to_string(),
                "wait_enable".to_string(),
                op_type("hello"),
                op_send(b"\n"),
                op_type("world"),
                op_send(b"\r"),
            ];
            let run = run_peer("xterm-256color", &ops);

            assert_single_submit(&run, "hello\nworld");
        }

        #[test]
        fn native_ctrl_c_on_empty_line_exits() {
            let ops = vec![
                "wait_prompt".to_string(),
                "wait_enable".to_string(),
                op_send(b"\x03"),
            ];
            let run = run_peer("xterm-256color", &ops);

            assert_eq!(run.app_output().matches("SUBMIT=").count(), 0);
            assert!(run.app_output().contains("EXIT"), "{:?}", run.tty());
            assert!(run.disable_observed(), "raw mode was not restored");
        }

        #[test]
        fn native_ctrl_c_on_nonempty_line_cancels_without_submitting() {
            let ops = vec![
                "wait_prompt".to_string(),
                "wait_enable".to_string(),
                op_type("partial"),
                op_send(b"\x03"),
            ];
            let run = run_peer("xterm-256color", &ops);

            assert_eq!(run.app_output().matches("SUBMIT=").count(), 0);
            assert!(run.app_output().contains("CANCEL"), "{:?}", run.tty());
            assert!(run.disable_observed(), "raw mode was not restored");
        }

        // ---------------------------------------------------------------
        // Tier C: genuinely limited or unknown terminals.
        //
        // Stack-Code asserts no capabilities here at all. It must never ask
        // such a terminal to turn bracketed paste on, and must never take
        // ownership of echo or editing.
        // ---------------------------------------------------------------

        /// One ordinary cooked line, plus the invariant that matters most on
        /// this tier: Stack-Code requested nothing of the terminal.
        fn assert_cooked_ordinary_line(term: &str) {
            let ops = vec!["wait_prompt".to_string(), op_type("hello"), op_send(b"\r")];
            let run = run_peer(term, &ops);

            assert_single_submit(&run, "hello");
            assert!(
                !run.enable_observed(),
                "Stack-Code asked a limited terminal ({term}) for bracketed paste: {:?}",
                run.tty()
            );
            assert!(
                !run.disable_observed(),
                "Stack-Code emitted a bracketed-paste disable to {term}"
            );
        }

        #[test]
        fn term_dumb_is_cooked_and_requests_no_capabilities() {
            assert_cooked_ordinary_line("dumb");
        }

        #[test]
        fn term_cons25_is_cooked_and_requests_no_capabilities() {
            assert_cooked_ordinary_line("cons25");
        }

        #[test]
        fn term_emacs_is_cooked_and_requests_no_capabilities() {
            assert_cooked_ordinary_line("emacs");
        }

        #[test]
        fn term_unset_is_cooked_and_requests_no_capabilities() {
            assert_cooked_ordinary_line(TERM_UNSET_SENTINEL);
        }

        #[test]
        fn term_empty_is_cooked_and_requests_no_capabilities() {
            assert_cooked_ordinary_line("");
        }

        /// INTENTIONAL LIMITATION, asserted so it can never regress silently.
        ///
        /// A terminal that never received a bracketed-paste enable does not
        /// frame its pastes, so pasted newlines are ordinary line
        /// terminators and the paste arrives as several logical messages.
        /// Stack-Code does not fabricate framing to hide this: it cannot
        /// prove such a terminal would understand the request.
        ///
        /// Tier A (see the real-paste tests above) is where a multiline
        /// paste becomes one message.
        #[test]
        fn limited_terminal_unframed_paste_submits_line_by_line_by_design() {
            let ops = vec![
                "wait_prompt".to_string(),
                op_paste("line one\nline two\nline three\n"),
                op_send(b"\x04"),
            ];
            let run = run_peer_child(LOOP_CHILD, "dumb", &ops, "");

            assert!(
                !run.enable_observed(),
                "limited terminal must not be asked for bracketed paste"
            );
            assert_eq!(
                run.paste_framing(),
                "0",
                "peer must not frame a paste the application never enabled"
            );
            assert_eq!(
                run.submits(),
                vec![
                    "\"line one\"".to_string(),
                    "\"line two\"".to_string(),
                    "\"line three\"".to_string(),
                ],
                "honest cooked behavior changed: {:?}",
                run.tty()
            );
            assert!(run.app_output().contains("EXIT"));
        }

        #[test]
        fn cooked_backspace_erases_last_typed_character() {
            let ops = vec![
                "wait_prompt".to_string(),
                op_type("helloo"),
                op_send(b"\x7f"),
                op_send(b"\r"),
            ];
            let run = run_peer("dumb", &ops);

            assert_single_submit(&run, "hello");
        }

        /// The UTF-8 backspace finding, closed by delegation rather than by
        /// another editor: the line discipline erases the whole character.
        /// The peer sets `IUTF8` on the pty, which is what a real UTF-8
        /// terminal has set.
        #[test]
        fn cooked_backspace_removes_a_whole_multibyte_character() {
            let ops = vec![
                "wait_prompt".to_string(),
                op_type("é"),
                op_send(b"\x7f"),
                op_type("x"),
                op_send(b"\r"),
            ];
            let run = run_peer("dumb", &ops);

            assert_single_submit(&run, "x");
            assert!(
                !run.app_output().contains("InvalidData"),
                "partial UTF-8 survived the erase: {:?}",
                run.tty()
            );
        }

        #[test]
        fn partial_begin_marker_then_enter_is_not_lost_on_limited_terminal() {
            let ops = vec![
                "wait_prompt".to_string(),
                op_send(b"\x1b[20"),
                op_send(b"\r"),
            ];
            let run = run_peer("dumb", &ops);

            assert_single_submit(&run, "\x1b[20");
        }

        #[test]
        fn lone_escape_then_enter_is_not_lost_on_limited_terminal() {
            let ops = vec!["wait_prompt".to_string(), op_send(b"\x1b"), op_send(b"\r")];
            let run = run_peer("dumb", &ops);

            assert_single_submit(&run, "\x1b");
        }

        // ---------------------------------------------------------------
        // Tier D: interactive stdin with non-interactive stdout.
        // ---------------------------------------------------------------

        fn temp_redirect_path(tag: &str) -> std::path::PathBuf {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after the epoch")
                .as_nanos();
            std::env::temp_dir().join(format!(
                "stack-code-pr176-{tag}-{}-{nanos}.out",
                std::process::id()
            ))
        }

        /// Closes the redirected-stdout finding.
        ///
        /// With stdin a tty and stdout a file, Stack-Code must leave the tty
        /// cooked: the terminal keeps echoing (so the user is not typing
        /// blind) and no keystroke or erase byte is written into the captured
        /// output.
        #[test]
        fn stdin_tty_with_redirected_stdout_stays_cooked_and_uncontaminated() {
            let path = temp_redirect_path("redirected-stdout");
            let ops = vec![
                "wait_prompt".to_string(),
                op_type("hello"),
                op_send(b"\x7f"),
                op_send(b"\r"),
            ];
            let run = run_peer_child(
                SINGLE_READ_CHILD,
                "dumb",
                &ops,
                path.to_str().expect("temp path should be utf-8"),
            );
            let _ = std::fs::remove_file(&path);

            assert!(
                !run.enable_observed(),
                "no capability request may be made here"
            );

            // Terminal echo still belongs to the tty: raw mode would have
            // silenced this completely.
            let tty = run.tty();
            assert!(
                tty.contains("hell"),
                "terminal echo was suppressed, so the user types blind: {tty:?}"
            );
            assert!(
                tty.contains('\u{8}'),
                "line-discipline erase echo missing from tty: {tty:?}"
            );

            // Captured output carries no input bytes and no erase sequences.
            let redirected = run.redirected();
            assert!(
                !redirected.contains('\u{8}'),
                "erase bytes contaminated redirected stdout: {redirected:?}"
            );
            assert!(
                !redirected.contains("hello"),
                "raw keystroke echo contaminated redirected stdout: {redirected:?}"
            );

            // And the line is still read correctly.
            assert_single_submit(&run, "hell");
        }
    }
}
