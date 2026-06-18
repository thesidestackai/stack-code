// Northstar Phase N3 — NON-EXECUTING PLAN DRAFT model + offline validator (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md
// §9 (plan draft model) + §12 (validation/linting).
//
// A plan draft is a REVIEW ARTIFACT, not a runnable plan. It carries no plan
// YAML body, no command string, and no claw invocation. `not_executable_reason`
// is REQUIRED and non-empty; validation fails closed if it is missing. PURE: no
// fs, no spawn, no network.

import {
  RiskCategory,
  isStopRisk,
  validateDeclaredPath,
  isForbiddenFamily,
  ALWAYS_FORBIDDEN_MARKERS,
} from "./n3RiskClassifier";

export interface PlanDraft {
  draft_id: string;
  task_id: string;
  candidate_steps: string[];
  declared_paths: string[];
  forbidden_paths: string[];
  expected_outputs: string[];
  risk_notes: string;
  required_evidence: string[];
  stop_gates: string[];
  // REQUIRED, non-empty: why this draft cannot be run by claw or the orchestrator.
  not_executable_reason: string;
  risk_level: RiskCategory;
}

// The canonical reason a draft is inert, used when the extension builds a draft.
export const DEFAULT_NOT_EXECUTABLE_REASON =
  "plan draft is a review artifact: it carries no runnable plan schema, no claw plan body, and no executable command";

export type PlanDraftValidationStatus = "PLAN_DRAFT_VALIDATED" | "PLAN_DRAFT_BLOCKED";

export interface PlanDraftValidation {
  status: PlanDraftValidationStatus;
  reasons: string[];
}

// Substrings that would make a "step" look executable. A candidate step is
// descriptive text only; if any of these appear it is rejected (the draft must
// never smuggle a runnable command). Checked by .includes (no regex that could
// resemble a spawn call).
const EXECUTABLE_STEP_MARKERS: readonly string[] = [
  "claw ",
  "claw\t",
  "plan run",
  "plan apply",
  "plan approve",
  "package-plan",
  "package-commit",
  "package-push",
  "package-pr",
  "git push",
  "git commit",
  "&&",
  "||",
  "$(",
  "`",
  ";",
  "| sh",
  "| bash",
  "#!/",
];

function stepLooksExecutable(step: string): boolean {
  const s = step.toLowerCase();
  return EXECUTABLE_STEP_MARKERS.some((m) => s.includes(m.toLowerCase()));
}

// Offline, fail-closed validator. Returns VALIDATED only when every gate holds;
// otherwise BLOCKED with the accumulated reasons. It never mutates the draft and
// never makes it runnable.
export function validatePlanDraft(draft: PlanDraft): PlanDraftValidation {
  const reasons: string[] = [];

  if (typeof draft.not_executable_reason !== "string" || draft.not_executable_reason.trim().length === 0) {
    reasons.push("not_executable_reason is required and must be non-empty");
  }

  for (const d of draft.declared_paths) {
    const chk = validateDeclaredPath(d);
    if (!chk.ok) {
      reasons.push(`declared path "${d}": ${chk.reason}`);
    }
    if (isForbiddenFamily(d)) {
      reasons.push(`declared path "${d}" is in an always-forbidden family`);
    }
    if (draft.forbidden_paths.map((f) => f.trim()).includes(d.trim())) {
      reasons.push(`declared path "${d}" intersects forbidden_paths (deny-list wins)`);
    }
  }

  const lowerForbidden = draft.forbidden_paths.map((f) => f.trim().toLowerCase());
  for (const m of ALWAYS_FORBIDDEN_MARKERS) {
    if (!lowerForbidden.includes(m)) {
      reasons.push(`forbidden_paths missing always-denied family: ${m}`);
    }
  }

  if (isStopRisk(draft.risk_level)) {
    reasons.push(`risk_level ${draft.risk_level} is a STOP — cannot proceed to draft review`);
  }

  for (const step of draft.candidate_steps) {
    if (stepLooksExecutable(step)) {
      reasons.push(`candidate step looks executable (must be descriptive text only): "${step}"`);
    }
  }

  return reasons.length === 0
    ? { status: "PLAN_DRAFT_VALIDATED", reasons: [] }
    : { status: "PLAN_DRAFT_BLOCKED", reasons };
}

// Structural guard: a plan draft is NEVER runnable. This is always true by
// construction (no command/plan-body field exists), and this predicate proves
// it for the tests — it inspects the candidate steps and the reason field.
export function planDraftIsNonExecutable(draft: PlanDraft): boolean {
  if (typeof draft.not_executable_reason !== "string" || draft.not_executable_reason.trim().length === 0) {
    return false;
  }
  return !draft.candidate_steps.some(stepLooksExecutable);
}

// Pre-format the plan draft as read-only display lines.
export function renderPlanDraftLines(draft: PlanDraft): string[] {
  return [
    `draft_id: ${draft.draft_id}`,
    `task_id: ${draft.task_id}`,
    `risk_level: ${draft.risk_level}`,
    `declared_paths: ${draft.declared_paths.length > 0 ? draft.declared_paths.join(", ") : "(none)"}`,
    `forbidden_paths: ${draft.forbidden_paths.join(", ")}`,
    `candidate_steps: ${draft.candidate_steps.length}`,
    `expected_outputs: ${draft.expected_outputs.length}`,
    `not_executable_reason: ${draft.not_executable_reason}`,
  ];
}
