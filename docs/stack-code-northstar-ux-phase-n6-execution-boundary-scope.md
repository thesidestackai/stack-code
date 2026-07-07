# Stack-Code Northstar UX Phase N6 — Execution Boundary Scope — 2026-07-07

> **Docs-only scope.** This document designs Phase N6 of the Northstar UX roadmap. It implements
> nothing, edits no IDE/panel source / tests / scripts / Rust / schemas / CI / runtime / services / HQ,
> runs no live A2 (no preview / approval / apply-bundle / apply), runs no package-plan / package-commit /
> package-push / package-pr, opens no PR, calls no model / broker / `/v1/chat/completions` /
> `/status/vram`, touches no Vault/secret, and introduces no raw `:11434` app inference.
>
> **This scope document does not authorize implementation.**
> **This scope document does not authorize execution.**
> **Implementation requires a separate exact activation token.**
> **Execution requires separate exact sub-token approval per rung.**

---

## 1. Executive Summary

Phase N5 added a **read-only gated-execution readiness board** (merged PR #154, `007ba56`): it derives
per-rung package-ladder readiness from the N3/N4 local state, labels every datum
VERIFIED / INFERRED / MISSING / BLOCKED / EXECUTION_REQUIRED, fails closed on ambiguity, and **runs
nothing**. The operator can now see whether they are ready to move to execution — but the panel gives
them no button to do it.

Phase N6 defines the first **execution-capable boundary** in the panel. It may add direct, per-rung
execution controls, but only under strict, layered, sub-token-gated operator approval. Every N5 safety
invariant must survive N6 intact. N6 must not weaken the apply gate, the merge gate, or the Law 1
(no raw `:11434`) boundary.

```text
N6 is an execution-capable boundary design.
N6 must not auto-approve, auto-merge, or hide apply behind package language.
N6 must not call runtime / model / broker / Vault.
N6 must not introduce raw :11434 app inference.
N6 must preserve every N5 STOP gate and superset its forbidden targets.
N6 must require a separate explicit sub-token per rung before any execution control is shown.
N6 must keep merge human-only.
N6 must keep apply in a separate higher-gated lane (not N6 scope).
```

---

## 2. Current Baseline

```text
Merged on main (2026-07-07):
  N1  Northstar UX gap scope                          (PR #147)
  N2  workspace dashboard + read-only state model     (PR #148, 56b8141)
  N3  task intake + non-executing plan draft          (PR #150, a44b588)
  N4  read-only preview/diff/evidence viewer          (PR #152, 16ae373)
  N5  gated-execution readiness board                 (PR #154, 007ba56)  ← main HEAD

N5 delivered:
  src/n5TrustLevel.ts      — N5TrustLevel extending TrustLevel with EXECUTION_REQUIRED
  src/n5ReadinessModel.ts  — deriveLadderReadiness: 4-rung package ladder readiness (pure, read-only)
  src/n5State.ts           — N5State machine; N5_FORBIDDEN_TARGETS (superset of N4_FORBIDDEN_TARGETS);
                             assertN5Safe (throws on forbidden/unknown); n5NextStepLabel
  src/n5View.ts            — buildN5View: board view from TaskDraft; fail-closed on blocked input
  src/render.ts            — n5Block, n5RungHtml: pure display HTML; zero action buttons
  src/extension.ts         — N5 wiring in recomputeViews + model(); no new spawn boundary
  test/n5*.test.ts         — 5 test suites; 474 total passing
  no execution controls; no spawn/network; no target/.claw writes; no runtime/model/broker/Vault

Panel discipline still in force (N2→N5):
  print/validate-only spawn; single spawn boundary (helperRunner.ts); pure render.ts;
  no Run-* control; fail-closed on ambiguity; no raw :11434; no model/broker/runtime/Vault.

N5_FORBIDDEN_TARGETS (current superset of N4's):
  ...N4_FORBIDDEN_TARGETS (PREVIEW_READY, AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY,
                            COMMITTED, PUSHED, DRAFT_PR_OPEN)
  "EXECUTION_APPROVED"
  "PACKAGE_PLAN_EXECUTING"
  "PACKAGE_COMMIT_EXECUTING"
  "PACKAGE_PUSH_EXECUTING"
  "PACKAGE_PR_EXECUTING"
```

---

## 3. Why N6 Exists

```text
- After N5, the operator knows exactly what is READY, what is BLOCKED, and what is EXECUTION_REQUIRED.
  They can also see the exact command they would run. But they must leave the panel to run it.
- N6 closes that gap by introducing controlled, sub-token-gated execution buttons — but only after
  a full operator approval ceremony that is visible, explicit, and non-automatable.
- Without a defined execution boundary, ad-hoc "just add a button" pressure will erode N5's safety
  invariants. N6 draws the line first, in a scope doc, before any execution-capable code exists.
- N6 also defines which execution rungs are in-scope (package ladder only) and which remain out-of-scope
  (apply, merge, raw :11434), so that future N7+ scopes cannot simply inherit a weaker gate.
```

---

## 4. Non-Goals

```text
- No apply or preview-run / approval-run / apply-bundle-run (these remain a separate higher-gated lane).
- No automatic execution of any kind (no auto-run, no auto-approve, no auto-merge).
- No merge automation (merge is human-only in all N6 states).
- No model / broker / runtime / Vault call.
- No raw :11434 app inference.
- No new spawn boundary beyond the existing helperRunner.ts path.
- No target writes or .claw writes from the panel.
- No multi-phase execution in one lane (each rung is independently sub-token-gated).
- No evidence/readiness persistence to disk in this phase.
- No draft-PR automatic marking-ready or approval (PR-open stays draft-only with sub-token).
- No weakening of any N5 STOP gate.
- No N7+ scope in this document.
```

---

## 5. Threat Model

N6 introduces execution-capable behavior into the panel for the first time. The following threats must
be explicitly closed by the N6 design:

```text
T1 — Rung creep: "READY" silently becomes "running". Guard: sub-token gate before any execution button
     is shown; assertN6Safe throws if any state routes to execution without token.

T2 — Apply conflation: package language hides an apply. Guard: apply is a separate lane with its own
     gate; N6 explicitly forbids any state named or equivalent to APPLIED/AWAITING_APPLY_APPROVAL;
     N6_FORBIDDEN_TARGETS includes those and more.

T3 — Merge automation: a "PR READY" state silently opens/merges. Guard: DRAFT_PR_OPEN and MERGED are
     forbidden targets; no merge control may appear in any N6-gated section.

T4 — Sub-token forgery: operator supplies a sub-token from a prior session. Guard: sub-tokens must be
     checked against the current session's N6 token manifest; stale tokens are not honoured.

T5 — Hidden spawn: execution runs through a new, unaudited code path. Guard: helperRunner.ts remains
     the single spawn boundary; any N6 execution button dispatches ONLY through helperRunner.ts;
     the guard test audits the full src/ tree for new spawn boundaries.

T6 — Vault/secrets bleed: execution picks up a credential from the environment. Guard: N6 must not
     read, display, or log any credential; no Vault API call; the forbidden-surface scan covers this.

T7 — Law 1 bypass: a package/push step introduces a raw :11434 call. Guard: N6_FORBIDDEN_TARGETS
     prevents routing to any model-inference state; the raw-11434 safety scan is mandatory in CI.

T8 — N7+ scope creep: "while we're here, add X". Guard: N6 implementation prompt includes an explicit
     STOP gate for any feature not listed in this scope; N7+ must be a separate scope doc.
```

---

## 6. N6 Capability Classes

N6 introduces three classes of capability, each independently gated:

```text
Class A — Execution-capable display (display readiness WITH an execution button, still sub-token-gated)
  - The execution button is only shown AFTER the operator supplies the rung-specific sub-token.
  - Without the sub-token, the section degrades to the N5 display-only view (no regression).

Class B — Single-rung controlled execution (operator clicks, helperRunner.ts dispatches, output shown)
  - Each rung is individually gated; clicking one rung does not auto-trigger the next.
  - Output is shown in the existing helper output section; no auto-advance.

Class C — Print/copy (N5 behavior, unchanged; always available regardless of sub-token)
  - Inert text; bound to no action; copy-to-clipboard only.
```

```text
Class D (NOT in N6) — Automatic multi-rung execution / auto-advance
Class E (NOT in N6) — Apply / merge / PR-mark-ready / PR-approve
Class F (NOT in N6) — Model / broker / runtime / Vault call
Class G (NOT in N6) — New spawn boundary
```

---

## 7. Readiness vs Execution

N6 introduces the first point at which a panel state can trigger a side-effectful action. The following
boundary applies unconditionally:

```text
READINESS (N5, unchanged)          |  EXECUTION (N6 new, sub-token-gated only)
-----------------------------------|---------------------------------------------------
show readiness board               |  run package-plan via helperRunner.ts (sub-token required)
show per-rung preconditions        |  run package-commit via helperRunner.ts (sub-token required)
show trust labels                  |  run package-push via helperRunner.ts (sub-token required)
show EXECUTION_REQUIRED            |  run package-pr (draft-only) via helperRunner.ts (sub-token required)
print/copy command text            |  (nothing else — apply/merge/runtime remain separate)
```

```text
INVARIANT: READY does not mean "run it". A rung moves from READY (N5) to EXECUTING (N6) ONLY when:
  (a) The operator has supplied the rung-specific sub-token in the current session.
  (b) The rung's readiness is READY (not BLOCKED / NOT_READY / EXECUTION_REQUIRED).
  (c) The operator explicitly clicks the rung's execution button (no auto-advance).
  (d) helperRunner.ts dispatches the rung's helper subcommand (no new spawn boundary).
  (e) assertN6Safe confirms the resulting state is not a forbidden target.
```

---

## 8. Approval / Token Model

N6 uses a two-level token model:

```text
Level 1 — Implementation token (activates the implementation lane):
  APPROVED: Implement Stack-Code Northstar UX Phase N6

  Must appear as the first non-empty line of the implementation prompt.
  Does NOT authorize execution-capable behavior in the panel.
  Authorizes ONLY: source/test implementation of the N6 boundary (pure modules + render + extension wiring).

Level 2 — Execution sub-tokens (enable each rung's execution control at runtime, per session):
  APPROVED: N6 Package Plan Only
  APPROVED: N6 Package Commit Only
  APPROVED: N6 Package Push Only
  APPROVED: N6 Draft PR Only

  These sub-tokens are RUNTIME operator inputs, not compile-time grants.
  Each sub-token enables at most ONE rung's execution control for the current session.
  Sub-tokens are NOT automatically inherited across sessions.
  Sub-tokens are NOT implied by Level 1.
  Supplying a sub-token without the rung being READY is a no-op (the button remains unavailable).
  No sub-token enables apply, merge, runtime, Vault, or :11434.
```

```text
Why separate sub-tokens?
  Without sub-tokens, "implement N6" would implicitly authorize all four rungs simultaneously. By
  requiring a per-rung sub-token at runtime, the operator must make a deliberate, explicit decision
  for each rung, in each session, rather than inheriting broad execution rights from a once-off
  implementation approval. This prevents T1 (rung creep) and T8 (scope creep).
```

---

## 9. Proposed N6 State Machine

N6 extends the N5 state machine. All N5 states are preserved unchanged. N6 adds execution-in-progress
states and execution-complete states, each immediately followed by a panel update (no auto-advance).

```text
Inherited N5 states (unchanged):
  N5_NOT_READY | N5_REVIEW_READY | N5_PACKAGE_PLAN_READY
  N5_PACKAGE_COMMIT_READY | N5_PACKAGE_PUSH_READY | N5_PACKAGE_PR_READY
  N5_BLOCKED_UNSAFE_TARGET | N5_BLOCKED_EXECUTABLE_STEP | N5_BLOCKED_MISSING_EVIDENCE
  N5_BLOCKED_AMBIGUOUS_ARTIFACTS | N5_DEFERRED_REQUIRES_EXECUTION_TOKEN

Proposed N6-only states:
  N6_AWAITING_PACKAGE_PLAN_TOKEN       operator has not yet supplied N6 Package Plan sub-token
  N6_PACKAGE_PLAN_TOKEN_ACTIVE         N6 Package Plan sub-token present; execution button shown
  N6_PACKAGE_PLAN_RUNNING              package-plan helper is executing (in-flight)
  N6_PACKAGE_PLAN_DONE                 package-plan completed; output shown; no auto-advance
  N6_PACKAGE_PLAN_FAILED               package-plan helper exited non-zero; fail closed

  N6_AWAITING_PACKAGE_COMMIT_TOKEN     (analogous)
  N6_PACKAGE_COMMIT_TOKEN_ACTIVE
  N6_PACKAGE_COMMIT_RUNNING
  N6_PACKAGE_COMMIT_DONE
  N6_PACKAGE_COMMIT_FAILED

  N6_AWAITING_PACKAGE_PUSH_TOKEN       (analogous)
  N6_PACKAGE_PUSH_TOKEN_ACTIVE
  N6_PACKAGE_PUSH_RUNNING
  N6_PACKAGE_PUSH_DONE
  N6_PACKAGE_PUSH_FAILED

  N6_AWAITING_DRAFT_PR_TOKEN           (analogous; draft-only)
  N6_DRAFT_PR_TOKEN_ACTIVE
  N6_DRAFT_PR_RUNNING
  N6_DRAFT_PR_DONE
  N6_DRAFT_PR_FAILED

  N6_BLOCKED_TOKEN_MISMATCH            sub-token present but does not match session state; fail closed
  N6_BLOCKED_EXECUTION_REFUSED         helperRunner refused the subcommand (not allowlisted); fail closed
  N6_BLOCKED_MISSING_PRECONDITION      rung became unready between token supply and click; fail closed
```

```text
INVARIANT: an assertN6Safe guard (strict superset of assertN5Safe) throws on every forbidden target
and every unknown state. No N6 execution state routes to apply / merge / runtime / Vault / :11434.
N6_FORBIDDEN_TARGETS ⊇ N5_FORBIDDEN_TARGETS ⊇ N4_FORBIDDEN_TARGETS.
```

---

## 10. Proposed Trust / Evidence Model

N6 inherits N5's trust levels unchanged, including EXECUTION_REQUIRED, and adds two runtime-only
evidence categories:

```text
Inherited (N5 / N4):
  VERIFIED           committed source, validated N3/N4, explicit read-only helper output
  INFERRED           safe local metadata, not independently validated
  MISSING            not present
  BLOCKED            unsafe or ambiguous; wins over everything
  EXECUTION_REQUIRED cannot be proven from read-only data (unchanged: N6 never upgrades this silently)

N6 runtime evidence (observed AFTER a rung executes, never fabricated):
  EXECUTION_OBSERVED helper exited 0; output captured; shown verbatim; never auto-verified
  EXECUTION_FAILED   helper exited non-zero; output shown; rung state transitions to *_FAILED
```

```text
Rules:
  - EXECUTION_REQUIRED never silently becomes VERIFIED after a rung runs. The operator must review
    the output and the next rung's preconditions must re-derive from the observed output.
  - EXECUTION_OBSERVED is not VERIFIED. It is shown as "helper output observed (exit 0)"; the
    operator must judge whether the output constitutes the expected rung result.
  - A rung's failure (EXECUTION_FAILED) locks the current rung in a failed state and does NOT
    auto-unblock downstream rungs. The operator must explicitly reset or proceed after review.
```

---

## 11. Package Ladder Boundary

### 11a. package-plan

```text
Preconditions (all must be VERIFIED before the execution button is shown):
  - N5_PACKAGE_PLAN_READY (N5 readiness board shows READY)
  - N6 Package Plan sub-token active in current session
  - helperRunner.ts has package-plan on its allowlist (existing or explicitly added)
  - No forbidden-family target (deny-list must be re-checked at button-show time, not just at N5 derivation)

N6 behavior:
  - If sub-token is NOT active: show N5 display-only board (no regression). State: N6_AWAITING_PACKAGE_PLAN_TOKEN.
  - If sub-token IS active and rung is READY: show execution button labeled exactly "Run package-plan".
  - Operator clicks: dispatches package-plan subcommand through helperRunner.ts (single spawn boundary).
  - State transitions to N6_PACKAGE_PLAN_RUNNING while in-flight; to N6_PACKAGE_PLAN_DONE (exit 0) or
    N6_PACKAGE_PLAN_FAILED (non-zero exit) when complete.
  - Output is shown verbatim in the existing helper output section.
  - No auto-advance to package-commit.

N6 must NOT:
  - Run package-plan without the sub-token.
  - Run package-plan if any precondition is BLOCKED or NOT_READY (assertN6Safe must throw first).
  - Write a target file, a .claw artifact, or any apply artifact.
  - Call a model, broker, runtime service, or Vault.
  - Introduce a new spawn boundary.
```

### 11b. package-commit

```text
Preconditions (all must be VERIFIED):
  - N6_PACKAGE_PLAN_DONE (package-plan ran and exited 0 in THIS session)
  - N6 Package Commit sub-token active
  - Commit scope verified against declared N4 diff scope (operator-reviewed)
  - No forbidden-family target (re-checked)

N6 behavior (analogous to package-plan with its own token, state, and STOP gate):
  - No auto-advance from package-plan completion.
  - Output shown verbatim; state → N6_PACKAGE_COMMIT_DONE or N6_PACKAGE_COMMIT_FAILED.

N6 must NOT:
  - Treat package-plan success as implicit commit approval.
  - Commit outside the declared scope.
  - Write to the target file or .claw artifacts.
```

### 11c. package-push

```text
Preconditions (all must be VERIFIED):
  - N6_PACKAGE_COMMIT_DONE (package-commit ran and exited 0 in THIS session)
  - N6 Package Push sub-token active
  - Branch identity and no-forbidden-surface re-checked
  - Non-force push only (gh pr --no-force or equivalent; force-push is forbidden)

N6 behavior (analogous):
  - No auto-advance.
  - State → N6_PACKAGE_PUSH_DONE or N6_PACKAGE_PUSH_FAILED.

N6 must NOT:
  - Force-push (git push --force is a banned command at all levels).
  - Push to main or any protected branch without operator explicitly overriding.
  - Introduce a new spawn boundary.
```

### 11d. package-pr

```text
Preconditions (all must be VERIFIED):
  - N6_PACKAGE_PUSH_DONE (package-push ran and exited 0 in THIS session)
  - N6 Draft PR sub-token active
  - PR metadata (title, body, base, head) declared and operator-reviewed
  - Draft-only flag enforced (--draft; no ready-for-review flag)

N6 behavior (analogous):
  - Opens a DRAFT PR only; never opens a ready-for-review PR.
  - No auto-approve, no auto-merge, no mark-ready.
  - State → N6_DRAFT_PR_DONE or N6_DRAFT_PR_FAILED.

N6 must NOT:
  - Mark a PR ready for review automatically.
  - Approve or merge any PR.
  - Re-use the N6 implementation token as authorization for PR open.
  - Open more than one PR per sub-token invocation.
```

---

## 12. Apply Boundary

```text
- apply (claw plan apply / apply-bundle / apply-gate) remains outside N6 scope entirely.
- N6 must NOT name any execution state APPLIED, AWAITING_APPLY_APPROVAL, or any equivalent.
- N6 must NOT write a target file, a .claw artifact, or an apply artifact.
- N6 must NOT hide apply behind package language (a "package-commit DONE" chip is NOT an apply).
- N6 must NOT auto-approve and must expose no auto-approve / hidden-apply path.
- All apply-level behavior requires a separately-scoped, separately-activated N7+ lane.
```

---

## 13. PR Open Boundary

```text
- package-pr in N6 opens a DRAFT PR only, under the N6 Draft PR sub-token.
- The PR is NEVER automatically marked ready, approved, or merged.
- No ready-for-review flag may be set programmatically.
- Merge is human-only at all times (N6 state machine has no MERGED state).
- A future "mark ready" capability requires its own N7+ scope and sub-token.
```

---

## 14. Merge Boundary

```text
- Merge is human-only. No N6 state, action button, or helper subcommand may merge a PR.
- MERGED and PR_APPROVED are N6_FORBIDDEN_TARGETS.
- No panel gesture may trigger gh pr merge, git merge, or any equivalent.
- The panel may DISPLAY a "merged" observation (read-only, from helper output) but may not produce it.
```

---

## 15. Target / .claw Write Boundary

```text
- N6 must NOT write to the target file (the file a claw plan apply would write).
- N6 must NOT create or modify .claw artifacts, apply artifacts, or evidence snapshots.
- Package-plan output is captured from helper stdout only; it is not written to disk by N6.
- The panel never calls writeFile / appendFile / mkdir / rmSync or any equivalent.
- The existing evidence-snapshot acquisition path (operator-paste / print-tier3-evidence) is unchanged.
```

---

## 16. Spawn / Network Boundary

```text
- helperRunner.ts remains the ONE AND ONLY spawn boundary in the panel.
- N6 execution buttons dispatch ONLY through helperRunner.ts and only to allowlisted subcommands.
- No new child_process / execFile / spawn call may be introduced anywhere in src/.
- No network call (fetch, XMLHttpRequest, WebSocket) may be introduced by N6.
- The guards script (run-guards.js) must detect any new spawn boundary and fail the lint gate.
- CI must audit the full src/ tree for new spawn boundaries as a required check.
```

---

## 17. Runtime / Model / Broker / Vault Boundary

```text
- N6 must NOT call any model endpoint (/v1/chat/completions or equivalent).
- N6 must NOT call the local broker (:11435/v1 or any port).
- N6 must NOT call /status/vram or any runtime-status endpoint.
- N6 must NOT read from or write to Vault.
- N6 must NOT read, log, or display any credential or secret.
- Package-plan / package-commit / package-push / package-pr are dispatched through helperRunner.ts
  to the a2-ide-harness.sh helper only; they do not reach runtime/model/broker/Vault paths.
```

---

## 18. Raw :11434 Boundary (Law 1)

```text
- Raw :11434 app inference is unconditionally forbidden in N6, as in all prior phases.
- No N6 execution action may route to the Ollama raw port.
- All app inference routes through :11435 (the SideStack broker); N6 introduces NO inference at all.
- The CI safety scan (grep for 11434) is a required gate and must remain in force after N6.
- N6_FORBIDDEN_TARGETS must include any state that would imply a model call.
```

---

## 19. UI Control Rules

```text
- N6 execution buttons exist ONLY in the execution-capable sections of the panel.
- Each button is labeled unambiguously: "Run package-plan", "Run package-commit", "Run package-push",
  "Open Draft PR". No euphemism (e.g. "proceed", "continue", "apply", "deploy") is permitted.
- Each button carries a data-ui-action attribute identifying the rung and the sub-token requirement.
- Each button is hidden (not merely disabled) until the rung's sub-token is active AND the rung is READY.
- When hidden (N5 display-only fallback), the section degrades gracefully to the N5 readiness board.
- Blocked or failed states show NO execution button — the section degrades to an error/stop display.
- No button in N6 may trigger apply, merge, mark-ready, or any runtime/Vault action.
- The Safety / Stop Gates section (always-on, N2→N6) must be updated to describe N6's execution posture.
```

---

## 20. Copy / Print / Export Rules

```text
- Class C (print/copy) is always available from N5, regardless of N6 sub-token state.
- Copy produces the exact command text only; it does not execute it.
- N6 execution output (helper stdout/stderr) may be exported via the existing "Export Evidence" path.
- No new export behavior that writes to disk may be introduced in N6.
- Printed/copied commands carry a clear label: "Copy command (does not execute)".
```

---

## 21. Audit Evidence Requirements

```text
Before any N6 execution button may be shown, the following must be in the session evidence ledger:
  - N5 readiness board was observed (READY for the target rung) in this session.
  - The rung-specific sub-token was supplied in this session.
  - The previous rung (if any) completed with exit 0 in this session.
  - The operator explicitly clicked the execution button (no auto-dispatch).

After each N6 execution, the following must be recorded:
  - Rung name, subcommand dispatched, exit code, timestamp, and truncated stdout (first 500 chars).
  - Whether the rung was marked DONE or FAILED.

Evidence must NOT include:
  - Credentials, tokens, secrets, or Vault paths.
  - Auto-derived success verdicts (EXECUTION_OBSERVED ≠ VERIFIED).
```

---

## 22. Proposed N6 Forbidden Targets

```text
N6_FORBIDDEN_TARGETS is a STRICT SUPERSET of N5_FORBIDDEN_TARGETS (which is itself a strict superset
of N4_FORBIDDEN_TARGETS). N6 adds the following targets that the N6 state machine must never reach:

Inherited from N5_FORBIDDEN_TARGETS:
  PREVIEW_READY, AWAITING_APPLY_APPROVAL, APPLIED, PACKAGE_READY,
  COMMITTED, PUSHED, DRAFT_PR_OPEN  [N4 inherited]
  EXECUTION_APPROVED, PACKAGE_PLAN_EXECUTING, PACKAGE_COMMIT_EXECUTING,
  PACKAGE_PUSH_EXECUTING, PACKAGE_PR_EXECUTING  [N5 added]

N6 additions (execution-level states that must never be reached without the full token+ready+click ceremony):
  "APPLY_EXECUTING"
  "APPLY_APPROVED"
  "APPLY_DONE"
  "PR_APPROVED"
  "PR_MERGED"
  "MERGED"
  "MODEL_CALL_EXECUTING"
  "BROKER_CALL_EXECUTING"
  "VAULT_READ_EXECUTING"
  "AUTO_APPROVED"
  "HIDDEN_APPLY"
  "PUSH_FORCE"
  "PR_MARK_READY"
```

---

## 23. Proposed assertN6Safe Contract

```text
assertN6Safe(state: string): N6State
  - If state ∈ N6_FORBIDDEN_TARGETS → throw Error("unsafe N6 state (routes to execution or apply gate or beyond): " + state)
  - If state ∉ N6_STATES (known N6 states) → throw Error("unknown N6 state: " + state)
  - Otherwise → return state as N6State

Properties (all must be tested):
  - assertN6Safe(s) succeeds for every s ∈ N6_STATES.
  - assertN6Safe(f) throws for every f ∈ N6_FORBIDDEN_TARGETS.
  - assertN6Safe("UNKNOWN") throws with "unknown N6 state".
  - N6_FORBIDDEN_TARGETS ⊃ N5_FORBIDDEN_TARGETS ⊃ N4_FORBIDDEN_TARGETS (strict containment).
  - No element of N6_STATES appears in N6_FORBIDDEN_TARGETS.
  - N6_FORBIDDEN_TARGETS includes every apply-gate-or-beyond state.
  - N6_FORBIDDEN_TARGETS includes MERGED, PR_APPROVED, MODEL_CALL_EXECUTING, VAULT_READ_EXECUTING.
```

---

## 24. Tests Required Before Implementation Can Pass

```text
n6TrustLevel.test.ts
  - EXECUTION_OBSERVED and EXECUTION_FAILED are distinct from VERIFIED and EXECUTION_REQUIRED.
  - classifyN6Trust fails closed: BLOCKED > MISSING > EXECUTION_REQUIRED > EXECUTION_FAILED > VERIFIED/INFERRED.
  - isN6Reviewable: EXECUTION_OBSERVED is not reviewable-as-VERIFIED.

n6State.test.ts
  - Every N6 state routes to a known, testable state (no wildcard transitions).
  - No N6 state collides with N6_FORBIDDEN_TARGETS.
  - assertN6Safe throws on every N6_FORBIDDEN_TARGET (including all N5/N4 inherited targets).
  - assertN6Safe throws on unknown states.
  - N6_FORBIDDEN_TARGETS is a strict superset of N5_FORBIDDEN_TARGETS.
  - READY-state sub-token gate: N6_PACKAGE_PLAN_TOKEN_ACTIVE only when sub-token present AND READY.
  - No auto-advance: N6_PACKAGE_PLAN_DONE does NOT auto-transition to N6_PACKAGE_COMMIT_TOKEN_ACTIVE.

n6ExecutionModel.test.ts
  - package-plan execution only dispatches when preconditions all VERIFIED and sub-token active.
  - package-commit requires N6_PACKAGE_PLAN_DONE in current session (not just N5 READY).
  - package-push requires N6_PACKAGE_COMMIT_DONE; non-force only.
  - package-pr requires N6_PACKAGE_PUSH_DONE; draft-only flag enforced.
  - A FAILED rung blocks all downstream rungs.
  - No rung is named "apply" or "apply-bundle".
  - No execution button is shown without sub-token (N5 display-only fallback verified).

n6Render.test.ts
  - Execution buttons are absent when sub-token is not active.
  - Execution buttons degrade to N5 display-only view (no broken layout).
  - Button labels are exact: "Run package-plan", "Run package-commit", "Run package-push", "Open Draft PR".
  - No "apply", "merge", "approve", or "mark ready" button appears.
  - Safety / Stop Gates section updated text is present.
  - FAILED state shows no execution button (error/stop display only).
  - data-ui-action attributes correctly identify rung and require sub-token.

n6View.test.ts
  - buildN6View degrades to N5 display-only when no sub-token is active.
  - buildN6View shows execution button only when sub-token active AND rung READY.
  - assertN6Safe is called on every derived state (verified by spy or static analysis).

Spawn/network guard test (existing run-guards.js, must be extended):
  - No new child_process / spawn / execFile reference in src/.
  - No new fetch / XMLHttpRequest / WebSocket reference in src/.
  - helperRunner.ts remains the sole spawn boundary.

Apply/merge boundary test:
  - No N6 state, view, or render section references APPLIED / AWAITING_APPLY_APPROVAL / MERGED / PR_APPROVED.
  - No N6 execution dispatches to "apply", "apply-bundle", or "claw plan apply" subcommands.
```

---

## 25. STOP Gates

```text
STOP if any N6 state can route to apply / merge / Vault / runtime without explicit sub-token + READY + click.
STOP if N6 introduces a new spawn boundary (new child_process / execFile / spawn in src/).
STOP if N6 introduces a network call (fetch / XMLHttpRequest / WebSocket in src/).
STOP if N6 writes to a target file, .claw artifact, or apply artifact.
STOP if N6 adds an auto-approve, hidden-apply, or auto-merge code path.
STOP if N6 marks a PR ready, approves a PR, or merges a PR.
STOP if N6 calls a model, broker, runtime, or Vault endpoint.
STOP if N6 introduces raw :11434 (Law 1 violation).
STOP if N6 runs package/apply/PR without the full sub-token + ready + click ceremony.
STOP if N6 allows stale sub-tokens from prior sessions to unlock execution.
STOP if N6 degrades the N5 display-only fallback (the board must remain visible without tokens).
STOP if run-guards.js does not catch new spawn/network boundaries.
STOP if assertN6Safe can be bypassed or does not throw on all forbidden targets.
STOP if N7+ behavior is introduced in the N6 implementation lane.
STOP if any forbidden surface (scripts/, rust/, runtime/, services/, HQ, Vault, CI) is touched.
STOP if the implementation request arrives without the exact Level 1 activation token as first non-empty line.
```

---

## 26. Implementation Token

```text
This scope document does not authorize implementation.
Implementation requires the following exact activation token as the FIRST NON-EMPTY LINE of the prompt:

  APPROVED: Implement Stack-Code Northstar UX Phase N6

Separately, any execution-capable behavior in the panel requires the following sub-tokens to be
declared in the implementation prompt (each authorizing at most one rung's execution control):

  APPROVED: N6 Package Plan Only
  APPROVED: N6 Package Commit Only
  APPROVED: N6 Package Push Only
  APPROVED: N6 Draft PR Only

The Level 1 token DOES NOT automatically authorize any sub-token.
Sub-tokens MUST be listed explicitly in the implementation prompt for each rung that will receive
an execution control.
An implementation prompt that omits a sub-token MUST NOT implement that rung's execution button.

NOTES:
  - "APPROVED: N6 Draft PR Only" authorizes a draft PR open only; it does not authorize mark-ready,
    approve, or merge.
  - Sub-tokens not listed in the implementation prompt should be treated as DEFERRED, not BLOCKED.
    The implementation may leave the corresponding rung in N5 display-only mode.
  - A sub-token for a rung that does not yet have a READY readiness path should be rejected
    with a STOP gate rather than silently no-oped.
```

---

## 27. Out-of-Scope For N6

```text
The following are explicitly NOT in N6 scope and must not be implemented in the N6 lane:

- apply / apply-gate / apply-bundle / claw plan apply
- claw plan run / claw plan approve
- PR mark-ready / PR approve / PR merge
- Any model / broker / runtime / Vault call
- Raw :11434 inference
- Evidence/readiness persistence to disk
- Multi-rung auto-advance
- Stale-session sub-token inheritance
- Credential display or logging
- Force-push
- New spawn boundary
- Dependency changes (no new npm packages without separate scope)
- CI/CD pipeline changes
- HQ / SideStack services / runtime config changes
- Any N7+ feature
```

---

## 28. Future N7+ Candidates

```text
The following are candidates for a future N7+ scope once N6 is merged and validated:

- PR mark-ready (separate gate: N7 Draft PR → N7 PR Mark-Ready with separate sub-token)
- PR approve (requires human review; automation allowed only with explicit multi-party sub-token)
- Evidence freeze to disk (snapshot after each rung; separate sub-token; read-only write path)
- Apply gate — the highest-risk boundary; requires its own dedicated, separately-scoped lane with
  a multi-step approval ceremony and independent safety review
- Merge automation (NOT recommended without extensive operator-safety research)
- Multi-rung batched execution (would require session-scoped batch sub-token with STOP on first failure)
- Model-powered plan assistant (requires Law 1 compliance review: only via :11435 broker)
```

---

## 29. Operator Decision Points

The N6 implementation team must resolve the following before implementation begins:

```text
D1 — Sub-token delivery mechanism: how does the operator supply sub-tokens at runtime?
     Options: (a) paste into a dedicated input field; (b) VS Code command palette entry;
     (c) a new helper subcommand that validates and activates the token.
     Recommendation: VS Code input box (no new spawn boundary for option c in this phase).

D2 — Sub-token storage: are sub-tokens stored in session state (in-memory only) or persisted?
     Recommendation: in-memory only, no persistence (prevents stale-token threat T4).

D3 — Execution button placement: inline in the N5 rung card, or in a new N6 section below?
     Recommendation: inline in the N5 rung card when sub-token is active (single, clear surface);
     N6 section header should distinguish the execution-capable state from the display-only state.

D4 — Non-zero exit handling: should a FAILED rung allow retry in the same session?
     Recommendation: yes, with explicit "Retry" button (same sub-token, same rung, fresh dispatch);
     automatic retry is forbidden.

D5 — package-pr body: how is the PR body / title / base-branch supplied?
     Recommendation: operator supplies via VS Code input box, shown before the "Open Draft PR" button
     is enabled; pre-filled from task summary and declared target; operator must confirm before dispatch.

D6 — helperRunner allowlist: which subcommands are added for N6 execution?
     Must be explicitly listed in the implementation scope and in helperRunner.ts ALLOWED_SUBCOMMANDS.
     Recommendation: define the exact N6 subcommand names (e.g. "package-plan", "package-commit",
     "package-push", "package-pr") and confirm they are already exposed by the a2-ide-harness.sh helper
     before the N6 implementation begins.

D7 — Guard script extension: does run-guards.js need a new rule for sub-token patterns?
     Recommendation: yes; add a rule that asserts no execution button has data-ui-action without a
     corresponding sub-token-gate attribute; this prevents accidental "always shown" buttons.
```

---

> **This scope document does not authorize implementation.**
> **This scope document does not authorize execution.**
> **Implementation requires: `APPROVED: Implement Stack-Code Northstar UX Phase N6`**
> **Execution sub-tokens are separately required per rung at implementation time.**
