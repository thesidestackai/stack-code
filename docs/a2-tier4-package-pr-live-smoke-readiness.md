# A2 Tier-4 `package-pr` — Live Smoke Readiness Scope (Stage 0, docs-only)

> **Docs-only readiness scope.** The Tier-4 packaging ladder
> (`package-plan` → `package-commit` → `package-push` → `package-pr`) is now
> merged and CI-covered, but Stage 4 (`package-pr`) has only ever run against a
> **hermetic fake `gh` shim** — never against real GitHub. This document scopes
> the FIRST genuinely-live exercise of `package-pr`: one operator-controlled smoke
> that opens exactly **one real DRAFT PR** from a real, already-applied,
> already-pushed disposable worktree. It implements nothing, edits no
> source/script/test/CI, creates no worktree, opens no PR, and runs no live chain.
> It is **Stage 0 (readiness) only**. Source of truth:
> `docs/a2-tier3-tier4-pr-packaging-design-scope.md` (§6 Stage 4, §13 tokens,
> §16 Validation, §20 STOP gates).

---

## 0. Why this scope

`package-pr` is the first packaging stage that performs an **outward-facing,
account-scoped** action: it calls `gh pr create --draft` against the real
`thesidestackai/stack-code` remote. Every prior stage was provably reversible by
abandoning a local worktree; a real PR is visible, notifies reviewers, and is
removed only by a human. That asymmetry is exactly why the live smoke must be its
own separately-approved, token-gated lane — never folded into the implementation
or merge-gate lanes that produced it.

This scope does NOT run that smoke. It defines the preconditions, the single unit
of work, the gates, the evidence, and the rollback so the eventual smoke lane is
unambiguous and fail-closed before anyone types the token.

## 1. Current State (verified on `origin/main`)

```text
ladder (merged + CI-covered on main @ the Stage-4 merge commit):
  package-plan   (Stage 1, #138)  read-only packaging plan; would_push/would_open_pr=false
  package-commit (Stage 2, #140)  exact-path stage + ONE in-worktree commit; no push/PR
  package-push   (Stage 3, #141)  ONE exact non-force branch:branch push; no PR
  package-pr     (Stage 4, #142)  open ONE DRAFT PR for an already-pushed branch; no merge

CI: rust-ci.yml `shell tests` runs shellcheck + bash -n + the offline gate matrix
    (132 cases incl. 11 Stage-4 cases) on every change to the orchestrator/test.
proof to date: Stage 4 exercised ONLY against a hermetic fake `gh` shim
    (A2_GH override) — NO real GitHub call has ever been made by package-pr.
```

## 2. North Star Gap

```text
Stage 4 code merged + CI-green  ✅
→ [THIS GAP] prove package-pr opens exactly ONE real DRAFT PR end-to-end against
   the real remote, from a real applied+pushed disposable worktree, under a fresh
   per-run PR-OPEN token — never merging, never approving, never marking ready,
   never opening a non-draft PR, never touching the control checkout.
→ Stage 5 (human merge of that isolated-mutation draft PR) — entirely human,
   entirely separate, never automated.
```

## 3. Non-Goals

```text
No implementation in this lane (Stage 0 / docs only).
No live package-pr / package-push / package-commit / package-plan run.
No apply / approval / apply-bundle / validate-lane / apply-lane run.
No real PR opened, merged, approved, or marked ready by THIS lane.
No worktree creation; no target write; no .claw mutation.
No control-checkout mutation. No source/script/test/CI edit.
No model/broker/runtime/Vault access. No raw :11434 app inference.
No webview approval capture. No worktree/branch cleanup.
```

## 4. Live-Smoke Preconditions (the smoke lane must verify, refuse otherwise)

```text
A real disposable worktree under /mnt/vast-data/git-worktrees/ that the EXISTING
chain already produced, carrying COMPLETE apply evidence, AND already pushed:
  (a) on a unique branch from origin/main (never main/master/HEAD);
  (b) .claw apply evidence present: apply-bundle.json + l2b-checkpoints + payload
      after.sha256; on-disk bytes re-hash == recorded after_sha256 (no drift);
  (c) working tree contains ONLY the declared exact-path set as changes
      (ignored .claw excepted) — any out-of-set change = REFUSE;
  (d) HEAD is the Stage-2 package-commit: a real commit with a parent whose diff
      equals EXACTLY the declared set (package-commit shape);
  (e) the branch is ALREADY PUSHED (Stage 3) to `origin` at the EXACT
      package-commit sha — missing/unpushed/different-sha = REFUSE (no push here);
  (f) the control checkout /home/suki/stack-code is clean;
  (g) `gh` is authenticated for thesidestackai/stack-code (read auth status only;
      never print the token).
```

Producing such a worktree (a real apply via `apply-lane` at a TTY, then
`package-commit`, then `package-push`) is itself a SEPARATE, separately-approved
chain run; the smoke lane consumes its output read-only and does not re-run it
unless the smoke prompt explicitly scopes the whole chain under its own approvals.

## 5. The Single Unit of Work (live smoke)

```text
1. Re-verify all §4 preconditions read-only; REFUSE (no PR) on any failure.
2. Run EXACTLY: a2-tier3-write-orchestrator.sh package-pr
       --worktree <real disposable worktree>
       --approved-lane <the approved lane.json>
       [--plan <plan.yaml>]
   under the fresh per-run PR-OPEN token (§6). This opens ONE real DRAFT PR.
3. Capture evidence: the emitted a2-tier4-package-pr.v0 JSON (pr_url, draft=true,
   pr_opened=true, merged=false, marked_ready=false) AND, independently,
   `gh pr view <pr_url> --json isDraft,state,mergeStateStatus` showing isDraft=true.
4. STOP. Do NOT merge, approve, or mark ready. Leave the draft PR for human review.
Out of this unit: merge (Stage 5, human-only), approve, ready-for-review,
   non-draft creation, any second PR, any branch/worktree cleanup.
```

## 6. Approval Contract

```text
PR-OPEN TOKEN (required, per-run, as the first non-empty line of the smoke prompt):
    APPROVED: Open A2 Tier 3 isolated-mutation PR

Rules:
  - Without the EXACT token, the smoke lane STOPS before running package-pr.
  - The token authorizes opening ONE DRAFT PR only — never a merge, an approval,
    a ready-for-review, or a non-draft PR.
  - The token is never composed, captured, faked, batched, or entered in a webview.
  - Token strings appearing inside the prompt body are not approval; only the
    first non-empty line counts (mirrors the Stage-4 implementation lane's gate).
```

## 7. Hard Gates (the smoke lane is fail-closed on each)

```text
STOP if the PR-OPEN token is absent / not exact / not the first non-empty line.
STOP if the control checkout is dirty or not on main.
STOP if the worktree is not under /mnt/vast-data/git-worktrees/ or not on a unique
   origin/main branch (never main/master/HEAD).
STOP if apply evidence is incomplete, drift is present, or any on-disk hash !=
   recorded after_sha256.
STOP if HEAD is not the clean package-commit (diff != declared set).
STOP if the branch is not already pushed at the exact package-commit sha
   (the smoke NEVER pushes and NEVER forces).
STOP if gh is unauthenticated (read auth status only).
STOP before merge — Stage 5 is human-only; never `gh pr merge`.
STOP before approve / ready — never `gh pr review`, never `gh pr ready`.
STOP if package-pr would (or did) open a non-draft PR, a second PR, or return no
   real URL — success is claimed ONLY on a real returned draft PR URL.
STOP on any model/broker/runtime/Vault/:11434 reference.
```

## 8. Evidence Contract

```text
The smoke lane emits a timestamped (stamped post-run, never invented mid-run)
evidence record:
  - PR-OPEN token present (yes) + exact (yes);
  - worktree, branch, base, declared exact-path set;
  - package-commit sha + the verified remote sha (equal);
  - the a2-tier4-package-pr.v0 JSON (pr_url, draft=true, pr_opened=true,
    merged=false, marked_ready=false, idempotent_existing as applicable);
  - an INDEPENDENT `gh pr view` confirmation that the opened PR isDraft=true and
    is OPEN (not merged, not ready);
  - the second-run idempotency check (re-running package-pr surfaces the SAME
    draft PR and opens no second PR);
  - control checkout clean (unchanged); no merge/approve/ready performed.
A success is claimed ONLY with a real returned draft PR URL — never inferred.
```

## 9. Rollback / Retention Contract

```text
rollback     : the only artifact is ONE draft PR. If the operator rejects it, the
               remedy is a HUMAN action (close the PR; optionally delete the branch
               via a SEPARATE explicitly-approved lane) — never automated here.
               Never `gh pr merge`, never force, never delete a remote branch in
               the smoke lane, never `worktree remove --force` / `branch -D` /
               `git clean` / `reset --hard`.
retention    : the disposable worktree + its .claw evidence are PRESERVED until the
               operator harvests evidence; cleanup is a separate, non-force,
               explicitly-approved lane.
idempotency  : a second smoke run against the same pushed branch must surface the
               SAME draft PR (no second PR); a pre-existing NON-draft PR for the
               branch must REFUSE (the smoke never makes a PR ready).
```

## 10. Recommended Future Smoke Lane (token-gated, NOT this lane)

```text
Name        : A2 Tier-4 package-pr First Live Draft-PR Smoke (token-gated)
Type        : live smoke; opens EXACTLY one real DRAFT PR; never merges/approves.
Token       : APPROVED: Open A2 Tier 3 isolated-mutation PR   (first non-empty line)
Objective   : prove package-pr opens one real draft PR end-to-end from a real
              applied+pushed disposable worktree, with independent gh-side
              confirmation of isDraft=true and a second-run idempotency check.
Recommended : Claude Code (phase-gated, STOP-gated, token-gated) + operator review.
Surfaces    : drives scripts/a2-tier3-write-orchestrator.sh package-pr only; no
              source/script/test/CI edit; control checkout untouched.
Mutation    : opens ONE real draft PR (the only outward action); no merge, no
              approve, no ready, no non-draft, no second PR, no push/force/delete.
Preconditions: a real disposable worktree satisfying §4 (produced by a separate,
              separately-approved apply → package-commit → package-push chain run).
STOP gate   : no token (exact, first non-empty line) → STOP before any package-pr
              run; never merge/approve/ready; never non-draft; gh auth read-only.
First step  : read-only — re-verify §4 preconditions and gh auth status; only then,
              with the token present, run package-pr against the real worktree.
```

## 11. Risk Assessment

```text
Risk: a real PR is merged/approved by automation.   Mitigation: smoke opens a DRAFT
  only; no gh pr merge / review / ready in package-pr (enforced by code + tests);
  Stage 5 is human-only.
Risk: a non-draft PR is opened.                      Mitigation: --draft is a fixed
  argv element in package-pr; a non-draft PR is impossible by construction.
Risk: the smoke pushes/force-pushes to fix a stale   Mitigation: package-pr NEVER
  remote head.                                          pushes; it REFUSES a
  missing/unpushed/different-sha remote head (push is Stage 3's job).
Risk: a second/duplicate PR per re-run.              Mitigation: an existing OPEN
  draft PR is an idempotent no-op; an existing non-draft PR is refused.
Risk: packaging from a dirty/wrong base.             Mitigation: control-clean gate;
  worktree-root + origin/main + unique-branch + package-commit-shape gates.
Risk: success claimed without a real PR.             Mitigation: success only on a
  real returned draft PR URL, independently re-confirmed via `gh pr view`.
Risk: token leakage / webview capture.               Mitigation: token is the first
  non-empty line at a real terminal; never composed/captured/printed; gh auth is
  read-only (never print the token).
```

---

## Appendix A — Source of Truth

```text
docs/a2-tier3-tier4-pr-packaging-design-scope.md      §6 Stage 4, §13 tokens, §16, §20
scripts/a2-tier3-write-orchestrator.sh                package-{plan,commit,push,pr}
tests/shell/test_a2_tier3_write_orchestrator.sh       offline gate matrix (132 cases)
handoffs/a2_tier4_stage4_open_draft_pr_implementation_report_20260612.md  Stage-4 impl report
```

## Appendix B — Explicit Non-Goals (this note)

```text
No implementation. No live chain run. No real PR opened/merged/approved/marked-ready.
No worktree creation. No target write. No .claw mutation. No control-checkout edit.
No source/script/test/CI edit. No model/broker/runtime/Vault access. No raw :11434
app inference. No webview approval capture. No worktree/branch cleanup.
```
