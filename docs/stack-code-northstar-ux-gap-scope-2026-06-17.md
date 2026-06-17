# Stack-Code — Northstar UX Gap Scope — 2026-06-17

> **Docs-only scope.** This document designs the next product layer for Stack-Code. It implements
> nothing, edits no scripts / Rust / IDE-panel source / tests / CI / runtime / services / HQ, runs no
> `apply` / `package-plan` / `package-commit` / `package-push` / `package-pr`, opens no PR, calls no
> model / broker / `/status/vram` / `/v1/chat/completions`, touches no Vault/secret, and introduces no
> raw `:11434` app inference. It is the source-of-truth scope a separately-approved implementation lane
> would build to. It does not authorize implementation.

---

## 1. Executive Summary

Stack-Code has proven the **isolated mutation → package → draft PR** backend, end-to-end, live, against
real GitHub. The backend execution/packaging ladder is real and reversible.

**Stack-Code has not yet reached the Northstar UX.** The next workstream is to turn the proven backbone
into an operator-friendly **workspace experience** — to make the daily interface feel closer to Codex /
Claude Code / Cursor — without weakening a single safety gate.

```text
The execution ladder is proven.
The UX is not yet Northstar.
The current experience is still prompt-heavy, terminal-heavy, and operator-orchestrated.
Stack-Code is ready for controlled internal use, but not yet a polished
Codex / Claude Code / Cursor-like daily interface.
```

The central insight from discovery (§6) is the same one that powered the workspace-first panel work:
most of the Northstar layer is a **presentation + read-only detection + gated-control** layer over
machinery that already exists. The packaging ladder, the read-only chain-state detection, the
print/validate-only panel, and the 13-state next-step machine are all already built and tested. What is
missing is the **product surface** that joins them into one guided, workspace-first flow — and a small
set of **explicitly-gated control wires** that let the operator drive the proven ladder from inside the
product instead of from a separate terminal.

---

## 2. Current Proven Backbone

The backend execution/packaging ladder is proven end-to-end:

```text
real-TTY apply              real, human-typed approval in a real terminal; only the declared file changes
package-plan                read-only; validates worktree/branch/lane + on-disk hash before any write
package-commit              commits exactly the declared change on the disposable branch
package-push                pushes the disposable branch (non-force) to origin
package-pr                  opens exactly ONE real GitHub DRAFT PR (base main, head disposable branch)
real GitHub draft PR        independently confirmed isDraft=true, mergedAt=null, reviewDecision=""
idempotency                 confirmed — re-running package-pr opens no duplicate PR
evidence closeout           frozen, docs-only, on main
```

Proven via:

```text
Lane A fixture     real-TTY apply (SMOKE_NOTES.md) -> package-plan -> package-commit -> package-push
Lane B live smoke  package-pr -> opened exactly ONE real GitHub DRAFT PR, idempotent on re-run
PR #142            Stage 4 package-pr implementation (open draft PR)
PR #143            tier-4 live-smoke readiness scope
PR #144            prerequisite fixture plan + DRAFT build prompt
PR #146            Lane B closeout / evidence freeze (merged on main)
PR #145            the single live-smoke DRAFT PR; mechanism proof, not content for main; never merged
```

Implementation locations (read-only discovery, §6):

```text
scripts/a2-tier3-write-orchestrator.sh    cmd_package_plan / cmd_package_commit / cmd_package_push / cmd_package_pr
scripts/a2-ide-harness.sh                 print/validate-only chain helper (the only binary the panel spawns)
ide/vscode/a2-harness-panel/              print/validate-only VS Code / Cursor panel (no Run-* buttons)
ide/vscode/a2-harness-panel/src/stateMachine.ts   read-only 13-state next-step machine
```

---

## 3. Honest Product Status

```text
Ready for controlled internal use.
Not yet equivalent to a polished Codex / Claude Code / Cursor-like experience.
Still too prompt-heavy and terminal-heavy.
Too many decisions happen outside the product UI.
```

Specifically, today:

- The proven Tier-4 ladder (`package-plan/commit/push/pr`) runs **in a terminal** via the orchestrator
  shell script, **separate** from the IDE panel. The panel is print/validate-only and has **no `Run-*`
  buttons** — it copies commands for the operator to paste.
- The operator must already know artifact names, lane JSON paths, worktree conventions, and the chain
  sequence. Setup is manual.
- "Describe a task and get a safe plan" is not yet a product surface — planning today is prompt-driven
  and operator-orchestrated.
- Evidence exists as frozen handoff documents, not as a live in-product timeline.
- Cleanup / disposition is a human decision narrated in closeout docs, not a guided checklist.

This is a strong, safe, **internally usable** system. It is **not** a finished agent UX. Do not frame
it as one.

---

## 4. Northstar Experience Definition

The target daily experience:

```text
Open workspace
Describe task
System scopes a safe plan
System creates an isolated worktree
System shows preview / diff / evidence
Operator approves the risky write in a real TTY or an explicit safe UI gate
System packages the commit
System pushes the disposable branch
System opens a draft PR
System freezes the evidence
Operator decides: close / retain / human merge
```

Mapped to the proven backbone:

```text
open workspace        -> workspace detection (read-only)
describe task         -> task intake (new surface)
agent scopes plan     -> plan generation -> plan validation (offline schema validator exists)
clean worktree        -> disposable worktree creation (gated; plan module exists)
preview/diff shown    -> preview rendering + diff review (read-only)
tests/guards run      -> test/guard execution (read-only, reportable)
human approves        -> real-TTY apply approval OR explicit safe UI approval gate (human-gated)
package-commit        -> package-commit (gated)
package-push          -> package-push (gated)
draft PR              -> package-pr -> draft PR card (gated)
evidence              -> evidence timeline (read-only, frozen on completion)
cleanup/disposition   -> disposition checklist (gated; merge is human-only)
```

The Northstar is **not** "more automation of the risky steps." It is **a guided product surface around
the proven ladder**, with risky steps still gated and merge still human-only.

---

## 5. UX Gap Matrix

| Capability | Current state | Northstar target | Can automate safely? | Must stay gated? | Evidence required | First implementation lane |
|---|---|---|---|---|---|---|
| workspace detection | manual field-set; no auto-detect on open | auto-detect on workspace open; status card | Yes (read-only) | No | workspace path, branch, clean/dirty | N2 |
| task intake | none (prompt-driven, out of product) | in-product task box | Yes (capture only) | No (capture is read-only) | task text, timestamp | N3 |
| plan generation | offline/manual; planner invoked by operator | one-click "scope plan" producing a plan artifact | Partial (plan draft is read-only output) | No (read-only draft) | plan artifact, model/source note | N3 |
| safe target selection | manual field entry | guided selector from discovered artifacts | Yes (read-only discovery) | No | selected target, allowed-paths check | N3 |
| worktree creation | manual / orchestrator | one-click create disposable worktree from origin/main | Partial (creation is gated) | Yes | worktree path, base sha, collision check | N5 |
| preview rendering | helper prints; panel renders stdout | rendered preview surface | Yes (read-only) | No | preview bundle, generator result | N4 |
| diff review | raw stdout text | structured diff viewer | Yes (read-only) | No | diff, changed-file list | N4 |
| real apply approval | real-TTY, human-typed | real-TTY OR explicit safe UI gate; never hidden | No | Yes | approval result/output, real-TTY note | N5 |
| package-plan | terminal orchestrator | one-click after checks | Yes (read-only validation) | No (it is read-only) | plan validation, hash check | N5 |
| package-commit | terminal orchestrator | gated control | No | Yes | commit sha, declared-file-only proof | N5 |
| package-push | terminal orchestrator | gated control | No | Yes | remote sha, non-force proof | N5 |
| package-pr | terminal orchestrator | gated control | No | Yes | PR url, isDraft=true, idempotency | N6 |
| draft PR verification | manual `gh pr view` | draft PR card with independent verification | Yes (read-only display) | No | PR state, isDraft, mergedAt, reviewDecision | N6 |
| idempotency | proven; manual re-check | surfaced on the PR card | Yes (read-only display) | No | idempotent_existing flag | N6 |
| evidence timeline | frozen handoff docs | live in-product timeline | Yes (read-only) | No | per-step evidence records | N6 |
| cleanup/disposition | human decision in closeout doc | guided disposition checklist | Partial (inventory yes; teardown gated) | Yes (teardown) | inventory, operator decision | N7 |
| Stage 5 human merge | not started; out of product | surfaced as human-only decision, never auto | No | Human-only | merge decision record | (none — human-only) |

---

## 6. Operator Journey Today

```text
1. Operator opens a terminal (not the product).
2. Operator hand-creates or reuses a worktree, knowing the naming convention.
3. Operator hand-writes / discovers a plan and lane JSON, knowing artifact names.
4. Operator runs the print/validate-only helper or opens the panel and sets every field by hand.
5. Operator reads raw helper stdout to understand chain position.
6. Operator runs a real-TTY apply, typing the approval by hand.
7. Operator runs package-plan, package-commit, package-push in the terminal orchestrator.
8. Operator runs package-pr in the terminal; one DRAFT PR opens.
9. Operator verifies the PR with `gh pr view`.
10. Operator writes / freezes evidence in a handoff doc.
11. Operator decides disposition (close / retain / merge) and runs a separate cleanup lane by hand.
```

Discovery confirms the machinery for steps 2–9 already exists and is tested; the burden is that the
operator is the orchestrator and the terminal is the interface.

---

## 7. Operator Journey Northstar

```text
1. Operator opens the workspace; the product shows a workspace status card automatically.
2. Operator types a task in an intake box.
3. The product scopes a safe plan and shows it as a plan preview.
4. The product proposes an isolated worktree from origin/main; operator approves creation (gated).
5. The product renders preview / diff / evidence for the planned change.
6. The product runs read-only tests/guards and shows results.
7. Operator approves the risky write in a real TTY or an explicit safe UI gate (human-gated).
8. The product packages the commit (gated), pushes the disposable branch (gated).
9. The product opens exactly one DRAFT PR (gated) and shows a draft PR card with independent verification.
10. The product freezes an evidence timeline.
11. The product shows a disposition checklist: close / retain / human merge — merge stays human-only.
```

Every risky transition in this journey is still an explicit human gate. No step auto-applies, auto-pushes,
auto-opens-ready, or auto-merges.

---

## 8. Safe Automation Candidates

Read-only or post-check one-click actions that can be automated safely (they mutate nothing irreversible):

```text
workspace status detection
origin/main freshness check
worktree collision check
plan linting (offline schema validation)
artifact discovery
preview rendering
package-plan                 (read-only: validates worktree/branch/lane + on-disk hash, writes nothing)
test/guard execution         (read-only)
evidence collection
draft PR status display       (read-only gh pr view)
cleanup inventory             (read-only listing of worktrees/branches/scratch)
```

These are safe to make one-click **after** their checks pass, because none performs an irreversible
outward mutation.

---

## 9. Human-Gated Actions

Actions that require an explicit per-action operator gesture inside the product (no auto-trigger,
no batch, no "remember my choice"):

```text
real apply approval           (real-TTY or explicit safe UI gate; never hidden)
package-commit
package-push
package-pr
PR close
remote branch deletion
fixture teardown
any runtime / service target
```

Each must show what it will do, require an explicit confirm, and produce evidence.

---

## 10. Human-Only Actions

Actions the product must **never** perform automatically, even with a gate — they remain human, outside
any auto path:

```text
PR merge
PR approval
mark-ready
force delete
force push
destructive cleanup
Vault / secrets changes
runtime / service restart
model / broker changes
```

The state machine (§12) must make these terminal/manual: there is no transition that reaches "merged"
or "ready" without a human acting outside the product's automation.

---

## 11. Required UX Surfaces

The future product surface should include:

```text
workspace status card        path, branch, clean/dirty, origin/main freshness
task intake box              free-text task description capture
plan preview                 scoped plan artifact, source/model note, validation status
safe target selector         choose target from discovered artifacts; allowed-paths check
risk classifier              labels each proposed step safe / gated / human-only
diff / preview viewer        structured preview + diff of the planned change
approval gate display        the explicit real-TTY or safe-UI approval surface
package ladder progress      plan -> commit -> push -> pr status, each gated
GitHub draft PR card         PR url, isDraft, mergedAt, reviewDecision, idempotency, base/head
evidence timeline            per-step evidence, frozen on completion
cleanup / disposition checklist   inventory + close/retain/merge decision (merge human-only)
STOP gate banner             always-on safety boundaries; never hidden
```

The existing panel already renders many read-only states; the Northstar surface extends it with the
intake/plan/diff/PR-card/timeline/disposition layers and the **gated** ladder controls.

---

## 12. Required State Machine

A Northstar state model (a superset of the existing read-only 13-state next-step machine):

```text
NO_WORKSPACE
WORKSPACE_READY
TASK_DESCRIBED
PLAN_DRAFTED
PLAN_VALIDATED
PREVIEW_READY
AWAITING_APPLY_APPROVAL
APPLIED
PACKAGE_READY
COMMITTED
PUSHED
DRAFT_PR_OPEN
EVIDENCE_FROZEN
DISPOSITION_PENDING
CLOSED_RETAINED
HUMAN_MERGE_PENDING
```

Transition rules (safety-load-bearing):

```text
- NO_WORKSPACE -> WORKSPACE_READY               read-only detection
- WORKSPACE_READY -> TASK_DESCRIBED              task captured (read-only)
- TASK_DESCRIBED -> PLAN_DRAFTED -> PLAN_VALIDATED   plan draft + offline schema validation (read-only)
- PLAN_VALIDATED -> PREVIEW_READY                preview/diff rendered (read-only)
- PREVIEW_READY -> AWAITING_APPLY_APPROVAL       NO auto-advance; waits for a human gate
- AWAITING_APPLY_APPROVAL -> APPLIED             ONLY via real-TTY or explicit safe-UI approval (human-gated)
- APPLIED -> PACKAGE_READY -> COMMITTED          package-plan is read-only; package-commit is gated
- COMMITTED -> PUSHED                            package-push is gated (non-force)
- PUSHED -> DRAFT_PR_OPEN                         package-pr is gated; opens exactly ONE draft PR; idempotent
- DRAFT_PR_OPEN -> EVIDENCE_FROZEN               read-only evidence freeze
- EVIDENCE_FROZEN -> DISPOSITION_PENDING         read-only
- DISPOSITION_PENDING -> CLOSED_RETAINED         gated (close/retain), never destructive by default
- DISPOSITION_PENDING -> HUMAN_MERGE_PENDING     surfaced only; merge/approve/ready are HUMAN-ONLY, never auto

INVARIANTS:
- No transition auto-advances past AWAITING_APPLY_APPROVAL without a human gate.
- No transition performs a hidden apply.
- No transition reaches "merged", "approved", or "ready" via automation.
- DRAFT_PR_OPEN may only ever open a DRAFT PR, never a ready PR.
- No transition force-pushes, force-deletes, or destructively cleans.
```

---

## 13. Required Evidence Model

Every state transition that performs or gates an action must produce a structured evidence record:

```text
per-step record:
  step            workspace-detect | plan-validate | preview | apply | package-commit | package-push | package-pr | disposition
  inputs          worktree, branch, base sha, target file, declared hash
  result          exit code / outcome / markers
  proof           changed-file list, sha256 of target, remote sha (non-force), PR url + isDraft
  gating          which gate was crossed and that it was human-typed where required
  timestamp       captured at action time (supplied to the product, not invented by this scope)

timeline:
  ordered, append-only, read-only once frozen
  freezes at EVIDENCE_FROZEN
  mirrors the existing frozen-handoff evidence model, surfaced live in-product
```

Evidence must be **independently verifiable** (e.g. PR state read directly via `gh pr view`, not
inferred from orchestrator stdout), matching the Lane B verification discipline.

---

## 14. Required Cleanup / Disposition Model

```text
inventory (read-only, safe to automate):
  list disposable worktrees, disposable branches, scratch inputs, open draft PRs tied to this run

disposition options (operator decision):
  1. Keep the draft PR open for review.
  2. Close the draft PR without merge      -> human-gated, separate explicit action.
  3. Proceed to a human-only Stage 5 merge -> HUMAN-ONLY, never automated.
  4. Retain fixture/worktree/branch until decided.

teardown (human-gated, never default, never destructive-by-default):
  remove disposable worktree              -> gated, no --force
  delete disposable branch                -> gated, no -D / no remote --delete by default
  clear scratch inputs                    -> gated, scoped paths only, never `rm -rf` broad
```

Forensic artifacts (fixture worktree/branch, install-smoke worktree) are **never** auto-cleaned and are
out of scope for any automated teardown.

---

## 15. IDE / Panel Requirements

Build on the existing `ide/vscode/a2-harness-panel/` package (VS Code / Cursor):

```text
- Auto-detect workspace on open; render the workspace status card without manual field-set.
- Add the task intake box, plan preview, safe target selector, risk classifier, diff/preview viewer.
- Add the package ladder progress surface and the GitHub draft PR card.
- Add the evidence timeline and the disposition checklist.
- Keep the always-on STOP gate banner.
- Gated controls (apply / package-commit / package-push / package-pr / teardown) must each require an
  explicit, per-action confirm and surface exactly what they will do before doing it.
- The single spawn boundary discipline (array-argv only, no shell, basename allowlist, per-subcommand
  flag allowlist) must extend to any new gated control wire.
- No hidden apply, no auto-approval, no Run-* that bypasses a gate.
```

---

## 16. Terminal / CLI Requirements

```text
- The terminal orchestrator (scripts/a2-tier3-write-orchestrator.sh) remains the source of truth for
  package-plan/commit/push/pr; the product wires to it, it is not reimplemented in the panel.
- Real-TTY apply approval must remain available and authoritative from a real terminal.
- The CLI path must stay usable standalone (terminal-first operators are not regressed).
- Any product-driven gated action must map 1:1 to an existing CLI subcommand with the same gates.
- Exit codes / schemas (e.g. a2-tier4-package-pr.v0) remain the contract the UI renders.
```

---

## 17. GitHub PR Workflow Requirements

```text
- package-pr opens exactly ONE DRAFT PR (base main, head disposable branch). Never a ready PR.
- Idempotency: re-running package-pr opens no duplicate; the existing draft is detected and surfaced.
- The draft PR card displays independently-verified state: url, isDraft, mergedAt, baseRefName,
  headRefName, headRefOid, reviewDecision.
- PR merge, PR approval, and mark-ready are HUMAN-ONLY and never reachable from product automation.
- PR close and remote branch deletion are human-gated, explicit, separate actions.
- No force push; pushes are non-force only.
```

---

## 18. Safety Boundaries

```text
No raw :11434 app inference.
No model / broker / runtime call as part of this scope.
No hidden apply.
No auto-approval.
No auto-merge.
No PR-ready transition without explicit operator command.
No force cleanup.
No broad deletion.
No secret printing.
```

Additional standing boundaries inherited from the proven backbone:

```text
- One lane = one worktree = one branch = one PR.
- Disposable branches only; non-force pushes only.
- Forensic artifacts (fixture worktree/branch, install-smoke worktree) are never auto-cleaned.
- Vault / secrets / runtime / service / model / broker changes are out of every automated path.
```

---

## 19. Non-Goals

```text
- Not implementing any Northstar phase in this lane (this is scope only).
- Not automating any risky step (apply / commit / push / pr / merge stay gated or human-only).
- Not building a hidden or headless apply path.
- Not adding a local model/broker inference path as part of the core flow.
- Not reimplementing the packaging ladder inside the panel.
- Not auto-merging, auto-approving, or auto-marking-ready under any circumstance.
- Not auto-cleaning forensic artifacts.
- Not redesigning the terminal-first CLI out of existence.
```

---

## 20. Phased Implementation Roadmap

Implementation must not start until this scope and the DRAFT prompt are reviewed and merged. Each phase
is its own approved lane.

```text
Phase N1 — Northstar UX scope review / merge
           Review and merge this scope + the DRAFT implementation prompt. No code.

Phase N2 — workspace dashboard + state model
           Auto-detect workspace on open; render the workspace status card; implement the read-only
           Northstar state model (superset of the existing 13-state machine). No apply/package/PR.

Phase N3 — task intake + plan draft UX
           Task intake box; one-click safe plan draft (read-only) + offline plan validation; safe target
           selector from discovered artifacts. No apply/package/PR.

Phase N4 — preview / diff / evidence viewer
           Rendered preview + structured diff viewer; read-only test/guard execution display; per-step
           evidence record capture. No apply/package/PR.

Phase N5 — guided package ladder controls
           Gated worktree creation, real-TTY/safe-UI apply approval, package-plan (read-only),
           package-commit (gated), package-push (gated). Each per-action confirm; no auto-advance.

Phase N6 — draft PR card + evidence timeline
           Gated package-pr opening exactly one DRAFT PR; idempotency surfaced; independently-verified
           draft PR card; live evidence timeline that freezes on completion.

Phase N7 — cleanup / disposition UX
           Read-only inventory; guided disposition checklist; human-gated teardown (no force, no broad
           deletion); merge stays human-only.

Phase N8 — optional local agent planner integration
           OPTIONAL. Only if separately approved. Wire a planner into task intake. Subject to all
           model/broker/runtime boundaries; never on the core apply/package path without its own gates.
```

---

## 21. Validation Strategy

```text
- Each implementation phase ships with unit tests/guards proving its safety invariants
  (mirroring the existing 49/49 panel test discipline and the orchestrator shell tests).
- Safety assertions must include: no Run-* bypasses a gate; no transition auto-advances past
  AWAITING_APPLY_APPROVAL; no path reaches merged/approved/ready via automation; pushes are non-force.
- Read-only surfaces validated against fixtures (workspace status, plan preview, diff, PR card).
- Gated controls validated to require explicit confirm and to map 1:1 to existing CLI subcommands.
- Live smoke for N5/N6 follows the proven Lane A / Lane B discipline: real-TTY apply, one DRAFT PR,
  idempotency re-check, independent gh verification, evidence freeze.
- No live smoke runs as part of THIS scope lane.
```

---

## 22. STOP Gates

```text
- STOP if any implementation is attempted in a scope/review lane.
- STOP if any automated path can reach apply without a human gate.
- STOP if any automated path can reach merge / approve / mark-ready.
- STOP if package-pr could open a non-draft PR or a duplicate PR.
- STOP if any push could be a force push.
- STOP if cleanup could force-delete, broadly delete, or touch a forensic artifact.
- STOP if any flow introduces a model / broker / runtime / Vault / raw :11434 call without explicit
  separate approval and its own gates.
- STOP if the terminal-first CLI path is regressed or removed.
```

---

## 23. Final Recommendation

Stack-Code has proven the isolated mutation → package → draft PR backend. It has not yet reached the
Northstar UX. The next workstream is to turn the proven backbone into an operator-friendly workspace
experience, phase by phase, with every risky step gated and merge human-only.

**Recommendation:** review and merge this scope and the DRAFT implementation prompt first (Phase N1).
Then implement **only Phase N2** (workspace dashboard + state model) in a separate approved lane. Do not
implement further phases until each prior phase is reviewed and merged. Do not weaken any safety gate to
gain UX convenience.
