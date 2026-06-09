# A2 Tier 3 Mutation Executor v0 — Implementation Prompt (DRAFT, 2026-06-08)

> DRAFT implementation prompt for the next code lane. Hand this to Claude Code only after the executor
> design scope (`docs/a2-tier3-mutation-executor-design-scope.md`) is reviewed, or after the operator
> explicitly skips the docs gate. This lane implements **plan / dry-run only** — it creates NO
> disposable worktree, writes NO file, and keeps the panel read-only.

---

## Role

You are operating as a careful Stack-Code local coding-agent safety implementer.

Your job is to implement **Tier 3 Mutation Executor v0 — plan / dry-run only**: an external,
operator-invoked tool that validates an approved Tier 3 lane against the Foundation v0 pure models and
prints exactly what it WOULD do, performing NO worktree creation and NO writes; plus a read-only panel
"Proposed Executor Plan" view that PRINTS the dry-run command and renders dry-run evidence. You add NO
actual mutation, NO worktree creation, and NO write capability.

---

## Objective

Add the FIRST executor lane as a non-mutating dry-run, while preserving every panel invariant (single
spawn boundary, no `fs` outside it, no network/broker/model/secret/:11434 egress, no watcher/polling/
timer, no Run-the-chain control, honest status). The executor is external and operator-invoked; the
panel never spawns it, never creates a worktree, and never writes a file.

Source of truth: `docs/a2-tier3-mutation-executor-design-scope.md`.

---

## Current Proven State

```text
Merged on origin/main:
  PR #106  docs(a2): scope tier 3 disposable mutation            4bcd8a21b7f721382aa9c4549b9432cb2be18c3a
  PR #107  feat(a2): add tier 3 disposable mutation foundation    6efc29a33cf0593dd827260f556e696f7ec530a1
  PR #108  docs(a2): record tier 3 foundation v0 smoke evidence   8e9eed64927115d255c74bb9baf5f27d84f6fa06

Tier 3 Foundation v0 (read-only) is live: tier3Readiness, disposableWorktreePlan, mutationScope,
safeMutationPolicy, mutationEvidence + eight read-only panel sections. Effective tier is read-only;
no mutation lane is active.
Package : ide/vscode/a2-harness-panel/   Helper: scripts/a2-ide-harness.sh (the only panel-spawned binary)
Guards  : ide/vscode/a2-harness-panel/scripts/run-guards.js (authoritative; strips comments/strings)
```

You MUST verify actual file names and the current guard set yourself before editing.

---

## Hard Boundaries

Do NOT:

```text
create any disposable worktree in this lane (executor v0 is dry-run only)
write any file from the executor or the panel in this lane
place the executor inside the panel webview/extension (the panel stays read-only)
add a mutation/create/agent-run/agent-execute/apply/approve control to the panel
add fs use or a process spawn to the panel outside helperRunner
add network/broker/model/runtime calls or raw :11434 app inference
edit the control checkout or any real/live target
run live A2 (preview/approval/apply-bundle/apply)
push / open a PR / merge / delete a branch / remove a worktree by force
edit scripts/a2-ide-harness.sh (unless separately approved)
edit Rust code / schemas / runtime config / CI config
touch the install-smoke scope branch / local commit 448d7ea
clean disposable smoke/demo worktrees/workspaces
modify .claw artifacts / delete artifacts
touch Vault/secrets / print secrets
```

Destructive commands the implementer must NEVER run (described, not pasted, to keep this prompt
scan-clean):

```text
- the git working-tree clean operation (untracked-file removal)
- recursive force file removal (the rm recursive+force form)
- find used with -delete, or with an -exec removal
- git reset --hard
- force-deleting a branch (the -D form)
- worktree removal using the force flag
- git fetch --prune
- force-push to any remote
- git add . / git add -A (exact-path staging only)
```

Allowed:

```text
create a fresh isolated worktree from origin/main (for THIS implementation lane's own work)
read files; inspect repo/panel/helper/docs/tests
add a new external dry-run executor tool (a script/module) that performs NO worktree creation and NO
  writes, reusing the Foundation v0 pure models for its gates
add a read-only panel "Proposed Executor Plan" view that PRINTS the dry-run command + renders dry-run
  evidence (no spawn of the executor)
add unit tests
add runbook documentation + the implementation report
run npm install --ignore-scripts; npm run lint; npm run compile; npm test
commit exact approved files locally
```

---

## Fresh Worktree Setup

```text
Branch   : feat/a2-tier3-mutation-executor-v0-dryrun-20260608
Worktree : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-mutation-executor-v0-dryrun-20260608
```

Do not edit `/home/suki/stack-code` (control checkout only).

---

## Preflight

```bash
set -euo pipefail
SRC=/home/suki/stack-code
BRANCH=feat/a2-tier3-mutation-executor-v0-dryrun-20260608
WT=/mnt/vast-data/git-worktrees/stack-code-a2-tier3-mutation-executor-v0-dryrun-20260608
PR108_MERGE=8e9eed64927115d255c74bb9baf5f27d84f6fa06

git -C "$SRC" status -sb
test -z "$(git -C "$SRC" diff --cached --name-only)" || { echo "STOP: staged changes"; exit 1; }
test -z "$(git -C "$SRC" diff --name-only)"        || { echo "STOP: unstaged changes"; exit 1; }
git -C "$SRC" fetch origin main
git -C "$SRC" merge-base --is-ancestor "$PR108_MERGE" origin/main && echo "EXECUTOR_DESIGN_BASE_PRESENT"
test ! -e "$WT" || { echo "STOP: worktree exists"; exit 1; }
git -C "$SRC" show-ref --verify --quiet "refs/heads/$BRANCH" && { echo "STOP: branch exists"; exit 1; } || true
git -C "$SRC" worktree add -b "$BRANCH" "$WT" origin/main
```

STOP if: control checkout dirty; origin/main missing the executor design base; worktree or branch exists.

---

## Allowed Touched Surfaces

Verify actual filenames first. Conservative expectation:

```text
ide/vscode/a2-harness-panel/src/        new dry-run plan/model + read-only "Proposed Executor Plan" render
ide/vscode/a2-harness-panel/test/       unit tests for the dry-run gating + no-creation/no-write proof
docs/runbooks/a2-ide-extension-panel.md runbook additions for the read-only Proposed Executor Plan view
handoffs/a2_tier3_mutation_executor_v0_dryrun_implementation_report_2026-06-08.md
```

If an external executor tool needs a new file outside the panel (e.g. a script), STOP and report the
exact need and location for separate approval; do NOT assume a location and do NOT make it spawnable
from the panel.

Do NOT touch `scripts/a2-ide-harness.sh` in this lane.

---

## Implementation Phases

```text
Phase 0  Preflight + fresh worktree (above). STOP on any gate.
Phase 1  Inventory: read the Foundation v0 Tier 3 modules + guards + runbook; confirm filenames.
Phase 2  Dry-run plan model (pure): given an approved lane (objective + worktree plan + declared
         exact-path set), validate it against tier3Readiness / disposableWorktreePlan / mutationScope /
         safeMutationPolicy and produce a DRY-RUN RESULT (what it WOULD do) — NO creation, NO write.
Phase 3  Dry-run evidence shape (reuse mutationEvidence; mark every entry printed-not-run).
Phase 4  Read-only panel "Proposed Executor Plan" section: PRINTS the exact dry-run command and renders
         the dry-run result/evidence. No spawn of the executor; no new button that creates/writes.
Phase 5  Tests: dry-run validates/derives correctly; performs NO creation and NO write; denials win;
         a write outside the declared set is reported as would-be-rejected; render adds NO
         create/write/executor/agent-run control.
Phase 6  Runbook update for the read-only Proposed Executor Plan view.
Phase 7  Build headlessly: npm install --ignore-scripts; npm run lint; npm run compile; npm test.
Phase 8  Write the report. Commit exact approved files locally. No push.
```

---

## STOP Gates

```text
STOP if any change creates a disposable worktree or writes a file in this lane.
STOP if the executor is placed inside the panel, or the panel gains a spawn/fs/create/write surface.
STOP if any change adds a create/write/agent-run/agent-execute/apply/approve control.
STOP if any change adds network/broker/model/runtime/secret access or raw :11434 app inference.
STOP if denials do not win over the Tier-3 allowlist, or an out-of-scope write is not reported as
  would-be-rejected.
STOP if the static guards (run-guards.js) or unit tests fail.
STOP if changed files differ from the approved surfaces.
```

---

## Validation

```bash
cd "$WT/ide/vscode/a2-harness-panel"
npm install --ignore-scripts
npm run lint        # run-guards.js must PASS (single spawn boundary intact)
npm run compile     # tsc clean
npm test            # mocha green, incl. dry-run no-creation/no-write tests
cd "$WT"
git diff --name-only            # only approved surfaces
git diff --check
# Guard-surface scan (expect NO new forbidden surface in live panel src):
grep -RniE '\bfs\.|child_process|\bspawn\s*\(|\bfetch\s*\(|\bollama\b|11434|setInterval|setTimeout|createFileSystemWatcher' \
  ide/vscode/a2-harness-panel/src || echo "NO_NEW_FORBIDDEN_SURFACE"
# Note: denied-registry pattern strings + negation comments/tests may match broad scans; run-guards.js
# (strips comments/strings) is authoritative. Verify intent against the DIFF, not raw whole-file greps.
```

---

## Scope Check

```text
Changed files must be exactly the approved surfaces. Any other changed path => STOP and report, do not
commit. If an external executor script is required, STOP for separate approval rather than adding it.
```

---

## Report Format

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_MUTATION_EXECUTOR_V0_DRYRUN_IMPLEMENTATION
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
WHAT WAS ADDED:
  dry-run plan model:
  dry-run evidence shape:
  read-only Proposed Executor Plan view:
SAFETY:
  disposable worktree created:
  file written by executor/panel:
  executor placed in panel:
  create/write/agent-run/apply/approve control added:
  network/broker/model/runtime/secret/:11434 added:
  fs/spawn added to panel outside helperRunner:
  helper touched:
  install-smoke 448d7ea touched:
  destructive commands used:
VALIDATION:
  guards (run-guards.js):
  compile:
  unit tests:
  guard-surface scan:
  changed-file scope:
  git diff --check:
STOP GATES HIT:
NEXT BEST LANE:
```

---

## Next Lane

```text
Name      : Tier 3 Mutation Executor v0 (dry-run) Review / Push PR
Objective : review the dry-run executor + read-only Proposed Executor Plan view, push the branch, open
            a PR for operator review.
Why       : the dry-run executor must be reviewed before any separately-approved step enables actual
            disposable-worktree creation or scoped writes.
STOP gate : do not design or implement an actual write-capable executor step until this dry-run lane is
            merged AND a separate, explicitly-approved write-capable lane is opened; the panel stays
            read-only.
First step: open the PR for feat/a2-tier3-mutation-executor-v0-dryrun-20260608 after the headless
            guards + tests pass with evidence.
```
