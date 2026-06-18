# Stack-Code — Northstar UX Phase N4 Scope: Preview / Diff / Evidence Viewer — 2026-06-18

> **Docs-only scope.** This document designs Phase N4 of the Northstar UX roadmap. It implements
> nothing, edits no IDE/panel source / tests / scripts / Rust / schemas / CI / runtime / services / HQ,
> runs no live A2 (no preview / approval / apply-bundle / apply), runs no package-plan / commit / push /
> pr, opens no PR, calls no model / broker / `/v1/chat/completions` / `/status/vram`, touches no
> Vault/secret, and introduces no raw `:11434` app inference. It is the source-of-truth scope a
> separately-approved N4 implementation lane would build to. It does not authorize implementation.

---

## 1. Executive Summary

Phase N3 added **task intake + a non-executing plan draft** (merged on `main`, PR #150, `a44b588`).
Phase N4 should add a **read-only preview / diff / evidence VIEWER**: a way for the operator to visually
inspect *what a proposed change would look like* — the task summary, the validated safe-target boundary,
the risk category, the non-executing plan draft, and whatever preview / diff / evidence data is already
present — **before** any future execution lane.

```text
N4 is a preview/diff/evidence review layer.
N4 must not execute plans.
N4 must not apply changes.
N4 must not run package controls.
N4 must not open PRs.
N4 must not call model/broker/runtime.
N4 must make proposed changes easier to inspect before any later execution lane.
```

N4 builds directly on N3: the N3 plan draft already states it hands off "to a future, separately-approved
preview/apply lane (N4+)". N4 is the *review* half of that handoff — it renders, it never runs.

---

## 2. Current State

```text
Merged on main:
  N1  Northstar UX gap scope        (PR #147)
  N2  workspace dashboard + read-only state model  (PR #148, 56b8141)
  N3  task intake + non-executing plan draft        (PR #150, a44b588)

N3 delivered (the data N4 reviews):
  - task intake reducer/state (TaskDraft)
  - safe target boundary validation (exact paths; deny-list wins)
  - risk classification (8 categories; STOP/UNKNOWN fail closed)
  - non-executing plan draft (required not_executable_reason; executable steps rejected)
  - offline plan draft validator (PLAN_DRAFT_VALIDATED / PLAN_DRAFT_BLOCKED)
  - N3 state derivation (7 states; never targets the apply gate)
  - N2 ladder signal wiring (taskDescribed/planDrafted/planValidated)
  - pure render sections + tests

Already present in the panel (read-only, helper-sourced) that N4 can surface:
  - chain preview-bundle.json discovery via the print/validate-only audit (read-only)
  - evidence-timeline lines (read-only)
  - Tier-3 evidence snapshot view (operator-provided, read-only)
```

No preview/diff/evidence *viewer* unifies these for the N3 plan-draft flow yet — that is the N4 gap.

---

## 3. Product Problem

```text
- After N3, the operator has a validated, non-executing plan draft — but no unified, visual way to
  review what it would change before any future execution lane.
- Preview / diff / evidence data exists in scattered, chain-level forms (helper audit artifacts,
  evidence lines, snapshot view); nothing presents them together against the N3 plan draft.
- Without a clear "verified vs inferred vs missing vs blocked" view, an operator could over-trust
  incomplete or ambiguous data.
```

N4 closes this by giving a single, honest, read-only review surface — and by failing closed on
ambiguous data.

---

## 4. Source of Truth

```text
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                            N1 roadmap (N4 row: §20 "preview / diff / evidence viewer")
docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md N3 scope (the data N4 reviews)
ide/vscode/a2-harness-panel/src/n3PlanDraft.ts                                   plan draft model + not_executable_reason + validator
ide/vscode/a2-harness-panel/src/n3TaskIntake.ts                                  TaskDraft + reducer
ide/vscode/a2-harness-panel/src/n3State.ts                                       N3 states + n3ToLadderSignals (handoff to N4)
ide/vscode/a2-harness-panel/src/n3RiskClassifier.ts                             risk categories + safe-target boundary
ide/vscode/a2-harness-panel/src/render.ts                                        pure renderer (N4 adds read-only sections here)
ide/vscode/a2-harness-panel/src/discovery.ts                                     read-only audit parsing (preview-bundle / artifacts)
PR #150 (a44b588)                                                               merged N3 implementation
```

---

## 5. N4 Goal

```text
task intake
→ non-executing plan draft
→ preview / diff / evidence review (READ-ONLY)
→ operator understands what would change
→ still no apply / no package / no PR execution from N4 scope
```

N4 makes proposed changes easier to inspect. It is a **viewer**, not an apply engine. It renders data
that is already present (from validated N3 state or explicit read-only helper output); it never
generates preview/diff/evidence by running the chain.

---

## 6. Operator Journey

```text
1. Operator has a validated N3 plan draft (PLAN_DRAFT_VALIDATED) — or a blocked one.
2. Operator opens the N4 review viewer (read-only).
3. The viewer shows: task summary, validated safe-target boundary, risk category, the non-executing
   plan draft, and the preview / diff / evidence readiness — each labelled VERIFIED / INFERRED /
   MISSING / BLOCKED.
4. Where preview/diff/evidence data is present (from the existing read-only helper audit or an
   operator-provided snapshot), the viewer renders it read-only.
5. Where it is missing or ambiguous, the viewer says so honestly and FAILS CLOSED (no optimistic
   rendering, no inferred-as-verified).
6. The viewer shows the operator's next safe step.
7. N4 STOPS. It never runs preview/apply/package/PR and authorizes no execution.
```

---

## 7. Preview Model

```text
- Preview is DISPLAY-ONLY: N4 renders preview data that already exists; it never runs `claw plan run`
  or any preview generator.
- Sources (read-only): the validated N3 plan draft (candidate_steps, declared_paths, expected_outputs);
  and, when present, the chain preview-bundle discovered via the existing print/validate-only audit.
- Preview readiness states: N4_PREVIEW_DATA_MISSING (nothing to show) → N4_PREVIEW_READY (data present
  and VERIFIED/INFERRED).
- The preview surface carries NO "generate preview" / "run preview" control.
```

---

## 8. Diff Viewer Model

```text
- Diff is DISPLAY-ONLY: N4 renders a structured view of declared target paths and (when available
  read-only) the proposed change shape; it never computes a diff by writing files or running apply.
- Minimum N4 diff content (from N3, VERIFIED): declared_target_paths, forbidden_paths (deny-list),
  expected_outputs, and the non_executable_reason.
- Richer file-level diff content is rendered ONLY when it is already present as read-only data; absent
  → N4_DIFF_READY is not reached and the viewer shows MISSING.
- The diff surface writes nothing, runs no apply-bundle, and produces no apply artifact.
```

---

## 9. Evidence Viewer Model

```text
- Evidence is DISPLAY-ONLY: N4 surfaces the evidence the operator can already see — the N3
  required_evidence (boundary check, risk classification, operator-confirmation requirement), the
  read-only evidence-timeline lines, and any operator-provided evidence snapshot.
- Evidence readiness: N4_EVIDENCE_READY only when the evidence set is present and internally consistent
  (e.g. the plan draft is non-executable and the boundary check passed). Otherwise MISSING/BLOCKED.
- N4 freezes nothing to disk, writes no .claw, and pushes no evidence anywhere. It is a live read-only
  view; persistence/freezing is a later phase (N6).
```

---

## 10. Data Inputs and Trust Levels

Every datum the viewer shows carries an explicit trust level, and N4 **fails closed** on ambiguity:

```text
VERIFIED  came from committed source, validated N3 state, or explicit read-only helper output
INFERRED  derived from safe local metadata but not independently validated
MISSING   not present yet
BLOCKED   unsafe or ambiguous (e.g. a STOP risk, an executable-looking step, ambiguous artifacts)

Rules:
  - VERIFIED data may be rendered as established fact.
  - INFERRED data must be labelled inferred (never shown as verified).
  - MISSING data is shown as missing (never green-by-default; mirrors N2/N3 honesty).
  - BLOCKED data routes the UI to a blocked state; N4 fails closed — it never renders ambiguous data as
    if it were a safe, reviewable preview.
```

---

## 11. UI State Model

N4-only read-only states (a review layer over the N3 result):

```text
N4_NOT_READY                  no validated plan draft to review yet
N4_PLAN_DRAFT_PRESENT         a plan draft exists (validated or blocked) and can be reviewed
N4_PREVIEW_DATA_MISSING       no preview data present
N4_PREVIEW_READY              preview data present (VERIFIED/INFERRED)
N4_DIFF_READY                 diff data present (VERIFIED/INFERRED)
N4_EVIDENCE_READY             evidence set present and internally consistent
N4_BLOCKED_UNSAFE_TARGET      a declared target is unsafe / in a forbidden family
N4_BLOCKED_EXECUTABLE_STEP    a candidate step looks executable (must be descriptive only)
N4_BLOCKED_AMBIGUOUS_ARTIFACTS preview/diff/evidence data is ambiguous → fail closed
```

```text
INVARIANT: no N4 state routes to apply / package / PR execution. There is no transition from any N4
state to PREVIEW_READY-as-execution, AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY, COMMITTED,
PUSHED, or DRAFT_PR_OPEN. N4 is read-only; the apply gate remains entirely outside it.
```

---

## 12. Safety Boundaries

```text
- Read-only rendering only.
- No live plan execution (no claw plan run / approve / apply-bundle / apply).
- No generated apply artifact.
- No target writes.
- No .claw writes.
- No package controls.
- No PR controls.
- No hidden shell execution.
- No new spawn boundary (only helperRunner.ts may spawn; render.ts stays pure).
- No model / broker / runtime / Vault call; no raw :11434 app inference.
- No auto-approval, no hidden apply, no auto-merge.
- Fail closed on ambiguous data (BLOCKED states), never optimistic.
```

---

## 13. Non-Goals

```text
- No apply, preview-run, approval-run, apply-bundle-run.
- No package-plan / package-commit / package-push / package-pr.
- No PR open / draft PR card (PR card is N6).
- No evidence freeze/persistence to disk (frozen evidence timeline is N6).
- No model-powered planner (N8).
- No cleanup/disposition UX (N7).
- No execution buttons of any kind.
- No production target writes, no .claw writes.
- No multi-phase implementation in one lane.
```

---

## 14. Candidate Implementation Surface

```text
Likely future implementation surfaces (separately approved N4 lane only):
  ide/vscode/a2-harness-panel/src/n4*        new pure modules (preview/diff/evidence view models + state)
  ide/vscode/a2-harness-panel/src/render.ts  pure read-only N4 sections
  ide/vscode/a2-harness-panel/test/*         unit tests for the above
  docs/runbooks/*                            optional operator runbook
  handoffs/*                                 implementation report

Out of scope for any N4 edit (unless a future prompt proves necessity AND gets separate approval):
  scripts/  rust/  schemas/  runtime/  services/  HQ/  Vault/  CI/  model routing
```

---

## 15. Validation Plan

```text
- Pure N4 modules ship with unit tests: trust-level classification, state derivation, fail-closed on
  ambiguous/blocked data, render sections, and the safety invariant that no N4 state routes to
  apply/package/PR.
- Guard tests: run-guards still PASS; no new spawn boundary; render.ts stays pure; no execution control.
- No-control tests: the rendered N4 markup contains no apply/package/PR/Run-* control.
- The full panel suite stays green (N3 baseline was 364 passing; N4 adds to it).
- No live A2 / package / apply / PR runs as part of validation.
```

---

## 16. STOP Conditions

```text
- STOP if any N4 state can route to apply / package / PR execution.
- STOP if N4 would run preview / approval / apply-bundle / apply, or generate an apply artifact.
- STOP if N4 would write a target file or a .claw artifact.
- STOP if N4 renders ambiguous/blocked data as if it were verified (must fail closed).
- STOP if N4 introduces a new spawn boundary, a model/broker/runtime/Vault call, or raw :11434.
- STOP if render.ts gains impurity (fs/spawn/network/watcher/polling).
- STOP if any execution / Run-* control appears in the N4 markup.
```

---

## 17. Future Lanes

```text
N4 (this scope) — preview / diff / evidence VIEWER (read-only)
N5 — guided package ladder controls (gated)
N6 — draft PR card + frozen evidence timeline
N7 — cleanup / disposition UX
N8 — optional local agent planner integration
```

N4 implementation must not start until this scope + the DRAFT prompt are reviewed and merged, and only
under the activation token. N5–N8 stay deferred.

---

## 18. Final Recommendation

Phase N3 (task intake + non-executing plan draft) is merged and cleaned up. Phase N4 should add a
**read-only preview / diff / evidence viewer** that renders the N3 plan draft and any already-present
preview/diff/evidence data, labels every datum VERIFIED / INFERRED / MISSING / BLOCKED, fails closed on
ambiguity, shows the operator's next safe step, and **never** runs preview/apply/package/PR — preserving
every N2/N3 safety invariant (pure render, single spawn boundary, no model/broker/runtime/Vault, no
`:11434`).

**Recommendation:** review and merge this scope and the Phase N4 DRAFT implementation prompt first.
Then implement N4 in a separate approved lane, only under the activation token
`APPROVED: Implement Stack-Code Northstar UX Phase N4`. Do not start N5–N8 until N4 is merged. Do not
weaken any safety gate to gain review convenience.
