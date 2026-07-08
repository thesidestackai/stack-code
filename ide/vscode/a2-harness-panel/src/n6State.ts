// Northstar Phase N6 — execution boundary STATE MACHINE (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md §9/§22/§23.
//
// N6 is the first execution-capable boundary. Every N6 state is either a safe
// per-rung progress state or a global blocked state. N6_FORBIDDEN_TARGETS is a
// strict superset of N5_FORBIDDEN_TARGETS, which is itself a strict superset of
// N4_FORBIDDEN_TARGETS. assertN6Safe throws on any forbidden or unknown state.
// PURE: no fs, no spawn, no network.

import { N5_FORBIDDEN_TARGETS } from "./n5State";

// Per-rung execution states. Operator decisions baked in:
//   D4=B: FAILED clears the rung token (fresh sub-token required to retry).
export type N6RungExecState =
  | "AWAITING_TOKEN"  // no sub-token supplied yet; show token-entry button
  | "TOKEN_ACTIVE"    // sub-token validated; rung may or may not be ready
  | "RUNNING"         // helper dispatched; in-flight
  | "DONE"            // helper exited 0; output captured
  | "FAILED";         // helper exited non-zero; token cleared (D4=B)

export const N6_RUNG_EXEC_STATES: readonly N6RungExecState[] = [
  "AWAITING_TOKEN",
  "TOKEN_ACTIVE",
  "RUNNING",
  "DONE",
  "FAILED",
];

// Full N6 state name union (per-rung × 5 + 3 global blocked).
// Inherits all N5 states unchanged (passed through without mutation).
export type N6State =
  // package-plan rung
  | "N6_AWAITING_PACKAGE_PLAN_TOKEN"
  | "N6_PACKAGE_PLAN_TOKEN_ACTIVE"
  | "N6_PACKAGE_PLAN_RUNNING"
  | "N6_PACKAGE_PLAN_DONE"
  | "N6_PACKAGE_PLAN_FAILED"
  // package-commit rung
  | "N6_AWAITING_PACKAGE_COMMIT_TOKEN"
  | "N6_PACKAGE_COMMIT_TOKEN_ACTIVE"
  | "N6_PACKAGE_COMMIT_RUNNING"
  | "N6_PACKAGE_COMMIT_DONE"
  | "N6_PACKAGE_COMMIT_FAILED"
  // package-push rung
  | "N6_AWAITING_PACKAGE_PUSH_TOKEN"
  | "N6_PACKAGE_PUSH_TOKEN_ACTIVE"
  | "N6_PACKAGE_PUSH_RUNNING"
  | "N6_PACKAGE_PUSH_DONE"
  | "N6_PACKAGE_PUSH_FAILED"
  // package-pr rung (draft-only)
  | "N6_AWAITING_DRAFT_PR_TOKEN"
  | "N6_DRAFT_PR_TOKEN_ACTIVE"
  | "N6_DRAFT_PR_RUNNING"
  | "N6_DRAFT_PR_DONE"
  | "N6_DRAFT_PR_FAILED"
  // Global N6 blocked states (fail closed)
  | "N6_BLOCKED_TOKEN_MISMATCH"
  | "N6_BLOCKED_EXECUTION_REFUSED"
  | "N6_BLOCKED_MISSING_PRECONDITION";

export const N6_STATES: readonly N6State[] = [
  "N6_AWAITING_PACKAGE_PLAN_TOKEN",
  "N6_PACKAGE_PLAN_TOKEN_ACTIVE",
  "N6_PACKAGE_PLAN_RUNNING",
  "N6_PACKAGE_PLAN_DONE",
  "N6_PACKAGE_PLAN_FAILED",
  "N6_AWAITING_PACKAGE_COMMIT_TOKEN",
  "N6_PACKAGE_COMMIT_TOKEN_ACTIVE",
  "N6_PACKAGE_COMMIT_RUNNING",
  "N6_PACKAGE_COMMIT_DONE",
  "N6_PACKAGE_COMMIT_FAILED",
  "N6_AWAITING_PACKAGE_PUSH_TOKEN",
  "N6_PACKAGE_PUSH_TOKEN_ACTIVE",
  "N6_PACKAGE_PUSH_RUNNING",
  "N6_PACKAGE_PUSH_DONE",
  "N6_PACKAGE_PUSH_FAILED",
  "N6_AWAITING_DRAFT_PR_TOKEN",
  "N6_DRAFT_PR_TOKEN_ACTIVE",
  "N6_DRAFT_PR_RUNNING",
  "N6_DRAFT_PR_DONE",
  "N6_DRAFT_PR_FAILED",
  "N6_BLOCKED_TOKEN_MISMATCH",
  "N6_BLOCKED_EXECUTION_REFUSED",
  "N6_BLOCKED_MISSING_PRECONDITION",
];

// Exact sub-token strings accepted at runtime per rung (Level 2 tokens).
// These are the strings the operator must type in the VS Code input box.
// Not implied by the Level 1 implementation token.
export const N6_SUB_TOKEN_PLAN   = "APPROVED: N6 Package Plan Only";
export const N6_SUB_TOKEN_COMMIT = "APPROVED: N6 Package Commit Only";
export const N6_SUB_TOKEN_PUSH   = "APPROVED: N6 Package Push Only";
export const N6_SUB_TOKEN_PR     = "APPROVED: N6 Draft PR Only";

// N6_FORBIDDEN_TARGETS is a strict superset of N5_FORBIDDEN_TARGETS.
// N6 adds apply-gate-and-beyond, merge, model/broker/vault, force-push,
// pr-mark-ready, and auto-approval states that must never be reachable.
export const N6_FORBIDDEN_TARGETS: readonly string[] = [
  ...N5_FORBIDDEN_TARGETS,
  // Apply-gate and beyond (scope doc §22):
  "APPLY_EXECUTING",
  "APPLY_APPROVED",
  "APPLY_DONE",
  // PR lifecycle beyond draft:
  "PR_APPROVED",
  "PR_MERGED",
  "MERGED",
  // Runtime/model/broker/Vault:
  "MODEL_CALL_EXECUTING",
  "BROKER_CALL_EXECUTING",
  "VAULT_READ_EXECUTING",
  // Hidden-execution and force patterns:
  "AUTO_APPROVED",
  "HIDDEN_APPLY",
  "PUSH_FORCE",
  "PR_MARK_READY",
];

// assertN6Safe: invariant guard. Throws on any forbidden or unknown state.
// N6_FORBIDDEN_TARGETS ⊃ N5_FORBIDDEN_TARGETS ⊃ N4_FORBIDDEN_TARGETS.
export function assertN6Safe(state: string): N6State {
  if ((N6_FORBIDDEN_TARGETS as readonly string[]).includes(state)) {
    throw new Error(
      "unsafe N6 state (routes to execution or apply gate or beyond): " + state,
    );
  }
  if (!(N6_STATES as readonly string[]).includes(state)) {
    throw new Error("unknown N6 state: " + state);
  }
  return state as N6State;
}

// Derive the canonical N6State name for a per-rung exec state.
// Used by tests and assertN6Safe to validate states before rendering.
export function deriveN6RungStateName(
  rung: "plan" | "commit" | "push" | "pr",
  exec: N6RungExecState,
): N6State {
  const prefixMap: Record<typeof rung, string> = {
    plan:   "N6_PACKAGE_PLAN",
    commit: "N6_PACKAGE_COMMIT",
    push:   "N6_PACKAGE_PUSH",
    pr:     "N6_DRAFT_PR",
  };
  const prefix = prefixMap[rung];
  const suffixMap: Record<N6RungExecState, string> = {
    AWAITING_TOKEN: rung === "pr" ? "AWAITING_DRAFT_PR_TOKEN" : `AWAITING_PACKAGE_${rung.toUpperCase()}_TOKEN`,
    TOKEN_ACTIVE:   `${prefix}_TOKEN_ACTIVE`,
    RUNNING:        `${prefix}_RUNNING`,
    DONE:           `${prefix}_DONE`,
    FAILED:         `${prefix}_FAILED`,
  };
  if (exec === "AWAITING_TOKEN") {
    const awaitMap: Record<typeof rung, N6State> = {
      plan:   "N6_AWAITING_PACKAGE_PLAN_TOKEN",
      commit: "N6_AWAITING_PACKAGE_COMMIT_TOKEN",
      push:   "N6_AWAITING_PACKAGE_PUSH_TOKEN",
      pr:     "N6_AWAITING_DRAFT_PR_TOKEN",
    };
    return awaitMap[rung];
  }
  const name = suffixMap[exec];
  return assertN6Safe(name);
}

export function n6RungStateNote(rung: "plan" | "commit" | "push" | "pr", exec: N6RungExecState): string {
  switch (exec) {
    case "AWAITING_TOKEN": {
      const labels: Record<typeof rung, string> = {
        plan:   "Package Plan",
        commit: "Package Commit",
        push:   "Package Push",
        pr:     "Draft PR",
      };
      return `Supply ${labels[rung]} sub-token to enable this rung.`;
    }
    case "TOKEN_ACTIVE":
      return "Sub-token active. Click run button if rung is READY.";
    case "RUNNING":
      return "Helper executing — wait for output.";
    case "DONE":
      return "Rung complete (exit 0). Review output before proceeding.";
    case "FAILED":
      return "FAILED (non-zero exit). Token cleared (D4-B). Supply a new sub-token to retry.";
    default:
      return "";
  }
}
