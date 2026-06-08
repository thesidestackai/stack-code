# A2 Local Coding Agent Foundation v0 — Implementation Prompt (DRAFT, 2026-06-08)

> DRAFT implementation prompt for the next lane. Hand this to Claude Code only after the
> scope package (`docs/a2-local-coding-agent-foundation-scope.md`) is reviewed, or after
> the operator explicitly opts to skip docs review. This lane adds the foundation
> UI/state model only — it adds no new mutation capability.

---

## Role

You are operating as a senior local coding-agent implementer for Stack-Code.

Your job is to implement **A2 Local Coding Agent Foundation v0**: a permissioned control-
plane *foundation* in the A2 IDE Extension Panel. You add the agent session model, the
permission tier model and display, a read-only repo/git readiness detector, a dirty
worktree detector, the safe command allowlist model + denied command registry, and the
agent evidence ledger schema/render — plus tests. You add **no** new mutation capability.

---

## Objective

Move the A2 panel from a read-only print/validate cockpit toward a local coding-agent
cockpit by adding the foundation UI/state model, while preserving every existing safety
invariant (single spawn boundary, no `fs` outside that boundary, no network/broker/model/
secret/:11434 egress, no watcher/polling/timer, no Run-the-chain buttons, honest status).

Source of truth: `docs/a2-local-coding-agent-foundation-scope.md`.

---

## Context

```text
Proven, merged on origin/main:
  PR #101  feat(a2): add workspace-first panel UX           affedf999ef69d26ad8b32c4a22d6357f4a08e2b
  PR #102  docs(a2): record workspace-first UX smoke evid.  35da6a11dc2bc16182749e0bbfd241d03764d2d7

Package : ide/vscode/a2-harness-panel/   (TypeScript; mocha unit tests; static guards)
Helper  : scripts/a2-ide-harness.sh      (print/validate only; the only spawned binary)
Guards  : ide/vscode/a2-harness-panel/scripts/run-guards.js  (+ test/guards.test.ts)
Runbook : docs/runbooks/a2-ide-extension-panel.md

Current capability is read-only / print-validate (Tier 0–2 in the scope's tier model).
A live artifact-backed preview/approval/apply GUI chain is NOT proven and is out of scope.
```

You MUST verify actual file names and the current guard set yourself before editing.

---

## Hard Boundaries

Do NOT:

```text
add any new mutation capability in this lane
edit source files outside the approved surfaces
add file editing of repo files from the panel
add PR creation from the panel
add branch creation/deletion from the panel
add runtime/model/broker calls
add live :11434 app inference
add preview/approval/apply-bundle/apply execution
add a Run-the-chain button
add a filesystem watcher, polling, or a background timer
add auto-run on panel open
add network/telemetry/analytics egress
add secret-storage access
compose or capture an approval line
spawn any process outside the single spawn boundary (helperRunner)
use fs outside the single spawn boundary
push
open a PR
merge
delete any branch
touch the install-smoke scope branch / local commit 448d7ea
clean disposable GUI-smoke worktrees/workspaces
modify .claw artifacts
delete artifacts
print secrets
touch Vault/secrets
```

Destructive commands the implementer must NEVER run (described, not pasted verbatim, so
this prompt stays scan-clean):

```text
- recursive force file removal (the rm recursive+force form)
- the git working-tree clean operation (untracked-file removal)
- find used with -delete, or with an -exec removal
- git reset --hard
- git add . / git add -A
- force-deleting a branch (-D)
- worktree removal using the force flag
- git fetch --prune
- force-push to any remote
```

Allowed:

```text
read files; inspect repo/panel/helper/docs/tests
add new TypeScript modules + render additions in the approved src surface
add unit tests in the approved test surface
add runbook documentation for the new sections
create the implementation report
commit the exact approved files locally
build + lint + unit-test the package headlessly (no GUI, no chain)
```

---

## Clean Worktree Requirement

Use a fresh isolated worktree from `origin/main`. Do not edit `/home/suki/stack-code`
(control checkout only).

```text
Branch   : feat/a2-local-coding-agent-foundation-v0-20260608
Worktree : /mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-20260608
```

---

## Preflight

```bash
set -euo pipefail

SRC=/home/suki/stack-code
BRANCH=feat/a2-local-coding-agent-foundation-v0-20260608
WT=/mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-20260608

PR101_MERGE=affedf999ef69d26ad8b32c4a22d6357f4a08e2b
PR102_MERGE=35da6a11dc2bc16182749e0bbfd241d03764d2d7

echo "===== control checkout status ====="
git -C "$SRC" status -sb
git -C "$SRC" diff --cached --stat
git -C "$SRC" diff --stat

test -z "$(git -C "$SRC" diff --cached --name-only)" || { echo "STOP: staged changes in control checkout"; exit 1; }
test -z "$(git -C "$SRC" diff --name-only)"        || { echo "STOP: unstaged tracked changes in control checkout"; exit 1; }

echo "===== fetch origin main ====="
git -C "$SRC" fetch origin main

echo "===== verify workspace-first UX + evidence present ====="
git -C "$SRC" merge-base --is-ancestor "$PR101_MERGE" origin/main && echo "PR101_PRESENT"
git -C "$SRC" merge-base --is-ancestor "$PR102_MERGE" origin/main && echo "PR102_PRESENT"

echo "===== branch/path collision checks ====="
test ! -e "$WT" || { echo "STOP: target worktree exists: $WT"; exit 1; }
git -C "$SRC" show-ref --verify --quiet "refs/heads/$BRANCH" && { echo "STOP: branch exists: $BRANCH"; exit 1; } || true

echo "===== worktree list ====="
git -C "$SRC" worktree list
```

STOP if: control checkout dirty; origin/main cannot be fetched; PR #101 or #102 missing;
target worktree exists; branch exists.

Then create the worktree:

```bash
git -C "$SRC" worktree add -b "$BRANCH" "$WT" origin/main
cd "$WT"
git status -sb
git log --oneline -8
```

---

## Allowed Touched Surfaces

Verify actual filenames first; do not assume. Conservative expectation:

```text
ide/vscode/a2-harness-panel/src/        new model/state modules + render additions
ide/vscode/a2-harness-panel/test/       unit tests for permission tiers + denied registry
docs/runbooks/a2-ide-extension-panel.md runbook additions for the new sections
handoffs/a2_local_coding_agent_foundation_v0_implementation_report_2026-06-08.md
```

Only if the git readiness probe is implemented via a new read-only helper subcommand AND
the operator separately approves it: `scripts/a2-ide-harness.sh`. Prefer the read-only
VS Code Git extension API instead (no `fs`, no spawn, no network) so the script and the
single spawn boundary stay untouched. If the git probe cannot be implemented without
violating a guard, render the readiness fields as `not-checked` and defer the live probe
to a follow-on lane — do not fabricate git state.

---

## Phases

```text
Phase 0  Preflight + fresh worktree (above). STOP on any gate.
Phase 1  Inventory: read the panel src/test, helper, runbook, guards; confirm filenames.
Phase 2  Add the agent session model (in-memory, session-local; the scope §7 shape).
Phase 3  Add the permission tier model (Tier 0–5) + current-tier display (read-only).
Phase 4  Add the read-only repo/git readiness detector + dirty worktree detector +
         prominent dirty-checkout warning. Honest tri-state, never green-by-default.
Phase 5  Add the safe command allowlist model (allowlist-by-tier, labels only) and the
         global denied command registry (scope §8). Denials win over allowlists.
Phase 6  Add the agent evidence ledger schema + render (structured; printed-not-run
         markers; exit codes). No persistence in v0.
Phase 7  Add panel sections: Agent Readiness, Proposed Next Agent Lane.
Phase 8  Add unit tests for the permission tier model and the denied command registry
         (every tier; denied registry blocks the destructive families regardless of tier).
Phase 9  Update the runbook for the new sections.
Phase 10 Build headlessly: npm install --ignore-scripts; npm run lint (guards);
         npm run compile; npm test. All must pass. No GUI, no chain.
Phase 11 Write the implementation report. Commit exact approved files locally. No push.
```

---

## STOP Gates

```text
STOP if the control checkout is dirty, or the worktree/branch already exists.
STOP if PR #101 or #102 is missing from origin/main.
STOP if any change adds a new mutation capability, a Run-the-chain button, auto-run, a
  watcher, polling, or a background timer.
STOP if any change adds network/broker/model/runtime/secret access or a :11434 call.
STOP if any change adds fs use or a process spawn outside the single spawn boundary.
STOP if the git readiness probe cannot be done without violating a guard (render
  not-checked and defer instead).
STOP if the static guards (run-guards.js / guards.test.ts) fail.
STOP if any unit test fails or compile fails.
STOP if changed files differ from the approved surfaces.
```

---

## Validation

```bash
cd "$WT"

echo "===== changed files ====="
git status --short
git diff --name-only
git diff --stat

echo "===== package guards + tests (headless; no GUI, no chain) ====="
cd ide/vscode/a2-harness-panel
npm install --ignore-scripts
npm run lint     # static guards must PASS
npm run compile  # tsc clean
npm test         # mocha unit tests green (incl. new tier + denied-registry tests)
cd "$WT"

echo "===== guard-surface scan (no new forbidden surface in src) ====="
# Expect NO matches in live src (helperRunner remains the only spawn boundary):
grep -RniE '\bfs\.|child_process|\bspawn\s*\(|\bfetch\s*\(|\bollama\b|11434|setInterval|setTimeout|createFileSystemWatcher' \
  ide/vscode/a2-harness-panel/src || echo "NO_NEW_FORBIDDEN_SURFACE"

echo "===== changed-file scope check ====="
# Compare git diff --name-only against the approved surface list; STOP on drift.

echo "===== lint check ====="
git diff --check
```

---

## Tests

```text
- permission tier model: every Tier (0–5) has the expected allowed/denied shape; raising
  a tier is explicit; default tier is read-only.
- denied command registry: blocks each destructive family (recursive force removal, git
  working-tree clean, find -delete / -exec removal, git reset --hard, git add . / -A,
  force-deleting a branch (-D), worktree force removal, git fetch --prune) regardless
  of granted tier;
  denials win over allowlists.
- safe command allowlist: only allowlisted commands pass at a given tier; read-only
  helper subcommands map to Tier 2.
- agent session model: holds no secret; defaults tier to read-only; serializes to the
  ledger shape.
- evidence ledger: print-only steps recorded as printed-not-run; entries carry tier,
  command, decision, exit code.
- existing guard tests still pass unchanged (single spawn boundary intact).
```

---

## Scope Check

```text
Changed files must be exactly the approved surfaces (src modules + tests + runbook +
report; helper script only if separately approved). Any other changed path => STOP and
report, do not commit.
```

---

## Report Format

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_LOCAL_CODING_AGENT_FOUNDATION_V0_IMPLEMENTATION
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
WHAT WAS ADDED:
  agent session model:
  permission tier model + display:
  repo/git readiness detector:
  dirty worktree detector + warning:
  safe command allowlist model:
  denied command registry:
  evidence ledger schema/render:
  panel sections (Agent Readiness / Proposed Next Agent Lane):
GIT READINESS APPROACH: VS Code Git API | helper subcommand (approved?) | deferred not-checked
SAFETY:
  new mutation capability added:
  source mutated outside disposable worktree:
  Run-the-chain button added:
  watcher/polling/timer added:
  network/broker/model/runtime/secret/:11434 added:
  fs/spawn added outside single boundary:
  install-smoke scope (448d7ea) touched:
  disposable smoke cleanup performed:
  destructive commands used:
VALIDATION:
  changed-file scope:
  guards (run-guards.js):
  compile:
  unit tests:
  guard-surface scan:
  git diff --check:
STOP GATES HIT: none | details
NEXT BEST LANE:
```

---

## Next-Best-Lane Recommendation

```text
Name      : A2 Local Coding Agent Foundation v0 Review / Push PR
Objective : review the v0 foundation, push the branch, open a PR for operator review.
Tool      : Claude Code (review/push lane) + operator review.
Why       : the foundation adds a permission/control-plane model; it must be reviewed
            before any tier raising or mutation lane (Tier 3+) is designed.
Touched   : none beyond this branch's approved files.
Mutation  : none in the review lane; the v0 foundation itself adds no new mutation.
STOP gate : do not design or implement Tier 3 (disposable worktree mutation) until v0 is
            merged and a separate, explicitly-approved mutation lane is opened.
First step: open the PR for `feat/a2-local-coding-agent-foundation-v0-20260608` after the
            headless guards + tests pass with evidence.
```
