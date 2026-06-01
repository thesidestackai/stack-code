//! Disposable-workspace classifier.
//!
//! AND-semantics over four signals — path-prefix allowlist, marker
//! file, workspace owner, explicit caller declaration. Refusal is
//! the default whenever any signal is missing; the only way to
//! operate against a non-disposable workspace is to supply a per-
//! deployment authorisation doc reference (recorded verbatim; never
//! parsed by the harness).

use std::fs;
use std::path::{Path, PathBuf};

/// Pinned relative path of the marker file the classifier reads under
/// the workspace root. The file's existence is one AND-signal — never
/// the whole classifier.
pub const DISPOSABLE_MARKER_REL_PATH: &str = ".claw/harness-disposable.marker";

/// Caller-supplied classifier configuration.
#[derive(Debug, Clone, Default)]
pub struct ClassifierConfig {
    /// Allowlist of disposable-workspace root prefixes. Empty
    /// disables the path-prefix signal. The default is empty; the
    /// caller MUST configure at least one prefix for a workspace to
    /// classify as disposable.
    pub disposable_path_prefixes: Vec<PathBuf>,
    /// Caller-supplied UID expected to own the workspace root. When
    /// `Some`, the classifier compares against
    /// `MetadataExt::uid()` from the workspace root's metadata. When
    /// `None`, the owner signal is treated as not-yet-checked and
    /// classifies as missing.
    pub expected_owner_uid: Option<u32>,
    /// Caller's explicit declaration that the workspace is disposable.
    pub caller_declared_disposable: bool,
    /// Optional per-deployment scope card reference that authorises a
    /// non-disposable workspace. The classifier records this verbatim
    /// in its decision; it never parses the doc.
    pub non_disposable_authorization_doc: Option<String>,
}

/// Per-signal report. The classifier emits the four signals and the
/// caller's report records each one's pass/fail individually for
/// audit. Four bools are by design (one per AND-signal); pedantic
/// "struct excessive bools" lint is silenced here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ClassifierSignals {
    pub path_prefix_allowed: bool,
    pub marker_file_present: bool,
    pub owner_matches: bool,
    pub caller_declared: bool,
}

impl ClassifierSignals {
    #[must_use]
    pub const fn all_pass(&self) -> bool {
        self.path_prefix_allowed
            && self.marker_file_present
            && self.owner_matches
            && self.caller_declared
    }
}

/// Decision produced by the classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceClassification {
    /// All four AND-signals passed.
    Disposable { signals: ClassifierSignals },
    /// Workspace was not disposable but the caller supplied a per-
    /// deployment authorisation doc reference. Reference recorded
    /// verbatim.
    NonDisposableButAuthorizedBy {
        signals: ClassifierSignals,
        authorization_doc: String,
    },
    /// Workspace was not disposable and no authorisation was supplied.
    /// The harness refuses to invoke the subprocess.
    NonDisposableAndRefused { signals: ClassifierSignals },
}

impl WorkspaceClassification {
    #[must_use]
    pub const fn is_refused(&self) -> bool {
        matches!(self, Self::NonDisposableAndRefused { .. })
    }

    #[must_use]
    pub const fn signals(&self) -> &ClassifierSignals {
        match self {
            Self::Disposable { signals }
            | Self::NonDisposableButAuthorizedBy { signals, .. }
            | Self::NonDisposableAndRefused { signals } => signals,
        }
    }
}

/// Classify the workspace. Reads-only operations: `Path::starts_with`,
/// `fs::metadata`, marker-file `fs::read`. No filesystem mutation.
#[must_use]
pub fn classify_workspace(
    workspace_root: &Path,
    cfg: &ClassifierConfig,
) -> WorkspaceClassification {
    let signals = collect_signals(workspace_root, cfg);
    if signals.all_pass() {
        return WorkspaceClassification::Disposable { signals };
    }
    if let Some(doc) = &cfg.non_disposable_authorization_doc {
        return WorkspaceClassification::NonDisposableButAuthorizedBy {
            signals,
            authorization_doc: doc.clone(),
        };
    }
    WorkspaceClassification::NonDisposableAndRefused { signals }
}

fn collect_signals(workspace_root: &Path, cfg: &ClassifierConfig) -> ClassifierSignals {
    ClassifierSignals {
        path_prefix_allowed: signal_path_prefix_allowed(workspace_root, cfg),
        marker_file_present: signal_marker_file_present(workspace_root),
        owner_matches: signal_owner_matches(workspace_root, cfg),
        caller_declared: cfg.caller_declared_disposable,
    }
}

fn signal_path_prefix_allowed(workspace_root: &Path, cfg: &ClassifierConfig) -> bool {
    if cfg.disposable_path_prefixes.is_empty() {
        return false;
    }
    let canon = match fs::canonicalize(workspace_root) {
        Ok(p) => p,
        Err(_) => workspace_root.to_path_buf(),
    };
    cfg.disposable_path_prefixes.iter().any(|prefix| {
        let canon_prefix = fs::canonicalize(prefix).unwrap_or_else(|_| prefix.clone());
        canon.starts_with(&canon_prefix)
    })
}

fn signal_marker_file_present(workspace_root: &Path) -> bool {
    let marker_path = workspace_root.join(DISPOSABLE_MARKER_REL_PATH);
    match fs::metadata(&marker_path) {
        Ok(meta) => meta.is_file(),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn signal_owner_matches(workspace_root: &Path, cfg: &ClassifierConfig) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some(expected) = cfg.expected_owner_uid else {
        return false;
    };
    match fs::metadata(workspace_root) {
        Ok(meta) => meta.uid() == expected,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn signal_owner_matches(_workspace_root: &Path, _cfg: &ClassifierConfig) -> bool {
    // Non-unix platforms do not expose a stable uid signal here. The
    // classifier conservatively treats the signal as missing, which
    // (with AND-semantics) means non-unix callers MUST supply the
    // authorisation doc reference path explicitly.
    false
}
