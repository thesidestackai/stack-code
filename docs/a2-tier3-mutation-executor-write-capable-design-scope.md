# A2 Tier 3 Mutation Executor — Write-Capable Step Design Scope

> Docs-only design/scope. It defines the FIRST capability that would actually create a disposable
> worktree and write files into it, building on the merged dry-run executor. It implements nothing,
> edits no source, adds no write capability, and runs no live A2 workflow. It is a plan with hard
> safety boundaries.

---

## 1. Executive Summary

The merged Tier 3 Mutation Executor v0 is **plan / dry-run only**: it classifies what an external
executor WOULD do and creates/writes nothing. The next capability is the **write-capable step** — the
external, operator-invoked executor that, only after a passing dry-run AND an explicit per-lane
operator approval, creates ONE fresh disposable worktree from origin/main and applies the
already-declared, already-dry-run-validated edits **inside that disposable worktree only**, then
produces a diff and runs approved validation and STOPS for operator review.

Two boundaries make this safe by construction:

```text
1. The only thing ever written is files INSIDE a fresh, throwaway, disposable worktree — never the
   control checkout, never real/live targets. The disposable worktree IS the sandbox.
2. A passing dry-run is a hard precondition, and the operator explicitly approves the exact lane
   (objective + worktree plan + declared exact-path set + the dry-run diff) before any write.
```

The panel stays read-only (it prints the executor plan/command and renders evidence; it never spawns
the executor, creates a worktree, or writes a file). No push / PR / merge happens here — that is Tier
4. Even the write-capable step is staged: the recommended first implementation is minimal (one lane,
one disposable worktree, the declared files only, mandatory dry-run precondition, no batching).

---

## 2. What the Write-Capable Step Is (and Is Not)

The write-capable step IS:

```text
An external, operator-invoked tool that — only after a passing dry-run and an explicit per-lane
operator approval — creates ONE isolated disposable worktree from origin/main and writes ONLY the
declared exact-path files INTO that disposable worktree, then produces a diff, runs approved
validation, records evidence inside the worktree, and STOPS for operator review.
```

The write-capable step is NOT:

```text
Part of the panel (the panel stays read-only).
Uncontrolled, unbounded, or self-directed editing.
Anything that writes the control checkout or real/live targets.
Anything that runs a live A2 chain, an apply to a real target, a model/broker/runtime call, or raw
  :11434 inference.
Anything that creates PRs, merges, pushes, deletes branches, or removes worktrees by force.
Anything that proceeds without a passing dry-run and an explicit per-lane operator approval.
```

---

## 3. Current Proven Foundation

Merged and evidenced on `origin/main`:

```text
PR #109  docs(a2): scope tier 3 mutation executor             90c83a4f003b37dd9a902c64383a7e1712b7e22d
PR #110  feat(a2): add tier 3 mutation executor dry-run        8795d0b1239ec460d698152868061588dc751f7c
PR #111  docs(a2): record tier 3 executor dry-run smoke evid.  27deacacf4c9f3c31416c31ad4ca76d6227d3b95
```

The dry-run model (`src/executorDryRun.ts`) already computes, for an approved lane, a `DryRunResult`
with `ready`, per-step `would-accept`/`would-reject`, and `wouldCreateWorktree`/`wouldWriteFiles`
(always false in dry-run). The write-capable step reuses this: it refuses to run unless the dry-run
for the same lane returns `ready` with every declared write `would-accept`.

It also reuses the Foundation v0 pure models for runtime enforcement: `tier3Readiness`,
`disposableWorktreePlan`, `mutationScope` (`classifyWrite` reject-outside / control-checkout-reject),
`safeMutationPolicy` (denials win over the Tier-3 allowlist).

---

## 4. Non-Negotiable Safety Principles

```text
1. The panel stays read-only. The write-capable executor is external and operator-invoked.
2. Dry-run is a hard precondition: no write without a passing dry-run for the exact same lane.
3. Explicit per-lane operator approval before any write; never inferred, never automatic.
4. Clean control checkout first; the control checkout is never the execution worktree.
5. ONE fresh disposable worktree from origin/main per lane; it is the only place files are written.
6. Writes are limited to the declared exact-path set; each write is gated by classifyWrite at runtime.
7. Deny by default; denials win over any allowlist.
8. Checkpoint before writes; produce a diff after; rollback PREFERS abandoning the disposable worktree.
9. Approved-only validation; a failed guard/test is a STOP.
10. No writing the control checkout or real/live targets. No runtime/model/broker/service actions. No
    raw :11434 app inference.
11. No push / PR / merge / branch-delete / force-remove (Tier 4, separate and later).
12. Evidence is written only inside the disposable worktree, never the control checkout.
```

---

## 5. Architecture

```text
[ Operator ] --(1) dry-run ready + (2) approves exact lane--> [ Panel (read-only) ]
     |                                                            | prints the write-capable executor command
     | runs at a real terminal                                    | + renders evidence (never spawns it, never writes)
     v
[ Write-Capable Mutation Executor (external CLI) ]
     - refuses unless the dry-run for the same lane returned ready (precondition re-checked)
     - verifies clean control checkout + creates ONE disposable worktree from origin/main + mutation branch
     - writes ONLY the declared exact-path files INTO the disposable worktree (classifyWrite-gated)
     - produces a diff (inside the worktree) + runs ONLY approved validation
     - records evidence INSIDE the disposable worktree
     - STOPS for operator review; performs no push / PR / merge / branch-delete / force-remove
```

The panel's single spawn boundary (`helperRunner.ts`) is unchanged; the write-capable executor is not
reachable from the panel.

---

## 6. Preconditions

```text
A passing dry-run for the EXACT lane (same objective + worktree plan + declared set + proposed writes):
  - DryRunResult.ready === true
  - every declared write step would-accept
  - wouldCreateWorktree/wouldWriteFiles were false (dry-run) — the write-capable step is what flips
    creation/writes on, and only inside the disposable worktree.
Explicit per-lane operator approval of the objective + worktree plan + declared exact-path set + the
  dry-run diff. Approval is per-lane, never inferred, never automatic.
Clean control checkout; origin/main fetched; intended worktree path + branch free.
```

If any precondition is unmet, the write-capable step refuses and reports the unmet gate.

---

## 7. Write-Capable Lifecycle (each step gated)

```text
1.  Operator runs the dry-run for the lane; it returns ready (hard precondition).
2.  Operator explicitly approves the exact lane (objective + plan + declared set + dry-run diff).
3.  Executor re-verifies the dry-run precondition and the approval.
4.  Executor verifies the control checkout is clean (hard STOP if dirty).
5.  Executor confirms origin/main is the base and the branch/worktree path are free.
6.  Executor creates ONE disposable worktree from origin/main + a unique mutation branch.
7.  Executor records a checkpoint (base commit, branch, status, declared files, optional hashes).
8.  Executor writes ONLY the declared exact-path files INTO the disposable worktree; each write is
       classifyWrite-gated (reject-outside / control-checkout-reject). No add-all; exact paths only.
9.  Executor produces a diff summary inside the disposable worktree.
10. Executor runs ONLY approved validation inside the worktree; a failure is a STOP.
11. Executor records structured evidence INSIDE the disposable worktree.
12. Executor STOPS for operator review.
13. No push / PR / merge / branch-delete / force-remove — Tier 4, separate and later.
```

---

## 8. Clean-Worktree and Isolation Requirements

```text
The control checkout must be on main and clean before the executor runs; a dirty control checkout is a
hard STOP. The control checkout is never the execution worktree.
ONE fresh disposable worktree from origin/main per lane; it starts clean and is the only write surface.
The executor verifies branch/path are free before creating anything.
```

---

## 9. Exact-Path Write Enforcement (runtime)

```text
Every write is classified by mutationScope.classifyWrite BEFORE it happens: accepted only if the path
is in the declared set, inside the disposable worktree, and not under the control checkout. Pure path
normalization rejects traversal escapes.
A rejected path aborts the lane (STOP); the executor does not "best-effort" partial writes outside the
declared set.
No repo-wide formatting; no drive-by changes; no add-all staging semantics.
```

---

## 10. Denials-Win Enforcement

```text
Before any command (e.g. approved validation), the executor checks the denied-command registry FIRST
(reuse safeMutationPolicy.evaluateTier3Command). A denied-family command is denied even when the
Tier-3 allowlist would permit it. Denials always win.
```

---

## 11. Diff Summary

```text
After writing into the disposable worktree, the executor produces a diff summary (changed files + diff
stat + per-file summary), computed inside the worktree, and records it in the evidence ledger. The
operator reviews it before any later (Tier 4) packaging step. No hidden execution.
```

---

## 12. Checkpoint and Rollback-by-Abandon

```text
Before writes: record base commit, mutation branch, clean status, declared files, optional file hashes.
After writes: record changed files, diff stat, touched-file hashes, validation results.
Rollback PREFERS abandoning the disposable worktree (leave it for a separate, safe, non-force cleanup
lane). The executor never reverts the control checkout, never removes a worktree by force, and never
force-deletes a branch.
```

---

## 13. Approved-Only Validation

```text
Only explicitly-approved (allowlisted) validation commands run, inside the disposable worktree
(for the panel package: npm install --ignore-scripts; npm run lint; npm run compile; npm test).
Validation never calls a model/broker/runtime and never touches :11434.
A failed guard or test is a STOP: the executor records the failure and proposes no further step.
```

---

## 14. Evidence Ledger

```text
The executor writes structured evidence (checkpoint / write / validation / decision events; reuse the
mutationEvidence shape) INSIDE the disposable worktree only — never the control checkout. The panel
renders this evidence read-only; it writes nothing.
```

---

## 15. Hard Denials

The write-capable executor must deny these globally, regardless of any allowlist (described, not
pasted verbatim, so this design stays scan-clean):

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
- writing to any real/live target outside the disposable worktree
- writing to any path outside the declared scope
```

Rule: **denials win over allowlists.**

---

## 16. What Stays Out (Tier 4)

```text
Staging exact paths for a commit, composing a commit, and opening a PR are Tier 4 — out of scope for
the write-capable step. The write-capable step STOPS after diff + validation inside the disposable
worktree, for operator review. No push / PR / merge / branch-delete / force-remove here.
```

---

## 17. Explicit Non-Goals

```text
No autonomous mutation in this design lane.
No implementation in this design lane.
No write-capable executor in the panel; the panel stays read-only.
No writing to the control checkout.
No writing to real/live targets.
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
STOP if the dry-run for the exact lane did not return ready (hard precondition).
STOP if operator approval for the exact lane is absent.
STOP if the control checkout is dirty before an executor run.
STOP if the disposable worktree or mutation branch already exists.
STOP if origin/main cannot be fetched.
STOP if a write resolves outside the declared set, outside the disposable worktree, or under the
  control checkout.
STOP if any denied-registry family matches a proposed command (denials win).
STOP if approved validation fails.
STOP if any step would push / open a PR / merge / delete a branch / remove a worktree by force.
STOP if any step would touch runtime/model/broker/service state or raw :11434 app inference.
STOP if the write-capable executor is ever placed inside the panel (the panel stays read-only).
```

---

## 19. Implementation Plan Recommendation

Stage the write-capable step minimally:

```text
Tier 3 Mutation Executor — Write-Capable Step v0 (minimal):
- An EXTERNAL, operator-invoked tool (a standalone script), NOT the panel and NOT spawnable by it.
  Its file location requires SEPARATE operator approval (a STOP-for-location in the implementation lane).
- Mandatory dry-run precondition: refuses unless the dry-run for the exact lane returned ready.
- ONE lane, ONE disposable worktree, the declared exact-path files only — no batching, no auto-anything.
- Creates the worktree from origin/main, writes the declared files INTO it (classifyWrite-gated),
  produces a diff, runs approved validation, records evidence INSIDE the worktree, STOPS for review.
- The panel adds (at most) a read-only "Proposed Write-Capable Executor Plan" section that PRINTS the
  exact command and renders evidence — never spawns the executor, never writes.
- Tests (for any panel-side model): precondition enforcement; exact-path gating; denials win; the
  panel adds no spawn/create/write control.

Later, separately-approved:
- Tier 4: stage + commit + open a PR from the disposable worktree.
```

---

## 20. Recommended Next Lane

```text
Name        : Tier 3 Mutation Executor Write-Capable Step Design Scope Review / Push PR
Type        : docs-only
Objective   : review this design scope + the implementation-prompt DRAFT, then push the branch and
              open a docs-only PR for operator review.
Tool        : Claude Code (docs review/push lane) + operator review.
Why         : the write-capable design must be reviewed (or the operator must explicitly skip the docs
              gate) before any implementation lane begins — and the implementation itself needs a
              separate explicit approval, especially for the external executor script's location.
Mutation    : none (docs-only).
STOP gate   : do not begin write-capable implementation until this scope is reviewed/merged AND a
              separate, explicitly-approved implementation lane is opened; the panel stays read-only;
              the external executor script location requires separate approval.
```

The implementation lane is driven by
`handoffs/a2_tier3_mutation_executor_write_capable_implementation_prompt_DRAFT_2026-06-09.md`.
