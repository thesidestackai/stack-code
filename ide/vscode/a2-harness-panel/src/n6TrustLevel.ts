// Northstar Phase N6 — execution trust level extension (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md §10.
//
// N6 adds two runtime-only evidence categories on top of N5's trust levels.
// EXECUTION_OBSERVED and EXECUTION_FAILED are observed AFTER a rung runs;
// they are never fabricated or silently promoted to VERIFIED. PURE: no fs/spawn.

export type N6TrustLevel =
  | "VERIFIED"             // committed source, validated N3/N4, explicit read-only output
  | "INFERRED"             // safe local metadata, not independently validated
  | "MISSING"              // not present
  | "BLOCKED"              // unsafe or ambiguous; wins over all other levels
  | "EXECUTION_REQUIRED"   // cannot be proven from read-only data (unchanged from N5)
  | "EXECUTION_OBSERVED"   // helper exited 0; output captured; NOT auto-verified
  | "EXECUTION_FAILED";    // helper exited non-zero; output shown; rung locked

// Ordering for fail-closed classification:
//   BLOCKED > MISSING > EXECUTION_REQUIRED > EXECUTION_FAILED > EXECUTION_OBSERVED > INFERRED > VERIFIED
const TRUST_ORDER: N6TrustLevel[] = [
  "BLOCKED",
  "MISSING",
  "EXECUTION_REQUIRED",
  "EXECUTION_FAILED",
  "EXECUTION_OBSERVED",
  "INFERRED",
  "VERIFIED",
];

function trustRank(t: N6TrustLevel): number {
  return TRUST_ORDER.indexOf(t);
}

// classifyN6Trust: pick the most-conservative (lowest-rank) level from a set.
// Never promotes EXECUTION_OBSERVED to VERIFIED. BLOCKED always wins.
export function classifyN6Trust(levels: N6TrustLevel[]): N6TrustLevel {
  if (levels.length === 0) {
    return "MISSING";
  }
  let best = levels[0];
  for (const l of levels) {
    if (trustRank(l) < trustRank(best)) {
      best = l;
    }
  }
  return best;
}

// isN6Reviewable: EXECUTION_OBSERVED is reviewable (operator must judge the
// output), but it is NOT the same as VERIFIED. Never returns true for VERIFIED
// when input is EXECUTION_OBSERVED.
export function isN6Reviewable(trust: N6TrustLevel): boolean {
  return trust === "EXECUTION_OBSERVED";
}

// isN6Verified: only VERIFIED counts as verified. EXECUTION_OBSERVED does not.
export function isN6Verified(trust: N6TrustLevel): boolean {
  return trust === "VERIFIED";
}
