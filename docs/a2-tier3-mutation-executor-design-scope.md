# A2 Tier 3 Mutation Executor Design Scope

> Docs-only design/scope. It defines the FIRST actual-mutation capability for the A2 local
> coding-agent cockpit — the executor that would create a disposable worktree and apply scoped edits —
> building on the merged, read-only Tier 3 Foundation v0. It implements nothing, edits no source, adds
> no mutation capability, and runs no live A2 workflow. It is a plan with hard safety boundaries.

---

## 1. Executive Summary

Tier 3 Foundation v0 is the merged, read-only control plane (readiness/state/render) for the
disposable worktree mutation path. The next capability is the **mutation executor** — the component
that would actually create a fresh disposable worktree and apply the operator-approved, exact-scope
edits, then validate and produce evidence. This document scopes that executor.

The central safety decision: **the executor is an external, operator-invoked tool, NOT the panel.**
The VS Code panel stays read-only — it prints the exact executor plan/command and renders the
executor's evidence, but it never creates a worktree, never writes a file, and never spawns the
executor. This mirrors the proven A2 chain model (the panel prints; the operator runs at a real
terminal) and preserves the panel's structural invariants (single spawn boundary, no `fs`, no
network/broker/secret egress).

Even the executor itself is staged: its FIRST lane is **plan / dry-run only** — it validates an
operator-approved plan against the Foundation v0 models and prints exactly what it WOULD do, while
creating no worktree and writing nothing. Actual disposable-worktree creation and scoped writes are a
later, separately-approved executor step.

---

## 2. What the Executor Is (and Is Not)

The executor IS:

```text
An external, operator-invoked, allowlisted tool (a dedicated mutation runner / CLI), separate from the
panel, that — only under an explicit per-lane operator grant — creates an isolated disposable worktree
from origin/main, applies edits limited to the declared exact-path set, runs only approved validation,
produces a diff summary and structured evidence, and STOPS for operator review.
```

The executor is NOT:

```text
Part of the panel webview/extension (the panel stays read-only).
Uncontrolled, unbounded, or autonomous editing.
Anything that writes to the control checkout or to real/live targets.
Anything that runs a live A2 chain, an apply, a model/broker/runtime call, or raw :11434 inference.
Anything that creates PRs, deletes branches, removes worktrees by force, or cleans artifacts.
```

---

## 3. Current Proven Foundation

Merged and evidenced on `origin/main`:

```text
PR #106  docs(a2): scope tier 3 disposable mutation            4bcd8a21b7f721382aa9c4549b9432cb2be18c3a
PR #107  feat(a2): add tier 3 disposable mutation foundation    6efc29a33cf0593dd827260f556e696f7ec530a1
PR #108  docs(a2): record tier 3 foundation v0 smoke evidence   8e9eed64927115d255c74bb9baf5f27d84f6fa06
```

Tier 3 Foundation v0 already provides the pure models the executor must reuse (not re-derive):

```text
src/tier3Readiness.ts          honest Tier 3 readiness (not-checked when unprobed; not-ready by default)
src/disposableWorktreePlan.ts  worktree plan validation (control-checkout-safe; under the disposable root)
src/mutationScope.ts           declared exact-path set; classifyWrite accept/reject; control-checkout reject
src/safeMutationPolicy.ts      denials win + Tier-3 allowlist; write gated by declared scope (classification)
src/mutationEvidence.ts        mutation ledger shape (printed-not-run)
```

The executor enforces, at runtime, exactly what these models classify: a write is performed only if
`classifyWrite` accepts it; a command runs only if `evaluateTier3Command` allows it (denials win).

---

## 4. Non-Negotiable Safety Principles

```text
1. The panel stays read-only. The executor is external and operator-invoked; the panel only prints
   the plan/command and renders evidence — it never creates a worktree, writes a file, or spawns the
   executor.
2. Clean control checkout first. No executor run if /home/suki/stack-code is dirty.
3. Disposable worktree only. Mutation occurs in a fresh isolated worktree from origin/main, never the
   control checkout.
4. One lane = one disposable worktree = one mutation branch.
5. Exact-path scoping enforced at runtime: the executor rejects any write outside the declared set or
   resolving under the control checkout (reuse mutationScope).
6. Deny by default; denials win over any allowlist (reuse safeMutationPolicy / deniedCommands).
7. Explicit per-lane operator approval before any mutation; never inferred, never automatic.
8. Mandatory pre-apply diff summary, produced inside the disposable worktree and shown before any
   step that would package the change; no hidden execution.
9. Approved-only validation; a failed guard/test is a STOP.
10. Rollback-by-abandon: prefer abandoning the disposable worktree over reverting state.
11. No control-checkout writes, no real/live target writes, no runtime/model/broker/service actions,
    no raw :11434 app inference.
12. Structured evidence for every gesture/command; evidence is written only inside the disposable
    worktree, never the control checkout.
```

---

## 5. Executor Architecture

```text
[ Operator ] --approves exact lane--> [ Panel (read-only) ]
     |                                    | prints the exact executor plan/command + renders evidence
     | runs at a real terminal            | (never creates a worktree, never writes, never spawns the executor)
     v
[ Mutation Executor (external CLI) ]
     - reuses Foundation v0 pure models (readiness/plan/scope/policy/evidence) for its gates
     - creates ONE disposable worktree from origin/main (later step; not in executor v0)
     - applies edits ONLY to the declared exact-path set inside that worktree (later step)
     - runs ONLY approved validation inside that worktree
     - produces a diff summary + structured evidence INSIDE the disposable worktree
     - STOPS for operator review; performs no push / PR / merge / branch-delete / force-remove
```

The executor never touches the control checkout, never writes real targets, and never spawns a
model/broker/runtime. The panel's single spawn boundary (`helperRunner.ts`) is unchanged; the executor
is not reachable from the panel.

---

## 6. Executor Lifecycle (each step gated)

```text
1.  Operator approves the exact lane (objective + worktree plan + declared exact-path set).
2.  Executor verifies the control checkout is clean (hard STOP if dirty).
3.  Executor confirms origin/main is the base and the intended branch/worktree path are free.
4.  Executor validates the worktree plan (reuse disposableWorktreePlan).
5.  Executor validates the declared scope (reuse mutationScope.validateDeclaredSet).
6.  Executor creates ONE disposable worktree from origin/main + a unique mutation branch.
       (executor v0: NOT performed — plan/dry-run only.)
7.  Executor records a checkpoint (base commit, branch, status, declared files, optional hashes).
8.  Executor applies edits ONLY to declared paths inside the worktree, each write gated by
       classifyWrite (reject-outside / control-checkout-reject).  (executor v0: NOT performed.)
9.  Executor produces a diff summary inside the disposable worktree.
10. Executor runs ONLY approved validation inside the worktree; a failure is a STOP.
11. Executor writes structured evidence inside the disposable worktree (never the control checkout).
12. Executor STOPS for operator review.
13. No push / PR / merge / branch-delete / force-remove — those are separate, later, explicitly-
       approved lanes (Tier 4).
```

---

## 7. Permission and Approval Model

```text
The executor runs at Tier 3 ONLY under an explicit, per-lane operator approval of the exact objective,
worktree plan, and declared exact-path set. Approval is never inferred and never automatic.
The executor refuses to proceed if the Foundation v0 readiness model would render not-ready.
There is no approval-phrase composition or capture by the panel; the operator approves the exact lane
explicitly (consistent with the A2 chain's real-terminal, human-typed discipline).
```

---

## 8. Clean-Worktree and Isolation Requirements

```text
The control checkout must be on main and clean before the executor runs; a dirty control checkout is a
hard STOP.
The disposable worktree must be freshly created from origin/main and start clean.
The control checkout is never the execution worktree.
The executor verifies branch/path are free before creating anything.
```

---

## 9. Exact-Path Enforcement

```text
The operator declares the exact intended file paths before mutation; the set is immutable for the lane
once approved.
The executor rejects any write whose resolved path is outside the declared set, outside the disposable
worktree, or under the control checkout (reuse mutationScope.classifyWrite — pure normalization
rejects traversal escapes).
No repo-wide formatting; no drive-by changes to unrelated files; no add-all staging semantics.
```

---

## 10. Denials-Win Enforcement

```text
Before running any command, the executor checks the denied-command registry FIRST (reuse
deniedCommands / safeMutationPolicy.evaluateTier3Command). A denied-family command is denied even when
the Tier-3 allowlist would permit it. Denials always win.
Writes are gated solely by the declared exact-path scope (safeMutationPolicy.evaluateTier3Write).
```

---

## 11. Pre-Apply Diff Summary

```text
The executor produces a diff summary (changed files + diff stat + a readable per-file summary) inside
the disposable worktree and surfaces it for operator review BEFORE any step that would package the
change. No hidden execution: the operator sees the proposed change first. The diff is recorded in the
evidence ledger.
```

---

## 12. Checkpoint and Rollback-by-Abandon

```text
Before any edit, the executor records a checkpoint (base commit, mutation branch, clean status,
declared files, optional file hashes). After edits, it records changed files, diff stat, touched-file
hashes, and validation results.
Rollback PREFERS abandoning the disposable worktree (leave it for a separate, safe, non-force cleanup
lane). The executor never removes a worktree by force and never force-deletes a branch.
```

---

## 13. Approved-Only Validation

```text
Only explicitly-approved (allowlisted) validation commands run, inside the disposable worktree
(for the panel package: npm install --ignore-scripts; npm run lint; npm run compile; npm test).
Validation never calls a model/broker/runtime and never touches :11434.
A failed guard or test is a STOP: the executor proposes no further step.
```

---

## 14. Evidence Ledger

```text
The executor writes structured evidence (checkpoint / mutation / validation / decision events; reuse
the mutationEvidence shape) INSIDE the disposable worktree only — never the control checkout. Print/
checkpoint steps are marked printed-not-run. The panel renders this evidence read-only; it writes
nothing.
```

---

## 15. Hard Denials

The executor must deny these globally, regardless of any allowlist (described, not pasted verbatim, so
this design stays scan-clean):

```text
- the git working-tree clean operation (untracked-file removal)
- recursive force file removal (the rm recursive+force form)
- find used with -delete, or with an -exec removal
- git reset --hard
- force-deleting a branch (the -D form)
- worktree removal using the force flag
- git fetch --prune
- force-push to any remote
- restarting services / runtime/service control (runtime-or-service-restart and service-control families)
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

Rule: **denials win over allowlists.**

---

## 16. Explicit Non-Goals

```text
No autonomous mutation in this design lane.
No implementation in this design lane.
No executor in the panel; the panel stays read-only.
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

## 17. STOP Gates

```text
STOP if the control checkout is dirty before an executor run.
STOP if the disposable worktree or mutation branch already exists.
STOP if origin/main cannot be fetched.
STOP if the Foundation v0 readiness model renders not-ready.
STOP if a write resolves outside the declared set or under the control checkout.
STOP if any denied-registry family matches a proposed command (denials win).
STOP if approved validation fails.
STOP if any step would push / open a PR / merge / delete a branch / remove a worktree by force.
STOP if any step would touch runtime/model/broker/service state or raw :11434 app inference.
STOP if the executor is ever placed inside the panel (the panel stays read-only).
STOP if operator approval for the exact lane is absent.
```

---

## 18. Implementation Plan Recommendation

Stage the executor so even its first lane is non-mutating:

```text
Tier 3 Mutation Executor v0 — PLAN / DRY-RUN ONLY:
- An external, operator-invoked tool that takes an approved lane (objective + worktree plan +
  declared exact-path set) and:
    * validates readiness/plan/scope/policy against the Foundation v0 pure models,
    * prints exactly the worktree it WOULD create and the edits it WOULD apply,
    * runs NO worktree creation and writes NOTHING,
    * emits a structured dry-run evidence record (printed-not-run).
- The panel adds a read-only "Proposed Executor Plan" view that PRINTS the exact dry-run command and
  renders the dry-run evidence — it never spawns the executor.
- Tests: plan/scope/policy gating; dry-run performs no creation/write; denials win.

Later, separately-approved steps (NOT this lane):
- Executor write step: actual disposable-worktree creation + scoped writes, under a fresh explicit
  approval, with checkpoint + diff + approved validation + evidence.
- Tier 4: PR packaging from the disposable worktree.
```

This keeps the move from design to executor a controlled, non-mutating first step.

---

## 19. Recommended Next Lane

```text
Name        : Tier 3 Mutation Executor Design Scope Review / Push PR
Type        : docs-only
Objective   : review this scope package + the implementation-prompt DRAFT, then push the branch and
              open a docs-only PR for operator review.
Tool        : Claude Code (docs review/push lane) + operator review.
Why         : the executor design must be reviewed (or the operator must explicitly skip the docs gate)
              before any Tier 3 Mutation Executor v0 (dry-run) implementation lane begins.
Mutation    : none (docs-only).
STOP gate   : do not begin executor implementation until this scope is reviewed/merged or review is
              explicitly skipped; the first executor lane is plan/dry-run only — no worktree creation,
              no writes; the panel stays read-only.
```

The implementation lane is driven by
`handoffs/a2_tier3_mutation_executor_implementation_prompt_DRAFT_2026-06-08.md`.
