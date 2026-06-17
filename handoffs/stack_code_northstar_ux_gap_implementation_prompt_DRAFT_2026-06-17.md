# DRAFT — Stack-Code Northstar UX — Phase N2 Implementation Prompt (NOT EXECUTABLE YET)

> **This is a DRAFT future prompt. It is NOT authorized to run now.** It implements only **Phase N2**
> (workspace dashboard + read-only state model) of the Northstar UX roadmap, and only after the scope
> and this prompt are reviewed and merged. It does not implement every phase at once.

---

## Activation Token (REQUIRED)

This prompt is inert until the operator supplies, verbatim, the future token:

```text
APPROVED: Implement Stack-Code Northstar UX Phase N2
```

```text
STOP CONDITION:
If the activation token "APPROVED: Implement Stack-Code Northstar UX Phase N2" is not present
verbatim in the invoking instruction, STOP immediately. Do nothing. Make no change, run no command,
create no worktree. Report: "BLOCKED — Phase N2 activation token missing."
```

---

## Source of Truth

```text
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md   the Northstar UX gap scope (this prompt builds Phase N2 of it)
ide/vscode/a2-harness-panel/                           the existing print/validate-only panel package
ide/vscode/a2-harness-panel/src/stateMachine.ts        the existing read-only 13-state next-step machine
ide/vscode/a2-harness-panel/src/discovery.ts           existing read-only chain-state detection
scripts/a2-ide-harness.sh                              the print/validate-only chain helper (panel's only spawn)
scripts/a2-tier3-write-orchestrator.sh                 the package-plan/commit/push/pr ladder (NOT touched in N2)
```

## Role

Operate as a careful Stack-Code IDE-panel engineer working strictly inside the print/validate-only,
safety-gated discipline already established in the panel package.

## Objective (Phase N2 ONLY)

Implement **Phase N2 — workspace dashboard + state model**:

```text
- Auto-detect the workspace on panel open and render a read-only workspace status card
  (path, branch, clean/dirty, origin/main freshness) without requiring manual field-set.
- Implement the read-only Northstar state model as a superset of the existing 13-state machine:
  NO_WORKSPACE, WORKSPACE_READY, TASK_DESCRIBED, PLAN_DRAFTED, PLAN_VALIDATED, PREVIEW_READY,
  AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY, COMMITTED, PUSHED, DRAFT_PR_OPEN, EVIDENCE_FROZEN,
  DISPOSITION_PENDING, CLOSED_RETAINED, HUMAN_MERGE_PENDING.
- The state model is READ-ONLY guidance: it derives the current state from read-only signals and never
  triggers an apply/package/PR transition. It only displays "where you are" and "the next safe step".
```

Phase N2 does NOT implement task intake, plan generation, diff viewer, gated package controls, PR card,
evidence timeline, or disposition. Those are later approved phases (N3–N7).

## Procedure (Phase N2)

```text
1. Verify the activation token is present verbatim. STOP if missing.
2. Begin with docs/source discovery: read the scope doc + existing stateMachine.ts / discovery.ts /
   render.ts / extension.ts / helperRunner.ts to ground the implementation in existing patterns.
3. Create a FRESH isolated worktree from current origin/main (one lane = one worktree = one branch).
   Do not edit /home/suki/stack-code (control checkout). Verify no collision before creating.
4. Implement ONLY the workspace status card (read-only auto-detect on open) and the read-only Northstar
   state model (superset of the existing machine). Keep render.ts pure; keep helperRunner.ts the single
   spawn boundary (array-argv only, no shell, basename allowlist, per-subcommand flag allowlist).
5. Add unit tests/guards proving the safety invariants below.
6. Run the panel test suite; confirm green. Capture evidence.
7. Commit locally only. Do not push. Do not open a PR.
8. Report classification + evidence + recommended next phase (N3).
```

## Hard Boundaries (Phase N2)

```text
Do NOT run apply.
Do NOT run package-plan / package-commit / package-push / package-pr.
Do NOT open / close / merge / approve / mark-ready any PR.
Do NOT push or force-push; do NOT delete branches (local or remote).
Do NOT remove worktrees; do NOT run git clean / rm -rf / find -delete / git reset --hard.
Do NOT use git add . or git add -A; stage exact paths only.
Do NOT call a model / broker / /v1/chat/completions / /status/vram.
Do NOT introduce raw :11434 app inference.
Do NOT touch Vault / secrets / runtime / services / HQ / CI.
Do NOT edit scripts/a2-tier3-write-orchestrator.sh or the packaging ladder.
Do NOT touch the install-smoke 448d7ea branch/worktree or any preserved forensic fixture worktree/branch.
Do NOT add any Run-* control or any path that bypasses a gate (N2 is read-only display only).
```

## Safety Invariants (must be test-enforced in N2)

```text
- The state model NEVER recommends or triggers apply / package / push / pr / merge.
- No transition auto-advances past AWAITING_APPLY_APPROVAL.
- No code path reaches merged / approved / ready via automation.
- The workspace status card is read-only; it mutates nothing.
- The single spawn boundary discipline is preserved (array-argv, no shell, basename + flag allowlist).
- render.ts stays pure; no chain state is computed in a way that could drive a write.
```

## Clean Worktree Requirement

```text
branch:   feat/stack-code-northstar-ux-n2-workspace-dashboard-<date>
worktree: /mnt/vast-data/git-worktrees/stack-code-northstar-ux-n2-<date>
base:     current origin/main
Verify: control checkout on main, no staged/unstaged tracked changes, fetch succeeds,
        target worktree path does not exist, branch does not exist, no worktree-list collision.
STOP on any collision.
```

## Commit / Push Policy

```text
- Stage exact paths only (the N2 source files + tests).
- Commit locally only with a conventional message, e.g.:
    feat(a2): northstar ux n2 workspace dashboard + read-only state model
- Do NOT push. Do NOT open a PR. A separate approved lane handles push/PR.
```

## Non-Goals (Phase N2)

```text
- No task intake, plan generation, diff viewer, gated package controls, PR card, evidence timeline,
  or disposition UX (those are N3–N7).
- No automation of any risky step.
- No model/broker/runtime/Vault integration.
- No multi-phase implementation in one lane.
```

## Required Report

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: STACK_CODE_NORTHSTAR_UX_PHASE_N2_IMPLEMENTATION
TOKEN PRESENT: yes/no
WORKTREE: repo / branch / worktree / base / commit
FILES CHANGED:
TESTS: suite result (e.g. N/N green)
SAFETY: apply run / package-* run / PR opened / pushed / force / runtime / model-broker / Vault /
        raw 11434 / destructive commands — all must be "no"
NEXT PHASE: Phase N3 — task intake + plan draft UX (separate approved lane)
```
