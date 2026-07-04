// Northstar Phase N5 — read-only UI STATE MODEL (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n5-gated-execution-boundary-scope.md §13.
//
// N5 is a read-only readiness board over the N4-reviewed change. It NEVER
// routes to apply / package / PR execution: no N5 state equals any execution-
// capable target. `assertN5Safe` and the tests enforce this invariant.
// N5_FORBIDDEN_TARGETS is a strict superset of N4_FORBIDDEN_TARGETS.
// Blocked states win (fail closed) over any ready facet. PURE: no fs/spawn.

import { N4State, N4_FORBIDDEN_TARGETS } from "./n4State";
import { RungReadiness } from "./n5ReadinessModel";

export type N5State =
  | "N5_NOT_READY"
  | "N5_REVIEW_READY"
  | "N5_PACKAGE_PLAN_READY"
  | "N5_PACKAGE_COMMIT_READY"
  | "N5_PACKAGE_PUSH_READY"
  | "N5_PACKAGE_PR_READY"
  | "N5_BLOCKED_UNSAFE_TARGET"
  | "N5_BLOCKED_EXECUTABLE_STEP"
  | "N5_BLOCKED_MISSING_EVIDENCE"
  | "N5_BLOCKED_AMBIGUOUS_ARTIFACTS"
  | "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN";

export const N5_STATES: readonly N5State[] = [
  "N5_NOT_READY",
  "N5_REVIEW_READY",
  "N5_PACKAGE_PLAN_READY",
  "N5_PACKAGE_COMMIT_READY",
  "N5_PACKAGE_PUSH_READY",
  "N5_PACKAGE_PR_READY",
  "N5_BLOCKED_UNSAFE_TARGET",
  "N5_BLOCKED_EXECUTABLE_STEP",
  "N5_BLOCKED_MISSING_EVIDENCE",
  "N5_BLOCKED_AMBIGUOUS_ARTIFACTS",
  "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN",
];

export function isN5BlockedState(s: N5State): boolean {
  return (
    s === "N5_BLOCKED_UNSAFE_TARGET" ||
    s === "N5_BLOCKED_EXECUTABLE_STEP" ||
    s === "N5_BLOCKED_MISSING_EVIDENCE" ||
    s === "N5_BLOCKED_AMBIGUOUS_ARTIFACTS"
  );
}

// N5_FORBIDDEN_TARGETS is a strict superset of N4_FORBIDDEN_TARGETS. It adds
// execution-side states that N5 must never route to (apply-gate plus package
// execution states plus explicit-approval states).
export const N5_FORBIDDEN_TARGETS: readonly string[] = [
  // All N4 apply-gate-or-beyond targets (inherited):
  ...N4_FORBIDDEN_TARGETS,
  // Additional execution-side states N5 must never reach:
  "EXECUTION_APPROVED",
  "PACKAGE_PLAN_EXECUTING",
  "PACKAGE_COMMIT_EXECUTING",
  "PACKAGE_PUSH_EXECUTING",
  "PACKAGE_PR_EXECUTING",
];

export interface N5Inputs {
  n4State: N4State;
  packagePlanReadiness: RungReadiness;
  hasEvidenceData: boolean;
}

// Derive the single primary N5 state. Blocked states win (fail closed). Most-
// advanced ready state next. N5_NOT_READY is the floor for not-yet-reviewed.
export function deriveN5State(input: N5Inputs): N5State {
  // Blocked N4 states map to corresponding N5 blocked states (fail closed, wins first).
  if (input.n4State === "N4_BLOCKED_UNSAFE_TARGET") {
    return "N5_BLOCKED_UNSAFE_TARGET";
  }
  if (input.n4State === "N4_BLOCKED_EXECUTABLE_STEP") {
    return "N5_BLOCKED_EXECUTABLE_STEP";
  }
  if (input.n4State === "N4_BLOCKED_AMBIGUOUS_ARTIFACTS") {
    return "N5_BLOCKED_AMBIGUOUS_ARTIFACTS";
  }

  // N4 must be at evidence-ready for N5 to compute a readiness board.
  if (input.n4State !== "N4_EVIDENCE_READY") {
    return "N5_NOT_READY";
  }

  // N4 is at EVIDENCE_READY. Required evidence must be present.
  if (!input.hasEvidenceData) {
    return "N5_BLOCKED_MISSING_EVIDENCE";
  }

  // Derive from package-plan readiness (the highest rung provable read-only).
  if (input.packagePlanReadiness === "BLOCKED") {
    return "N5_BLOCKED_AMBIGUOUS_ARTIFACTS";
  }
  if (input.packagePlanReadiness === "READY") {
    return "N5_PACKAGE_PLAN_READY";
  }
  if (input.packagePlanReadiness === "EXECUTION_REQUIRED") {
    return "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN";
  }

  // packagePlanReadiness === "NOT_READY": board shows but plan not yet READY.
  return "N5_REVIEW_READY";
}

// Invariant guard: an N5 state must be a known N5 state and must NOT be a
// forbidden (execution-capable or apply-gate-or-beyond) target. Throws on violation.
export function assertN5Safe(state: string): N5State {
  if (N5_FORBIDDEN_TARGETS.includes(state)) {
    throw new Error(
      "unsafe N5 state (routes to execution or the apply gate or beyond): " + String(state),
    );
  }
  if (!(N5_STATES as readonly string[]).includes(state)) {
    throw new Error("unknown N5 state: " + String(state));
  }
  return state as N5State;
}

export function n5NextStepLabel(state: N5State): string {
  switch (state) {
    case "N5_NOT_READY":
      return "No N4-reviewed change to assess readiness for yet. Produce a validated plan draft and N4 evidence first.";
    case "N5_REVIEW_READY":
      return "N4 review present. Readiness board is shown; one or more package-plan preconditions are not yet VERIFIED.";
    case "N5_PACKAGE_PLAN_READY":
      return "package-plan is READY (all preconditions VERIFIED). Ready for a separately-approved execution lane. N5 does not run it.";
    case "N5_PACKAGE_COMMIT_READY":
      return "package-commit is READY (separately confirmed). Ready for a separately-approved execution lane.";
    case "N5_PACKAGE_PUSH_READY":
      return "package-push is READY (separately confirmed). Ready for a separately-approved execution lane.";
    case "N5_PACKAGE_PR_READY":
      return "package-pr is READY (separately confirmed, draft-only intent). Ready for a separately-approved execution lane.";
    case "N5_BLOCKED_UNSAFE_TARGET":
      return "STOP — a declared target is unsafe (forbidden family / secrets / runtime). Fail closed.";
    case "N5_BLOCKED_EXECUTABLE_STEP":
      return "STOP — a plan step looks executable. The draft must be descriptive only. Fail closed.";
    case "N5_BLOCKED_MISSING_EVIDENCE":
      return "STOP — required evidence is absent. N5 cannot show honest readiness; fail closed.";
    case "N5_BLOCKED_AMBIGUOUS_ARTIFACTS":
      return "STOP — package/preview/diff/evidence data is ambiguous or blocked. Fail closed.";
    case "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN":
      return "Some rungs are EXECUTION_REQUIRED — they cannot be proven from read-only data. A separately-approved execution lane is required to proceed.";
    default:
      return "STOP — unrecognized N5 state; investigate.";
  }
}
