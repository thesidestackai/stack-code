# A2 IDE Extension Panel — Operator Runbook (v1)

> v1 is a **visual VS Code / Cursor panel** that drives the print/validate-only A2 IDE harness v0
> ([`scripts/a2-ide-harness.sh`](../../scripts/a2-ide-harness.sh)). It does **not** run any A2 chain
> command and it does **not** weaken any safety gate. You still run preview / approval / apply
> yourself, with approval at a **real terminal**.

Scope source of truth: [`a2-l4-ide-extension-panel-scope.md`](../a2-l4-ide-extension-panel-scope.md).
Package: [`ide/vscode/a2-harness-panel/`](../../ide/vscode/a2-harness-panel/).

---

## What it gives you

A single panel with labeled sections and buttons, instead of separate command-palette tasks:

```text
[ Safety / Stop Gates ]   always-on banner of the invariants + STOP conditions
[ Workspace status ]      read-only setup status (helper, claw, plan, artifacts, verification)
[ Next safe step ]        read-only recommendation of the next safe action
[ Discovered (read-only) ]discovered plan/artifact paths, shown before use
[ Workspace / Plan / Artifact selection ]  set the paths the helper needs
[ Actions ]               one button per read-only/print helper subcommand
[ Helper output ]         the helper's stdout, verbatim, with a Copy button
[ Evidence timeline ]     read-only, session-local record of safe actions
```

Each **helper button** runs exactly one read-only/print subcommand through an argv-bounded wrapper and
shows its stdout. The `Show/Copy … Command` buttons display the command the helper printed so you can
copy it and run it yourself at a real terminal.

The panel never executes the A2 chain. It shows/copies commands; it does not run preview, approval,
apply-bundle, or apply.

### Workspace-first status (read-only)

On open — and whenever you click **Refresh Workspace Status** — the panel runs a single, read-only
inspection and fills in three sections without you typing a path:

- **Workspace status** — an honest tri-state for each setup dimension: `helper path`
  (found/missing/not-checked), `claw binary` (`configured`/`unknown` — the panel never verifies or runs
  claw), `workspace root`, `plan.yaml`, `target`, `after_sha`, `preview bundle`, `approval result`,
  `apply bundle`, and `final verification` (match/mismatch/not-checked). Status is never green-by-default.
- **Next safe step** — a read-only recommendation (e.g. `Validate Input`, `Print Preview Command`,
  `Verify Final Target`) derived from the helper-reported chain state. It only points you at an existing
  safe button; it never runs the chain.
- **Discovered (read-only)** — plan.yaml candidates (found via the editor's file index, not node `fs`)
  and `.claw` artifact paths (parsed from the helper's read-only `audit-workspace`). A field is
  auto-filled only when there is exactly one unambiguous candidate; otherwise you pick it. Every
  discovered path is shown before it is used.

How the detection stays safe: it runs only the allowlisted read-only helper subcommands (`help`,
`audit-workspace`) plus a one-shot editor file search. It spawns no `claw`, writes nothing, creates no
`.claw` artifact, and adds no filesystem watcher, polling, or timer — every refresh is one explicit
gesture.

---

## Open the panel

1. Build the package once (see below) or install it from source.
2. Command Palette → **A2 Harness: Open Panel**.
3. In the **Workspace / Plan / Artifact selection** section, use the field-setter controls to set the
   fields each action needs. These controls set fields only — they never run a chain command.
4. Click the action buttons in chain order.

### Field-setter controls (set fields only)

The selection section exposes one control per input field, shown next to the field table:

| Control | Field it sets |
| --- | --- |
| Select Workspace | workspace root (contains `.claw` + the target) |
| Select Plan | `plan.yaml` (after_file must be relative) |
| Select Target | the target file `plan apply` writes (for Verify Final) |
| Set After SHA | expected `after_sha256` of the target (for Verify Final) |
| Select Preview Bundle | `preview-bundle.json` (for Show/Copy Approval Command) |
| Select Generator Result | `preview-generator-result.json` (for Show/Copy Apply-Bundle Command) |
| Select Approval Result | persisted `approval-result.json` (for Show/Copy Apply-Bundle Command) |
| Set Approval Output | path to write the new `approval-result.json` (for Show/Copy Approval Command) |
| Select Apply Bundle | `apply-bundle.json` (for Show/Copy Apply Command) |

Each control only stores a path/hash in the panel session; nothing is executed. Verify Final, Show/Copy
Approval, and Show/Copy Apply stay blocked (with a notice) until their fields are set — set them here.

| Button | Helper subcommand | Runs an A2 command? |
| --- | --- | --- |
| Validate Input | `validate-input` | No |
| Audit Workspace | `audit-workspace` | No (read-only artifact/hash audit) |
| Find Artifacts | `find-artifacts` | No |
| Show/Copy Preview Command | `print-preview` | No (prints only) |
| Show/Copy Approval Command | `print-approval` | No (prints only; REAL-terminal note) |
| Show/Copy Apply-Bundle Command | `print-apply-bundle` | No (prints generator command) |
| Show/Copy Apply Command | `print-apply` | No (prints executor command) |
| Verify Final Target | `verify-final` | No (read-only hash check) |
| Refresh Workspace Status | `help` + `audit-workspace` | No (read-only status/discovery refresh) |
| Open Runbook | — | No (opens this/the v0 runbook) |
| Export Evidence Summary | — | No (opens an unsaved summary doc incl. status + timeline) |

There is intentionally **no** Run Preview / Run Approval / Run Apply-Bundle / Run Apply button.

---

## The chain you still run yourself

```text
1. PREVIEW   claw plan run <plan.yaml> --workspace-root <ws> --workspace-write-preview
2. APPROVE   claw plan approve <preview-bundle.json> --approval-result-output <out.json>
             (REAL terminal; at the prompt type:  apply <step-id> <preview_sha256>)
3. BUNDLE    claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
4. APPLY     claw plan apply <apply-bundle.json>
```

Copy each command from its `Show/Copy …` button and run it at a real terminal. Approval must be typed
by you at a real TTY — the panel never composes the approval line and never captures it.

---

## Safety rules this panel preserves

```text
- The panel spawns ONLY the helper, with a read-only/print subcommand. It never spawns `claw`.
- Preview/approval/apply-bundle write no target; only `claw plan apply` writes, once, run by you.
- No auto-approval. No hidden apply. No batch/--yes/fake-TTY.
- No model / broker / runtime / :11434 call. No secrets. No `fs` access from the panel.
- No filesystem watcher, no polling, no auto-refresh — every action is one explicit gesture.
```

---

## A2 Local Coding Agent Foundation v0 (read-only control plane)

Foundation v0 is a **state/control-plane foundation** layered on the read-only panel. It is the
first step toward a true local coding-agent cockpit (Claude Code / Codex / Cursor-like workflow
feel) **with Stack-Code permission tiers, evidence, and operator control**. It adds **no**
autonomous editing, **no** live A2 chain execution, and **no** PR packaging.

What Foundation v0 adds — all read-only, all status-only (no new buttons):

```text
[ A2 Local Coding Agent Foundation v0 ]
  [ Permission Tier ]            current effective tier (read-only) + the full Tier 0–5 model
  [ Agent Readiness ]            honest tri-state: workspace, repo/git, dirty/staged/untracked,
                                 current tier, denied-registry loaded, safe-executor mode
  [ Denied Command Registry ]    command families denied globally (denials win over allowlists)
  [ Agent Evidence Ledger ]      session-local, read-only record; print-only steps marked printed-not-run
  [ Proposed Next Agent Lane ]   what comes next, and why no mutation is enabled in v0
```

Permission tiers (described; only Tier 0–2 are reachable as the effective tier in v0):

```text
Tier 0 — Observe Only
Tier 1 — Print Commands Only          (default effective tier)
Tier 2 — Safe Read-Only Execution     (effective only after a read-only helper call)
Tier 3 — Disposable Worktree Mutation (requires explicit future approval; not enabled in v0)
Tier 4 — PR Packaging                 (requires explicit future approval; not enabled in v0)
Tier 5 — Runtime / Model / Service    (denied by default; external to this cockpit)
```

Agent readiness honesty: the panel has **no guard-safe git probe in v0** (panel source forbids
`fs`, process spawn, watchers, and timers). Git/dirty state is rendered as `not-checked` with a
stated reason — never fabricated green, and never a false all-clear. A future, separately approved
lane may supply guard-safe git facts (e.g. via the read-only VS Code Git API) to the same pure
readiness model.

What Foundation v0 does NOT do:

```text
- No mutation lane is enabled. No file editing by the panel.
- No autonomous source edits.
- No PR creation / branch deletion by the panel.
- No live A2 chain execution (preview / approval / apply-bundle / apply).
- No model / broker / runtime / service calls. No raw :11434 inference. No secret reads.
- No hidden command execution; no watcher / polling / timer; no new spawn boundary.
```

---

## Build and test (from source)

```bash
cd ide/vscode/a2-harness-panel
npm install --ignore-scripts
npm run lint     # static guards
npm run compile  # tsc -p .
npm test         # mocha unit tests
```

The package is not packaged as a `.vsix` in this lane.
