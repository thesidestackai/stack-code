# Stack-Code explicit v1 cancellation contract

Foreground interactive sessions: Ctrl-C / SIGINT terminates the whole Stack-Code
session. The expected result is exit 130 or native SIGINT termination.

Background, noninteractive and headless sessions: SIGTERM terminates the whole
Stack-Code session. The expected result is exit 143 or native SIGTERM termination.
The caller must not deliberately launch Stack-Code with SIGTERM ignored
(SIG_IGN) or blocked. Caller-supplied ignored or blocked SIGTERM is unsupported.

Turn-only cancellation is not supported in explicit v1. After supported
cancellation there may be no new provider call, tool invocation, command,
project/worktree write or session continuation.

## UNSUPPORTED_HEADLESS_STARTUP_SIGINT

PID-targeted background SIGINT during pre-runtime startup is not a supported
v1 headless cancellation mechanism. Ordinary noninteractive Bash background
jobs inherit SIGINT ignored; a signal discarded in this state cannot be recovered
by a later handler. The inherited-SIG_IGN witness is intentionally retained as
an unsupported-contract case, not a supported cancellation success.

## Caller-owned preparation subtree settlement

The headless caller/orchestrator owns cancellation and settlement of its broader
preparation process subtree. PID-only cancellation does not imply whole
preparation-tree cleanup. PID-targeted SIGTERM does not automatically settle
every ancestor, sibling or descendant created by an external orchestrator.

## Architecture and qualification boundary

Option C requires no new production process group mechanism, signal supervisor,
readiness handshake or subreaper. Requalification preserves the frozen runtime,
accepted explicit-v1 tool, Daily Launcher, provider and broker bytes.

The offline contract fixture uses actual OS signals, the frozen CLI monitor and
input code, real conversation runtime and disposable fake provider/tool effects.
A command event is a fake dispatch marker, not a host command execution claim.
Runtime exec-stop probes measure the earliest tested post-exec boundary; they do
not claim cleanup of arbitrary preparation helpers. Qualification is specific to
explicit v1; global R3/R5 remain open and inapplicable to explicit v1.
