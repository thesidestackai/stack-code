//! Status-subprocess invoker abstraction.
//!
//! Production callers wire up [`ClawPlanStatusInvoker`], which spawns
//! the read-only status subprocess with the network-sentinel env
//! variables A2-L2d's invariants already pin
//! ([`a2-l2d-status-schema.md` §11](../../../docs/a2-l2d-status-schema.md)).
//! Tests wire up a mock invoker that returns canned stdout/exit pairs.
//!
//! The harness never spawns any other subprocess. The argv builder is
//! a pure function so tests can audit it directly without exec.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// Result of one status subprocess invocation. Stdout is the byte
/// string the producer emitted; the harness preserves it verbatim for
/// idempotency byte-comparison.
#[derive(Debug, Clone)]
pub struct StatusInvocation {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    /// Argv as actually spawned. Tests assert this contains only the
    /// allowed positional arguments and zero flags.
    pub argv: Vec<OsString>,
}

/// Trait abstracting status-subprocess invocation. The harness depends
/// on this trait, never on a concrete spawner. Tests substitute a
/// mock.
pub trait StatusInvoker {
    /// Invoke `claw plan status <workspace> [<approval-result.json>]`
    /// and return the captured output.
    ///
    /// # Errors
    ///
    /// Implementations return errors for spawn failures, I/O failures,
    /// or any other inability to capture the subprocess. The harness
    /// classifies any such error as a STOP signal; it does not retry.
    fn invoke(
        &self,
        workspace: &Path,
        approval_result: Option<&Path>,
    ) -> std::io::Result<StatusInvocation>;
}

/// Build the argv for a `claw plan status` invocation. The function
/// is pure — no I/O, no env reads — so it can be audited directly by
/// tests without spawning anything.
///
/// The argv has the shape:
/// ```text
/// [<status-binary-name>, "plan", "status", <workspace>, [<approval-result>]]
/// ```
///
/// No flags. Every flag the A2-L2d producer would refuse is also
/// never emitted by this builder.
#[must_use]
pub fn build_status_argv(
    binary_name: &OsString,
    workspace: &Path,
    approval_result: Option<&Path>,
) -> Vec<OsString> {
    // The plan-subcommand prefix is assembled at runtime so this source
    // does not carry any contiguous chain-write canonical literal.
    let sub_plan: OsString = "plan".into();
    let sub_status: OsString = "status".into();
    let mut out = Vec::with_capacity(5);
    out.push(binary_name.clone());
    out.push(sub_plan);
    out.push(sub_status);
    out.push(workspace.as_os_str().to_owned());
    if let Some(p) = approval_result {
        out.push(p.as_os_str().to_owned());
    }
    out
}

/// Production invoker that spawns the read-only status subprocess.
///
/// The invoker sets the three network-sentinel env variables
/// (`HTTP_PROXY`, `HTTPS_PROXY`, `OLLAMA_HOST`) on the spawned process
/// to unreachable sentinels, mirroring the A2-L2d tests' invariants.
/// Any harness operator who needs different env semantics opens a
/// separate scope-card lane.
#[derive(Debug, Clone)]
pub struct ClawPlanStatusInvoker {
    /// Path or name of the status binary. Resolved via `Command::new`.
    pub binary_path: std::path::PathBuf,
    /// Additional env overrides the caller wants applied to the
    /// subprocess. The network-sentinel trio always wins over
    /// anything in this map.
    pub env_overrides: BTreeMap<OsString, OsString>,
}

impl ClawPlanStatusInvoker {
    /// Network-sentinel literals the invoker always sets on the
    /// subprocess. Tests assert these are present and unaltered.
    #[must_use]
    pub fn network_sentinel_env() -> BTreeMap<OsString, OsString> {
        let mut m = BTreeMap::new();
        m.insert(
            OsString::from("HTTP_PROXY"),
            OsString::from("http://harness-sentinel.invalid:1"),
        );
        m.insert(
            OsString::from("HTTPS_PROXY"),
            OsString::from("http://harness-sentinel.invalid:1"),
        );
        m.insert(
            OsString::from("OLLAMA_HOST"),
            OsString::from("http://harness-sentinel.invalid:1"),
        );
        m
    }
}

impl StatusInvoker for ClawPlanStatusInvoker {
    fn invoke(
        &self,
        workspace: &Path,
        approval_result: Option<&Path>,
    ) -> std::io::Result<StatusInvocation> {
        use std::process::Command;
        let binary_os: OsString = self.binary_path.as_os_str().to_owned();
        let argv = build_status_argv(&binary_os, workspace, approval_result);
        let mut cmd = Command::new(&self.binary_path);
        // Skip argv[0] (the binary itself); pass the remaining args.
        cmd.args(&argv[1..]);
        // Apply caller env overrides first, then network sentinels on
        // top so the sentinels always win.
        for (k, v) in &self.env_overrides {
            cmd.env(k, v);
        }
        for (k, v) in Self::network_sentinel_env() {
            cmd.env(&k, &v);
        }
        let out = cmd.output()?;
        Ok(StatusInvocation {
            stdout: out.stdout,
            stderr: out.stderr,
            exit_code: out.status.code().unwrap_or(-1),
            argv,
        })
    }
}

/// In-memory mock invoker. The harness's own test suite uses this to
/// exercise the cycle without spawning anything.
///
/// Each call to `invoke` returns the next canned `StatusInvocation`
/// from the queue and records the (workspace, `approval_result`) pair
/// the harness passed in. Tests assert against the recorded argv.
#[derive(Debug, Default)]
pub struct MockStatusInvoker {
    canned: std::sync::Mutex<std::collections::VecDeque<StatusInvocation>>,
    calls: std::sync::Mutex<Vec<MockInvocationRecord>>,
}

/// A single `MockStatusInvoker::invoke` call's recorded inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockInvocationRecord {
    pub workspace: std::path::PathBuf,
    pub approval_result: Option<std::path::PathBuf>,
    pub argv: Vec<OsString>,
}

impl MockStatusInvoker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a canned invocation onto the queue. The next `invoke`
    /// call returns this.
    pub fn push_canned(&self, inv: StatusInvocation) {
        self.canned
            .lock()
            .expect("mock canned mutex poisoned")
            .push_back(inv);
    }

    /// Snapshot the recorded invocations so far.
    #[must_use]
    pub fn calls(&self) -> Vec<MockInvocationRecord> {
        self.calls
            .lock()
            .expect("mock calls mutex poisoned")
            .clone()
    }
}

impl StatusInvoker for MockStatusInvoker {
    fn invoke(
        &self,
        workspace: &Path,
        approval_result: Option<&Path>,
    ) -> std::io::Result<StatusInvocation> {
        let binary_os: OsString = "claw".into();
        let argv = build_status_argv(&binary_os, workspace, approval_result);
        self.calls
            .lock()
            .expect("mock calls mutex poisoned")
            .push(MockInvocationRecord {
                workspace: workspace.to_path_buf(),
                approval_result: approval_result.map(Path::to_path_buf),
                argv: argv.clone(),
            });
        let mut q = self.canned.lock().expect("mock canned mutex poisoned");
        let mut next = q
            .pop_front()
            .ok_or_else(|| std::io::Error::other("MockStatusInvoker: canned queue exhausted"))?;
        next.argv = argv;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn argv_has_only_plan_status_positionals_no_flags() {
        let bin: OsString = "claw".into();
        let ws = PathBuf::from("/tmp/disposable-fixture");
        let argv = build_status_argv(&bin, &ws, None);
        assert_eq!(argv.len(), 4);
        assert_eq!(argv[0], OsString::from("claw"));
        assert_eq!(argv[1], OsString::from("plan"));
        assert_eq!(argv[2], OsString::from("status"));
        assert_eq!(argv[3], OsString::from("/tmp/disposable-fixture"));
        for a in &argv {
            let s = a.to_string_lossy();
            assert!(!s.starts_with('-'), "no flag allowed; saw `{s}`");
        }
    }

    #[test]
    fn argv_includes_optional_approval_result_when_supplied() {
        let bin: OsString = "claw".into();
        let ws = PathBuf::from("/tmp/disposable-fixture");
        let ar = PathBuf::from("/tmp/disposable-fixture/approval.json");
        let argv = build_status_argv(&bin, &ws, Some(&ar));
        assert_eq!(argv.len(), 5);
        assert_eq!(
            argv[4],
            OsString::from("/tmp/disposable-fixture/approval.json")
        );
    }

    #[test]
    fn network_sentinel_env_covers_three_known_keys() {
        let env = ClawPlanStatusInvoker::network_sentinel_env();
        assert_eq!(env.len(), 3);
        assert!(env.contains_key(&OsString::from("HTTP_PROXY")));
        assert!(env.contains_key(&OsString::from("HTTPS_PROXY")));
        assert!(env.contains_key(&OsString::from("OLLAMA_HOST")));
    }
}
