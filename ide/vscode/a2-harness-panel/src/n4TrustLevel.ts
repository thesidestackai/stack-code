// Northstar Phase N4 — DATA TRUST LEVEL classifier (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n4-preview-diff-evidence-scope.md §10.
//
// Every datum the read-only viewer shows carries an explicit trust level, and
// N4 FAILS CLOSED on ambiguity. PURE: no fs, no spawn, no network.
//
//   VERIFIED  came from committed source, validated N3 state, or explicit
//             read-only helper output.
//   INFERRED  derived from safe local metadata but not independently validated.
//   MISSING   not present yet.
//   BLOCKED   unsafe or ambiguous (a STOP risk, an executable-looking step,
//             ambiguous artifacts). Fail closed.

export type TrustLevel = "VERIFIED" | "INFERRED" | "MISSING" | "BLOCKED";

export const TRUST_LEVELS: readonly TrustLevel[] = ["VERIFIED", "INFERRED", "MISSING", "BLOCKED"];

export interface TrustInputs {
  // The datum is present at all.
  present: boolean;
  // It is independently validated (committed/validated-N3/explicit helper output).
  verified: boolean;
  // A blocking condition holds (unsafe/ambiguous). Wins over everything.
  blocked: boolean;
}

// Classify a datum's trust level. BLOCKED wins (fail closed); then MISSING;
// then VERIFIED vs INFERRED. Never optimistic: a datum is VERIFIED only when it
// is both present and independently verified.
export function classifyTrust(input: TrustInputs): TrustLevel {
  if (input.blocked) {
    return "BLOCKED";
  }
  if (!input.present) {
    return "MISSING";
  }
  return input.verified ? "VERIFIED" : "INFERRED";
}

// A datum is reviewable (safe to render as content) only when VERIFIED or
// INFERRED. MISSING and BLOCKED are not reviewable.
export function isReviewable(t: TrustLevel): boolean {
  return t === "VERIFIED" || t === "INFERRED";
}

export function isBlocked(t: TrustLevel): boolean {
  return t === "BLOCKED";
}

// INFERRED data must never be presented as VERIFIED. This predicate states the
// rule the render layer must honor (exercised by the tests).
export function mustLabelInferred(t: TrustLevel): boolean {
  return t === "INFERRED";
}
