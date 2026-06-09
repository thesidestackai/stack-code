// Safe mutation policy model (pure) — Tier 3 Foundation v0.
//
// Classifies a proposed Tier 3 action (a command, or a file write) under the
// safe-executor model of docs/a2-tier3-disposable-worktree-mutation-scope.md §9.
// This module is PURE and EXECUTES NOTHING — there is no executor in v0. It
// composes the Foundation v0 denied-command registry (denials win) with a
// conservative Tier-3 command allowlist and the declared mutation scope, so the
// panel can display, and a future approved executor can enforce, the policy.
//
// Order of decision (denials always win):
//   1. denied-command registry FIRST  -> denied
//   2. Tier-3 allowlist SECOND         -> non-allowlisted command => denied
//   3. for writes: declared exact-path scope must accept the path => else denied

import { evaluate, EvaluateResult } from "./deniedCommands";
import { classifyWrite, MutationScopeInput, ScopeResult } from "./mutationScope";

// The conservative Tier-3 command allowlist: read-only/print helper subcommands
// and the explicitly-approved local validation commands (run inside the
// disposable worktree only). Matched case-insensitively against the command text.
// Stored as source strings so sensitive tokens stay in string literals.
export const TIER3_ALLOWED_COMMAND_PATTERNS: readonly string[] = [
  // read-only / print helper subcommands
  "^\\s*(validate-input|audit-workspace|find-artifacts|verify-final|help)\\b",
  // approved local validation for the panel package (disposable worktree only)
  "^\\s*npm\\s+install\\s+--ignore-scripts\\b",
  "^\\s*npm\\s+run\\s+lint\\b",
  "^\\s*npm\\s+run\\s+compile\\b",
  "^\\s*npm\\s+test\\b",
];

export function tier3Allowlist(command: string): boolean {
  const text = (command || "").toString();
  for (const src of TIER3_ALLOWED_COMMAND_PATTERNS) {
    let re: RegExp;
    try {
      re = new RegExp(src, "i");
    } catch {
      continue;
    }
    if (re.test(text)) {
      return true;
    }
  }
  return false;
}

export interface PolicyDecision {
  decision: "allowed" | "denied";
  reason: string;
}

// Classify a proposed COMMAND under Tier 3: denied-registry first, then the
// Tier-3 allowlist. Classification only — nothing runs.
export function evaluateTier3Command(command: string): PolicyDecision {
  const res: EvaluateResult = evaluate(command, tier3Allowlist);
  return { decision: res.decision, reason: res.reason };
}

// Classify a proposed WRITE under Tier 3: the declared exact-path scope must
// accept the path (inside the disposable worktree, in the declared set, not
// under the control checkout). A write is never an allowlisted "command"; it is
// gated solely by the declared scope. Classification only — nothing is written.
export function evaluateTier3Write(candidate: string, scope: MutationScopeInput): PolicyDecision {
  const res: ScopeResult = classifyWrite(candidate, scope);
  return {
    decision: res.decision === "accepted" ? "allowed" : "denied",
    reason: res.reason,
  };
}

// Convenience: a one-line statement of the policy invariant, for the panel.
export function policyInvariant(): string {
  return "Denials win over the Tier-3 allowlist; writes are limited to the declared exact-path set inside the disposable worktree; nothing executes in v0.";
}
