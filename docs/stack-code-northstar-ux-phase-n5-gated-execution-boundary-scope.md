# Stack-Code — Northstar UX Phase N5 Scope: Gated Execution Boundary / Package-Ladder Controls — 2026-07-01

> **Docs-only scope.** This document designs Phase N5 of the Northstar UX roadmap. It implements
> nothing, edits no IDE/panel source / tests / scripts / Rust / schemas / CI / runtime / services / HQ,
> runs no live A2 (no preview / approval / apply-bundle / apply), runs no package-plan / package-commit /
> package-push / package-pr, opens no PR, calls no model / broker / `/v1/chat/completions` /
> `/status/vram`, touches no Vault/secret, and introduces no raw `:11434` app inference. It is the
> source-of-truth scope a separately-approved N5 implementation lane would build to. It does not
> authorize implementation.

---

## 1. Executive Summary

Phase N3 added **task intake + a non-executing plan draft** (merged, PR #150, `a44b588`). Phase N4 added
a **read-only preview / diff / evidence VIEWER** (merged, PR #152, `16ae373`). The operator can now
describe a task, get a validated non-executing plan draft, and visually inspect what a change would look
like — each datum labelled VERIFIED / INFERRED / MISSING / BLOCKED, failing closed on ambiguity.

Phase N5 should add the **gated execution boundary**: a read-only, honest **readiness model** that tells
the operator *whether* — and *how* — they could move from review into a separately-approved
execution/package lane, **without** turning the panel into an executor.

```text
N5 is a gated-execution boundary design.
N5 must not run apply in this scope lane.
N5 must not run package controls in this scope lane.
N5 must not open PRs in this scope lane.
N5 must not call model/broker/runtime.
N5 must define operator-facing readiness and gate posture before any later execution-capable lane.
```

N5 is the *readiness* half of the review→execution handoff. It renders whether each package-ladder rung
is *ready to be run in a separate lane*; it does not run any rung. The **default recommendation** (§18)
is that N5 implementation may **display readiness and optionally print/copy the exact command an operator
would run elsewhere** — but **direct execution controls require a later, separately-scoped N6 lane**.

---

## 2. Current State

```text
Merged on main:
  N1  Northstar UX gap scope                          (PR #147)
  N2  workspace dashboard + read-only state model     (PR #148, 56b8141)
  N3  task intake + non-executing plan draft          (PR #150, a44b588)
  N4  read-only preview/diff/evidence viewer          (PR #152, 16ae373)  ← main HEAD

N4 delivered (the data N5 reasons about):
  - n4TrustLevel.ts  — TrustLevel = VERIFIED | INFERRED | MISSING | BLOCKED; classifyTrust fails closed
  - n4State.ts       — N4State (N4_NOT_READY .. N4_BLOCKED_AMBIGUOUS_ARTIFACTS); assertN4Safe;
                       N4_FORBIDDEN_TARGETS = the apply-gate-or-beyond states N4 may never reach
  - n4View.ts        — preview / diff / evidence read-only view models
  - render.ts        — pure read-only N4 sections; no execution control
  - read-only wiring from validated N3 local state
  - full panel suite green (396 passing)

Panel discipline still in force (N2→N4): print/validate-only, single spawn boundary (helperRunner.ts),
pure render.ts, no Run-* control, fail-closed on ambiguity.

Package/apply machinery already exists at the CHAIN/helper level (not in the panel UI): the A2 package
ladder (package-plan / package-commit / package-push / package-pr) and the apply gate were exercised in
Tier-4 live smoke (handoffs/a2_tier4_lane_b_live_package_pr_smoke_closeout_2026-06-15.md). N5 must reason
about that ladder's *readiness* without wiring the panel to run it.
```

No **readiness/gate-posture** layer connects the N4 review to that ladder yet — that is the N5 gap.

---

## 3. Product Problem

```text
- After N4, the operator can review a proposed change but has no honest, unified signal for
  "am I ready to move to execution, and what is the exact next safe step?"
- The package ladder (plan → commit → push → pr) and the apply gate exist at the chain/helper level,
  but nothing in the panel states each rung's preconditions, required evidence, and blocked conditions.
- Without an explicit gated-execution boundary, a readiness UI could drift into an executor: a
  "ready" signal is one careless step from a "Run" button. N5 must draw that line first, in a scope
  doc, before any execution-capable code exists.
```

N5 closes this by defining a **read-only readiness model** with a hard, test-enforced boundary between
*reviewing readiness* and *running execution* — and by failing closed on ambiguous or blocked data.

---

## 4. Source of Truth

```text
docs/stack-code-northstar-ux-gap-scope-2026-06-17.md                                  N1 roadmap (Future Lanes: N5 = guided package ladder controls, gated)
docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md                   N4 scope (Future Lanes §17: N5 row) + trust/state model N5 extends
docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md       N3 scope (task/risk/target/plan-draft data)
ide/vscode/a2-harness-panel/src/n4TrustLevel.ts                                        TrustLevel + classifyTrust (N5 extends with EXECUTION_REQUIRED)
ide/vscode/a2-harness-panel/src/n4State.ts                                             N4State + N4_FORBIDDEN_TARGETS + assertN4Safe (N5's forbidden set is a superset)
ide/vscode/a2-harness-panel/src/n4View.ts                                              preview/diff/evidence view models (N5 readiness reads these)
ide/vscode/a2-harness-panel/src/n3PlanDraft.ts                                         plan draft + not_executable_reason + validator
ide/vscode/a2-harness-panel/src/n3RiskClassifier.ts                                    risk categories + safe-target boundary
ide/vscode/a2-harness-panel/src/render.ts                                              pure renderer (N5 adds read-only readiness sections)
handoffs/a2_tier4_lane_b_live_package_pr_smoke_closeout_2026-06-15.md                  chain-level package ladder + apply gate (readiness reference only)
PR #152 (16ae373)                                                                      merged N4 implementation
```

---

## 5. N5 Goal

```text
task intake
→ non-executing plan draft
→ read-only preview / diff / evidence viewer (N4)
→ explicit gated execution READINESS (N5, read-only)
→ operator chooses a separate approved package/apply lane (N6+)
```

N5 makes the review→execution boundary **explicit and honest**. It states, read-only, whether each
package-ladder rung is *ready to be run in a separate lane*, what preconditions and evidence each rung
needs, what is missing or blocked, and the operator's next safe step. It **runs no rung**, opens no PR,
writes no target, and calls no model/broker/runtime. It is a readiness board, not an execution console.

---

## 6. Operator Journey

```text
1. Operator has an N4-reviewed change (validated N3 plan draft + preview/diff/evidence) — or a blocked one.
2. Operator opens the N5 readiness board (read-only).
3. The board shows: task summary, validated safe-target boundary, risk category, the non-executing plan
   draft, the N4 preview/diff/evidence state, and a readiness checklist per package-ladder rung — each
   datum labelled VERIFIED / INFERRED / MISSING / BLOCKED / EXECUTION_REQUIRED.
4. For each rung (plan → commit → push → pr) the board shows: purpose, preconditions met/unmet, evidence
   present/absent, whether operator confirmation is required, and — at most — the exact command the
   operator would run in a separate approved lane (print/copy only, if the implementation adopts that).
5. Where preconditions are unmet, evidence is missing, or data is ambiguous, the board says so honestly
   and FAILS CLOSED (no optimistic "ready", no inferred-as-verified, no silent routing to execution).
6. The board shows the operator next safe step and the separate-lane activation requirement.
7. N5 STOPS. It never runs plan/commit/push/pr or apply, and authorizes no execution.
```

---

## 7. Gated Execution Boundary

The single most important thing N5 defines is the line between **reviewing readiness** and **running
execution**. N5 lives entirely on the review side of that line.

```text
REVIEW SIDE (N5 may do this)            |  EXECUTION SIDE (N5 must NEVER do this)
----------------------------------------|------------------------------------------------
show task/target/risk/plan/preview      |  run claw plan run / approve / apply-bundle / apply
show per-rung readiness + preconditions |  run package-plan / package-commit / package-push / package-pr
show required/missing evidence          |  write a target file / a .claw artifact / an apply artifact
show blocked conditions (fail closed)   |  open / approve / merge / mark-ready a PR
show operator next safe step            |  call a model / broker / runtime / Vault / raw :11434
print/copy the exact command (optional) |  auto-approve, hidden-apply, or bind a command to a Run action
```

```text
INVARIANT: no N5 state routes to execution. N5_FORBIDDEN_TARGETS is a SUPERSET of N4_FORBIDDEN_TARGETS
(PREVIEW_READY-as-execution, AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY, COMMITTED, PUSHED,
DRAFT_PR_OPEN) plus any future execution-capable state. An N5 "…_READY" state means "ready to be run in a
separate approved lane", NOT "run it". Printing or copying a command is display output, never execution.
```

---

## 8. Readiness Model

```text
- Readiness is DISPLAY-ONLY: N5 derives whether a rung COULD be run, from data that already exists
  (validated N3 state, N4 trust/view state, and present read-only helper evidence). It runs nothing to
  find out.
- A rung is READY only when every precondition is VERIFIED and every required piece of evidence is
  present and internally consistent. Otherwise the rung is NOT_READY / BLOCKED / EXECUTION_REQUIRED.
- Readiness never upgrades trust: an INFERRED precondition keeps a rung out of READY; it is shown as
  inferred, never as verified.
- EXECUTION_REQUIRED is an honest "cannot be proven from here" — some facts (e.g. remote push state,
  real apply result) cannot be established without a separate approved execution lane. N5 shows
  EXECUTION_REQUIRED rather than guessing, and never runs anything to resolve it.
- Readiness is a live derivation, not a persisted verdict. Freezing readiness/evidence to disk is a
  later phase, not N5.
```

---

## 9. Package Ladder Control Model

N5 defines the ladder **without running it**. Each rung is a readiness descriptor, not an action.

```text
RUNG            purpose                          preconditions (all VERIFIED to be READY)                    evidence required                          operator confirmation   N5 posture (this scope)          deferred to N6+
--------------  -------------------------------  ----------------------------------------------------------  -----------------------------------------  ----------------------  --------------------------------  ------------------------------
package-plan    assemble the change package      N4 preview/diff VERIFIED; plan non-executable; target safe  plan draft + declared/forbidden paths      not for display          display readiness; print/copy    binding a "run plan" action
package-commit  commit the assembled package     package-plan READY + package evidence present               commit-scope = declared N4 diff scope      yes (scope confirm)      display readiness; print/copy    running the commit
package-push    push the branch                  package-commit READY; remote/branch state known             branch identity; no forbidden surface      yes (push confirm)       display readiness; print/copy    performing the push
package-pr      open the change PR                package-push READY; PR body/base/head known                 PR metadata; draft-only intent             yes (PR-open confirm)    display readiness; print/copy    opening any real PR
```

```text
For EVERY rung, N5 (this scope):
  - MAY display the rung's readiness (READY / NOT_READY / BLOCKED / EXECUTION_REQUIRED) with reasons.
  - MAY, if the implementation adopts option B/C (§18), print or copy the exact command the operator
    would run in a separate approved lane — as inert text, bound to no action.
  - MUST NOT run the rung, bind it to a button, or route any state into it.
  - Defers all direct execution controls to a later, separately-scoped N6 execution-control lane.
```

---

## 10. Apply Boundary Model

The apply gate is the highest-risk boundary in the whole system. N5 treats it as untouchable.

```text
- apply remains the highest-risk boundary; N5 must NOT directly apply.
- N5 must NOT hide apply behind package language (a "package-commit READY" chip is not an apply, and must
  never silently perform one).
- N5 must NOT create target writes.
- N5 must NOT create .claw writes.
- N5 must NOT auto-approve, and must expose no auto-approve / hidden-apply path.
- Where the true apply/remote result cannot be proven from read-only data, N5 shows EXECUTION_REQUIRED,
  never an optimistic "applied".
```

---

## 11. PR Boundary Model

```text
- N5 must NOT open real PRs in the scope lane, and the N5 implementation lane must NOT open real PRs.
- Any future PR-open behavior must remain DRAFT-only.
- Any future PR-open behavior must require an explicit, separate PR-open token (not the N5 activation
  token, and not reused from any earlier phase).
- Future merge remains human-only; no automation may approve, mark-ready, or merge a PR.
- In N5's scope, "package-pr readiness" means only: the board may show that a PR could be opened in a
  separate approved lane, and may print/copy the command — it never opens one.
```

---

## 12. Data Inputs and Trust Levels

N5 extends the N4 trust levels with one execution-honest level and **fails closed** on ambiguity:

```text
VERIFIED            committed source, validated N3/N4 state, or explicit read-only helper/orchestrator output
INFERRED            safe local metadata, not independently validated
MISSING             not present
BLOCKED             unsafe or ambiguous (a STOP risk, an executable-looking step, ambiguous artifacts)
EXECUTION_REQUIRED  cannot be proven without a separate approved execution lane (e.g. real push/apply result)

Rules:
  - VERIFIED data may be rendered as established fact and may make a rung READY.
  - INFERRED data must be labelled inferred, never shown as verified, and never makes a rung READY.
  - MISSING data is shown as missing (never green-by-default; mirrors N2/N3/N4 honesty).
  - BLOCKED data routes the UI to a blocked state; N5 fails closed.
  - EXECUTION_REQUIRED is shown honestly; N5 never runs anything to resolve it.
```

---

## 13. UI State Model

N5-only read-only states (a readiness layer over the N4 result). States never silently route to
apply/package/PR execution:

```text
N5_NOT_READY                      no N4-reviewed change to gauge readiness for yet
N5_REVIEW_READY                   N4 review present; readiness board can be shown
N5_PACKAGE_PLAN_READY             package-plan rung's preconditions/evidence VERIFIED (ready to run in a separate lane)
N5_PACKAGE_COMMIT_READY           package-commit rung ready to run in a separate lane
N5_PACKAGE_PUSH_READY             package-push rung ready to run in a separate lane
N5_PACKAGE_PR_READY               package-pr rung ready to run in a separate lane (draft-only intent)
N5_BLOCKED_UNSAFE_TARGET          a declared target is unsafe / in a forbidden family
N5_BLOCKED_EXECUTABLE_STEP        a candidate step looks executable (must be descriptive only)
N5_BLOCKED_MISSING_EVIDENCE       a required evidence input is absent
N5_BLOCKED_AMBIGUOUS_ARTIFACTS    package/preview/diff/evidence data is ambiguous → fail closed
N5_DEFERRED_REQUIRES_EXECUTION_TOKEN  a rung can only proceed under a separate approved execution lane/token
```

```text
INVARIANT: a "…_READY" state means "ready to be run in a separate approved lane", never "run it". No N5
state equals or transitions to any member of N5_FORBIDDEN_TARGETS (⊇ N4_FORBIDDEN_TARGETS). Blocked and
deferred states win (fail closed) over any ready facet. An assertN5Safe guard (mirroring assertN4Safe)
must throw on any forbidden/unknown state.
```

---

## 14. Safety Boundaries

```text
- Read-only rendering only.
- No live plan execution (no claw plan run / approve / apply-bundle / apply).
- No package-plan / package-commit / package-push / package-pr run.
- No generated apply artifact; no target writes; no .claw writes.
- No PR open / approve / merge / mark-ready (draft-only, separate-token even in future).
- No execution / Run-* control or button of any kind; print/copy is inert text only.
- No hidden shell execution; no new spawn boundary (only helperRunner.ts may spawn; render.ts stays pure).
- No model / broker / runtime / Vault call; no raw :11434 app inference.
- No auto-approval, no hidden apply, no auto-merge.
- Fail closed on ambiguous/blocked/execution-required data; never optimistic.
```

---

## 15. Non-Goals

```text
- No apply, preview-run, approval-run, apply-bundle-run.
- No package-plan / package-commit / package-push / package-pr run.
- No direct execution controls / Run-* buttons (these require a separate N6 execution-control scope).
- No real PR open / merge (draft-only + separate PR-open token, even in a future lane).
- No evidence/readiness freeze or persistence to disk.
- No model-powered planner.
- No cleanup / disposition UX.
- No production target writes, no .claw writes.
- No multi-phase implementation in one lane.
```

---

## 16. Candidate Implementation Surface

```text
Likely future implementation surfaces (separately approved N5 lane only):
  ide/vscode/a2-harness-panel/src/n5*        new pure modules (readiness model + ladder descriptors + state)
  ide/vscode/a2-harness-panel/src/render.ts  pure read-only N5 readiness sections
  ide/vscode/a2-harness-panel/test/*         unit tests for the above
  docs/runbooks/*                            optional operator runbook
  handoffs/*                                 implementation report

Out of scope for any N5 edit (unless a future prompt proves necessity AND gets separate approval):
  scripts/  rust/  schemas/  runtime/  services/  HQ/  Vault/  CI/  model routing
```

---

## 17. Validation Plan

```text
- Pure N5 modules ship with unit tests: trust-level extension (incl. EXECUTION_REQUIRED), readiness
  derivation per rung, state derivation, fail-closed on ambiguous/blocked/missing-evidence, render
  sections, and the safety invariant that no N5 state routes to apply/package/PR (assertN5Safe).
- Guard tests: run-guards still PASS; no new spawn boundary; render.ts stays pure; no execution control.
- No-control tests: the rendered N5 markup contains no apply/package/PR/Run-* control; any printed
  command is inert text bound to no action.
- Superset-invariant test: N5_FORBIDDEN_TARGETS ⊇ N4_FORBIDDEN_TARGETS; assertN5Safe throws on each.
- The full panel suite stays green (N4 baseline 396 passing; N5 adds to it).
- No live A2 / package / apply / PR runs as part of validation.
```

---

## 18. Default Recommendation (Execution-Control Posture)

The scope must explicitly decide whether N5 implementation should allow:

```text
A. read-only readiness display only
B. copy-only command output (operator copies the exact command; runs it elsewhere)
C. print-only external command output (board prints the exact command as inert text)
D. direct package/apply/PR controls
```

```text
DEFAULT RECOMMENDATION: A + optionally B and/or C.
  N5 implementation MAY display readiness and optionally print/copy the exact command an operator would
  run in a separate approved lane. Direct execution controls (D) require a later, separately-scoped N6
  execution-control lane. Print/copy is inert text bound to no action; it is NOT execution.
```

```text
Note (roadmap renumber): the N4 scope's Future Lanes listed N6 as "draft PR card + frozen evidence
timeline". This N5 scope proposes the next EXECUTION-CAPABLE lane as N6, so the draft-PR-card / evidence-
freeze work re-sequences to N7+. Final numbering defers to the N5 scope review.
```

---

## 19. STOP Conditions

```text
- STOP if any N5 state can route to apply / package / PR execution (assertN5Safe must throw).
- STOP if N5 would run preview / approval / apply-bundle / apply, or any package-plan/commit/push/pr rung.
- STOP if N5 would write a target file, a .claw artifact, or an apply artifact.
- STOP if N5 binds a printed/copied command to a Run action, or adds any execution / Run-* control.
- STOP if N5 renders ambiguous/blocked/execution-required data as if it were verified/ready (fail closed).
- STOP if N5 opens / approves / merges / marks-ready any PR (draft-only + separate token even in future).
- STOP if N5 introduces a new spawn boundary, a model/broker/runtime/Vault call, or raw :11434.
- STOP if render.ts gains impurity (fs/spawn/network/watcher/polling).
- STOP if the implementation request asks for direct execution controls — those require a separate N6 scope.
```

---

## 20. Final Recommendation

Phase N4 (read-only preview/diff/evidence viewer) is merged and cleaned. Phase N5 should add a
**read-only gated-execution readiness board**: it renders, against the N4-reviewed change, a per-rung
package-ladder readiness checklist (package-plan → package-commit → package-push → package-pr), labels
every datum VERIFIED / INFERRED / MISSING / BLOCKED / EXECUTION_REQUIRED, fails closed on ambiguity,
shows the operator next safe step and the separate-lane activation requirement, and **never** runs any
rung, apply, or PR — preserving every N2/N3/N4 safety invariant (pure render, single spawn boundary, no
model/broker/runtime/Vault, no `:11434`). Its default posture is **display readiness, optionally
print/copy commands; no direct execution controls** (those are a later N6 lane).

**Recommendation:** review and merge this scope and the Phase N5 DRAFT implementation prompt first. Then
implement N5 in a separate approved lane, only under the activation token
`APPROVED: Implement Stack-Code Northstar UX Phase N5`. Do not start the N6 execution-control lane until
N5 is merged. Do not weaken any safety gate to gain review convenience.
