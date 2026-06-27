// Northstar Phase N4 — read-only UI STATE MODEL (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md §11.
//
// N4 is a read-only review layer over the validated N3 plan draft. It NEVER
// routes to apply / package / PR execution: no N4 state equals any of the N2
// states at or beyond the apply gate. `assertN4Safe` and the tests enforce that.
// Blocked states win (fail closed) over any "ready" facet. PURE: no fs/spawn.

import { RiskCategory } from "./n3RiskClassifier";

export type N4State =
  | "N4_NOT_READY"
  | "N4_PLAN_DRAFT_PRESENT"
  | "N4_PREVIEW_DATA_MISSING"
  | "N4_PREVIEW_READY"
  | "N4_DIFF_READY"
  | "N4_EVIDENCE_READY"
  | "N4_BLOCKED_UNSAFE_TARGET"
  | "N4_BLOCKED_EXECUTABLE_STEP"
  | "N4_BLOCKED_AMBIGUOUS_ARTIFACTS";

export const N4_STATES: readonly N4State[] = [
  "N4_NOT_READY",
  "N4_PLAN_DRAFT_PRESENT",
  "N4_PREVIEW_DATA_MISSING",
  "N4_PREVIEW_READY",
  "N4_DIFF_READY",
  "N4_EVIDENCE_READY",
  "N4_BLOCKED_UNSAFE_TARGET",
  "N4_BLOCKED_EXECUTABLE_STEP",
  "N4_BLOCKED_AMBIGUOUS_ARTIFACTS",
];

export function isBlockedState(s: N4State): boolean {
  return (
    s === "N4_BLOCKED_UNSAFE_TARGET" ||
    s === "N4_BLOCKED_EXECUTABLE_STEP" ||
    s === "N4_BLOCKED_AMBIGUOUS_ARTIFACTS"
  );
}

// N2 states at or beyond the apply gate that N4 must NEVER reach/route to.
export const N4_FORBIDDEN_TARGETS: readonly string[] = [
  "PREVIEW_READY", // the N2/chain apply-preview execution state (NOT the N4 read-only facet)
  "AWAITING_APPLY_APPROVAL",
  "APPLIED",
  "PACKAGE_READY",
  "COMMITTED",
  "PUSHED",
  "DRAFT_PR_OPEN",
];

// Read-only inputs derived from the validated N3 plan draft + any present
// read-only data. Every field is an observation; N4 invents nothing.
export interface N4Inputs {
  hasPlanDraft: boolean;
  riskLevel: RiskCategory | null;
  // A declared target sits in an always-forbidden family (deny-list violated).
  hasForbiddenFamilyTarget: boolean;
  // The plan draft is provably non-executable (planDraftIsNonExecutable).
  planNonExecutable: boolean;
  // The N3 offline validator result, or null if not validated.
  validationStatus: "PLAN_DRAFT_VALIDATED" | "PLAN_DRAFT_BLOCKED" | null;
  // Present read-only data per facet.
  hasPreviewData: boolean;
  hasDiffData: boolean;
  hasEvidenceData: boolean;
}

// Risk categories that make a TARGET unsafe (deny-list / secrets / runtime).
function unsafeTargetRisk(r: RiskCategory | null): boolean {
  return r === "SECRETS_OR_VAULT" || r === "RUNTIME_CONFIG";
}

// Risk categories that make the artifacts AMBIGUOUS / fail-closed.
function ambiguousRisk(r: RiskCategory | null): boolean {
  return r === "UNKNOWN" || r === "DESTRUCTIVE_OR_FORCE" || r === null;
}

// Derive the single primary N4 state. Blocked states win (fail closed); then the
// most-advanced ready facet; then plan-present / preview-missing / not-ready.
export function deriveN4State(input: N4Inputs): N4State {
  if (!input.hasPlanDraft) {
    return "N4_NOT_READY";
  }

  // Fail-closed blocked checks (highest priority, in safety order).
  if (input.hasForbiddenFamilyTarget || unsafeTargetRisk(input.riskLevel)) {
    return "N4_BLOCKED_UNSAFE_TARGET";
  }
  if (!input.planNonExecutable) {
    return "N4_BLOCKED_EXECUTABLE_STEP";
  }
  if (input.validationStatus === "PLAN_DRAFT_BLOCKED" || ambiguousRisk(input.riskLevel)) {
    return "N4_BLOCKED_AMBIGUOUS_ARTIFACTS";
  }

  // Ready facets, most-advanced first.
  if (input.hasEvidenceData) {
    return "N4_EVIDENCE_READY";
  }
  if (input.hasDiffData) {
    return "N4_DIFF_READY";
  }
  if (input.hasPreviewData) {
    return "N4_PREVIEW_READY";
  }
  return "N4_PREVIEW_DATA_MISSING";
}

// Invariant guard: an N4 state must be a known N4 state and must NOT be a
// forbidden (apply-gate-or-beyond) target. Throws on violation.
export function assertN4Safe(state: string): N4State {
  if (N4_FORBIDDEN_TARGETS.includes(state)) {
    throw new Error("unsafe N4 state (routes to the apply gate or beyond): " + String(state));
  }
  if (!(N4_STATES as readonly string[]).includes(state)) {
    throw new Error("unknown N4 state: " + String(state));
  }
  return state as N4State;
}

export function n4NextStepLabel(state: N4State): string {
  switch (state) {
    case "N4_NOT_READY":
      return "Produce a validated N3 plan draft first; there is nothing to review yet.";
    case "N4_PLAN_DRAFT_PRESENT":
      return "Plan draft present. Review the preview / diff / evidence below (read-only).";
    case "N4_PREVIEW_DATA_MISSING":
      return "No preview data present. N4 shows nothing it cannot verify; it runs nothing to produce it.";
    case "N4_PREVIEW_READY":
      return "Preview ready (read-only). Review what would change; N4 runs no preview/apply.";
    case "N4_DIFF_READY":
      return "Diff ready (read-only). Review declared/forbidden paths and expected outputs.";
    case "N4_EVIDENCE_READY":
      return "Evidence ready (read-only). A future, separately-approved N5 lane handles gated execution.";
    case "N4_BLOCKED_UNSAFE_TARGET":
      return "STOP — a declared target is unsafe (forbidden family / secrets / runtime). Fail closed.";
    case "N4_BLOCKED_EXECUTABLE_STEP":
      return "STOP — a plan step looks executable. The draft must be descriptive only. Fail closed.";
    case "N4_BLOCKED_AMBIGUOUS_ARTIFACTS":
      return "STOP — preview/diff/evidence data is ambiguous or blocked. Fail closed; render nothing as verified.";
    default:
      return "STOP — unrecognized N4 state; investigate.";
  }
}
