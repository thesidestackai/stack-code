# A2 Tier 3 — Tier-4 Packaging — Stage 1 Dry-Run Plan — Implementation Prompt (DRAFT, token-gated)

> **DRAFT / token-gated.** This is the future implementation prompt for the FIRST Tier-4 packaging
> increment: a **read-only `package-plan`** subcommand (Stage 1). It writes nothing, runs no git
> mutation, pushes nothing, opens no PR. It must not be executed until the required token is present
> AND this design scope + this DRAFT are reviewed and merged. Source of truth:
> `docs/a2-tier3-tier4-pr-packaging-design-scope.md`.

---

## 1. Required Approval Token

This implementation lane MUST NOT begin (must STOP before creating any worktree or writing any source)
unless the operator's prompt contains this EXACT token:

```text
APPROVED: Execute A2 Tier 3 Tier-4 packaging implementation
```

A real PR-open (a LATER, separate lane — Stages 3–4) additionally requires this distinct token, which
is NOT granted by the implementation token above:

```text
APPROVED: Open A2 Tier 3 isolated-mutation PR
```

This Stage-1 lane is read-only planning ONLY and never reaches the PR-open token's scope.

## 2. Role

You are a careful Stack-Code safety implementer. You add ONE read-only planning subcommand that
reasons over EXISTING apply evidence. You reuse the hardened `a2-plan-runner` write core and the
existing orchestrator; you add no write/checkpoint/rollback logic and no new executor.

OBSERVE → VERIFY → IMPLEMENT (read-only planner) → TEST → GUARD → VALIDATE → COMMIT → REPORT

## 3. Objective

Add a read-only `package-plan` subcommand to `scripts/a2-tier3-write-orchestrator.sh` (or a separately
named sibling under `scripts/`, name pre-approved) that, given a disposable worktree + the approved
lane + the apply evidence, PRINTS the packaging plan defined in the design scope §12 and asserts
`would_push=false` and `would_open_pr=false`. It performs NO git mutation: no add, no commit, no push,
no PR. It is the Stage-1 precondition for any future staging.

## 4. Source of Truth

```text
docs/a2-tier3-tier4-pr-packaging-design-scope.md                  (this design; §7,§12,§14,§16,§20)
docs/a2-tier3-write-executor-reconciliation.md                   (drive, don't duplicate)
scripts/a2-tier3-write-orchestrator.sh                           (existing lane; mirror its gate style)
ide/vscode/a2-harness-panel/src/{mutationScope,safeMutationPolicy,disposableWorktreePlan}.ts
                                                                 (authoritative exact-path/deny rules)
handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md  (apply evidence shape)
```

## 5. Hard Boundaries

The future implementation lane must explicitly:

```text
Do not mutate /home/suki/stack-code.
Do not write outside the disposable worktree (Stage 1 writes nothing at all).
Do not mutate non-allowlisted files.
Do not use git add .
Do not use git add -A
Do not use git clean
Do not use rm -rf
Do not use git reset --hard
Do not run git push, gh pr create, or gh pr merge in this Stage-1 lane.
Do not run apply, approve, apply-bundle, preview, validate-lane, apply-lane, collector, or orchestrator
  live chains.
Do not run apply without the apply/PR-open token (and never in this Stage-1 lane at all).
Do not touch runtime/model/broker/Vault.
Do not introduce raw :11434 app inference.
Do not capture approval phrases in a webview.
Do not hide writes behind read-only labels.
Do not edit rust/crates/a2-plan-runner/** (reuse it; never modify the write core).
Do not edit helperRunner.ts or scripts/a2-ide-harness.sh (the read-only panel boundary is unchanged).
Do not add a panel button or any panel execution control.
Do not push, open a PR, or merge for the implementation itself beyond the normal docs/code review flow,
  and never auto-merge.
```

## 6. Clean Worktree Setup

```text
Source repo : /home/suki/stack-code (control checkout; never edited; must be clean).
Branch      : feat/a2-tier3-tier4-package-plan-<date>
Worktree    : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-tier4-package-plan-<date>
Base        : origin/main
Preflight   : control checkout on main, clean (no staged/unstaged); fetch origin main; ff-only;
              branch/worktree collision checks; STOP on any unexpected state.
```

## 7. Discovery (read-only)

```text
Read scripts/a2-tier3-write-orchestrator.sh (gate helpers: classify_write, json_array, plan_paths,
  gate_validate_lane, drive_chain_for_plan) and mirror its discipline (artifact/status-based, no
  free-text log parsing; denials win; exact-path).
Read the apply evidence shape from the smoke closure + a real .claw tree (read-only).
Read mutationScope.ts / safeMutationPolicy.ts / disposableWorktreePlan.ts for the authoritative
  exact-path + deny-by-default rules to mirror.
```

## 8. Implementation Scope

```text
Add exactly ONE read-only subcommand: package-plan.
Inputs (flags, exact, no globs): --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>].
Behaviour:
  - verify (read-only) the §16 "before staging" preconditions: worktree under the worktree root, on a
    unique origin/main branch; control checkout clean; apply evidence complete; drift guard
    (only declared set changed); per-file after-hash == recorded after_sha256.
  - emit the packaging plan JSON (§12): worktree, branch, base, declaredPaths[], perFile hashes +
    applied marker, commit_message_preview, would_push=false, would_open_pr=false.
  - print the would-stage set and the EXACT operator commands that a LATER, separately-approved lane
    would run (git add -- <paths>; git commit; and — only under the PR-open token — push + gh pr create)
    WITHOUT running any of them.
  - fail-closed: any precondition/drift/hash failure → print cause, emit no plan-success, exit nonzero.
No git mutation of any kind. No file writes (plan goes to stdout; an optional evidence file may be
  written ONLY inside the disposable worktree's .claw/packaging/, never the control checkout — but
  Stage 1 may keep it stdout-only to stay write-free).
```

## 9. Worktree Isolation Contract

```text
Operate only against a disposable worktree under /mnt/vast-data/git-worktrees/.
Refuse the control checkout path. Refuse a worktree not under the worktree root. Refuse main /
  protected refs. Never write/stage/commit in /home/suki/stack-code. Never force-remove a worktree.
```

## 10. Target Allowlist Contract

```text
The declared exact-path set (lane declaredPaths) is the ONLY set the plan references for staging.
No glob, no git add ., no -A, no directory-wide add (Stage 1 stages nothing anyway).
Drift guard: `git -C <worktree> status --porcelain` must show ONLY the declared set (ignored .claw
  excepted); ANY out-of-set change → REFUSE. Reject declared paths matching secret/runtime/CI/Docker/
  systemd shapes.
```

## 11. Mutation Plan Contract

```text
The "plan" is a packaging plan over existing evidence (design §12); it proposes NO new target write.
would_push and would_open_pr MUST be false in this Stage-1 output.
```

## 12. Approval Contract

```text
Implementation token (this lane): APPROVED: Execute A2 Tier 3 Tier-4 packaging implementation
PR-open token (a LATER lane only): APPROVED: Open A2 Tier 3 isolated-mutation PR
The implementation token does NOT authorize any push/PR/merge. Stage 1 never pushes or opens a PR.
No token is ever captured in a webview.
```

## 13. Evidence Contract

```text
The package-plan output is itself read-only evidence: timestamp (stamped post-run, not invented
  mid-run), base SHA, branch, worktree, declared set, per-file before/after sha256 + applied marker,
  drift result, would-stage set, would_push=false, would_open_pr=false. Print it; claim nothing not
  shown by the evidence.
```

## 14. Tests Required

```text
Stdlib/bash-testable, no live chain:
  - argv audit: package-plan builds the exact read-only invocation; refuses unknown flags/globs.
  - precondition refusals: dirty control checkout; worktree not under root; main/protected branch;
    incomplete apply evidence; drift (out-of-set file); hash mismatch — each REFUSES with no mutation.
  - happy path on a FIXTURE worktree/evidence: emits a plan with would_push=false / would_open_pr=false
    and the declared set as would-stage; performs zero git mutation (assert worktree status unchanged).
  - asserts the subcommand runs NO git add/commit/push and NO gh.
If the orchestrator has a bash test harness, extend it; otherwise add a minimal bats/shell test that
  CI runs. (If a panel readiness view is added later, it is a SEPARATE lane with its own tests.)
```

## 15. Guard Scans Required

```text
- No new fs-write / network / process-spawn beyond the read-only git/gh-absent planner.
- No shell:true; array-argv only; no arbitrary subcommand forwarding.
- No git add . / -A / clean / reset --hard / rm -rf anywhere in the new code.
- No model/broker/runtime/Vault/:11434 reference.
- No panel execution control added; helperRunner / a2-ide-harness.sh unchanged.
- grep the new code for push|pr create|pr merge → must be PRINTED-for-operator strings only, never executed.
```

## 16. STOP Gates

```text
STOP if the implementation token is absent.
STOP if the control checkout is not on main or not clean.
STOP if asked to operate on /home/suki/stack-code.
STOP if the worktree is not under /mnt/vast-data/git-worktrees/ or not on a unique origin/main branch.
STOP if apply evidence is incomplete or drift / hash mismatch is detected (refuse, no mutation).
STOP if the lane would run ANY git mutation, push, gh pr create, or gh pr merge (Stage 1 is read-only).
STOP if any declared path matches a secret/runtime/CI/Docker/systemd shape.
STOP on any model/broker/runtime/Vault/:11434 reference.
```

## 17. Commit Rules

```text
Stage exact paths only (the new subcommand + its tests + any docs). Never git add . / -A.
One focused commit: "feat(a2): tier-4 packaging Stage 1 read-only package-plan".
Do not push or open a PR automatically beyond the normal review flow; never auto-merge.
```

## 18. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_TIER4_PACKAGING_STAGE1_PACKAGE_PLAN_IMPL
TOKEN PRESENT: yes/no
BRANCH / WORKTREE / BASE / COMMIT:
SCOPE: subcommand added: ; flags: ; read-only proven (zero git mutation):
PRECONDITION REFUSALS TESTED: dirty-checkout / not-under-root / protected-branch / incomplete-evidence
  / drift / hash-mismatch:
HAPPY PATH: plan emitted; would_push=false; would_open_pr=false; would-stage==declared set:
TESTS: ; GUARDS: ; git diff --check:
SAFETY: control checkout untouched ; no git mutation ; no push/PR/merge ; no model/broker/Vault/:11434 ;
  panel unchanged ; rust write core unedited:
STOP GATES HIT: none | details
NEXT BEST LANE: Stage 2 (stage+commit in worktree) — separately approved; then Stage 3 push
  (PR-OPEN TOKEN) → Stage 4 draft PR → Stage 5 human merge.
```

---

### DRAFT status

This DRAFT and its design scope (`docs/a2-tier3-tier4-pr-packaging-design-scope.md`) must be reviewed
and merged before any implementation lane is opened. Do not begin implementation without the exact
token `APPROVED: Execute A2 Tier 3 Tier-4 packaging implementation`. The panel stays read-only; the
Rust write core is reused, never modified; no push/PR/merge occurs at Stage 1.
