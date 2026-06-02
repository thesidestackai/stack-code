# A2-L3 IDE Adapter — Usage Guide (VS Code Claw Status Panel)

This is the operator-facing usage guide for the merged A2-L3 VS Code
Claw Status Panel
([`ide/vscode/claw-status-panel/`](../ide/vscode/claw-status-panel/),
PR #52, merge commit `553434a`). It explains what the panel is, what
it reads, how to use it, and — just as importantly — what it does
**not** do.

This document is **docs-only**. It describes a shipped read-only
observer. It does **not** authorize IDE implementation changes, runtime
changes, approve/apply controls, autonomous writes, or any weakening of
an A2-L2b / A2-L2c / A2-L2d / A2-L3 STOP gate. It is bounded by, and
subordinate to:

- [`docs/a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md)
- [`docs/a2-l3-ide-adapter-implementation-scope-card.md`](./a2-l3-ide-adapter-implementation-scope-card.md)
- [`docs/a2-l3-ide-adapter-readiness-review.md`](./a2-l3-ide-adapter-readiness-review.md)
- [`docs/a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
- [`docs/a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)
- [`docs/a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md)

Where this guide and any scope card differ, the scope cards and (above
them) the A2-L2d status schema remain authoritative on the contract.

## 1. Purpose

The Claw Status Panel is a **read-only visual observer** for the
`a2-l2d-status.v1` envelope. It renders the output of

```text
claw plan status <workspace> [<approval-result.json>]
```

inside a VS Code webview so the operator can read chain state in the
editor instead of re-typing the status command in a terminal on every
chain transition. It is a viewer, never a controller: it never advances
the chain, never approves, never applies, and never writes.

## 2. What The VS Code Status Panel Is

The panel is a single VS Code extension package at
[`ide/vscode/claw-status-panel/`](../ide/vscode/claw-status-panel/). It:

- contributes exactly one command, **`Claw Status: Refresh`**
  (command id `clawStatus.refresh`);
- on that command, invokes `claw plan status` once and renders the
  resulting envelope in a webview panel;
- renders every envelope field verbatim, with STOP signals shown at
  parity with (or greater prominence than) non-STOP state;
- offers copy-to-clipboard for single fields and a collapsible raw-JSON
  view.

It is the first per-host A2-L3 IDE adapter implementation. JetBrains and
language-server-backed hosts, if built, are separate future lanes.

## 3. What It Reads

The panel consumes **only** these inputs:

- `claw plan status` **stdout** (the `a2-l2d-status.v1` envelope, or the
  refusal envelope);
- `claw plan status` **exit code** (`0` success, `12`
  `EXIT_STATUS_REFUSED`, anything else classified as a STOP in its own
  right);
- the **operator-selected workspace path** — taken from the open VS Code
  workspace folder (if more than one folder is open, the panel prompts
  the operator to pick one);
- an **operator-triggered refresh event** — the explicit
  `Claw Status: Refresh` gesture.

The contract also allows an **optional operator-selected
`<approval-result.json>` path** as a second positional argument. The
subprocess wrapper accepts it, but the current VS Code build does not
yet expose a picker for it, so today the panel invokes
`claw plan status <workspace>` with the workspace folder only. See
[§14 Troubleshooting](#14-troubleshooting).

The `claw` binary location is configurable via the `clawStatus.binaryPath`
setting (default: `claw`). The panel only ever runs that binary with the
`plan status` subcommand and the positional arguments above — no flags.

## 4. What It Does Not Read

The panel explicitly does **not**:

- parse any `.claw/**` file directly — chain state is read only through
  `claw plan status` stdout;
- read, hash, preview, or summarize the contents of any
  `evidence_paths` file (clicking a link delegates the open to the VS
  Code editor; the panel never reads the bytes itself);
- talk to a broker, model, or Ollama endpoint;
- emit telemetry or analytics;
- background-poll on any timer;
- subscribe to filesystem watchers or VS Code file-change events
  (`onDidSaveTextDocument`, `onDidChangeTextDocument`, file-system
  watchers, etc.).

Every status read is the direct result of an explicit operator gesture.

## 5. Installation

The package is shipped as source, not as a published `.vsix`
(marketplace packaging is a separate, out-of-scope lane). To build and
run the tests locally:

```bash
cd ide/vscode/claw-status-panel
npm install
npm run lint      # static-grep guards: forbidden API / network / .claw reads
npm run compile   # tsc -p .
npm test          # parser, render, manifest, refresh-boundary, etc.
```

To run the extension inside VS Code, open the
`ide/vscode/claw-status-panel/` folder in VS Code and launch an Extension
Development Host (Run → Start Debugging) after `npm run compile`. Ensure
the `claw` binary is on `PATH`, or set `clawStatus.binaryPath` to its
absolute path in your settings.

## 6. Refreshing Status

There is one way to update the panel: the explicit refresh gesture.

- Open the Command Palette and run **`Claw Status: Refresh`**, or
- click the **Refresh** button rendered at the top of the panel.

Each refresh invokes `claw plan status` **exactly once** and re-renders
the new envelope. There is no auto-refresh, no refresh-on-save, no
refresh-on-focus, and no timer. If the workspace state changes, the
operator must refresh again to see it. A STOP rendering persists until a
later operator-initiated refresh returns an envelope that no longer
carries that STOP.

## 7. Understanding Status Fields

The panel renders the following `a2-l2d-status.v1` fields verbatim
(closed-enum values are shown as the exact literal; `null` renders as a
distinguishable `(none)` placeholder, and an absent
`read_only_invariant` renders as `(absent)`):

| Field | Meaning |
|-------|---------|
| `schema_version` | Must be exactly `a2-l2d-status.v1`. Any other literal is a STOP. |
| `workspace_root` | The workspace the status was read against, verbatim (never re-resolved). |
| `run_id` / `step_id` | The current run / step identifiers, or `(none)`. |
| `phase` | Closed-enum chain phase (see below). |
| `next_operator_command` | The exact command the operator should run next, as copyable text. On a STOP this is the literal `STOP — escalate`. |
| `is_approvable` | Read-only boolean. Never unlocks any approve control. |
| `is_apply_ready` | Read-only boolean. Never unlocks any apply control. |
| `before_sha256` / `after_sha256` / `payload_sha256` / `live_target_sha256` | Verbatim hex digests, or `(none)`. |
| `stop_condition` | Closed-enum STOP reason when non-null (see [§8](#8-stop-conditions)); `(none)` otherwise. |
| `evidence_paths` | List of paths for the operator to inspect (see [§9](#9-evidence-paths)). |
| `audit_markers` | Verbatim list of `a2-l2d-*` markers the producer emitted. |
| `read_only_invariant` | The pinned literal `this command does not mutate state`, surfaced on every envelope. Absence or substitution is itself a STOP. |

Closed `phase` values: `no_run_found`, `preview_ready`,
`awaiting_approval`, `approval_captured`, `apply_bundle_ready`,
`applied`, `rolled_back`, `non_approvable`, `unknown`. A `phase` value
outside this set is rendered verbatim and classified as a STOP.

The raw envelope JSON and the process exit code are available in the
collapsible **raw status JSON** disclosure at the bottom of the panel.

## 8. STOP Conditions

STOP is the chain's escalation signal, and the panel treats it as
load-bearing. When a STOP is present, the panel renders a prominent
**`STOP — escalate`** banner listing each STOP reason as
`kind: literal`, in addition to rendering the offending field verbatim.

The closed `stop_condition` enum is:

```text
workspace-root-invalid
run-manifest-unreadable
preview-bundle-unreadable
payload-sha-mismatch
live-target-missing
live-target-sha-changed
approval-decision-not-approved
approval-sha-mismatch
approval-step-id-mismatch
apply-bundle-schema-mismatch
apply-bundle-target-path-mismatch
```

STOP-handling rules the panel honors:

- **STOP values are rendered verbatim.** The exact enum literal (e.g.
  `payload-sha-mismatch`) is shown — never substituted with friendly
  prose like "mismatch detected". The literal *is* the escalation
  signal.
- **Unknown values are STOP.** Any unknown `phase`, `stop_condition`,
  `next_operator_command` shape, or audit marker — plus schema-version
  drift, a missing/substituted `read_only_invariant`, a missing required
  field, or unparseable stdout — classifies the panel as STOP and is
  shown verbatim.
- **STOP is never downgraded.** A STOP is never re-classified as a
  warning, info, or soft failure.
- **STOP is never hidden.** STOP rendering is at least as prominent as
  non-STOP rendering, and there is no snooze, mute, dismiss, or ignore
  affordance. A STOP persists across refreshes until a refresh returns
  an envelope without it.

On any STOP, follow `next_operator_command` (`STOP — escalate`) and
inspect the `evidence_paths` in your own terminal/editor; do not attempt
to advance the chain from the panel.

## 9. Evidence Paths

`evidence_paths` is the operator's primary STOP-diagnosis surface. The
panel renders each entry as:

- a **local file link** that, when clicked, opens the file in the VS
  Code editor (the host's normal file-open behavior);
- the **verbatim** envelope-carried path text (no canonicalization, no
  rewriting);
- an **`[out-of-workspace]`** flag when the path lies outside the
  workspace root;
- a **`[missing]`** flag when the path does not resolve on disk (the
  path text is still shown — a missing file is surfaced, not hidden).

Evidence paths are **links, not previews**. The panel does **not**:

- read or preview the file contents,
- summarize the file,
- hash the file,
- offer an "open all" gesture (each path is opened by an individual
  operator click).

When a STOP is rendered, the evidence-paths list stays visible without
any extra disclosure step.

## 10. Copy-To-Clipboard Actions

The panel offers single-field copy actions only. Each copies the exact
verbatim value to the system clipboard and does nothing else:

- **`next_operator_command`** — copy the next-command string;
- **a single `evidence_paths` entry** — copy that one path (per-entry
  copy button);
- **raw JSON** — copy the raw status envelope.

Copy actions never compose multiple fields into one payload, never
decorate or shell-quote the value, and are never chained to a
terminal-open or terminal-run action. There is no "copy approval line"
action — the approval line is not an envelope field and is not composed
inside the panel.

## 11. What The Panel Does NOT Do

The panel is a read-only observer. It does **not**, by construction,
expose any of the following — as a button, command, keybinding,
context-menu item, setting, or any composed gesture:

- approve
- apply
- apply-bundle
- run
- approve-and-apply
- automatic approval
- automatic apply
- batch approval
- preapproval
- trust-this-workspace
- ignore STOP
- mute STOP
- dismiss STOP
- hide STOP
- workspace mutation

The panel's only contributed command is the read-only
`Claw Status: Refresh`. The only subprocess it spawns is
`claw plan status` with at most the two A2-L2d positional arguments and
no flags; an input that tries to direct it at a chain-write subcommand
is refused at construction time.

## 12. Security / Safety Model

The panel is a **read-only observer**, **not a workflow controller**,
**not an approval executor**, and **not an apply executor**.

- **No writes.** It writes nothing under `.claw/`, the workspace tree,
  the home directory, or VS Code workspace/global/secret storage. It
  keeps a single parsed envelope in memory for the panel session and
  persists nothing to disk.
- **No network.** No broker, model, Ollama, telemetry, analytics,
  error-reporting, or marketplace traffic at any phase. Static-grep
  guards (`npm run lint`) refuse forbidden networking and write APIs in
  the package source.
- **No secrets.** It reads no environment variables, shell history,
  terminal state, git credentials, or tokens; the envelope contains no
  secrets by A2-L2d construction, and the panel introduces no path that
  injects any.
- **Operator-gated.** Every status read requires an explicit operator
  gesture. There is no background activity.

The chain re-validates every input at apply time in the operator's own
terminal; the panel's rendering is informational and is never treated as
authoritative for a write decision.

## 13. Relationship To The Terminal Workflow

The panel **surfaces** the A2-L2b/L2c/L2d terminal workflow; it does not
replace it. The operator still:

1. runs the gated chain (`claw plan run --workspace-write-preview` →
   `approve` → `apply-bundle` → `apply`) in their own terminal, with TTY
   approval enforcement intact
   ([`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md));
2. uses the panel to *read* where the chain is, copy the next command,
   and click into evidence paths;
3. pastes `next_operator_command` into the terminal and runs it there.

The panel saves re-typing `claw plan status` and makes STOP conditions
harder to miss. It is the read-only counterpart to the machine-facing
[A2-L3 harness adapter](./a2-l3-harness-adapter-usage.md); both consume
`a2-l2d-status.v1`, and neither composes any write action.

## 14. Troubleshooting

- **"open a workspace folder before refreshing."** No workspace folder
  is open in VS Code. Open one and refresh again.
- **Subprocess error / `claw` not found.** The `claw` binary is not on
  `PATH`. Set `clawStatus.binaryPath` to its absolute path.
- **Panel shows a STOP banner.** This is expected behavior, not a panel
  bug — the envelope carries a STOP. Read the `stop_condition` literal
  and `evidence_paths`, and follow `next_operator_command`
  (`STOP — escalate`) in your terminal.
- **Exit code is not `0` or `12`.** Any other exit code is classified as
  a STOP in its own right; the raw stdout is preserved in the raw-JSON
  disclosure for escalation.
- **No way to pass an `<approval-result.json>`.** The current build
  invokes `claw plan status <workspace>` without the optional
  approval-result argument; an in-panel picker for it is not yet wired.
  To inspect approval-result-derived state, run
  `claw plan status <workspace> <approval-result.json>` directly in your
  terminal.

## 15. References

- [`docs/a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md)
  — behavioral scope card (responsibilities, forbidden actions, surface
  contract, STOP-visibility, evidence-path, refresh, and copy rules).
- [`docs/a2-l3-ide-adapter-implementation-scope-card.md`](./a2-l3-ide-adapter-implementation-scope-card.md)
  — implementation scope card (recommended shape, allowed/forbidden
  touched surfaces, CI/golden-test matrix).
- [`docs/a2-l3-ide-adapter-readiness-review.md`](./a2-l3-ide-adapter-readiness-review.md)
  — readiness review that authorized the first per-host lane.
- [`docs/a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — cross-adapter boundary and its five invariants.
- [`docs/a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)
  — `a2-l2d-status.v1` schema-of-record (authoritative on the envelope).
- [`docs/a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md)
  — `claw plan status` operator quick reference.
- [`ide/vscode/claw-status-panel/README.md`](../ide/vscode/claw-status-panel/README.md)
  — package-local build/test notes and file map.
