# DRAFT — Stack-Code Northstar UX — Phase N3 Implementation Prompt (NOT EXECUTABLE YET)

> **This is a DRAFT future prompt. It is NOT authorized to run now.** It implements only **Phase N3**
> (task intake + non-executing plan draft UX) of the Northstar UX roadmap, and only after the scope and
> this prompt are reviewed and merged. It does not implement N4–N8.

---

## Activation Token (REQUIRED)

This prompt is inert until the operator supplies, verbatim, the future token:

```text
APPROVED: Implement Stack-Code Northstar UX Phase N3
```

```text
STOP CONDITION:
If the activation token "APPROVED: Implement Stack-Code Northstar UX Phase N3" is not present
verbatim in the invoking instruction, STOP immediately. Do nothing. Make no change, run no command,
create no worktree. Report: "BLOCKED — Phase N3 activation token missing."
```

---

## Source of Truth

```text
docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md   the N3 scope (this prompt builds it)
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                               N1 roadmap (N3 row)
ide/vscode/a2-harness-panel/src/northstarState.ts                                  N2 ladder (consumes taskDescribed/planDrafted/planValidated)
ide/vscode/a2-harness-panel/src/workspaceStatus.ts                                 N2 read-only card (the pattern to mirror)
ide/vscode/a2-harness-panel/src/render.ts                                          pure renderer (add N3 sections here)
ide/vscode/a2-harness-panel/src/extension.ts                                       recomputeViews() read-only aggregation
ide/vscode/a2-harness-panel/scripts/run-guards.js                                  the guard set N3 must keep green
```

## Role

Operate as a careful Stack-Code IDE-panel engineer working strictly inside the print/validate-only,
safety-gated discipline established in N2 (pure modules, single spawn boundary, pure render).

## Objective (Phase N3 ONLY)

Implement **Phase N3 — task intake + non-executing plan draft UX**:

```text
- Task intake state model + pure reducer (local/session-only; no fs/spawn/model).
- Safe target boundary model + risk classifier (pure; deny-list wins; STOP/UNKNOWN fail closed).
- Non-executing plan draft model + offline validator (pure; not_executable_reason required;
  the draft is structurally NOT runnable by claw or the orchestrator).
- N3 state-machine states: TASK_INTAKE_EMPTY, TASK_DESCRIBED, TARGETS_DECLARED, RISK_CLASSIFIED,
  PLAN_DRAFTED, PLAN_DRAFT_VALIDATED, PLAN_DRAFT_BLOCKED — none transitions to PREVIEW_READY or beyond.
- Render sections (intake box / declared paths / forbidden paths / risk badge / plan draft card /
  lint results), render.ts stays pure.
- Extension wiring (read-only): recomputeViews() produces the taskDescribed/planDrafted/planValidated
  signals the existing N2 ladder already consumes. No new spawn boundary.
```

## Procedure (Phase N3)

```text
1. Verify the activation token is present verbatim. STOP if missing.
2. Begin with source discovery: read the N3 scope + northstarState.ts / workspaceStatus.ts /
   render.ts / extension.ts / run-guards.js to build to existing patterns.
3. Create a FRESH isolated worktree from current origin/main (one lane = one worktree = one branch).
   Do not edit /home/suki/stack-code. Verify no collision before creating.
4. Implement the pure N3 modules first (reducer, boundary+risk, plan draft + validator), each with
   unit tests, before any render/wiring.
5. Add render sections (pure) + extension wiring (read-only). Keep helperRunner.ts the single spawn
   boundary; keep render.ts pure.
6. Run guards + the panel test suite; confirm green. Capture evidence.
7. Commit locally only. Do not push. Do not open a PR.
8. Report classification + evidence + recommended next phase (N4).
```

## Hard Boundaries (Phase N3)

```text
Do NOT run apply / package-plan / package-commit / package-push / package-pr.
Do NOT open / close / merge / approve / mark-ready any PR.
Do NOT push or force-push; do NOT delete branches (local or remote).
Do NOT remove worktrees; do NOT run git clean / rm -rf / find -delete / git reset --hard.
Do NOT use git add . or git add -A; stage exact paths only.
Do NOT call a model / broker / /v1/chat/completions / /status/vram.
Do NOT introduce raw :11434 app inference.
Do NOT touch Vault / secrets / runtime / services / HQ / CI.
Do NOT edit scripts/a2-tier3-write-orchestrator.sh or the packaging ladder.
Do NOT touch the install-smoke 448d7ea branch/worktree or any preserved forensic fixture worktree/branch.
Do NOT add any Run-* / apply / package / PR control or any path that bypasses a gate.
Do NOT make the plan draft runnable (no plan-body, no command string, no claw invocation).
```

## Safety Invariants (must be test-enforced in N3)

```text
- No N3 transition targets PREVIEW_READY / AWAITING_APPLY_APPROVAL / APPLIED / PACKAGE_READY /
  COMMITTED / PUSHED / DRAFT_PR_OPEN.
- The plan draft is non-executing: not_executable_reason is required and non-empty; the draft carries
  no command/plan-body/claw-invocation; a test proves it cannot be run.
- Deny-list (forbidden_paths) always wins over declared_target_paths.
- RUNTIME_CONFIG / SECRETS_OR_VAULT / DESTRUCTIVE_OR_FORCE / UNKNOWN risk fail closed (BLOCKED).
- No new spawn boundary (guards: only helperRunner.ts may spawn); render.ts stays pure.
- No apply/package/PR/Run-* control exists in the rendered markup.
```

## Clean Worktree Requirement

```text
branch:   feat/stack-code-northstar-ux-n3-task-intake-plan-draft-<date>
worktree: /mnt/vast-data/git-worktrees/stack-code-northstar-ux-n3-<date>
base:     current origin/main
Verify: control checkout on main, no staged/unstaged tracked changes, fetch succeeds,
        target worktree path does not exist, branch does not exist, no worktree-list collision.
STOP on any collision.
```

## Commit / Push Policy

```text
- Stage exact paths only (the N3 source modules + tests + the render/extension edits).
- Commit locally only with a conventional message, e.g.:
    feat(a2): northstar ux n3 task intake + non-executing plan draft
- Do NOT push. Do NOT open a PR. A separate approved lane handles push/PR.
```

## Deferred Phases (explicitly NOT in N3)

```text
N4 — preview / diff / evidence viewer
N5 — guided package ladder controls
N6 — draft PR card + evidence timeline
N7 — cleanup / disposition UX
N8 — optional local agent planner integration
```

## Non-Goals (Phase N3)

```text
- No plan execution, preview, or diff rendering.
- No package ladder controls, PR card, evidence timeline, or disposition UX.
- No automation of any risky step.
- No model/broker/runtime/Vault integration; no autonomous planner.
- No persistence beyond local/session state.
- No multi-phase implementation in one lane.
```

## Required Report

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: STACK_CODE_NORTHSTAR_UX_PHASE_N3_IMPLEMENTATION
TOKEN PRESENT: yes/no
WORKTREE: repo / branch / worktree / base / commit
FILES CHANGED:
TESTS: suite result (e.g. N/N green) + guards PASS
SAFETY: apply run / package-* run / PR opened / pushed / force / runtime / model-broker / Vault /
        raw 11434 / new spawn boundary / plan-draft-runnable / destructive commands — all must be "no"
NEXT PHASE: Phase N4 — preview / diff / evidence viewer (separate approved lane)
```
