# A2 Tier 3 Mutation Executor — Write-Capable Step v0 — Implementation Prompt (DRAFT, 2026-06-09)

> DRAFT implementation prompt for a FUTURE code lane. Hand this to Claude Code only after the
> write-capable design scope (`docs/a2-tier3-mutation-executor-write-capable-design-scope.md`) is
> reviewed/merged AND the operator opens a SEPARATE, explicitly-approved implementation lane. This is
> the first lane that would actually create a disposable worktree and write files into it — it is the
> highest-risk lane in the chain and must not be started on a generic "continue".

---

## Role

You are operating as a careful Stack-Code local coding-agent safety implementer.

Your job is to implement **Tier 3 Mutation Executor — Write-Capable Step v0 (minimal)**: an EXTERNAL,
operator-invoked tool that, only after a passing dry-run AND an explicit per-lane operator approval,
creates ONE disposable worktree from origin/main and writes ONLY the declared exact-path files INTO
that disposable worktree, then produces a diff, runs approved validation, records evidence inside the
worktree, and STOPS. The panel stays read-only. The external script's file location requires SEPARATE
operator approval before you create it.

---

## Objective

Add the minimal write-capable executor as an external tool with a mandatory dry-run precondition and
runtime enforcement of every Foundation v0 gate, while the panel remains read-only (it may add at most
a read-only "Proposed Write-Capable Executor Plan" section that PRINTS the command and renders
evidence; it never spawns the executor, creates a worktree, or writes a file).

Source of truth: `docs/a2-tier3-mutation-executor-write-capable-design-scope.md`.

---

## Required Operator Authorization (two gates)

```text
1. This implementation lane must be explicitly opened by the operator (not a generic "continue").
2. Before creating the external executor script, STOP and obtain explicit approval of its exact file
   location and name (it must live OUTSIDE the panel and must NOT be spawnable by the panel).
```

If either is absent, do read-only design/inventory only and report BLOCKED.

---

## Current Proven State

```text
Merged on origin/main:
  PR #109  docs(a2): scope tier 3 mutation executor             90c83a4f003b37dd9a902c64383a7e1712b7e22d
  PR #110  feat(a2): add tier 3 mutation executor dry-run        8795d0b1239ec460d698152868061588dc751f7c
  PR #111  docs(a2): record tier 3 executor dry-run smoke evid.  27deacacf4c9f3c31416c31ad4ca76d6227d3b95

Reusable pure models: tier3Readiness, disposableWorktreePlan, mutationScope (classifyWrite),
safeMutationPolicy (evaluateTier3Command/Write, denials win), mutationEvidence, executorDryRun
(computeDryRun -> DryRunResult.ready + per-step would-accept).
Package : ide/vscode/a2-harness-panel/   Helper: scripts/a2-ide-harness.sh (panel's only spawned binary)
Guards  : ide/vscode/a2-harness-panel/scripts/run-guards.js (authoritative; strips comments/strings)
```

You MUST verify actual file names and the current guard set yourself before editing.

---

## Hard Boundaries

Do NOT:

```text
write to the control checkout or any real/live target (writes go ONLY into a fresh disposable worktree)
proceed without a passing dry-run for the exact lane (hard precondition) + explicit per-lane approval
place the executor inside the panel, or make it spawnable by the panel (the panel stays read-only)
add a create/write/agent-run/agent-execute/apply/approve control to the panel
add fs use or a process spawn to the panel outside helperRunner
create the external executor script before its exact location/name is separately approved (STOP)
push / open a PR / merge / delete a branch / remove a worktree by force (Tier 4, separate)
add network/broker/model/runtime calls or raw :11434 app inference
run live A2 (preview/approval/apply-bundle/apply)
edit scripts/a2-ide-harness.sh (unless separately approved)
edit Rust code / schemas / runtime config / CI config
touch the install-smoke scope branch / local commit 448d7ea
clean disposable smoke/demo worktrees/workspaces
modify .claw artifacts / delete artifacts / touch Vault/secrets / print secrets
```

Destructive commands the implementer must NEVER run (described, not pasted, to keep this prompt
scan-clean): the git working-tree clean operation; recursive force file removal (the rm
recursive+force form); find with -delete or -exec removal; git reset --hard; force-deleting a branch
(the -D form); worktree removal using the force flag; git fetch --prune; force-push to any remote;
git add . / git add -A (exact-path staging only).

Allowed:

```text
create a fresh isolated worktree from origin/main (for THIS implementation lane's own work)
read files; inspect repo/panel/helper/docs/tests
add the external write-capable executor tool ONLY at the separately-approved location (outside the
  panel), with a mandatory dry-run precondition and runtime enforcement of all Foundation v0 gates
add (at most) a read-only panel "Proposed Write-Capable Executor Plan" section (PRINTS command +
  renders evidence; no spawn/create/write)
add unit tests; add runbook documentation + the implementation report
run npm install --ignore-scripts; npm run lint; npm run compile; npm test
commit exact approved files locally
```

---

## Fresh Worktree Setup

```text
Branch   : feat/a2-tier3-mutation-executor-write-capable-v0-<date>
Worktree : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-mutation-executor-write-capable-v0-<date>
```

Do not edit `/home/suki/stack-code` (control checkout only).

---

## Preflight

```text
- control checkout clean; origin/main fetched; PR #111 (dry-run smoke evidence) present on origin/main
- target worktree + branch free
- BOTH operator-authorization gates satisfied (lane opened explicitly + external script location approved)
- create the fresh worktree from origin/main
```

STOP if: control checkout dirty; dry-run base missing; worktree/branch exists; either authorization gate absent.

---

## Implementation Phases

```text
Phase 0  Preflight + authorization gates + fresh worktree. STOP on any gate.
Phase 1  Inventory: read the Foundation v0 + dry-run modules + guards + runbook; confirm filenames.
Phase 2  External write-capable executor (at the approved location, OUTSIDE the panel):
           * refuses unless the dry-run for the exact lane returns ready (re-checked at runtime),
           * verifies clean control checkout + creates ONE disposable worktree from origin/main,
           * writes ONLY declared exact-path files INTO the worktree (classifyWrite-gated),
           * produces a diff inside the worktree, runs ONLY approved validation,
           * records evidence INSIDE the worktree, STOPS for review,
           * performs no push/PR/merge/branch-delete/force-remove; touches no control checkout/real target.
Phase 3  (Optional, panel-side) read-only "Proposed Write-Capable Executor Plan" section: PRINTS the
           exact command + renders evidence. No spawn/create/write control.
Phase 4  Tests: dry-run precondition enforced; exact-path gating (reject-outside / control-checkout);
           denials win; writes confined to the disposable worktree; panel adds no spawn/create/write
           control; a missing approval or failing dry-run aborts.
Phase 5  Runbook update + implementation report.
Phase 6  Build headlessly: npm install --ignore-scripts; npm run lint; npm run compile; npm test.
Phase 7  Commit exact approved files locally. No push.
```

---

## STOP Gates

```text
STOP if any write would land outside a fresh disposable worktree (control checkout / real target / out-of-scope path).
STOP if the dry-run precondition or the per-lane approval is absent.
STOP before creating the external script if its location/name is not separately approved.
STOP if the executor is placed inside the panel, or the panel gains a spawn/fs/create/write surface.
STOP if denials do not win over the Tier-3 allowlist.
STOP if any step would push / open a PR / merge / delete a branch / remove a worktree by force.
STOP if any step would touch runtime/model/broker/service state or raw :11434 app inference.
STOP if the static guards (run-guards.js) or unit tests fail.
STOP if changed files differ from the approved surfaces.
```

---

## Validation

```text
- npm install --ignore-scripts; npm run lint (run-guards.js PASS); npm run compile; npm test green.
- changed-file scope = the approved surfaces only.
- git diff --check clean.
- Confirm (with evidence) that any write performed by tests/validation landed only inside a disposable
  worktree and never the control checkout or a real target.
- Guard-surface scan on panel src (run-guards.js authoritative; verify intent against the diff).
```

---

## Report Format

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_MUTATION_EXECUTOR_WRITE_CAPABLE_V0_IMPLEMENTATION
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
WHAT WAS ADDED:
  external write-capable executor (location; approved?):
  dry-run precondition enforcement:
  exact-path write enforcement:
  diff + approved validation:
  evidence-in-disposable-worktree:
  read-only panel Proposed-Write-Capable-Executor-Plan section (if any):
SAFETY:
  control checkout written:
  real/live target written:
  writes confined to disposable worktree:
  executor placed in panel:
  panel spawn/fs/create/write surface added:
  push/PR/merge/branch-delete/force-remove:
  runtime/model/broker/:11434:
  external script location separately approved:
  install-smoke 448d7ea touched:
  destructive commands used:
VALIDATION:
  guards / compile / tests:
  changed-file scope:
  write-confinement evidence:
  git diff --check:
STOP GATES HIT:
NEXT BEST LANE:
```

---

## Next Lane

```text
Name      : Tier 3 Mutation Executor Write-Capable Step v0 Review / Push PR
Why       : the write-capable executor must be reviewed before any Tier 4 (stage/commit/PR-packaging) lane.
STOP gate : do not design or implement Tier 4 packaging until this lane is merged AND a separate,
            explicitly-approved Tier 4 lane is opened; the panel stays read-only.
```
