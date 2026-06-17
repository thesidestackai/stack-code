# Stack-Code — Northstar UX Phase N3 Scope: Task Intake + Plan Draft UX — 2026-06-17

> **Docs-only scope.** This document designs Phase N3 of the Northstar UX roadmap. It implements
> nothing, edits no IDE/panel source / scripts / tests / Rust / schemas / CI / runtime / services / HQ,
> runs no apply / package-plan / package-commit / package-push / package-pr, opens no PR, calls no
> model / broker / `/v1/chat/completions` / `/status/vram`, touches no Vault/secret, and introduces no
> raw `:11434` app inference. It is the source-of-truth scope a separately-approved N3 implementation
> lane would build to. It does not authorize implementation.

---

## 1. Executive Summary

Phase N2 added the **workspace dashboard + read-only state model** (merged on `main`, PR #148,
`56b8141`). Phase N3 should add **task intake + plan draft UX**: a way for the operator to describe a
task in-product, declare safe target boundaries, get a risk classification, and produce a **non-
executing plan draft** for review — then STOP before any preview / apply / package / PR behavior.

```text
Phase N2 added workspace dashboard + read-only state model.
Phase N3 should add task intake + plan draft UX.
Phase N3 must not execute plans, run apply, run package commands, open PRs, or call models.
```

The integration is clean: the N2 Northstar ladder (`northstarState.ts`) **already consumes** the
read-only signals `taskDescribed`, `planDrafted`, and `planValidated`, but nothing in the product yet
**produces** them. N3 builds exactly the intake + plan-draft surfaces that flip those existing early-
ladder signals — extending the model, not rewriting it, and never crossing the apply gate.

---

## 2. Source of Truth

```text
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                     N1 Northstar gap scope + roadmap (N3 row: §5 + §20)
ide/vscode/a2-harness-panel/src/northstarState.ts                        N2 read-only 16-state model (consumes taskDescribed/planDrafted/planValidated)
ide/vscode/a2-harness-panel/src/workspaceStatus.ts                       N2 read-only workspace status card
ide/vscode/a2-harness-panel/src/render.ts                                pure renderer (workspace card + northstar ladder sections)
ide/vscode/a2-harness-panel/src/extension.ts                             recomputeViews() read-only aggregation; auto-detect on open
ide/vscode/a2-harness-panel/src/setupStatus.ts                           the "never green-by-default" honesty precedent
ide/vscode/a2-harness-panel/scripts/run-guards.js                        the no-fs/no-spawn/no-network/no-watcher/no-polling guard set
PR #148 (56b81413b61f8ed3b40712457dfd3a55afa9d352)                       merged N2 implementation
```

---

## 3. Current State After N2

```text
Workspace status card exists.
Northstar ladder/read-only state model exists.
No task intake exists yet.
No plan draft UX exists yet.
No preview/diff viewer exists yet.
No package ladder controls exist yet.
No draft PR card exists yet.
```

Structurally true in the merged source (grounding):

- `northstarState.ts` already defines `taskDescribed`, `planDrafted`, `planValidated` as inputs to
  `deriveNorthstarState`, and the ladder states `TASK_DESCRIBED`, `PLAN_DRAFTED`, `PLAN_VALIDATED`.
  These signals are currently always `false` in `recomputeViews()` — N3 is what sets them.
- The panel is print/validate-only: no `Run-*` button, a single spawn boundary (`helperRunner.ts`),
  `render.ts` is pure, guards forbid `fs` / spawn-elsewhere / network / watcher / polling.
- N3 must preserve every one of those invariants.

---

## 4. Phase N3 Goal

```text
Operator describes a task.
System captures task intent as structured draft state.
System asks for safe target boundaries.
System classifies risk.
System produces a non-executing plan draft for review.
System validates the plan draft shape.
System stops before preview/apply/package/PR behavior.
```

N3 is a **UI + data-model** step that prepares the operator for the future N4 preview/diff work. It is
not plan execution and not a model-powered autonomous planner (that is N8, separately scoped).

---

## 5. Non-Goals

```text
No apply.
No package-plan.
No package-commit.
No package-push.
No package-pr.
No real PR open.
No model/broker/runtime/Vault.
No raw :11434 app inference.
No autonomous model planner.
No preview/diff execution.
No file mutation.
No branch push.
No merge/approve/mark-ready.
No cleanup/disposition UX.
```

Deferred to later phases: N4 preview/diff/evidence viewer, N5 package ladder controls, N6 draft PR card
+ evidence timeline, N7 cleanup/disposition UX, N8 optional planner integration.

---

## 6. Operator Journey

```text
1. Open workspace -> N2 workspace status card auto-detects the root (read-only).
2. Describe a task in an intake box (free text).
3. System captures the task intent as structured local draft state (TASK_DESCRIBED).
4. Operator declares exact target paths + forbidden paths (TARGETS_DECLARED).
5. System classifies risk from the declared boundaries (RISK_CLASSIFIED).
6. System produces a NON-EXECUTING plan draft for review (PLAN_DRAFTED).
7. System validates the draft shape (PLAN_DRAFT_VALIDATED) or blocks it (PLAN_DRAFT_BLOCKED).
8. System STOPS. It never advances to PREVIEW_READY / AWAITING_APPLY_APPROVAL / apply / package / PR.
```

Every step is read-only/local. No step writes a file, spawns a new process, calls a model, or runs a
chain command. The plan draft is a review artifact, not a runnable plan.

---

## 7. Task Intake Model

Structured, **local/session-only** task intake state (no persistence, no fs, no network in N3):

```text
task_id                 stable session id (provided to the reducer, not invented by pure code)
task_summary            one-line operator summary
operator_intent         free-text description of the desired change
workspace_root          from the N2 read-only workspace detection
declared_target_paths   exact paths the operator intends to touch (no globs)
forbidden_paths         explicit deny list (always includes runtime/services/HQ/Vault/secrets)
risk_level              one of the §11 risk categories
requires_real_tty       true when any future apply would be a real-TTY human gate
requires_human_approval true for any non-READ_ONLY/DOCS_ONLY outcome
draft_status            empty | described | targets-declared | risk-classified | drafted | validated | blocked
created_at              timestamp supplied to the model (pure code does not read the clock)
updated_at              timestamp supplied to the model
```

All task intake state is **local/session-only** in N3 unless a later phase scopes persistence. It is a
pure reducer over operator gestures; it performs no IO.

---

## 8. Task Draft Data Contract

The intake reducer is a pure function `(state, event) -> state` with:

```text
events:   DescribeTask | DeclareTargets | DeclareForbidden | ClassifyRisk | DraftPlan | ValidateDraft | Reset
guards:   declared_target_paths must be exact (no glob chars, no absolute production paths)
          forbidden_paths always supersede declared_target_paths (deny wins)
          a non-READ_ONLY/DOCS_ONLY risk sets requires_human_approval=true
invariant: no event transitions draft_status past "validated"/"blocked" — there is no apply/package/PR event
output:   a TaskDraft object (see §7 fields) that is inert (cannot be run)
```

The contract is offline and deterministic; it never calls a model or a helper.

---

## 9. Plan Draft Model

A **non-executing** draft, not a runnable plan. Required fields:

```text
draft_id
task_id
candidate_steps          human-readable step descriptions (text, not commands)
declared_paths           exact paths (mirrors task intake)
forbidden_paths          explicit deny list
expected_outputs         what the operator expects (descriptive)
risk_notes               from the risk classifier
required_evidence        what a future apply/package lane would have to prove
stop_gates               the STOP conditions that must hold before any future preview/apply
not_executable_reason    a REQUIRED, non-empty string stating why this draft cannot be run by
                         claw or the orchestrator (e.g. "plan draft is a review artifact: it carries
                         no runnable plan schema, no claw plan body, and no executable command")
```

Hard requirement: the plan draft must **not** be directly runnable by `claw` or the orchestrator. It
carries no plan YAML body, no command string, and no schema the runner accepts. `not_executable_reason`
must be present and non-empty; validation (§12) fails closed if it is missing.

---

## 10. Safe Target Boundary Model

```text
allowed disposable paths                 exact, operator-declared, workspace-relative
explicitly declared paths only           no inference of targets
no runtime/services/HQ/Vault             these are always in forbidden_paths
no secrets                               never declared, never displayed
no broad globs                           "**", "*", trailing-slash dirs are rejected
no inferred production paths             absolute / outside-workspace paths are rejected
operator confirmation required           before any future preview/apply lane (N4+), never in N3
```

The boundary model is read-only validation over declared text; it touches no file.

---

## 11. Risk Classification Model

Categories:

```text
READ_ONLY
DOCS_ONLY
DISPOSABLE_FIXTURE
SOURCE_EDIT
RUNTIME_CONFIG
SECRETS_OR_VAULT
DESTRUCTIVE_OR_FORCE
UNKNOWN
```

Rules:

```text
READ_ONLY / DOCS_ONLY        may proceed to draft review.
DISPOSABLE_FIXTURE           requires an explicit future apply lane (N5); draft only in N3.
SOURCE_EDIT                  requires implementation review; draft only in N3.
RUNTIME_CONFIG               STOP.
SECRETS_OR_VAULT             STOP.
DESTRUCTIVE_OR_FORCE         STOP.
UNKNOWN                      STOP (never optimistic; fail closed).
```

A STOP classification routes the state machine to `PLAN_DRAFT_BLOCKED`, never to a draft the operator
could mistake for runnable.

---

## 12. Validation / Linting Model

A pure offline validator over a plan draft, fail-closed:

```text
- not_executable_reason present and non-empty                 (else BLOCKED)
- declared_paths all exact + workspace-relative + non-glob     (else BLOCKED)
- forbidden_paths include runtime/services/HQ/Vault/secrets     (else BLOCKED)
- no declared_path intersects forbidden_paths                   (else BLOCKED)
- risk_level not in {RUNTIME_CONFIG, SECRETS_OR_VAULT,
  DESTRUCTIVE_OR_FORCE, UNKNOWN}                                (else BLOCKED)
- candidate_steps carry no command/plan-body/claw-invocation    (else BLOCKED)
- result: PLAN_DRAFT_VALIDATED or PLAN_DRAFT_BLOCKED(reasons[])
```

The validator never makes a draft runnable; it only certifies the draft is a safe, inert review
artifact or blocks it.

---

## 13. State Machine Extension

N3-only states (refine the early, pre-preview section of the N2 ladder):

```text
TASK_INTAKE_EMPTY
TASK_DESCRIBED
TARGETS_DECLARED
RISK_CLASSIFIED
PLAN_DRAFTED
PLAN_DRAFT_VALIDATED
PLAN_DRAFT_BLOCKED
```

Integration with N2: `TASK_DESCRIBED`, `PLAN_DRAFTED`, and `PLAN_VALIDATED`/`PLAN_DRAFT_VALIDATED`
correspond to the existing N2 ladder signals `taskDescribed` / `planDrafted` / `planValidated`; N3
produces those observations the N2 model already consumes. `TASK_INTAKE_EMPTY`, `TARGETS_DECLARED`,
`RISK_CLASSIFIED`, and `PLAN_DRAFT_BLOCKED` are new intermediate/terminal states.

State transitions must NOT jump to:

```text
PREVIEW_READY
AWAITING_APPLY_APPROVAL
APPLIED
PACKAGE_READY
COMMITTED
PUSHED
DRAFT_PR_OPEN
```

Those are future N4 / N5 / N6 concerns. The N3 reducer has no event that reaches them, and a test must
prove no N3 transition targets any of them.

---

## 14. UI Surface Requirements

```text
task intake box           free-text task description capture
declared paths editor      add/remove exact declared paths (no globs)
forbidden paths display    always shows runtime/services/HQ/Vault/secrets as denied
risk badge                 renders the §11 category; STOP categories are visually distinct
plan draft card            renders the non-executing draft (text steps + not_executable_reason)
plan draft lint results    PLAN_DRAFT_VALIDATED / PLAN_DRAFT_BLOCKED(reasons)
STOP gate banner           always-on (inherited from the panel)
next safe step display     the N2 ladder's read-only recommendation
```

All surfaces are read-only/local. No surface has a `Run-*`/apply/package/PR control. `render.ts` stays
pure; new sections render pre-computed view models exactly like the N2 workspace card.

---

## 15. Evidence Requirements

```text
- Task intake + plan draft state is captured in the existing read-only evidence timeline (text lines).
- Each N3 transition records: event, resulting state, risk_level, validation result.
- The plan draft card surfaces not_executable_reason as evidence the draft is inert.
- No evidence record claims any apply/package/PR happened (none can in N3).
- Evidence is local/session-only; nothing is pushed or persisted to disk by N3.
```

---

## 16. Safety Boundaries

```text
No hidden apply.
No auto-approval.
No auto-merge.
No direct execution button.
No package controls in N3.
No model/broker/runtime/Vault.
No raw :11434 app inference.
No new spawn boundary.
Render remains pure.
Single helper spawn boundary remains preserved.
```

Additional inherited invariants: deny-list wins over declared paths; STOP/UNKNOWN risk fails closed;
the plan draft is structurally non-runnable; no N3 state reaches the apply gate or beyond.

---

## 17. STOP Gates

```text
- STOP if any N3 transition can reach PREVIEW_READY / AWAITING_APPLY_APPROVAL / APPLIED / PACKAGE_READY
  / COMMITTED / PUSHED / DRAFT_PR_OPEN.
- STOP if a plan draft is missing a non-empty not_executable_reason.
- STOP if risk classifies as RUNTIME_CONFIG / SECRETS_OR_VAULT / DESTRUCTIVE_OR_FORCE / UNKNOWN
  (route to PLAN_DRAFT_BLOCKED).
- STOP if a declared path is a glob, absolute, outside the workspace, or intersects forbidden_paths.
- STOP if implementation introduces a new spawn boundary, a model/broker/runtime/Vault call, a raw
  :11434 path, or any apply/package/PR control.
- STOP if render.ts gains impurity (fs/spawn/network/watcher/polling).
```

---

## 18. Testing Strategy

```text
task intake state reducer tests        every event/guard; deny-list wins; no past-validated transition
plan draft validation tests            fail-closed on missing not_executable_reason, globs, deny overlap, STOP risk
risk classifier tests                  every §11 category + the STOP routing rules
render tests                           intake box / paths / risk badge / draft card / lint results render; degrade muted
safety invariant tests                 no N3 transition targets PREVIEW_READY..DRAFT_PR_OPEN; plan draft is non-runnable
guard tests                            run-guards still PASS; no new spawn boundary; render.ts pure
no-control tests                       no apply/package/PR/Run-* control exists in the rendered markup
```

Mirrors the N2 discipline (305 tests, guards PASS, pure render). New tests must keep the suite green.

---

## 19. Phased Implementation Plan

Implementation must not start until this scope + DRAFT prompt are reviewed and merged. Each is its own
approved lane.

```text
N3-A — task intake state model + reducer (pure) + tests
N3-B — safe target boundary + risk classifier (pure) + tests
N3-C — non-executing plan draft model + offline validator (pure) + tests
N3-D — render sections (intake box / paths / risk badge / draft card / lint) + render tests
N3-E — extension wiring (read-only; recomputeViews produces taskDescribed/planDrafted/planValidated
       signals into the existing N2 ladder) + guard/no-control tests
```

All N3-* sub-lanes are read-only/local, commit-locally-only, and never cross the apply gate.

---

## 20. Final Recommendation

Phase N2 (workspace dashboard + read-only state model) is merged and cleaned up. Phase N3 should add
**task intake + a non-executing plan draft UX** that produces the early-ladder signals the N2 model
already consumes, classifies risk, declares safe boundaries, and STOPS before any preview / apply /
package / PR behavior — preserving every N2 safety invariant (pure render, single spawn boundary, no
model/broker/runtime/Vault, no `:11434`).

**Recommendation:** review and merge this scope and the Phase N3 DRAFT implementation prompt first.
Then implement N3 in a separate approved lane, only under the activation token
`APPROVED: Implement Stack-Code Northstar UX Phase N3`. Do not start N4–N8 until N3 is merged. Do not
weaken any safety gate to gain UX convenience.
