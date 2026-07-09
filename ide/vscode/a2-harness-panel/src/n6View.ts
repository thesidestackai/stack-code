// Northstar Phase N6 — execution boundary VIEW MODEL (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md §6/§7/§8.
//
// Operator decisions baked in:
//   D2=A: sub-tokens are in-memory only (no persistence).
//   D3=B: N6 renders in a SEPARATE section below N5 (not inline in N5 rung cards).
//   D4=B: FAILED rung clears its token; fresh sub-token required to retry.
//   D5=A: package-pr body file path supplied by operator via input box.
//
// buildN6View is PURE: no fs, no spawn, no network. It derives each rung's
// display state from the in-memory session and the N5 ladder readiness.

import { N6RungExecState, N6_SUB_TOKEN_COMMIT, N6_SUB_TOKEN_PLAN, N6_SUB_TOKEN_PR, N6_SUB_TOKEN_PUSH, assertN6Safe, deriveN6RungStateName, n6RungStateNote } from "./n6State";

// Minimal structural snapshot of the N5 ladder readiness that N6 needs.
// Satisfies N5PanelView (duck-typed by TypeScript) without creating a
// circular import with render.ts (which imports N6PanelView from this file).
export interface N5LadderForN6 {
  ladder: Array<{ readiness: string }>;
}

// N6-native workspace context: the plan rung is ready when all three are true.
// Derived from the operator-selected workspace/plan fields and the helper-probed
// claw binary path — not from the N3/N4 task-draft pipeline.
export interface N6WorkspaceContext {
  hasWorkspace: boolean;
  hasPlan: boolean;
  hasClawPath: boolean;
}

// In-memory N6 session state (held by extension.ts; never persisted — D2=A).
export interface N6SessionState {
  // Sub-token presence flags (D2=A: in-memory only; cleared on deactivate).
  planTokenActive: boolean;
  commitTokenActive: boolean;
  pushTokenActive: boolean;
  prTokenActive: boolean;
  // Per-rung execution state (D4=B: FAILED clears the corresponding token).
  planExec: N6RungExecState;
  commitExec: N6RungExecState;
  pushExec: N6RungExecState;
  prExec: N6RungExecState;
  // Per-rung captured output (null until run).
  planOutput: string | null;
  commitOutput: string | null;
  pushOutput: string | null;
  prOutput: string | null;
  // Per-rung exit codes (null until run).
  planExitCode: number | null;
  commitExitCode: number | null;
  pushExitCode: number | null;
  prExitCode: number | null;
}

export function emptyN6SessionState(): N6SessionState {
  return {
    planTokenActive:   false,
    commitTokenActive: false,
    pushTokenActive:   false,
    prTokenActive:     false,
    planExec:          "AWAITING_TOKEN",
    commitExec:        "AWAITING_TOKEN",
    pushExec:          "AWAITING_TOKEN",
    prExec:            "AWAITING_TOKEN",
    planOutput:        null,
    commitOutput:      null,
    pushOutput:        null,
    prOutput:          null,
    planExitCode:      null,
    commitExitCode:    null,
    pushExitCode:      null,
    prExitCode:        null,
  };
}

// Per-rung view passed to the render layer.
export interface N6RungView {
  rung: "plan" | "commit" | "push" | "pr";
  // Human-readable labels.
  label: string;       // "package-plan", "package-commit", "package-push", "package-pr"
  buttonLabel: string; // "Run package-plan", "Run package-commit", "Run package-push", "Open Draft PR"
  // VS Code message action ids.
  uiAction: string;    // "n6RunPlan", "n6RunCommit", "n6RunPush", "n6RunPr"
  tokenAction: string; // "n6ActivatePlanToken", "n6ActivateCommitToken", etc.
  // The exact sub-token string the operator must supply.
  expectedToken: string;
  // Derived display flags.
  execState: N6RungExecState;
  // True when the rung's own preconditions are met (does NOT imply button visible).
  isReady: boolean;
  tokenActive: boolean;
  // True only when tokenActive AND isReady; the run button is shown iff this is true.
  showRunButton: boolean;
  // Execution output (null until rung has run).
  output: string | null;
  exitCode: number | null;
  // Step note for operator guidance.
  stepNote: string;
  // Validated N6 state name for this rung (used by assertN6Safe).
  stateName: string;
}

export interface N6PanelView {
  rungs: [N6RungView, N6RungView, N6RungView, N6RungView];
  // True when any rung has a token active or has been run (controls section visibility).
  anyActivity: boolean;
}

// Build the N6 panel view from the current session and N5 ladder readiness.
// Pure: reads only the inputs; no side effects.
//
// ctx (optional): N6-native workspace/plan/claw-path readiness used to gate the
// plan rung. When omitted the plan rung is NOT ready (fail closed). The N5
// ladder (n5) is retained for callers that still pass it but is no longer used
// to determine plan rung readiness — N6 has its own validated workspace/plan fields.
export function buildN6View(
  n5: N5LadderForN6 | null,
  session: N6SessionState,
  ctx?: N6WorkspaceContext,
): N6PanelView {
  // Plan rung: ready when N6-native workspace/plan/claw-path context is all present.
  // Fail closed: no ctx → not ready.
  const planContextReady = ctx
    ? ctx.hasWorkspace && ctx.hasPlan && ctx.hasClawPath
    : false;
  // Downstream rungs: N6 requires prior rung DONE in this session (scope doc §11b-d).
  const commitReady  = session.planExec === "DONE";
  const pushReady    = session.commitExec === "DONE";
  const prReady      = session.pushExec === "DONE";

  const planRung    = buildRungView("plan",   "package-plan",   "Run package-plan",   "n6RunPlan",    "n6ActivatePlanToken",   N6_SUB_TOKEN_PLAN,   session.planExec,   session.planTokenActive,   planContextReady,  session.planOutput,   session.planExitCode);
  const commitRung  = buildRungView("commit", "package-commit", "Run package-commit", "n6RunCommit",  "n6ActivateCommitToken", N6_SUB_TOKEN_COMMIT, session.commitExec, session.commitTokenActive, commitReady,  session.commitOutput, session.commitExitCode);
  const pushRung    = buildRungView("push",   "package-push",   "Run package-push",   "n6RunPush",    "n6ActivatePushToken",   N6_SUB_TOKEN_PUSH,   session.pushExec,   session.pushTokenActive,   pushReady,    session.pushOutput,   session.pushExitCode);
  const prRung      = buildRungView("pr",     "package-pr",     "Open Draft PR",      "n6RunPr",      "n6ActivatePrToken",     N6_SUB_TOKEN_PR,     session.prExec,     session.prTokenActive,     prReady,      session.prOutput,     session.prExitCode);

  const anyActivity =
    session.planTokenActive   || session.commitTokenActive ||
    session.pushTokenActive   || session.prTokenActive     ||
    session.planExec   !== "AWAITING_TOKEN" ||
    session.commitExec !== "AWAITING_TOKEN" ||
    session.pushExec   !== "AWAITING_TOKEN" ||
    session.prExec     !== "AWAITING_TOKEN";

  return { rungs: [planRung, commitRung, pushRung, prRung], anyActivity };
}

function buildRungView(
  rung: "plan" | "commit" | "push" | "pr",
  label: string,
  buttonLabel: string,
  uiAction: string,
  tokenAction: string,
  expectedToken: string,
  exec: N6RungExecState,
  tokenActive: boolean,
  isReady: boolean,
  output: string | null,
  exitCode: number | null,
): N6RungView {
  // showRunButton: ONLY when token is active AND rung preconditions met.
  // RUNNING/DONE/FAILED suppress the button (rung already dispatched).
  const canRun = exec === "TOKEN_ACTIVE";
  const showRunButton = tokenActive && isReady && canRun;

  // Derive and validate the N6 state name (assertN6Safe throws on forbidden state).
  const stateName = deriveN6RungStateName(rung, exec);
  assertN6Safe(stateName);

  return {
    rung,
    label,
    buttonLabel,
    uiAction,
    tokenAction,
    expectedToken,
    execState: exec,
    isReady,
    tokenActive,
    showRunButton,
    output,
    exitCode,
    stepNote: n6RungStateNote(rung, exec),
    stateName,
  };
}
