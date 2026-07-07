// Northstar Phase N5 — DATA TRUST LEVEL extension (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n5-gated-execution-boundary-scope.md §12.
//
// Extends the N4 trust levels with EXECUTION_REQUIRED: a fact that cannot be
// proven from read-only data alone and requires a separate approved execution
// lane. N5 FAILS CLOSED on all ambiguity. PURE: no fs, no spawn, no network.
//
//   VERIFIED            same as N4: from committed source / validated N3/N4 state.
//   INFERRED            same as N4: derived from safe local metadata, not independently validated.
//   MISSING             same as N4: not present.
//   BLOCKED             same as N4: unsafe / ambiguous. Wins over everything.
//   EXECUTION_REQUIRED  cannot be proven without a separate approved execution lane
//                       (e.g. real push state, apply result). NOT treated as ready.

import { TrustLevel } from "./n4TrustLevel";

export type N5TrustLevel = TrustLevel | "EXECUTION_REQUIRED";

export const N5_TRUST_LEVELS: readonly N5TrustLevel[] = [
  "VERIFIED",
  "INFERRED",
  "MISSING",
  "BLOCKED",
  "EXECUTION_REQUIRED",
];

export interface N5TrustInputs {
  // The datum is present at all.
  present: boolean;
  // It is independently validated (committed/validated-N3/explicit helper output).
  verified: boolean;
  // A blocking condition holds (unsafe/ambiguous). Wins over everything.
  blocked: boolean;
  // Proving this fact requires a separate live operation (e.g. remote push state).
  requiresExecution: boolean;
}

// Classify an N5 datum's trust level.
// Priority (fail closed): BLOCKED > MISSING > EXECUTION_REQUIRED > VERIFIED/INFERRED.
export function classifyN5Trust(input: N5TrustInputs): N5TrustLevel {
  if (input.blocked) {
    return "BLOCKED";
  }
  if (!input.present) {
    return "MISSING";
  }
  if (input.requiresExecution) {
    return "EXECUTION_REQUIRED";
  }
  return input.verified ? "VERIFIED" : "INFERRED";
}

// An N5 datum is renderable (safe to show content) only when VERIFIED or
// INFERRED. MISSING, BLOCKED, and EXECUTION_REQUIRED all suppress content.
export function isN5Reviewable(t: N5TrustLevel): boolean {
  return t === "VERIFIED" || t === "INFERRED";
}

export function isN5Blocked(t: N5TrustLevel): boolean {
  return t === "BLOCKED";
}

// EXECUTION_REQUIRED facts are never treated as ready. The render layer must
// label them honestly and never display them as VERIFIED or ready to run.
export function requiresExecutionLane(t: N5TrustLevel): boolean {
  return t === "EXECUTION_REQUIRED";
}
