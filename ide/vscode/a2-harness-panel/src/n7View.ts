// N7-D — Draft PR Card VIEW MODEL (pure).
//
// Source of truth: docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md.
//
// buildN7PrCardView is PURE: no fs, no spawn, no network, no clock reads, no
// hidden global state, no mutation of inputs, no randomness. It NEVER
// recomputes N7-A's comparison/precedence logic — every state, severity, and
// next-permitted-action value is taken directly from the already-derived
// N7DerivedStateResult (produced by n7State.deriveN7PrimaryState, called by
// the extension, out of N7-D's scope) and passed straight through or
// formatted into display text. This module renders no GitHub data, reads no
// `.claw/n7` evidence store, and calls no N7EvidenceStore method — it only
// turns already-validated, already-derived N7 facts into a display-ready
// shape. It authorizes no PR mutation, package-rung action, or write of any
// kind: "next permitted action" is informational text only.

import { FrozenReviewSnapshot, PrLiveSnapshot } from "./n7Schemas";
import {
  CiComparisonState,
  N7DerivedStateResult,
  N7NextAction,
  N7PrimaryState,
  N7Severity,
  ReviewComparisonState,
  assertNextActionNeverAuthorizesWrite,
  deriveNextPermittedAction,
} from "./n7State";

export interface N7PrCardInput {
  repository: { owner: string; name: string };
  live: PrLiveSnapshot | null;
  frozen: FrozenReviewSnapshot | null;
  // Already-computed by the caller via n7State.deriveN7PrimaryState — never
  // recomputed here.
  derived: N7DerivedStateResult;
}

export interface N7PrCardIdentityView {
  headSha: string | null;
  baseSha: string | null;
  capturedAt: string | null;
  statusLabel: string;
}

export interface N7PrCardFrozenIdentityView extends N7PrCardIdentityView {
  freezeId: string | null;
}

export interface N7PrCardComparisonView {
  primaryState: N7PrimaryState;
  severity: N7Severity;
  // Always begins with the severity word ("OK:"/"WARN:"/"STOP:"/
  // "UNKNOWN:"/"TERMINAL:") so the state is legible without relying on
  // color, icon, CSS class, or border.
  exactLabel: string;
  detail: string;
}

// CiComparisonState already includes NOT_REQUIRED (an explicit, verified
// policy fact — see n7State.ts); the card must not collapse it into UNKNOWN,
// which would misrepresent a verified "no CI required" fact as unproven data.
export interface N7PrCardCiView {
  state: CiComparisonState;
  headSha: string | null;
  summary: string;
}

export interface N7PrCardReviewView {
  // Reuses N7-A's own ReviewComparisonState vocabulary (CLEAN/BLOCKED/
  // UNKNOWN) rather than inventing a parallel "CLEAR" synonym.
  state: ReviewComparisonState;
  summary: string;
  unresolvedThreadCount: number | null;
}

export interface N7PrCardBlocker {
  code: string;
  label: string;
  detail: string;
}

export interface N7PrCardNextAction {
  action: N7NextAction;
  label: string;
  detail: string;
}

export interface N7PrCardUnknown {
  field: string;
  reason: string;
}

export interface N7PrCardViewModel {
  repository: { owner: string; name: string };
  prNumber: number | null;
  prTitle: string | null;
  current: N7PrCardIdentityView;
  frozen: N7PrCardFrozenIdentityView;
  comparison: N7PrCardComparisonView;
  ci: N7PrCardCiView;
  review: N7PrCardReviewView;
  blockers: readonly N7PrCardBlocker[];
  nextPermittedAction: N7PrCardNextAction;
  unknowns: readonly N7PrCardUnknown[];
}

// A git SHA field that is present-but-empty is treated identically to
// "not available" everywhere in N7-A (see n7State.ts's deriveHeadComparison/
// deriveBaseComparison, which both test `.length === 0`) — this view model
// follows the same convention rather than inventing a third state.
function shaOrNull(value: string | undefined | null): string | null {
  return value && value.length > 0 ? value : null;
}

function isoOrNull(value: string | undefined | null): string | null {
  return value && value.length > 0 ? value : null;
}

const PRIMARY_STATE_LABELS: Readonly<Record<N7PrimaryState, string>> = {
  NO_PR: "WARN: no PR identity is known yet",
  LIVE_UNCHECKED: "WARN: current live PR state has never been refreshed",
  LIVE_FETCH_FAILED: "STOP: the last live refresh attempt failed",
  DRAFT_CLEAN: "OK: draft PR has no known blockers",
  DRAFT_BLOCKED: "STOP: draft PR has unresolved blockers",
  READY_CLEAN: "OK: current head equals frozen reviewed head and CI succeeded for it",
  READY_BLOCKED: "STOP: non-draft PR has unresolved blockers",
  HEAD_DRIFT: "STOP: current head differs from frozen reviewed head",
  BASE_DRIFT: "STOP: current base differs from frozen base",
  CI_PENDING: "WARN: required CI state for the current head is not proven",
  CI_FAILED: "STOP: a required check for the current head failed",
  REVIEW_BLOCKED: "STOP: review is blocked or review state is incomplete",
  MERGE_CONFLICT: "STOP: mergeability reports conflict",
  FROZEN_MATCH: "MATCH: current head equals frozen reviewed head",
  FROZEN_STALE: "WARN: live PR state is stale; refresh before relying on it",
  MERGED: "TERMINAL: PR is merged",
  CLOSED_UNMERGED: "TERMINAL: PR is closed without merging",
  UNKNOWN: "UNKNOWN: condition did not match any defined N7 state",
};

const NEXT_ACTION_COPY: Readonly<Record<N7NextAction, { label: string; detail: string }>> = {
  PROVIDE_PR_IDENTITY: { label: "Provide PR identity", detail: "No PR number or URL is known yet." },
  REFRESH_LIVE_STATE: { label: "Refresh Live PR State", detail: "Re-read the current PR state before relying on it." },
  FREEZE_REVIEW_EVIDENCE: { label: "Freeze Review Evidence", detail: "Capture the current reviewed state as the frozen approved snapshot." },
  INSPECT_HEAD_DRIFT: { label: "Inspect head drift", detail: "The current head no longer matches the frozen reviewed head." },
  INSPECT_BASE_DRIFT: { label: "Inspect base drift", detail: "The current base no longer matches the frozen base." },
  INSPECT_REVIEW_BLOCKERS: { label: "Inspect review blockers", detail: "Review is blocked or review state is incomplete." },
  INSPECT_CI_FAILURE: { label: "Inspect CI failure", detail: "A required check for the current head failed." },
  INSPECT_MERGE_CONFLICT: { label: "Inspect merge conflict", detail: "Mergeability reports a conflict." },
  INSPECT_EVIDENCE_INTEGRITY: { label: "Inspect evidence integrity", detail: "The local evidence chain failed verification." },
  NO_ACTION_TERMINAL: { label: "No action — terminal state", detail: "The PR is in a terminal state (merged or closed); no further action is offered here." },
  STOP_UNKNOWN_DATA: { label: "Stop — unknown data", detail: "The available data did not match any defined N7 state; do not proceed." },
};

function ciSummary(ci: CiComparisonState, ciReason: string | null, headSha: string | null): string {
  switch (ci) {
    case "SUCCESS":
      return headSha ? `SUCCESS for ${headSha}` : "SUCCESS, but no current head is available to name";
    case "FAILED":
      return `FAILED${ciReason ? ` — ${ciReason}` : ""}`;
    case "PENDING":
      return `PENDING${ciReason ? ` — ${ciReason}` : ""}`;
    case "NOT_REQUIRED":
      return "NOT_REQUIRED — no CI is required for this PR";
    case "UNKNOWN":
    default:
      return `UNKNOWN — CI status unknown for current head${ciReason ? ` (${ciReason})` : ""}`;
  }
}

function reviewSummary(input: N7PrCardInput): string {
  const state = input.derived.comparison.review;
  const live = input.live;
  if (state === "UNKNOWN") {
    return "UNKNOWN — review-thread data incomplete";
  }
  if (state === "CLEAN") {
    return "CLEAN — approved, no unresolved threads, no requested changes";
  }
  // BLOCKED — name the specific reason(s) when available.
  if (!live) {
    return "BLOCKED";
  }
  const reasons: string[] = [];
  if (live.reviews.requested_changes.length > 0) {
    reasons.push(`${live.reviews.requested_changes.length} reviewer(s) requested changes`);
  }
  if (live.reviews.unresolved_review_threads.count > 0) {
    reasons.push(`${live.reviews.unresolved_review_threads.count} unresolved thread(s)`);
  }
  if (live.reviews.blocking_automated_findings.length > 0) {
    reasons.push(`${live.reviews.blocking_automated_findings.length} blocking automated finding(s)`);
  }
  if (live.reviews.review_decision !== "APPROVED") {
    reasons.push(`review decision: ${live.reviews.review_decision}`);
  }
  return reasons.length > 0 ? `BLOCKED — ${reasons.join("; ")}` : "BLOCKED";
}

function buildBlockers(input: N7PrCardInput): N7PrCardBlocker[] {
  const c = input.derived.comparison;
  const blockers: N7PrCardBlocker[] = [];
  if (c.headComparison === "DRIFT") {
    blockers.push({
      code: "HEAD_DRIFT",
      label: "Current head differs from frozen reviewed head",
      detail: `current: ${shaOrNull(input.live?.head_sha) ?? "unknown"} · frozen: ${shaOrNull(input.frozen?.approved_head_sha) ?? "unknown"}`,
    });
  }
  if (c.baseComparison === "DRIFT") {
    blockers.push({
      code: "BASE_DRIFT",
      label: "Current base differs from frozen base",
      detail: `current: ${shaOrNull(input.live?.base_sha) ?? "unknown"} · frozen: ${shaOrNull(input.frozen?.base_sha) ?? "unknown"}`,
    });
  }
  if (c.ci === "FAILED") {
    blockers.push({ code: "CI_FAILED", label: "CI failed for the current head", detail: c.ciReason ?? "a required check failed" });
  } else if (c.ci === "PENDING") {
    blockers.push({ code: "CI_PENDING", label: "CI has not proven success for the current head", detail: c.ciReason ?? "required CI state is not proven" });
  } else if (c.ci === "UNKNOWN") {
    blockers.push({ code: "CI_UNKNOWN", label: "CI status unknown for the current head", detail: c.ciReason ?? "CI requirement policy is unknown" });
  }
  if (c.review === "BLOCKED") {
    blockers.push({ code: "REVIEW_BLOCKED", label: "Review is blocked", detail: reviewSummary(input) });
  } else if (c.review === "UNKNOWN") {
    blockers.push({ code: "REVIEW_UNKNOWN", label: "Review state is incomplete", detail: "review-thread data incomplete" });
  }
  if (c.mergeability === "CONFLICT") {
    blockers.push({ code: "MERGE_CONFLICT", label: "Mergeability reports conflict", detail: "the current head cannot be merged cleanly" });
  }
  if (c.freshness === "STALE") {
    blockers.push({ code: "STALE_LIVE_DATA", label: "Live PR state is stale", detail: "the current live snapshot is older than the freshness policy" });
  }
  if (input.derived.blockingReason) {
    const alreadyCovered = blockers.some((b) => b.detail === input.derived.blockingReason);
    if (!alreadyCovered) {
      blockers.push({ code: "STATE_BLOCKING_REASON", label: "State-blocking condition", detail: input.derived.blockingReason });
    }
  }
  return blockers;
}

function buildUnknowns(input: N7PrCardInput): N7PrCardUnknown[] {
  const c = input.derived.comparison;
  const unknowns: N7PrCardUnknown[] = [];
  if (c.headComparison === "UNKNOWN") {
    unknowns.push({ field: "current_head_vs_frozen_head", reason: "current head or frozen reviewed head is not available" });
  }
  if (c.baseComparison === "UNKNOWN") {
    unknowns.push({ field: "current_base_vs_frozen_base", reason: "current base or frozen base is not available" });
  }
  if (c.ci === "UNKNOWN") {
    unknowns.push({ field: "ci", reason: c.ciReason ?? "CI requirement policy or current head CI state is unknown" });
  }
  if (c.review === "UNKNOWN") {
    unknowns.push({ field: "review", reason: "review-thread data incomplete" });
  }
  if (c.mergeability === "UNKNOWN") {
    unknowns.push({ field: "mergeability", reason: "mergeability is not proven" });
  }
  for (const u of input.live?.unknowns ?? []) {
    unknowns.push({ field: `live.${u.id}`, reason: u.reason });
  }
  for (const u of input.frozen?.unknowns ?? []) {
    unknowns.push({ field: `frozen.${u.id}`, reason: u.reason });
  }
  return unknowns;
}

export function buildN7PrCardView(input: N7PrCardInput): N7PrCardViewModel {
  const live = input.live;
  const frozen = input.frozen;
  const comparison = input.derived.comparison;

  const currentStatusLabel = live
    ? `${live.state}${live.draft ? " (draft)" : ""}`
    : "Unknown — current head was not available";
  const frozenStatusLabel = frozen ? "Frozen reviewed evidence captured" : "No frozen review snapshot exists";

  const nextAction = assertNextActionNeverAuthorizesWrite(deriveNextPermittedAction(input.derived.primaryState));
  const nextActionCopy = NEXT_ACTION_COPY[nextAction];

  return {
    repository: { owner: input.repository.owner, name: input.repository.name },
    prNumber: live?.pr_number ?? frozen?.pr_number ?? null,
    prTitle: live?.title ?? null,
    current: {
      headSha: shaOrNull(live?.head_sha),
      baseSha: shaOrNull(live?.base_sha),
      capturedAt: isoOrNull(live?.captured_at),
      statusLabel: currentStatusLabel,
    },
    frozen: {
      headSha: shaOrNull(frozen?.approved_head_sha),
      baseSha: shaOrNull(frozen?.base_sha),
      capturedAt: isoOrNull(frozen?.frozen_at),
      freezeId: frozen?.snapshot_id && frozen.snapshot_id.length > 0 ? frozen.snapshot_id : null,
      statusLabel: frozenStatusLabel,
    },
    comparison: {
      primaryState: input.derived.primaryState,
      severity: input.derived.severity,
      exactLabel: PRIMARY_STATE_LABELS[input.derived.primaryState],
      detail: input.derived.blockingReason ?? (comparison.reasons.length > 0 ? comparison.reasons.join("; ") : "no blocking condition detected"),
    },
    ci: {
      state: comparison.ci,
      headSha: shaOrNull(live?.head_sha),
      summary: ciSummary(comparison.ci, comparison.ciReason, shaOrNull(live?.head_sha)),
    },
    review: {
      state: comparison.review,
      summary: reviewSummary(input),
      unresolvedThreadCount: live ? live.reviews.unresolved_review_threads.count : null,
    },
    blockers: buildBlockers(input),
    nextPermittedAction: {
      action: nextAction,
      label: nextActionCopy.label,
      detail: nextActionCopy.detail,
    },
    unknowns: buildUnknowns(input),
  };
}
