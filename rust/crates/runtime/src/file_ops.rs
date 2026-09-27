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

    Ok(EditFileOutput {
        file_path: absolute_path.to_string_lossy().into_owned(),
        old_string: old_string.to_owned(),
        new_string: new_string.to_owned(),
        original_file: original_file.clone(),
        structured_patch: make_patch(&original_file, &updated),
        user_modified: false,
        replace_all,
        git_diff: None,
    })
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
        confinement_denied, expand_braces, fs, grep_files, io, make_patch, text_window,
        workspace_relative, EditFileOutput, GlobSearchOutput, GrepSearchInput, GrepSearchOutput,
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
            Ok(EditFileOutput {
                file_path: relative.to_string_lossy().into_owned(),
                old_string: old_string.to_owned(),
                new_string: new_string.to_owned(),
                original_file: original.clone(),
                structured_patch: make_patch(&original, &updated),
                user_modified: false,
                replace_all,
                git_diff: None,
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
        edit_file, expand_braces, glob_search, grep_search, is_symlink_escape, read_file,
        read_file_in_workspace, write_file, GrepSearchInput, MAX_WRITE_SIZE,
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
}

#[cfg(all(test, target_os = "linux"))]
mod confined_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustix::fs::OFlags;

    use super::{GrepSearchInput, WorkspaceRoot};

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
}
