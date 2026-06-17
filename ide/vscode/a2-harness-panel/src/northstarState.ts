// Read-only NORTHSTAR state model (pure) — Phase N2.
//
// Source of truth: docs/stack-code-northstar-ux-gap-scope-2026-06-17.md §12.
//
// This is a SUPERSET of the print/validate-only next-step machine in
// stateMachine.ts. Where that machine tracks the chain (preview→approval→apply
// →verify), this model tracks the full Northstar operator ladder
// (workspace→task→plan→preview→apply→package→draft-PR→evidence→disposition).
//
// It GUIDES only. Like stateMachine.ts it is PURE: it derives exactly one state
// from read-only observation signals and recommends exactly one next safe step.
// It NEVER triggers apply / package-commit / package-push / package-pr / merge,
// and it NEVER auto-advances past AWAITING_APPLY_APPROVAL — every state at or
// beyond APPLIED is entered ONLY when its own human-gated event was OBSERVED
// (read-only), never inferred or automated. `assertNorthstarSafe`,
// `isAutoAdvanceAllowed`, and the unit tests enforce these invariants.

export type NorthstarState =
  | "NO_WORKSPACE"
  | "WORKSPACE_READY"
  | "TASK_DESCRIBED"
  | "PLAN_DRAFTED"
  | "PLAN_VALIDATED"
  | "PREVIEW_READY"
  | "AWAITING_APPLY_APPROVAL"
  | "APPLIED"
  | "PACKAGE_READY"
  | "COMMITTED"
  | "PUSHED"
  | "DRAFT_PR_OPEN"
  | "EVIDENCE_FROZEN"
  | "DISPOSITION_PENDING"
  | "CLOSED_RETAINED"
  | "HUMAN_MERGE_PENDING";

// Ladder order, least→most advanced. Index is used by isAutoAdvanceAllowed.
export const NORTHSTAR_STATES: readonly NorthstarState[] = [
  "NO_WORKSPACE",
  "WORKSPACE_READY",
  "TASK_DESCRIBED",
  "PLAN_DRAFTED",
  "PLAN_VALIDATED",
  "PREVIEW_READY",
  "AWAITING_APPLY_APPROVAL",
  "APPLIED",
  "PACKAGE_READY",
  "COMMITTED",
  "PUSHED",
  "DRAFT_PR_OPEN",
  "EVIDENCE_FROZEN",
  "DISPOSITION_PENDING",
  "CLOSED_RETAINED",
  "HUMAN_MERGE_PENDING",
];

// How a state may be entered:
//   read-only   — derivable from read-only observation/wait (the panel may
//                 auto-reflect it; it is an observation, not an action).
//   human-gated — entered ONLY when an explicit human-gated event was OBSERVED
//                 (apply / package-commit / package-push / package-pr / close).
//                 The model never performs these; it only reflects that they
//                 already happened.
//   human-only  — never reached by any automation; only ever surfaced as
//                 "pending a human decision" (PR merge / approve / mark-ready).
export type StateClass = "read-only" | "human-gated" | "human-only";

export function stateClass(state: NorthstarState): StateClass {
  switch (state) {
    case "APPLIED":
    case "COMMITTED":
    case "PUSHED":
    case "DRAFT_PR_OPEN":
    case "CLOSED_RETAINED":
      return "human-gated";
    case "HUMAN_MERGE_PENDING":
      return "human-only";
    default:
      // NO_WORKSPACE, WORKSPACE_READY, TASK_DESCRIBED, PLAN_DRAFTED,
      // PLAN_VALIDATED, PREVIEW_READY, AWAITING_APPLY_APPROVAL, PACKAGE_READY
      // (package-plan is read-only validation), EVIDENCE_FROZEN,
      // DISPOSITION_PENDING.
      return "read-only";
  }
}

// Read-only observation signals. The extension fills the ones it can observe
// without fs/spawn (workspace folder; the helper's read-only audit). Signals
// the current phase does not yet observe default false — the model then rests
// at the most-advanced OBSERVED milestone, which is honest, never optimistic.
export interface NorthstarSignals {
  workspaceReady: boolean;
  taskDescribed: boolean;
  planDrafted: boolean;
  planValidated: boolean;
  // Chain observations (read-only, e.g. from parseAuditWorkspace):
  previewReady: boolean;
  // Preview reviewed; standing AT the human apply gate (not yet applied).
  awaitingApplyApproval: boolean;
  // Human-gated events, each OBSERVED read-only (never inferred/automated):
  appliedObserved: boolean;
  packageReadyObserved: boolean; // package-plan validated (read-only) post-apply
  committedObserved: boolean;
  pushedObserved: boolean;
  draftPrObserved: boolean;
  // Read-only post-PR observations:
  evidenceFrozen: boolean;
  dispositionPending: boolean;
  // Terminal disposition, only when explicitly resolved by a human:
  dispositionResolved: "closed-retained" | "human-merge-pending" | null;
}

export function emptyNorthstarSignals(): NorthstarSignals {
  return {
    workspaceReady: false,
    taskDescribed: false,
    planDrafted: false,
    planValidated: false,
    previewReady: false,
    awaitingApplyApproval: false,
    appliedObserved: false,
    packageReadyObserved: false,
    committedObserved: false,
    pushedObserved: false,
    draftPrObserved: false,
    evidenceFrozen: false,
    dispositionPending: false,
    dispositionResolved: null,
  };
}

// Derive the single most-advanced Northstar state for which there is read-only
// evidence. Scans from the top of the ladder down: each human-gated state is
// returned ONLY when its own OBSERVED signal is set, so no read-only signal
// (preview/awaiting) can ever advance the model into APPLIED-or-beyond. This is
// the structural form of "never auto-advance past AWAITING_APPLY_APPROVAL".
export function deriveNorthstarState(s: NorthstarSignals): NorthstarState {
  if (!s.workspaceReady) {
    return "NO_WORKSPACE";
  }
  if (s.dispositionResolved === "human-merge-pending") {
    return "HUMAN_MERGE_PENDING";
  }
  if (s.dispositionResolved === "closed-retained") {
    return "CLOSED_RETAINED";
  }
  if (s.dispositionPending) {
    return "DISPOSITION_PENDING";
  }
  if (s.evidenceFrozen) {
    return "EVIDENCE_FROZEN";
  }
  if (s.draftPrObserved) {
    return "DRAFT_PR_OPEN";
  }
  if (s.pushedObserved) {
    return "PUSHED";
  }
  if (s.committedObserved) {
    return "COMMITTED";
  }
  if (s.packageReadyObserved) {
    return "PACKAGE_READY";
  }
  if (s.appliedObserved) {
    return "APPLIED";
  }
  if (s.awaitingApplyApproval) {
    return "AWAITING_APPLY_APPROVAL";
  }
  if (s.previewReady) {
    return "PREVIEW_READY";
  }
  if (s.planValidated) {
    return "PLAN_VALIDATED";
  }
  if (s.planDrafted) {
    return "PLAN_DRAFTED";
  }
  if (s.taskDescribed) {
    return "TASK_DESCRIBED";
  }
  return "WORKSPACE_READY";
}

// The next safe step the model may recommend. None is a thing the model runs.
// "Approve*/Package*/OpenDraftPr/HumanMerge" are GUIDANCE pointing the operator
// at a human gate; they are flagged automatable:false (see stepMeta).
export type NorthstarStep =
  | "DetectWorkspace"
  | "DescribeTask"
  | "DraftPlan"
  | "ValidatePlan"
  | "ReviewPreview"
  | "ApproveApplyAtGate"
  | "PackagePlan"
  | "PackageCommit"
  | "PackagePush"
  | "OpenDraftPr"
  | "FreezeEvidence"
  | "ReviewDisposition"
  | "HumanMergeDecision"
  | "Done"
  | "StopInvestigate";

export const NORTHSTAR_STEPS: readonly NorthstarStep[] = [
  "DetectWorkspace",
  "DescribeTask",
  "DraftPlan",
  "ValidatePlan",
  "ReviewPreview",
  "ApproveApplyAtGate",
  "PackagePlan",
  "PackageCommit",
  "PackagePush",
  "OpenDraftPr",
  "FreezeEvidence",
  "ReviewDisposition",
  "HumanMergeDecision",
  "Done",
  "StopInvestigate",
];

export interface StepMeta {
  // read-only      — the panel may perform this itself (detection/display).
  // human-gated    — requires an explicit, per-action human gesture; the panel
  //                  may guide/print but never auto-run it.
  // human-only     — never reachable from any panel automation.
  // terminal       — Done / Stop; nothing to do.
  kind: "read-only" | "human-gated" | "human-only" | "terminal";
  // True ONLY for read-only steps the panel may auto-perform. Every gated /
  // human-only step is false. No apply/package/push/pr/merge is ever true.
  automatable: boolean;
  // True when the step's authoritative gate is a REAL terminal (human-typed).
  requiresRealTty: boolean;
  label: string;
}

export function stepMeta(step: NorthstarStep): StepMeta {
  switch (step) {
    case "DetectWorkspace":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Detect the workspace (read-only)" };
    case "DescribeTask":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Describe the task (capture only)" };
    case "DraftPlan":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Draft a safe plan (read-only)" };
    case "ValidatePlan":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Validate the plan (offline schema)" };
    case "ReviewPreview":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Review the preview / diff (read-only)" };
    case "ApproveApplyAtGate":
      return { kind: "human-gated", automatable: false, requiresRealTty: true, label: "Approve the apply at a REAL terminal (human-typed)" };
    case "PackagePlan":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Package-plan (read-only validation)" };
    case "PackageCommit":
      return { kind: "human-gated", automatable: false, requiresRealTty: false, label: "Package-commit (human-gated, explicit confirm)" };
    case "PackagePush":
      return { kind: "human-gated", automatable: false, requiresRealTty: false, label: "Package-push the disposable branch (human-gated, non-force)" };
    case "OpenDraftPr":
      return { kind: "human-gated", automatable: false, requiresRealTty: false, label: "Open exactly one DRAFT PR (human-gated)" };
    case "FreezeEvidence":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Freeze the evidence timeline (read-only)" };
    case "ReviewDisposition":
      return { kind: "read-only", automatable: true, requiresRealTty: false, label: "Review disposition: close / retain / human merge" };
    case "HumanMergeDecision":
      return { kind: "human-only", automatable: false, requiresRealTty: false, label: "Human-only: merge / approve / mark-ready decision" };
    case "Done":
      return { kind: "terminal", automatable: false, requiresRealTty: false, label: "Done — disposition resolved (closed / retained)" };
    case "StopInvestigate":
      return { kind: "terminal", automatable: false, requiresRealTty: false, label: "STOP — unrecognized state; investigate before acting" };
    default:
      return { kind: "terminal", automatable: false, requiresRealTty: false, label: "STOP — unrecognized state; investigate" };
  }
}

// The single recommended next safe step for a state.
export function northstarNextStep(state: NorthstarState): NorthstarStep {
  switch (state) {
    case "NO_WORKSPACE":
      return "DetectWorkspace";
    case "WORKSPACE_READY":
      return "DescribeTask";
    case "TASK_DESCRIBED":
      return "DraftPlan";
    case "PLAN_DRAFTED":
      return "ValidatePlan";
    case "PLAN_VALIDATED":
      return "ReviewPreview";
    case "PREVIEW_READY":
      return "ReviewPreview";
    case "AWAITING_APPLY_APPROVAL":
      return "ApproveApplyAtGate";
    case "APPLIED":
      return "PackagePlan";
    case "PACKAGE_READY":
      return "PackageCommit";
    case "COMMITTED":
      return "PackagePush";
    case "PUSHED":
      return "OpenDraftPr";
    case "DRAFT_PR_OPEN":
      return "FreezeEvidence";
    case "EVIDENCE_FROZEN":
      return "ReviewDisposition";
    case "DISPOSITION_PENDING":
      return "ReviewDisposition";
    case "CLOSED_RETAINED":
      return "Done";
    case "HUMAN_MERGE_PENDING":
      return "HumanMergeDecision";
    default:
      return "StopInvestigate";
  }
}

// Executor verbs that no automatable step may name. A step that is automatable
// AND matches one of these is a safety violation (it would mean the panel auto-
// runs a write/outward action).
const EXECUTOR_VERB = /(apply|commit|push|opendraftpr|merge|approve|ready|force|run|execute)/i;

// Invariant guard: a recommended step must be in the known set, and a step that
// is automatable must NOT be a write/outward executor, and any human-gated /
// human-only step must NOT be automatable. Throws on violation. Exercised for
// every state by the unit tests.
export function assertNorthstarSafe(step: NorthstarStep): NorthstarStep {
  if (!NORTHSTAR_STEPS.includes(step)) {
    throw new Error("unsafe northstar step (not in NORTHSTAR_STEPS): " + String(step));
  }
  const meta = stepMeta(step);
  if ((meta.kind === "human-gated" || meta.kind === "human-only") && meta.automatable) {
    throw new Error("unsafe northstar step (gated/human-only marked automatable): " + String(step));
  }
  if (meta.automatable && EXECUTOR_VERB.test(step)) {
    throw new Error("unsafe northstar step (automatable executor verb): " + String(step));
  }
  return step;
}

// Whether the model may AUTO-advance from one state to another without a human
// gate. Allowed ONLY between read-only states. Any transition whose TARGET is a
// human-gated or human-only state is never auto-allowed — it requires the
// target's OBSERVED human-gated event. Backward/equal transitions are not
// auto-advances. Used by the tests to prove no automated path crosses the apply
// gate or reaches merge.
export function isAutoAdvanceAllowed(from: NorthstarState, to: NorthstarState): boolean {
  const fi = NORTHSTAR_STATES.indexOf(from);
  const ti = NORTHSTAR_STATES.indexOf(to);
  if (fi < 0 || ti < 0) {
    return false;
  }
  if (ti <= fi) {
    return false;
  }
  // The target must be read-only AND every state strictly between from and to
  // must also be read-only (no skipping a human gate).
  for (let i = fi + 1; i <= ti; i++) {
    if (stateClass(NORTHSTAR_STATES[i]) !== "read-only") {
      return false;
    }
  }
  return true;
}

// Convenience read-only view assembled for the render layer.
export interface NorthstarView {
  state: NorthstarState;
  stateClass: StateClass;
  stepLabel: string;
  stepKind: StepMeta["kind"];
  automatable: boolean;
  requiresRealTty: boolean;
}

export function buildNorthstarView(signals: NorthstarSignals): NorthstarView {
  const state = deriveNorthstarState(signals);
  const step = assertNorthstarSafe(northstarNextStep(state));
  const meta = stepMeta(step);
  return {
    state,
    stateClass: stateClass(state),
    stepLabel: meta.label,
    stepKind: meta.kind,
    automatable: meta.automatable,
    requiresRealTty: meta.requiresRealTty,
  };
}
