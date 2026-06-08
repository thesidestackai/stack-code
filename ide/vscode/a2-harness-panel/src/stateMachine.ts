// Read-only NEXT-STEP state machine (pure).
//
// Maps the current setup + helper-reported chain state to exactly one
// recommended NEXT SAFE STEP. It GUIDES only: every recommendation is a
// print/validate/select/verify action drawn from the panel's existing safe
// button set. It can NEVER recommend running the A2 chain (no run/approve/apply
// executor) — `assertSafe` and the unit tests enforce that invariant.

import { ChainState } from "./discovery";

export type PanelState =
  | "NO_WORKSPACE"
  | "WORKSPACE_SELECTED"
  | "PLAN_SELECTED"
  | "INPUT_VALIDATED"
  | "NO_PREVIEW_ARTIFACTS"
  | "PREVIEW_READY"
  | "APPROVAL_RESULT_MISSING"
  | "APPROVAL_RESULT_FOUND"
  | "APPLY_BUNDLE_MISSING"
  | "APPLY_BUNDLE_FOUND"
  | "FINAL_VERIFY_READY"
  | "FINAL_MATCH"
  | "FINAL_MISMATCH";

export const PANEL_STATES: readonly PanelState[] = [
  "NO_WORKSPACE",
  "WORKSPACE_SELECTED",
  "PLAN_SELECTED",
  "INPUT_VALIDATED",
  "NO_PREVIEW_ARTIFACTS",
  "PREVIEW_READY",
  "APPROVAL_RESULT_MISSING",
  "APPROVAL_RESULT_FOUND",
  "APPLY_BUNDLE_MISSING",
  "APPLY_BUNDLE_FOUND",
  "FINAL_VERIFY_READY",
  "FINAL_MATCH",
  "FINAL_MISMATCH",
];

// The ONLY recommendations the machine may emit. None is a chain executor.
export type NextSafeStep =
  | "OpenWorkspace"
  | "SelectPlan"
  | "ValidateInput"
  | "PrintPreviewCommand"
  | "SetApprovalOutput"
  | "PrintApprovalCommand"
  | "PrintApplyBundleCommand"
  | "PrintApplyCommand"
  | "VerifyFinalTarget"
  | "Done"
  | "StopInvestigate";

export const SAFE_NEXT_STEPS: readonly NextSafeStep[] = [
  "OpenWorkspace",
  "SelectPlan",
  "ValidateInput",
  "PrintPreviewCommand",
  "SetApprovalOutput",
  "PrintApprovalCommand",
  "PrintApplyBundleCommand",
  "PrintApplyCommand",
  "VerifyFinalTarget",
  "Done",
  "StopInvestigate",
];

export interface StateInput {
  workspaceDetected: boolean;
  planKnown: boolean;
  // True once validate-input has passed in this session (exit 0).
  validated: boolean;
  // Chain state from a parsed audit, or null if no audit ran.
  chainState: ChainState | null;
  // From the audit target-hash check (overrides to FINAL_* when present).
  targetHashChecked: boolean;
  targetHashMatch: boolean | null;
}

// Derive the single most-advanced panel state from the read-only signals.
export function deriveState(input: StateInput): PanelState {
  if (!input.workspaceDetected) {
    return "NO_WORKSPACE";
  }
  if (!input.planKnown) {
    return "WORKSPACE_SELECTED";
  }

  // A completed target-hash check is the strongest terminal signal.
  if (input.targetHashChecked) {
    return input.targetHashMatch ? "FINAL_MATCH" : "FINAL_MISMATCH";
  }

  switch (input.chainState) {
    case "applied":
      return "FINAL_VERIFY_READY";
    case "apply-bundle-ready":
      return "APPLY_BUNDLE_FOUND";
    case "approval-ready":
      return "APPROVAL_RESULT_FOUND";
    case "preview-ready":
      return "PREVIEW_READY";
    case "not-started":
      return input.validated ? "NO_PREVIEW_ARTIFACTS" : "PLAN_SELECTED";
    case "unknown":
      return input.validated ? "NO_PREVIEW_ARTIFACTS" : "PLAN_SELECTED";
    case null:
    default:
      return input.validated ? "INPUT_VALIDATED" : "PLAN_SELECTED";
  }
}

// The single recommended next safe step for a state.
export function nextSafeStep(state: PanelState): NextSafeStep {
  switch (state) {
    case "NO_WORKSPACE":
      return "OpenWorkspace";
    case "WORKSPACE_SELECTED":
      return "SelectPlan";
    case "PLAN_SELECTED":
      return "ValidateInput";
    case "INPUT_VALIDATED":
    case "NO_PREVIEW_ARTIFACTS":
      return "PrintPreviewCommand";
    case "PREVIEW_READY":
    case "APPROVAL_RESULT_MISSING":
      return "PrintApprovalCommand";
    case "APPROVAL_RESULT_FOUND":
    case "APPLY_BUNDLE_MISSING":
      return "PrintApplyBundleCommand";
    case "APPLY_BUNDLE_FOUND":
      return "PrintApplyCommand";
    case "FINAL_VERIFY_READY":
      return "VerifyFinalTarget";
    case "FINAL_MATCH":
      return "Done";
    case "FINAL_MISMATCH":
      return "StopInvestigate";
    default:
      return "StopInvestigate";
  }
}

// Human-readable label for a step (rendered in the next-step section).
export function stepLabel(step: NextSafeStep): string {
  switch (step) {
    case "OpenWorkspace":
      return "Open a workspace folder";
    case "SelectPlan":
      return "Select Plan";
    case "ValidateInput":
      return "Validate Input";
    case "PrintPreviewCommand":
      return "Print Preview Command";
    case "SetApprovalOutput":
      return "Set Approval Output";
    case "PrintApprovalCommand":
      return "Print Approval Command (REAL terminal; human-typed)";
    case "PrintApplyBundleCommand":
      return "Print Apply-Bundle Command";
    case "PrintApplyCommand":
      return "Print Apply Command (printed only; you run it at a real terminal)";
    case "VerifyFinalTarget":
      return "Verify Final Target";
    case "Done":
      return "Done — target matches the expected after_sha256";
    case "StopInvestigate":
      return "STOP — hash mismatch or unexpected state; investigate before acting";
    default:
      return "STOP — unrecognized state; investigate";
  }
}

// Map a step to an EXISTING safe panel button id (so the UI can point the
// operator at the button to click), or null for guidance-only steps.
export function stepButtonId(step: NextSafeStep): string | null {
  switch (step) {
    case "SelectPlan":
      return "select-plan";
    case "ValidateInput":
      return "validate-input";
    case "PrintPreviewCommand":
      return "show-preview-command";
    case "SetApprovalOutput":
      return "set-approval-output";
    case "PrintApprovalCommand":
      return "show-approval-command";
    case "PrintApplyBundleCommand":
      return "show-apply-bundle-command";
    case "PrintApplyCommand":
      return "show-apply-command";
    case "VerifyFinalTarget":
      return "verify-final";
    case "OpenWorkspace":
    case "Done":
    case "StopInvestigate":
    default:
      return null;
  }
}

// Invariant guard: a step must be in the safe set and must not be a chain
// executor. Throws on violation. Exercised by the unit tests for every state.
export function assertSafe(step: NextSafeStep): NextSafeStep {
  if (!SAFE_NEXT_STEPS.includes(step)) {
    throw new Error("unsafe next step (not in SAFE_NEXT_STEPS): " + String(step));
  }
  // No executor verbs. "Print*"/"Verify"/"Select"/"Validate"/"Set" are safe;
  // a "Run*"/"Execute*"/"Approve"/bare "Apply" step would be a violation.
  if (/^(run|execute|approve|applybundle|apply)$/i.test(step)) {
    throw new Error("unsafe next step (chain executor verb): " + String(step));
  }
  return step;
}
