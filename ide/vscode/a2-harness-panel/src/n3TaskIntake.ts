// Northstar Phase N3 — TASK INTAKE state model + pure reducer.
//
// Source of truth: docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md
// §7 (task intake model) + §8 (task draft data contract).
//
// PURE: no fs, no spawn, no network, no clock read. The reducer is a total
// function (state, event) -> state. All state is local/session-only. No event
// advances draft_status past "validated"/"blocked": there is NO apply / package
// / preview / PR event — those are future phases and structurally absent here.

import {
  RiskCategory,
  classifyRisk,
  riskDisposition,
  isStopRisk,
  defaultForbiddenPaths,
} from "./n3RiskClassifier";
import {
  PlanDraft,
  PlanDraftValidation,
  validatePlanDraft,
  DEFAULT_NOT_EXECUTABLE_REASON,
} from "./n3PlanDraft";

export type DraftStatus =
  | "empty"
  | "described"
  | "targets-declared"
  | "risk-classified"
  | "drafted"
  | "validated"
  | "blocked";

export interface TaskDraft {
  task_id: string;
  task_summary: string;
  operator_intent: string;
  workspace_root: string | null;
  declared_target_paths: string[];
  forbidden_paths: string[];
  risk_level: RiskCategory | null;
  requires_real_tty: boolean;
  requires_human_approval: boolean;
  draft_status: DraftStatus;
  created_at: string | null;
  updated_at: string | null;
  // The non-executing plan draft + its validation, populated at DraftPlan/Validate.
  plan_draft: PlanDraft | null;
  plan_validation: PlanDraftValidation | null;
}

// Timestamps are SUPPLIED to the reducer (pure code never reads the clock).
export interface Stamp {
  now: string;
}

export function emptyTaskDraft(taskId: string): TaskDraft {
  return {
    task_id: taskId,
    task_summary: "",
    operator_intent: "",
    workspace_root: null,
    declared_target_paths: [],
    forbidden_paths: defaultForbiddenPaths(),
    risk_level: null,
    requires_real_tty: false,
    requires_human_approval: false,
    draft_status: "empty",
    created_at: null,
    updated_at: null,
    plan_draft: null,
    plan_validation: null,
  };
}

export type TaskIntakeEvent =
  | { type: "DescribeTask"; summary: string; intent: string; workspaceRoot: string | null; stamp: Stamp }
  | { type: "DeclareTarget"; path: string; stamp: Stamp }
  | { type: "DeclareForbidden"; path: string; stamp: Stamp }
  | { type: "ClassifyRisk"; stamp: Stamp }
  | { type: "DraftPlan"; stamp: Stamp }
  | { type: "ValidateDraft"; stamp: Stamp }
  | { type: "Reset"; taskId: string; stamp: Stamp };

function uniqPush(list: ReadonlyArray<string>, value: string): string[] {
  const v = value.trim();
  if (v.length === 0 || list.includes(v)) {
    return [...list];
  }
  return [...list, v];
}

// requires_human_approval is true for anything beyond a pure read-only/docs
// outcome; requires_real_tty is true when a future apply would be human-typed
// (i.e. anything that is not purely read-only).
function derivedApproval(cat: RiskCategory): { tty: boolean; approval: boolean } {
  if (cat === "READ_ONLY") {
    return { tty: false, approval: false };
  }
  if (cat === "DOCS_ONLY") {
    return { tty: false, approval: true };
  }
  return { tty: true, approval: true };
}

function buildPlanDraft(state: TaskDraft): PlanDraft {
  return {
    draft_id: `${state.task_id}-draft`,
    task_id: state.task_id,
    candidate_steps: [
      `Review intent: ${state.operator_intent || "(none)"}`,
      `Confirm declared target paths (${state.declared_target_paths.length}) are exact and in-scope`,
      "Confirm forbidden paths are respected (deny-list wins)",
      "Hand off to a future, separately-approved preview/apply lane (N4+) — not in N3",
    ],
    declared_paths: [...state.declared_target_paths],
    forbidden_paths: [...state.forbidden_paths],
    expected_outputs: [`a reviewed, non-executing plan draft for: ${state.task_summary || "(untitled task)"}`],
    risk_notes: `risk_level=${state.risk_level ?? "UNKNOWN"} disposition=${riskDisposition(state.risk_level ?? "UNKNOWN")}`,
    required_evidence: [
      "declared-vs-forbidden boundary check",
      "risk classification",
      "operator confirmation before any future preview/apply lane",
    ],
    stop_gates: [
      "STOP before preview/apply/package/PR",
      "STOP if risk is RUNTIME_CONFIG/SECRETS_OR_VAULT/DESTRUCTIVE_OR_FORCE/UNKNOWN",
    ],
    not_executable_reason: DEFAULT_NOT_EXECUTABLE_REASON,
    risk_level: state.risk_level ?? "UNKNOWN",
  };
}

// The pure reducer. Total: an event that does not apply to the current state
// returns the state unchanged (except updated_at when a stamp is given). No
// event reaches preview/apply/package/PR.
export function reduceTaskIntake(state: TaskDraft, event: TaskIntakeEvent): TaskDraft {
  switch (event.type) {
    case "Reset":
      return { ...emptyTaskDraft(event.taskId), created_at: event.stamp.now, updated_at: event.stamp.now, draft_status: "empty" };

    case "DescribeTask": {
      const summary = event.summary.trim();
      const intent = event.intent.trim();
      if (summary.length === 0 && intent.length === 0) {
        return state;
      }
      return {
        ...state,
        task_summary: summary,
        operator_intent: intent,
        workspace_root: event.workspaceRoot,
        draft_status: state.draft_status === "empty" ? "described" : state.draft_status,
        created_at: state.created_at ?? event.stamp.now,
        updated_at: event.stamp.now,
      };
    }

    case "DeclareTarget": {
      if (state.draft_status === "empty") {
        return state; // describe the task first
      }
      const declared = uniqPush(state.declared_target_paths, event.path);
      return {
        ...state,
        declared_target_paths: declared,
        draft_status: "targets-declared",
        updated_at: event.stamp.now,
      };
    }

    case "DeclareForbidden": {
      const forbidden = uniqPush(state.forbidden_paths, event.path);
      return { ...state, forbidden_paths: forbidden, updated_at: event.stamp.now };
    }

    case "ClassifyRisk": {
      if (state.draft_status === "empty") {
        return state;
      }
      const cat = classifyRisk(state.declared_target_paths, state.operator_intent);
      const d = derivedApproval(cat);
      return {
        ...state,
        risk_level: cat,
        requires_real_tty: d.tty,
        requires_human_approval: d.approval,
        draft_status: "risk-classified",
        updated_at: event.stamp.now,
      };
    }

    case "DraftPlan": {
      if (state.draft_status === "empty" || state.draft_status === "described" || state.draft_status === "targets-declared") {
        // Need a risk classification first; classify implicitly to stay total.
        const classified = reduceTaskIntake(state, { type: "ClassifyRisk", stamp: event.stamp });
        if (classified.draft_status !== "risk-classified") {
          return state;
        }
        return reduceTaskIntake(classified, event);
      }
      const plan = buildPlanDraft(state);
      return {
        ...state,
        plan_draft: plan,
        draft_status: "drafted",
        updated_at: event.stamp.now,
      };
    }

    case "ValidateDraft": {
      if (state.plan_draft === null) {
        return state;
      }
      const validation = validatePlanDraft(state.plan_draft);
      const blocked = validation.status === "PLAN_DRAFT_BLOCKED" || isStopRisk(state.risk_level ?? "UNKNOWN");
      return {
        ...state,
        plan_validation: validation,
        draft_status: blocked ? "blocked" : "validated",
        updated_at: event.stamp.now,
      };
    }

    default:
      return state;
  }
}

// Pre-format the task intake state as read-only display lines.
export function renderTaskIntakeLines(state: TaskDraft): string[] {
  return [
    `task: ${state.task_summary || "(none)"}`,
    `intent: ${state.operator_intent || "(none)"}`,
    `declared paths: ${state.declared_target_paths.length > 0 ? state.declared_target_paths.join(", ") : "(none)"}`,
    `forbidden paths: ${state.forbidden_paths.join(", ")}`,
    `risk: ${state.risk_level ?? "(unclassified)"}`,
    `requires real-TTY: ${state.requires_real_tty ? "yes" : "no"}`,
    `requires human approval: ${state.requires_human_approval ? "yes" : "no"}`,
    `draft status: ${state.draft_status}`,
  ];
}
