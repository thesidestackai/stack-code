// Northstar Phase N5 — READINESS BOARD view model (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n5-gated-execution-boundary-scope.md §8/§9.
//
// Builds the read-only N5 readiness board from the existing local N3/N4 session
// state (TaskDraft). N5 renders only data that already exists; it runs nothing,
// writes nothing, calls no model/broker/runtime, and fails closed. PURE: no fs,
// no spawn, no network.

import { TaskDraft } from "./n3TaskIntake";
import { planDraftIsNonExecutable } from "./n3PlanDraft";
import { isForbiddenFamily } from "./n3RiskClassifier";
import { buildN4View } from "./n4View";
import { N4State } from "./n4State";
import { deriveLadderReadiness, RungReadinessResult } from "./n5ReadinessModel";
import {
  N5State,
  N5Inputs,
  assertN5Safe,
  deriveN5State,
  isN5BlockedState,
  n5NextStepLabel,
} from "./n5State";

export interface N5RungView {
  rung: string;
  purpose: string;
  // "READY" | "NOT_READY" | "BLOCKED" | "EXECUTION_REQUIRED"
  readiness: string;
  // Each precondition formatted with trust level label.
  preconditionLines: string[];
  evidencePresent: boolean;
  operatorConfirmationRequired: boolean;
  note: string;
}

export interface N5View {
  state: N5State;
  stepLabel: string;
  isBlocked: boolean;
  // N4 summary (read-only context for the N5 board).
  n4State: N4State;
  n4StepLabel: string;
  // Task context from the N3 draft.
  taskSummary: string;
  riskLevel: string;
  // Per-rung readiness board (read-only; never run).
  ladder: N5RungView[];
}

function rungToView(r: RungReadinessResult): N5RungView {
  return {
    rung: r.rung,
    purpose: r.purpose,
    readiness: r.readiness,
    preconditionLines: r.preconditions.map(
      (p) => `[${p.trust}] ${p.label}: ${p.met ? "met" : "not met"}`,
    ),
    evidencePresent: r.evidencePresent,
    operatorConfirmationRequired: r.operatorConfirmationRequired,
    note: r.note,
  };
}

export function buildN5View(draft: TaskDraft): N5View {
  const n4v = buildN4View(draft);
  const n4State = n4v.state;

  const plan = draft.plan_draft;
  const planNonExecutable = plan ? planDraftIsNonExecutable(plan) : true;
  const targetSafe = !draft.declared_target_paths.some((p) => isForbiddenFamily(p));
  const planValidated = draft.plan_validation?.status === "PLAN_DRAFT_VALIDATED";
  // hasEvidenceData: true only when evidence facet rendered content (VERIFIED/INFERRED).
  const hasEvidenceData = n4v.evidence.lines.length > 0;

  const ladder = deriveLadderReadiness(n4State, {
    planNonExecutable,
    targetSafe,
    evidencePresent: hasEvidenceData,
    planValidated,
  });

  const n5Input: N5Inputs = {
    n4State,
    packagePlanReadiness: ladder.packagePlan.readiness,
    hasEvidenceData,
  };
  const state = assertN5Safe(deriveN5State(n5Input));
  const blocked = isN5BlockedState(state);

  return {
    state,
    stepLabel: n5NextStepLabel(state),
    isBlocked: blocked,
    n4State,
    n4StepLabel: n4v.stepLabel,
    taskSummary: draft.task_summary ?? "(no task described yet)",
    riskLevel: draft.risk_level ?? "(unclassified)",
    ladder: [
      rungToView(ladder.packagePlan),
      rungToView(ladder.packageCommit),
      rungToView(ladder.packagePush),
      rungToView(ladder.packagePr),
    ],
  };
}
