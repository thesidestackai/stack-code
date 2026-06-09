// Tier 3 Mutation Executor v0 — dry-run plan model (pure) — dry-run only.
//
// Per docs/a2-tier3-mutation-executor-design-scope.md §18, the executor's FIRST
// lane is plan / dry-run ONLY. This module is PURE: given an operator-approved
// lane (objective + worktree plan + declared exact-path set + proposed
// writes/commands), it validates the lane against the merged Tier 3 Foundation
// v0 models and computes exactly what an external executor WOULD do — while
// creating NO worktree and writing NOTHING. It performs no IO: no fs, no process
// spawn, no network, no watcher, no timer. It enables no mutation.
//
// The actual executor is external and operator-invoked; the panel only PRINTS
// the dry-run command this module describes and renders the dry-run result. The
// panel never creates a worktree, never writes a file, and never spawns the
// executor.

import { Tier3Facts, computeTier3Readiness } from "./tier3Readiness";
import { WorktreePlan, validateWorktreePlan } from "./disposableWorktreePlan";
import { MutationScopeInput, validateDeclaredSet } from "./mutationScope";
import { evaluateTier3Command, evaluateTier3Write } from "./safeMutationPolicy";

export interface ApprovedLane {
  objective: string | null;
  worktreePlan: WorktreePlan | null;
  // The exact declared touched-file paths (absolute, inside the worktree).
  declaredPaths: string[];
  // Candidate write paths the lane would apply (dry-run classifies; never writes).
  proposedWrites: string[];
  // Candidate commands the lane would run (dry-run classifies; never runs).
  proposedCommands: string[];
  // Whether the operator has explicitly approved this exact lane.
  operatorApproved: boolean;
}

export type DryRunStepKind = "write" | "command";
export type DryRunDecision = "would-accept" | "would-reject";

export interface DryRunStep {
  kind: DryRunStepKind;
  target: string;
  decision: DryRunDecision;
  reason: string;
}

export interface DryRunResult {
  // Would the lane be allowed to proceed at all (readiness + plan + scope)?
  ready: boolean;
  readinessOverall: string; // "ready" | "not-ready"
  planValid: boolean;
  planProblems: string[];
  scopeProblems: string[];
  // Per-step classification (writes + commands). Pure classification only.
  steps: DryRunStep[];
  // ALWAYS false in v0 — dry-run never creates a worktree and never writes.
  wouldCreateWorktree: boolean;
  wouldWriteFiles: boolean;
  // A one-line honest summary.
  summary: string;
  // The exact (hypothetical) external executor command the operator would run.
  // Printed only — the panel never spawns it, and the dry-run performs nothing.
  printedCommand: string;
}

function isNonEmpty(s: unknown): s is string {
  return typeof s === "string" && s.trim().length > 0;
}

// The external, operator-invoked dry-run command the panel PRINTS. It is a
// describe-only string: there is no executor binary in v0, the panel never
// spawns it, and the dry-run creates/writes nothing.
export function dryRunCommand(): string {
  return "a2-mutation-executor --dry-run --approved-lane <approved-lane.json>  # external; operator-run; NO worktree creation, NO writes";
}

// Compute the dry-run result for an approved lane. Pure: classifies what the
// executor WOULD do; creates nothing and writes nothing.
export function computeDryRun(lane: ApprovedLane, facts?: Tier3Facts): DryRunResult {
  const planValidation = validateWorktreePlan(lane.worktreePlan);
  const declaredScopePresent = Array.isArray(lane.declaredPaths) && lane.declaredPaths.length > 0;

  const readiness = computeTier3Readiness({
    facts,
    planValid: planValidation.valid,
    declaredScopePresent,
    deniedRegistryLoaded: true,
  });

  // The scope the executor would enforce (worktree root from the plan).
  const scope: MutationScopeInput = {
    worktreeRoot: lane.worktreePlan && isNonEmpty(lane.worktreePlan.worktreePath)
      ? lane.worktreePlan.worktreePath
      : "",
    declaredPaths: lane.declaredPaths || [],
  };
  const scopeProblems = isNonEmpty(scope.worktreeRoot)
    ? validateDeclaredSet(scope)
    : ["no worktree plan: cannot validate declared scope"];

  const steps: DryRunStep[] = [];
  for (const w of lane.proposedWrites || []) {
    const d = evaluateTier3Write(w, scope);
    steps.push({
      kind: "write",
      target: w,
      decision: d.decision === "allowed" ? "would-accept" : "would-reject",
      reason: d.reason,
    });
  }
  for (const c of lane.proposedCommands || []) {
    const d = evaluateTier3Command(c);
    steps.push({
      kind: "command",
      target: c,
      decision: d.decision === "allowed" ? "would-accept" : "would-reject",
      reason: d.reason,
    });
  }

  const ready =
    readiness.overall === "ready" &&
    planValidation.valid &&
    scopeProblems.length === 0 &&
    lane.operatorApproved === true;

  const rejects = steps.filter((s) => s.decision === "would-reject").length;
  const summary = ready
    ? `dry-run: lane would be allowed to proceed; ${steps.length - rejects}/${steps.length} steps would-accept, ${rejects} would-reject (NO creation, NO writes performed)`
    : `dry-run: lane is NOT ready to proceed (readiness ${readiness.overall}; plan ${planValidation.valid ? "valid" : "invalid"}; ${scopeProblems.length} scope problem(s); operator approved: ${lane.operatorApproved ? "yes" : "no"}) — NO creation, NO writes performed`;

  return {
    ready,
    readinessOverall: readiness.overall,
    planValid: planValidation.valid,
    planProblems: planValidation.problems,
    scopeProblems,
    steps,
    wouldCreateWorktree: false,
    wouldWriteFiles: false,
    summary,
    printedCommand: dryRunCommand(),
  };
}

// Render-ready lines for the dry-run result (display only).
export function summarizeDryRun(result: DryRunResult): string[] {
  const lines = [
    "ready: " + (result.ready ? "yes" : "no"),
    "readiness: " + result.readinessOverall,
    "plan valid: " + (result.planValid ? "yes" : "no"),
    "scope problems: " + (result.scopeProblems.length === 0 ? "none" : String(result.scopeProblems.length)),
    "would create worktree: " + (result.wouldCreateWorktree ? "yes" : "no"),
    "would write files: " + (result.wouldWriteFiles ? "yes" : "no"),
  ];
  for (const s of result.steps) {
    lines.push(`${s.kind}: ${s.target} -> ${s.decision} (${s.reason})`);
  }
  return lines;
}
