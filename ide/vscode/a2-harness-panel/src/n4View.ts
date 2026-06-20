// Northstar Phase N4 — PREVIEW / DIFF / EVIDENCE view models (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md §7/§8/§9.
//
// Read-only DISPLAY models built from the validated N3 plan draft. N4 renders
// only data that already exists; it runs no preview/apply, writes no target and
// no .claw, and FAILS CLOSED — when blocked, every facet is BLOCKED with no
// rendered content (nothing ambiguous shown as verified). PURE: no fs/spawn.

import { TaskDraft } from "./n3TaskIntake";
import { planDraftIsNonExecutable } from "./n3PlanDraft";
import { isForbiddenFamily } from "./n3RiskClassifier";
import { classifyTrust, isReviewable, TrustLevel } from "./n4TrustLevel";
import {
  N4Inputs,
  N4State,
  deriveN4State,
  assertN4Safe,
  isBlockedState,
  n4NextStepLabel,
} from "./n4State";

// Derive the read-only N4 inputs from the local N3 task draft. Every field is an
// observation of already-present state; N4 generates nothing.
export function buildN4Inputs(draft: TaskDraft): N4Inputs {
  const plan = draft.plan_draft;
  const hasPlanDraft = plan !== null;
  const planNonExecutable = plan ? planDraftIsNonExecutable(plan) : true;
  const hasForbiddenFamilyTarget = draft.declared_target_paths.some((p) => isForbiddenFamily(p));
  const hasPreviewData = !!plan && (plan.candidate_steps.length > 0 || plan.expected_outputs.length > 0);
  const hasDiffData = draft.declared_target_paths.length > 0;
  const hasEvidenceData =
    draft.plan_validation !== null && !!plan && plan.required_evidence.length > 0;
  return {
    hasPlanDraft,
    riskLevel: draft.risk_level,
    hasForbiddenFamilyTarget,
    planNonExecutable,
    validationStatus: draft.plan_validation ? draft.plan_validation.status : null,
    hasPreviewData,
    hasDiffData,
    hasEvidenceData,
  };
}

export interface N4Facet {
  trust: TrustLevel;
  // Rendered ONLY when the facet is reviewable (VERIFIED/INFERRED). Empty when
  // MISSING or BLOCKED — N4 never renders ambiguous data as if it were content.
  lines: string[];
}

export interface N4View {
  state: N4State;
  stepLabel: string;
  isBlocked: boolean;
  preview: N4Facet;
  diff: N4Facet;
  evidence: N4Facet;
}

function facet(trust: TrustLevel, contentLines: string[]): N4Facet {
  return { trust, lines: isReviewable(trust) ? contentLines : [] };
}

export function buildN4View(draft: TaskDraft): N4View {
  const inputs = buildN4Inputs(draft);
  const state = assertN4Safe(deriveN4State(inputs));
  const blocked = isBlockedState(state);
  const verified = inputs.validationStatus === "PLAN_DRAFT_VALIDATED";
  const plan = draft.plan_draft;

  const previewTrust = classifyTrust({ present: inputs.hasPreviewData, verified, blocked });
  const diffTrust = classifyTrust({ present: inputs.hasDiffData, verified, blocked });
  const evidenceTrust = classifyTrust({ present: inputs.hasEvidenceData, verified, blocked });

  const previewContent: string[] = plan
    ? [
        `task: ${draft.task_summary || "(untitled)"}`,
        ...plan.candidate_steps.map((s) => `step: ${s}`),
        ...plan.expected_outputs.map((o) => `expected: ${o}`),
      ]
    : [];

  const diffContent: string[] = plan
    ? [
        `declared paths: ${draft.declared_target_paths.length > 0 ? draft.declared_target_paths.join(", ") : "(none)"}`,
        `forbidden paths: ${draft.forbidden_paths.join(", ")}`,
        `expected outputs: ${plan.expected_outputs.length}`,
        `not_executable_reason: ${plan.not_executable_reason}`,
      ]
    : [];

  const evidenceContent: string[] = plan
    ? [
        ...plan.required_evidence.map((e) => `evidence: ${e}`),
        `validation: ${draft.plan_validation ? draft.plan_validation.status : "(none)"}`,
        `risk: ${draft.risk_level ?? "(unclassified)"}`,
      ]
    : [];

  return {
    state,
    stepLabel: n4NextStepLabel(state),
    isBlocked: blocked,
    preview: facet(previewTrust, previewContent),
    diff: facet(diffTrust, diffContent),
    evidence: facet(evidenceTrust, evidenceContent),
  };
}

// Pre-format the N4 view as read-only summary lines (state + per-facet trust).
export function renderN4SummaryLines(view: N4View): string[] {
  return [
    `state: ${view.state}`,
    `preview: ${view.preview.trust}`,
    `diff: ${view.diff.trust}`,
    `evidence: ${view.evidence.trust}`,
  ];
}
