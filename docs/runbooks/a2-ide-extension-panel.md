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

## Tier 3 — Disposable Worktree Mutation (Foundation v0, read-only)

Tier 3 Foundation v0 is the **readiness/state/render layer** for the future disposable-worktree
mutation path (scope: `docs/a2-tier3-disposable-worktree-mutation-scope.md`). It adds **no** mutation
executor, **no** worktree-creation control, and **no** write button. It only makes the Tier 3 control
plane legible and honest.

Read-only sections (all status-only):

```text
[ Tier 3 Readiness ]              control-checkout-clean / origin-main / worktree-path-free /
                                  branch-name-free / operator-approved / plan-valid / declared-scope /
                                  denied-registry — honest tri-state; overall ready/not-ready.
[ Disposable Worktree Plan ]      intended worktree path + mutation branch + base (plan only; never created).
[ Declared Touched Files ]        the exact declared path set (empty in v0); mutation is limited to it.
[ Mutation Approval Gate ]        operator-approved? (no in v0); read-only until explicit per-lane approval.
[ Diff Summary ]                  placeholder — a diff would be computed in the disposable worktree before any apply.
[ Validation Results ]            placeholder — only explicitly-approved validation would run in the worktree.
[ Rollback / Abandon Worktree ]   rollback prefers abandoning the disposable worktree (never force-remove/force-delete).
[ Mutation Evidence Ledger ]      session-local, read-only; checkpoint/print steps marked printed-not-run.
```

Honesty + safety in v0:

```text
- No guard-safe Tier 3 probe is wired; control-checkout/origin/worktree/branch readiness renders
  not-checked (never fabricated green), and overall is not-ready by default.
- A dirty control checkout is a hard block, surfaced prominently.
- The safe-mutation policy is classification only: denials win over the Tier-3 allowlist, and writes
  are limited to the declared exact-path set inside the disposable worktree.
- No mutation lane is enabled. No file editing by the panel. No worktree creation. No mutation
  executor. No agent-run / agent-execute / apply / approve control. No live A2 / runtime / model /
  broker / :11434. No new spawn boundary, fs use, watcher, polling, or timer.
```

---

## Tier 3 Mutation Executor v0 (dry-run, read-only)

Tier 3 Mutation Executor v0 is the **plan / dry-run only** first lane of the executor (scope:
`docs/a2-tier3-mutation-executor-design-scope.md`). It adds **no** worktree creation, **no** writes,
and keeps the panel read-only — the executor is external and operator-invoked; the panel never spawns
it.

Read-only "Proposed Executor Plan" section:

```text
[ Proposed Executor Plan ]  Tier 3 Mutation Executor v0 — dry-run, read-only:
  - PRINTS the exact external dry-run command the operator would run (operator-run; printed only).
  - Renders the dry-run RESULT: would the lane proceed (readiness + plan + scope + approval),
    and per-step classification (each proposed write/command shown would-accept / would-reject).
  - Shows would-create-worktree: no and would-write-files: no (dry-run creates/writes nothing in v0).
  - Renders dry-run evidence (printed-not-run).
```

The dry-run model (`src/executorDryRun.ts`) is pure: given an operator-approved lane (objective +
worktree plan + declared exact-path set + proposed writes/commands), it reuses the Foundation v0
models — `tier3Readiness`, `disposableWorktreePlan`, `mutationScope` (exact-path / control-checkout
reject), `safeMutationPolicy` (denials win over the Tier-3 allowlist) — to classify what an external
executor WOULD do. It creates nothing and writes nothing.

What v0 does NOT do:

```text
- No worktree creation. No file write by the executor or the panel.
- No executor inside the panel; the panel never spawns the executor.
- No create / write / agent-run / agent-execute / apply / approve control.
- No live A2 / runtime / model / broker / :11434. No new spawn boundary, fs use, watcher, polling, timer.
- An actual write-capable executor step is a separate, explicitly-approved later lane.
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

---

## Tier 3 write-capable orchestrator v0 (`scripts/a2-tier3-write-orchestrator.sh`)

The panel stays read-only. The write-capable step lives OUTSIDE the panel as an operator-run
orchestrator that DRIVES the existing, tested `claw plan apply` / `a2-plan-runner` write-executor +
checkpoint chain — it does NOT duplicate the Rust write logic, and the panel never spawns it (the
panel's `helperRunner` allowlists exactly one basename, `a2-ide-harness.sh`).

Source of truth: `docs/a2-tier3-write-executor-reconciliation.md` (drive, don't duplicate) and the
revised DRAFT `handoffs/a2_tier3_mutation_executor_write_capable_implementation_prompt_DRAFT_2026-06-09.md`.

Two subcommands:

```text
validate-lane  --approved-lane <lane.json> --dry-run-evidence <evidence.json> [--plan <plan.yaml>]
   Pure gate check. No git, no claw, no worktree, no writes. Safe to run anywhere. Confirms the lane
   WOULD be drivable: operator-approved, dry-run-ready, exact-path scope, denials win over the Tier-3
   allowlist, worktree-plan rules (base origin/main, under /mnt/vast-data/git-worktrees/, never the
   control checkout, branch != main), and each plan step's write_target.path (the file actually
   written) inside the declared set (after_file, the byte source, must be workspace-relative).

apply-lane     --approved-lane <lane.json> --dry-run-evidence <evidence.json> --plan <plan.yaml>
   Runs validate-lane, then — only at a REAL interactive terminal (exit 7 off-TTY), with a clean
   control checkout, origin/main, and a free worktree path — creates exactly ONE disposable worktree
   from origin/main and drives the existing chain inside it (run -> approve -> apply-bundle -> apply).
   `claw plan apply` remains the only writer. Approval is human-typed at your terminal (never composed,
   captured, faked, or batched). After apply it prints checkpoint/apply-result evidence and a git diff
   summary, then STOPS for review. It never pushes, opens a PR, merges, deletes a branch, or
   force-removes a worktree; rollback is by ABANDONING the disposable worktree.
```

Gate-matrix test (offline; no claw, no writes):

```bash
bash tests/shell/test_a2_tier3_write_orchestrator.sh
```

Note: `scripts/a2-tier3-write-orchestrator.sh` and `tests/shell/test_a2_tier3_write_orchestrator.sh`
are outside the `rust-ci.yml` path filter, so CI does not run them automatically; run the test locally.
