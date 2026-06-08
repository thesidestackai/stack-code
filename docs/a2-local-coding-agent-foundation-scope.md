# A2 Local Coding Agent Foundation Scope

> Docs-only strategic foundation. This document defines the next safe implementation
> layer toward a true local coding agent for Stack-Code. It changes no source, runs
> no A2 chain command, and makes no model / broker / runtime call. It is a plan, not
> an implementation.

---

## 1. Executive Summary

The next strategic move is **not full autonomy**. It is a permissioned local agent
control plane that can eventually support agentic edit/test/PR loops while preserving
clean-worktree, exact-scope, evidence-first safety.

Today the A2 IDE Extension Panel is a proven **read-only / print-validate operator
cockpit**: it opens a workspace, computes an honest setup status, discovers plan and
`.claw` artifacts, recommends exactly one safe next step, and prints (never runs) the
A2 chain commands the operator types at a real terminal. It is structurally prevented
— by static guards and unit tests — from spawning `claw`, touching the filesystem,
calling a model/broker, or recommending a chain executor.

That same structural discipline is the asset we build on. The path to a **true local
coding agent** (comparable in workflow feel to Claude Code / Codex / Cursor, but with
Stack-Code / SideStackAI-grade safety controls) is to add a **permission tier model**,
an **agent session model**, a **safe executor model**, and the **IDE cockpit surfaces**
that make agent readiness, proposed scope, and evidence legible — *before* we grant any
new mutation capability. The recommended next implementation, **A2 Local Coding Agent
Foundation v0**, adds only the foundation UI/state model. It introduces no new mutation
capability and keeps the panel honest about what it has and has not verified.

---

## 2. North Star

```text
Open a repo in VS Code.
The panel understands the workspace.
The operator gives an objective.
The agent proposes a scoped plan.
The agent creates or uses an isolated worktree.
The agent edits only approved files.
The agent runs approved tests/builds.
The panel shows diffs/evidence.
The agent packages a PR.
The operator remains in control at each mutation boundary.
```

The north star is a **local coding agent** whose autonomy is bounded by an explicit,
visible permission tier at every step. The operator remains in control at each mutation
boundary: nothing is edited, executed, or packaged without the operator seeing the
proposed scope, the exact commands, and the resulting evidence first. There is no hidden
command execution and no autonomous source edits without an explicit, tiered, operator-
visible grant.

---

## 3. Current Proven Capabilities

Merged and evidenced on `origin/main`:

```text
PR #101  feat(a2): add workspace-first panel UX           affedf999ef69d26ad8b32c4a22d6357f4a08e2b
PR #102  docs(a2): record workspace-first UX smoke evid.  35da6a11dc2bc16182749e0bbfd241d03764d2d7
```

Package and surfaces (verified read-only in this lane):

- **Panel package** — `ide/vscode/a2-harness-panel/` (TypeScript). Source modules:
  `extension.ts`, `panel.ts`, `render.ts`, `discovery.ts`, `setupStatus.ts`,
  `stateMachine.ts`, `helperRunner.ts`, `buttons.ts`, `evidence.ts`, `clipboard.ts`.
  Matching unit tests under `test/` (the workspace-first smoke evidence records
  `113 passing`).
- **Helper** — `scripts/a2-ide-harness.sh` (print/validate only, ~497 lines). The only
  binary the panel spawns. Read-only / print subcommands: `help`, `validate-input`,
  `audit-workspace`, `find-artifacts`, `print-preview`, `print-approval`,
  `print-apply-bundle`, `print-apply`, `verify-final`. It never executes `claw`.
- **Single spawn boundary** — only `src/helperRunner.ts` may spawn a process; it spawns
  only the helper (basename `a2-ide-harness.sh`), with no `exec`/`eval`, no sync spawn,
  no `shell:true`.
- **Static guards** — `scripts/run-guards.js` (mirrored by `test/guards.test.ts`) fail
  the build on any: network/telemetry/broker/`ollama`/`:11434` egress; filesystem
  watcher / polling / background refresh; any use of `fs`; secret-storage API; chain-
  write command literal in live code; approval-line composition; or a process spawn
  outside the helper runner.
- **Honest status model** — `setupStatus.ts` computes a tri-state per dimension
  (`found` / `missing` / `not-checked`, etc.); status is never green-by-default. The
  panel reports `claw` as `configured` (a path parsed from helper usage) or `unknown`,
  never `found`, because it cannot prove the binary exists without forbidden `fs`/spawn.
- **Read-only next-step machine** — `stateMachine.ts` maps setup + helper-reported
  chain state to exactly one safe next step (`Print*` / `Validate` / `Select` / `Verify`).
  `assertSafe` (exercised for every state) throws if a step is ever a chain executor.
- **Read-only discovery** — `discovery.ts` is a pure parser of helper stdout; it
  auto-fills a field only when there is exactly one unambiguous candidate, otherwise the
  operator selects. Every discovered path is shown before use.
- **Session-local evidence timeline** — `evidence.ts` records safe actions in order with
  exit codes; print steps are recorded as **printed-not-run**. Export opens an unsaved
  untitled document; the panel writes no file.
- **Runbook** — `docs/runbooks/a2-ide-extension-panel.md`.

Operator-usable flow today: **open workspace → inspect setup status → discover
plan/artifacts → show next safe step → print/validate commands → export evidence.**

What is *not* yet proved (from PR #102 evidence): a full artifact-backed **live**
preview/approval/apply GUI chain. That remains a separate, explicitly token-gated lane.

---

## 4. Gaps to True Local Coding Agent

Based on current repo evidence and operational requirements — not external marketing.

```text
Capability                         Current State                  Target State                          Gap                                   Risk
Workspace open/readiness           workspace root detected;        workspace + repo root + git +          add repo/git/branch/worktree          low
                                   helper/plan/artifacts probed    branch + worktree + dirty state        readiness detection                   (read-only)
                                   (read-only)                     understood automatically
Repo/git awareness                 none (no git probe; fs/spawn    detect branch, worktree, dirty/        new read-only git state surface       low-med
                                   forbidden in panel)             staged/untracked, ahead/behind         (VS Code Git API or new helper sub)   (must stay read-only)
Plan generation                    operator supplies plan.yaml;    agent proposes a scoped plan from      objective input + plan proposal       high
                                   panel discovers/validates it    an objective                           (model-backed) — deferred             (autonomy boundary)
File editing                       none (no fs; guards forbid)     scoped edits in a disposable           tiered mutation in isolated           high
                                                                   worktree, approved file set only       worktree only                         (mutation boundary)
Command execution                  read-only/print helper subs     tiered safe executor with             allowlist-by-tier executor +          high
                                   only; no chain exec             allowlist + denied registry            structured evidence                   (exec boundary)
Test/build validation             none                            run detected test/build under          command detection + tiered run        med
                                                                   Tier 2/Tier 3                          surface
Diff review                        target-hash verify only         show proposed diffs before any         diff summary surface                  med
                                                                   apply; no hidden command execution
Evidence ledger                    session-local timeline,         structured agent evidence ledger       ledger schema + render + persistence  med
                                   printed-not-run, not persisted  (session id, objective, tier, steps)   decision
PR packaging                       none                            package a PR after Tier 4 grant        PR-packaging tier (deferred)          high
Rollback/checkpoint               disposable worktree (manual)    explicit checkpoint before mutation;   checkpoint/restore model              med
                                                                   non-destructive restore
Runtime/model/service guardrails  total deny (guards block all)   same deny by default; Tier 5 only      preserve denies while adding agency   high if relaxed
                                                                   via explicit token-gated lane
Permission model                  implicit (hardcoded read-only)  explicit Tier 0–5 model, displayed     permission tier model + display       foundational
Operator approval UX              real-terminal typed approval    per-mutation operator approval at      tiered approval surface; operator     foundational
                                   for the chain                  each boundary                          remains in control
```

---

## 5. Safety Principles

```text
1. Clean worktree before mutation. No mutation against a dirty checkout.
2. One lane = one worktree = one branch = one PR. Mutation happens in an isolated,
   disposable worktree, never the control checkout.
3. Exact-path scoping. The agent touches only the approved file set; no broad globs.
4. Deny by default. Destructive, runtime, model, broker, and service actions are denied
   unless an explicit, tiered, token-gated grant is present.
5. Evidence first. Every command produces structured evidence; print steps are recorded
   as printed-not-run; nothing is claimed without evidence.
6. No hidden command execution. Commands are shown before they run; no auto-run on open;
   no background watcher/polling/timer.
7. No autonomous source edits outside a disposable worktree under an explicit grant.
8. Honest status. Never green-by-default; report not-checked when not verified.
9. Operator remains in control at every mutation boundary.
10. No raw :11434 app inference. No model / broker / runtime / secret access from the
    panel; the single spawn boundary stays the only process surface.
```

---

## 6. Permission Tier Model

Each tier is strictly additive over the one below it. The current panel sits at
**Tier 0–2** (observe, print, safe read-only helper subcommands). Tiers 3–5 are the
target capability and are *defined* here but **not** granted by the v0 foundation.

### Tier 0 — Observe Only

```text
allowed actions   : render setup/readiness status; show discovered paths; show the
                    current permission tier; render the evidence ledger.
denied actions    : any process spawn; any file read/write; any command execution.
required gates    : none (default state).
evidence required : ledger entry for each observation gesture (read-only).
```

### Tier 1 — Print Commands Only

```text
allowed actions   : print the exact command the operator would run (preview/approval/
                    apply-bundle/apply), copy-to-clipboard; print proposed scoped plan
                    text for human review.
denied actions    : executing any printed command; spawning claw; mutation.
required gates    : Tier 0 satisfied.
evidence required : ledger entry recorded as printed-not-run (command text captured).
```

### Tier 2 — Safe Read-Only Execution

```text
allowed actions   : run allowlisted read-only/print helper subcommands (validate-input,
                    audit-workspace, find-artifacts, verify-final, help) through the
                    single spawn boundary; read-only repo/git status probe.
denied actions    : any command that writes a target, .claw artifact, or repo file;
                    any model/broker/runtime call; any :11434 inference.
required gates    : Tiers 0–1 satisfied; helper path resolved; argv-bounded wrapper.
evidence required : ledger entry with subcommand, argv, exit code (read-only).
```

### Tier 3 — Disposable Worktree Mutation

```text
allowed actions   : within an isolated, disposable worktree only: scoped file edits to
                    the approved file set; allowlisted local build/test commands.
denied actions    : mutation in the control checkout; touching unapproved paths;
                    destructive commands (see §8); runtime/model/service actions.
required gates    : clean control checkout verified; isolated worktree created from
                    origin/main; explicit operator approval of the exact file set and
                    commands; exact-path scoping enforced.
evidence required : checkpoint before mutation; per-command structured evidence; diff
                    summary of the proposed change before any apply.
```

### Tier 4 — PR Packaging

```text
allowed actions   : stage exact approved paths in the disposable worktree; compose a
                    commit; open a PR for operator review.
denied actions    : merging; force-push; force-deleting a branch; history rewrite;
                    packaging from a dirty or ambiguous checkout.
required gates    : Tier 3 satisfied; tests/build green with evidence; diff reviewed;
                    explicit operator approval to package.
evidence required : commit summary; PR link; full diff and test/build evidence in the
                    ledger.
```

### Tier 5 — Runtime / Model / Service Actions

```text
allowed actions   : NONE by default. Model load/unload, service start/stop/restart,
                    broker calls, and live :11434 inference are out of scope for the
                    agent cockpit.
denied actions    : all of the above, unless a dedicated, explicitly token-gated lane
                    is opened outside this cockpit.
required gates    : explicit token-gated lane; separate operator authorization; not
                    reachable from the panel.
evidence required : N/A in this cockpit — Tier 5 is intentionally external.
```

---

## 7. Proposed Agent Session Model

A single in-memory, session-local manifest the panel renders and the ledger references.
It is the spine of the agent cockpit and introduces no new capability by itself.

```text
agent session
  session id          : stable id for this cockpit session (no PII, no secret).
  objective           : the operator-stated goal (free text; advisory until a tier
                        grant exists; the v0 foundation captures and displays it only).
  workspace root      : detected repo/workspace root.
  source repo         : origin/remote identity (read-only).
  target branch       : the branch the eventual PR would target.
  target worktree     : the isolated, disposable worktree path (when one exists).
  touched surfaces    : the proposed/approved exact file set (empty until approved).
  allowed command tier: current permission tier (Tier 0–5); defaults to the highest
                        tier currently granted (read-only by default).
  evidence ledger     : ordered, structured record (see §8) of every gesture/command.
```

Constraints: the session manifest holds **no secrets**, performs **no IO** of its own,
and is **not persisted** by the v0 foundation (persistence is an explicit later
decision). Tier defaults to read-only; raising it requires an explicit grant.

---

## 8. Proposed Safe Executor Model

The safe executor is the gate between an intended command and its execution. The v0
foundation defines the **model and the registries** (allowlist by tier, denied
registry) and renders them; it does **not** add a new execution capability.

```text
allowlist by tier   : commands are permitted only if they appear in the allowlist for
                      the currently granted tier (e.g. read-only helper subcommands at
                      Tier 2; scoped local build/test at Tier 3).
denied command       : a global denied registry blocks destructive and out-of-scope
                      commands regardless of tier. Denials always win over allowlists.
deny destructive     : globally deny destructive families — recursive force file
                      removal (the `rm` recursive+force form), the git working-tree
                      clean operation, `find` used with `-delete` or with an `-exec`
                      removal, `git reset --hard`, `git add .` / `git add -A`, branch
                      force-delete (`-D`), worktree removal using the force flag, and
                      `git fetch --prune`.
deny runtime/model   : globally deny runtime/model/broker/service actions by default
                      (Tier 5 only, external).
deny raw inference   : globally deny raw app inference to :11434.
require clean worktree: mutation requires a verified-clean control checkout and an
                      isolated worktree.
require exact scope  : mutation is limited to the approved exact file set; no broad
                      globs.
structured evidence  : every command (allowed or denied) produces a structured evidence
                      record: tier, command, argv, decision (allowed/denied + reason),
                      exit code, and a printed-not-run marker for print-only steps.
```

The executor must preserve the existing structural invariants: a single spawn boundary,
no `fs` outside that boundary, no network/broker/secret egress, and no chain-write
literal in live code.

---

## 9. Proposed IDE Cockpit Evolution

New, read-only-by-default panel surfaces layered above today's sections:

```text
[ Agent Readiness ]        repo root, git state, branch/worktree, dirty/staged/untracked
                           warning, helper/claw readiness — honest tri-state, never
                           green-by-default.
[ Permission Tier ]        the current tier (Tier 0–5) and what it allows/denies, always
                           visible; raising a tier is an explicit, separate gesture.
[ Proposed Next Agent Lane ] the objective, proposed touched surfaces, and the commands
                           that would run — shown before any execution.
[ Diff Summary ]           (future tiers) the proposed change before any apply.
[ Evidence Ledger ]        structured, session-local record of every gesture/command,
                           with printed-not-run markers and exit codes.
```

Carried-forward invariants: no Run-the-chain buttons; no hidden command execution; no
auto-run on open; no background watcher/polling/timer; every action is one explicit
operator gesture; dirty-checkout warnings are surfaced prominently.

---

## 10. MVP Implementation Recommendation

Recommend the next real implementation as:

```text
A2 Local Coding Agent Foundation v0
```

It should implement or scaffold **only the foundation UI/state model** — no new
mutation, no live apply, no runtime/model/service control:

```text
- agent session manifest (in-memory, session-local; §7 shape)
- permission tier display (Tier 0–5; current tier always visible)
- repo/git readiness detector (read-only; via VS Code Git API or a new read-only
  helper subcommand — implementer must pick the approach that preserves the existing
  guards: no fs/spawn outside the single boundary)
- dirty worktree detector + prominent dirty-checkout warning
- safe command allowlist model (allowlist-by-tier; labels only at v0)
- denied command registry (global denied registry; §8)
- agent evidence ledger shape (structured schema + render; printed-not-run markers)
- panel section for Agent Readiness
- panel section for Proposed Next Agent Lane
- tests for the permission tier model and the denied command registry
- no source mutation yet unless in a disposable worktree and SEPARATELY approved
```

The git readiness probe is the one design decision the implementer must resolve while
preserving guards: prefer the read-only VS Code Git extension API (no `fs`, no spawn,
no network), or add a new read-only `git-status`-style helper subcommand behind the
existing single spawn boundary. Until wired, the detector must render `not-checked`
honestly rather than fabricate state.

---

## 11. Files Likely Touched by the Next Implementation

Conservative and to be **verified by the implementer first** (names may differ):

```text
ide/vscode/a2-harness-panel/src/        new state/model modules + render additions
ide/vscode/a2-harness-panel/test/       unit tests for tiers + denied registry
docs/runbooks/a2-ide-extension-panel.md runbook additions for the new sections
handoffs/a2_local_coding_agent_foundation_v0_implementation_report_2026-06-08.md
```

Possibly (only if the git probe uses a helper subcommand, and only if separately
approved): `scripts/a2-ide-harness.sh` (a new read-only `git-status` subcommand). The
implementer must confirm actual filenames and the guard impact before touching scripts.

---

## 12. Explicit Non-Goals

```text
No autonomous source edits in this scope doc lane.
No live apply.
No runtime/model/service control.
No raw :11434 app inference.
No hidden command execution.
No approval phrase generation/capture.
No broad cleanup.
No replacing Claude Code/Codex/Cursor.
```

The foundation is a control plane, not an autonomous agent. It adds legibility and a
permission model; it does not add a single new mutation capability.

---

## 13. STOP Gates

```text
STOP if the control checkout is dirty before creating a worktree.
STOP if the target worktree path or branch already exists.
STOP if PR #101 or PR #102 is missing from origin/main.
STOP if any implementation lane attempts source/test/helper mutation outside an
  approved disposable worktree.
STOP if any change would add network/broker/model/runtime/secret access or a :11434
  call to the panel.
STOP if any change would add a Run-the-chain button, auto-run, watcher, polling, or a
  background timer.
STOP if the static guards or unit tests fail.
STOP if changed files differ from the approved scope.
```

---

## 14. Validation Requirements

```text
- changed-file scope: exactly the approved files for the lane, in scope order.
- required content scan: the strategic vocabulary (local coding agent, north star,
  permission tier, Tier 0–5, agent session, safe executor, denied command, evidence
  ledger, dirty worktree, isolated worktree, operator remains in control) is present.
- unsafe overreach scan: no autonomy-bypass / hidden-execution / destructive / runtime-
  control language is present.
- git diff --check: clean (no whitespace errors / conflict markers).
- for any future code lane: static guards PASS and unit tests green, with evidence.
```

---

## 15. Recommended Next Lane

```text
Name        : A2 Local Coding Agent Foundation Scope Review / Push PR
Type        : docs-only
Objective   : review this scope package and its implementation prompt, then push the
              branch and open a PR for operator review.
Tool        : Claude Code (docs-only lane) + operator review.
Why         : the scope package must be reviewed (or the operator must explicitly opt to
              skip docs review) before any implementation lane begins.
Touched     : none beyond this branch's two docs files.
Mutation    : none (docs-only).
STOP gate   : do not begin A2 Local Coding Agent Foundation v0 implementation until this
              scope package is reviewed or the operator explicitly asks to skip review.
```

After review, the implementation lane is driven by
`handoffs/a2_local_coding_agent_foundation_implementation_prompt_DRAFT_2026-06-08.md`.
