# A2 Tier-4 — Stage 4 Open Draft PR — Implementation Report (2026-06-12)

> Token-gated lane. Implemented Tier-4 **Stage 4** (`package-pr`) on a fresh
> disposable worktree from `origin/main`. Local commit only — NOT pushed, NO real
> GitHub PR opened. Source of truth:
> `docs/a2-tier3-tier4-pr-packaging-design-scope.md` (§6 Stage 4, §13 PR-OPEN
> token, §16 Validation, §20 STOP gates).

## Token

- Required: `APPROVED: Open A2 Tier 4 draft isolated-mutation PR`
- Present as the first non-empty line of the operator message, exact match. ✅

## Worktree

- repo (control checkout): `/home/suki/stack-code` (on `main`, clean; never edited)
- branch: `feat/a2-tier4-stage4-open-draft-pr-20260612`
- worktree: `/mnt/vast-data/git-worktrees/stack-code-a2-tier4-stage4-open-draft-pr-20260612`
- base: `origin/main` @ `c02c78e` (Stage 3 package-push merge `c02c78e8…` confirmed ancestor)

## Ladder state

| Stage | Command | State |
|------|---------|-------|
| 1 | `package-plan`   | merged (#138) — present on main |
| 2 | `package-commit` | merged (#140) — present on main |
| 3 | `package-push`   | merged (#141) — present on main |
| 4 | `package-pr`     | **this lane** — local commit only |
| 5 | merge PR         | human-only, never automated |

## What Stage 4 (`package-pr`) does

`package-pr --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]`:

1. Runs the SAME read-only `_tier4_gate_package` readiness gate as Stages 1–3
   (base/branch/approval/exact-path scope, denials win, drift guard, per-file
   after-hash == recorded `after.sha256`, apply-evidence presence, control
   checkout clean).
2. Re-derives the Stage-2 package-commit from the worktree HEAD: tree clean of
   tracked changes, HEAD has a parent, and HEAD changed EXACTLY the declared set.
3. **Requires the disposable branch to be ALREADY PUSHED (Stage 3)** to `origin`
   at the EXACT package-commit sha. Fail-closed if missing/unpushed or at a
   different sha — it performs NO push and NO force.
4. Resolves the base branch from the lane base (`origin/main` → `main`); refuses
   an empty/unstripped (ambiguous) base or a base equal to the head branch.
5. Opens a DRAFT PR via `gh pr create --draft …`. `--draft` is a **fixed element
   of the argv array**, so a non-draft PR cannot be produced here.
6. Idempotency: an existing OPEN PR for the branch is a no-op **only when it is
   itself a draft** (surfaces its URL, opens nothing new); a non-draft existing
   PR is **refused** (this lane never makes a PR ready).
7. Claims success ONLY on a real returned PR URL (never inferred).
8. Emits `a2-tier4-package-pr.v0` evidence JSON: `pr_opened=true`, `draft=true`,
   `merged=false`, `marked_ready=false`, plus `pr_url`, branch, base, declared
   files, package-commit sha.

It NEVER merges, approves, marks a draft ready-for-review, force-pushes, deletes
a branch, or touches the control checkout. Stage 5 (merge) stays human-only.

## Files changed

- `scripts/a2-tier3-write-orchestrator.sh` — new `A2_GH` constant (test-only
  override; defaults to `gh`); new `cmd_package_pr` + `package_pr_body` +
  `gh_pr_view_fields` + `emit_package_pr`; dispatch + usage + safety-footer
  updates; refined top-of-file safety invariant for Stages 3–4.
- `tests/shell/test_a2_tier3_write_orchestrator.sh` — hermetic Stage-4 suite with
  a fake `gh` shim (`A2_GH` override; no network, no real GitHub); happy path,
  evidence contract, idempotent-draft, non-draft-refuse, unpushed-refuse,
  stale-remote-sha-refuse, dirty-worktree-refuse, HEAD-diff-refuse, main-refuse,
  gh-missing-refuse, dirty-control-refuse, usage; static invariants pinning
  draft-only and forbidding any gh merge/ready/review/approve.

## Validation

- `bash -n` — clean (orchestrator + test).
- `shellcheck` — clean (orchestrator + test) — matches CI (`rust-ci.yml` shell-tests job).
- `bash tests/shell/test_a2_tier3_write_orchestrator.sh` — **132 passed, 0 failed**
  (Stages 1–3 unchanged + new Stage-4 cases).
- `git diff --check` — clean.
- safety scans: no forbidden runtime/service surface, no `11434`, no destructive
  command, no `gh pr merge`/`gh pr ready`/`gh pr review --approve`/`--fill-first`/`--web`,
  no auto-approve/hidden-apply.

## Safety

- No real GitHub PR opened. No PR merged/approved/marked-ready. No branch pushed
  by this lane (Stage 4 requires Stage 3's push to already exist). No force push.
  No remote branch deleted. No live A2 workflow / collector / orchestrator run.
  No model/broker/runtime/Vault call. No raw app-inference. No destructive
  commands. Control checkout untouched. Rust write core unedited. Panel untouched.

## Git

- Local commit only on `feat/a2-tier4-stage4-open-draft-pr-20260612`. NOT pushed.
- Next lane reviews this commit before any review PR is opened for the Stage-4
  code itself. The orchestrator's own real isolated-mutation draft PR is only ever
  opened by a later, operator-controlled smoke lane.
