# A2 Tier-4 Lane B — First Live `package-pr` Draft-PR Smoke Closeout (2026-06-15)

> **Evidence freeze, docs-only.** Lane B opened exactly one real GitHub **DRAFT** PR
> (#145) from the Lane A applied + package-committed + package-pushed disposable
> fixture branch, proving the `package-pr` capability end-to-end against real
> GitHub. This document VERIFIES and DOCUMENTS that result. It modifies nothing:
> it does not merge, approve, mark ready, close, or edit PR #145, and it does not
> touch the fixture worktree/branch. Stage 5 (human merge) is NOT started.

---

## 1. Executive Summary

The full Tier-4 packaging ladder is now proven live, end-to-end:

```text
Lane A: real-TTY apply (SMOKE_NOTES.md) -> package-plan -> package-commit -> package-push
Lane B: package-pr -> opened ONE real GitHub DRAFT PR (#145), independently confirmed
        isDraft=true, idempotent on re-run, with NO merge/approve/ready/force/delete.
```

`package-plan -> package-commit -> package-push -> package-pr` are all exercised
against a real disposable mutation branch and real GitHub. The only outward action
was opening one DRAFT PR; everything else stayed gated and reversible.

## 2. Scope

```text
IN:  read-only verification of the Lane A fixture + PR #145, and freezing this evidence.
OUT: no package-pr/plan/commit/push/apply run; no PR modification (no merge/approve/
     ready/close/edit); no branch/worktree cleanup; no Stage 5; no runtime/model/
     broker/Vault; no raw :11434 app inference.
```

## 3. Source of Truth

```text
docs/a2-tier4-package-pr-live-smoke-readiness.md             live-smoke preconditions/gates (#143)
docs/a2-tier4-live-draft-pr-smoke-prerequisite-fixture-plan.md  fixture plan (#144)
handoffs/a2_tier4_live_draft_pr_smoke_prerequisite_fixture_prompt_DRAFT_2026-06-14.md  Lane A prompt (#144)
scripts/a2-tier3-write-orchestrator.sh                       package-{plan,commit,push,pr} (#142)
<fixture worktree>/.claw/packaging/lane-a-fixture-evidence.v1.json  Lane A frozen evidence
```

## 4. Lane A Fixture Consumed

```text
fixture worktree : /mnt/vast-data/git-worktrees/stack-code-a2-tier4-live-package-pr-smoke-20260615
fixture branch   : fixture/a2-tier4-live-package-pr-smoke-20260615
fixture HEAD     : 968934da49cdeea202903b2dd8c64af4717aed8b  (the package-commit)
package commit   : 968934da49cdeea202903b2dd8c64af4717aed8b
remote sha       : 968934da49cdeea202903b2dd8c64af4717aed8b  (origin == package-commit, non-force)
target file      : SMOKE_NOTES.md
target after sha256 : cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8
state (verified) : only declared file changed; tracked tree clean (only .claw untracked);
                   control checkout /home/suki/stack-code clean on main.
```

The disposable `SMOKE_NOTES.md` was written with the bytes of the tracked file
`examples/a2_l1a_valid_readonly_plan.yaml` (the proven 2026-06-10 smoke target); it
is a throwaway smoke fixture, NOT real content intended for `main`.

## 5. Lane B `package-pr` First Run Result

```text
command : a2-tier3-write-orchestrator.sh package-pr --worktree <fixture wt> --approved-lane <lane.json>
result  : exit 0 — opened ONE draft PR
pr_url  : https://github.com/thesidestackai/stack-code/pull/145
base    : main
head    : fixture/a2-tier4-live-package-pr-smoke-20260615
draft   : true     pr_opened : true     idempotent_existing : false
merged  : false    marked_ready : false
schema  : a2-tier4-package-pr.v0
```

## 6. Independent GitHub Verification

Verified directly via `gh pr view 145` (not inferred from the orchestrator output):

```text
number         : 145
url            : https://github.com/thesidestackai/stack-code/pull/145
state          : OPEN
isDraft        : true
mergedAt       : null
baseRefName    : main
headRefName    : fixture/a2-tier4-live-package-pr-smoke-20260615
headRefOid     : 968934da49cdeea202903b2dd8c64af4717aed8b   (== package commit)
reviewDecision : ""      (not approved)
title          : a2(tier4): isolated-mutation draft PR — fixture/a2-tier4-live-package-pr-smoke-20260615
```

## 7. Idempotency Result

```text
- A second AND third package-pr run against the same pushed branch returned exit 0
  with "a DRAFT PR already exists … idempotent no-op" and idempotent_existing=true,
  surfacing the SAME pr_url (#145).
- No second PR was created. `gh pr list --head fixture/…20260615 --state all` returns
  exactly one PR (#145, OPEN, draft=true).
```

## 8. Safety Boundaries (held)

```text
- no merge; this lane never merges (state OPEN, mergedAt null)
- no approval (reviewDecision "")
- no mark-ready (isDraft true)
- no force push; no tags; no ref delete; no branch deletion
- exactly one PR for the fixture head (no duplicates)
- control checkout untouched (clean on main)
- no runtime/model/broker/Vault access; no raw :11434 app inference
- Stage 5 (human merge) NOT started
```

## 9. What Was Not Done (by this closeout lane)

```text
- did not run package-pr / package-plan / package-commit / package-push / apply / validate-lane / apply-lane
- did not modify PR #145 (no merge/approve/ready/close/edit)
- did not delete any branch (local or remote) or remove any worktree
- did not push or open another PR (this closeout commit is local-only)
- did not clean the fixture worktree or its .claw evidence
```

## 10. Open Artifact State

```text
- PR #145 : OPEN draft on origin (the live-smoke proof artifact).
- remote branch origin/fixture/a2-tier4-live-package-pr-smoke-20260615 : present at 968934da (the PR head).
- fixture worktree : present, package-commit HEAD, .claw evidence + .claw/packaging/lane-a-fixture-evidence.v1.json.
- scratch inputs : /tmp/a2-tier4-lane-a-fixture-20260615/{lane.json,evidence.json,plan.yaml}.
All preserved pending the operator's decision (Section 11).
```

## 11. Operator Decision Options

```text
1. Keep PR #145 open for review.
2. Close PR #145 without merge in a separate explicit cleanup lane.
3. Proceed to a separate human-only Stage 5 merge decision lane.
4. Retain fixture worktree/branch until decision is made.
```

Note: PR #145 adds a throwaway `SMOKE_NOTES.md`; it is a mechanism proof, not content
for `main`. The expected disposition is to review then close (option 2), not merge —
but that is an explicit operator decision, performed in a separate lane.

## 12. Recommended Next Step

```text
Review PR #145 manually; do not merge automatically.
```

## 13. Evidence Appendix

```text
package-pr (a2-tier4-package-pr.v0): pr_url #145, base main, head fixture/…20260615,
  draft=true, pr_opened=true, merged=false, marked_ready=false, idempotent_existing
  flips true on re-run.
gh pr view 145: state OPEN, isDraft true, mergedAt null, base main,
  headRefOid 968934da49cdeea202903b2dd8c64af4717aed8b, reviewDecision "".
gh pr list --head fixture/a2-tier4-live-package-pr-smoke-20260615 --state all: count 1.
Lane A apply-result: outcome "applied", markers a2-l2b-write-applied / -validated;
  SMOKE_NOTES.md sha256 cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8.
```
