use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::io::{self, IsTerminal, Read, Write};
use std::mem;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
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

/// A capability-safe accumulator for direct (non-rustyline) line input.
///
/// Recognizes an exact bracketed-paste begin marker only at the very start
/// of a fresh line, then treats every subsequent byte as literal paste
/// content (including control bytes) until the exact end marker is seen.
/// Both markers are matched incrementally so they are recognized correctly
/// even when the underlying reads split them across multiple syscalls.
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

    fn is_empty(&self) -> bool {
        self.content.is_empty() && self.begin_match == 0 && self.end_match == 0 && !self.in_paste
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

    /// Feeds one byte into the accumulator. Returns the bytes (if any) that
    /// were appended to the visible content as a direct result, so the
    /// caller can echo exactly what changed.
    fn push(&mut self, byte: u8) -> io::Result<Vec<u8>> {
        let mut echoed = Vec::new();

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
                return Ok(echoed);
            }

            if self.end_match > 0 {
                let flushed = BRACKETED_PASTE_END[..self.end_match].to_vec();
                self.append(&flushed)?;
                echoed.extend_from_slice(&flushed);
                self.end_match = 0;

                if byte == BRACKETED_PASTE_END[0] {
                    self.end_match = 1;
                    return Ok(echoed);
                }
            }

            self.append(&[byte])?;
            echoed.push(byte);
            return Ok(echoed);
        }

        if self.content.is_empty() && self.begin_match < BRACKETED_PASTE_BEGIN.len() {
            if byte == BRACKETED_PASTE_BEGIN[self.begin_match] {
                self.begin_match += 1;
                if self.begin_match == BRACKETED_PASTE_BEGIN.len() {
                    self.in_paste = true;
                    self.begin_match = 0;
                }
                return Ok(echoed);
            }

            if self.begin_match > 0 {
                let flushed = BRACKETED_PASTE_BEGIN[..self.begin_match].to_vec();
                self.append(&flushed)?;
                echoed.extend_from_slice(&flushed);
                self.begin_match = 0;
            }
        }

        self.append(&[byte])?;
        echoed.push(byte);
        Ok(echoed)
    }

    /// Removes the last visible content byte, returning `true` if a
    /// previously echoed byte was removed (caller should erase it on
    /// screen). Rewinds a pending, never-echoed begin-marker match
    /// silently, without signaling a visible erase.
    fn backspace(&mut self) -> bool {
        if self.content.pop().is_some() {
            return true;
        }
        if self.begin_match > 0 {
            self.begin_match -= 1;
        }
        false
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

/// RAII guard that restores the terminal's previous mode on drop, even on
/// early return via `?`.
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
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
/// capability-safe direct path rather than guessing.
fn is_current_term_supported() -> bool {
    match env::var_os("TERM") {
        Some(term) if !term.is_empty() => !is_rustyline_unsupported_term(&term),
        _ => false,
    }
}

fn should_use_line_editor() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && is_current_term_supported()
}

fn write_echo(stdout: &mut io::Stdout, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    stdout.write_all(bytes)?;
    stdout.flush()
}

/// Direct, capability-safe line reader for an interactive but rustyline-
/// unsupported terminal (or one with unset/empty `TERM`). Uses raw mode
/// only to read and echo bytes one at a time and to intercept Ctrl-C/
/// Ctrl-D/backspace as plain data; it never emits cursor-addressing or
/// color escape sequences and never forces a different `TERM` value.
///
/// `\r` (Enter) submits. `\n` (Ctrl-J) inserts a literal newline without
/// submitting, mirroring the supported-terminal newline key binding. A
/// bracketed paste is collected verbatim (including any embedded control
/// bytes) between the exact begin/end markers and folded into a single
/// logical submission once the trailing Enter is pressed.
///
/// Emits the prompt itself, and only after raw mode is active, so that a
/// visible prompt is a reliable readiness barrier: any peer that waits for
/// the prompt before writing is guaranteed to find this reader already
/// owning the tty. Flushing the prompt first would leave a window in which
/// the line discipline still owns it, and input arriving in that window is
/// cooked rather than delivered raw -- control bytes echoed as `^[`-style
/// renderings, and Enter (`\r`) translated to `\n` by ICRNL. A translated
/// `\n` is Ctrl-J here, which inserts a newline instead of submitting, so
/// the read would never complete.
fn read_line_capability_safe_interactive(
    stdout: &mut io::Stdout,
    prompt: &str,
) -> io::Result<ReadOutcome> {
    let _raw_mode = RawModeGuard::enable()?;
    write!(stdout, "{prompt}")?;
    stdout.flush()?;
    let mut stdin = io::stdin().lock();
    let mut reader = PasteAwareBuffer::new();
    let mut byte = [0u8; 1];

    loop {
        let n = stdin.read(&mut byte)?;
        if n == 0 {
            write!(stdout, "\r\n")?;
            stdout.flush()?;
            return Ok(if reader.is_empty() {
                ReadOutcome::Exit
            } else {
                ReadOutcome::Submit(reader.take_line(false)?)
            });
        }
        let b = byte[0];

        if reader.in_paste() {
            let echoed = reader.push(b)?;
            write_echo(stdout, &echoed)?;
            continue;
        }

        match b {
            b'\r' => {
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(ReadOutcome::Submit(reader.take_line(false)?));
            }
            0x03 => {
                let has_input = !reader.is_empty();
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(if has_input {
                    ReadOutcome::Cancel
                } else {
                    ReadOutcome::Exit
                });
            }
            0x04 if reader.is_empty() => {
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                return Ok(ReadOutcome::Exit);
            }
            0x7f | 0x08 => {
                if reader.backspace() {
                    write_echo(stdout, b"\x08 \x08")?;
                }
            }
            _ => {
                let echoed = reader.push(b)?;
                write_echo(stdout, &echoed)?;
            }
        }
    }
}

/// Direct line reader for non-terminal (piped/redirected) stdin.
///
/// Matches the prior `BufRead::read_line` fallback contract: LF alone
/// terminates a line, a trailing run of CR/LF is stripped (so CRLF is one
/// logical terminator and never yields a spurious empty turn), and a bare CR
/// is ordinary content. Bracketed-paste payload is collected verbatim between
/// markers and is never touched by terminator stripping.
fn read_line_capability_safe_piped(source: &mut impl Read) -> io::Result<ReadOutcome> {
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

        if reader.in_paste() {
            reader.push(b)?;
            continue;
        }

        if b == b'\n' {
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

    fn read_line_fallback(&self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();

        if io::stdin().is_terminal() {
            // The interactive reader emits the prompt itself, after raw mode
            // is enabled, so a visible prompt implies a ready reader.
            read_line_capability_safe_interactive(&mut stdout, &self.prompt)
        } else {
            write!(stdout, "{}", self.prompt)?;
            stdout.flush()?;
            read_line_capability_safe_piped(&mut io::stdin().lock())
        }
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

    mod direct_piped_tests {
        use super::super::{read_line_capability_safe_piped, ReadOutcome};

        fn drain(mut stream: &[u8]) -> Vec<ReadOutcome> {
            let mut outcomes = Vec::new();
            loop {
                let outcome = read_line_capability_safe_piped(&mut stream)
                    .expect("piped read should succeed");
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
            let error = read_line_capability_safe_piped(&mut stream)
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
            let mut result = Ok(Vec::new());
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
        fn backspace_removes_last_content_byte() {
            let mut buffer = PasteAwareBuffer::new();
            feed_all(&mut buffer, b"ab");
            assert!(buffer.backspace());
            assert_eq!(buffer.take_line(false).expect("valid utf-8"), "a");
        }

        #[test]
        fn backspace_on_empty_buffer_is_a_safe_no_op() {
            let mut buffer = PasteAwareBuffer::new();
            assert!(!buffer.backspace());
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

        #[test]
        fn is_empty_reflects_pending_begin_marker_progress() {
            let mut buffer = PasteAwareBuffer::new();
            assert!(buffer.is_empty());
            buffer.push(0x1b).unwrap();
            assert!(!buffer.is_empty());
        }
    }

    #[cfg(unix)]
    mod pty_tests {
        use super::LineEditor;
        use crate::input::ReadOutcome;
        use rustyline::history::History;
        use std::process::Command;

        const TERM_UNSET_SENTINEL: &str = "__STACK_CODE_TERM_UNSET__";

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
if term == "__STACK_CODE_TERM_UNSET__":
    env.pop("TERM", None)
else:
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

        fn assert_no_marker_leakage(output: &str) {
            assert!(!output.contains("\\u{1b}[200~"), "{output:?}");
            assert!(!output.contains("\\u{1b}[201~"), "{output:?}");
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
            assert_no_marker_leakage(&output);
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

        #[test]
        fn ctrl_c_on_nonempty_line_cancels_without_submitting() {
            let output = run_editor_child(b"partial\x03", "dumb");

            assert_eq!(output.matches("SUBMIT=").count(), 0, "{output:?}");
            assert!(output.contains("CANCEL"), "{output:?}");
        }

        #[test]
        fn backspace_erases_last_typed_character() {
            let output = run_editor_child(b"helloo\x7f\r", "dumb");

            assert_single_submit(&output, "hello");
        }

        #[test]
        fn term_cons25_normal_line() {
            let output = run_editor_child(b"hello\r", "cons25");

            assert_single_submit(&output, "hello");
        }

        #[test]
        fn term_cons25_bracketed_multiline() {
            let content = "line one\nline two";
            let output = run_editor_child(&bracketed_paste(content), "cons25");

            assert_single_submit(&output, content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
            assert_no_marker_leakage(&output);
        }

        #[test]
        fn term_emacs_normal_line() {
            let output = run_editor_child(b"hello\r", "emacs");

            assert_single_submit(&output, "hello");
        }

        #[test]
        fn term_emacs_bracketed_multiline() {
            let content = "line one\nline two";
            let output = run_editor_child(&bracketed_paste(content), "emacs");

            assert_single_submit(&output, content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
            assert_no_marker_leakage(&output);
        }

        #[test]
        fn term_xterm_bracketed_multiline_via_native_editor() {
            let content = "Create the project.\n\nRequirements:\n- one\n- two";
            let output = run_editor_child(&bracketed_paste(content), "xterm");

            assert_single_submit(&output, content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
            assert_no_marker_leakage(&output);
        }

        #[test]
        fn term_xterm_256color_bracketed_multiline_via_native_editor() {
            let content = "Create the project.\n\nRequirements:\n- one\n- two";
            let output = run_editor_child(&bracketed_paste(content), "xterm-256color");

            assert_single_submit(&output, content);
            assert!(output.contains("HISTORY_LEN=1"), "{output:?}");
            assert_no_marker_leakage(&output);
        }

        #[test]
        fn partial_begin_marker_then_enter_is_not_lost_under_dumb_term() {
            let output = run_editor_child(b"\x1b[20\r", "dumb");

            assert_single_submit(&output, "\x1b[20");
        }

        #[test]
        fn lone_escape_then_enter_is_not_lost_under_dumb_term() {
            let output = run_editor_child(b"\x1b\r", "dumb");

            assert_single_submit(&output, "\x1b");
        }

        #[test]
        fn term_unset_falls_back_to_capability_safe_direct_path() {
            let output = run_editor_child(b"hello\r", TERM_UNSET_SENTINEL);

            assert_single_submit(&output, "hello");
        }

        #[test]
        fn term_empty_falls_back_to_capability_safe_direct_path() {
            let output = run_editor_child(b"hello\r", "");

            assert_single_submit(&output, "hello");
        }
    }
}
