use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glob::Pattern;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

/// Maximum file size that can be read (10 MB).
const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum file size that can be written (10 MB).
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

/// Check whether a file appears to contain binary content by examining
/// the first chunk for NUL bytes.
fn is_binary_file(path: &Path) -> io::Result<bool> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buffer = [0u8; 8192];
    let bytes_read = file.read(&mut buffer)?;
    Ok(buffer[..bytes_read].contains(&0))
}

/// Validate that a resolved path stays within the given workspace root.
/// Returns the canonical path on success, or an error if the path escapes
/// the workspace boundary (e.g. via `../` traversal or symlink).
#[allow(dead_code)]
fn validate_workspace_boundary(resolved: &Path, workspace_root: &Path) -> io::Result<()> {
    if !resolved.starts_with(workspace_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "path {} escapes workspace boundary {}",
                resolved.display(),
                workspace_root.display()
            ),
        ));
    }
    Ok(())
}

/// Text payload returned by file-reading operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextFilePayload {
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub content: String,
    #[serde(rename = "numLines")]
    pub num_lines: usize,
    #[serde(rename = "startLine")]
    pub start_line: usize,
    #[serde(rename = "totalLines")]
    pub total_lines: usize,
}

/// Output envelope for the `read_file` tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadFileOutput {
    #[serde(rename = "type")]
    pub kind: String,
    pub file: TextFilePayload,
}

/// Structured patch hunk emitted by write and edit operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredPatchHunk {
    #[serde(rename = "oldStart")]
    pub old_start: usize,
    #[serde(rename = "oldLines")]
    pub old_lines: usize,
    #[serde(rename = "newStart")]
    pub new_start: usize,
    #[serde(rename = "newLines")]
    pub new_lines: usize,
    pub lines: Vec<String>,
}

/// Output envelope for full-file write operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteFileOutput {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub content: String,
    #[serde(rename = "structuredPatch")]
    pub structured_patch: Vec<StructuredPatchHunk>,
    #[serde(rename = "originalFile")]
    pub original_file: Option<String>,
    #[serde(rename = "gitDiff")]
    pub git_diff: Option<serde_json::Value>,
}

/// Output envelope for targeted string-replacement edits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditFileOutput {
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// Bounded unified diff of this one edit, before → after. Declared ahead
    /// of the bulky fields so it is serialized, and read, first.
    #[serde(rename = "operationDiff", default)]
    pub operation_diff: String,
    #[serde(rename = "oldString")]
    pub old_string: String,
    #[serde(rename = "newString")]
    pub new_string: String,
    #[serde(rename = "originalFile")]
    pub original_file: String,
    #[serde(rename = "structuredPatch")]
    pub structured_patch: Vec<StructuredPatchHunk>,
    #[serde(rename = "userModified")]
    pub user_modified: bool,
    #[serde(rename = "replaceAll")]
    pub replace_all: bool,
    #[serde(rename = "gitDiff")]
    pub git_diff: Option<serde_json::Value>,
}

/// Which side of its anchor an anchored insertion lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    /// `insert_before`: the inserted text ends where the anchor begins.
    Before,
    /// `insert_after`: the inserted text begins where the anchor ends.
    After,
}

/// Output envelope for anchored insertions (`insert_before`, `insert_after`).
/// Nothing is replaced, so `operationDiff` is the whole story of the change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InsertTextOutput {
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub success: bool,
    /// Bounded unified diff of this one insertion, before → after.
    #[serde(rename = "operationDiff")]
    pub operation_diff: String,
}

/// Result of a glob-based filename search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobSearchOutput {
    #[serde(rename = "durationMs")]
    pub duration_ms: u128,
    #[serde(rename = "numFiles")]
    pub num_files: usize,
    pub filenames: Vec<String>,
    pub truncated: bool,
}

/// Parameters accepted by the grep-style search tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrepSearchInput {
    pub pattern: String,
    pub path: Option<String>,
    pub glob: Option<String>,
    #[serde(rename = "output_mode")]
    pub output_mode: Option<String>,
    #[serde(rename = "-B")]
    pub before: Option<usize>,
    #[serde(rename = "-A")]
    pub after: Option<usize>,
    #[serde(rename = "-C")]
    pub context_short: Option<usize>,
    pub context: Option<usize>,
    #[serde(rename = "-n")]
    pub line_numbers: Option<bool>,
    #[serde(rename = "-i")]
    pub case_insensitive: Option<bool>,
    #[serde(rename = "type")]
    pub file_type: Option<String>,
    pub head_limit: Option<usize>,
    pub offset: Option<usize>,
    pub multiline: Option<bool>,
}

/// Result payload returned by the grep-style search tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrepSearchOutput {
    pub mode: Option<String>,
    #[serde(rename = "numFiles")]
    pub num_files: usize,
    pub filenames: Vec<String>,
    pub content: Option<String>,
    #[serde(rename = "numLines")]
    pub num_lines: Option<usize>,
    #[serde(rename = "numMatches")]
    pub num_matches: Option<usize>,
    #[serde(rename = "appliedLimit")]
    pub applied_limit: Option<usize>,
    #[serde(rename = "appliedOffset")]
    pub applied_offset: Option<usize>,
}

/// Reads a text file and returns a line-windowed payload.
pub fn read_file(
    path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> io::Result<ReadFileOutput> {
    let absolute_path = normalize_path(path)?;

    // Check file size before reading
    let metadata = fs::metadata(&absolute_path)?;
    if metadata.len() > MAX_READ_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "file is too large ({} bytes, max {} bytes)",
                metadata.len(),
                MAX_READ_SIZE
            ),
        ));
    }

    // Detect binary files
    if is_binary_file(&absolute_path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file appears to be binary",
        ));
    }

    let content = fs::read_to_string(&absolute_path)?;
    Ok(text_window(
        absolute_path.to_string_lossy().into_owned(),
        &content,
        offset,
        limit,
    ))
}

fn text_window(
    file_path: String,
    content: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> ReadFileOutput {
    let lines: Vec<&str> = content.lines().collect();
    let start_index = offset.unwrap_or(0).min(lines.len());
    let end_index = limit.map_or(lines.len(), |limit| {
        start_index.saturating_add(limit).min(lines.len())
    });
    let selected = lines[start_index..end_index].join("\n");

    ReadFileOutput {
        kind: String::from("text"),
        file: TextFilePayload {
            file_path,
            content: selected,
            num_lines: end_index.saturating_sub(start_index),
            start_line: start_index.saturating_add(1),
            total_lines: lines.len(),
        },
    }
}

/// Replaces a file's contents and returns patch metadata.
pub fn write_file(path: &str, content: &str) -> io::Result<WriteFileOutput> {
    if content.len() > MAX_WRITE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "content is too large ({} bytes, max {} bytes)",
                content.len(),
                MAX_WRITE_SIZE
            ),
        ));
    }

    let absolute_path = normalize_path_allow_missing(path)?;
    let original_file = fs::read_to_string(&absolute_path).ok();
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&absolute_path, content)?;

    Ok(WriteFileOutput {
        kind: if original_file.is_some() {
            String::from("update")
        } else {
            String::from("create")
        },
        file_path: absolute_path.to_string_lossy().into_owned(),
        content: content.to_owned(),
        structured_patch: make_patch(original_file.as_deref().unwrap_or(""), content),
        original_file,
        git_diff: None,
    })
}

/// Performs an in-file string replacement and returns patch metadata.
pub fn edit_file(
    path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> io::Result<EditFileOutput> {
    let absolute_path = normalize_path(path)?;
    let original_file = fs::read_to_string(&absolute_path)?;
    if old_string == new_string {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "old_string and new_string must differ",
        ));
    }
    if !original_file.contains(old_string) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "old_string not found in file",
        ));
    }

    let updated = if replace_all {
        original_file.replace(old_string, new_string)
    } else {
        original_file.replacen(old_string, new_string, 1)
    };
    fs::write(&absolute_path, &updated)?;

    let file_path = absolute_path.to_string_lossy().into_owned();
    Ok(EditFileOutput {
        operation_diff: make_operation_diff(&file_path, &original_file, &updated),
        file_path,
        old_string: old_string.to_owned(),
        new_string: new_string.to_owned(),
        original_file: original_file.clone(),
        structured_patch: make_patch(&original_file, &updated),
        user_modified: false,
        replace_all,
        git_diff: None,
    })
}

/// Inserts `content` immediately before or after the one occurrence of
/// `anchor` in an existing file, changing no existing byte.
pub fn insert_text(
    path: &str,
    anchor: &str,
    content: &str,
    position: InsertPosition,
) -> io::Result<InsertTextOutput> {
    let absolute_path = normalize_path(path)?;
    let original_file = fs::read_to_string(&absolute_path)?;
    let updated = insert_at_anchor(&original_file, anchor, content, position)?;
    fs::write(&absolute_path, &updated)?;

    let file_path = absolute_path.to_string_lossy().into_owned();
    Ok(InsertTextOutput {
        operation_diff: make_operation_diff(&file_path, &original_file, &updated),
        file_path,
        success: true,
    })
}

/// `original` with `content` inserted immediately before or after the one
/// occurrence of `anchor`: `original[..at] + content + original[at..]`, where
/// `at` is where the anchor starts (`Before`) or ends (`After`). Every
/// original byte, the anchor included, is kept as it was; only `content` is
/// added, verbatim, with no newline, indentation or line ending of its own.
///
/// Refused before anything is built: an empty anchor or content, an anchor
/// that does not occur, one that occurs more than once (overlapping
/// occurrences count), and a result over [`MAX_WRITE_SIZE`].
fn insert_at_anchor(
    original: &str,
    anchor: &str,
    content: &str,
    position: InsertPosition,
) -> io::Result<String> {
    if anchor.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "anchor must not be empty",
        ));
    }
    if content.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "content must not be empty",
        ));
    }
    let Some(start) = original.find(anchor) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "anchor not found in file",
        ));
    };
    // A second occurrence may overlap the first, so look again from the
    // anchor's second character rather than from its end.
    let second = start + anchor.chars().next().map_or(1, char::len_utf8);
    if original[second..].contains(anchor) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "anchor occurs more than once in file; it must occur exactly once",
        ));
    }
    let size = original.len().saturating_add(content.len());
    if size > MAX_WRITE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file would be too large after insertion ({size} bytes, max {MAX_WRITE_SIZE} bytes)"),
        ));
    }
    let at = match position {
        InsertPosition::Before => start,
        InsertPosition::After => start + anchor.len(),
    };
    let mut updated = String::with_capacity(size);
    updated.push_str(&original[..at]);
    updated.push_str(content);
    updated.push_str(&original[at..]);
    Ok(updated)
}

/// Expands a glob pattern and returns matching filenames.
pub fn glob_search(pattern: &str, path: Option<&str>) -> io::Result<GlobSearchOutput> {
    let started = Instant::now();
    let base_dir = path
        .map(normalize_path)
        .transpose()?
        .unwrap_or(std::env::current_dir()?);
    let search_pattern = if Path::new(pattern).is_absolute() {
        pattern.to_owned()
    } else {
        base_dir.join(pattern).to_string_lossy().into_owned()
    };

    // The `glob` crate does not support brace expansion ({a,b,c}).
    // Expand braces into multiple patterns so patterns like
    // `Assets/**/*.{cs,uxml,uss}` work correctly.
    let expanded = expand_braces(&search_pattern);

    let mut seen = std::collections::HashSet::new();
    let mut matches = Vec::new();
    for pat in &expanded {
        let entries = glob::glob(pat)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        for entry in entries.flatten() {
            if entry.is_file() && seen.insert(entry.clone()) {
                matches.push(entry);
            }
        }
    }

    matches.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(Reverse)
    });

    let truncated = matches.len() > 100;
    let filenames = matches
        .into_iter()
        .take(100)
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    Ok(GlobSearchOutput {
        duration_ms: started.elapsed().as_millis(),
        num_files: filenames.len(),
        filenames,
        truncated,
    })
}

/// Runs a regex search over workspace files with optional context lines.
pub fn grep_search(input: &GrepSearchInput) -> io::Result<GrepSearchOutput> {
    let base_path = input
        .path
        .as_deref()
        .map(normalize_path)
        .transpose()?
        .unwrap_or(std::env::current_dir()?);

    grep_files(
        input,
        || collect_search_files(&base_path),
        |path| fs::read_to_string(path).ok(),
    )
}

/// Shared grep core: `files` enumerates candidates (after the pattern and
/// filters are validated) and `read` yields a candidate's text, or `None` to
/// skip it.
fn grep_files(
    input: &GrepSearchInput,
    files: impl FnOnce() -> io::Result<Vec<PathBuf>>,
    read: impl Fn(&Path) -> Option<String>,
) -> io::Result<GrepSearchOutput> {
    let regex = RegexBuilder::new(&input.pattern)
        .case_insensitive(input.case_insensitive.unwrap_or(false))
        .dot_matches_new_line(input.multiline.unwrap_or(false))
        .build()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;

    let glob_filter = input
        .glob
        .as_deref()
        .map(Pattern::new)
        .transpose()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let file_type = input.file_type.as_deref();
    let output_mode = input
        .output_mode
        .clone()
        .unwrap_or_else(|| String::from("files_with_matches"));
    let context = input.context.or(input.context_short).unwrap_or(0);

    let mut filenames = Vec::new();
    let mut content_lines = Vec::new();
    let mut total_matches = 0usize;

    for file_path in files()? {
        if !matches_optional_filters(&file_path, glob_filter.as_ref(), file_type) {
            continue;
        }

        let Some(file_contents) = read(&file_path) else {
            continue;
        };

        if output_mode == "count" {
            let count = regex.find_iter(&file_contents).count();
            if count > 0 {
                filenames.push(file_path.to_string_lossy().into_owned());
                total_matches += count;
            }
            continue;
        }

        let lines: Vec<&str> = file_contents.lines().collect();
        let mut matched_lines = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                total_matches += 1;
                matched_lines.push(index);
            }
        }

        if matched_lines.is_empty() {
            continue;
        }

        filenames.push(file_path.to_string_lossy().into_owned());
        if output_mode == "content" {
            for index in matched_lines {
                let start = index.saturating_sub(input.before.unwrap_or(context));
                let end = (index + input.after.unwrap_or(context) + 1).min(lines.len());
                for (current, line) in lines.iter().enumerate().take(end).skip(start) {
                    let prefix = if input.line_numbers.unwrap_or(true) {
                        format!("{}:{}:", file_path.to_string_lossy(), current + 1)
                    } else {
                        format!("{}:", file_path.to_string_lossy())
                    };
                    content_lines.push(format!("{prefix}{line}"));
                }
            }
        }
    }

    let (filenames, applied_limit, applied_offset) =
        apply_limit(filenames, input.head_limit, input.offset);
    let content_output = if output_mode == "content" {
        let (lines, limit, offset) = apply_limit(content_lines, input.head_limit, input.offset);
        return Ok(GrepSearchOutput {
            mode: Some(output_mode),
            num_files: filenames.len(),
            filenames,
            num_lines: Some(lines.len()),
            content: Some(lines.join("\n")),
            num_matches: None,
            applied_limit: limit,
            applied_offset: offset,
        });
    } else {
        None
    };

    Ok(GrepSearchOutput {
        mode: Some(output_mode.clone()),
        num_files: filenames.len(),
        filenames,
        content: content_output,
        num_lines: None,
        num_matches: (output_mode == "count").then_some(total_matches),
        applied_limit,
        applied_offset,
    })
}

fn collect_search_files(base_path: &Path) -> io::Result<Vec<PathBuf>> {
    if base_path.is_file() {
        return Ok(vec![base_path.to_path_buf()]);
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(base_path) {
        let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn matches_optional_filters(
    path: &Path,
    glob_filter: Option<&Pattern>,
    file_type: Option<&str>,
) -> bool {
    if let Some(glob_filter) = glob_filter {
        let path_string = path.to_string_lossy();
        if !glob_filter.matches(&path_string) && !glob_filter.matches_path(path) {
            return false;
        }
    }

    if let Some(file_type) = file_type {
        let extension = path.extension().and_then(|extension| extension.to_str());
        if extension != Some(file_type) {
            return false;
        }
    }

    true
}

fn apply_limit<T>(
    items: Vec<T>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> (Vec<T>, Option<usize>, Option<usize>) {
    let offset_value = offset.unwrap_or(0);
    let mut items = items.into_iter().skip(offset_value).collect::<Vec<_>>();
    let explicit_limit = limit.unwrap_or(250);
    if explicit_limit == 0 {
        return (items, None, (offset_value > 0).then_some(offset_value));
    }

    let truncated = items.len() > explicit_limit;
    items.truncate(explicit_limit);
    (
        items,
        truncated.then_some(explicit_limit),
        (offset_value > 0).then_some(offset_value),
    )
}

fn make_patch(original: &str, updated: &str) -> Vec<StructuredPatchHunk> {
    let mut lines = Vec::new();
    for line in original.lines() {
        lines.push(format!("-{line}"));
    }
    for line in updated.lines() {
        lines.push(format!("+{line}"));
    }

    vec![StructuredPatchHunk {
        old_start: 1,
        old_lines: original.lines().count(),
        new_start: 1,
        new_lines: updated.lines().count(),
        lines,
    }]
}

/// Context lines around each change in `operationDiff`: the radius the A2 diff
/// preview (`a2-plan-runner`) already uses.
const OPERATION_DIFF_CONTEXT_LINES: usize = 3;

/// Byte budget for `operationDiff`, truncation marker included: the 16 KiB the
/// bash tool already allows for model-visible output (`bash::MAX_OUTPUT_BYTES`).
const MAX_OPERATION_DIFF_BYTES: usize = 16_384;

/// Bounds on the line search behind `operationDiff`: edit distance in lines
/// (its trace needs memory quadratic in it) and total search steps. Past
/// either, the changed region is reported as one replacement block, which is
/// still exact, only not minimal.
const OPERATION_DIFF_MAX_EDIT_DISTANCE: usize = 1_024;
const OPERATION_DIFF_MAX_SEARCH_STEPS: usize = 10_000_000;

/// One maximal run of changed lines: `old_start..old_end` of the original is
/// replaced by `new_start..new_end` of the update.
#[derive(Debug, Clone, Copy)]
struct ChangedLines {
    old_start: usize,
    old_end: usize,
    new_start: usize,
    new_end: usize,
}

/// Renders `operationDiff`: the unified diff of one edit, derived only from the
/// before/after text the edit already holds. Hunks carry three context lines
/// and conventional ranges; whatever exceeds [`MAX_OPERATION_DIFF_BYTES`] is
/// replaced by an explicit marker, never silently dropped.
fn make_operation_diff(path: &str, original: &str, updated: &str) -> String {
    let old: Vec<&str> = original.split_inclusive('\n').collect();
    let new: Vec<&str> = updated.split_inclusive('\n').collect();
    let context = OPERATION_DIFF_CONTEXT_LINES;
    let changes = changed_lines(&old, &new);

    let mut diff = BoundedDiff::default();
    diff.push(&format!("--- {path}\n"));
    diff.push(&format!("+++ {path}\n"));
    let mut rest = changes.as_slice();
    while let Some(&first) = rest.first() {
        // One hunk spans every change whose unchanged gap fits two contexts.
        let len = 1 + rest
            .windows(2)
            .take_while(|pair| pair[1].old_start - pair[0].old_end <= 2 * context)
            .count();
        let (group, tail) = rest.split_at(len);
        rest = tail;
        let last = group[len - 1];
        let old_start = first.old_start.saturating_sub(context);
        let old_end = (last.old_end + context).min(old.len());
        let new_start = first.new_start - (first.old_start - old_start);
        let new_end = last.new_end + (old_end - last.old_end);
        diff.push(&format!(
            "@@ -{} +{} @@\n",
            hunk_range(old_start, old_end),
            hunk_range(new_start, new_end)
        ));
        let mut cursor = old_start;
        for change in group {
            diff.lines(' ', &old[cursor..change.old_start]);
            diff.lines('-', &old[change.old_start..change.old_end]);
            diff.lines('+', &new[change.new_start..change.new_end]);
            cursor = change.old_end;
        }
        diff.lines(' ', &old[cursor..old_end]);
    }
    diff.finish()
}

/// `start,len` of one side of a hunk; an empty side names the line before it.
fn hunk_range(start: usize, end: usize) -> String {
    let len = end - start;
    format!("{},{len}", if len == 0 { start } else { start + 1 })
}

/// Maximal runs of changed lines between `old` and `new`. Lines keep their
/// terminators, so a dropped final newline counts as a change.
fn changed_lines(old: &[&str], new: &[&str]) -> Vec<ChangedLines> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    if old_mid.is_empty() && new_mid.is_empty() {
        return Vec::new();
    }
    let whole = ChangedLines {
        old_start: 0,
        old_end: old_mid.len(),
        new_start: 0,
        new_end: new_mid.len(),
    };
    let mut changes = if old_mid.is_empty() || new_mid.is_empty() {
        vec![whole]
    } else {
        shortest_edit(old_mid, new_mid).unwrap_or_else(|| vec![whole])
    };
    for change in &mut changes {
        change.old_start += prefix;
        change.old_end += prefix;
        change.new_start += prefix;
        change.new_end += prefix;
    }
    changes
}

/// Myers' greedy O(ND) search for a shortest line edit script, returned as
/// maximal changed runs. `None` once the search bounds above are exceeded.
// Keeps the paper's names (`n`, `m`, `d`, `k`, `v`, `x`, `y`) for review.
#[allow(clippy::many_single_char_names)]
fn shortest_edit(old: &[&str], new: &[&str]) -> Option<Vec<ChangedLines>> {
    let (n, m) = (old.len(), new.len());
    let max_d = (n + m).min(OPERATION_DIFF_MAX_EDIT_DISTANCE);
    // `v[offset + k]` is the furthest `x` reached on diagonal `k = x - y`;
    // `trace[d]` keeps the `v[offset - d - 1..=offset + d + 1]` that step `d`
    // started from, which is all the walk back needs.
    let offset = max_d + 1;
    let mut v = vec![0_usize; 2 * max_d + 3];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut steps = 0_usize;
    let mut distance = None;
    'search: for d in 0..=max_d {
        trace.push(v[offset - d - 1..=offset + d + 1].to_vec());
        for k in (offset - d..=offset + d).step_by(2) {
            let mut x = if k == offset - d || (k != offset + d && v[k - 1] < v[k + 1]) {
                v[k + 1]
            } else {
                v[k - 1] + 1
            };
            let mut y = (x + offset).checked_sub(k)?;
            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
                steps += 1;
            }
            steps += 1;
            if steps > OPERATION_DIFF_MAX_SEARCH_STEPS {
                return None;
            }
            v[k] = x;
            if x >= n && y >= m {
                distance = Some(d);
                break 'search;
            }
        }
    }

    // Walk back from the end, replaying each step's choice to recover its
    // single insertion (down) or deletion (right); edits arrive in reverse.
    let distance = distance?;
    let mut edits = Vec::with_capacity(distance);
    let (mut x, mut y) = (n, m);
    for d in (1..=distance).rev() {
        let at = |k: usize| trace[d][k + d + 1 - offset];
        let k = offset + x - y;
        let down = k == offset - d || (k != offset + d && at(k - 1) < at(k + 1));
        let prev_k = if down { k + 1 } else { k - 1 };
        let prev_x = at(prev_k);
        let prev_y = (prev_x + offset).checked_sub(prev_k)?;
        edits.push(ChangedLines {
            old_start: prev_x,
            old_end: prev_x + usize::from(!down),
            new_start: prev_y,
            new_end: prev_y + usize::from(down),
        });
        (x, y) = (prev_x, prev_y);
    }

    let mut runs: Vec<ChangedLines> = Vec::new();
    for edit in edits.into_iter().rev() {
        match runs.last_mut() {
            Some(run) if run.old_end == edit.old_start && run.new_end == edit.new_start => {
                run.old_end = edit.old_end;
                run.new_end = edit.new_end;
            }
            _ => runs.push(edit),
        }
    }
    Some(runs)
}

/// Diff text capped at [`MAX_OPERATION_DIFF_BYTES`]. The complete diff is
/// rendered first, so `finish` bounds what it actually is.
#[derive(Default)]
struct BoundedDiff {
    text: String,
}

impl BoundedDiff {
    fn push(&mut self, line: &str) {
        self.text.push_str(line);
    }

    fn lines(&mut self, tag: char, lines: &[&str]) {
        for line in lines {
            self.text.push(tag);
            self.text.push_str(line);
            if !line.ends_with('\n') {
                self.text.push_str("\n\\ No newline at end of file\n");
            }
        }
    }

    fn finish(self) -> String {
        bound_operation_diff(self.text)
    }
}

/// A complete rendered diff within [`MAX_OPERATION_DIFF_BYTES`] is returned
/// unchanged. A longer one keeps the longest prefix of whole rendered lines
/// that fits together with a marker giving the exact number of rendered lines
/// after it, `\ No newline at end of file` annotations included.
fn bound_operation_diff(diff: String) -> String {
    if diff.len() <= MAX_OPERATION_DIFF_BYTES {
        return diff;
    }
    let marker = |omitted: usize| {
        format!(
            "[operation diff truncated — exceeded {MAX_OPERATION_DIFF_BYTES} bytes; {omitted} more diff lines omitted]\n"
        )
    };
    let total = diff.split_inclusive('\n').count();
    // Keeping one more line never shortens the result: the line adds at least
    // a byte and the marker loses at most a digit. So the first line that no
    // longer fits ends the longest prefix that does.
    let (mut kept, mut kept_bytes) = (0, 0);
    for line in diff.split_inclusive('\n') {
        let omitted_after = total - kept - 1;
        if kept_bytes + line.len() + marker(omitted_after).len() > MAX_OPERATION_DIFF_BYTES {
            break;
        }
        kept += 1;
        kept_bytes += line.len();
    }
    let mut bounded = diff;
    bounded.truncate(kept_bytes);
    bounded.push_str(&marker(total - kept));
    bounded
}

fn normalize_path(path: &str) -> io::Result<PathBuf> {
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()?.join(path)
    };
    candidate.canonicalize()
}

fn normalize_path_allow_missing(path: &str) -> io::Result<PathBuf> {
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()?.join(path)
    };

    if let Ok(canonical) = candidate.canonicalize() {
        return Ok(canonical);
    }

    if let Some(parent) = candidate.parent() {
        let canonical_parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        if let Some(name) = candidate.file_name() {
            return Ok(canonical_parent.join(name));
        }
    }

    Ok(candidate)
}

/// Read a file with workspace boundary enforcement.
#[allow(dead_code)]
pub fn read_file_in_workspace(
    path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
    workspace_root: &Path,
) -> io::Result<ReadFileOutput> {
    let absolute_path = normalize_path(path)?;
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    validate_workspace_boundary(&absolute_path, &canonical_root)?;
    read_file(path, offset, limit)
}

/// Write a file with workspace boundary enforcement.
#[allow(dead_code)]
pub fn write_file_in_workspace(
    path: &str,
    content: &str,
    workspace_root: &Path,
) -> io::Result<WriteFileOutput> {
    let absolute_path = normalize_path_allow_missing(path)?;
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    validate_workspace_boundary(&absolute_path, &canonical_root)?;
    write_file(path, content)
}

/// Edit a file with workspace boundary enforcement.
#[allow(dead_code)]
pub fn edit_file_in_workspace(
    path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
    workspace_root: &Path,
) -> io::Result<EditFileOutput> {
    let absolute_path = normalize_path(path)?;
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    validate_workspace_boundary(&absolute_path, &canonical_root)?;
    edit_file(path, old_string, new_string, replace_all)
}

/// Check whether a path is a symlink that resolves outside the workspace.
#[allow(dead_code)]
pub fn is_symlink_escape(path: &Path, workspace_root: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_symlink() {
        return Ok(false);
    }
    let resolved = path.canonicalize()?;
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    Ok(!resolved.starts_with(&canonical_root))
}

/// Parse a confined tool path into workspace-relative components.
///
/// Lexical half of the confined-path contract: absolute paths and `..` are
/// refused outright. The kernel half (`RESOLVE_BENEATH` and friends in
/// [`WorkspaceRoot`]) is what actually keeps resolution beneath the root.
fn workspace_relative(path: &str) -> io::Result<PathBuf> {
    use std::path::Component;

    if path.is_empty() {
        return Err(confinement_denied(path, "is empty"));
    }
    let mut relative = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(confinement_denied(path, "contains `..`")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(confinement_denied(path, "is absolute"));
            }
        }
    }
    Ok(relative)
}

fn confinement_denied(path: &str, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("path `{path}` refused by workspace confinement: {reason}"),
    )
}

#[cfg(target_os = "linux")]
pub use confined::WorkspaceRoot;

/// Descriptor-bound workspace authority for confined (controlled-smoke) file
/// tools.
///
/// The workspace directory is opened once. Every later access resolves
/// relative to that descriptor with `openat2(RESOLVE_BENEATH |
/// RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV)`, and all
/// type checks and I/O happen on the descriptor that was opened; nothing is
/// ever re-resolved by pathname, and the process CWD is never consulted.
#[cfg(target_os = "linux")]
mod confined {
    use std::ffi::OsStr;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    use rustix::fd::OwnedFd;
    use rustix::fs::{
        fstat, openat2, statat, AtFlags, Dir, FileType, Mode, OFlags, ResolveFlags, Stat,
    };
    use rustix::io::Errno;

    use super::{
        confinement_denied, expand_braces, fs, grep_files, insert_at_anchor, io,
        make_operation_diff, make_patch, text_window, workspace_relative, EditFileOutput,
        GlobSearchOutput, GrepSearchInput, GrepSearchOutput, InsertPosition, InsertTextOutput,
        Instant, Path, PathBuf, Pattern, ReadFileOutput, Reverse, WriteFileOutput, MAX_READ_SIZE,
        MAX_WRITE_SIZE,
    };

    const RESOLVE: ResolveFlags = ResolveFlags::BENEATH
        .union(ResolveFlags::NO_SYMLINKS)
        .union(ResolveFlags::NO_MAGICLINKS)
        .union(ResolveFlags::NO_XDEV);

    /// Directory recursion bound for confined glob/grep enumeration.
    const MAX_WALK_DEPTH: usize = 64;
    /// Entry bound for confined glob/grep enumeration.
    const MAX_WALK_ENTRIES: usize = 100_000;

    #[derive(Debug)]
    pub struct WorkspaceRoot {
        dir: OwnedFd,
        display: PathBuf,
        dev: u64,
        ino: u64,
        writable: Vec<PathBuf>,
    }

    impl WorkspaceRoot {
        /// Open `root` once and bind it as the confinement authority.
        ///
        /// `writable` lists the only workspace-relative files that confined
        /// writes and edits may modify. They must already exist; confined
        /// tools never create files or directories.
        pub fn bind(root: &Path, writable: &[&str]) -> io::Result<Self> {
            let dir = rustix::fs::open(
                root,
                OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| {
                io::Error::new(
                    io::Error::from(error).kind(),
                    format!("workspace confinement root {}: {error}", root.display()),
                )
            })?;
            let stat = fstat(&dir)?;
            let writable = writable
                .iter()
                .map(|name| {
                    let relative = workspace_relative(name)?;
                    if relative.as_os_str().is_empty() {
                        return Err(confinement_denied(name, "is not a file"));
                    }
                    Ok(relative)
                })
                .collect::<io::Result<Vec<_>>>()?;
            Ok(Self {
                display: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
                dev: stat.st_dev,
                ino: stat.st_ino,
                dir,
                writable,
            })
        }

        /// Human-readable root, for messages only; never used for access.
        #[must_use]
        pub fn display(&self) -> &Path {
            &self.display
        }

        /// `(st_dev, st_ino)` of the bound directory.
        #[must_use]
        pub fn identity(&self) -> (u64, u64) {
            (self.dev, self.ino)
        }

        /// The bound directory's current host path, resolved through the
        /// held descriptor (it follows renames of the bound directory).
        pub fn current_path(&self) -> io::Result<PathBuf> {
            fs::read_link(format!("/proc/self/fd/{}", self.dir.as_raw_fd()))
        }

        /// Whether `path` names the bound directory itself (same inode).
        pub fn is_same_directory(&self, path: &Path) -> io::Result<bool> {
            let metadata = fs::metadata(path)?;
            Ok(metadata.is_dir() && metadata.dev() == self.dev && metadata.ino() == self.ino)
        }

        /// Verify that each named fixture file is a regular file with exactly
        /// one link. A hardlinked designated file is refused: writes through
        /// it would reach an inode that may also be reachable from outside.
        pub fn verify_single_link_files(&self, names: &[&str]) -> io::Result<()> {
            for name in names {
                let relative = workspace_relative(name)?;
                let (_, stat) = self.open_regular(&relative, OFlags::RDONLY, name)?;
                if stat.st_nlink != 1 {
                    return Err(confinement_denied(
                        name,
                        &format!(
                            "has {} hard links; the controlled fixture requires exactly one",
                            stat.st_nlink
                        ),
                    ));
                }
            }
            Ok(())
        }

        pub(super) fn open_beneath(
            &self,
            relative: &Path,
            flags: OFlags,
            shown: &str,
        ) -> io::Result<OwnedFd> {
            let target: &Path = if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            };
            openat2(
                &self.dir,
                target,
                flags | OFlags::CLOEXEC,
                Mode::empty(),
                RESOLVE,
            )
            .map_err(|error| resolution_error(shown, error))
        }

        /// Open a regular file beneath the root and return it with the
        /// `fstat` of the very descriptor that was opened.
        fn open_regular(
            &self,
            relative: &Path,
            flags: OFlags,
            shown: &str,
        ) -> io::Result<(fs::File, Stat)> {
            // O_NONBLOCK: opening a FIFO must not block before the type check.
            let fd =
                self.open_beneath(relative, flags | OFlags::NOCTTY | OFlags::NONBLOCK, shown)?;
            let stat = fstat(&fd)?;
            if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
                return Err(confinement_denied(shown, "is not a regular file"));
            }
            Ok((fs::File::from(fd), stat))
        }

        fn read_text(file: &mut fs::File, stat: &Stat, shown: &str) -> io::Result<String> {
            let size = u64::try_from(stat.st_size).unwrap_or(u64::MAX);
            if size > MAX_READ_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("file is too large ({size} bytes, max {MAX_READ_SIZE} bytes)"),
                ));
            }
            let mut bytes = Vec::new();
            Read::by_ref(file)
                .take(MAX_READ_SIZE + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_READ_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("file `{shown}` grew beyond {MAX_READ_SIZE} bytes while reading"),
                ));
            }
            if bytes[..bytes.len().min(8192)].contains(&0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "file appears to be binary",
                ));
            }
            String::from_utf8(bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        }

        /// Confined `read_file`.
        pub fn read_file(
            &self,
            path: &str,
            offset: Option<usize>,
            limit: Option<usize>,
        ) -> io::Result<ReadFileOutput> {
            let relative = workspace_relative(path)?;
            let (mut file, stat) = self.open_regular(&relative, OFlags::RDONLY, path)?;
            let content = Self::read_text(&mut file, &stat, path)?;
            Ok(text_window(
                relative.to_string_lossy().into_owned(),
                &content,
                offset,
                limit,
            ))
        }

        /// Open a designated writable file for in-place update.
        fn open_designated(&self, path: &str) -> io::Result<(PathBuf, fs::File, String)> {
            let relative = workspace_relative(path)?;
            if !self.writable.contains(&relative) {
                return Err(confinement_denied(
                    path,
                    "is not a designated writable file",
                ));
            }
            let (mut file, stat) = self.open_regular(&relative, OFlags::RDWR, path)?;
            if stat.st_nlink != 1 {
                return Err(confinement_denied(
                    path,
                    &format!("has {} hard links", stat.st_nlink),
                ));
            }
            let original = Self::read_text(&mut file, &stat, path)?;
            Ok((relative, file, original))
        }

        fn replace_contents(file: &mut fs::File, content: &str) -> io::Result<()> {
            file.seek(SeekFrom::Start(0))?;
            file.set_len(0)?;
            file.write_all(content.as_bytes())?;
            file.flush()
        }

        /// Confined `write_file`: replaces a designated, existing file.
        pub fn write_file(&self, path: &str, content: &str) -> io::Result<WriteFileOutput> {
            if content.len() > MAX_WRITE_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "content is too large ({} bytes, max {MAX_WRITE_SIZE} bytes)",
                        content.len()
                    ),
                ));
            }
            let (relative, mut file, original) = self.open_designated(path)?;
            Self::replace_contents(&mut file, content)?;
            Ok(WriteFileOutput {
                kind: String::from("update"),
                file_path: relative.to_string_lossy().into_owned(),
                content: content.to_owned(),
                structured_patch: make_patch(&original, content),
                original_file: Some(original),
                git_diff: None,
            })
        }

        /// Confined `edit_file`: string replacement in a designated file.
        pub fn edit_file(
            &self,
            path: &str,
            old_string: &str,
            new_string: &str,
            replace_all: bool,
        ) -> io::Result<EditFileOutput> {
            let (relative, mut file, original) = self.open_designated(path)?;
            if old_string == new_string {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "old_string and new_string must differ",
                ));
            }
            if !original.contains(old_string) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "old_string not found in file",
                ));
            }
            let updated = if replace_all {
                original.replace(old_string, new_string)
            } else {
                original.replacen(old_string, new_string, 1)
            };
            if updated.len() > MAX_WRITE_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "edited content is too large",
                ));
            }
            Self::replace_contents(&mut file, &updated)?;
            let file_path = relative.to_string_lossy().into_owned();
            Ok(EditFileOutput {
                operation_diff: make_operation_diff(&file_path, &original, &updated),
                file_path,
                old_string: old_string.to_owned(),
                new_string: new_string.to_owned(),
                original_file: original.clone(),
                structured_patch: make_patch(&original, &updated),
                user_modified: false,
                replace_all,
                git_diff: None,
            })
        }

        /// Confined `insert_before`/`insert_after`: anchored insertion into
        /// a designated file, written through the descriptor it was read from.
        pub fn insert_text(
            &self,
            path: &str,
            anchor: &str,
            content: &str,
            position: InsertPosition,
        ) -> io::Result<InsertTextOutput> {
            let (relative, mut file, original) = self.open_designated(path)?;
            let updated = insert_at_anchor(&original, anchor, content, position)?;
            Self::replace_contents(&mut file, &updated)?;
            let file_path = relative.to_string_lossy().into_owned();
            Ok(InsertTextOutput {
                operation_diff: make_operation_diff(&file_path, &original, &updated),
                file_path,
                success: true,
            })
        }

        /// Enumerate regular files beneath `base` (workspace-relative),
        /// returning root-relative paths with their mtimes. Symlinks, special
        /// files and mount crossings are never followed or listed.
        fn walk_regular_files(&self, base: &Path, shown: &str) -> io::Result<Vec<(PathBuf, i64)>> {
            let fd = self.open_beneath(base, OFlags::PATH, shown)?;
            let stat = fstat(&fd)?;
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::RegularFile => return Ok(vec![(base.to_path_buf(), stat.st_mtime)]),
                FileType::Directory => {}
                _ => return Err(confinement_denied(shown, "is not a file or directory")),
            }
            let base_dir = self.open_beneath(base, OFlags::RDONLY | OFlags::DIRECTORY, shown)?;
            let mut files = Vec::new();
            let mut visited = 0usize;
            let mut stack = vec![(base_dir, base.to_path_buf(), 0usize)];
            while let Some((dir, prefix, depth)) = stack.pop() {
                for entry in Dir::read_from(&dir)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if name.to_bytes() == b"." || name.to_bytes() == b".." {
                        continue;
                    }
                    visited += 1;
                    if visited > MAX_WALK_ENTRIES {
                        return Ok(files);
                    }
                    let Ok(stat) = statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) else {
                        continue;
                    };
                    let relative = prefix.join(OsStr::from_bytes(name.to_bytes()));
                    match FileType::from_raw_mode(stat.st_mode) {
                        FileType::RegularFile => files.push((relative, stat.st_mtime)),
                        FileType::Directory if depth < MAX_WALK_DEPTH => {
                            if let Ok(child) = openat2(
                                &dir,
                                name,
                                OFlags::RDONLY
                                    | OFlags::DIRECTORY
                                    | OFlags::NOFOLLOW
                                    | OFlags::CLOEXEC,
                                Mode::empty(),
                                RESOLVE,
                            ) {
                                stack.push((child, relative, depth + 1));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(files)
        }

        /// Confined `glob_search`: patterns match workspace-relative paths of
        /// enumerated regular files; results are workspace-relative.
        pub fn glob_search(
            &self,
            pattern: &str,
            path: Option<&str>,
        ) -> io::Result<GlobSearchOutput> {
            let started = Instant::now();
            let base = path
                .map(workspace_relative)
                .transpose()?
                .unwrap_or_default();
            let mut compiled = Vec::new();
            for expanded in expand_braces(pattern) {
                let relative = workspace_relative(&expanded)?;
                compiled.push(Pattern::new(&relative.to_string_lossy()).map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
                })?);
            }
            let options = glob::MatchOptions {
                case_sensitive: true,
                require_literal_separator: true,
                require_literal_leading_dot: false,
            };
            let mut matches = self
                .walk_regular_files(&base, path.unwrap_or("."))?
                .into_iter()
                .filter(|(relative, _)| {
                    let below_base = relative.strip_prefix(&base).unwrap_or(relative);
                    compiled
                        .iter()
                        .any(|pattern| pattern.matches_path_with(below_base, options))
                })
                .collect::<Vec<_>>();
            matches.sort_by_key(|(_, mtime)| Reverse(*mtime));
            let truncated = matches.len() > 100;
            let filenames = matches
                .into_iter()
                .take(100)
                .map(|(relative, _)| relative.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            Ok(GlobSearchOutput {
                duration_ms: started.elapsed().as_millis(),
                num_files: filenames.len(),
                filenames,
                truncated,
            })
        }

        /// Confined `grep_search`: an omitted `path` searches the bound root,
        /// never the process CWD.
        pub fn grep_search(&self, input: &GrepSearchInput) -> io::Result<GrepSearchOutput> {
            let shown = input.path.as_deref().unwrap_or(".");
            let base = input
                .path
                .as_deref()
                .map(workspace_relative)
                .transpose()?
                .unwrap_or_default();
            grep_files(
                input,
                || {
                    Ok(self
                        .walk_regular_files(&base, shown)?
                        .into_iter()
                        .map(|(relative, _)| relative)
                        .collect())
                },
                |relative| {
                    let shown = relative.to_string_lossy();
                    let (mut file, stat) =
                        self.open_regular(relative, OFlags::RDONLY, &shown).ok()?;
                    Self::read_text(&mut file, &stat, &shown).ok()
                },
            )
        }
    }

    fn resolution_error(shown: &str, error: Errno) -> io::Error {
        match error {
            Errno::XDEV => confinement_denied(shown, "resolves outside the workspace"),
            Errno::LOOP => confinement_denied(shown, "traverses a symlink"),
            other => {
                let error = io::Error::from(other);
                io::Error::new(error.kind(), format!("`{shown}`: {error}"))
            }
        }
    }
}

/// Non-Linux stand-in: confinement needs `openat2`, so every operation fails
/// closed rather than degrading to unconfined path handling.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct WorkspaceRoot {
    _unconstructible: (),
}

#[cfg(not(target_os = "linux"))]
impl WorkspaceRoot {
    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "workspace confinement requires Linux (openat2)",
        )
    }
    pub fn bind(_root: &Path, _writable: &[&str]) -> io::Result<Self> {
        Err(Self::unsupported())
    }
    #[must_use]
    pub fn display(&self) -> &Path {
        Path::new("")
    }
    #[must_use]
    pub fn identity(&self) -> (u64, u64) {
        (0, 0)
    }
    pub fn current_path(&self) -> io::Result<PathBuf> {
        Err(Self::unsupported())
    }
    pub fn is_same_directory(&self, _path: &Path) -> io::Result<bool> {
        Err(Self::unsupported())
    }
    pub fn verify_single_link_files(&self, _names: &[&str]) -> io::Result<()> {
        Err(Self::unsupported())
    }
    pub fn read_file(
        &self,
        _path: &str,
        _offset: Option<usize>,
        _limit: Option<usize>,
    ) -> io::Result<ReadFileOutput> {
        Err(Self::unsupported())
    }
    pub fn write_file(&self, _path: &str, _content: &str) -> io::Result<WriteFileOutput> {
        Err(Self::unsupported())
    }
    pub fn edit_file(
        &self,
        _path: &str,
        _old_string: &str,
        _new_string: &str,
        _replace_all: bool,
    ) -> io::Result<EditFileOutput> {
        Err(Self::unsupported())
    }
    pub fn insert_text(
        &self,
        _path: &str,
        _anchor: &str,
        _content: &str,
        _position: InsertPosition,
    ) -> io::Result<InsertTextOutput> {
        Err(Self::unsupported())
    }
    pub fn glob_search(&self, _pattern: &str, _path: Option<&str>) -> io::Result<GlobSearchOutput> {
        Err(Self::unsupported())
    }
    pub fn grep_search(&self, _input: &GrepSearchInput) -> io::Result<GrepSearchOutput> {
        Err(Self::unsupported())
    }
}

/// Expand shell-style brace groups in a glob pattern.
///
/// Handles one level of braces: `foo.{a,b,c}` → `["foo.a", "foo.b", "foo.c"]`.
/// Nested braces are not expanded (uncommon in practice).
/// Patterns without braces pass through unchanged.
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let Some(close) = pattern[open..].find('}').map(|i| open + i) else {
        // Unmatched brace — treat as literal.
        return vec![pattern.to_owned()];
    };
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    let alternatives = &pattern[open + 1..close];
    alternatives
        .split(',')
        .flat_map(|alt| expand_braces(&format!("{prefix}{alt}{suffix}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        bound_operation_diff, edit_file, expand_braces, glob_search, grep_search, insert_at_anchor,
        insert_text, is_symlink_escape, make_operation_diff, read_file, read_file_in_workspace,
        write_file, GrepSearchInput, InsertPosition, MAX_WRITE_SIZE,
    };

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!("clawd-native-{name}-{unique}"))
    }

    #[test]
    fn reads_and_writes_files() {
        let path = temp_path("read-write.txt");
        let write_output = write_file(path.to_string_lossy().as_ref(), "one\ntwo\nthree")
            .expect("write should succeed");
        assert_eq!(write_output.kind, "create");

        let read_output = read_file(path.to_string_lossy().as_ref(), Some(1), Some(1))
            .expect("read should succeed");
        assert_eq!(read_output.file.content, "two");
    }

    #[test]
    fn edits_file_contents() {
        let path = temp_path("edit.txt");
        write_file(path.to_string_lossy().as_ref(), "alpha beta alpha")
            .expect("initial write should succeed");
        let output = edit_file(path.to_string_lossy().as_ref(), "alpha", "omega", true)
            .expect("edit should succeed");
        assert!(output.replace_all);
    }

    #[test]
    fn rejects_binary_files() {
        let path = temp_path("binary-test.bin");
        std::fs::write(&path, b"\x00\x01\x02\x03binary content").expect("write should succeed");
        let result = read_file(path.to_string_lossy().as_ref(), None, None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("binary"));
    }

    #[test]
    fn rejects_oversized_writes() {
        let path = temp_path("oversize-write.txt");
        let huge = "x".repeat(MAX_WRITE_SIZE + 1);
        let result = write_file(path.to_string_lossy().as_ref(), &huge);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("too large"));
    }

    #[test]
    fn enforces_workspace_boundary() {
        let workspace = temp_path("workspace-boundary");
        std::fs::create_dir_all(&workspace).expect("workspace dir should be created");
        let inside = workspace.join("inside.txt");
        write_file(inside.to_string_lossy().as_ref(), "safe content")
            .expect("write inside workspace should succeed");

        // Reading inside workspace should succeed
        let result =
            read_file_in_workspace(inside.to_string_lossy().as_ref(), None, None, &workspace);
        assert!(result.is_ok());

        // Reading outside workspace should fail
        let outside = temp_path("outside-boundary.txt");
        write_file(outside.to_string_lossy().as_ref(), "unsafe content")
            .expect("write outside should succeed");
        let result =
            read_file_in_workspace(outside.to_string_lossy().as_ref(), None, None, &workspace);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("escapes workspace"));
    }

    #[test]
    fn detects_symlink_escape() {
        let workspace = temp_path("symlink-workspace");
        std::fs::create_dir_all(&workspace).expect("workspace dir should be created");
        let outside = temp_path("symlink-target.txt");
        std::fs::write(&outside, "target content").expect("target should write");

        let link_path = workspace.join("escape-link.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &link_path).expect("symlink should create");
            assert!(is_symlink_escape(&link_path, &workspace).expect("check should succeed"));
        }

        // Non-symlink file should not be an escape
        let normal = workspace.join("normal.txt");
        std::fs::write(&normal, "normal content").expect("normal file should write");
        assert!(!is_symlink_escape(&normal, &workspace).expect("check should succeed"));
    }

    #[test]
    fn globs_and_greps_directory() {
        let dir = temp_path("search-dir");
        std::fs::create_dir_all(&dir).expect("directory should be created");
        let file = dir.join("demo.rs");
        write_file(
            file.to_string_lossy().as_ref(),
            "fn main() {\n println!(\"hello\");\n}\n",
        )
        .expect("file write should succeed");

        let globbed = glob_search("**/*.rs", Some(dir.to_string_lossy().as_ref()))
            .expect("glob should succeed");
        assert_eq!(globbed.num_files, 1);

        let grep_output = grep_search(&GrepSearchInput {
            pattern: String::from("hello"),
            path: Some(dir.to_string_lossy().into_owned()),
            glob: Some(String::from("**/*.rs")),
            output_mode: Some(String::from("content")),
            before: None,
            after: None,
            context_short: None,
            context: None,
            line_numbers: Some(true),
            case_insensitive: Some(false),
            file_type: None,
            head_limit: Some(10),
            offset: Some(0),
            multiline: Some(false),
        })
        .expect("grep should succeed");
        assert!(grep_output.content.unwrap_or_default().contains("hello"));
    }

    #[test]
    fn expand_braces_no_braces() {
        assert_eq!(expand_braces("*.rs"), vec!["*.rs"]);
    }

    #[test]
    fn expand_braces_single_group() {
        let mut result = expand_braces("Assets/**/*.{cs,uxml,uss}");
        result.sort();
        assert_eq!(
            result,
            vec!["Assets/**/*.cs", "Assets/**/*.uss", "Assets/**/*.uxml",]
        );
    }

    #[test]
    fn expand_braces_nested() {
        let mut result = expand_braces("src/{a,b}.{rs,toml}");
        result.sort();
        assert_eq!(
            result,
            vec!["src/a.rs", "src/a.toml", "src/b.rs", "src/b.toml"]
        );
    }

    #[test]
    fn expand_braces_unmatched() {
        assert_eq!(expand_braces("foo.{bar"), vec!["foo.{bar"]);
    }

    #[test]
    fn glob_search_with_braces_finds_files() {
        let dir = temp_path("glob-braces");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("b.toml"), "[package]").unwrap();
        std::fs::write(dir.join("c.txt"), "hello").unwrap();

        let result =
            glob_search("*.{rs,toml}", Some(dir.to_str().unwrap())).expect("glob should succeed");
        assert_eq!(
            result.num_files, 2,
            "should match .rs and .toml but not .txt"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Model-visible byte budget for `operationDiff`, truncation marker included.
    const OPERATION_DIFF_LIMIT: usize = 16_384;

    /// Runs one real `edit_file` on a fresh file and returns its path, the
    /// edited bytes and the serialized `operationDiff`.
    fn edit_and_diff(
        name: &str,
        before: &str,
        old: &str,
        new: &str,
        replace_all: bool,
    ) -> (String, String, String) {
        let path = temp_path(name);
        std::fs::write(&path, before).expect("seed file");
        let path = path.to_string_lossy().into_owned();
        let output = edit_file(&path, old, new, replace_all).expect("edit should succeed");
        let after = std::fs::read_to_string(&path).expect("read edited file");
        let _ = std::fs::remove_file(&path);
        let serialized = serde_json::to_value(&output).expect("edit output serializes");
        let diff = serialized["operationDiff"]
            .as_str()
            .expect("edit results must carry operationDiff")
            .to_owned();
        (output.file_path, after, diff)
    }

    /// Applies an `operationDiff` to `before`, asserting that every hunk
    /// header matches its body and that context stays conventional: three
    /// lines on each side unless the file ends first, disjoint hunks.
    fn apply_operation_diff(before: &str, diff: &str) -> String {
        let old: Vec<&str> = before.split_inclusive('\n').collect();
        let mut lines = diff.split_inclusive('\n').peekable();
        assert!(lines.next().is_some_and(|line| line.starts_with("--- ")));
        assert!(lines.next().is_some_and(|line| line.starts_with("+++ ")));
        let parse = |range: &str| {
            let (start, len) = range.split_once(',').expect("start,len");
            let start: usize = start.parse().expect("start");
            let len: usize = len.parse().expect("len");
            (if len == 0 { start } else { start - 1 }, len)
        };
        let mut out = String::new();
        let (mut cursor, mut out_lines) = (0, 0);
        while let Some(header) = lines.next() {
            let ranges = header
                .strip_prefix("@@ -")
                .and_then(|rest| rest.strip_suffix(" @@\n"))
                .unwrap_or_else(|| panic!("hunk header expected, got {header:?}"));
            let (old_range, new_range) = ranges.split_once(" +").expect("both ranges");
            let ((old_first, old_len), (new_first, new_len)) = (parse(old_range), parse(new_range));
            assert!(
                old_first > cursor || (cursor == 0 && old_first == 0),
                "hunks must be ordered, disjoint and separated: {header:?}"
            );
            for line in &old[cursor..old_first] {
                out.push_str(line);
            }
            out_lines += old_first - cursor;
            assert_eq!(new_first, out_lines, "new-side start of {header:?}");
            cursor = old_first;
            let (mut seen_old, mut seen_new, mut leading, mut trailing) = (0, 0, 0, 0);
            let mut changed = false;
            while seen_old < old_len || seen_new < new_len {
                let line = lines.next().expect("hunk body shorter than its header");
                let (tag, text) = line.split_at(1);
                let mut text = text.to_owned();
                if lines.peek() == Some(&"\\ No newline at end of file\n") {
                    lines.next();
                    assert_eq!(text.pop(), Some('\n'));
                }
                match tag {
                    " " => {
                        assert_eq!(old[cursor], text);
                        out.push_str(&text);
                        cursor += 1;
                        (seen_old, seen_new, out_lines) =
                            (seen_old + 1, seen_new + 1, out_lines + 1);
                        if changed {
                            trailing += 1;
                        } else {
                            leading += 1;
                        }
                    }
                    "-" | "+" => {
                        assert!(!changed || trailing <= 6, "unsplit gap in {header:?}");
                        if tag == "-" {
                            assert_eq!(old[cursor], text);
                            cursor += 1;
                            seen_old += 1;
                        } else {
                            out.push_str(&text);
                            seen_new += 1;
                            out_lines += 1;
                        }
                        changed = true;
                        trailing = 0;
                    }
                    _ => panic!("unexpected diff line {line:?}"),
                }
            }
            assert_eq!((seen_old, seen_new), (old_len, new_len), "{header:?}");
            assert!(changed, "hunk without a change: {header:?}");
            assert!(
                leading <= 3 && (leading == 3 || old_first == 0),
                "{header:?}"
            );
            assert!(
                trailing <= 3 && (trailing == 3 || cursor == old.len()),
                "{header:?}"
            );
        }
        for line in &old[cursor..] {
            out.push_str(line);
        }
        out
    }

    #[test]
    fn edit_result_diff_shows_a_substitution_with_bounded_context() {
        let before = "one\ntwo\nthree\nfour\nfoo = 1\nsix\nseven\neight\nnine\n";
        let (path, after, diff) =
            edit_and_diff("diff-substitution.txt", before, "foo = 1", "foo = 2", false);
        assert_eq!(
            after,
            "one\ntwo\nthree\nfour\nfoo = 2\nsix\nseven\neight\nnine\n"
        );
        assert_eq!(
            diff,
            format!(
                "--- {path}\n+++ {path}\n@@ -2,7 +2,7 @@\n two\n three\n four\n-foo = 1\n+foo = 2\n six\n seven\n eight\n"
            )
        );
        assert!(!diff.contains("one\n") && !diff.contains("nine"), "{diff}");
    }

    #[test]
    fn edit_result_diff_shows_an_insertion_as_additions_only() {
        let before = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let (path, after, diff) = edit_and_diff(
            "diff-insertion.txt",
            before,
            "beta\n",
            "beta\nINSERTED-1\nINSERTED-2\n",
            false,
        );
        assert_eq!(
            after,
            "alpha\nbeta\nINSERTED-1\nINSERTED-2\ngamma\ndelta\nepsilon\n"
        );
        assert_eq!(
            diff,
            format!(
                "--- {path}\n+++ {path}\n@@ -1,5 +1,7 @@\n alpha\n beta\n+INSERTED-1\n+INSERTED-2\n gamma\n delta\n epsilon\n"
            )
        );
    }

    #[test]
    fn edit_result_diff_shows_a_deletion_as_removed_lines() {
        let before = "keep-1\nkeep-2\ndrop-me\nalso-drop\nkeep-3\n";
        let (path, after, diff) = edit_and_diff(
            "diff-deletion.txt",
            before,
            "drop-me\nalso-drop\n",
            "",
            false,
        );
        assert_eq!(after, "keep-1\nkeep-2\nkeep-3\n");
        assert_eq!(
            diff,
            format!(
                "--- {path}\n+++ {path}\n@@ -1,5 +1,3 @@\n keep-1\n keep-2\n-drop-me\n-also-drop\n keep-3\n"
            )
        );
    }

    #[test]
    fn edit_result_diff_emits_separate_local_hunks() {
        let before: String = (1..=20)
            .map(|n| match n {
                2 => "x = TOKEN\n".to_string(),
                18 => "y = TOKEN\n".to_string(),
                _ => format!("line{n:02}\n"),
            })
            .collect();
        let (path, after, diff) = edit_and_diff("diff-hunks.txt", &before, "TOKEN", "VALUE", true);
        assert_eq!(after, before.replace("TOKEN", "VALUE"));
        assert_eq!(
            diff,
            format!(
                "--- {path}\n+++ {path}\n\
                 @@ -1,5 +1,5 @@\n line01\n-x = TOKEN\n+x = VALUE\n line03\n line04\n line05\n\
                 @@ -15,6 +15,6 @@\n line15\n line16\n line17\n-y = TOKEN\n+y = VALUE\n line19\n line20\n"
            )
        );
        assert!(!diff.contains("line10"), "{diff}");
    }

    #[test]
    fn edit_result_diff_keeps_sparse_changes_local_in_a_large_file() {
        let before: String = (0..3_000)
            .map(|n| {
                if n % 600 == 300 {
                    format!("mark {n} = OLD\n")
                } else {
                    format!("filler line {n}\n")
                }
            })
            .collect();
        let (_, after, diff) = edit_and_diff("diff-sparse.txt", &before, "OLD", "NEW", true);
        assert_eq!(after, before.replace("OLD", "NEW"));
        assert_eq!(diff.matches("\n@@ -").count(), 5, "{diff}");
        assert_eq!(diff.lines().filter(|line| line.starts_with('-')).count(), 6);
        assert!(diff.len() < 2_048, "{} bytes", diff.len());
        assert_eq!(apply_operation_diff(&before, &diff), after);
    }

    #[test]
    fn edit_result_diff_marks_a_missing_final_newline() {
        let (path, after, diff) =
            edit_and_diff("diff-eof.txt", "first\nlast\n", "last\n", "last", false);
        assert_eq!(after, "first\nlast");
        assert_eq!(
            diff,
            format!(
                "--- {path}\n+++ {path}\n@@ -1,2 +1,2 @@\n first\n-last\n+last\n\\ No newline at end of file\n"
            )
        );
    }

    #[test]
    fn edit_result_diff_is_bounded_with_an_explicit_truncation_marker() {
        use std::fmt::Write as _;

        let before = (0..2_000).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "value_{n:04} = OLD");
            text
        });
        let (_, after, diff) = edit_and_diff("diff-truncated.txt", &before, "OLD", "NEW", true);
        assert_eq!(after, before.replace("OLD", "NEW"));
        assert!(diff.len() <= OPERATION_DIFF_LIMIT, "{} bytes", diff.len());
        let (shown, marker) = diff
            .strip_suffix('\n')
            .and_then(|body| body.rsplit_once('\n'))
            .expect("a final marker line");
        let omitted: usize = marker
            .strip_prefix("[operation diff truncated — exceeded 16384 bytes; ")
            .and_then(|rest| rest.strip_suffix(" more diff lines omitted]"))
            .unwrap_or_else(|| panic!("explicit truncation marker expected, got {marker:?}"))
            .parse()
            .expect("omitted line count");
        // 2 file headers + 1 hunk header + 2 000 removed + 2 000 added lines.
        assert_eq!(shown.lines().count() + omitted, 4_003);
        assert!(
            shown.lines().count() > 100,
            "the budget is used, not skipped"
        );
        // Whatever is shown is whole lines: truncation never cuts one.
        for line in shown.lines().skip(3) {
            assert_eq!(line.len(), "-value_0000 = OLD".len(), "{line:?}");
        }
    }

    #[test]
    fn edit_result_diff_stays_exact_past_the_line_search_bound() {
        let before = "a\n".repeat(1_100);
        let (_, after, diff) = edit_and_diff("diff-wide.txt", &before, "a", "b", true);
        assert_eq!(after, "b\n".repeat(1_100));
        assert!(
            !diff.contains("[operation diff truncated"),
            "fits the budget"
        );
        assert_eq!(apply_operation_diff(&before, &diff), after);
    }

    #[test]
    fn edit_result_diff_round_trips_for_generated_edits() {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut below = |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            usize::try_from(state % u64::try_from(bound).expect("bound fits u64"))
                .expect("index fits usize")
        };
        let pieces = ["a\n", "b\n", "c\n", "\n", "ab", "a", "c"];
        for case in 0..400 {
            let before: String = (0..below(40))
                .map(|_| pieces[below(pieces.len())])
                .collect();
            if before.is_empty() {
                continue;
            }
            let start = below(before.len());
            let old = &before[start..=start + below(before.len() - start).min(12)];
            let new: String = (0..below(6)).map(|_| pieces[below(pieces.len())]).collect();
            if old == new {
                continue;
            }
            let replace_all = below(2) == 0;
            let (_, after, diff) = edit_and_diff(
                &format!("diff-roundtrip-{case}.txt"),
                &before,
                old,
                &new,
                replace_all,
            );
            let expected = if replace_all {
                before.replace(old, &new)
            } else {
                before.replacen(old, &new, 1)
            };
            assert_eq!(after, expected, "case {case}: edit semantics");
            assert_eq!(
                apply_operation_diff(&before, &diff),
                after,
                "case {case}: {before:?} {old:?} -> {new:?}\n{diff}"
            );
        }
    }

    /// The final line of an `operationDiff` that leaves `omitted` rendered
    /// diff lines out.
    fn truncation_marker(omitted: usize) -> String {
        format!(
            "[operation diff truncated — exceeded 16384 bytes; {omitted} more diff lines omitted]\n"
        )
    }

    /// What a complete rendered diff must become, worked out independently of
    /// the code under test: unchanged when it fits the budget; otherwise,
    /// trying the longest prefix of whole rendered lines first, the first one
    /// that fits together with the exact marker for the lines it leaves out.
    fn expected_bounded(full: &str) -> String {
        if full.len() <= OPERATION_DIFF_LIMIT {
            return full.to_owned();
        }
        let ends: Vec<usize> = full.match_indices('\n').map(|(at, _)| at + 1).collect();
        // A final segment without a terminator is a rendered line too.
        let total = full.lines().count();
        let prefix = |kept: usize| if kept == 0 { 0 } else { ends[kept - 1] };
        let kept = (0..total)
            .rev()
            .find(|&kept| {
                prefix(kept) + truncation_marker(total - kept).len() <= OPERATION_DIFF_LIMIT
            })
            .expect("the marker alone always fits");
        format!(
            "{}{}",
            &full[..prefix(kept)],
            truncation_marker(total - kept)
        )
    }

    /// Complete rendered diff of an edit whose before and after share no
    /// line: one hunk that removes every old line and adds every new one.
    fn whole_replacement_diff(path: &str, before: &str, after: &str) -> String {
        let side = |tag: char, text: &str| -> String {
            text.split_inclusive('\n')
                .map(|line| {
                    if line.ends_with('\n') {
                        format!("{tag}{line}")
                    } else {
                        format!("{tag}{line}\n\\ No newline at end of file\n")
                    }
                })
                .collect()
        };
        format!(
            "--- {path}\n+++ {path}\n@@ -1,{} +1,{} @@\n{}{}",
            before.lines().count(),
            after.lines().count(),
            side('-', before),
            side('+', after)
        )
    }

    /// The lines a truncated `operationDiff` shows, and the number of
    /// rendered lines its marker says are omitted.
    fn shown_and_omitted(diff: &str) -> (&str, usize) {
        let (shown, marker) = diff
            .rsplit_once("[operation diff truncated — exceeded 16384 bytes; ")
            .unwrap_or_else(|| panic!("truncation marker expected in {diff:?}"));
        let omitted = marker
            .strip_suffix(" more diff lines omitted]\n")
            .expect("marker ends the diff")
            .parse()
            .expect("omitted line count");
        (shown, omitted)
    }

    #[test]
    fn edit_result_diff_within_the_budget_is_complete_and_unmarked() {
        // (removed line, added line, final newlines, complete diff bytes):
        // the review's 16 272-byte case, one byte under, exactly at the budget.
        for (old_len, new_len, newline, complete_len) in [
            (8_120, 8_120, "\n", 16_272),
            (8_175, 8_176, "\n", 16_383),
            (8_176, 8_176, "\n", 16_384),
            (8_148, 8_148, "", 16_384),
        ] {
            let before = format!("{}{newline}", "a".repeat(old_len));
            let after = format!("{}{newline}", "b".repeat(new_len));
            let complete = whole_replacement_diff("f", &before, &after);
            assert_eq!(complete.len(), complete_len);
            let diff = make_operation_diff("f", &before, &after);
            assert_eq!(
                diff, complete,
                "a {complete_len}-byte diff is returned whole"
            );
            assert!(!diff.contains("[operation diff truncated"));
            assert!(diff.contains(&format!("\n+{}", after.trim_end())));
        }
    }

    #[test]
    fn edit_result_diff_one_byte_over_the_budget_is_marked_exactly() {
        let before = format!("{}\n", "a".repeat(8_176));
        let after = format!("{}\n", "b".repeat(8_177));
        let complete = whole_replacement_diff("f", &before, &after);
        assert_eq!(complete.len(), OPERATION_DIFF_LIMIT + 1);
        let diff = make_operation_diff("f", &before, &after);
        assert_eq!(
            diff,
            format!(
                "--- f\n+++ f\n@@ -1,1 +1,1 @@\n-{before}{}",
                truncation_marker(1)
            )
        );
        assert_eq!(diff, expected_bounded(&complete));

        // Without final newlines the added line takes its annotation with it.
        let (before, after) = ("a".repeat(8_148), "b".repeat(8_149));
        let complete = whole_replacement_diff("f", &before, &after);
        assert_eq!(complete.len(), OPERATION_DIFF_LIMIT + 1);
        let diff = make_operation_diff("f", &before, &after);
        assert_eq!(
            diff,
            format!(
                "--- f\n+++ f\n@@ -1,1 +1,1 @@\n-{before}\n\\ No newline at end of file\n{}",
                truncation_marker(2)
            )
        );
        assert_eq!(diff, expected_bounded(&complete));
    }

    #[test]
    fn edit_result_diff_far_over_the_budget_counts_every_omitted_line() {
        use std::fmt::Write as _;

        let before = (0..20_000).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "row {n:05} = OLD");
            text
        });
        let (path, after, diff) = edit_and_diff("diff-far-over.txt", &before, "OLD", "NEW", true);
        assert_eq!(after, before.replace("OLD", "NEW"));
        let complete = whole_replacement_diff(&path, &before, &after);
        assert!(complete.len() > 40 * OPERATION_DIFF_LIMIT);
        assert!(diff.len() <= OPERATION_DIFF_LIMIT, "{} bytes", diff.len());
        assert_eq!(diff, expected_bounded(&complete));
        let (shown, omitted) = shown_and_omitted(&diff);
        assert_eq!(shown.lines().count() + omitted, complete.lines().count());
        assert!(complete.starts_with(shown), "shown lines are a prefix");
    }

    #[test]
    fn edit_result_diff_never_cuts_an_oversized_line() {
        let (long_old, long_new) = ("a".repeat(20_000), "b".repeat(20_000));
        for (newline, omitted) in [("\n", 2), ("", 4)] {
            let before = format!("keep 1\nkeep 2\n{long_old}{newline}");
            let after = format!("keep 1\nkeep 2\n{long_new}{newline}");
            let annotation = if newline.is_empty() {
                "\\ No newline at end of file\n"
            } else {
                ""
            };
            let complete = format!(
                "--- f\n+++ f\n@@ -1,3 +1,3 @@\n keep 1\n keep 2\n-{long_old}\n{annotation}+{long_new}\n{annotation}"
            );
            let diff = make_operation_diff("f", &before, &after);
            assert_eq!(
                diff,
                format!(
                    "--- f\n+++ f\n@@ -1,3 +1,3 @@\n keep 1\n keep 2\n{}",
                    truncation_marker(omitted)
                )
            );
            assert_eq!(diff, expected_bounded(&complete));
            assert_eq!(complete.lines().count(), 5 + omitted);
        }
    }

    #[test]
    fn edit_result_diff_marker_width_follows_the_omitted_count() {
        // One long removed line, then added "b" lines of which `kept_added`
        // fit; `tight` sizes the removed line so that prefix plus the exact
        // marker for `target` omitted lines fills the budget to the byte. A
        // final oversized added line keeps the complete diff over the budget.
        let kept_added = 100;
        for target in [9, 10, 99, 100, 999, 1_000] {
            for missing_newline in [false, true] {
                // A missing final newline adds an annotation line to omit.
                let added = kept_added + target - usize::from(missing_newline);
                let mut after = "b\n".repeat(added - 1) + &"c".repeat(OPERATION_DIFF_LIMIT);
                if !missing_newline {
                    after.push('\n');
                }
                let headers = format!("--- f\n+++ f\n@@ -1,1 +1,{added} @@\n");
                let tight = OPERATION_DIFF_LIMIT
                    - headers.len()
                    - truncation_marker(target).len()
                    - 3 * kept_added
                    - 2;
                for long in tight - 2..=tight + 2 {
                    let before = format!("{}\n", "a".repeat(long));
                    let complete = whole_replacement_diff("f", &before, &after);
                    let diff = make_operation_diff("f", &before, &after);
                    let case = format!("target {target}, eof {missing_newline}, long {long}");
                    assert!(diff.len() <= OPERATION_DIFF_LIMIT, "{case}");
                    assert_eq!(diff, expected_bounded(&complete), "{case}");
                    let (shown, omitted) = shown_and_omitted(&diff);
                    assert_eq!(
                        shown.lines().count() + omitted,
                        complete.lines().count(),
                        "{case}"
                    );
                    if long == tight {
                        assert_eq!(
                            (diff.len(), omitted),
                            (OPERATION_DIFF_LIMIT, target),
                            "{case}"
                        );
                    }
                    if long == tight + 1 {
                        assert_eq!(omitted, target + 1, "{case}");
                    }
                }
            }
        }
    }

    #[test]
    fn diff_budget_counts_rendered_lines_including_an_unterminated_last_one() {
        // Within the budget any text comes back unchanged, terminated or not.
        assert_eq!(
            bound_operation_diff("--- f\n+++ f\nlast".to_owned()),
            "--- f\n+++ f\nlast"
        );

        // Newline-terminated: 8 151 two-byte lines fit beside a 4-digit marker.
        let terminated = "x\n".repeat(10_000);
        let bounded = bound_operation_diff(terminated.clone());
        assert_eq!(
            bounded,
            format!("{}{}", "x\n".repeat(8_151), truncation_marker(1_849))
        );
        assert_eq!(bounded.len(), OPERATION_DIFF_LIMIT);
        assert_eq!(bounded, expected_bounded(&terminated));

        // A final segment without a newline is one more rendered line.
        let unterminated = format!("{}tail", "x\n".repeat(9_000));
        let bounded = bound_operation_diff(unterminated.clone());
        assert_eq!(
            bounded,
            format!("{}{}", "x\n".repeat(8_151), truncation_marker(850))
        );
        assert_eq!(bounded, expected_bounded(&unterminated));

        // One annotation, then two, each counted as a rendered line.
        let eof = "\\ No newline at end of file\n";
        let long = "a".repeat(OPERATION_DIFF_LIMIT);
        let one = format!("--- f\n+++ f\n@@ -1,1 +1,1 @@\n-{long}\n{eof}+b\n");
        assert_eq!(
            bound_operation_diff(one.clone()),
            format!("--- f\n+++ f\n@@ -1,1 +1,1 @@\n{}", truncation_marker(3))
        );
        assert_eq!(bound_operation_diff(one.clone()), expected_bounded(&one));
        let two = format!("--- f\n+++ f\n@@ -1,1 +1,1 @@\n-b\n{eof}+{long}\n{eof}");
        assert_eq!(
            bound_operation_diff(two.clone()),
            format!(
                "--- f\n+++ f\n@@ -1,1 +1,1 @@\n-b\n{eof}{}",
                truncation_marker(2)
            )
        );
        assert_eq!(bound_operation_diff(two.clone()), expected_bounded(&two));
    }

    // --- anchored insertion ------------------------------------------------

    use super::InsertPosition::{After, Before};

    /// Inserts through the pure transform and checks the one invariant every
    /// success must satisfy: the result is the original split at the anchor
    /// boundary with `content` between, so removing exactly those bytes
    /// restores the original and the anchor sits where it was.
    fn insert_exactly(
        original: &str,
        anchor: &str,
        content: &str,
        position: InsertPosition,
    ) -> String {
        let updated = insert_at_anchor(original, anchor, content, position).expect("insert");
        let start = original.find(anchor).expect("anchor");
        let at = if position == Before {
            start
        } else {
            start + anchor.len()
        };
        assert_eq!(
            updated,
            format!("{}{content}{}", &original[..at], &original[at..])
        );
        assert_eq!(&updated[at..at + content.len()], content);
        let anchor_at = if position == Before {
            at + content.len()
        } else {
            start
        };
        assert_eq!(&updated[anchor_at..anchor_at + anchor.len()], anchor);
        let mut restored = updated.clone();
        restored.replace_range(at..at + content.len(), "");
        assert_eq!(restored, original, "only the content was added");
        updated
    }

    /// Runs the file-backed insertion, returning the result and the bytes
    /// on disk afterwards.
    fn insert_in_file(
        name: &str,
        before: &str,
        anchor: &str,
        content: &str,
        position: InsertPosition,
    ) -> (std::io::Result<super::InsertTextOutput>, String) {
        let path = temp_path(name);
        std::fs::write(&path, before).expect("seed file");
        let result = insert_text(path.to_str().expect("utf8"), anchor, content, position);
        let after = std::fs::read_to_string(&path).expect("read file");
        let _ = std::fs::remove_file(&path);
        (result, after)
    }

    #[test]
    fn insertion_keeps_the_anchor_on_both_sides() {
        let original = "AAA\nANCHOR\nBBB\n";
        assert_eq!(
            insert_exactly(original, "ANCHOR\n", "NEW\n", Before),
            "AAA\nNEW\nANCHOR\nBBB\n"
        );
        assert_eq!(
            insert_exactly(original, "ANCHOR\n", "NEW\n", After),
            "AAA\nANCHOR\nNEW\nBBB\n"
        );
        // An anchor without its newline sets the boundary inside the line.
        assert_eq!(
            insert_exactly(original, "ANCHOR", "\nNEW", After),
            "AAA\nANCHOR\nNEW\nBBB\n"
        );
        assert_eq!(
            insert_exactly(original, "ANCHOR", "NEW\n", Before),
            "AAA\nNEW\nANCHOR\nBBB\n"
        );
    }

    #[test]
    fn insertion_refuses_missing_ambiguous_and_empty_requests() {
        let original = "one\ntwo\none\n";
        for (anchor, content, kind, message) in [
            (
                "three",
                "x",
                std::io::ErrorKind::NotFound,
                "anchor not found in file",
            ),
            (
                "one",
                "x",
                std::io::ErrorKind::InvalidInput,
                "anchor occurs more than once in file; it must occur exactly once",
            ),
            (
                "",
                "x",
                std::io::ErrorKind::InvalidInput,
                "anchor must not be empty",
            ),
            (
                "two",
                "",
                std::io::ErrorKind::InvalidInput,
                "content must not be empty",
            ),
            // Exact bytes only: no case folding, trimming or line-ending slack.
            (
                "TWO",
                "x",
                std::io::ErrorKind::NotFound,
                "anchor not found in file",
            ),
            (
                " two",
                "x",
                std::io::ErrorKind::NotFound,
                "anchor not found in file",
            ),
            (
                "two\r\n",
                "x",
                std::io::ErrorKind::NotFound,
                "anchor not found in file",
            ),
        ] {
            for position in [Before, After] {
                let error = insert_at_anchor(original, anchor, content, position)
                    .expect_err("must be refused");
                assert_eq!(error.kind(), kind, "{anchor:?}");
                assert_eq!(error.to_string(), message, "{anchor:?}");
            }
        }
    }

    #[test]
    fn overlapping_occurrences_make_an_anchor_ambiguous() {
        for (original, anchor) in [
            ("aaa", "aa"),
            ("abcabcabc", "abcabc"),
            ("ababab", "abab"),
            ("ééé", "éé"),
            ("☃☃☃\n", "☃☃"),
        ] {
            // Not overlapping-aware, `matches` sees only one occurrence.
            assert_eq!(original.matches(anchor).count(), 1, "{original:?}");
            for position in [Before, After] {
                let error = insert_at_anchor(original, anchor, "N", position)
                    .expect_err("an overlapping second occurrence is ambiguous");
                assert!(error.to_string().contains("more than once"), "{error}");
            }
        }
        // NEGATIVE CONTROL: adjacent but distinct text stays unique.
        insert_exactly("abaXab", "aba", "N", After);
        insert_exactly("éaé", "éa", "N", Before);
    }

    #[test]
    fn inserted_content_is_byte_exact() {
        let original = "first\nANCHOR\nlast\n";
        for content in [
            "    leading spaces\n",
            "trailing spaces   \n",
            "\t\ttabs\tinside\n",
            "\n\n\nblank lines\n\n\n",
            "no trailing newline",
            "  \t mixed \t  ",
            "\r\ncarriage returns stay\r\n",
            "naïve café ☃ 日本語 🎉\n",
        ] {
            for position in [Before, After] {
                let updated = insert_exactly(original, "ANCHOR\n", content, position);
                assert_eq!(updated.matches(content).count(), 1, "{content:?}");
                assert_eq!(updated.len(), original.len() + content.len());
            }
        }
    }

    #[test]
    fn insertion_respects_utf8_and_newline_boundaries() {
        // Multibyte anchors, at the very start and the very end.
        let original = "ünïcödé start\nmiddle ☃\nend 🎉";
        assert_eq!(
            insert_exactly(original, "ünïcödé", "→ ", Before),
            "→ ünïcödé start\nmiddle ☃\nend 🎉"
        );
        assert_eq!(
            insert_exactly(original, "☃", " ❄", After),
            "ünïcödé start\nmiddle ☃ ❄\nend 🎉"
        );
        assert_eq!(
            insert_exactly(original, "🎉", " ✓", After),
            "ünïcödé start\nmiddle ☃\nend 🎉 ✓"
        );
        // A file without a final newline gains none; neither does content.
        let unterminated = "a\nb";
        assert_eq!(insert_exactly(unterminated, "b", "\nc", After), "a\nb\nc");
        assert_eq!(insert_exactly(unterminated, "b", "c", After), "a\nbc");
        assert_eq!(insert_exactly(unterminated, "a\n", "z", Before), "za\nb");
        // An anchor that includes its newline versus one that stops short.
        let terminated = "x\ny\n";
        assert_eq!(insert_exactly(terminated, "y\n", "z\n", After), "x\ny\nz\n");
        assert_eq!(insert_exactly(terminated, "y", "z", After), "x\nyz\n");
        // CRLF stays CRLF; LF content is not converted.
        assert_eq!(
            insert_exactly("a\r\nb\r\n", "b\r\n", "c\n", Before),
            "a\r\nc\nb\r\n"
        );
    }

    #[test]
    fn insertion_refuses_results_over_the_write_limit() {
        let original = format!("ANCHOR{}", "x".repeat(MAX_WRITE_SIZE - 8));
        // Exactly at the limit is allowed...
        let at_limit = insert_at_anchor(&original, "ANCHOR", "12", Before).expect("at limit");
        assert_eq!(at_limit.len(), MAX_WRITE_SIZE);
        // ...one byte more is refused before anything is built.
        let error = insert_at_anchor(&original, "ANCHOR", "123", After)
            .expect_err("over the limit must be refused");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            format!(
                "file would be too large after insertion ({} bytes, max {MAX_WRITE_SIZE} bytes)",
                MAX_WRITE_SIZE + 1
            )
        );
    }

    #[test]
    fn insertion_writes_the_file_and_reports_an_additions_only_diff() {
        let (before_result, before_after) = insert_in_file(
            "insert-before",
            "AAA\nANCHOR\nBBB\n",
            "ANCHOR\n",
            "NEW\n",
            Before,
        );
        let output = before_result.expect("insert before");
        assert_eq!(before_after, "AAA\nNEW\nANCHOR\nBBB\n");
        let path = &output.file_path;
        assert_eq!(
            output.operation_diff,
            format!("--- {path}\n+++ {path}\n@@ -1,3 +1,4 @@\n AAA\n+NEW\n ANCHOR\n BBB\n")
        );
        assert!(output.success);

        let (after_result, after_after) = insert_in_file(
            "insert-after",
            "AAA\nANCHOR\nBBB\n",
            "ANCHOR\n",
            "NEW\n",
            After,
        );
        let output = after_result.expect("insert after");
        assert_eq!(after_after, "AAA\nANCHOR\nNEW\nBBB\n");
        let path = &output.file_path;
        assert_eq!(
            output.operation_diff,
            format!("--- {path}\n+++ {path}\n@@ -1,3 +1,4 @@\n AAA\n ANCHOR\n+NEW\n BBB\n")
        );

        // The serialized result is minimal: path, success, then the diff.
        let serialized = serde_json::to_string_pretty(&output).expect("serializes");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("json");
        assert_eq!(
            value.as_object().map(serde_json::Map::len),
            Some(3),
            "{serialized}"
        );
        assert_eq!(value["success"], true);
        let at = |key: &str| serialized.find(&format!("\"{key}\":")).expect(key);
        assert!(at("filePath") < at("success") && at("success") < at("operationDiff"));
    }

    #[test]
    fn insertion_diffs_are_truthful_and_bounded() {
        use std::fmt::Write as _;

        let original = (0..400).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "line {n}");
            text
        });
        let cases = [
            ("line 0\n", "top\n", Before),
            ("line 399\n", "bottom\n", After),
            ("line 200\n", "a\nb\n\nc\n", After),
            ("line 100\n", "line 100\nline 100\n", Before),
            ("line 7\n", "(", Before),
        ];
        for (anchor, content, position) in cases {
            let updated = insert_exactly(&original, anchor, content, position);
            let diff = make_operation_diff("f", &original, &updated);
            assert_eq!(
                apply_operation_diff(&original, &diff),
                updated,
                "{anchor:?}"
            );
            let removed = diff
                .lines()
                .skip(2)
                .filter(|line| line.starts_with('-'))
                .count();
            if content.ends_with('\n') && anchor.ends_with('\n') {
                // Line-aligned insertions show additions only.
                assert_eq!(removed, 0, "{diff}");
                assert_eq!(
                    diff.lines().filter(|line| line.starts_with('+')).count(),
                    1 + content.lines().count(),
                    "{diff}"
                );
            } else {
                // Inside a line, that one line truthfully changes.
                assert_eq!(removed, 1, "{diff}");
            }
        }

        // A large insertion keeps the reviewed budget and explicit marker.
        let block = "added\n".repeat(5_000);
        let updated = insert_exactly(&original, "line 10\n", &block, After);
        let diff = make_operation_diff("f", &original, &updated);
        assert!(diff.len() <= OPERATION_DIFF_LIMIT, "{} bytes", diff.len());
        assert_eq!(diff, expected_bounded(&diff_unbounded(&original, &updated)));
        assert!(diff.contains("[operation diff truncated — exceeded 16384 bytes; "));
    }

    /// The complete diff `make_operation_diff` would render without a budget,
    /// for a single contiguous line-aligned insertion.
    fn diff_unbounded(original: &str, updated: &str) -> String {
        let old: Vec<&str> = original.split_inclusive('\n').collect();
        let new: Vec<&str> = updated.split_inclusive('\n').collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let added = new.len() - old.len();
        let (start, end) = (prefix.saturating_sub(3), (prefix + 3).min(old.len()));
        let mut diff = format!(
            "--- f\n+++ f\n@@ -{},{} +{},{} @@\n",
            start + 1,
            end - start,
            start + 1,
            end - start + added
        );
        for line in &old[start..prefix] {
            diff.push(' ');
            diff.push_str(line);
        }
        for line in &new[prefix..prefix + added] {
            diff.push('+');
            diff.push_str(line);
        }
        for line in &old[prefix..end] {
            diff.push(' ');
            diff.push_str(line);
        }
        diff
    }

    #[test]
    fn refused_insertions_leave_the_file_untouched() {
        let before = "one\ntwo\none\n";
        for (anchor, content) in [("three", "x"), ("one", "x"), ("", "x"), ("two", "")] {
            for position in [Before, After] {
                let (result, after) =
                    insert_in_file("insert-refused", before, anchor, content, position);
                assert!(result.is_err(), "{anchor:?}");
                assert_eq!(after, before, "{anchor:?}");
            }
        }
        let missing = temp_path("insert-missing");
        let error = insert_text(missing.to_str().expect("utf8"), "a", "b", After)
            .expect_err("insertion never creates a file");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(!missing.exists());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod confined_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustix::fs::OFlags;

    use super::{GrepSearchInput, InsertPosition, WorkspaceRoot, MAX_WRITE_SIZE};

    struct Fixture {
        base: PathBuf,
        workspace: PathBuf,
        outside: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos();
            let base = std::env::temp_dir().join(format!("claw-confined-{name}-{unique}"));
            let workspace = base.join("workspace");
            let outside = base.join("outside");
            fs::create_dir_all(&workspace).expect("workspace");
            fs::create_dir_all(&outside).expect("outside");
            fs::write(
                workspace.join("calculator.py"),
                "def add(a, b):\n    return a - b\n",
            )
            .expect("calculator");
            fs::write(workspace.join("test_calculator.py"), "ORACLE\n").expect("oracle");
            fs::write(outside.join("sentinel.txt"), "OUTSIDE\n").expect("sentinel");
            Self {
                base,
                workspace,
                outside,
            }
        }

        fn root(&self) -> WorkspaceRoot {
            WorkspaceRoot::bind(&self.workspace, &["calculator.py"]).expect("bind")
        }

        fn sentinel(&self) -> String {
            fs::read_to_string(self.outside.join("sentinel.txt")).expect("sentinel")
        }

        fn oracle(&self) -> String {
            fs::read_to_string(self.workspace.join("test_calculator.py")).expect("oracle")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn grep(pattern: &str, path: Option<&str>) -> GrepSearchInput {
        GrepSearchInput {
            pattern: pattern.to_string(),
            path: path.map(str::to_string),
            glob: None,
            output_mode: Some("content".to_string()),
            before: None,
            after: None,
            context_short: None,
            context: None,
            line_numbers: None,
            case_insensitive: None,
            file_type: None,
            head_limit: None,
            offset: None,
            multiline: None,
        }
    }

    #[test]
    fn kernel_resolution_refuses_parent_escape_without_the_lexical_check() {
        // Bypasses `workspace_relative` on purpose: RESOLVE_BENEATH alone must
        // stop `..` even if the lexical layer were missing.
        let fixture = Fixture::new("beneath");
        let root = fixture.root();
        let error = root
            .open_beneath(
                Path::new("../outside/sentinel.txt"),
                OFlags::RDONLY,
                "../outside/sentinel.txt",
            )
            .expect_err("openat2 must refuse to resolve above the root");
        assert!(
            error.to_string().contains("outside the workspace"),
            "{error}"
        );
    }

    #[test]
    fn relative_paths_resolve_beneath_the_bound_root_only() {
        let fixture = Fixture::new("relative");
        let root = fixture.root();
        let read = root.read_file("calculator.py", None, None).expect("read");
        assert_eq!(read.file.file_path, "calculator.py");
        assert!(read.file.content.contains("return a - b"));
        for path in [
            "../outside/sentinel.txt",
            "new-dir/../../outside/new.txt",
            "./../outside/sentinel.txt",
        ] {
            let error = root.read_file(path, None, None).expect_err(path);
            assert!(error.to_string().contains("`..`"), "{error}");
            assert!(root.write_file(path, "X").is_err(), "{path}");
        }
        let absolute = fixture.workspace.join("calculator.py");
        let error = root
            .read_file(absolute.to_str().expect("utf8"), None, None)
            .expect_err("absolute paths are refused, even inside the workspace");
        assert!(error.to_string().contains("absolute"), "{error}");
        assert!(!fixture.workspace.join("new-dir").exists());
        assert!(!fixture.outside.join("new.txt").exists());
        assert_eq!(fixture.sentinel(), "OUTSIDE\n");
    }

    #[test]
    fn symlinks_are_refused_even_when_they_point_inside() {
        let fixture = Fixture::new("symlink");
        std::os::unix::fs::symlink(
            fixture.outside.join("sentinel.txt"),
            fixture.workspace.join("escape.txt"),
        )
        .expect("symlink");
        std::os::unix::fs::symlink(&fixture.outside, fixture.workspace.join("door"))
            .expect("symlink");
        let root = fixture.root();
        // Replace the designated file with an inside-pointing symlink after binding.
        fs::remove_file(fixture.workspace.join("calculator.py")).expect("remove");
        std::os::unix::fs::symlink(
            "test_calculator.py",
            fixture.workspace.join("calculator.py"),
        )
        .expect("symlink");
        for path in ["escape.txt", "door/sentinel.txt", "calculator.py"] {
            let error = root.read_file(path, None, None).expect_err(path);
            assert!(error.to_string().contains("symlink"), "{path}: {error}");
        }
        assert!(root.write_file("calculator.py", "PWNED").is_err());
        assert!(root
            .edit_file("calculator.py", "ORACLE", "PWNED", false)
            .is_err());
        assert_eq!(fixture.oracle(), "ORACLE\n");
        assert_eq!(fixture.sentinel(), "OUTSIDE\n");
    }

    #[test]
    fn only_designated_existing_regular_files_are_writable() {
        let fixture = Fixture::new("designated");
        let root = fixture.root();
        let error = root
            .write_file("test_calculator.py", "PWNED")
            .expect_err("the oracle is not designated");
        assert!(error.to_string().contains("designated"), "{error}");
        assert!(root
            .edit_file("test_calculator.py", "ORACLE", "PWNED", false)
            .is_err());
        assert!(root.write_file("new.py", "x").is_err());
        assert!(!fixture.workspace.join("new.py").exists());
        assert_eq!(fixture.oracle(), "ORACLE\n");

        let edit = root
            .edit_file("calculator.py", "return a - b", "return a + b", false)
            .expect("designated edit");
        assert_eq!(edit.file_path, "calculator.py");
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("calculator.py")).expect("calculator"),
            "def add(a, b):\n    return a + b\n"
        );
    }

    #[test]
    fn hardlinked_designated_files_are_refused() {
        let fixture = Fixture::new("hardlink");
        let root = fixture.root();
        root.verify_single_link_files(&["calculator.py", "test_calculator.py"])
            .expect("clean fixture");
        fs::hard_link(
            fixture.workspace.join("calculator.py"),
            fixture.outside.join("linked.py"),
        )
        .expect("hardlink");
        let error = root
            .verify_single_link_files(&["calculator.py"])
            .expect_err("hardlinked fixture must be rejected");
        assert!(error.to_string().contains("hard links"), "{error}");
        assert!(root.write_file("calculator.py", "PWNED").is_err());
        assert_eq!(
            fs::read_to_string(fixture.outside.join("linked.py")).expect("linked"),
            "def add(a, b):\n    return a - b\n"
        );
    }

    #[test]
    fn special_files_are_refused_without_blocking() {
        let fixture = Fixture::new("fifo");
        let fifo = fixture.workspace.join("pipe");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .expect("fifo");
        let root = fixture.root();
        let error = root.read_file("pipe", None, None).expect_err("fifo");
        assert!(error.to_string().contains("not a regular file"), "{error}");
    }

    #[test]
    fn glob_and_grep_enumerate_beneath_the_root_only() {
        let fixture = Fixture::new("search");
        std::os::unix::fs::symlink(&fixture.outside, fixture.workspace.join("dir-link"))
            .expect("symlink");
        fs::create_dir(fixture.workspace.join("sub")).expect("sub");
        fs::write(fixture.workspace.join("sub/inner.py"), "OUTSIDE-looking\n").expect("inner");
        let root = fixture.root();

        let all = root.glob_search("**/*", None).expect("glob");
        let mut names = all.filenames.clone();
        names.sort();
        assert_eq!(
            names,
            ["calculator.py", "sub/inner.py", "test_calculator.py"]
        );
        assert_eq!(
            root.glob_search("dir-link/*", None)
                .expect("glob")
                .num_files,
            0
        );
        for pattern in [
            "../outside/*",
            "/etc/*",
            "{../outside/*,*.py}",
            "{..,sub}/*",
        ] {
            assert!(root.glob_search(pattern, None).is_err(), "{pattern}");
        }
        assert!(root.glob_search("*", Some("../outside")).is_err());
        assert!(root.glob_search("*", Some("dir-link")).is_err());

        let found = root.grep_search(&grep("OUTSIDE", None)).expect("grep");
        assert_eq!(found.filenames, ["sub/inner.py"]);
        assert!(root
            .grep_search(&grep("OUTSIDE", Some("../outside")))
            .is_err());
        assert!(root
            .grep_search(&grep("OUTSIDE", Some("dir-link")))
            .is_err());
    }

    #[test]
    fn confined_edit_result_carries_the_operation_diff() {
        let fixture = Fixture::new("operation-diff");
        let edit = fixture
            .root()
            .edit_file("calculator.py", "return a - b", "return a + b", false)
            .expect("designated edit");
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("calculator.py")).expect("calculator"),
            "def add(a, b):\n    return a + b\n"
        );
        let serialized = serde_json::to_value(&edit).expect("edit output serializes");
        assert_eq!(
            serialized["operationDiff"],
            "--- calculator.py\n+++ calculator.py\n@@ -1,2 +1,2 @@\n def add(a, b):\n-    return a - b\n+    return a + b\n"
        );
    }

    /// Replaces the whole of a designated file `f` through the confined edit,
    /// as the independent review did, and returns the serialized diff.
    fn confined_whole_file_diff(fixture: &Fixture, before: &str, after: &str) -> String {
        fs::write(fixture.workspace.join("f"), before).expect("seed f");
        let edit = WorkspaceRoot::bind(&fixture.workspace, &["f"])
            .expect("bind")
            .edit_file("f", before, after, false)
            .expect("designated edit");
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("f")).expect("f"),
            after
        );
        serde_json::to_value(&edit).expect("edit output serializes")["operationDiff"]
            .as_str()
            .expect("operationDiff")
            .to_owned()
    }

    #[test]
    fn confined_edit_diff_that_fits_the_budget_is_never_marked_truncated() {
        let fixture = Fixture::new("diff-under-budget");
        let before = format!("{}\n", "a".repeat(8_120));
        let after = format!("{}\n", "b".repeat(8_120));
        let complete = format!("--- f\n+++ f\n@@ -1,1 +1,1 @@\n-{before}+{after}");
        assert_eq!(complete.len(), 16_272, "under the 16 384-byte budget");
        let diff = confined_whole_file_diff(&fixture, &before, &after);
        assert_eq!(diff, complete);
        assert!(!diff.contains("[operation diff truncated"));
        assert!(
            diff.ends_with(&format!("\n+{after}")),
            "the addition is kept"
        );
    }

    #[test]
    fn confined_edit_diff_marker_counts_no_newline_annotations() {
        let fixture = Fixture::new("diff-eof-count");
        let (before, after) = ("a".repeat(17_000), "b".repeat(17_000));
        let complete = format!(
            "--- f\n+++ f\n@@ -1,1 +1,1 @@\n-{before}\n\\ No newline at end of file\n+{after}\n\\ No newline at end of file\n"
        );
        assert_eq!((complete.len(), complete.lines().count()), (34_088, 7));
        let shown = "--- f\n+++ f\n@@ -1,1 +1,1 @@\n";
        // Removal, its annotation, addition, its annotation.
        assert_eq!(complete.lines().count() - shown.lines().count(), 4);
        assert_eq!(
            confined_whole_file_diff(&fixture, &before, &after),
            format!(
                "{shown}[operation diff truncated — exceeded 16384 bytes; 4 more diff lines omitted]\n"
            )
        );
    }

    // --- anchored insertion ------------------------------------------------

    const CALCULATOR: &str = "def add(a, b):\n    return a - b\n";

    fn calculator(fixture: &Fixture) -> String {
        fs::read_to_string(fixture.workspace.join("calculator.py")).expect("calculator")
    }

    #[test]
    fn confined_insertion_preserves_the_designated_file_and_reports_the_diff() {
        let fixture = Fixture::new("insert");
        let root = fixture.root();
        let before = root
            .insert_text(
                "calculator.py",
                "def add",
                "import math\n\n\n",
                InsertPosition::Before,
            )
            .expect("designated insertion");
        assert_eq!(
            calculator(&fixture),
            "import math\n\n\ndef add(a, b):\n    return a - b\n"
        );
        assert_eq!(before.file_path, "calculator.py");
        assert!(before.success);
        assert_eq!(
            before.operation_diff,
            "--- calculator.py\n+++ calculator.py\n@@ -1,2 +1,5 @@\n+import math\n+\n+\n def add(a, b):\n     return a - b\n"
        );
        let after = root
            .insert_text(
                "calculator.py",
                "    return a - b\n",
                "\n\ndef sub(a, b):\n    return a - b\n",
                InsertPosition::After,
            )
            .expect("designated insertion");
        assert_eq!(
            calculator(&fixture),
            "import math\n\n\ndef add(a, b):\n    return a - b\n\n\ndef sub(a, b):\n    return a - b\n"
        );
        assert!(!after
            .operation_diff
            .lines()
            .skip(2)
            .any(|line| line.starts_with('-')));
        assert_eq!(fixture.oracle(), "ORACLE\n");
    }

    #[test]
    fn confined_insertion_refuses_undeclared_escaping_and_aliased_targets() {
        let fixture = Fixture::new("insert-authority");
        std::os::unix::fs::symlink(
            fixture.outside.join("sentinel.txt"),
            fixture.workspace.join("escape.txt"),
        )
        .expect("symlink");
        std::os::unix::fs::symlink(&fixture.outside, fixture.workspace.join("door"))
            .expect("symlink");
        let absolute = fixture.outside.join("sentinel.txt");
        let absolute = absolute.to_str().expect("utf8");
        let inside = fixture.workspace.join("calculator.py");
        let inside = inside.to_str().expect("utf8");
        // Every path names text the anchor matches, so only authority refuses.
        let root = WorkspaceRoot::bind(
            &fixture.workspace,
            &[
                "calculator.py",
                "escape.txt",
                "door/sentinel.txt",
                "missing.py",
            ],
        )
        .expect("bind");
        for (path, anchor, reason) in [
            (
                "test_calculator.py",
                "ORACLE",
                "not a designated writable file",
            ),
            ("../outside/sentinel.txt", "OUTSIDE", "`..`"),
            ("new-dir/../../outside/sentinel.txt", "OUTSIDE", "`..`"),
            (absolute, "OUTSIDE", "is absolute"),
            (inside, "def add", "is absolute"),
            ("escape.txt", "OUTSIDE", "symlink"),
            ("door/sentinel.txt", "OUTSIDE", "symlink"),
            ("missing.py", "x", "No such file"),
        ] {
            for position in [InsertPosition::Before, InsertPosition::After] {
                let error = root
                    .insert_text(path, anchor, "PWNED", position)
                    .expect_err(path);
                assert!(error.to_string().contains(reason), "{path}: {error}");
            }
        }
        assert_eq!(fixture.oracle(), "ORACLE\n");
        assert_eq!(fixture.sentinel(), "OUTSIDE\n");
        assert_eq!(calculator(&fixture), CALCULATOR);
        assert!(!fixture.workspace.join("missing.py").exists());
        assert!(!fixture.workspace.join("new-dir").exists());
    }

    #[test]
    fn confined_insertion_refuses_designated_files_swapped_after_binding() {
        let fixture = Fixture::new("insert-swapped");
        let root = fixture.root();
        // A symlink planted in place of the designated file.
        fs::remove_file(fixture.workspace.join("calculator.py")).expect("remove");
        std::os::unix::fs::symlink(
            "test_calculator.py",
            fixture.workspace.join("calculator.py"),
        )
        .expect("symlink");
        let error = root
            .insert_text("calculator.py", "ORACLE", "PWNED", InsertPosition::Before)
            .expect_err("symlinked designated file");
        assert!(error.to_string().contains("symlink"), "{error}");
        assert_eq!(fixture.oracle(), "ORACLE\n");

        // A second link to the designated inode, reachable from outside.
        fs::remove_file(fixture.workspace.join("calculator.py")).expect("remove");
        fs::write(fixture.workspace.join("calculator.py"), CALCULATOR).expect("restore");
        fs::hard_link(
            fixture.workspace.join("calculator.py"),
            fixture.outside.join("linked.py"),
        )
        .expect("hardlink");
        let error = root
            .insert_text("calculator.py", "def add", "PWNED", InsertPosition::After)
            .expect_err("hardlinked designated file");
        assert!(error.to_string().contains("hard links"), "{error}");
        assert_eq!(
            fs::read_to_string(fixture.outside.join("linked.py")).expect("linked"),
            CALCULATOR
        );

        // A FIFO in place of the designated file is refused without blocking.
        fs::remove_file(fixture.workspace.join("calculator.py")).expect("remove");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            fixture.workspace.join("calculator.py"),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .expect("fifo");
        let error = root
            .insert_text("calculator.py", "def add", "PWNED", InsertPosition::Before)
            .expect_err("fifo");
        assert!(error.to_string().contains("not a regular file"), "{error}");
    }

    #[test]
    fn confined_insertion_refusals_leave_the_designated_file_unchanged() {
        let fixture = Fixture::new("insert-refusals");
        fs::write(
            fixture.workspace.join("calculator.py"),
            "x = 1\nx = 1\ny = 2\n",
        )
        .expect("seed");
        let root = fixture.root();
        for (anchor, content, reason) in [
            ("z = 3", "w = 0\n", "anchor not found in file"),
            ("x = 1\n", "w = 0\n", "more than once"),
            ("", "w = 0\n", "anchor must not be empty"),
            ("y = 2\n", "", "content must not be empty"),
        ] {
            for position in [InsertPosition::Before, InsertPosition::After] {
                let error = root
                    .insert_text("calculator.py", anchor, content, position)
                    .expect_err(anchor);
                assert!(error.to_string().contains(reason), "{anchor:?}: {error}");
                assert_eq!(calculator(&fixture), "x = 1\nx = 1\ny = 2\n");
            }
        }

        // A result one byte over the write limit is refused before writing.
        let big = format!("ANCHOR{}", "x".repeat(MAX_WRITE_SIZE - 7));
        fs::write(fixture.workspace.join("calculator.py"), &big).expect("seed big");
        let error = root
            .insert_text("calculator.py", "ANCHOR", "12", InsertPosition::After)
            .expect_err("over the limit");
        assert!(error.to_string().contains("too large"), "{error}");
        assert!(calculator(&fixture) == big, "file must be unchanged");
    }
}
