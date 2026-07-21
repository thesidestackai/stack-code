// N7-A — pure live-vs-frozen comparison, 18-state precedence model, and
// next-permitted-action guidance.
//
// Source of truth: docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md
// (State Machine §, STOP Gates §, Approval Gate Contract §).
//
// PURE: no fs, no spawn, no network, no clock reads (`nowIso` is always
// supplied by the caller), no hidden global state, no mutation of inputs.
// This module never calls GitHub, never writes `.claw/n7`, never invokes
// Claw/helper/broker, and never authorizes or executes a workflow-rung
// action — every next-permitted-action value is a read-only guidance label.

import { FrozenReviewSnapshot, PrLiveSnapshot, validateRfc3339UtcTimestamp } from "./n7Schemas";

// ---------------------------------------------------------------------------
// CI requirement policy (explicit input — never inferred from checks.length)
// ---------------------------------------------------------------------------
//
// Whether CI is required for this PR/repository is a policy fact, not
// something derivable from the shape of a single live snapshot's checks
// array. An empty `checks` array is evidence only that zero checks were
// OBSERVED at capture time — it does not say whether that is because no CI
// is configured, because CI has not started reporting yet, or because the
// caller simply hasn't looked. The caller (eventually a future N7-B GitHub
// reader / branch-protection reader, out of scope here) must supply this
// policy explicitly.
export type CiRequirementPolicy =
  // A verified policy fact: these named checks are required. An EMPTY
  // requiredCheckNames list is deliberately NOT treated as "no checks
  // required" — see deriveCi below, which fails closed (PENDING) for it
  // rather than silently downgrading to NOT_REQUIRED.
  | { kind: "REQUIRED"; requiredCheckNames: readonly string[] }
  // A verified policy fact: no check is required for this PR/repository.
  // This is the ONLY input shape that may unlock the FROZEN_MATCH primary
  // state; it must come from an explicit, verified source — never from
  // observing `checks.length === 0`.
  | { kind: "NOT_REQUIRED" }
  // The caller does not know the CI requirement policy. Must fail closed.
  | { kind: "UNKNOWN" };

export const CI_REQUIREMENT_POLICY_KINDS: readonly CiRequirementPolicy["kind"][] = [
  "REQUIRED",
  "NOT_REQUIRED",
  "UNKNOWN",
];

// ---------------------------------------------------------------------------
// Live-vs-frozen comparison
// ---------------------------------------------------------------------------

export type HeadComparison = "MATCH" | "DRIFT" | "UNKNOWN";
export type BaseComparison = "MATCH" | "DRIFT" | "UNKNOWN";
// "NOT_REQUIRED" is reachable ONLY when the caller supplied an explicit
// verified `{kind:"NOT_REQUIRED"}` policy AND no observed check blocks it
// (see deriveCi). It is never inferred from an empty checks array alone.
export type CiComparisonState = "SUCCESS" | "PENDING" | "FAILED" | "UNKNOWN" | "NOT_REQUIRED";
export type ReviewComparisonState = "CLEAN" | "BLOCKED" | "UNKNOWN";
export type MergeabilityComparisonState = "MERGEABLE" | "CONFLICT" | "UNKNOWN";
export type FreshnessState = "FRESH" | "STALE" | "NONE";

export interface LiveFrozenComparison {
  headComparison: HeadComparison;
  baseComparison: BaseComparison;
  ci: CiComparisonState;
  // Diagnostic detail for the `ci` value (e.g. which required checks are
  // missing, or that the policy itself is unknown). Null when `ci` needs no
  // further explanation (SUCCESS, FAILED, or NOT_REQUIRED).
  ciReason: string | null;
  review: ReviewComparisonState;
  mergeability: MergeabilityComparisonState;
  freshness: FreshnessState;
  reasons: readonly string[];
}

export interface ComparisonInput {
  live: PrLiveSnapshot | null;
  frozen: FrozenReviewSnapshot | null;
  // Explicit, verified CI requirement policy. Never derived from `live`.
  ciRequirementPolicy: CiRequirementPolicy;
  // RFC3339 UTC timestamp supplied by the caller. This function never reads
  // a clock; "now" is always an explicit input.
  nowIso: string;
  // Freshness policy in whole milliseconds, supplied by the caller.
  freshnessThresholdMs: number;
}

// parseIsoMs is a pure string->number function over an explicitly supplied
// input, using the same strict RFC3339 UTC `Z` contract as schema
// validation (no lenient `Date.parse` fallback). It is not a clock read: no
// argument-less Date.now()/new Date().
function parseIsoMs(iso: string): number | null {
  const r = validateRfc3339UtcTimestamp(iso);
  return r.ok ? r.epochMs : null;
}

function deriveHeadComparison(live: PrLiveSnapshot | null, frozen: FrozenReviewSnapshot | null): HeadComparison {
  if (live === null || live.head_sha.length === 0) return "UNKNOWN";
  if (frozen === null || frozen.approved_head_sha.length === 0) return "UNKNOWN";
  return live.head_sha === frozen.approved_head_sha ? "MATCH" : "DRIFT";
}

function deriveBaseComparison(live: PrLiveSnapshot | null, frozen: FrozenReviewSnapshot | null): BaseComparison {
  if (live === null || live.base_sha.length === 0) return "UNKNOWN";
  if (frozen === null || frozen.base_sha.length === 0) return "UNKNOWN";
  return live.base_sha === frozen.base_sha ? "MATCH" : "DRIFT";
}

function isFailureLikeConclusion(c: { conclusion: string | null }): boolean {
  return (
    c.conclusion === "FAILURE" ||
    c.conclusion === "CANCELLED" ||
    c.conclusion === "TIMED_OUT" ||
    c.conclusion === "ACTION_REQUIRED"
  );
}

// deriveCi NEVER infers the CI requirement policy from `live.checks`. The
// policy is always the caller-supplied `policy` parameter; `live.checks` is
// used only to evaluate outcomes (pending/failed/succeeded) against that
// already-known policy.
function deriveCi(
  live: PrLiveSnapshot | null,
  policy: CiRequirementPolicy,
): { ci: CiComparisonState; reason: string | null } {
  if (live === null) return { ci: "UNKNOWN", reason: null };

  if (policy.kind === "UNKNOWN") {
    // empty_checks_alone_do_not_prove_no_ci_policy: an unknown policy fails
    // closed regardless of what `live.checks` contains, including empty.
    return { ci: "UNKNOWN", reason: "ci_requirement_unknown" };
  }

  if (!live.pagination.checks_complete) {
    // Incomplete pagination must never be treated as "no CI configured" —
    // partial data cannot become clean, so this forces the PENDING branch
    // regardless of policy.
    return { ci: "PENDING", reason: "checks pagination is incomplete" };
  }

  // Exact-head correlation: a check for an old head can never clear the
  // current head (green_ci_for_old_head_does_not_clear_current_head).
  const correlated = live.checks.filter((c) => c.head_sha === live.head_sha);

  if (policy.kind === "NOT_REQUIRED") {
    // A verified NOT_REQUIRED policy still honors any checks that DID run:
    // a failure is still a failure, and an in-flight check is still
    // pending. Only the *absence* of a required-success proof is waived.
    if (correlated.some(isFailureLikeConclusion)) {
      return { ci: "FAILED", reason: "a check for the current head failed under a NOT_REQUIRED CI policy" };
    }
    if (correlated.some((c) => c.status !== "COMPLETED")) {
      return { ci: "PENDING", reason: "a check for the current head is still in progress" };
    }
    return { ci: "NOT_REQUIRED", reason: null };
  }

  // policy.kind === "REQUIRED"
  if (policy.requiredCheckNames.length === 0) {
    // An empty required-check list must not silently become NOT_REQUIRED.
    return { ci: "PENDING", reason: "CI policy is REQUIRED but names no required checks" };
  }
  const missing = policy.requiredCheckNames.filter((name) => !correlated.some((c) => c.name === name));
  if (missing.length > 0) {
    return { ci: "PENDING", reason: `required checks missing for current head: ${missing.join(", ")}` };
  }
  const requiredCorrelated = correlated.filter((c) => policy.requiredCheckNames.includes(c.name));
  if (requiredCorrelated.some(isFailureLikeConclusion)) {
    return { ci: "FAILED", reason: "a required check for the current head failed" };
  }
  if (requiredCorrelated.some((c) => c.status !== "COMPLETED")) {
    return { ci: "PENDING", reason: "a required check for the current head is still in progress" };
  }
  return { ci: "SUCCESS", reason: null };
}

function deriveReview(live: PrLiveSnapshot | null): ReviewComparisonState {
  if (live === null) return "UNKNOWN";
  // Missing review pagination is not reported as "no blockers".
  if (!live.pagination.review_threads_complete) return "UNKNOWN";
  const r = live.reviews;
  if (r.requested_changes.length > 0) return "BLOCKED";
  if (r.unresolved_review_threads.count > 0) return "BLOCKED";
  if (!r.unresolved_review_threads.complete) return "BLOCKED";
  if (r.blocking_automated_findings.length > 0) return "BLOCKED";
  if (r.review_decision !== "APPROVED") return "BLOCKED";
  return "CLEAN";
}

function deriveMergeability(live: PrLiveSnapshot | null): MergeabilityComparisonState {
  if (live === null) return "UNKNOWN";
  if (live.mergeability === "MERGEABLE") return "MERGEABLE";
  if (live.mergeability === "CONFLICTING") return "CONFLICT";
  return "UNKNOWN";
}

function deriveFreshness(live: PrLiveSnapshot | null, nowIso: string, freshnessThresholdMs: number): FreshnessState {
  if (live === null) return "NONE";
  const capturedMs = parseIsoMs(live.captured_at);
  const nowMs = parseIsoMs(nowIso);
  if (capturedMs === null || nowMs === null) return "NONE";
  const ageMs = nowMs - capturedMs;
  return ageMs > freshnessThresholdMs ? "STALE" : "FRESH";
}

export function deriveLiveFrozenComparison(input: ComparisonInput): LiveFrozenComparison {
  const headComparison = deriveHeadComparison(input.live, input.frozen);
  const baseComparison = deriveBaseComparison(input.live, input.frozen);
  const { ci, reason: ciReason } = deriveCi(input.live, input.ciRequirementPolicy);
  const review = deriveReview(input.live);
  const mergeability = deriveMergeability(input.live);
  const freshness = deriveFreshness(input.live, input.nowIso, input.freshnessThresholdMs);

  const reasons: string[] = [];
  if (headComparison === "DRIFT") {
    reasons.push(
      `current head ${input.live?.head_sha ?? ""} differs from frozen reviewed head ${input.frozen?.approved_head_sha ?? ""}`,
    );
  }
  if (baseComparison === "DRIFT") {
    reasons.push(`current base ${input.live?.base_sha ?? ""} differs from frozen base ${input.frozen?.base_sha ?? ""}`);
  }
  if (ciReason !== null) {
    reasons.push(ciReason);
  }
  if (review === "BLOCKED") {
    reasons.push("review has requested changes, unresolved threads, blocking findings, or is not approved");
  }
  if (mergeability === "CONFLICT") {
    reasons.push("mergeability reports conflict");
  }
  if (freshness === "STALE") {
    reasons.push("live refresh is older than the freshness policy");
  }

  return {
    headComparison,
    baseComparison,
    ci,
    ciReason,
    review,
    mergeability,
    freshness,
    reasons,
  };
}

// ---------------------------------------------------------------------------
// 18-state primary model
// ---------------------------------------------------------------------------

export type N7PrimaryState =
  | "NO_PR"
  | "LIVE_UNCHECKED"
  | "LIVE_FETCH_FAILED"
  | "DRAFT_CLEAN"
  | "DRAFT_BLOCKED"
  | "READY_CLEAN"
  | "READY_BLOCKED"
  | "HEAD_DRIFT"
  | "BASE_DRIFT"
  | "CI_PENDING"
  | "CI_FAILED"
  | "REVIEW_BLOCKED"
  | "MERGE_CONFLICT"
  | "FROZEN_MATCH"
  | "FROZEN_STALE"
  | "MERGED"
  | "CLOSED_UNMERGED"
  | "UNKNOWN";

// Exact 18 states, no nineteenth state.
export const N7_PRIMARY_STATES: readonly N7PrimaryState[] = [
  "NO_PR",
  "LIVE_UNCHECKED",
  "LIVE_FETCH_FAILED",
  "DRAFT_CLEAN",
  "DRAFT_BLOCKED",
  "READY_CLEAN",
  "READY_BLOCKED",
  "HEAD_DRIFT",
  "BASE_DRIFT",
  "CI_PENDING",
  "CI_FAILED",
  "REVIEW_BLOCKED",
  "MERGE_CONFLICT",
  "FROZEN_MATCH",
  "FROZEN_STALE",
  "MERGED",
  "CLOSED_UNMERGED",
  "UNKNOWN",
];

export type N7Severity = "OK" | "WARN" | "STOP" | "TERMINAL" | "UNKNOWN";

const SEVERITY_BY_STATE: Readonly<Record<N7PrimaryState, N7Severity>> = {
  NO_PR: "WARN",
  LIVE_UNCHECKED: "WARN",
  LIVE_FETCH_FAILED: "STOP",
  DRAFT_CLEAN: "OK",
  DRAFT_BLOCKED: "STOP",
  READY_CLEAN: "OK",
  READY_BLOCKED: "STOP",
  HEAD_DRIFT: "STOP",
  BASE_DRIFT: "STOP",
  CI_PENDING: "WARN",
  CI_FAILED: "STOP",
  REVIEW_BLOCKED: "STOP",
  MERGE_CONFLICT: "STOP",
  FROZEN_MATCH: "OK",
  FROZEN_STALE: "WARN",
  MERGED: "TERMINAL",
  CLOSED_UNMERGED: "TERMINAL",
  UNKNOWN: "UNKNOWN",
};

export function severityForPrimaryState(state: N7PrimaryState): N7Severity {
  return SEVERITY_BY_STATE[state];
}

export type EvidenceChainIntegrity = "OK" | "FAILED" | "NOT_CHECKED";

export interface N7DerivationInput {
  // Whether a PR number/URL is known at all (may be OPERATOR_ASSERTION
  // before any live refresh — see Provenance enum in n7Schemas.ts).
  prIdentityKnown: boolean;
  live: PrLiveSnapshot | null;
  // Explicit signal that the most recent live-refresh attempt failed. This
  // module never performs the fetch itself; the caller (a future read-only
  // GitHub reader, out of N7-A's scope) supplies the outcome.
  liveFetchFailed: boolean;
  frozen: FrozenReviewSnapshot | null;
  // Explicit, verified CI requirement policy — see CiRequirementPolicy.
  // Never derived from `live.checks.length` inside this module.
  ciRequirementPolicy: CiRequirementPolicy;
  // Whether the local hash-linked evidence chain (owned by a future N7-C
  // storage module, out of scope here) has been verified. N7-A treats only
  // an explicit FAILED as a STOP; NOT_CHECKED (no chain exists yet, which is
  // always true for N7-A callers) never blocks a clean state on its own.
  evidenceChainIntegrity: EvidenceChainIntegrity;
  nowIso: string;
  freshnessThresholdMs: number;
}

export interface N7DerivedStateResult {
  primaryState: N7PrimaryState;
  severity: N7Severity;
  comparison: LiveFrozenComparison;
  blockingReason: string | null;
}

function result(
  primaryState: N7PrimaryState,
  comparison: LiveFrozenComparison,
  blockingReason: string | null,
): N7DerivedStateResult {
  return { primaryState, severity: severityForPrimaryState(primaryState), comparison, blockingReason };
}

// Derive the N7 primary state by strict precedence. Precedence is expressed
// as an ordered sequence of early returns so the ranking is visible in code
// and does not depend on object/array iteration order. At minimum this
// upholds:
//   - integrity failure outranks clean states;
//   - head drift outranks green CI;
//   - review blockers outrank clean readiness;
//   - merge conflict outranks clean readiness;
//   - CI failure/pending can never become clean;
//   - partial or missing data can never become clean;
//   - terminal PR states remain terminal (checked first, before anything
//     else, including integrity failure).
export function deriveN7PrimaryState(input: N7DerivationInput): N7DerivedStateResult {
  const comparison = deriveLiveFrozenComparison({
    live: input.live,
    frozen: input.frozen,
    ciRequirementPolicy: input.ciRequirementPolicy,
    nowIso: input.nowIso,
    freshnessThresholdMs: input.freshnessThresholdMs,
  });

  // 1. MERGED — terminal, outranks everything, including integrity failure.
  if (input.live?.state === "MERGED") {
    return result("MERGED", comparison, null);
  }
  // 2. CLOSED_UNMERGED — terminal.
  if (input.live?.state === "CLOSED") {
    return result("CLOSED_UNMERGED", comparison, null);
  }
  // 3. Evidence chain verification failure renders primary UNKNOWN (STOP).
  if (input.evidenceChainIntegrity === "FAILED") {
    return result("UNKNOWN", comparison, "evidence chain verification failed");
  }
  // 4. NO_PR — no PR identity known at all.
  if (!input.prIdentityKnown) {
    return result("NO_PR", comparison, null);
  }
  // 5. LIVE_FETCH_FAILED.
  if (input.liveFetchFailed) {
    return result("LIVE_FETCH_FAILED", comparison, "the last live refresh attempt failed");
  }
  // 6. LIVE_UNCHECKED or FROZEN_STALE (never-refreshed vs. stale-refresh).
  if (input.live === null) {
    return result("LIVE_UNCHECKED", comparison, null);
  }
  if (input.frozen !== null && comparison.freshness === "STALE") {
    return result("FROZEN_STALE", comparison, "live PR state is stale; refresh before relying on it");
  }
  // 7. HEAD_DRIFT outranks green CI and clean readiness.
  if (comparison.headComparison === "DRIFT") {
    return result("HEAD_DRIFT", comparison, "current head differs from frozen reviewed head");
  }
  // 8. BASE_DRIFT.
  if (comparison.baseComparison === "DRIFT") {
    return result("BASE_DRIFT", comparison, "current base differs from frozen base");
  }
  // 9. MERGE_CONFLICT.
  if (comparison.mergeability === "CONFLICT") {
    return result("MERGE_CONFLICT", comparison, "mergeability reports conflict");
  }
  // 10. REVIEW_BLOCKED outranks green CI. Missing review pagination also
  //     blocks (never reported as "no blockers").
  if (comparison.review === "BLOCKED" || comparison.review === "UNKNOWN") {
    return result("REVIEW_BLOCKED", comparison, "review is blocked or review state is incomplete");
  }
  // 11. CI_FAILED.
  if (comparison.ci === "FAILED") {
    return result("CI_FAILED", comparison, "a required check for the current head failed");
  }
  // 12. CI_PENDING — covers PENDING outcomes and an UNKNOWN CI requirement
  //     policy (ci === "UNKNOWN" only ever arises from an unknown policy or
  //     a null live snapshot, both already fail-closed); CI failure/pending
  //     can never become clean.
  if (comparison.ci === "PENDING" || comparison.ci === "UNKNOWN") {
    return result("CI_PENDING", comparison, comparison.ciReason ?? "required CI state for the current head is not proven");
  }

  const draft = input.live.draft;

  // 13. Partial/unknown data can never become clean, for either posture.
  if (comparison.mergeability === "UNKNOWN") {
    return result(draft ? "DRAFT_BLOCKED" : "READY_BLOCKED", comparison, "mergeability is not proven");
  }
  if (!input.live.pagination.changed_files_complete) {
    return result(draft ? "DRAFT_BLOCKED" : "READY_BLOCKED", comparison, "changed-file pagination is incomplete");
  }

  // 14. DRAFT_CLEAN — draft PRs may be clean with no freeze yet
  //     (headComparison UNKNOWN is permitted for drafts).
  if (draft) {
    return result("DRAFT_CLEAN", comparison, null);
  }

  // 15. READY_BLOCKED — a non-draft PR without a matching freeze is never
  //     READY_CLEAN; unknown/missing frozen approval fails closed.
  if (comparison.headComparison !== "MATCH") {
    return result("READY_BLOCKED", comparison, "no frozen reviewed head matches the current head");
  }
  if (comparison.baseComparison === "UNKNOWN") {
    return result("READY_BLOCKED", comparison, "base comparison could not be established");
  }

  // 16. READY_CLEAN requires proven CI success under a REQUIRED policy at
  //     the current head — `ci === "SUCCESS"` is only ever produced by
  //     deriveCi under an explicit REQUIRED policy with every named check
  //     complete, correlated, and non-failing.
  if (comparison.ci === "SUCCESS") {
    return result("READY_CLEAN", comparison, null);
  }

  // 17. FROZEN_MATCH — frozen snapshot matches the current head and nothing
  //     above blocked, but CI is permitted to be absent only because the
  //     caller supplied an EXPLICIT VERIFIED NOT_REQUIRED policy (never
  //     inferred from an empty checks array) — see deriveCi. This is the
  //     ONLY path that can produce `ci === "NOT_REQUIRED"`.
  if (input.ciRequirementPolicy.kind === "NOT_REQUIRED" && comparison.ci === "NOT_REQUIRED") {
    return result("FROZEN_MATCH", comparison, null);
  }

  // 18. UNKNOWN — unclassified/internally-inconsistent condition. Unknown or
  //     missing data must never produce a clean state.
  return result("UNKNOWN", comparison, "condition did not match any defined N7 state");
}

// ---------------------------------------------------------------------------
// Next-permitted-action guidance (read-only)
// ---------------------------------------------------------------------------

export type N7NextAction =
  | "PROVIDE_PR_IDENTITY"
  | "REFRESH_LIVE_STATE"
  | "FREEZE_REVIEW_EVIDENCE"
  | "INSPECT_HEAD_DRIFT"
  | "INSPECT_BASE_DRIFT"
  | "INSPECT_REVIEW_BLOCKERS"
  | "INSPECT_CI_FAILURE"
  | "INSPECT_MERGE_CONFLICT"
  | "INSPECT_EVIDENCE_INTEGRITY"
  | "NO_ACTION_TERMINAL"
  | "STOP_UNKNOWN_DATA";

export const N7_NEXT_ACTIONS: readonly N7NextAction[] = [
  "PROVIDE_PR_IDENTITY",
  "REFRESH_LIVE_STATE",
  "FREEZE_REVIEW_EVIDENCE",
  "INSPECT_HEAD_DRIFT",
  "INSPECT_BASE_DRIFT",
  "INSPECT_REVIEW_BLOCKERS",
  "INSPECT_CI_FAILURE",
  "INSPECT_MERGE_CONFLICT",
  "INSPECT_EVIDENCE_INTEGRITY",
  "NO_ACTION_TERMINAL",
  "STOP_UNKNOWN_DATA",
];

const NEXT_ACTION_BY_STATE: Readonly<Record<N7PrimaryState, N7NextAction>> = {
  NO_PR: "PROVIDE_PR_IDENTITY",
  LIVE_UNCHECKED: "REFRESH_LIVE_STATE",
  LIVE_FETCH_FAILED: "REFRESH_LIVE_STATE",
  DRAFT_CLEAN: "FREEZE_REVIEW_EVIDENCE",
  DRAFT_BLOCKED: "REFRESH_LIVE_STATE",
  READY_CLEAN: "REFRESH_LIVE_STATE",
  READY_BLOCKED: "REFRESH_LIVE_STATE",
  HEAD_DRIFT: "INSPECT_HEAD_DRIFT",
  BASE_DRIFT: "INSPECT_BASE_DRIFT",
  CI_PENDING: "REFRESH_LIVE_STATE",
  CI_FAILED: "INSPECT_CI_FAILURE",
  REVIEW_BLOCKED: "INSPECT_REVIEW_BLOCKERS",
  MERGE_CONFLICT: "INSPECT_MERGE_CONFLICT",
  FROZEN_MATCH: "REFRESH_LIVE_STATE",
  FROZEN_STALE: "REFRESH_LIVE_STATE",
  MERGED: "NO_ACTION_TERMINAL",
  CLOSED_UNMERGED: "NO_ACTION_TERMINAL",
  UNKNOWN: "STOP_UNKNOWN_DATA",
};

export function deriveNextPermittedAction(primaryState: N7PrimaryState): N7NextAction {
  return NEXT_ACTION_BY_STATE[primaryState];
}

// Forbidden write-shaped verb PREFIXES. Matching is prefix-only (the action
// must be LED by the forbidden verb), not substring-anywhere: a read-only
// "INSPECT_MERGE_CONFLICT" legitimately names the MERGE_CONFLICT state it
// inspects without itself being a merge action, so a bare "MERGE" substring
// check would misfire on it. This is a structural guard mirroring
// n6State.ts's assertN6Safe, not merely a validator check, so a future call
// site cannot smuggle a write-authorizing action through this module.
const FORBIDDEN_ACTION_PREFIXES: readonly string[] = [
  "CREATE_PR",
  "MARK_READY",
  "APPROVE_REVIEW",
  "RERUN_WORKFLOW",
  "MERGE_",
  "CLOSE_",
  "DELETE_BRANCH",
  "PACKAGE_PLAN",
  "PACKAGE_COMMIT",
  "PACKAGE_PUSH",
  "PACKAGE_PR",
];

export function assertNextActionNeverAuthorizesWrite(action: N7NextAction): N7NextAction {
  for (const forbidden of FORBIDDEN_ACTION_PREFIXES) {
    if (action.startsWith(forbidden)) {
      throw new Error(`N7 next-permitted-action must never authorize a write: ${action}`);
    }
  }
  return action;
}
