# DRAFT — Stack-Code Northstar UX — Phase N4 Implementation Prompt (NOT EXECUTABLE YET)

> **This is a DRAFT future prompt. It is NOT authorized to run now.** It implements only **Phase N4**
> (read-only preview / diff / evidence viewer), and only after the scope and this prompt are reviewed
> and merged. It does not implement N5–N8.

---

## 1. Required Operator Approval

This prompt is inert until the operator supplies, verbatim, as the FIRST non-empty line of the invoking
instruction, the token:

```text
APPROVED: Implement Stack-Code Northstar UX Phase N4
```

```text
STOP CONDITION:
If the activation token "APPROVED: Implement Stack-Code Northstar UX Phase N4" is not present verbatim
as the first non-empty line, STOP immediately. Do nothing. Make no change, run no command, create no
worktree. Report: "BLOCKED — Phase N4 activation token missing."
```

## 2. Role

Operate as a careful Stack-Code IDE-panel engineer working strictly inside the print/validate-only,
safety-gated discipline established in N2/N3 (pure modules, single spawn boundary `helperRunner.ts`,
pure `render.ts`, no execution controls).

## 3. Objective

Implement **Phase N4 — read-only preview / diff / evidence viewer**:

```text
- A read-only review surface that renders, against the validated N3 plan draft:
  task summary, validated safe-target boundary, risk category, the non-executing plan draft, and the
  preview / diff / evidence readiness — each labelled VERIFIED / INFERRED / MISSING / BLOCKED.
- Pure N4 modules: trust-level classification, N4 UI state derivation, and view models for preview,
  diff, and evidence — all read-only, fail-closed on ambiguous/blocked data.
- Pure render sections for the viewer; read-only wiring in extension.ts that builds the view from the
  existing local N3 task draft + already-present read-only helper artifacts.
- N4 renders only data that already exists; it NEVER runs preview/approval/apply-bundle/apply, generates
  no apply artifact, and writes no target or .claw.
```

## 4. Source of Truth

```text
docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md   the N4 scope (this prompt builds it)
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                    N1 roadmap (N4 row)
ide/vscode/a2-harness-panel/src/n3PlanDraft.ts                          plan draft model + not_executable_reason + validator
ide/vscode/a2-harness-panel/src/n3TaskIntake.ts                         TaskDraft + reducer
ide/vscode/a2-harness-panel/src/n3State.ts                              N3 states + handoff to N4
ide/vscode/a2-harness-panel/src/n3RiskClassifier.ts                     risk categories + safe-target boundary
ide/vscode/a2-harness-panel/src/render.ts                               pure renderer (add read-only N4 sections)
ide/vscode/a2-harness-panel/src/discovery.ts                            read-only audit parsing (preview-bundle / artifacts)
ide/vscode/a2-harness-panel/scripts/run-guards.js                       the guard set N4 must keep green
```

## 5. Current State

```text
N1/N2/N3 merged on main (N3 = PR #150, a44b588). N3 produces a validated, non-executing plan draft and
hands off to "a future, separately-approved N4 preview/diff lane". The panel is print/validate-only:
no Run-* control, single spawn boundary, pure render. N4 must preserve every one of those invariants.
```

## 6. Hard Boundaries

```text
Do NOT implement if the token is absent.
Do NOT run live A2 (no preview / approval / apply-bundle / apply).
Do NOT run package-plan / package-commit / package-push / package-pr.
Do NOT add any execution / Run-* / apply / package / PR control or button.
Do NOT open / close / merge / approve / mark-ready any PR.
Do NOT push or force-push; do NOT delete branches or worktrees.
Do NOT run git clean / rm -rf / find -delete / git reset --hard; do NOT use git add . / git add -A.
Do NOT call a model / broker / /v1/chat/completions / /status/vram.
Do NOT introduce raw :11434 app inference.
Do NOT touch runtime / services / HQ / Vault / secrets / CI / scripts / rust / schemas.
Do NOT write a target file or a .claw artifact; generate no apply artifact.
Do NOT add a new spawn boundary (only helperRunner.ts may spawn); keep render.ts pure.
Do NOT touch the install-smoke 448d7ea worktree/branch or the forensic fixture 968934d.
Do NOT render ambiguous/blocked data as verified — fail closed.
```

## 7. Clean Worktree Setup

```text
branch:   feat/stack-code-northstar-ux-n4-preview-diff-evidence-<date>
worktree: /mnt/vast-data/git-worktrees/stack-code-northstar-ux-n4-<date>
base:     current origin/main
Verify: control checkout on main, no staged/unstaged tracked changes, fetch succeeds, target worktree
        path does not exist, branch does not exist, no worktree-list collision. STOP on any collision.
Do not edit /home/suki/stack-code (control checkout).
```

## 8. Discovery

```text
Begin with source discovery: read the N4 scope + n3PlanDraft.ts / n3TaskIntake.ts / n3State.ts /
n3RiskClassifier.ts / render.ts / discovery.ts to build to existing patterns. Do not assume a data
source exists unless observed in the read-only N3 state or the read-only helper audit.
```

## 9. Implementation Scope

```text
Pure modules first, each with unit tests, before any render/wiring:
  - n4 trust-level classifier (VERIFIED / INFERRED / MISSING / BLOCKED), fail-closed.
  - n4 UI state model (N4_NOT_READY .. N4_BLOCKED_AMBIGUOUS_ARTIFACTS) + derivation; assert no N4 state
    routes to apply/package/PR.
  - n4 view models for preview / diff / evidence, read-only, sourced from validated N3 state + present
    read-only artifacts.
Then:
  - pure render sections for the viewer (no execution control).
  - read-only wiring in extension.ts that builds the N4 view from the local N3 task draft + present
    read-only helper artifacts.
Tests + guards green. Commit locally only. Do not push. Do not open a PR.
```

## 10. Preview/Diff/Evidence Model

```text
Preview:  DISPLAY-ONLY. Renders the validated plan draft + any present chain preview data (read-only).
          No "generate/run preview" control. States: N4_PREVIEW_DATA_MISSING -> N4_PREVIEW_READY.
Diff:     DISPLAY-ONLY. Renders declared_target_paths / forbidden_paths / expected_outputs /
          not_executable_reason (VERIFIED from N3) + richer diff ONLY when present read-only. Writes
          nothing, runs no apply-bundle, produces no apply artifact.
Evidence: DISPLAY-ONLY. Surfaces N3 required_evidence + read-only evidence-timeline lines + any
          operator-provided snapshot. Freezes nothing to disk, writes no .claw.
Every datum is labelled VERIFIED / INFERRED / MISSING / BLOCKED; ambiguous data fails closed.
```

## 11. Safety Invariants (must be test-enforced)

```text
- No N4 state routes to apply / package / PR (PREVIEW_READY-as-execution, AWAITING_APPLY_APPROVAL,
  APPLIED, PACKAGE_READY, COMMITTED, PUSHED, DRAFT_PR_OPEN are unreachable from N4).
- N4 runs no live A2 and generates no apply artifact; it writes no target and no .claw.
- Ambiguous/blocked data fails closed (BLOCKED state); never rendered as VERIFIED.
- No new spawn boundary (guards: only helperRunner.ts spawns); render.ts stays pure.
- No apply/package/PR/Run-* control in the rendered markup.
- No model/broker/runtime/Vault call; no raw :11434.
```

## 12. Validation Plan

```text
- node scripts/run-guards.js — PASS.
- npx tsc -p . — PASS (production build).
- npm test — full suite green, including new N4 trust/state/view/render/safety tests.
- safety scans: no execution authority, no new spawn site, no forbidden surface.
- No live A2 / package / apply / PR run during validation.
```

## 13. No-Live-A2 Boundary

```text
N4 is a VIEWER. It never invokes the A2 chain (no claw plan run / approve / apply-bundle / apply), never
runs the package ladder, never opens a PR, and never calls a model/broker/runtime. It only renders data
that already exists from validated N3 state or explicit read-only helper output. If the data needed for
a preview/diff/evidence view is absent or ambiguous, N4 shows MISSING/BLOCKED and fails closed — it does
NOT run anything to produce that data.
```

## 14. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: STACK_CODE_NORTHSTAR_UX_PHASE_N4_IMPLEMENTATION
TOKEN PRESENT: yes/no
WORKTREE: repo / branch / worktree / base / commit
FILES CHANGED:
TESTS: suite result (N/N green) + guards PASS + build PASS
SAFETY: live A2 run / preview run / apply run / package-* run / PR opened / pushed / force / new spawn
        boundary / target-or-.claw write / apply-artifact generated / model-broker / runtime / Vault /
        raw 11434 / ambiguous-rendered-as-verified — all must be "no"
NEXT PHASE: Phase N5 — guided package ladder controls (separate approved lane)
```
