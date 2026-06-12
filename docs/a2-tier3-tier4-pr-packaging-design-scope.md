# A2 Tier 3 — Tier 4 Isolated-Mutation PR Packaging — Design Scope (Stage 0, docs-only)

> **Docs-only design.** This document scopes the NEXT genuinely-new write-capable bridge —
> packaging an already-applied, operator-approved, evidence-backed **isolated mutation** (produced
> inside a disposable worktree by the existing chain) into a **reviewable pull request**. It
> implements nothing, edits no source/Rust/script/test/CI, creates no worktree, writes no target,
> opens no PR, and runs no live chain. It is **Stage 0 (design) only**.

---

## 0. Why this scope (and why it is NOT a new executor)

A prior lane's framing assumed Stack-Code "still does not safely perform write-capable executor
work." Inventory of `origin/main @ 9e57087` shows that is **already false**:

```text
ALREADY MERGED + LIVE-SMOKE-PROVEN on origin/main:
- rust/crates/a2-plan-runner/src/write_executor.rs (1725 lines) + checkpoint.rs (864) + write_preview
  / diff_preview / approval / approval_ux / write_payload / write_runtime  + l2b_* test suites
  → the hardened single-file write authority (authority chain, atomic write, post-write re-hash,
    bounded rollback). `claw plan apply` is the one and only target-writing command.
- scripts/a2-tier3-write-orchestrator.sh (783 lines)  → the disposable-worktree LANE: validate the
  operator-approved lane, create one worktree from origin/main, enforce exact-path scope + denials-win,
  drive PREVIEW → human APPROVE → APPLY inside the worktree, gather evidence, STOP.
- docs/a2-tier3-mutation-executor-write-capable-design-scope.md (PR #112)  → the merged design.
- docs/a2-tier3-write-executor-reconciliation.md (PR #113)  → "drive, don't duplicate the Rust core".
- handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md  → one end-to-end live apply
  succeeded (wrote SMOKE_NOTES.md inside a disposable worktree; control checkout stayed clean).
```

The reconciliation note (merged) explicitly warns: *"Do NOT build a new write executor… Building a
new parallel executor would duplicate — and risk diverging from — an already-hardened safety
surface."* This scope honours that: it adds **no** write/checkpoint/rollback logic and **no** new
executor. It scopes only the **packaging step that explicitly Stays Out today** — the merged design's
**§16 "What Stays Out (Tier 4): stage + commit + open a PR from the disposable worktree."**

The genuine remaining North-Star gaps, from the merged closeouts, are: (a) **Tier 4 PR packaging**
(this scope), (b) multi-file declared-set live-apply *proof* (the orchestrator already gates a set;
only one file is smoke-proven), (c) `apply-result.json` persistence, (d) panel mutation-readiness
display. This scope targets (a) — the cleanest, unambiguously-unbuilt, additive next bridge.

---

## 1. Executive Summary

```text
Tier 3 read-only observability is complete and merged.
Tier 3 isolated single-file write/apply is built, hardened, and live-smoke-proven (inside a
  disposable worktree; control checkout stays clean; human-typed approval at a real terminal).
The next North Star bridge is Tier 4: turn a proven isolated mutation into a REVIEWABLE PR —
  without ever auto-merging, without ever touching the control checkout, and without weakening any
  existing gate.
This document designs that packaging path but does NOT implement it. The packaging step must never
  mutate the control checkout, never stage anything outside the declared exact-path set, never
  force-push, and never merge. A real PR-open requires a second, distinct operator approval token.
```

## 2. Current State

```text
control authority : claw plan apply / a2-plan-runner write_executor (single-file, hash-bound).
lane driver       : scripts/a2-tier3-write-orchestrator.sh (validate-lane, apply-lane).
isolation         : every write occurs ONLY in a fresh disposable worktree under
                    /mnt/vast-data/git-worktrees/, created from origin/main; control checkout
                    /home/suki/stack-code is required clean and is never written.
approval          : human-typed `apply <step-id> <preview_sha256>` at a real TTY; never composed,
                    captured, faked, batched, or pre-approved.
evidence          : .claw artifact tree (preview-bundle, approval-result, apply-bundle, checkpoint
                    manifest, payload after.bin/after.sha256, run manifest/status) + stdout
                    a2-l2b-apply-result.v1 (markers incl. a2-l2b-write-applied / -validated).
panel             : read-only. mutationEnabled=false. It PRINTS commands; it spawns nothing, creates
                    no worktree, writes nothing, composes no approval.
proof             : one live apply smoke (2026-06-10, PASS_WITH_NOTES) wrote exactly one declared
                    file inside a throwaway worktree.
```

## 3. North Star Gap

```text
read-only observability  ✅ done
→ safe isolated mutation design  ✅ done (merged)
→ isolated worktree executor  ✅ done (orchestrator + a2-plan-runner)
→ diff / test / evidence  ✅ done (per-file diff + .claw evidence + apply-result markers)
→ explicit operator approval before any real write/apply  ✅ done (human TTY approval)
→ [THIS GAP] package the proven isolated mutation into a REVIEWABLE PR, operator-approved,
   never auto-merged, never from the control checkout.
```

A proven mutation currently dies in a disposable worktree: there is no safe, gated path to turn it
into a PR a human can review and merge. Operators must hand-package, which is exactly the
error-prone manual step the harness exists to remove — but packaging touches branches/remotes and
must be designed with stronger gates than read-only observability.

## 4. Non-Goals

```text
No implementation in this lane (Stage 0 / docs only).
No current checkout mutation (never write/stage/commit in /home/suki/stack-code).
No new write executor; no new checkpoint/rollback logic; reuse a2-plan-runner verbatim.
No direct production write; the only writes are inside a disposable worktree by the EXISTING chain.
No write without operator approval; no auto-approval; no hidden apply.
No auto-merge; no force-push; no branch deletion; no remote branch deletion.
No broad cleanup; no git add .; no git add -A; no git clean; no rm -rf; no git reset --hard.
No runtime/model/broker/Vault access; no raw :11434 app inference.
No write-capable or PR-opening GUI button without a separately token-gated, separately-reviewed lane.
No webview capture of any approval phrase.
No packaging of a worktree whose state drifted from the proven apply evidence.
```

## 5. Design Principles

```text
Stability > Speed — reuse the hardened write core; add only a thin, gated packaging wrapper.
Control > Complexity — exact-path staging, two distinct approval tokens, no auto-merge.
Evidence > Guessing — package only what the apply-result + checkpoint evidence proves was written.
Surgical Change > Broad Rewrite — stage EXACTLY the declared set; refuse on any out-of-set drift.
Clean Lane > Dirty Checkout — never package from a dirty control checkout; package only the worktree.
Never imply success without evidence — a PR is "opened" only after the create call returns a real URL.
```

## 6. Proposed Capability Stages

```text
Stage 0: docs/design only.                                            ← THIS LANE
Stage 1: dry-run packaging PLAN only — print what WOULD be staged/committed/PR'd from the worktree;
         write nothing; run no git mutation; open no PR. (executorDryRun-style, packaging flavour.)
Stage 2: stage + commit INSIDE the disposable worktree only — exact-path `git add <paths>`, an
         evidence-bound commit; NO push, NO PR. Control checkout untouched.
Stage 3: push the disposable worktree's branch to origin — gated on the distinct PR-open token; never
         force; never to an existing protected ref.
Stage 4: open a DRAFT PR for operator review — never merge; surface the evidence in the PR body.
Stage 5: operator-reviewed merge — entirely human, entirely separate, never automated by this harness.
```

This document is **Stage 0 only.** Stages 1–5 each require their own separately-approved lane.

## 7. First Safe Write-Capable Unit (for Tier 4)

The minimal future Tier-4 unit, building on a worktree the existing chain already proved:

```text
Precondition (verified read-only, refuse otherwise):
  - a disposable worktree under /mnt/vast-data/git-worktrees/ on a unique branch from origin/main;
  - it carries a complete apply evidence set: apply-bundle.json + checkpoint manifest + payload
    after.sha256, and an a2-l2b-apply-result.v1 with outcome "applied" and markers
    a2-l2b-write-applied + a2-l2b-write-validated for each declared step;
  - working tree contains ONLY the declared exact-path set as changes (plus the ignored .claw tree);
  - the control checkout /home/suki/stack-code is clean.
Unit of work:
  1. Re-verify each declared file's on-disk after-hash == the recorded after_sha256 (no drift).
  2. Stage EXACTLY the declared set: `git -C <worktree> add -- <path1> <path2> …` (never `.`/`-A`).
  3. Commit with an evidence-bound message (run-id, step-id(s), per-file before/after sha256,
     preview_sha256). No push.
  4. STOP. Print the push + PR-open commands for operator review; do not run them without the
     PR-open token.
Out of this unit: push, PR-open, merge (Stages 3–5, each separately gated).
```

## 8. Future Implementation Surfaces

A future, separately-approved implementation may add ONLY:

```text
- a NEW, operator-invoked packaging subcommand in the EXISTING external orchestrator
  (scripts/a2-tier3-write-orchestrator.sh) — e.g. `package-lane` (Stage 2) and a separate
  `open-pr-lane` (Stages 3–4) — OR a NEW sibling external script under scripts/ whose exact name is
  separately approved; the panel must never spawn it (panel allowlists only a2-ide-harness.sh).
- a NEW docs/handoff describing the packaging evidence contract.
- (optional, later, separately approved) a READ-ONLY panel "packaging readiness" view that DISPLAYS
  whether a worktree is package-ready and PRINTS the operator command — never executes it.
```

## 9. Forbidden Surfaces

```text
- /home/suki/stack-code (control checkout) — never staged/committed/written.
- rust/crates/a2-plan-runner/** — the write core is reused, not edited, in the packaging lane.
- helperRunner.ts / scripts/a2-ide-harness.sh — the read-only panel spawn boundary is unchanged.
- runtime configs, systemd/.service units, Docker/compose, Vault paths, secrets, .env.
- generated/build artifacts, node_modules, .claw payloads (staged: never; read for evidence: yes).
- any path outside the declared exact-path set (deny-by-default; drift = refuse).
- CI workflows, package.json (no dependency or build changes from a packaging lane).
```

## 10. Worktree Isolation Contract

```text
source repo        : /home/suki/stack-code (control checkout; required CLEAN; never written).
worktree root      : /mnt/vast-data/git-worktrees/ (the only place writes/commits may occur).
base               : origin/main (the worktree must descend from origin/main).
branch             : the disposable worktree's own unique branch (the one the apply lane created);
                     packaging commits onto THAT branch — it never reuses or rewinds another branch.
collision checks   : reject if asked to operate on the control checkout path; reject if the worktree
                     is not under the worktree root; reject if the branch is main / a protected ref.
no editing control : the packaging lane never runs any write/stage/commit/add against /home/suki/stack-code.
no force removal   : rollback is by ABANDONING the disposable worktree; never `worktree remove --force`,
                     never `branch -D`, never `git clean`, never `reset --hard`.
```

## 11. Target File Allowlist Contract

```text
explicit set       : the declared exact-path touched-file set (lane `declaredPaths`), the SAME set the
                     apply lane validated and wrote. Packaging stages exactly this set, nothing else.
no glob            : no `git add .`, no `git add -A`, no directory-wide `git add`, no glob expansion.
no dir-wide        : staging is per exact path: `git add -- <path>` for each declared path.
no generated files : a declared path that is a generated/build artifact is rejected unless explicitly
                     allowlisted in the lane and reviewed.
drift guard        : `git status --porcelain` inside the worktree must show ONLY the declared set as
                     changes (ignored .claw excepted). ANY out-of-set modified/untracked file = REFUSE
                     (do not stage, do not commit) — surface it for operator review.
no secrets/runtime : declared paths matching vault/secret/.env/systemd/.service/Docker/runtime shapes
                     are rejected (mirrors the orchestrator's runtime/secret path warner, hardened to refuse).
```

## 12. Mutation Plan Contract

The packaging lane consumes (read-only) the artifacts the apply lane already produced; it plans no new
write to a target. Its "plan" is a **packaging plan**:

```text
inputs (read-only): the approved lane (declaredPaths, branch, worktree path), the apply evidence
                    (apply-bundle.json, checkpoint manifest, payload after.sha256, apply-result.v1).
plan output       : { worktree, branch, base=origin/main, declaredPaths[], perFile:{path, before_sha256,
                    after_sha256, applied:bool}, commit_message_preview, would_push:bool,
                    would_open_pr:bool }. Stage 1 prints this and writes NOTHING.
invariants        : would_push=false and would_open_pr=false at Stage 1/2; they flip true only under
                    the distinct PR-open token at Stage 3/4.
```

## 13. Approval Contract

Two DISTINCT operator approval tokens. The implementation token is NOT the apply/PR token.

```text
IMPLEMENTATION TOKEN (gates building the packaging lane at all — used by the future impl prompt):
    APPROVED: Execute A2 Tier 3 Tier-4 packaging implementation

PR-OPEN TOKEN (gates Stages 3–4 — pushing the branch + opening the PR; a SEPARATE, later approval):
    APPROVED: Open A2 Tier 3 isolated-mutation PR

Rules:
  - Without the IMPLEMENTATION TOKEN, the future implementation lane STOPS before creating a worktree
    or writing any source.
  - Stages 1–2 (dry-run plan; stage+commit in the worktree) require the implementation lane to be
    merged AND an explicit per-run operator go-ahead, but do NOT push or open a PR.
  - Stages 3–4 (push + open PR) require the PR-OPEN TOKEN, supplied per-run, in addition.
  - The merge itself (Stage 5) is human-only; this harness never merges, never `gh pr merge`,
    never auto-approves a review.
  - No token is ever captured/typed/composed in a webview; approval stays explicit and auditable at a
    real terminal / explicit operator action.
```

## 14. Evidence Contract

Every packaging run emits a timestamped evidence record (to stdout and/or a `.claw/packaging/` file
inside the disposable worktree — never to the control checkout):

```text
timestamp (operator-supplied or stamped post-run; scripts must not invent time mid-run)
base SHA (origin/main)
branch (the disposable worktree branch)
worktree path
declared exact-path set
per file: before_sha256, after_sha256, applied marker present (a2-l2b-write-applied/-validated)
drift check result (must be: only declared set changed)
staged paths (must equal declared set exactly)
commit sha (Stage 2) / push result (Stage 3) / PR url (Stage 4, only if a real URL returned)
tests/guards run + results (see §16)
STOP gates evaluated
rollback / retention instruction (abandon worktree; never force)
```

A success is claimed ONLY with the evidence present (e.g. a PR is "opened" only when the create
command returns a real URL — never inferred).

## 15. Diff / Hash Contract

```text
before/after hashes: reuse the EXISTING per-file after_sha256 from the apply payload/checkpoint;
                     re-read each declared file on disk and re-hash; REFUSE on mismatch (drift).
diff preview       : `git -C <worktree> diff --staged --stat` and `--name-only` over the staged set;
                     and a full `git diff --staged` for operator review. Never patch/porcelain shell-out
                     for writing — diff is for display only.
commit binding     : the commit message embeds run-id, step-id(s), preview_sha256, and per-file
                     before→after sha256, so the PR is auditable against the apply evidence.
```

## 16. Validation Contract

```text
before staging     : (a) control checkout clean; (b) worktree under the worktree root, on its unique
                     branch from origin/main; (c) apply evidence complete (outcome "applied", markers
                     present per step); (d) drift guard: only the declared set changed; (e) per-file
                     after-hash matches recorded after_sha256.
after staging      : staged set == declared set EXACTLY (no more, no fewer).
guards/tests       : run the declared, Tier-3-allowlisted validation commands for the lane INSIDE the
                     worktree (denials win over the allowlist); for panel-touching sets, the existing
                     panel guards/tests (npm ci → run-guards → mocha → tsc) must pass before commit.
after commit       : `git -C <worktree> status --porcelain` shows a clean tree (all declared changes
                     committed; nothing else); commit contains exactly the declared paths.
fail-closed        : any gate failure → no stage, no commit, no push, no PR; print the cause; STOP.
```

## 17. Rollback / Retention Contract

```text
rollback           : ABANDON the disposable worktree (operator action). Never `git reset --hard`,
                     never `git clean`, never `branch -D`, never `worktree remove --force`,
                     never revert-in-place, never delete a remote branch.
pre-push rollback  : trivial — the commit lives only on the disposable branch; abandoning the worktree
                     discards it.
post-push rollback : if a branch was pushed but the operator rejects it, the remedy is a human action
                     (close the PR; optionally delete the branch via a SEPARATE explicitly-approved
                     lane) — never automated here.
retention          : the disposable worktree + its .claw evidence are PRESERVED until the operator
                     harvests evidence; cleanup is a separate, non-force, explicitly-approved lane.
no auto-cleanup    : the packaging lane never removes worktrees or branches.
```

## 18. UI / Panel Contract

```text
- The panel stays READ-ONLY. mutationEnabled remains false until a separate, token-gated, reviewed lane.
- The panel may, ONLY after a separate approval, DISPLAY a read-only "packaging readiness" view
  (is the worktree package-ready? what would be staged? what command to run) and PRINT the operator
  command. It must never stage, commit, push, open a PR, or spawn the packaging script.
- No hidden write/stage/commit/push/PR button. No one-click apply or one-click PR.
- No webview approval capture: neither approval token is ever entered in the webview.
- Approval and execution remain explicit, operator-run, and auditable at a real terminal.
```

## 19. Helper / Spawn Boundary Contract

```text
- The panel's single spawn boundary (helperRunner.ts, basename a2-ide-harness.sh, shell:false,
  array-argv, allowlisted read-only subcommands) is UNCHANGED. The packaging script is NOT
  a2-ide-harness.sh and is therefore NOT panel-spawnable, by construction.
- The packaging lane is an EXTERNAL, operator-invoked script (the existing orchestrator's new
  subcommand, or a separately-named sibling). No shell:true; no arbitrary subcommand forwarding;
  array-argv only; exact-path arguments only.
- It drives `git` (add/commit/push) and `gh pr create` with explicit array argv; it never composes a
  shell string from operator input; it never forwards an arbitrary command.
```

## 20. STOP Gates

```text
STOP if no IMPLEMENTATION TOKEN — the future impl lane must not create a worktree or write source.
STOP if asked to operate on /home/suki/stack-code (control checkout) — packaging is worktree-only.
STOP if the control checkout is not clean.
STOP if the worktree is not under /mnt/vast-data/git-worktrees/ or not on a unique origin/main branch.
STOP if the apply evidence is incomplete (no outcome "applied"; missing applied/validated markers).
STOP if drift is detected (any out-of-declared-set modified/untracked file in the worktree).
STOP if a per-file on-disk hash != recorded after_sha256.
STOP if staging would touch anything outside the declared exact-path set.
STOP before push/PR without the PR-OPEN TOKEN.
STOP before any merge — merge is human-only, never automated.
STOP if any declared path matches a secret/runtime/CI/Docker/systemd shape.
STOP on any model/broker/runtime/Vault/:11434 reference.
```

## 21. Risk Assessment

```text
Risk: packaging stages an unintended file.                 Mitigation: exact-path `git add -- <p>`;
  staged-set == declared-set assertion; drift guard refuses on any out-of-set change; no git add . / -A.
Risk: packaging from a dirty/wrong base.                    Mitigation: control checkout clean gate;
  worktree-root + origin/main + unique-branch gates; protected-ref refusal.
Risk: auto-merge / silent PR.                               Mitigation: two distinct tokens; PR opened
  as DRAFT; no gh pr merge; success claimed only on a real returned PR URL.
Risk: drift between apply evidence and on-disk bytes.       Mitigation: per-file after-hash re-verify;
  fail-closed on mismatch.
Risk: panel gains a covert write/PR control.               Mitigation: panel stays read-only; packaging
  script is not panel-spawnable (basename boundary); guards reject fs/network/process/approval patterns.
Risk: irreversible cleanup.                                 Mitigation: rollback = abandon worktree;
  never force-remove / branch -D / reset --hard / clean; remote-branch deletion is a separate lane.
Risk: duplicating the hardened Rust write core.            Mitigation: this scope adds NO write/
  checkpoint/rollback logic; it reuses a2-plan-runner + the orchestrator verbatim.
```

## 22. Recommended Future Implementation Lane

```text
Name        : A2 Tier 3 Tier-4 Packaging — Stage 1 Dry-Run Plan (docs+impl, token-gated)
Type        : implementation, but Stage 1 = dry-run packaging plan ONLY (prints; writes nothing; no
              git mutation; no push; no PR).
Token       : APPROVED: Execute A2 Tier 3 Tier-4 packaging implementation
Objective   : add a read-only `package-plan` subcommand to the existing external orchestrator that, for
              a given disposable worktree + approved lane, prints the packaging plan (§12) and the
              would-stage set, asserting would_push=false / would_open_pr=false. No git writes.
Recommended : Claude Code (phase-gated, STOP-gated) + operator review; reuse a2-plan-runner evidence.
Why         : it is the smallest safe increment toward Tier 4 — pure read-only planning over existing
              evidence — and it validates the evidence/drift contracts before any staging exists.
Surfaces    : scripts/a2-tier3-write-orchestrator.sh (new read-only subcommand) + a tests lane; panel
              untouched; control checkout untouched.
Mutation    : none at Stage 1 (planning only).
STOP gate   : no token → STOP before any code; never stage/commit/push/PR at Stage 1; panel read-only.

Companion lanes (each separately approved, in order, never skipped):
  Stage 2 stage+commit-in-worktree → Stage 3 push (PR-OPEN TOKEN) → Stage 4 open DRAFT PR → Stage 5
  human merge. Parallel option: a "multi-file declared-set live-apply PROOF" lane to prove the existing
  orchestrator applies a >1-file declared set end-to-end before Tier-4 packaging is exercised on a set.
```

---

## Appendix A — Source-of-Truth (merged on origin/main @ 9e57087)

```text
docs/a2-tier3-mutation-executor-write-capable-design-scope.md   (PR #112)  write-capable design; §16 Tier 4 stays out
docs/a2-tier3-write-executor-reconciliation.md                  (PR #113)  drive, don't duplicate the Rust core
scripts/a2-tier3-write-orchestrator.sh                          (PR #120)  disposable-worktree apply orchestrator
rust/crates/a2-plan-runner/src/{write_executor,checkpoint,...}.rs          hardened single-file write authority
handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md      one live single-file apply, control clean
docs/a2-tier3-readonly-observability-closeout-20260610.md                  read-only observability complete
```

## Appendix B — Explicit Non-Goals (this note)

```text
No implementation. No new executor. No source/Rust/script/test/CI edit. No live A2 chain run.
No disposable worktree creation. No target write. No .claw mutation. No stage/commit/push/PR/merge.
No model/broker/runtime/Vault access. No raw :11434 app inference. No webview approval capture.
No worktree/branch cleanup. No touching the control checkout.
```
