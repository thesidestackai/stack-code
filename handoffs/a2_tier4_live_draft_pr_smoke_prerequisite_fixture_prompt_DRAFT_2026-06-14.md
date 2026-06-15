# A2 Tier-4 Live package-pr Disposable Fixture — Build Prompt (DRAFT, token-gated)

> **DRAFT / token-gated. Do NOT run this yet.** This is the future execution
> prompt for **Lane A** — building the disposable applied + package-committed +
> package-pushed mutation branch that the later live `package-pr` smoke (Lane B)
> consumes. It must not be executed until this DRAFT and its plan
> (`docs/a2-tier4-live-draft-pr-smoke-prerequisite-fixture-plan.md`) are reviewed
> and merged, AND the operator supplies the fixture token below. This lane opens
> NO PR and runs NO package-pr; the PR-OPEN token is reserved for Lane B.

---

## 1. Required Approval Token

This lane MUST NOT begin unless the FIRST non-empty line of the operator message
is EXACTLY:

```text
APPROVED: Build A2 Tier 4 live package-pr disposable fixture
```

If the exact token is missing, STOP immediately and return:

```text
CLASSIFICATION: BLOCKED
REASON: missing exact fixture-build approval token
ACTIONS TAKEN: none
```

Token appearances inside this prompt are NOT approval; only the first non-empty
line counts.

This lane MUST NOT reuse the PR-OPEN token
`APPROVED: Open A2 Tier 3 isolated-mutation PR` — that token is reserved for the
later live `package-pr` smoke (Lane B) and grants no authority here.

## 2. Role

You are a careful Stack-Code fixture builder. You drive the EXISTING, merged chain
(`apply-lane` → `package-plan` → `package-commit` → `package-push`) to produce one
disposable, pushed, evidence-backed mutation branch. You add no new write logic and
edit no source/script/test/CI. You open NO PR.

OBSERVE → VERIFY TOKEN → PREFLIGHT → REAL-TTY APPLY (separate approval) →
PACKAGE-PLAN → PACKAGE-COMMIT → PACKAGE-PUSH → FREEZE EVIDENCE → REPORT

## 3. Objective

Produce the Stage-3 fixture state for Lane B:

```text
a disposable worktree under /mnt/vast-data/git-worktrees/ on a unique origin/main
branch, carrying complete .claw apply evidence for ONE safe fixture file, whose
HEAD is a clean package-commit, and whose branch is pushed to origin at the exact
package-commit sha — with that evidence frozen for the live smoke. No PR opened.
```

## 4. Source of Truth

```text
docs/a2-tier4-live-draft-pr-smoke-prerequisite-fixture-plan.md   THIS lane's plan (§5–§16)
docs/a2-tier4-package-pr-live-smoke-readiness.md                 live-smoke preconditions Lane B needs (#143)
docs/a2-tier3-tier4-pr-packaging-design-scope.md                 Tier-4 ladder + tokens
scripts/a2-tier3-write-orchestrator.sh                           apply-lane + package-{plan,commit,push}
```

## 5. Hard Boundaries

Do NOT:

```text
run package-pr
open a real GitHub PR
merge any PR
approve any PR
mark any PR ready
reuse the PR-OPEN token
force push
push tags
delete a remote branch
delete a local branch with -D
git worktree remove --force
git clean / rm -rf / git reset --hard / git add . / git add -A
edit source / scripts / tests / CI
touch runtime / services / HQ / Vault / secrets
call model / broker / /v1/chat/completions / /status/vram
introduce raw :11434 app inference
mutate the control checkout /home/suki/stack-code
mutate any production/runtime/CI/Docker/systemd path (the fixture target is a single safe disposable file)
touch install-smoke 448d7ea branch/worktree
touch preserved prior-session artifacts
```

## 6. Separate Apply Approval

The real apply is itself a gated action. Before any real apply:

```text
- the operator must explicitly approve the apply at a REAL interactive terminal;
- approval is the human-typed grammar `apply <step-id> <preview_sha256>` — never
  composed, captured, faked, batched, or webview-entered;
- off-TTY, apply-lane fails closed (EXIT_TTY=7), creates no worktree, writes nothing;
- the fixture token in §1 authorizes BUILDING the fixture lane flow; it does not
  substitute the human-typed per-step apply approval.
```

## 7. Preflight (read-only)

```text
- control checkout on main, no staged/unstaged tracked changes;
- fetch origin main; ff-only awareness;
- a unique branch name + free worktree path under /mnt/vast-data/git-worktrees/
  (NOTE: apply-lane creates the worktree itself — do NOT pre-create it);
- prepare an operator-approved lane.json (operatorApproved=true; worktreePlan
  {worktreePath, branch, base=origin/main}; declaredPaths == ONE safe fixture
  file), a dry-run-ready evidence.json (ready=true), and a plan.yaml whose
  workspace-write target == that single declared file;
- STOP on any collision or unexpected tracked/staged state.
```

## 8. Build Steps (drive the EXISTING chain only)

```text
1. apply-lane --approved-lane <lane.json> --dry-run-evidence <evidence.json>
   --plan <plan.yaml>   (REAL TTY; human-typed apply approval; creates the
   disposable worktree, applies ONE safe file, STOPS). Opens NO PR.
2. package-plan --worktree <wt> --approved-lane <lane.json>   (read-only sanity;
   would_push=false / would_open_pr=false).
3. package-commit --worktree <wt> --approved-lane <lane.json>   (ONE in-worktree
   commit of EXACTLY the declared set).
4. package-push --worktree <wt> --approved-lane <lane.json>   (ONE exact non-force
   branch:branch push to origin at the package-commit sha).
5. Freeze evidence (§10). STOP. Do NOT run package-pr. Do NOT open a PR.
```

## 9. Forbidden in This Lane

```text
- package-pr (Lane B only).
- opening, merging, approving, or marking ready any PR.
- the PR-OPEN token.
- force push / tag push / ref delete / remote-branch delete.
- any source/script/test/CI edit.
- any runtime/model/broker/Vault/:11434 access.
```

## 10. Evidence To Freeze (for Lane B)

```text
- worktree path + unique branch + base (origin/main);
- declared exact-path set (the single safe fixture file);
- per-file before/after sha256 + applied markers; .claw apply-bundle + checkpoints
  + after.sha256 present; on-disk re-hash == recorded after_sha256 (no drift);
- package-commit SHA; remote branch + remote SHA (== package-commit SHA), pushed
  non-force; clean-tree confirmation (only .claw untracked); control checkout clean;
- explicit: NO PR opened, NO merge/approve/ready, NO push beyond the one non-force refspec.
```

## 11. STOP Gates

```text
STOP if the fixture token is absent / not exact / not the first non-empty line.
STOP if the control checkout is dirty or not on main.
STOP if the apply approval is not human-typed at a real TTY (off-TTY EXIT_TTY=7).
STOP if the declared set is not a single safe disposable fixture file or matches a
   secret/runtime/CI/Docker/systemd/production shape.
STOP if drift is present or any on-disk hash != recorded after_sha256.
STOP before any package-pr run, any PR open, any merge/approve/ready.
STOP before force-push / tag-push / ref-delete / remote-branch delete.
STOP on any model/broker/runtime/Vault/:11434 reference.
STOP if asked to reuse the PR-OPEN token.
```

## 12. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER4_LIVE_PACKAGE_PR_DISPOSABLE_FIXTURE_BUILD
TOKEN: fixture token present (yes/no) + exact (yes/no); PR-OPEN token reused (must be NO)
WORKTREE: worktree / branch / base / package-commit sha / remote sha
EVIDENCE: declared file / .claw evidence present / no drift / pushed non-force
SAFETY: package-pr run (no) / PR opened (no) / merged (no) / approved (no) /
  ready (no) / force-push (no) / remote-branch deleted (no) / runtime touched (no) /
  Vault/secrets (no) / raw 11434 (no) / control checkout mutated (no)
HANDOFF: evidence frozen for Lane B (the package-pr live smoke, PR-OPEN token)
NEXT BEST LANE: A2 Tier-4 package-pr First Live Draft-PR Smoke (PR-OPEN token)
```

---

### DRAFT status

This DRAFT and its plan (`docs/a2-tier4-live-draft-pr-smoke-prerequisite-fixture-plan.md`)
must be reviewed and merged before Lane A is opened. Do not begin without the exact
token `APPROVED: Build A2 Tier 4 live package-pr disposable fixture`. This lane opens
no PR and never reuses the PR-OPEN token.
