# DRAFT — Stack-Code Northstar UX — Phase N5 Implementation Prompt (NOT EXECUTABLE YET)

> **This is a DRAFT future prompt. It is NOT authorized to run now.** It implements only **Phase N5**
> (read-only gated-execution readiness board / package-ladder readiness), and only after the scope and
> this prompt are reviewed and merged. It does not implement the N6 execution-control lane or beyond.

---

## 1. Required Operator Approval

This prompt is inert until the operator supplies, verbatim, as the FIRST non-empty line of the invoking
instruction, the token:

```text
APPROVED: Implement Stack-Code Northstar UX Phase N5
```

```text
STOP CONDITION:
If the activation token "APPROVED: Implement Stack-Code Northstar UX Phase N5" is not present verbatim
as the first non-empty line, STOP immediately. Do nothing. Make no change, run no command, create no
worktree. Report: "BLOCKED — Phase N5 activation token missing."
```

## 2. Role

Operate as a careful Stack-Code IDE-panel engineer working strictly inside the print/validate-only,
safety-gated discipline established in N2/N3/N4 (pure modules, single spawn boundary `helperRunner.ts`,
pure `render.ts`, no execution controls, fail-closed trust levels).

## 3. Objective

Implement **Phase N5 — read-only gated-execution readiness board**:

```text
- A read-only readiness surface that renders, against the N4-reviewed change (validated N3 plan draft +
  N4 preview/diff/evidence): task summary, validated safe-target boundary, risk category, the
  non-executing plan draft, the N4 review state, and a per-rung package-ladder READINESS checklist —
  each datum labelled VERIFIED / INFERRED / MISSING / BLOCKED / EXECUTION_REQUIRED.
- Pure N5 modules: a trust-level extension adding EXECUTION_REQUIRED, per-rung readiness derivation
  (package-plan → package-commit → package-push → package-pr), N5 UI state derivation, and an
  assertN5Safe guard whose forbidden set is a SUPERSET of N4_FORBIDDEN_TARGETS.
- Pure render sections for the readiness board; read-only wiring in extension.ts that builds the board
  from existing local N3/N4 state + already-present read-only helper evidence.
- N5 renders only data that already exists and reasons about readiness only; it NEVER runs any package
  rung / apply / PR, generates no apply artifact, and writes no target or .claw. Any printed/copied
  command is inert text bound to no action.
```

## 4. Source of Truth

```text
docs/stack-code-northstar-ux-phase-n5-gated-execution-boundary-scope.md   the N5 scope (this prompt builds it)
docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md      N4 scope (trust/state model N5 extends)
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                      N1 roadmap (N5 row)
ide/vscode/a2-harness-panel/src/n4TrustLevel.ts                           TrustLevel + classifyTrust (extend with EXECUTION_REQUIRED)
ide/vscode/a2-harness-panel/src/n4State.ts                                N4State + N4_FORBIDDEN_TARGETS + assertN4Safe (N5 superset)
ide/vscode/a2-harness-panel/src/n4View.ts                                 preview/diff/evidence view models (readiness reads these)
ide/vscode/a2-harness-panel/src/n3PlanDraft.ts                            plan draft + not_executable_reason + validator
ide/vscode/a2-harness-panel/src/n3RiskClassifier.ts                       risk categories + safe-target boundary
ide/vscode/a2-harness-panel/src/render.ts                                 pure renderer (add read-only N5 sections)
ide/vscode/a2-harness-panel/scripts/run-guards.js                         the guard set N5 must keep green
handoffs/a2_tier4_lane_b_live_package_pr_smoke_closeout_2026-06-15.md     chain-level package ladder (readiness reference only)
```

## 5. Current State

```text
N1/N2/N3/N4 merged on main (N4 = PR #152, 16ae373). N4 produces a read-only preview/diff/evidence view
and hands off to "a future, separately-approved N5 lane [that] handles gated execution" (see
n4State.ts n4NextStepLabel for N4_EVIDENCE_READY). The panel is print/validate-only: no Run-* control,
single spawn boundary, pure render, fail-closed trust levels. N5 must preserve every one of those
invariants and add only a READINESS layer — no execution.
```

## 6. Hard Boundaries

```text
Do NOT implement if the token is absent.
Do NOT run live A2 (no preview / approval / apply-bundle / apply).
Do NOT run package-plan / package-commit / package-push / package-pr.
Do NOT add any execution / Run-* / apply / package / PR control or button.
Do NOT bind any printed/copied command to a Run action (print/copy is inert text only).
Do NOT open / close / merge / approve / mark-ready any PR (draft-only + separate PR-open token even in future).
Do NOT push or force-push; do NOT delete branches or worktrees.
Do NOT run git clean / rm -rf / find -delete / git reset --hard; do NOT use git add . / git add -A; do NOT prune refs.
Do NOT call a model / broker / /v1/chat/completions / /status/vram.
Do NOT introduce raw :11434 app inference.
Do NOT touch runtime / services / HQ / Vault / secrets / CI / scripts / rust / schemas.
Do NOT write a target file or a .claw artifact; generate no apply artifact.
Do NOT add a new spawn boundary (only helperRunner.ts may spawn); keep render.ts pure.
Do NOT touch the install-smoke 448d7ea worktree/branch or the forensic fixture 968934d.
Do NOT render ambiguous/blocked/execution-required data as verified/ready — fail closed.
Do NOT implement direct execution controls — those require a separate N6 scope (STOP; see §16).
Do NOT start the N6 execution-control lane or any later phase.
```

## 7. Clean Worktree Setup

```text
branch:   feat/stack-code-northstar-ux-n5-gated-execution-readiness-<date>
worktree: /mnt/vast-data/git-worktrees/stack-code-northstar-ux-n5-<date>
base:     current origin/main
Verify: control checkout on main, no staged/unstaged tracked changes, fetch succeeds, target worktree
        path does not exist, branch does not exist, no worktree-list collision. STOP on any collision.
Do not edit /home/suki/stack-code (control checkout).
```

## 8. Discovery

```text
Begin with source discovery: read the N5 scope + n4TrustLevel.ts / n4State.ts / n4View.ts /
n3PlanDraft.ts / n3RiskClassifier.ts / render.ts to build to existing patterns. Do not assume a data
source exists unless observed in read-only N3/N4 state or explicit read-only helper output. Reason about
the package ladder from the Tier-4 closeout as READINESS reference only — never wire the panel to run it.
```

## 9. Implementation Scope

```text
Pure modules first, each with unit tests, before any render/wiring:
  - n5 trust-level extension: TrustLevel + EXECUTION_REQUIRED; fail-closed classification.
  - n5 readiness model: per-rung (package-plan/commit/push/pr) readiness from VERIFIED preconditions +
    present evidence; a rung is READY only when all preconditions VERIFIED and evidence consistent.
  - n5 UI state model (N5_NOT_READY .. N5_DEFERRED_REQUIRES_EXECUTION_TOKEN) + derivation; assert no N5
    state routes to apply/package/PR (assertN5Safe; N5_FORBIDDEN_TARGETS ⊇ N4_FORBIDDEN_TARGETS).
Then:
  - pure render sections for the readiness board (no execution control; printed command inert if adopted).
  - read-only wiring in extension.ts that builds the N5 board from local N3/N4 state + present read-only
    helper evidence.
Tests + guards green. Commit locally only. Do not push. Do not open a PR.
```

## 10. Gated Execution Boundary Model

```text
N5 lives entirely on the REVIEW side of the execution boundary (N5 scope §7). A "…_READY" state means
"ready to be run in a separate approved lane", never "run it". N5 may display readiness and — only if the
scope's default posture (A + optional B/C) is adopted — print or copy the exact command an operator would
run elsewhere, as inert text bound to no action. N5 never runs a rung, apply, or PR.
```

## 11. Package Ladder Readiness Model

```text
For each rung (package-plan → package-commit → package-push → package-pr), derive and render (read-only):
  purpose, preconditions met/unmet (VERIFIED to be READY), evidence present/absent, whether operator
  confirmation is required, and readiness (READY / NOT_READY / BLOCKED / EXECUTION_REQUIRED). Facts that
  cannot be proven from read-only data (real push/apply/remote result) are EXECUTION_REQUIRED, never
  guessed. No rung is ever run, bound to a button, or routed into.
```

## 12. Apply/PR Boundary Model

```text
- apply is the highest-risk boundary: N5 must not apply, must not hide apply behind package language,
  must not create target/.claw writes, must not auto-approve, and shows EXECUTION_REQUIRED where the real
  apply result is unprovable.
- PR: N5 must not open real PRs; any future PR-open is draft-only and requires a separate PR-open token;
  merge is human-only. In N5, "package-pr readiness" is display/print/copy only.
```

## 13. Safety Invariants (must be test-enforced)

```text
- No N5 state routes to apply / package / PR; N5_FORBIDDEN_TARGETS ⊇ N4_FORBIDDEN_TARGETS
  (PREVIEW_READY-as-execution, AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY, COMMITTED, PUSHED,
  DRAFT_PR_OPEN); assertN5Safe throws on each forbidden/unknown state.
- N5 runs no live A2 and no package rung; it writes no target, no .claw, and generates no apply artifact.
- Ambiguous/blocked/execution-required data fails closed; never rendered as VERIFIED/READY.
- No new spawn boundary (guards: only helperRunner.ts spawns); render.ts stays pure.
- No apply/package/PR/Run-* control in the rendered markup; any printed command is inert text.
- No model/broker/runtime/Vault call; no raw :11434.
```

## 14. Validation Plan

```text
- node scripts/run-guards.js — PASS.
- npx tsc -p . — PASS (production build).
- npm test — full suite green, including new N5 trust/readiness/state/render/safety tests.
- safety scans: no execution authority, no new spawn site, no forbidden surface, no Run-* control.
- No live A2 / package / apply / PR run during validation.
```

## 15. No-Live-A2 Boundary

```text
N5 is a READINESS BOARD. It never invokes the A2 chain (no claw plan run / approve / apply-bundle /
apply), never runs the package ladder (no package-plan / package-commit / package-push / package-pr),
never opens a PR, and never calls a model/broker/runtime. It only renders and reasons about data that
already exists from validated N3/N4 state or explicit read-only helper output. If a fact needed for a
readiness verdict is absent or unprovable, N5 shows MISSING / BLOCKED / EXECUTION_REQUIRED and fails
closed — it does NOT run anything to produce that fact.
```

## 16. Direct-Execution-Control STOP Gate

```text
If the invoking instruction (or any mid-lane request) asks to add direct execution controls — a Run
Preview / Run Apply / Package / Commit / Push / PR button, a command bound to an action, an apply path,
or any real PR-open/merge — STOP immediately. Report: "BLOCKED — direct execution controls require a
separate N6 execution-control scope; not authorized under the N5 activation token." Direct execution
controls are OUT OF SCOPE for N5 and must be scoped and approved separately as N6.
```

## 17. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: STACK_CODE_NORTHSTAR_UX_PHASE_N5_IMPLEMENTATION
TOKEN PRESENT: yes/no
WORKTREE: repo / branch / worktree / base / commit
FILES CHANGED:
TESTS: suite result (N/N green) + guards PASS + build PASS
SAFETY: live A2 run / preview run / apply run / package-* run / PR opened / pushed / force / new spawn
        boundary / target-or-.claw write / apply-artifact generated / command-bound-to-action / model-
        broker / runtime / Vault / raw 11434 / ambiguous-rendered-as-verified — all must be "no"
NEXT PHASE: Phase N6 — direct execution controls (separate approved scope + lane)
```
