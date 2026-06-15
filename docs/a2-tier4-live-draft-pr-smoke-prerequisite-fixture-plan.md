# A2 Tier-4 Live Draft-PR Smoke — Prerequisite Fixture Plan (Stage 0, docs-only)

> **Docs-only.** The `package-pr` implementation (#142) and the live-smoke
> readiness scope (#143) are merged on `main`. The live `package-pr` smoke cannot
> run yet because there is no current disposable applied + package-committed +
> package-pushed mutation branch to point it at. This document scopes the
> **prerequisite fixture chain** that produces such a branch. It implements
> nothing and **does not execute** apply, package-plan, package-commit,
> package-push, or package-pr; it opens no PR and creates no worktree mutation.
> Source of truth: `docs/a2-tier4-package-pr-live-smoke-readiness.md` (§4–§9) and
> `docs/a2-tier3-tier4-pr-packaging-design-scope.md`.

---

## 1. Executive Summary

```text
The package-pr implementation and live-smoke readiness scope are merged on main.
The live smoke must not run until a real disposable applied+pushed mutation branch exists.
This document scopes the prerequisite fixture only; it does not execute apply, commit, push, or PR open.
```

The fixture is a real disposable worktree that the EXISTING chain has applied a
minimal safe mutation into, then `package-commit`-ed and `package-push`-ed — i.e.
exactly the Stage-3 output state the live `package-pr` smoke consumes read-only.
Producing it requires a real-TTY apply, which is its own separately-approved lane;
this plan defines that lane's boundaries and a token-gated DRAFT prompt for it.

## 2. Current State

```text
main top                : 9ed0915
#142 package-pr impl     : merged at 47c474f
#143 readiness scope     : merged at 9ed0915
ladder on main           : package-plan / package-commit / package-push / package-pr
orchestrator interfaces  : apply-lane --approved-lane <lane.json> --dry-run-evidence <evidence.json> --plan <plan.yaml>
                           package-{plan,commit,push,pr} --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
approval grammar (apply) : `apply <step-id> <preview_sha256>` — human-typed at a real TTY (EXIT_TTY=7 off-TTY)
session cleanup          : both prior Tier-4 worktrees removed; local branches safe-deleted (`git branch -d`);
                           control checkout clean at 9ed0915; install-smoke 448d7ea + prior-session lanes untouched
current fixture          : NONE — no disposable applied+pushed mutation branch currently exists
```

## 3. Why a Fixture Is Required

The live `package-pr` smoke (readiness doc §4) refuses unless it is given a real
disposable worktree that:

```text
- is under /mnt/vast-data/git-worktrees/ on a unique branch from origin/main;
- carries COMPLETE .claw apply evidence (apply-bundle.json + l2b-checkpoints + payload after.sha256);
- has on-disk bytes whose re-hash == recorded after_sha256 (no drift);
- has HEAD == a clean package-commit (its diff equals EXACTLY the declared set);
- has that branch ALREADY PUSHED to origin at the EXACT package-commit sha.
```

No such worktree exists after cleanup, and `package-pr` deliberately NEVER applies,
commits, or pushes — those are Stages 1–3's job. So the smoke needs a fixture
produced by a real apply → package-commit → package-push run FIRST. This plan
defines that run so it can be executed safely, later, under its own approval.

## 4. Non-Goals

```text
No live smoke in this lane.
No package-pr run in this lane.
No real GitHub PR opened in this lane.
No apply in this lane.
No package-commit or package-push in this lane.
No package-plan run in this lane.
No Stage 5 (human merge) in this lane.
No runtime/model/broker/Vault access.
No raw :11434 app inference.
No disposable mutation worktree creation in this lane.
No source/script/test/CI edit.
```

## 5. Required Prerequisite Chain

The fixture is produced by ONE separately-approved fixture lane (NOT this docs
lane) that runs the EXISTING chain in this exact order:

```text
1. Create a fresh disposable mutation worktree from origin/main.
   NOTE: `apply-lane` performs this itself as its first action (it creates ONE
   disposable worktree under the worktree root on a unique origin/main branch).
   The operator does NOT pre-create it; pre-creating would collide with
   apply-lane's free-path requirement. Step 1 is therefore satisfied BY step 2.
2. Run a separately approved real-TTY apply lane to create a minimal safe mutation
   (apply-lane --approved-lane <lane.json> --dry-run-evidence <evidence.json>
   --plan <plan.yaml>), approving with the human-typed `apply <step-id>
   <preview_sha256>` grammar at a real terminal.
3. Validate the mutation scope (only the declared exact-path set changed; .claw
   excepted) — apply-lane + the gate already enforce this; re-confirm read-only.
4. Run package-plan --worktree <wt> --approved-lane <lane.json> (read-only sanity:
   worktree is package-ready; would_push=false / would_open_pr=false).
5. Run package-commit --worktree <wt> --approved-lane <lane.json> (ONE in-worktree
   commit of EXACTLY the declared set).
6. Run package-push --worktree <wt> --approved-lane <lane.json> (ONE exact
   non-force branch:branch push to origin at the package-commit sha).
7. Freeze evidence: worktree path, branch, package-commit SHA, remote branch +
   remote SHA (== package-commit SHA), and clean-tree confirmation.
8. Hand off to the future package-pr live smoke lane (a SEPARATE, PR-OPEN-token
   lane) which opens exactly one real DRAFT PR for that pushed branch.
```

## 6. Fixture Preconditions

```text
- control checkout /home/suki/stack-code clean and on main;
- origin/main fetched; the fixture worktree descends from origin/main;
- a unique branch name (never main/master/HEAD) and a free worktree path under
  /mnt/vast-data/git-worktrees/;
- a minimal SAFE mutation target: a disposable fixture file inside the worktree
  (e.g. a SMOKE_NOTES.md-style file in the declared set), never a production,
  runtime, secret, CI, systemd, or Docker path;
- a dry-run-ready evidence file (ready=true) and an operator-approved lane
  (operatorApproved=true) whose declaredPaths == the single safe target;
- gh authenticated for thesidestackai/stack-code (used only by the LATER smoke;
  the fixture lane itself opens no PR).
```

## 7. Fixture Creation Contract

```text
- The fixture lane runs the EXISTING chain ONLY; it adds no new write/checkpoint/
  rollback logic and edits no source/script/test/CI.
- Writes occur ONLY inside the fresh disposable worktree, via `claw plan apply`
  driven by apply-lane — never the control checkout, never a live target.
- The declared exact-path set is a single safe fixture file; deny-by-default scope
  + drift guard refuse anything else.
- Approval for the apply is human-typed at a real TTY (`apply <step-id>
  <preview_sha256>`); never composed, captured, faked, batched, or webview-entered.
- The lane STOPS after package-push and records evidence; it never opens a PR.
```

## 8. Apply-Lane Boundary

```text
The fixture requires a real apply, but this docs lane does not run apply.
The apply lane must have its own approval token and STOP gates.
The apply lane must mutate only a disposable target/safe fixture file, not production or runtime surfaces.
```

Additional apply-lane facts (from the merged orchestrator):

```text
- apply-lane requires a clean control checkout, origin/main, a free worktree path,
  and a REAL interactive terminal; off-TTY it fails closed (EXIT_TTY=7) and creates
  no worktree, writes nothing.
- It drives claw plan run (write-preview) → human-typed approve → apply-bundle →
  apply inside the disposable worktree, then STOPS. It NEVER pushes, opens a PR,
  or merges.
```

## 9. package-commit Boundary

```text
- package-commit stages EXACTLY the declared set (exact-path `git add --`; never
  `git add .` / `-A`) and makes ONE evidence-bound commit INSIDE the disposable
  worktree.
- It refuses a pre-staged index or a staged set != declared set.
- It NEVER pushes, opens a PR, merges, or touches the control checkout;
  pushed/pr_opened/merged are always false.
```

## 10. package-push Boundary

```text
- package-push pushes ONLY the exact disposable branch at the exact package-commit
  sha to origin with a NON-force branch:branch refspec.
- A same-sha remote is an idempotent no-op; a different-sha remote is refused
  (no force). It NEVER force-pushes, pushes tags, deletes refs, pushes main, opens
  a PR, merges, commits, or touches the control checkout.
- After it, the branch is in the Stage-3 state the live smoke consumes.
```

## 11. package-pr Live Smoke Boundary

```text
The live smoke requires the PR-OPEN token:
APPROVED: Open A2 Tier 3 isolated-mutation PR

It must open exactly one real DRAFT PR.
It must independently verify isDraft=true using gh pr view.
It must perform a second-run idempotency check.
It must never merge, approve, mark ready, force-push, or delete branches.
```

The live smoke is a SEPARATE, later lane (readiness doc §10). It is NOT this lane
and NOT the fixture lane. The fixture lane MUST NOT run package-pr and MUST NOT
reuse the PR-OPEN token.

## 12. Evidence Requirements

The fixture lane freezes (and the live smoke later consumes) this evidence record
(timestamp stamped post-run, never invented mid-run):

```text
- worktree path + unique branch + base (origin/main);
- declared exact-path set (the single safe fixture file);
- per-file before/after sha256 + applied markers (a2-l2b-write-applied/-validated);
- .claw apply evidence present (apply-bundle.json + l2b-checkpoints + after.sha256);
- package-commit SHA;
- remote branch + remote SHA (== package-commit SHA), pushed non-force;
- clean-tree confirmation (only .claw untracked) + control checkout clean;
- NO PR opened, NO merge, NO approve, NO push beyond the one non-force refspec.
```

A success is claimed ONLY with the evidence present — never inferred.

## 13. Rollback / Cleanup Contract

```text
- rollback = ABANDON the disposable worktree (operator action). Pre-push, the
  commit lives only on the disposable branch; abandoning discards it.
- post-push, if the fixture is rejected, the remedy is a human action (delete the
  pushed branch via a SEPARATE explicitly-approved lane) — never automated here.
- never `git worktree remove --force`, never `git branch -D`, never `git clean`,
  never `git reset --hard`, never force-push, never delete a remote branch in the
  fixture lane.
- the disposable worktree + its .claw evidence are PRESERVED until the live smoke
  consumes them; cleanup is a separate, non-force, explicitly-approved lane.
```

## 14. Idempotency Contract

```text
- package-push is idempotent: a remote already at the exact package-commit sha is
  a no-op; a different sha is refused (no force).
- The later live smoke is idempotent too: a second package-pr run against the same
  pushed branch must surface the SAME draft PR and open NO second PR; a pre-existing
  NON-draft PR for the branch is REFUSED (the smoke never makes a PR ready).
- Re-running the fixture lane against an existing worktree must not double-apply or
  double-commit; a fresh run uses a FRESH disposable worktree per the apply-lane
  per-run requirement.
```

## 15. STOP Gates

For the future fixture lane (each fail-closed):

```text
STOP if the fixture token is absent / not exact / not the first non-empty line.
STOP if the control checkout is dirty or not on main.
STOP if the apply approval is not human-typed at a real TTY (off-TTY EXIT_TTY=7).
STOP if the declared set is not a single safe disposable fixture file, or matches a
   secret/runtime/CI/Docker/systemd/production shape.
STOP if drift is present or any on-disk hash != recorded after_sha256.
STOP before opening any PR — the fixture lane runs NO package-pr.
STOP before merge/approve/ready — never in any lane here.
STOP before force-push / tag-push / ref-delete.
STOP on any model/broker/runtime/Vault/:11434 reference.
STOP if asked to reuse the PR-OPEN token (reserved for the later live smoke).
```

## 16. Risk Assessment

```text
Risk: the fixture lane opens a PR early.        Mitigation: fixture lane runs NO
  package-pr; PR-OPEN token is reserved for the separate live smoke.
Risk: a real apply mutates production/runtime.  Mitigation: single safe disposable
  fixture file in the declared set; deny-by-default scope + drift guard; writes only
  inside the disposable worktree via the existing chain.
Risk: approval is faked/batched.                Mitigation: human-typed apply grammar
  at a real TTY; off-TTY fail-closed (EXIT_TTY=7).
Risk: irreversible cleanup.                     Mitigation: rollback = abandon worktree;
  never force-remove / branch -D / clean / reset --hard; remote-branch deletion is a
  separate lane.
Risk: stale/duplicate fixture across reruns.    Mitigation: fresh disposable worktree
  per run; package-push idempotent on same sha, refuses different sha.
Risk: token confusion.                          Mitigation: fixture token
  (APPROVED: Build A2 Tier 4 live package-pr disposable fixture) is DISTINCT from and
  never substitutes the PR-OPEN token (APPROVED: Open A2 Tier 3 isolated-mutation PR).
```

## 17. Recommended Future Lanes

```text
Lane A (next, token-gated): A2 Tier-4 Live package-pr Disposable Fixture Build
  Token : APPROVED: Build A2 Tier 4 live package-pr disposable fixture
  Does  : real-TTY apply of one safe fixture file -> package-plan -> package-commit
          -> package-push; freeze evidence; STOP. Opens NO PR.
  Draft : handoffs/a2_tier4_live_draft_pr_smoke_prerequisite_fixture_prompt_DRAFT_2026-06-14.md

Lane B (after A): A2 Tier-4 package-pr First Live Draft-PR Smoke
  Token : APPROVED: Open A2 Tier 3 isolated-mutation PR
  Does  : run package-pr against Lane A's pushed branch -> open ONE real DRAFT PR ->
          independently confirm isDraft=true via gh pr view -> second-run idempotency
          check. Never merge/approve/ready.

Lane C (after B, human): Stage 5 — human review + merge of the draft isolated-mutation PR.
```

## 18. Final Recommendation

```text
Review and merge this plan + the DRAFT fixture prompt FIRST. Do NOT build the fixture
(Lane A) until both are merged and the operator supplies the fixture token. Do NOT run
the live smoke (Lane B) until Lane A's evidence is frozen and the operator supplies the
PR-OPEN token. The two tokens are distinct and never interchangeable.
```

---

## Appendix A — Source of Truth

```text
docs/a2-tier4-package-pr-live-smoke-readiness.md             live-smoke preconditions/gates (#143)
docs/a2-tier3-tier4-pr-packaging-design-scope.md             Tier-4 ladder + tokens
scripts/a2-tier3-write-orchestrator.sh                       apply-lane + package-{plan,commit,push,pr}
tests/shell/test_a2_tier3_write_orchestrator.sh             offline gate matrix (132 cases)
handoffs/a2_tier4_stage4_open_draft_pr_implementation_report_20260612.md  Stage-4 impl report
```

## Appendix B — Explicit Non-Goals (this note)

```text
No implementation. No apply / package-plan / package-commit / package-push / package-pr run.
No real GitHub PR opened. No worktree mutation created. No Stage 5. No source/script/test/CI edit.
No model/broker/runtime/Vault access. No raw :11434 app inference. No webview approval capture.
No worktree/branch cleanup. No control-checkout edit.
```
