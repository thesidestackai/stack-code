//! Contained execution for workspace confinement.
//!
//! Exactly one approved command (the controlled smoke's built-in command or an
//! operator declaration, see [`ApprovedCommand`]) is run, mapped to a fixed
//! argv, inside a Bubblewrap sandbox (`/usr/bin/bwrap`): fresh user, mount,
//! PID, IPC, UTS and network namespaces, no capabilities, no further user
//! namespaces, a synthetic read-only root, the bound workspace mounted
//! read-only at `/work`, a private `/tmp` and a read-only empty `HOME`.
//!
//! Lifetime is part of the boundary. The supervisor binds the sandbox's PID
//! namespace init (the process Bubblewrap blocks on `--block-fd`) through a
//! pidfd *before* releasing it, so no untrusted code can run until identity is
//! bound. Whatever ends the run -- the command exiting, the timeout,
//! cancellation or a capture failure -- the namespace init is killed through
//! that pidfd, which makes the kernel kill every process in the namespace; the
//! supervisor then waits for the init to be gone, reaps the outer launcher, and
//! re-checks `/proc` for survivors before returning. If settlement cannot be
//! proven the run is reported as a containment failure, never as a completed
//! command.
//!
//! There is no fallback: when the sandbox cannot be established, nothing runs.

use std::borrow::Cow;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::file_ops::WorkspaceRoot;

/// A command contained execution may run, mapped to a trusted argv.
///
/// Entries come only from the controlled smoke's built-in table
/// ([`approved_command`]) or from an operator declaration validated by
/// [`ApprovedCommand::declared`].
#[derive(Debug, PartialEq, Eq)]
pub struct ApprovedCommand {
    command: Cow<'static, str>,
    argv: Cow<'static, [Cow<'static, str>]>,
}

/// The program every approved command names: the sandbox always execs
/// `/usr/bin/python3`, whatever argv[0] says.
const APPROVED_PROGRAM: &str = "python3";

/// Characters a declared command may contain besides ASCII letters and
/// digits. None of them is special to a POSIX shell, so reading a declaration
/// as shell words or as space-separated argv gives the same tokens.
const DECLARED_COMMAND_PUNCTUATION: &str = " _-./=:,+@%";

impl ApprovedCommand {
    /// Validate an operator-declared command and map it to its argv.
    ///
    /// The declaration is the exact string a `bash` request must equal to run
    /// it. It is never given to a shell: it is split on single spaces into the
    /// argv the sandbox execs. It is refused unless that split is unambiguous
    /// (tokens separated by exactly one space, no leading or trailing space,
    /// only ASCII letters, digits and [`DECLARED_COMMAND_PUNCTUATION`], so no
    /// quoting, globbing, chaining or redirection) and unless the first token
    /// is `python3`, the only program contained execution runs.
    pub fn declared(command: &str) -> Result<Self, String> {
        if command.is_empty() {
            return Err("declared command is empty".to_string());
        }
        if let Some(character) = command.chars().find(|character| {
            !character.is_ascii_alphanumeric() && !DECLARED_COMMAND_PUNCTUATION.contains(*character)
        }) {
            return Err(format!(
                "declared command {command:?} contains {character:?}; quoting and shell syntax are not supported"
            ));
        }
        let argv = command.split(' ').collect::<Vec<_>>();
        if argv.iter().any(|token| token.is_empty()) {
            return Err(format!(
                "declared command {command:?} must separate its arguments with single spaces and have no leading or trailing space"
            ));
        }
        if argv[0] != APPROVED_PROGRAM {
            return Err(format!(
                "declared command {command:?} must start with `{APPROVED_PROGRAM}`, the only program contained execution runs"
            ));
        }
        Ok(Self {
            command: Cow::Owned(command.to_string()),
            argv: argv
                .into_iter()
                .map(|token| Cow::Owned(token.to_string()))
                .collect(),
        })
    }

    /// The exact command string that selects this entry.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }
}

const APPROVED_COMMANDS: &[ApprovedCommand] = &[ApprovedCommand {
    command: Cow::Borrowed("python3 -B -m unittest -v test_calculator"),
    argv: Cow::Borrowed(&[
        Cow::Borrowed("python3"),
        Cow::Borrowed("-B"),
        Cow::Borrowed("-m"),
        Cow::Borrowed("unittest"),
        Cow::Borrowed("-v"),
        Cow::Borrowed("test_calculator"),
    ]),
}];

/// Look up an approved command by exact string equality. No normalization:
/// whitespace, quoting or any other variation is simply not approved.
#[must_use]
pub fn approved_command(command: &str) -> Option<&'static ApprovedCommand> {
    APPROVED_COMMANDS
        .iter()
        .find(|approved| approved.command == command)
}

/// Bytes of stdout / stderr retained per stream; the rest is drained and
/// discarded so a flooding payload can neither exhaust memory nor stall.
pub const CAPTURE_LIMIT_BYTES: usize = 16_384;

/// How a settled contained run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainedTermination {
    /// The command exited on its own; status as reported by the launcher.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    /// The deadline elapsed and the namespace was killed.
    TimedOut,
    /// The caller cancelled and the namespace was killed.
    Cancelled,
}

/// Evidence that the sandbox's whole process tree is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// Host PID of the sandbox PID-namespace init.
    pub namespace_init_pid: i32,
    /// Inode of the sandbox PID namespace.
    pub pid_namespace: u64,
    /// Processes found in that namespace after settlement (always 0 on `Ok`).
    pub residual_processes: usize,
}

/// A settled contained run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedOutcome {
    pub termination: ContainedTermination,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub settlement: Settlement,
}

/// Why a contained run did not produce an outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentError {
    /// Contained execution is not available on this platform.
    Unsupported,
    /// `/usr/bin/bwrap` is missing or not executable. Nothing was run.
    LauncherUnavailable(String),
    /// The launcher could not be started or supervised. Nothing was released.
    Spawn(String),
    /// The namespace init could not be found or verified. Nothing was released.
    IdentityNotBound(String),
    /// The sandbox failed before the approved command started.
    SetupFailed(String),
    /// Output capture failed; the namespace was settled before reporting.
    CaptureFailed(String),
    /// Termination of the sandbox process tree could not be proven.
    SettlementUnproven(String),
}

impl ContainmentError {
    /// Whether this failure leaves the sandbox's process tree unaccounted for,
    /// in which case the caller must stop further confined operations.
    #[must_use]
    pub fn is_settlement_failure(&self) -> bool {
        matches!(self, Self::SettlementUnproven(_))
    }
}

impl fmt::Display for ContainmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => write!(f, "contained execution requires Linux"),
            Self::LauncherUnavailable(detail) => {
                write!(f, "sandbox launcher unavailable: {detail}")
            }
            Self::Spawn(detail) => write!(f, "sandbox launch failed: {detail}"),
            Self::IdentityNotBound(detail) => {
                write!(f, "sandbox namespace identity not bound: {detail}")
            }
            Self::SetupFailed(detail) => write!(f, "sandbox setup failed: {detail}"),
            Self::CaptureFailed(detail) => write!(f, "output capture failed: {detail}"),
            Self::SettlementUnproven(detail) => {
                write!(f, "sandbox process settlement unproven: {detail}")
            }
        }
    }
}

impl std::error::Error for ContainmentError {}

/// A request to run an approved command against a bound workspace.
pub struct ContainedRequest<'a> {
    pub root: &'a WorkspaceRoot,
    pub command: &'a ApprovedCommand,
    pub timeout: Duration,
    /// Set to `true` from another thread to cancel; the run still settles.
    pub cancel: Option<&'a AtomicBool>,
}

/// Run an approved command in the sandbox and settle it. See module docs.
pub fn run_contained(request: &ContainedRequest<'_>) -> Result<ContainedOutcome, ContainmentError> {
    #[cfg(target_os = "linux")]
    {
        let spec = linux::LaunchSpec::production(request.root)?;
        linux::run(request, &spec)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = request;
        Err(ContainmentError::Unsupported)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::OsString;
    use std::fs;
    use std::io::{self, Read, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::{Duration, Instant};

    use rustix::event::{poll, PollFd, PollFlags, Timespec};
    use rustix::fd::OwnedFd;
    use rustix::io::Errno;
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};

    use super::{
        ContainedOutcome, ContainedRequest, ContainedTermination, ContainmentError, Settlement,
        WorkspaceRoot, CAPTURE_LIMIT_BYTES,
    };

    /// Trusted launcher. Never looked up on `PATH`.
    const BWRAP: &str = "/usr/bin/bwrap";
    /// Interpreter inside the sandbox (from the read-only `/usr` bind).
    const SANDBOX_PYTHON: &str = "/usr/bin/python3";
    const SANDBOX_WORKDIR: &str = "/work";
    const SANDBOX_HOME: &str = "/home/sandbox";

    /// The complete environment of the launcher and the sandbox. The parent
    /// environment is cleared; nothing is passed through.
    const SANDBOX_ENV: &[(&str, &str)] = &[
        ("PATH", "/usr/bin"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
        ("PYTHONNOUSERSITE", "1"),
        ("HOME", SANDBOX_HOME),
        ("TMPDIR", "/tmp"),
    ];

    const SETUP_TIMEOUT: Duration = Duration::from_secs(10);
    const SETTLE_TIMEOUT: Duration = Duration::from_secs(5);
    const TICK: Duration = Duration::from_millis(10);

    /// Written by the trusted prologue immediately before it execs the
    /// approved command, so its presence proves setup completed and the
    /// command started; the untrusted command cannot write before it.
    const START_MARKER: &str = "\u{1e}stack-code-contained-exec-start\u{1e}\n";

    /// Trusted in-sandbox prologue (`python3 -I -S -B -c`, so no site, user
    /// or workspace code is loaded). It closes every inherited descriptor
    /// above stderr, points stdin at `/dev/null` (fd 0 was the release
    /// channel), checks that `/work` is the bound workspace inode, emits the
    /// start marker and execs the approved argv.
    const PROLOGUE: &str = r#"import os, stat, sys
def refuse(reason):
    os.write(2, ("stack-code containment prologue refused: " + reason + "\n").encode())
    os._exit(125)
try:
    os.closerange(3, 1 << 20)
    null = os.open("/dev/null", os.O_RDONLY)
    if null != 0:
        os.dup2(null, 0)
        os.close(null)
    else:
        os.set_inheritable(0, True)
    st = os.stat("/work", follow_symlinks=False)
    if not stat.S_ISDIR(st.st_mode) or (st.st_dev, st.st_ino) != (int(sys.argv[1]), int(sys.argv[2])):
        refuse("/work is not the bound workspace")
    if os.getcwd() != "/work":
        refuse("working directory is not /work")
    os.write(1, sys.argv[3].encode())
    os.execv("/usr/bin/python3", sys.argv[4:])
except BaseException as error:
    refuse(repr(error))
"#;

    pub(super) struct LaunchSpec {
        pub(super) bwrap: PathBuf,
        pub(super) workspace_source: PathBuf,
        pub(super) expected_identity: (u64, u64),
    }

    impl LaunchSpec {
        pub(super) fn production(root: &WorkspaceRoot) -> Result<Self, ContainmentError> {
            let workspace_source = root.current_path().map_err(|error| {
                ContainmentError::Spawn(format!("bound workspace path unavailable: {error}"))
            })?;
            Ok(Self {
                bwrap: PathBuf::from(BWRAP),
                workspace_source,
                expected_identity: root.identity(),
            })
        }

        fn args(&self, argv: &[&str]) -> Vec<OsString> {
            let mut launcher_args: Vec<OsString> = [
                "--unshare-user",
                "--unshare-pid",
                "--unshare-ipc",
                "--unshare-uts",
                "--unshare-net",
                "--unshare-cgroup-try",
                "--disable-userns",
                "--cap-drop",
                "ALL",
                "--die-with-parent",
                "--new-session",
                "--hostname",
                "stack-code-sandbox",
                "--block-fd",
                "0",
                "--ro-bind",
                "/usr",
                "/usr",
                "--tmpfs",
                "/usr/local",
                "--remount-ro",
                "/usr/local",
                "--symlink",
                "usr/bin",
                "/bin",
                "--symlink",
                "usr/sbin",
                "/sbin",
                "--symlink",
                "usr/lib",
                "/lib",
                "--symlink",
                "usr/lib64",
                "/lib64",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--dir",
                SANDBOX_HOME,
                "--ro-bind",
            ]
            .iter()
            .map(OsString::from)
            .collect();
            launcher_args.push(self.workspace_source.clone().into_os_string());
            launcher_args.extend(
                [
                    SANDBOX_WORKDIR,
                    "--remount-ro",
                    "/",
                    "--chdir",
                    SANDBOX_WORKDIR,
                    "--",
                    SANDBOX_PYTHON,
                    "-I",
                    "-S",
                    "-B",
                    "-c",
                    PROLOGUE,
                ]
                .iter()
                .map(OsString::from),
            );
            launcher_args.push(self.expected_identity.0.to_string().into());
            launcher_args.push(self.expected_identity.1.to_string().into());
            launcher_args.push(START_MARKER.into());
            launcher_args.extend(argv.iter().map(OsString::from));
            launcher_args
        }
    }

    struct NamespaceInit {
        pid: i32,
        pidfd: OwnedFd,
        pid_namespace: u64,
    }

    /// Owns the launcher until settlement; kills and reaps on early exit
    /// (including unwinding) so no path leaves the sandbox running.
    struct Supervised {
        launcher: Child,
        launcher_pidfd: OwnedFd,
        init: Option<NamespaceInit>,
        settled: bool,
    }

    impl Supervised {
        fn kill_namespace(&self) {
            if let Some(init) = &self.init {
                // ESRCH (already exited) is fine: the goal is that it is gone.
                let _ = pidfd_send_signal(&init.pidfd, Signal::KILL);
            }
        }

        /// Kill (if still running) and prove the whole tree is gone.
        fn settle(&mut self) -> Result<(Option<i32>, Option<i32>), ContainmentError> {
            self.kill_namespace();
            if self.init.is_none() {
                // Never released: nothing untrusted ran, stop the launcher now.
                let _ = self.launcher.kill();
            }
            if let Some(init) = &self.init {
                if !wait_readable(&init.pidfd, SETTLE_TIMEOUT) {
                    return Err(ContainmentError::SettlementUnproven(format!(
                        "namespace init {} did not exit within {SETTLE_TIMEOUT:?}",
                        init.pid
                    )));
                }
            }
            if !wait_readable(&self.launcher_pidfd, SETTLE_TIMEOUT) {
                let _ = self.launcher.kill();
                if !wait_readable(&self.launcher_pidfd, SETTLE_TIMEOUT) {
                    return Err(ContainmentError::SettlementUnproven(
                        "launcher did not exit".to_string(),
                    ));
                }
            }
            let status = self.launcher.wait().map_err(|error| {
                ContainmentError::SettlementUnproven(format!("launcher not reaped: {error}"))
            })?;
            if let Some(init) = &self.init {
                let residents = namespace_residents(init.pid_namespace)?;
                if residents != 0 {
                    return Err(ContainmentError::SettlementUnproven(format!(
                        "{residents} process(es) remain in sandbox pid namespace {}",
                        init.pid_namespace
                    )));
                }
            }
            self.settled = true;
            Ok((status.code(), status.signal()))
        }
    }

    impl Drop for Supervised {
        fn drop(&mut self) {
            if !self.settled {
                self.kill_namespace();
                let _ = self.launcher.kill();
                let _ = self.launcher.wait();
            }
        }
    }

    struct Captured {
        bytes: Vec<u8>,
        truncated: bool,
    }

    fn capture(
        mut stream: impl Read,
        limit: usize,
        failed: &AtomicBool,
    ) -> Result<Captured, String> {
        let mut bytes = Vec::new();
        let mut truncated = false;
        let mut buffer = [0u8; 8192];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => return Ok(Captured { bytes, truncated }),
                Ok(read) => {
                    let room = limit.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&buffer[..read.min(room)]);
                    truncated |= read > room;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    failed.store(true, Ordering::SeqCst);
                    return Err(error.to_string());
                }
            }
        }
    }

    fn wait_readable(fd: &OwnedFd, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if poll_readable(fd, TICK) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
        }
    }

    /// A pidfd polls readable once its process has exited.
    fn poll_readable(fd: &OwnedFd, timeout: Duration) -> bool {
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        let mut fds = [PollFd::new(fd, PollFlags::IN)];
        matches!(poll(&mut fds, Some(&timeout)), Ok(ready) if ready > 0)
    }

    fn open_pidfd(pid: i32) -> Result<OwnedFd, Errno> {
        let pid = Pid::from_raw(pid).ok_or(Errno::SRCH)?;
        pidfd_open(pid, PidfdFlags::empty())
    }

    fn pid_namespace_of(pid: &str) -> io::Result<u64> {
        let link = fs::read_link(format!("/proc/{pid}/ns/pid"))?;
        link.to_str()
            .and_then(|text| text.strip_prefix("pid:["))
            .and_then(|text| text.strip_suffix(']'))
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| io::Error::other(format!("unexpected ns link {}", link.display())))
    }

    /// Count host processes still inside the sandbox PID namespace.
    fn namespace_residents(pid_namespace: u64) -> Result<usize, ContainmentError> {
        let entries = fs::read_dir("/proc").map_err(|error| {
            ContainmentError::SettlementUnproven(format!("cannot scan /proc: {error}"))
        })?;
        Ok(entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
            .filter(|name| pid_namespace_of(name).is_ok_and(|ns| ns == pid_namespace))
            .count())
    }

    /// Find the launcher's single child (the namespace init, blocked on
    /// `--block-fd`), hold a pidfd to it, and verify it is PID 1 of a new
    /// PID namespace whose parent is our launcher.
    fn bind_namespace_init(
        launcher_pid: u32,
        launcher_pidfd: &OwnedFd,
    ) -> Result<NamespaceInit, ContainmentError> {
        let deadline = Instant::now() + SETUP_TIMEOUT;
        let children_path = format!("/proc/{launcher_pid}/task/{launcher_pid}/children");
        let pid = loop {
            if poll_readable(launcher_pidfd, Duration::ZERO) {
                return Err(ContainmentError::SetupFailed(
                    "launcher exited before the sandbox was bound".to_string(),
                ));
            }
            let children = fs::read_to_string(&children_path).unwrap_or_default();
            let pids = children
                .split_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ContainmentError::IdentityNotBound(error.to_string()))?;
            match pids.as_slice() {
                [] => {}
                [pid] => break *pid,
                many => {
                    return Err(ContainmentError::IdentityNotBound(format!(
                        "launcher has {} children",
                        many.len()
                    )))
                }
            }
            if Instant::now() >= deadline {
                return Err(ContainmentError::IdentityNotBound(
                    "namespace init did not appear".to_string(),
                ));
            }
            thread::sleep(TICK);
        };
        let pidfd = open_pidfd(pid)
            .map_err(|error| ContainmentError::IdentityNotBound(format!("pidfd {pid}: {error}")))?;
        let unbound = |detail: String| ContainmentError::IdentityNotBound(detail);
        let status = fs::read_to_string(format!("/proc/{pid}/status"))
            .map_err(|error| unbound(format!("status of {pid}: {error}")))?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        };
        if field("PPid:") != launcher_pid.to_string() {
            return Err(unbound(format!("{pid} is not a child of the launcher")));
        }
        let nspid = field("NSpid:");
        let nspid = nspid.split_whitespace().collect::<Vec<_>>();
        if nspid.len() < 2 || nspid.last() != Some(&"1") {
            return Err(unbound(format!("{pid} is not a PID-namespace init")));
        }
        let pid_namespace = pid_namespace_of(&pid.to_string())
            .map_err(|error| unbound(format!("pid namespace of {pid}: {error}")))?;
        let own_namespace = pid_namespace_of("self")
            .map_err(|error| unbound(format!("own pid namespace: {error}")))?;
        if pid_namespace == own_namespace {
            return Err(unbound(format!("{pid} shares the host pid namespace")));
        }
        // Checked last: while the pidfd's process is alive, the /proc reads
        // above necessarily described that same process.
        if poll_readable(&pidfd, Duration::ZERO) {
            return Err(unbound(format!("{pid} exited during verification")));
        }
        Ok(NamespaceInit {
            pid,
            pidfd,
            pid_namespace,
        })
    }

    fn release(stdin: Option<ChildStdin>) -> io::Result<()> {
        let mut stdin = stdin.ok_or_else(|| io::Error::other("release channel missing"))?;
        stdin.write_all(b"1")?;
        stdin.flush()
    }

    enum Stop {
        Natural,
        TimedOut,
        Cancelled,
        CaptureFailed,
        ReleaseFailed(String),
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn run(
        request: &ContainedRequest<'_>,
        spec: &LaunchSpec,
    ) -> Result<ContainedOutcome, ContainmentError> {
        let launcher_meta = fs::metadata(&spec.bwrap).map_err(|error| {
            ContainmentError::LauncherUnavailable(format!("{}: {error}", spec.bwrap.display()))
        })?;
        if !launcher_meta.is_file() {
            return Err(ContainmentError::LauncherUnavailable(format!(
                "{} is not a regular file",
                spec.bwrap.display()
            )));
        }

        let argv = request
            .command
            .argv
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>();
        let mut command = Command::new(&spec.bwrap);
        command
            .args(spec.args(&argv))
            .env_clear()
            .envs(SANDBOX_ENV.iter().copied())
            .current_dir(Path::new("/"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut launcher = command.spawn().map_err(|error| match error.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => {
                ContainmentError::LauncherUnavailable(format!("{}: {error}", spec.bwrap.display()))
            }
            _ => ContainmentError::Spawn(error.to_string()),
        })?;
        let launcher_pid = launcher.id();
        let stdin = launcher.stdin.take();
        let stdout = launcher.stdout.take();
        let stderr = launcher.stderr.take();
        let launcher_pidfd = match i32::try_from(launcher_pid)
            .map_err(|_| Errno::SRCH)
            .and_then(open_pidfd)
        {
            Ok(fd) => fd,
            Err(error) => {
                let _ = launcher.kill();
                let _ = launcher.wait();
                return Err(ContainmentError::Spawn(format!("launcher pidfd: {error}")));
            }
        };
        let mut supervised = Supervised {
            launcher,
            launcher_pidfd,
            init: None,
            settled: false,
        };

        let capture_failed = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        for (index, stream) in [
            stdout.map(|s| Box::new(s) as Box<dyn Read + Send>),
            stderr.map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .enumerate()
        {
            let Some(stream) = stream else {
                return Err(ContainmentError::Spawn("output pipe missing".to_string()));
            };
            let limit = if index == 0 {
                CAPTURE_LIMIT_BYTES + START_MARKER.len()
            } else {
                CAPTURE_LIMIT_BYTES
            };
            let failed = Arc::clone(&capture_failed);
            let sender = sender.clone();
            thread::spawn(move || {
                let _ = sender.send((index, capture(stream, limit, &failed)));
            });
        }
        drop(sender);

        match bind_namespace_init(launcher_pid, &supervised.launcher_pidfd) {
            Ok(init) => supervised.init = Some(init),
            Err(error) => {
                // Nothing was released, so the approved command never ran.
                supervised.settle()?;
                let stderr = collect(&receiver)
                    .ok()
                    .map(|(_, stderr)| String::from_utf8_lossy(&stderr.bytes).into_owned())
                    .unwrap_or_default();
                return Err(match error {
                    ContainmentError::SetupFailed(detail) => {
                        ContainmentError::SetupFailed(format!("{detail}: {}", stderr.trim()))
                    }
                    other => other,
                });
            }
        }

        let started = Instant::now();
        let stop = match release(stdin) {
            Err(error) => Stop::ReleaseFailed(error.to_string()),
            Ok(()) => loop {
                let init = supervised.init.as_ref().expect("namespace init is bound");
                if poll_readable(&init.pidfd, TICK) {
                    break Stop::Natural;
                }
                if capture_failed.load(Ordering::SeqCst) {
                    break Stop::CaptureFailed;
                }
                if request
                    .cancel
                    .is_some_and(|cancel| cancel.load(Ordering::SeqCst))
                {
                    break Stop::Cancelled;
                }
                if started.elapsed() >= request.timeout {
                    break Stop::TimedOut;
                }
            },
        };

        let (code, signal) = supervised.settle()?;
        let (stdout, stderr) = collect(&receiver)?;
        let settlement = {
            let init = supervised.init.as_ref().expect("namespace init is bound");
            Settlement {
                namespace_init_pid: init.pid,
                pid_namespace: init.pid_namespace,
                residual_processes: 0,
            }
        };

        let started_payload = stdout.bytes.starts_with(START_MARKER.as_bytes());
        let stderr_text = String::from_utf8_lossy(&stderr.bytes).into_owned();
        let termination = match stop {
            Stop::CaptureFailed => {
                return Err(ContainmentError::CaptureFailed(
                    "output stream read failed".to_string(),
                ))
            }
            Stop::ReleaseFailed(detail) => {
                return Err(ContainmentError::SetupFailed(format!(
                    "release failed: {detail}: {}",
                    stderr_text.trim()
                )))
            }
            Stop::TimedOut => ContainedTermination::TimedOut,
            Stop::Cancelled => ContainedTermination::Cancelled,
            Stop::Natural if !started_payload => {
                return Err(ContainmentError::SetupFailed(format!(
                    "the approved command never started: {}",
                    stderr_text.trim()
                )))
            }
            Stop::Natural => ContainedTermination::Exited { code, signal },
        };
        let stdout_bytes = if started_payload {
            &stdout.bytes[START_MARKER.len()..]
        } else {
            &stdout.bytes[..]
        };
        Ok(ContainedOutcome {
            termination,
            stdout: String::from_utf8_lossy(stdout_bytes).into_owned(),
            stderr: stderr_text,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            settlement,
        })
    }

    /// Both capture threads end at EOF, which after settlement means every
    /// writer is gone. A missing EOF means something outside the settled
    /// tree still holds an output pipe.
    fn collect(
        receiver: &mpsc::Receiver<(usize, Result<Captured, String>)>,
    ) -> Result<(Captured, Captured), ContainmentError> {
        let mut streams: [Option<Captured>; 2] = [None, None];
        for _ in 0..2 {
            let (index, captured) = receiver.recv_timeout(SETTLE_TIMEOUT).map_err(|_| {
                ContainmentError::SettlementUnproven(
                    "an output pipe stayed open after settlement".to_string(),
                )
            })?;
            streams[index] = Some(captured.map_err(ContainmentError::CaptureFailed)?);
        }
        let [Some(stdout), Some(stderr)] = streams else {
            return Err(ContainmentError::CaptureFailed(
                "output stream missing".to_string(),
            ));
        };
        Ok((stdout, stderr))
    }

    #[cfg(test)]
    pub(super) fn test_spec(root: &WorkspaceRoot) -> LaunchSpec {
        LaunchSpec::production(root).expect("production spec")
    }
}

/// Launcher path used by the tests to prove fail-closed behavior.
#[cfg(all(test, target_os = "linux"))]
fn run_with_launcher(
    request: &ContainedRequest<'_>,
    bwrap: std::path::PathBuf,
    identity: Option<(u64, u64)>,
) -> Result<ContainedOutcome, ContainmentError> {
    let mut spec = linux::test_spec(request.root);
    spec.bwrap = bwrap;
    if let Some(identity) = identity {
        spec.expected_identity = identity;
    }
    linux::run(request, &spec)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use super::{
        approved_command, run_contained, run_with_launcher, ApprovedCommand, ContainedOutcome,
        ContainedRequest, ContainedTermination, ContainmentError,
    };
    use crate::file_ops::WorkspaceRoot;

    const SMOKE: &str = "python3 -B -m unittest -v test_calculator";
    const TESTS: &str = "import unittest\nfrom calculator import add\n\n\nclass TestAdd(unittest.TestCase):\n    def test_positive(self):\n        self.assertEqual(add(2, 3), 5)\n\n    def test_negative(self):\n        self.assertEqual(add(-2, -3), -5)\n\n    def test_zero(self):\n        self.assertEqual(add(0, 0), 0)\n";
    const ADD: &str = "\ndef add(a, b):\n    return a + b\n";

    struct Fixture {
        base: PathBuf,
        workspace: PathBuf,
        outside: PathBuf,
    }

    impl Fixture {
        fn new(name: &str, calculator: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos();
            let base = std::env::temp_dir().join(format!("claw-contained-{name}-{unique}"));
            let workspace = base.join("workspace");
            let outside = base.join("outside");
            fs::create_dir_all(&workspace).expect("workspace");
            fs::create_dir_all(&outside).expect("outside");
            fs::write(workspace.join("calculator.py"), calculator).expect("calculator");
            fs::write(workspace.join("test_calculator.py"), TESTS).expect("tests");
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

        fn run(&self, timeout_ms: u64) -> Result<ContainedOutcome, ContainmentError> {
            let root = self.root();
            run_contained(&ContainedRequest {
                root: &root,
                command: approved_command(SMOKE).expect("approved"),
                timeout: Duration::from_millis(timeout_ms),
                cancel: None,
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// Bubblewrap with user namespaces is required for the sandbox tests;
    /// hosts without it still run the fail-closed tests below.
    fn sandbox_ready() -> bool {
        let ready = Path::new("/usr/bin/bwrap").is_file()
            && Path::new("/usr/bin/python3").exists()
            && std::process::Command::new("/usr/bin/bwrap")
                .args([
                    "--unshare-user",
                    "--unshare-pid",
                    "--unshare-net",
                    "--ro-bind",
                    "/usr",
                    "/usr",
                    "--symlink",
                    "usr/lib64",
                    "/lib64",
                    "--symlink",
                    "usr/lib",
                    "/lib",
                    "/usr/bin/true",
                ])
                .status()
                .is_ok_and(|status| status.success());
        if !ready {
            eprintln!("skipping: Bubblewrap user-namespace sandbox unavailable on this host");
        }
        ready
    }

    /// A Python string literal for `path`.
    fn py(path: &Path) -> String {
        format!("{:?}", path.to_string_lossy())
    }

    fn pid_namespace_residents(pid_namespace: u64) -> usize {
        let target = format!("pid:[{pid_namespace}]");
        fs::read_dir("/proc")
            .expect("proc")
            .flatten()
            .filter(|entry| {
                fs::read_link(entry.path().join("ns/pid"))
                    .is_ok_and(|link| link == Path::new(&target))
            })
            .count()
    }

    fn exited(outcome: &ContainedOutcome) -> Option<i32> {
        match outcome.termination {
            ContainedTermination::Exited { code, .. } => code,
            _ => None,
        }
    }

    /// Payload whose setsid'd grandchild outlives its parent and, if it were
    /// not killed, would write a marker after `late` seconds.
    fn orphaning_payload(late: f64, parent_sleep: f64, marker: &Path) -> String {
        format!(
            "import os, time\nif os.fork() == 0:\n    os.setsid()\n    if os.fork() != 0:\n        os._exit(0)\n    for fd in (0, 1, 2):\n        os.close(fd)\n    time.sleep({late})\n    open({marker}, 'w').write('ALIVE')\n    os._exit(0)\ntime.sleep({parent_sleep})\n{ADD}",
            marker = py(marker)
        )
    }

    #[test]
    fn only_the_exact_smoke_command_is_approved() {
        assert_eq!(
            approved_command(SMOKE).map(super::ApprovedCommand::command),
            Some(SMOKE)
        );
        for near_miss in [
            " python3 -B -m unittest -v test_calculator",
            "python3 -B -m unittest -v test_calculator ",
            "python3  -B -m unittest -v test_calculator",
            "python3 -m unittest -v test_calculator",
            "/usr/bin/python3 -B -m unittest -v test_calculator",
            "python3 -B -m unittest -v test_calculator; true",
            "sh -c 'python3 -B -m unittest -v test_calculator'",
        ] {
            assert!(approved_command(near_miss).is_none(), "{near_miss:?}");
        }
    }

    const DECLARED: &str = "python3 -B -m unittest discover -v -s . -p test_calculator.py";

    fn argv(command: &ApprovedCommand) -> Vec<&str> {
        command.argv.iter().map(AsRef::as_ref).collect()
    }

    #[test]
    fn a_declared_command_maps_to_its_space_separated_argv() {
        let declared = ApprovedCommand::declared(DECLARED).expect("a plain python3 command");
        assert_eq!(declared.command(), DECLARED);
        assert_eq!(
            argv(&declared),
            [
                "python3",
                "-B",
                "-m",
                "unittest",
                "discover",
                "-v",
                "-s",
                ".",
                "-p",
                "test_calculator.py"
            ]
        );
        // The built-in smoke entry keeps its fixed argv.
        assert_eq!(
            argv(approved_command(SMOKE).expect("approved")),
            ["python3", "-B", "-m", "unittest", "-v", "test_calculator"]
        );
        // A declaration is not added to the built-in table.
        assert!(approved_command(DECLARED).is_none());
    }

    #[test]
    fn shell_syntax_in_a_declared_command_is_refused() {
        for (declared, character) in [
            ("python3 -B x; touch y", ';'),
            ("python3 -B x && y", '&'),
            ("python3 -B x || y", '|'),
            ("python3 -B x &", '&'),
            ("python3 -B x | tee y", '|'),
            ("python3 -B x > out", '>'),
            ("python3 -B x < in", '<'),
            ("python3 -c 'print(1)'", '\''),
            ("python3 -c \"print(1)\"", '"'),
            ("python3 -B $(id)", '$'),
            ("python3 -B `id`", '`'),
            ("python3 -B test_*.py", '*'),
            ("python3 -B ~/x.py", '~'),
            ("python3 -B x\\ y", '\\'),
            ("python3 -B x#y", '#'),
            ("python3\t-B x", '\t'),
            ("python3 -B x\n", '\n'),
        ] {
            let error = ApprovedCommand::declared(declared).expect_err("shell syntax is refused");
            assert!(
                error.contains(&format!("contains {character:?}")),
                "{declared:?}: {error}"
            );
        }
    }

    #[test]
    fn ambiguous_spacing_or_another_program_is_refused() {
        for (declared, reason) in [
            ("", "is empty"),
            (" python3 -B x", "single spaces"),
            ("python3 -B x ", "single spaces"),
            ("python3  -B x", "single spaces"),
            ("env X=1 python3 -B x", "must start with `python3`"),
            ("X=1 python3 -B x", "must start with `python3`"),
            ("bash -c x", "must start with `python3`"),
            ("sh -c x", "must start with `python3`"),
            ("/usr/bin/python3 -B x", "must start with `python3`"),
            ("python -B x", "must start with `python3`"),
            ("python3.12 -B x", "must start with `python3`"),
            ("pytest -q", "must start with `python3`"),
        ] {
            let error = ApprovedCommand::declared(declared).expect_err("must be refused");
            assert!(error.contains(reason), "{declared:?}: {error}");
        }
    }

    #[test]
    fn a_declared_command_runs_its_argv_inside_the_sandbox() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("declared", ADD);
        let root = fixture.root();
        let declared = ApprovedCommand::declared(DECLARED).expect("declarable");
        let outcome = run_contained(&ContainedRequest {
            root: &root,
            command: &declared,
            timeout: Duration::from_secs(30),
            cancel: None,
        })
        .expect("the declared command should run");
        assert_eq!(exited(&outcome), Some(0), "{outcome:?}");
        assert!(outcome.stderr.contains("Ran 3 tests"), "{outcome:?}");
        assert_eq!(outcome.settlement.residual_processes, 0);
    }

    #[test]
    fn missing_or_unusable_launcher_fails_closed_without_running_anything() {
        let fixture = Fixture::new("launcher", "");
        let marker = fixture.outside.join("UNSANDBOXED");
        fs::write(
            fixture.workspace.join("calculator.py"),
            format!("open({}, 'w').write('ran')\n{ADD}", py(&marker)),
        )
        .expect("payload");
        let root = fixture.root();
        let request = ContainedRequest {
            root: &root,
            command: approved_command(SMOKE).expect("approved"),
            timeout: Duration::from_secs(10),
            cancel: None,
        };
        let missing = run_with_launcher(&request, fixture.base.join("no-such-bwrap"), None)
            .expect_err("a missing launcher must fail");
        assert!(
            matches!(missing, ContainmentError::LauncherUnavailable(_)),
            "{missing:?}"
        );
        let directory = run_with_launcher(&request, fixture.base.clone(), None)
            .expect_err("a directory is not a launcher");
        assert!(
            matches!(directory, ContainmentError::LauncherUnavailable(_)),
            "{directory:?}"
        );
        for fake in ["/usr/bin/false", "/usr/bin/true"] {
            let error = run_with_launcher(&request, PathBuf::from(fake), None)
                .expect_err("a launcher that makes no sandbox must fail");
            assert!(
                matches!(error, ContainmentError::SetupFailed(_)),
                "{fake}: {error:?}"
            );
        }
        assert!(!marker.exists(), "nothing may run without the sandbox");
    }

    #[test]
    fn benign_run_distinguishes_failing_and_passing_tests() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("benign", "def add(a, b):\n    return a - b\n");
        let failing = fixture.run(30_000).expect("run");
        assert_eq!(exited(&failing), Some(1), "{failing:?}");
        assert!(
            failing.stderr.contains("FAILED (failures=2)"),
            "{}",
            failing.stderr
        );
        assert_eq!(failing.settlement.residual_processes, 0);
        assert!(failing.stdout.is_empty(), "start marker must be stripped");

        fs::write(fixture.workspace.join("calculator.py"), ADD).expect("repair");
        let passing = fixture.run(30_000).expect("run");
        assert_eq!(exited(&passing), Some(0), "{passing:?}");
        assert!(passing.stderr.contains("Ran 3 tests"), "{}", passing.stderr);
        assert!(
            passing.stderr.trim_end().ends_with("OK"),
            "{}",
            passing.stderr
        );
    }

    #[test]
    fn setup_failure_after_release_is_not_reported_as_a_command_result() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("identity", "");
        let marker = fixture.outside.join("RAN");
        fs::write(
            fixture.workspace.join("calculator.py"),
            format!("open('/tmp/x', 'w')\nraise SystemExit(1)\n{ADD}"),
        )
        .expect("payload");
        let root = fixture.root();
        let request = ContainedRequest {
            root: &root,
            command: approved_command(SMOKE).expect("approved"),
            timeout: Duration::from_secs(10),
            cancel: None,
        };
        // A wrong expected workspace identity makes the trusted prologue
        // refuse after release; that must surface as a setup failure.
        let error = run_with_launcher(&request, PathBuf::from("/usr/bin/bwrap"), Some((1, 1)))
            .expect_err("identity mismatch must not look like a test result");
        match error {
            ContainmentError::SetupFailed(detail) => {
                assert!(detail.contains("not the bound workspace"), "{detail}");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(!marker.exists());
    }

    #[test]
    fn workspace_is_read_only_and_host_is_not_visible() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("readonly", "");
        let sentinel = fixture.outside.join("sentinel.txt");
        fs::write(
            fixture.workspace.join("calculator.py"),
            format!(
                "import os\nfor action in (lambda: open('test_calculator.py', 'a').write('#'), lambda: open('new.py', 'w'), lambda: open({sentinel}, 'w').write('X'), lambda: open('../escape', 'w')):\n    try:\n        action(); print('WROTE')\n    except OSError:\n        print('REFUSED')\nprint('HOST', os.path.exists({outside}), os.path.exists('/home'), os.path.exists('/run'), os.path.exists('/var'), os.path.exists('/usr/local/bin'))\n{ADD}",
                sentinel = py(&sentinel),
                outside = py(&fixture.outside)
            ),
        )
        .expect("payload");
        let before = fs::read(fixture.workspace.join("test_calculator.py")).expect("oracle");
        let outcome = fixture.run(30_000).expect("run");
        assert_eq!(
            outcome.stdout.matches("REFUSED").count(),
            4,
            "{}",
            outcome.stdout
        );
        assert!(!outcome.stdout.contains("WROTE"), "{}", outcome.stdout);
        assert!(
            outcome.stdout.contains("HOST False True False False False"),
            "{}",
            outcome.stdout
        );
        assert_eq!(
            fs::read(fixture.workspace.join("test_calculator.py")).expect("oracle"),
            before
        );
        assert_eq!(
            fs::read_to_string(&sentinel).expect("sentinel"),
            "OUTSIDE\n"
        );
        assert!(!fixture.workspace.join("new.py").exists());
        assert!(!fixture.base.join("escape").exists());
    }

    #[test]
    fn network_is_private_to_the_sandbox() {
        if !sandbox_ready() {
            return;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let fixture = Fixture::new(
            "network",
            &format!(
                "import socket\ntry:\n    socket.create_connection(('127.0.0.1', {port}), timeout=2).sendall(b'LEAK'); print('CONNECTED')\nexcept OSError:\n    print('ISOLATED')\n{ADD}"
            ),
        );
        let outcome = fixture.run(30_000).expect("run");
        assert!(outcome.stdout.contains("ISOLATED"), "{}", outcome.stdout);
        assert!(
            listener.accept().is_err(),
            "host listener must receive nothing"
        );
    }

    #[test]
    fn environment_is_minimal_and_explicit() {
        if !sandbox_ready() {
            return;
        }
        // The supervisor clears the parent environment itself; this proves it
        // with a variable that is present in the test process. `PWD` is set by
        // Bubblewrap to the fixed sandbox working directory.
        std::env::set_var("CLAW_CONTAINMENT_PROBE_SECRET", "must-not-leak");
        let fixture = Fixture::new(
            "environment",
            &format!(
                "import os, sys\nprint(sorted(os.environ.items()))\nprint(sys.executable)\n{ADD}"
            ),
        );
        let outcome = fixture.run(30_000).expect("run");
        std::env::remove_var("CLAW_CONTAINMENT_PROBE_SECRET");
        assert!(
            outcome.stdout.starts_with("[('HOME', '/home/sandbox'), ('LANG', 'C.UTF-8'), ('LC_ALL', 'C.UTF-8'), ('PATH', '/usr/bin'), ('PWD', '/work'), ('PYTHONNOUSERSITE', '1'), ('TMPDIR', '/tmp')]\n/usr/bin/python3\n"),
            "{}",
            outcome.stdout
        );
    }

    #[test]
    fn timeout_settles_the_whole_namespace_before_returning() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("timeout", "");
        let host_marker = fixture.outside.join("late");
        fs::write(
            fixture.workspace.join("calculator.py"),
            orphaning_payload(1.0, 30.0, Path::new("/tmp/late")),
        )
        .expect("payload");
        let started = Instant::now();
        let outcome = fixture.run(300).expect("a timed-out run still settles");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(outcome.termination, ContainedTermination::TimedOut);
        assert_eq!(pid_namespace_residents(outcome.settlement.pid_namespace), 0);
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(pid_namespace_residents(outcome.settlement.pid_namespace), 0);
        assert!(!host_marker.exists());
    }

    #[test]
    fn orphaned_descendants_do_not_outlive_a_successful_command() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("orphan", "");
        fs::write(
            fixture.workspace.join("calculator.py"),
            orphaning_payload(1.0, 0.2, Path::new("/tmp/late")),
        )
        .expect("payload");
        let outcome = fixture.run(30_000).expect("run");
        assert_eq!(exited(&outcome), Some(0), "{outcome:?}");
        assert_eq!(pid_namespace_residents(outcome.settlement.pid_namespace), 0);
    }

    #[test]
    fn cancellation_settles_before_returning() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new("cancel", "");
        fs::write(
            fixture.workspace.join("calculator.py"),
            orphaning_payload(1.0, 30.0, Path::new("/tmp/late")),
        )
        .expect("payload");
        let root = fixture.root();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let outcome = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(400));
                cancel.store(true, Ordering::SeqCst);
            });
            run_contained(&ContainedRequest {
                root: &root,
                command: approved_command(SMOKE).expect("approved"),
                timeout: Duration::from_secs(60),
                cancel: Some(&cancel),
            })
        })
        .expect("a cancelled run still settles");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(outcome.termination, ContainedTermination::Cancelled);
        assert_eq!(pid_namespace_residents(outcome.settlement.pid_namespace), 0);
    }

    #[test]
    fn output_capture_is_bounded() {
        if !sandbox_ready() {
            return;
        }
        let fixture = Fixture::new(
            "flood",
            &format!("import os\nfor _ in range(512):\n    os.write(1, b'A' * 65536)\n    os.write(2, b'B' * 65536)\n{ADD}"),
        );
        let outcome = fixture.run(60_000).expect("run");
        assert!(outcome.stdout_truncated && outcome.stderr_truncated);
        assert_eq!(outcome.stdout.len(), super::CAPTURE_LIMIT_BYTES);
        assert_eq!(outcome.stderr.len(), super::CAPTURE_LIMIT_BYTES);
        assert_eq!(pid_namespace_residents(outcome.settlement.pid_namespace), 0);
    }
}
