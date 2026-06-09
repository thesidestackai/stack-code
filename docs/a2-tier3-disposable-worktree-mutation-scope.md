# A2 Tier 3 Disposable Worktree Mutation Scope

> Docs-only design/scope. It defines how Tier 3 (disposable worktree mutation) should work for the
> A2 local coding-agent cockpit, building on the merged read-only Foundation v0. It implements
> nothing, edits no source, adds no mutation capability, and runs no live A2 workflow. It is a plan
> with hard safety boundaries, not an implementation.

---

## 1. Executive Summary

Tier 3 is the **first mutation tier**, but only inside isolated disposable Git worktrees. Tier 3 must
not mutate the control checkout. Tier 3 must not write to real/live targets directly. Tier 3 must not
run a live A2 apply. Tier 3 must be operator-approved, exact-scope, evidence-led, and rollback-aware.

The strategic move is **not** uncontrolled mutation or autonomous editing. It is a tightly-gated,
disposable-worktree mutation path that preserves Foundation v0's invariants: clean-worktree-first,
exact-path scoping, denials-win, evidence-first, and operator control at every mutation boundary. The
recommended next code lane (Tier 3 Foundation v0) adds only the readiness/state/render layer for this
path — no actual mutation executor, no worktree-creation control — so the move from design to UI stays
a controlled step that enables no writes by itself.

---

## 2. Tier 3 Definition

```text
Tier 3 = Disposable Worktree Mutation.

Mutation is allowed only inside a fresh isolated disposable worktree.
The control checkout (/home/suki/stack-code) must remain untouched.
The source repo root must remain clean.
The operator must approve the exact lane before mutation.
The agent must declare exact intended touched files before mutation.
The agent must produce a diff summary and evidence after mutation.
The agent must run only explicitly-approved validation.
The agent must never touch runtime/model/broker/service state.
The agent must never write to real/live targets outside the disposable worktree.
```

Tier 3 must **not** mean:

```text
Uncontrolled, unbounded, or autonomous editing.
Writing to /home/suki/stack-code directly.
Mutating a dirty worktree.
Writing unknown/undeclared files.
Running a live A2 chain or apply.
Creating PRs / deleting branches / deleting worktrees / cleaning artifacts.
Touching runtime/services, calling models/brokers, or raw :11434 app inference.
```

---

## 3. Current Proven Foundation

Merged and evidenced on `origin/main`:

```text
PR #104  feat(a2): add local coding agent foundation        9e8781674ca38044210d5c615f4a6bce5ddd2a4b
PR #105  docs(a2): record foundation v0 smoke evidence       15647ba9a429d150b4ca18c04fdec1164ca88182
```

Foundation v0 already proves (read-only control plane):

```text
Permission tiers (Tier 0–5) exist; effective tier is read-only; Tier 5 denied by default.
A global denied command registry exists; denials win over allowlists.
An agent session model exists (non-persistent, no secrets).
An agent readiness model exists; git readiness renders honest not-checked (no guard-safe probe yet).
An agent evidence ledger exists (printed-not-run semantics).
A rendered operator GUI smoke passed; mutation is still disabled; no live A2 chain is enabled.
```

Tier 3 is the documented next tier in `docs/a2-local-coding-agent-foundation-scope.md` §6 (requires
explicit approval; not enabled by v0). The Tier 3 family ids in the denied registry
(`destructive-filesystem-cleanup`, `force-branch-or-worktree-deletion`, `history-rewrite-or-force-push`,
`service-control`, `runtime-or-service-restart`, `model-or-broker-call`, `raw-app-inference`,
`vault-or-secret-read`, `live-a2-chain-execution`, `approval-line-composition`, `network-egress`,
`watcher-polling-timer-automation`, `hidden-execution`) are reused as the enforcement vocabulary here.

---

## 4. Non-Negotiable Safety Principles

```text
1. Clean control checkout first. No mutation if /home/suki/stack-code is dirty.
2. Disposable worktree only. Mutation happens in a fresh isolated worktree from origin/main, never
   the control checkout.
3. One lane = one disposable worktree = one mutation branch.
4. Exact-path scoping. The agent declares intended files; the executor rejects writes outside them.
5. Deny by default; denials win over any allowlist.
6. Evidence first. Checkpoint before mutation; diff summary + per-command evidence after.
7. Operator control at every mutation boundary; no mutation without explicit operator approval.
8. Rollback-aware: prefer abandoning the disposable worktree over mutating state back.
9. No real/live target writes; no runtime/model/broker/service actions; no raw :11434 app inference.
10. No push / PR / merge in Tier 3 (those belong to a later, separately-approved Tier 4 lane).
```

---

## 5. Tier 3 Permission Boundary

Tier 3 is allowed **only** when all of the following hold:

```text
Allowed only after explicit operator approval of the exact lane.
Allowed only in a fresh disposable worktree created from origin/main.
Allowed only for the exact declared file path set.
Allowed only when the control checkout is verified clean.
Allowed only when the disposable worktree starts clean.
Allowed only with the denied-command registry enforced (denials win).
Allowed only with before/after diff evidence captured.
Allowed only with validation commands that are explicitly approved (allowlisted) for the lane.
```

If any condition is unmet, Tier 3 stays blocked and the cockpit reports the unmet gate honestly.

---

## 6. Disposable Worktree Lifecycle

```text
1.  Observe the control checkout.
2.  Confirm the control checkout is clean (no staged/unstaged/untracked tracked changes).
3.  Confirm origin/main is fetched and is the base.
4.  Confirm the intended branch and worktree path do not already exist.
5.  Create the disposable worktree from origin/main.
6.  Create a unique mutation branch for the lane.
7.  Record the worktree path and branch in the agent session.
8.  Declare the intended touched files before any mutation.
9.  Apply edits only inside the disposable worktree, only to declared paths.
10. Produce a diff summary of the proposed change.
11. Run only approved validation commands.
12. Record the evidence ledger (checkpoint, diff, validation).
13. STOP for operator review.
14. No push / PR / merge in Tier 3 — only a later, explicitly-approved Tier 4 lane may package.
```

---

## 7. Clean-Worktree Requirements

```text
Control checkout (/home/suki/stack-code) must be on main and clean before any Tier 3 lane begins.
The disposable worktree must start clean (freshly created from origin/main; no untracked carry-over).
A dirty control checkout is a hard STOP — the cockpit must surface it, not work around it.
The control checkout is never the execution worktree for Tier 3.
```

This mirrors the universal session operating rule (one lane = one worktree = one branch) and
Foundation v0's honest-status discipline (never green-by-default).

---

## 8. Exact-Path Mutation Scope

```text
The agent must declare its intended file paths before mutation.
The executor must reject any write outside the declared path set.
No broad/repo-wide formatting.
No drive-by cleanup of unrelated files.
No directory-wide mutation unless that directory is separately and explicitly approved.
No "stage everything" semantics: exact-path staging only; no add-all behavior.
Each declared path is shown to the operator before mutation.
```

The declared path set is part of the agent session manifest (`touchedSurfaces`) and is immutable for
the lane once approved.

---

## 9. Safe Executor Enforcement Model

The safe executor is the single gate between an intended mutation/command and its execution. Design
(not implementation) requirements:

```text
- Denied-registry check FIRST: a command on the denied registry is denied regardless of tier or
  allowlist (denials win).
- Allowlist-by-tier SECOND: a non-denied command runs only if it is on the Tier 3 allowlist for the
  lane (e.g. a declared-path file write, an approved local build/test command).
- Exact-path guard: a file write is denied unless its path is in the declared set AND inside the
  disposable worktree.
- Clean-worktree guard: mutation is denied unless the control checkout is clean and the disposable
  worktree was freshly created from origin/main.
- Control-checkout guard: any write whose resolved path is under the control checkout is denied.
- Structured evidence: every command (allowed or denied) yields a record (tier, command, decision,
  reason, exit code, printed-not-run marker for print-only steps).
- The executor preserves Foundation v0's structural invariants: the single spawn boundary, no fs
  outside it in the panel layer, no network/broker/secret egress, no chain-write literal in live code.
```

---

## 10. Denied Commands and Denials-Win Rule

Tier 3 must deny these globally, regardless of any allowlist (described, not pasted verbatim, so this
design stays scan-clean):

```text
- the git working-tree clean operation (untracked-file removal)
- recursive force file removal (the rm recursive+force form)
- find used with -delete, or with an -exec removal
- git reset --hard
- force-deleting a branch (the -D form)
- worktree removal using the force flag
- git fetch --prune
- force-push to any remote
- restarting services / runtime/service control (the runtime-or-service-restart and service-control
  denied families)
- model loads/unloads
- broker calls
- raw :11434 app inference
- Vault/secret reads
- live A2 chain execution (preview / approval / apply-bundle / apply)
- approval-phrase generation or capture
- network egress
- watcher/polling/timer automation
- hidden command execution
- writing to the control checkout
- writing to any path outside the declared scope
```

Rule: **denials win over allowlists.** A command that matches a denied family is denied even when the
Tier 3 allowlist would permit it. This reuses the Foundation v0 `deniedCommands` registry +
`evaluate(command, allowlist)` semantics.

---

## 11. Agent Session Mutation Ledger

Extend the Foundation v0 agent session + evidence ledger for the mutation path:

```text
session adds (for a Tier 3 lane):
  targetWorktree     : the disposable worktree path (only when one exists)
  targetBranch       : the unique mutation branch
  touchedSurfaces    : the exact declared file paths (immutable once approved)
  allowedTier        : 3 only after explicit operator approval; read-only otherwise

mutation ledger event adds:
  kind: checkpoint | mutation | validation | decision | note
  tier: 3
  action / status / summary / details
  decision: allowed | denied (+ reason)
  printedNotRun: true for any print-only step
  no secrets; session-local; not persisted in Tier 3 v0
```

---

## 12. Checkpoint and Rollback Model

Design (not implementation) of a checkpoint model:

```text
Before mutation:
- record the base commit (origin/main HEAD)
- record the mutation branch
- record the worktree status (clean)
- record the intended touched files
- optionally record file hashes for the intended files

After mutation:
- record the changed files
- record the diff stat
- record file hashes for touched files
- record the validation commands and their results
- record that rollback is possible by abandoning the disposable worktree
```

Important rollback rule:

```text
Tier 3 v0 rollback PREFERS abandoning the disposable worktree (leave it for the operator to remove in
a separate, safe, non-force cleanup lane) over mutating state back. The cockpit never force-removes a
worktree and never force-deletes a branch.
```

---

## 13. Diff Summary Requirements

```text
A diff summary (changed files + diff stat, and a readable per-file summary) must be produced and shown
to the operator BEFORE any apply/commit step is even proposed.
The diff is computed inside the disposable worktree only.
No hidden execution: the operator sees the proposed change before any further step.
The diff summary is recorded in the mutation ledger.
```

---

## 14. Validation Requirements

```text
Only explicitly-approved (allowlisted) validation commands may run, inside the disposable worktree.
For the panel package, the canonical validation is: npm install --ignore-scripts; npm run lint
(static guards); npm run compile; npm test.
Validation results (pass/fail + evidence) are recorded in the mutation ledger.
Validation never calls a model/broker/runtime and never touches :11434.
A failed guard or test is a STOP: do not propose any further step.
```

---

## 15. Operator Approval UX

```text
The operator must approve the exact Tier 3 lane: the objective, the disposable worktree plan, and the
exact declared touched-file set, before any mutation.
Approval is explicit and per-lane; it is never inferred and never auto-granted.
The cockpit raises the effective tier to 3 only after that explicit approval; it defaults to read-only.
The operator remains in control at every mutation boundary: declare → approve → mutate (disposable) →
diff → validate → STOP for review.
There is no approval-phrase composition or capture by the cockpit (consistent with the A2 chain's
real-terminal, human-typed approval discipline).
```

---

## 16. IDE Cockpit Changes

Propose these read-only-by-default panel sections (design only; no executor in this scope):

```text
[ Tier 3 Readiness ]              clean control checkout? origin/main fetched? disposable-worktree
                                  plan valid? — honest tri-state, never green-by-default.
[ Disposable Worktree Plan ]      the intended worktree path + mutation branch (before creation).
[ Declared Touched Files ]        the exact declared path set, shown before mutation.
[ Mutation Approval Gate ]        an explicit operator approval boundary; read-only until approved.
[ Diff Summary ]                  the proposed change (computed in the disposable worktree).
[ Validation Results ]            approved validation outcomes + evidence.
[ Rollback / Abandon Worktree Guidance ]  how to abandon the disposable worktree safely (non-force).
[ Evidence Ledger ]               the mutation ledger (checkpoint/mutation/validation/decision).
```

No implementation, no mutation executor, and no worktree-creation button in this design lane.

---

## 17. Explicit Non-Goals

```text
No autonomous mutation in this design lane.
No implementation in this design lane.
No writing to the control checkout.
No real target writes.
No live A2 chain execution.
No PR creation.
No branch deletion.
No remote push.
No service/runtime/model/broker operations.
No raw :11434 app inference.
No cleanup of smoke artifacts.
```

---

## 18. STOP Gates

```text
STOP if the control checkout is dirty before a Tier 3 lane.
STOP if the disposable worktree or mutation branch already exists.
STOP if origin/main cannot be fetched.
STOP if a write resolves outside the declared path set or under the control checkout.
STOP if any denied-registry family matches a proposed command (denials win).
STOP if static guards or approved validation fail.
STOP if any step would push / open a PR / merge / delete a branch / remove a worktree by force.
STOP if any step would touch runtime/model/broker/service state or raw :11434 app inference.
STOP if operator approval for the exact lane is absent.
```

---

## 19. Implementation Plan Recommendation

Recommend the next code lane as **Tier 3 Foundation v0** — readiness/state/render only, no executor:

```text
- Tier 3 readiness model (clean control checkout / origin-main / plan-valid — honest tri-state).
- Disposable worktree plan model (intended path + mutation branch; no creation).
- Declared mutation scope model (exact-path set; reject-outside semantics; no writes).
- Safe mutation policy model (denials-win + Tier-3 allowlist shape; classification only).
- Mutation evidence ledger shape (checkpoint/mutation/validation/decision render).
- Panel sections for the §16 surfaces, all read-only.
- Tests for the readiness model, the exact-path reject logic, and denials-win.
- No actual file writing, no worktree creation, no executor — those are separate, later,
  explicitly-approved lanes.
```

This keeps the next code step a controlled design-to-UI move that enables no writes by itself.

---

## 20. Recommended Next Lane

```text
Name        : Tier 3 Disposable Worktree Mutation Scope Review / Push PR
Type        : docs-only
Objective   : review this scope package + the implementation-prompt DRAFT, then push the branch and
              open a docs-only PR for operator review.
Tool        : Claude Code (docs review/push lane) + operator review.
Why         : the design must be reviewed (or the operator must explicitly skip the docs gate) before
              any Tier 3 Foundation v0 implementation lane begins.
Mutation    : none (docs-only).
STOP gate   : do not begin Tier 3 implementation until this scope is reviewed/merged or the operator
              explicitly skips review; the first code lane is readiness/state/render only — no executor.
```

The implementation lane is driven by
`handoffs/a2_tier3_disposable_worktree_mutation_implementation_prompt_DRAFT_2026-06-08.md`.
