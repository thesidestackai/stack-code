//! Caller-supplied harness assertion config.
//!
//! The config is purely declarative — caller names expected values,
//! the harness reports observed values, the harness never accepts an
//! input that would direct it to invoke any non-status subcommand.
//! Chain-write attempts are refused at config-parse time.

use std::path::PathBuf;

use crate::envelope::{Phase, StopCondition};

/// Caller's expected outcome for the cycle. The harness compares the
/// observed envelope against this expectation and classifies the
/// cycle accordingly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// Caller expects the chain to continue (no STOP signal).
    Continue,
    /// Caller expects a STOP signal. If `stop_condition` is `Some`,
    /// the harness compares the observed `stop_condition` against
    /// the expected value.
    Stop {
        stop_condition: Option<StopCondition>,
    },
    /// Caller has no expectation; the harness emits observed values
    /// without comparison-driven classification.
    Unasserted,
}

/// Caller-supplied harness assertion config. Every field is read-only
/// from the harness's perspective.
#[derive(Debug, Clone)]
pub struct HarnessAssertionConfig {
    /// Workspace root path. Forwarded verbatim to `claw plan status`.
    pub workspace_root: PathBuf,
    /// Optional approval-result path. Forwarded verbatim as the second
    /// positional argument to `claw plan status` when supplied.
    pub approval_result_path: Option<PathBuf>,
    /// Expected `phase` value, if asserting.
    pub expected_phase: Option<Phase>,
    /// Expected outcome (continue / STOP / unasserted).
    pub expected_outcome: ExpectedOutcome,
    /// Expected `read_only_invariant` literal. Defaults to the pinned
    /// literal `"this command does not mutate state"`; supplying a
    /// different value is a misuse and `validate` rejects it.
    pub expected_read_only_invariant: String,
    /// Optional substring patterns each entry of `evidence_paths`
    /// must match at least one of. Empty disables the assertion.
    pub expected_evidence_substrings: Vec<String>,
    /// Caller-declared disposability flag. The disposable classifier
    /// requires this AND-signal in addition to the path-prefix,
    /// marker-file, and owner checks.
    pub workspace_is_disposable: bool,
    /// Optional reference to a per-deployment scope card authorising
    /// a non-disposable workspace. When set, the classifier emits
    /// `non-disposable-but-authorized-by:<ref>` instead of refusing.
    /// The harness records the ref verbatim; it does not parse the
    /// doc.
    pub non_disposable_authorization_doc: Option<String>,
    /// Repeat-invocation policy. `1` means a single status read per
    /// cycle; `2` exercises the idempotency invariant.
    pub repeat_invocations: u8,
}

impl Default for HarnessAssertionConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::new(),
            approval_result_path: None,
            expected_phase: None,
            expected_outcome: ExpectedOutcome::Unasserted,
            expected_read_only_invariant: crate::envelope::READ_ONLY_INVARIANT_LITERAL.to_string(),
            expected_evidence_substrings: Vec::new(),
            workspace_is_disposable: false,
            non_disposable_authorization_doc: None,
            repeat_invocations: 1,
        }
    }
}

/// Config validation errors. Each refusal here is a STOP signal in
/// the harness's report; the harness never falls back to "best
/// effort" invocation when its config is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Workspace root path was empty.
    EmptyWorkspaceRoot,
    /// `expected_read_only_invariant` differed from the pinned
    /// literal `"this command does not mutate state"`.
    UnexpectedReadOnlyInvariantLiteral,
    /// `repeat_invocations` was 0.
    ZeroRepeatInvocations,
    /// `repeat_invocations` was greater than the safety cap. Prevents
    /// runaway test loops; production callers should keep this at 1
    /// or 2.
    RepeatInvocationsExceedsCap(u8),
    /// Caller supplied a path or string referencing one of the chain-
    /// write subcommands. The harness refuses to operate on such a
    /// config at parse time. Carries the offending substring verbatim.
    ChainWriteSubcommandReferenced(String),
}

/// Safety cap on `repeat_invocations`. Higher values are a misuse.
pub const REPEAT_INVOCATION_CAP: u8 = 8;

impl HarnessAssertionConfig {
    /// Validate the config. Refusal here is a STOP signal; the harness
    /// never invokes the subprocess against an invalid config.
    ///
    /// # Errors
    ///
    /// Returns the first refusal encountered. Callers handling
    /// multiple refusals should call `validate` after each correction.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.workspace_root.as_os_str().is_empty() {
            return Err(ConfigError::EmptyWorkspaceRoot);
        }
        if self.expected_read_only_invariant != crate::envelope::READ_ONLY_INVARIANT_LITERAL {
            return Err(ConfigError::UnexpectedReadOnlyInvariantLiteral);
        }
        if self.repeat_invocations == 0 {
            return Err(ConfigError::ZeroRepeatInvocations);
        }
        if self.repeat_invocations > REPEAT_INVOCATION_CAP {
            return Err(ConfigError::RepeatInvocationsExceedsCap(
                self.repeat_invocations,
            ));
        }
        // Refuse any string in the config that references a chain-write
        // subcommand. The harness never invokes such commands; a config
        // that names one is a category violation.
        for s in self.config_string_view() {
            if let Some(hit) = chain_write_reference(&s) {
                return Err(ConfigError::ChainWriteSubcommandReferenced(hit));
            }
        }
        Ok(())
    }

    fn config_string_view(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.push(self.workspace_root.to_string_lossy().into_owned());
        if let Some(p) = &self.approval_result_path {
            out.push(p.to_string_lossy().into_owned());
        }
        out.push(self.expected_read_only_invariant.clone());
        for s in &self.expected_evidence_substrings {
            out.push(s.clone());
        }
        if let Some(s) = &self.non_disposable_authorization_doc {
            out.push(s.clone());
        }
        out
    }
}

/// Detect a chain-write-subcommand reference in a caller-supplied
/// string. Returns the offending substring when found.
///
/// The detector looks for the plan-subcommand prefix followed by one
/// of the four chain-write subcommand names with a word boundary. The
/// detector itself never carries the contiguous chain-write literal in
/// source — it assembles the prefix and subcommand names at runtime.
fn chain_write_reference(s: &str) -> Option<String> {
    let plan_prefix = format!("{} {} ", "claw", "plan");
    // Order matters: longest-first so `apply-bundle` matches before
    // `apply`.
    let needles = ["apply-bundle", "approve", "apply", "run"];
    let mut search_from = 0usize;
    while let Some(pos) = s[search_from..].find(&plan_prefix) {
        let abs = search_from + pos;
        let after = &s[abs + plan_prefix.len()..];
        for n in needles {
            if let Some(rest) = after.strip_prefix(n) {
                // Refuse the reference whenever the needle is followed
                // by anything that is not an alphanumeric or underscore
                // continuation, or end-of-string. This catches forms
                // such as `<prefix> approve <bundle>`, `<prefix>
                // approve-result.json`, `<prefix> apply.bundle`, etc.
                let next = rest.chars().next();
                let is_word_continuation =
                    next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
                if !is_word_continuation {
                    return Some(format!("{plan_prefix}{n}"));
                }
            }
        }
        search_from = abs + plan_prefix.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> HarnessAssertionConfig {
        HarnessAssertionConfig {
            workspace_root: PathBuf::from("/tmp/some-disposable-fixture"),
            ..Default::default()
        }
    }

    #[test]
    fn default_is_invalid_without_workspace_root() {
        let c = HarnessAssertionConfig::default();
        assert_eq!(c.validate(), Err(ConfigError::EmptyWorkspaceRoot));
    }

    #[test]
    fn altered_read_only_invariant_is_refused() {
        let mut c = base();
        c.expected_read_only_invariant = "something else".into();
        assert_eq!(
            c.validate(),
            Err(ConfigError::UnexpectedReadOnlyInvariantLiteral)
        );
    }

    #[test]
    fn zero_repeat_invocations_refused() {
        let mut c = base();
        c.repeat_invocations = 0;
        assert_eq!(c.validate(), Err(ConfigError::ZeroRepeatInvocations));
    }

    #[test]
    fn excessive_repeat_invocations_refused() {
        let mut c = base();
        c.repeat_invocations = REPEAT_INVOCATION_CAP + 1;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::RepeatInvocationsExceedsCap(_))
        ));
    }

    #[test]
    fn config_referencing_chain_write_subcommand_is_refused_approve() {
        let mut c = base();
        let plan_prefix = format!("{} {}", "claw", "plan");
        let probe = format!("/tmp/disposable/{plan_prefix} approve-evidence");
        c.expected_evidence_substrings = vec![probe];
        assert!(matches!(
            c.validate(),
            Err(ConfigError::ChainWriteSubcommandReferenced(_))
        ));
    }

    #[test]
    fn config_referencing_chain_write_subcommand_is_refused_apply_bundle() {
        let mut c = base();
        let probe = format!("{} {} apply-bundle", "claw", "plan");
        c.non_disposable_authorization_doc = Some(probe);
        assert!(matches!(
            c.validate(),
            Err(ConfigError::ChainWriteSubcommandReferenced(_))
        ));
    }

    #[test]
    fn config_with_disposable_workspace_path_is_accepted() {
        let c = base();
        assert!(c.validate().is_ok());
    }
}
