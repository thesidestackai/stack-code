// Northstar Phase N3 — STATE MACHINE EXTENSION (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md
// §13 (state machine extension).
//
// N3 adds the early, pre-preview states. It NEVER targets any of the N2 states
// at or beyond the apply gate (PREVIEW_READY .. DRAFT_PR_OPEN) — those are
// future N4/N5/N6 concerns. `assertN3Safe` and the unit tests enforce that.
// This module also maps the N3 task-draft to the read-only N2-ladder signals
// (taskDescribed / planDrafted / planValidated) the N2 model already consumes.

import { TaskDraft, DraftStatus } from "./n3TaskIntake";

export type N3State =
  | "TASK_INTAKE_EMPTY"
  | "TASK_DESCRIBED"
  | "TARGETS_DECLARED"
  | "RISK_CLASSIFIED"
  | "PLAN_DRAFTED"
  | "PLAN_DRAFT_VALIDATED"
  | "PLAN_DRAFT_BLOCKED";

export const N3_STATES: readonly N3State[] = [
  "TASK_INTAKE_EMPTY",
  "TASK_DESCRIBED",
  "TARGETS_DECLARED",
  "RISK_CLASSIFIED",
  "PLAN_DRAFTED",
  "PLAN_DRAFT_VALIDATED",
  "PLAN_DRAFT_BLOCKED",
];

// N2 ladder states N3 must NEVER reach/recommend — at or beyond the apply gate.
export const N3_FORBIDDEN_TARGETS: readonly string[] = [
  "PREVIEW_READY",
  "AWAITING_APPLY_APPROVAL",
  "APPLIED",
  "PACKAGE_READY",
  "COMMITTED",
  "PUSHED",
  "DRAFT_PR_OPEN",
];

export function deriveN3State(draft: TaskDraft): N3State {
  switch (draft.draft_status) {
    case "empty":
      return "TASK_INTAKE_EMPTY";
    case "described":
      return "TASK_DESCRIBED";
    case "targets-declared":
      return "TARGETS_DECLARED";
    case "risk-classified":
      return "RISK_CLASSIFIED";
    case "drafted":
      return "PLAN_DRAFTED";
    case "validated":
      return "PLAN_DRAFT_VALIDATED";
    case "blocked":
      return "PLAN_DRAFT_BLOCKED";
    default:
      return "TASK_INTAKE_EMPTY";
  }
}

// Invariant guard: an N3 state must be one of the N3 states and must NOT be a
// forbidden (apply-gate-or-beyond) target. Throws on violation. The tests run
// this for every derivable state.
export function assertN3Safe(state: string): N3State {
  if (N3_FORBIDDEN_TARGETS.includes(state)) {
    throw new Error("unsafe N3 state (targets the apply gate or beyond): " + String(state));
  }
  if (!(N3_STATES as readonly string[]).includes(state)) {
    throw new Error("unknown N3 state: " + String(state));
  }
  return state as N3State;
}

// The single recommended next safe step for an N3 state (guidance only).
export function n3NextStepLabel(state: N3State): string {
  switch (state) {
    case "TASK_INTAKE_EMPTY":
      return "Describe a task (free text; capture only)";
    case "TASK_DESCRIBED":
      return "Declare exact target paths (no globs; deny-list always wins)";
    case "TARGETS_DECLARED":
      return "Classify risk from the declared boundaries";
    case "RISK_CLASSIFIED":
      return "Draft a non-executing plan for review";
    case "PLAN_DRAFTED":
      return "Validate the plan draft shape (offline)";
    case "PLAN_DRAFT_VALIDATED":
      return "STOP — draft validated. A future, separately-approved N4 preview/diff lane comes next.";
    case "PLAN_DRAFT_BLOCKED":
      return "STOP — draft blocked. Resolve the risk/boundary problems before any future lane.";
    default:
      return "STOP — unrecognized N3 state; investigate.";
  }
}

// Map the N3 task-draft to the read-only N2-ladder signals. N3 produces exactly
// the early-ladder observations the N2 model already consumes; it never sets a
// signal at or beyond the apply gate.
export interface LadderSignalSlice {
  taskDescribed: boolean;
  planDrafted: boolean;
  planValidated: boolean;
}

export function n3ToLadderSignals(draft: TaskDraft): LadderSignalSlice {
  const s: DraftStatus = draft.draft_status;
  const described = s !== "empty";
  const drafted = s === "drafted" || s === "validated" || s === "blocked";
  const validated = s === "validated";
  return { taskDescribed: described, planDrafted: drafted, planValidated: validated };
}

export interface N3View {
  state: N3State;
  stepLabel: string;
  isBlocked: boolean;
  isTerminal: boolean;
}

export function buildN3View(draft: TaskDraft): N3View {
  const state = assertN3Safe(deriveN3State(draft));
  return {
    state,
    stepLabel: n3NextStepLabel(state),
    isBlocked: state === "PLAN_DRAFT_BLOCKED",
    isTerminal: state === "PLAN_DRAFT_VALIDATED" || state === "PLAN_DRAFT_BLOCKED",
  };
}
