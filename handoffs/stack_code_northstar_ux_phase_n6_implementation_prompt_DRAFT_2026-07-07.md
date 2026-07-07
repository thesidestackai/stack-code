# Stack-Code Northstar UX Phase N6 — Implementation Prompt DRAFT — 2026-07-07

> **DRAFT — not activated.** This prompt is a prepared template for the N6 implementation lane.
> It will be delivered to a new Claude Code session in an isolated worktree. Replace `[OPERATOR]`
> fields before sending. Do NOT activate this in the current scope session.
>
> This document does not authorize N6 implementation. The exact Level 1 activation token must
> appear as the first non-empty line of the prompt at delivery time.
>
> Execution sub-tokens must be listed in the `[SUB-TOKENS DECLARED]` section below before delivery.
> Any rung whose sub-token is not listed must be left in N5 display-only mode by the implementation.

---

## DRAFT PROMPT (to be delivered as a new session message)

---

APPROVED: Implement Stack-Code Northstar UX Phase N6

[OPERATOR: Before delivering this prompt, fill in all [OPERATOR] fields. Verify the worktree is
isolated, the branch is new from origin/main, and the session-preflight has been run and cleared.
Remove all [OPERATOR] comments before sending.]

---

## CLAUDE CODE PROMPT — Stack-Code Northstar UX Phase N6 Implementation

### 0. Activation

The first non-empty line of this prompt is the exact Level 1 activation token:
  `APPROVED: Implement Stack-Code Northstar UX Phase N6`

Without this exact token as the first non-empty line, this prompt is NOT activated. Do not begin
implementation.

---

### 1. Scope

This prompt authorizes implementation of Phase N6 of the Stack-Code Northstar UX roadmap.

**Scope document (source of truth):**
  `docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md`

**N6 adds to the A2 Harness Panel:**
- Per-rung execution controls for the package ladder (plan / commit / push / pr), each gated by a
  runtime sub-token supplied by the operator.
- N6 state machine extending N5 (adds _TOKEN_ACTIVE, _RUNNING, _DONE, _FAILED states per rung).
- `N6_FORBIDDEN_TARGETS` as a strict superset of `N5_FORBIDDEN_TARGETS`.
- `assertN6Safe` throwing on forbidden or unknown N6 states.
- N6TrustLevel extending N5TrustLevel with `EXECUTION_OBSERVED` and `EXECUTION_FAILED`.
- N6 execution model (deriveLadderExecutionReadiness) gating on sub-token + readiness + no blocked state.
- N6 view (buildN6View) producing an N6PanelView from session state; degrades to N5 display-only when
  no sub-token is active.
- N6 render section (n6Block, n6RungHtml) showing execution buttons only when sub-token active + READY.
- Extension wiring (extension.ts): N6 state in SessionState; n6 field in model(); recomputeViews hook.
- 5 test suites: n6TrustLevel, n6State, n6ExecutionModel, n6View, n6Render.

**N6 does NOT:**
- Introduce apply / apply-gate / apply-bundle / claw plan apply.
- Introduce PR mark-ready / approve / merge.
- Call any model / broker / runtime / Vault / :11434.
- Introduce a new spawn boundary (helperRunner.ts remains the single boundary).
- Introduce network calls.
- Write target files, .claw artifacts, or evidence snapshots.
- Auto-advance across rungs.
- Inherit sub-tokens across sessions.
- Touch CI/CD, scripts/, services/, HQ, Rust, n8n, or any runtime config.

---

### 2. Sub-Tokens Declared

[OPERATOR: List the sub-tokens you are authorizing for this implementation. Each sub-token authorizes
ONLY that rung's execution button to be implemented. Rungs not listed must remain in N5 display-only
mode. Remove all [OPERATOR] comments before delivery.]

Example (all four rungs):
```
APPROVED: N6 Package Plan Only
APPROVED: N6 Package Commit Only
APPROVED: N6 Package Push Only
APPROVED: N6 Draft PR Only
```

**Rungs authorized by sub-tokens declared above:**
- [OPERATOR: list which rungs are authorized, e.g., "package-plan, package-commit, package-push, draft-pr"]

**Rungs NOT authorized (must remain N5 display-only):**
- [OPERATOR: list which rungs are deferred, e.g., "none" or "draft-pr"]

**NOTE:** The Level 1 token does NOT authorize any sub-token. Sub-tokens must appear explicitly above.

---

### 3. Forbidden Actions — STOP immediately if any of the following are required

```
STOP — any call to apply / apply-gate / apply-bundle / claw plan apply
STOP — any PR mark-ready / approve / merge automation
STOP — any model / broker / runtime / Vault / :11434 call
STOP — any new child_process / execFile / spawn outside helperRunner.ts
STOP — any new network call (fetch / XMLHttpRequest / WebSocket) in src/
STOP — any write to target files, .claw artifacts, or evidence snapshots
STOP — any auto-advance across rungs (N6_PACKAGE_PLAN_DONE must NOT auto-trigger N6_PACKAGE_COMMIT_TOKEN_ACTIVE)
STOP — any sub-token inheritance across sessions
STOP — any edit to scripts/, services/, HQ, Rust, n8n, CI/CD, or runtime config
STOP — raw :11434 reference anywhere in src/ (Law 1 violation)
STOP — N7+ feature (not in scope)
STOP — implementation token missing as first non-empty line of this prompt
```

---

### 4. Worktree and Branch

**Worktree:** Create under `/mnt/vast-data/git-worktrees/` from `origin/main`.

**Branch name:** `feat/stack-code-northstar-ux-n6-execution-controls-[OPERATOR: date, e.g. 20260714]`

**Control checkout:** `/home/suki/stack-code` — DO NOT EDIT. Read-only reference only.

**Preflight:** Before any mutation, run:
```
APPROVED_WORKTREE=<path> APPROVED_BRANCH=<branch> scripts/session_preflight.sh --strict
```
STOP on unexpected tracked or staged changes.

**Exact-path staging only:** Never `git add .` or `git add -A`. Stage only named files.

---

### 5. Files to Create

```
ide/vscode/a2-harness-panel/src/n6TrustLevel.ts
ide/vscode/a2-harness-panel/src/n6ExecutionModel.ts
ide/vscode/a2-harness-panel/src/n6State.ts
ide/vscode/a2-harness-panel/src/n6View.ts
test/n6TrustLevel.test.ts
test/n6State.test.ts
test/n6ExecutionModel.test.ts
test/n6View.test.ts
test/n6Render.test.ts
```

### 5a. Files to Modify

```
ide/vscode/a2-harness-panel/src/render.ts
  — add N6RungPanelView interface
  — add N6PanelView interface
  — add n6?: N6PanelView | null to RenderModel
  — add n6RungHtml(rung): string (execution buttons only when sub-token active + READY)
  — add n6Block(view): string (full board or N5 display-only fallback; NEVER skips N5 board)
  — add ${n6Block(model.n6)} to renderHtml after ${n5Block(model.n5)}

ide/vscode/a2-harness-panel/src/extension.ts
  — import N6PanelView from ./render
  — import buildN6View from ./n6View
  — add n6: N6PanelView | null to SessionState interface
  — add n6: null to initial session state literal
  — add N6 build block in recomputeViews() after N5 block
  — add n6: session.n6 to model() return object
```

---

### 6. Module Specifications

#### 6a. n6TrustLevel.ts

```typescript
// N6TrustLevel extends N5TrustLevel with two runtime-only evidence categories.
// EXECUTION_OBSERVED: helper exited 0; output captured; shown verbatim; NOT auto-verified.
// EXECUTION_FAILED: helper exited non-zero; output shown; rung locked in failed state.

export type N6TrustLevel =
  | import("./n5TrustLevel").N5TrustLevel
  | "EXECUTION_OBSERVED"
  | "EXECUTION_FAILED";

export const N6_TRUST_LEVELS: readonly N6TrustLevel[] = [
  "VERIFIED", "INFERRED", "MISSING", "BLOCKED", "EXECUTION_REQUIRED",
  "EXECUTION_OBSERVED", "EXECUTION_FAILED",
];

// Priority: BLOCKED > MISSING > EXECUTION_REQUIRED > EXECUTION_FAILED > VERIFIED > INFERRED.
// EXECUTION_OBSERVED never promotes to VERIFIED.
export function classifyN6Trust(levels: N6TrustLevel[]): N6TrustLevel;
export function isN6Reviewable(level: N6TrustLevel): boolean;
export function isN6Blocked(level: N6TrustLevel): boolean;
export function requiresExecutionLane(level: N6TrustLevel): boolean;
export function isExecutionObserved(level: N6TrustLevel): boolean;
```

#### 6b. n6ExecutionModel.ts

```typescript
// Derives execution-ready state per rung, gated by: sub-token active + rung READY + no blocked.
// PURE: no fs/spawn/network. All inputs are observations.

export type ExecutionReadiness =
  | "EXECUTION_READY"    // sub-token active + rung READY + no blocked
  | "AWAITING_TOKEN"     // rung is READY but no sub-token in session
  | "NOT_READY"          // rung preconditions not met
  | "BLOCKED"            // rung is BLOCKED (fail closed)
  | "EXECUTION_REQUIRED" // from N5 (package-commit/push/pr without plan-done)
  | "RUNNING"            // currently executing
  | "DONE"               // executed successfully this session
  | "FAILED";            // executed and failed this session

export interface RungExecutionInput {
  rung: "package-plan" | "package-commit" | "package-push" | "package-pr";
  subTokenActive: boolean;          // operator supplied the rung-specific sub-token this session
  n5Readiness: import("./n5ReadinessModel").RungReadiness; // READY | NOT_READY | BLOCKED | EXECUTION_REQUIRED
  previousRungDoneThisSession: boolean; // for commit/push/pr: the prior rung exited 0 this session
  currentState: "IDLE" | "RUNNING" | "DONE" | "FAILED"; // runtime execution state
}

export function deriveExecutionReadiness(input: RungExecutionInput): ExecutionReadiness;

export interface PackageLadderExecutionReadiness {
  plan: ExecutionReadiness;
  commit: ExecutionReadiness;
  push: ExecutionReadiness;
  pr: ExecutionReadiness;
}

export function deriveLadderExecutionReadiness(inputs: {
  plan: RungExecutionInput;
  commit: RungExecutionInput;
  push: RungExecutionInput;
  pr: RungExecutionInput;
}): PackageLadderExecutionReadiness;

// Rules:
// - package-plan: no previousRungDoneThisSession requirement; just sub-token + N5 READY.
// - package-commit: previousRungDoneThisSession = (plan DONE this session).
// - package-push: previousRungDoneThisSession = (commit DONE this session); non-force only.
// - package-pr: previousRungDoneThisSession = (push DONE this session); draft-only.
// - A FAILED rung has FAILED readiness; downstream rungs remain AWAITING_TOKEN or NOT_READY.
// - No auto-advance: DONE does NOT set the next rung's previousRungDoneThisSession automatically;
//   the caller derives it from the runtime state explicitly.
```

#### 6c. n6State.ts

```typescript
// N6 state machine (strict superset of N5).
// N5 states are preserved unchanged.
// N6 adds _TOKEN_ACTIVE, _RUNNING, _DONE, _FAILED states per rung, plus BLOCKED guard states.

export type N6State =
  // -- N5 inherited (unchanged) --
  | "N5_NOT_READY" | "N5_REVIEW_READY" | "N5_PACKAGE_PLAN_READY"
  | "N5_BLOCKED_UNSAFE_TARGET" | "N5_BLOCKED_EXECUTABLE_STEP"
  | "N5_BLOCKED_MISSING_EVIDENCE" | "N5_BLOCKED_AMBIGUOUS_ARTIFACTS"
  | "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN"
  // -- N6 package-plan --
  | "N6_AWAITING_PACKAGE_PLAN_TOKEN"
  | "N6_PACKAGE_PLAN_TOKEN_ACTIVE"
  | "N6_PACKAGE_PLAN_RUNNING"
  | "N6_PACKAGE_PLAN_DONE"
  | "N6_PACKAGE_PLAN_FAILED"
  // -- N6 package-commit --
  | "N6_AWAITING_PACKAGE_COMMIT_TOKEN"
  | "N6_PACKAGE_COMMIT_TOKEN_ACTIVE"
  | "N6_PACKAGE_COMMIT_RUNNING"
  | "N6_PACKAGE_COMMIT_DONE"
  | "N6_PACKAGE_COMMIT_FAILED"
  // -- N6 package-push --
  | "N6_AWAITING_PACKAGE_PUSH_TOKEN"
  | "N6_PACKAGE_PUSH_TOKEN_ACTIVE"
  | "N6_PACKAGE_PUSH_RUNNING"
  | "N6_PACKAGE_PUSH_DONE"
  | "N6_PACKAGE_PUSH_FAILED"
  // -- N6 draft-pr --
  | "N6_AWAITING_DRAFT_PR_TOKEN"
  | "N6_DRAFT_PR_TOKEN_ACTIVE"
  | "N6_DRAFT_PR_RUNNING"
  | "N6_DRAFT_PR_DONE"
  | "N6_DRAFT_PR_FAILED"
  // -- N6 blocked guards --
  | "N6_BLOCKED_TOKEN_MISMATCH"
  | "N6_BLOCKED_EXECUTION_REFUSED"
  | "N6_BLOCKED_MISSING_PRECONDITION";

// N6_FORBIDDEN_TARGETS: strict superset of N5_FORBIDDEN_TARGETS.
// Add apply/merge/runtime forbidden states (see scope §22).
export const N6_FORBIDDEN_TARGETS: readonly string[];

export function assertN6Safe(state: string): N6State;
// Throws "unsafe N6 state (routes to execution or apply gate or beyond): " + state if in forbidden.
// Throws "unknown N6 state: " + state if not in N6_STATES.

export function isN6BlockedState(s: N6State): boolean;
export function n6NextStepLabel(state: N6State): string;
// Labels must NOT suggest "run" unless the state is *_TOKEN_ACTIVE (where the button is visible).
// DONE states must say "completed; review output before proceeding to the next rung".
// FAILED states must say "STOP — rung failed; review output; retry manually".
// N5-inherited blocked states must preserve the original N5 label text.
```

#### 6d. n6View.ts

```typescript
// Builds the N6PanelView from the full session state.
// Degrades to N5 display-only (no button) when sub-tokens are absent.
// PURE: no fs/spawn/network. All inputs are derived.

import { N5PanelView } from "./render";
import { TaskDraft } from "./taskDraft";
import { N6SessionTokens } from "./n6State"; // set of active sub-tokens this session

export interface N6RungPanelView {
  rung: "package-plan" | "package-commit" | "package-push" | "package-pr";
  purpose: string;
  n5Readiness: string;            // from N5 rung readiness (READY / NOT_READY / BLOCKED / EXECUTION_REQUIRED)
  executionReadiness: string;     // from N6 execution model
  subTokenActive: boolean;
  showExecutionButton: boolean;   // true ONLY when subTokenActive + n5Readiness === "READY" + not BLOCKED/RUNNING/DONE/FAILED
  buttonLabel: string;            // "Run package-plan" | "Run package-commit" | etc. (null when !showExecutionButton)
  dataUiAction: string | null;    // data-ui-action attribute value (null when no button)
  rungState: string;              // N6State for this rung (e.g. N6_PACKAGE_PLAN_TOKEN_ACTIVE)
  executionOutput: string | null; // verbatim stdout/stderr (first 500 chars; null if not yet run)
  exitCode: number | null;        // null if not yet run
  preconditionLines: Array<{ label: string; trust: string }>;
  note: string | null;
}

export interface N6PanelView {
  state: N6State;
  stepLabel: string;
  isBlocked: boolean;
  n5View: N5PanelView;           // always included for degraded display
  ladder: N6RungPanelView[];     // 4 rungs in order: plan, commit, push, pr
  sessionHasAnyToken: boolean;   // true when at least one sub-token is active
  tokensActiveCount: number;     // 0-4
}

export function buildN6View(draft: TaskDraft, tokens: N6SessionTokens): N6PanelView;
// Rules:
// - Calls buildN5View(draft) for the N5 view.
// - Calls deriveLadderExecutionReadiness for each rung.
// - Calls deriveN6State (the top-level aggregated N6 state).
// - Calls assertN6Safe on the derived state (throws if forbidden or unknown).
// - showExecutionButton is true ONLY when: subTokenActive && n5Readiness === "READY"
//   && rungState ∈ {N6_*_TOKEN_ACTIVE}.
// - On any blocked N5 input state, degrades to N5 display-only (no execution buttons shown).
// - draft: TaskDraft may not be null; throw on null input.
```

---

### 7. Render Rules

```typescript
// n6RungHtml(rung: N6RungPanelView): string
// - renders one rung div with data-testid="n6-rung-${rung.rung}"
// - shows execution button (labeled exactly per rung.buttonLabel) ONLY when rung.showExecutionButton
// - the button must carry data-ui-action="${rung.dataUiAction}" and data-requires-token="true"
// - shows execution output section when rung.executionOutput !== null
// - shows FAILED stop banner when rung.rungState ends in _FAILED
// - NO "apply", "merge", "approve", "mark ready", or "deploy" label anywhere

// n6Block(view: N6PanelView | null | undefined): string
// - returns muted hint comment when view is null/undefined (N5 display-only; not a STOP state)
// - when view present:
//   - always renders n5View (N5 display-only board; never suppressed)
//   - when sessionHasAnyToken: adds N6 execution section header and rung cards
//   - when !sessionHasAnyToken: shows "Supply an N6 execution sub-token to enable execution controls."
//   - footer: "Execution controls require a separately-approved execution lane; operations run
//     through helperRunner.ts only; merge is human-only."
//   - NEVER renders an apply / merge / mark-ready / approve button
//   - NEVER suppresses the N5 board (degraded display must always be visible)
```

---

### 8. helperRunner Allowlist Extension

The following subcommands must be added to helperRunner.ts `ALLOWED_SUBCOMMANDS` (if not already present):

```
"package-plan"
"package-commit"
"package-push"
"package-pr"
```

[OPERATOR: Before implementation begins, verify these subcommands are exposed by a2-ide-harness.sh
and behave correctly in the local environment. If not, document as D6 and leave the rung in N5
display-only mode with a TODO comment.]

---

### 9. Guards Script Extension

Extend `run-guards.js` (or its TypeScript equivalent) with the following additional rules:

```
rule: no execution button in src/render.ts has data-ui-action without data-requires-token="true"
rule: no rung button label contains "apply", "merge", "approve", "mark ready", "deploy"
rule: no new child_process/execFile/spawn/fork reference in src/ outside helperRunner.ts
rule: no new fetch/XMLHttpRequest/WebSocket reference in src/
```

[OPERATOR: The guard extension is required before merging N6; it blocks T5 (hidden spawn) and T1
(rung creep via button label). Do not skip it.]

---

### 10. Test Specifications (summary — full bodies per scope §24)

#### n6TrustLevel.test.ts
- EXECUTION_OBSERVED and EXECUTION_FAILED are distinct types; neither equals VERIFIED.
- classifyN6Trust priority: BLOCKED > MISSING > EXECUTION_REQUIRED > EXECUTION_FAILED > VERIFIED > INFERRED.
- isN6Reviewable: EXECUTION_OBSERVED returns false (not verified-equivalent).
- Exactly 7 trust levels in N6_TRUST_LEVELS.

#### n6State.test.ts
- Every N6 state is in N6_STATES; no N6 state is in N6_FORBIDDEN_TARGETS.
- assertN6Safe passes for all N6_STATES.
- assertN6Safe throws for every N6_FORBIDDEN_TARGET (including all N5/N4 inherited targets).
- assertN6Safe throws for unknown string "UNKNOWN_XYZ".
- N6_FORBIDDEN_TARGETS ⊃ N5_FORBIDDEN_TARGETS (strict containment; test with Set difference).
- N5_FORBIDDEN_TARGETS ⊃ N4_FORBIDDEN_TARGETS (strict containment; test with Set difference).
- N6_FORBIDDEN_TARGETS includes "MERGED", "PR_APPROVED", "MODEL_CALL_EXECUTING", "VAULT_READ_EXECUTING",
  "APPLY_EXECUTING", "APPLIED", "AUTO_APPROVED", "HIDDEN_APPLY", "PUSH_FORCE".
- No N6 state named "apply" or containing "APPLY" (test via N6_STATES string scan).
- n6NextStepLabel for *_DONE states mentions "review output".
- n6NextStepLabel for *_FAILED states mentions "STOP".

#### n6ExecutionModel.test.ts
- package-plan: EXECUTION_READY when subTokenActive=true + n5Readiness=READY + state=IDLE.
- package-plan: AWAITING_TOKEN when subTokenActive=false + n5Readiness=READY.
- package-plan: NOT_READY when n5Readiness=NOT_READY.
- package-plan: BLOCKED when n5Readiness=BLOCKED.
- package-commit: NOT_READY when previousRungDoneThisSession=false (even if token active).
- package-commit: EXECUTION_READY only when previousRungDoneThisSession=true + subTokenActive=true + READY.
- FAILED rung: readiness=FAILED; downstream rungs show NOT_READY or AWAITING_TOKEN (not EXECUTION_READY).
- No rung is named "apply" or "apply-bundle" (scan deriveLadderExecutionReadiness rung keys).
- Exactly 4 rungs in the ladder output.

#### n6View.test.ts
- buildN6View with no tokens active: showExecutionButton=false for all rungs; n5View present.
- buildN6View with plan-token active + plan READY: showExecutionButton=true for plan only.
- buildN6View with blocked N5 input: all rungs show no execution button; n5View still present.
- assertN6Safe is called (spy or verified via state derivation); no unbounded state escapes.
- taskDraft: null input → throws.

#### n6Render.test.ts
- n6Block(null): returns muted hint; no execution button.
- n6Block(view) with no tokens: contains N5 board HTML; no execution button; has "Supply an N6…" hint.
- n6Block(view) with plan-token active + plan READY: contains "Run package-plan" button exactly once.
- "Run package-commit", "Run package-push", "Open Draft PR" buttons appear only with their sub-tokens active.
- No button label contains "apply", "merge", "approve", "mark ready", "deploy".
- data-ui-action present on execution buttons; data-requires-token="true" present.
- N5 board section present in all n6Block outputs (never suppressed).
- FAILED rung: no execution button; stop banner present.
- RUNNING rung: no new execution button; spinner/running indicator present (no double-dispatch).
- Footer contains "helperRunner.ts" and "merge is human-only".
- n5Block is still rendered correctly after n6Block is added (regression guard).

---

### 11. Validation Sequence

After all source and test files are written:

1. **Guards:** `node ide/vscode/a2-harness-panel/run-guards.js` → must pass with 0 violations.

2. **Compile:** `npx tsc -p ide/vscode/a2-harness-panel/tsconfig.json --noEmit` → 0 errors.

3. **Tests:**
   ```
   npx tsc -p ide/vscode/a2-harness-panel/tsconfig.test.json
   npx mocha --require out-test/test-setup.js 'out-test/test/**/*.test.js'
   ```
   All tests must pass (N6 suites + full regression suite for N1–N5).

4. **Safety scans (required):**
   - `grep -r "11434" ide/vscode/a2-harness-panel/src/` → NONE (Law 1 check).
   - `grep -r "localhost:11435" ide/vscode/a2-harness-panel/src/` → NONE (no broker call).
   - `grep -rn "child_process\|execFile\|\.spawn\|\.fork" ide/vscode/a2-harness-panel/src/ | grep -v helperRunner` → NONE.
   - `grep -rn "fetch\|XMLHttpRequest\|WebSocket" ide/vscode/a2-harness-panel/src/` → NONE.
   - `grep -rni "apply\|merge\|approve" ide/vscode/a2-harness-panel/src/render.ts | grep 'button\|action'` → NONE.

5. **Forbidden surface scan:**
   - `git diff HEAD --name-only | grep -vE '^ide/vscode/a2-harness-panel/(src|test)|^test/n6'` → NONE.
   - STOP if any file outside the panel src/test surface appears in the diff.

6. **Token checks:**
   - Confirm `N6_FORBIDDEN_TARGETS.length > N5_FORBIDDEN_TARGETS.length` in the n6State.test.ts.
   - Confirm no execution button appears in the rendered HTML without `data-requires-token="true"`.

7. **Commit:**
   ```
   git add ide/vscode/a2-harness-panel/src/n6TrustLevel.ts
   git add ide/vscode/a2-harness-panel/src/n6ExecutionModel.ts
   git add ide/vscode/a2-harness-panel/src/n6State.ts
   git add ide/vscode/a2-harness-panel/src/n6View.ts
   git add ide/vscode/a2-harness-panel/src/render.ts
   git add ide/vscode/a2-harness-panel/src/extension.ts
   git add test/n6TrustLevel.test.ts
   git add test/n6State.test.ts
   git add test/n6ExecutionModel.test.ts
   git add test/n6View.test.ts
   git add test/n6Render.test.ts
   git commit -m "feat(a2): northstar ux phase n6 — per-rung execution controls (sub-token-gated)"
   ```
   [OPERATOR: Stage exactly the files above. Use `git diff --cached --name-only` to verify.
   Do NOT `git add .` or `git add -A`.]

---

### 12. Push and PR Gate

After local commit and validation:

1. Push: `git push -u origin <branch>` (never force-push).
2. Open PR targeting `main` (draft if any STOP gate remains unresolved).
3. PR title: `feat(a2): northstar ux phase n6 — per-rung execution controls (sub-token-gated)`
4. PR body must include:
   - Sub-tokens declared in this implementation.
   - Test count delta (N5 baseline → N6 total).
   - Forbidden surface scan results (all NONE).
   - Safety scan results (all NONE).
   - D1–D7 operator decision resolution (or DEFERRED status).
   - Link to `docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md`.
5. Review gate:
   - Full-diff review required (scope §7 review/execution boundary table; panel discipline check).
   - Approval phrase format: `APPROVED: Squash-merge PR #<N> at exact head <sha>`
   - Do NOT merge without exact head SHA in the approval phrase.

---

### 13. Operator Decision Points (from scope §29)

Before implementation begins, resolve:

```
D1 — Sub-token delivery: VS Code input box (recommendation) or alternative.
     [OPERATOR: confirm before impl]

D2 — Sub-token storage: in-memory only (recommendation) or persisted.
     [OPERATOR: confirm before impl; in-memory is default]

D3 — Execution button placement: inline in N5 rung card (recommendation) or separate section.
     [OPERATOR: confirm before impl]

D4 — Non-zero exit handling: Retry button allowed (recommendation) vs. no retry.
     [OPERATOR: confirm before impl]

D5 — package-pr body: operator input box pre-filled from task summary (recommendation).
     [OPERATOR: confirm before impl; supply sample title/body format]

D6 — helperRunner allowlist: verify package-plan/commit/push/pr are exposed by a2-ide-harness.sh.
     [OPERATOR: pre-verify before impl; block any unverified rung]

D7 — Guards script extension: confirm run-guards.js will be extended with N6 rules (required).
     [OPERATOR: confirm before impl]
```

---

### 14. Out-of-Scope STOP (enforcement)

If any of the following surfaces appear in the diff, STOP and report immediately:

```
scripts/         services/      Rust/           HQ/             .github/
n8n/             runtime/       Vault/          CI/CD configs   package.json (new deps)
docs/ (other than the N6 scope doc, which is already written)
```

---

> **END OF DRAFT PROMPT**

---

## Delivery Checklist (complete before sending the draft prompt above)

```
[ ] D1–D7 operator decisions resolved and noted in the [OPERATOR] fields above.
[ ] Sub-tokens listed in §2 match the rungs the operator wants to enable.
[ ] Worktree created at /mnt/vast-data/git-worktrees/ from origin/main.
[ ] Branch name filled in.
[ ] Session preflight cleared.
[ ] helperRunner allowlist verified for each declared sub-token rung (D6).
[ ] All [OPERATOR] comments removed from the draft before sending.
[ ] Activation token present as first non-empty line ("APPROVED: Implement Stack-Code Northstar UX Phase N6").
```
