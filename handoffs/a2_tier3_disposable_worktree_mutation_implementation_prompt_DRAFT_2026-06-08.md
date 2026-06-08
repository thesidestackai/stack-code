# A2 Tier 3 Foundation v0 — Implementation Prompt (DRAFT, 2026-06-08)

> DRAFT implementation prompt for the next code lane. Hand this to Claude Code only after the Tier 3
> scope package (`docs/a2-tier3-disposable-worktree-mutation-scope.md`) is reviewed, or after the
> operator explicitly skips the docs gate. This lane adds the Tier 3 readiness/state/render layer
> only — it adds NO mutation executor, NO worktree-creation control, and NO write capability.

---

## Role

You are operating as a careful Stack-Code local coding-agent safety implementer.

Your job is to implement **Tier 3 Foundation v0**: the readiness/state/render layer for the disposable
worktree mutation path, in the A2 IDE Extension Panel. You add the Tier 3 readiness model, the
disposable worktree plan model, the declared mutation scope model (exact-path, reject-outside), the
safe mutation policy model (denials-win + Tier-3 allowlist shape; classification only), the mutation
evidence ledger shape, and read-only panel sections — plus tests. You add **no** write capability.

---

## Objective

Move the cockpit one controlled step toward Tier 3 by adding the readiness/state/render layer, while
preserving every Foundation v0 invariant (single spawn boundary, no fs outside it, no
network/broker/model/secret/:11434 egress, no watcher/polling/timer, no Run-the-chain control, honest
status). No actual file write, no worktree creation, no executor.

Source of truth: `docs/a2-tier3-disposable-worktree-mutation-scope.md`.

---

## Current Proven State

```text
Merged on origin/main:
  PR #104  feat(a2): add local coding agent foundation        9e8781674ca38044210d5c615f4a6bce5ddd2a4b
  PR #105  docs(a2): record foundation v0 smoke evidence       15647ba9a429d150b4ca18c04fdec1164ca88182

Foundation v0 (read-only control plane) is live: permission tiers, denied command registry
(denials win), agent session, agent readiness (honest not-checked git), agent evidence ledger, five
read-only panel sections. Effective tier is read-only; mutation is disabled; no live A2 chain.
Package : ide/vscode/a2-harness-panel/   Helper: scripts/a2-ide-harness.sh (the only spawned binary)
Guards  : ide/vscode/a2-harness-panel/scripts/run-guards.js (authoritative; strips comments/strings)
```

You MUST verify actual file names and the current guard set yourself before editing.

---

## Hard Boundaries

Do NOT:

```text
add any actual file-writing capability to the panel (unless separately approved in a later lane)
add any actual git worktree creation from the panel (unless separately approved in a later lane)
add a mutation executor
add a worktree-creation button
edit source files outside the approved surfaces
add PR creation / branch deletion from the panel
add a Run-the-chain / agent-run / agent-execute control
add network/broker/model/runtime calls or raw :11434 app inference
add a new process spawn boundary or any fs use outside helperRunner
add a filesystem watcher, polling, or a background timer
edit scripts/a2-ide-harness.sh (unless separately approved)
edit Rust code / schemas / runtime config / CI config
push / open a PR / merge / delete a branch
touch the install-smoke scope branch / local commit 448d7ea
clean disposable GUI-smoke worktrees/workspaces or the Foundation v0 demo workspace
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
- git add . / git add -A (use exact-path staging only)
```

Allowed:

```text
create a fresh isolated worktree from origin/main
read files; inspect repo/panel/helper/docs/tests
add new TypeScript model/state modules + read-only render additions in the approved src surface
add unit tests in the approved test surface
add runbook documentation for the new sections
create the implementation report
run npm install --ignore-scripts; npm run lint; npm run compile; npm test
commit exact approved files locally
```

---

## Fresh Worktree Setup

Use a fresh isolated worktree from `origin/main`. Do not edit `/home/suki/stack-code` (control
checkout only).

```text
Branch   : feat/a2-tier3-disposable-worktree-mutation-foundation-20260608
Worktree : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-disposable-worktree-mutation-foundation-20260608
```

---

## Preflight

```bash
set -euo pipefail
SRC=/home/suki/stack-code
BRANCH=feat/a2-tier3-disposable-worktree-mutation-foundation-20260608
WT=/mnt/vast-data/git-worktrees/stack-code-a2-tier3-disposable-worktree-mutation-foundation-20260608
PR105_MERGE=15647ba9a429d150b4ca18c04fdec1164ca88182

git -C "$SRC" status -sb
test -z "$(git -C "$SRC" diff --cached --name-only)" || { echo "STOP: staged changes"; exit 1; }
test -z "$(git -C "$SRC" diff --name-only)"        || { echo "STOP: unstaged changes"; exit 1; }
git -C "$SRC" fetch origin main
git -C "$SRC" merge-base --is-ancestor "$PR105_MERGE" origin/main && echo "TIER3_SCOPE_BASE_PRESENT"
test ! -e "$WT" || { echo "STOP: worktree exists"; exit 1; }
git -C "$SRC" show-ref --verify --quiet "refs/heads/$BRANCH" && { echo "STOP: branch exists"; exit 1; } || true
git -C "$SRC" worktree add -b "$BRANCH" "$WT" origin/main
```

STOP if: control checkout dirty; origin/main missing the Tier 3 scope base; worktree or branch exists.

---

## Allowed Touched Surfaces

Verify actual filenames first. Conservative expectation:

```text
ide/vscode/a2-harness-panel/src/        new readiness/state model modules + read-only render additions
ide/vscode/a2-harness-panel/test/       unit tests for readiness, exact-path reject, denials-win
docs/runbooks/a2-ide-extension-panel.md runbook additions for the Tier 3 readiness sections
handoffs/a2_tier3_disposable_worktree_mutation_foundation_implementation_report_2026-06-08.md
```

Possible new modules (subject to source verification; do not assume):

```text
src/tier3Readiness.ts          honest Tier 3 readiness (clean control checkout / origin-main / plan valid)
src/disposableWorktreePlan.ts  intended worktree path + mutation branch (plan only; no creation)
src/mutationScope.ts           declared exact-path set; reject-outside logic (no writes)
src/safeMutationPolicy.ts      denials-win + Tier-3 allowlist shape (classification only)
src/mutationEvidence.ts        mutation ledger event shape + render
```

Do NOT touch `scripts/a2-ide-harness.sh` in this lane. If a helper change seems required, STOP and
report the exact need; do not edit the helper.

---

## Implementation Phases

```text
Phase 0  Preflight + fresh worktree (above). STOP on any gate.
Phase 1  Inventory: read the panel src/test, guards, runbook; confirm filenames and patterns.
Phase 2  Tier 3 readiness model (pure; honest tri-state; not-checked when unprobed; no git side effects).
Phase 3  Disposable worktree plan model (pure; intended path + mutation branch; NO creation).
Phase 4  Declared mutation scope model (pure; exact-path set; reject any path outside the set or under
         the control checkout; NO writes).
Phase 5  Safe mutation policy model (pure; denied-registry FIRST then Tier-3 allowlist; denials win;
         classification/display only — no executor).
Phase 6  Mutation evidence ledger shape + render (checkpoint/mutation/validation/decision; printed-not-run).
Phase 7  Read-only panel sections (scope §16): Tier 3 Readiness, Disposable Worktree Plan, Declared
         Touched Files, Mutation Approval Gate, Diff Summary (placeholder), Validation Results
         (placeholder), Rollback/Abandon guidance, Evidence Ledger. No buttons that write or create.
Phase 8  Tests: readiness tri-state; exact-path reject-outside + control-checkout-reject; denials win
         over Tier-3 allowlist; ledger printed-not-run; render includes the sections and adds NO
         write/create/executor control.
Phase 9  Runbook update for the Tier 3 readiness layer.
Phase 10 Build headlessly: npm install --ignore-scripts; npm run lint; npm run compile; npm test.
Phase 11 Write the report. Commit exact approved files locally. No push.
```

---

## STOP Gates

```text
STOP if any change adds an actual file write, worktree creation, or mutation executor.
STOP if any change adds a write/create/agent-run/agent-execute/apply/approve control.
STOP if any change adds network/broker/model/runtime/secret access or raw :11434 app inference.
STOP if any change adds fs use or a process spawn outside the single spawn boundary.
STOP if the exact-path model fails to reject a path outside the declared set or under the control checkout.
STOP if denials do not win over the Tier-3 allowlist in the policy model.
STOP if the static guards (run-guards.js) or unit tests fail.
STOP if changed files differ from the approved surfaces.
```

---

## Tests

```text
- Tier 3 readiness: renders not-checked when unprobed; honest yes/no when facts supplied; never
  green-by-default; dirty control checkout surfaces as a hard not-ready.
- Declared mutation scope: accepts a path inside the declared set within the disposable worktree;
  rejects a path outside the set; rejects any path resolving under the control checkout.
- Safe mutation policy: a denied-registry command is denied even when the Tier-3 allowlist permits it
  (denials win); a non-denied, allowlisted command classifies allowed; nothing is executed.
- Mutation evidence ledger: print/checkpoint steps marked printed-not-run; entries carry tier 3 +
  decision + reason.
- Render: includes the §16 sections; adds NO write/create/executor/agent-run control; preserves the
  existing field-setter ordering invariant; existing guard tests still pass unchanged.
```

---

## Validation

```bash
cd "$WT/ide/vscode/a2-harness-panel"
npm install --ignore-scripts
npm run lint        # run-guards.js must PASS (single spawn boundary intact)
npm run compile     # tsc clean
npm test            # mocha green, incl. new Tier 3 model tests
cd "$WT"
git diff --name-only            # confirm only approved surfaces changed
git diff --check                # whitespace/conflict clean
# Guard-surface scan (expect NO new forbidden surface in live src):
grep -RniE '\bfs\.|child_process|\bspawn\s*\(|\bfetch\s*\(|\bollama\b|11434|setInterval|setTimeout|createFileSystemWatcher' \
  ide/vscode/a2-harness-panel/src || echo "NO_NEW_FORBIDDEN_SURFACE"
# Note: denied-registry pattern strings + negation comments/tests may match broad scans; run-guards.js
# (strips comments/strings) is authoritative. Verify intent against the DIFF, not raw whole-file greps.
```

---

## Scope Check

```text
Changed files must be exactly the approved surfaces (new src models + tests + runbook + report; helper
only if separately approved). Any other changed path => STOP and report, do not commit.
```

---

## Report Format

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_FOUNDATION_V0_IMPLEMENTATION
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
WHAT WAS ADDED:
  Tier 3 readiness model:
  disposable worktree plan model:
  declared mutation scope model (exact-path reject):
  safe mutation policy model (denials win):
  mutation evidence ledger:
  read-only panel sections:
SAFETY:
  file-writing capability added:
  worktree-creation capability added:
  mutation executor added:
  write/create/agent-run/apply/approve control added:
  network/broker/model/runtime/secret/:11434 added:
  fs/spawn added outside single boundary:
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
Name      : Tier 3 Foundation v0 Review / Push PR
Objective : review the Tier 3 readiness/state/render layer, push the branch, open a PR for operator review.
Why       : the readiness layer must be reviewed before any lane that enables actual disposable-worktree
            writes or worktree creation is designed.
STOP gate : do not design or implement an actual mutation executor or a worktree-creation control until
            Tier 3 Foundation v0 is merged and a separate, explicitly-approved mutation lane is opened.
First step: open the PR for feat/a2-tier3-disposable-worktree-mutation-foundation-20260608 after the
            headless guards + tests pass with evidence.
```
