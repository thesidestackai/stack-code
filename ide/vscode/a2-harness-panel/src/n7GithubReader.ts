// N7-B — read-only GitHub reader boundary (pure interface + injected transport).
//
// Source of truth: docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md
// (GitHub Read-Only Boundary §).
//
// N7-B defines ONLY:
//   - a narrow read-only N7GithubReader interface;
//   - an injected N7Transport boundary with NO default/live implementation
//     (no gh spawn, no fetch, no Octokit, no GraphQL client — a future live
//     adapter is a separate, later scope);
//   - fail-closed COMPLETE/PARTIAL/FAILED semantics for every read;
//   - exact-head correlation for checks/reviews/mergeability;
//   - assembly of a validated n7.pr-live.v1 snapshot via n7Schemas.ts's own
//     validatePrLiveSnapshot — this module never repairs or defaults
//     malformed provider data, it returns a validation failure instead.
//
// PURE BOUNDARY: no fs, no child_process, no network client, no environment
// reads, no clock reads (every `capturedAtIso` is an explicit caller input),
// no timers, no retries, no caching. The ONLY side effect this module ever
// performs is invoking the caller-injected `transport` function and awaiting
// its result.

import {
  ChangedFile,
  CheckConclusion,
  CheckResult,
  CheckStatus,
  CHECK_CONCLUSION_VALUES,
  CHECK_STATUS_VALUES,
  FieldProvenanceMap,
  Mergeability,
  MERGEABILITY_VALUES,
  N7_PR_LIVE_SCHEMA_VERSION,
  PrLiveSnapshot,
  PrProviderState,
  PR_PROVIDER_STATES,
  Provenance,
  ReviewDecision,
  REVIEW_DECISION_VALUES,
  isSafeCanonicalInteger,
  validatePrLiveSnapshot,
} from "./n7Schemas";

// ---------------------------------------------------------------------------
// Selector and rate-limit metadata
// ---------------------------------------------------------------------------

export interface PrSelector {
  owner: string;
  name: string;
  prNumber: number;
}

export interface RateLimitInfo {
  // A safe, non-provider-specific classification — never raw header dumps.
  limitClass: "PRIMARY" | "SECONDARY" | "UNKNOWN";
  remaining: number | null;
  retryAfterSeconds: number | null;
}

// ---------------------------------------------------------------------------
// Injected transport boundary — NO default/live implementation ships here.
// ---------------------------------------------------------------------------
//
// A future live adapter (network client, gh CLI wrapper, Octokit, GraphQL
// client — none of which exist in this file) would implement N7Transport in
// a separate, later-scoped module and be injected at the call site. Tests in
// this lane supply only deterministic fake transports.

export type N7TransportOperation =
  | "pull_request"
  | "changed_files"
  | "checks"
  | "reviews_and_threads"
  | "mergeability";

export interface N7TransportRequest {
  operation: N7TransportOperation;
  selector: PrSelector;
  // Present for head-scoped reads (changed_files/checks/reviews_and_threads/
  // mergeability); absent for the initial "pull_request" identity read.
  headSha?: string;
  // Explicit pagination cursor. null on the first page of a given read.
  pageToken: string | null;
}

export interface N7TransportSuccess {
  kind: "SUCCESS";
  // Provider-shaped payload for this operation; parsed and validated by the
  // corresponding readXxx function below, never trusted as-is.
  payload: unknown;
  hasMorePages: boolean;
  nextPageToken: string | null;
  rateLimit: RateLimitInfo | null;
}

export interface N7TransportAuthFailure {
  kind: "AUTH_FAILURE";
  // Safe classification only — never a token, header, credential path, or
  // scope list.
  errorClass: string;
}

export interface N7TransportRateLimited {
  kind: "RATE_LIMITED";
  rateLimit: RateLimitInfo;
}

export interface N7TransportTimeout {
  kind: "TIMEOUT";
}

export interface N7TransportError {
  kind: "PROVIDER_ERROR";
  // Safe classification only — never the original exception, message, or
  // response body.
  errorClass: string;
}

export type N7TransportResult =
  | N7TransportSuccess
  | N7TransportAuthFailure
  | N7TransportRateLimited
  | N7TransportTimeout
  | N7TransportError;

export type N7Transport = (req: N7TransportRequest) => Promise<N7TransportResult>;

// Invoke the injected transport defensively: if it throws instead of
// resolving to an N7TransportResult (a fake transport bug, or a future live
// transport's unexpected exception), normalize to a safe PROVIDER_ERROR
// rather than ever surfacing the raw thrown value (which could carry a
// credential-bearing message/stack in a future live implementation).
async function invokeTransport(transport: N7Transport, req: N7TransportRequest): Promise<N7TransportResult> {
  try {
    return await transport(req);
  } catch {
    return { kind: "PROVIDER_ERROR", errorClass: "transport_threw_exception" };
  }
}

// ---------------------------------------------------------------------------
// Read outcome envelope
// ---------------------------------------------------------------------------

export type N7ReadOutcome = "COMPLETE" | "PARTIAL" | "FAILED";

export interface N7ReadMeta {
  // Explicit caller-supplied capture timestamp. This module never reads a
  // clock; every meta.capturedAt is exactly the caller's input.
  capturedAt: string;
  headShaRequested: string | null;
  // What the provider itself reported as the head this data corresponds to,
  // when the provider payload carries that information. null when not
  // determinable (e.g. an AUTH_FAILURE before any payload arrived, or a
  // provider payload shape that omits it).
  headShaObserved: string | null;
  paginationComplete: boolean;
  rateLimit: RateLimitInfo | null;
  // Safe error class only, set exactly when outcome reflects an auth
  // failure. Never a token/header/credential path/scope list.
  authFailure: string | null;
  timedOut: boolean;
  // Safe, human-readable reason for PARTIAL/FAILED outcomes not otherwise
  // covered above (e.g. "changed_files pagination incomplete", "head
  // mismatch: requested X observed Y"). Never echoes provider response
  // bodies or exception messages.
  unknownReason: string | null;
}

function baseMeta(capturedAt: string, headShaRequested: string | null): N7ReadMeta {
  return {
    capturedAt,
    headShaRequested,
    headShaObserved: null,
    paginationComplete: false,
    rateLimit: null,
    authFailure: null,
    timedOut: false,
    unknownReason: null,
  };
}

// ---------------------------------------------------------------------------
// Per-operation read results
// ---------------------------------------------------------------------------

export interface PrIdentitySummary {
  repository: { owner: string; name: string; url: string };
  prNumber: number;
  prUrl: string;
  title: string;
  state: PrProviderState;
  draft: boolean;
  baseRef: string;
  baseSha: string;
  headRef: string;
  headSha: string;
  commitCount: number;
  mergeability: Mergeability;
  mergeStateStatus: string;
}

export interface PrIdentityRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  identity: PrIdentitySummary | null;
}

export interface PagedFilesRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  files: readonly ChangedFile[];
}

export interface ChecksRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  checks: readonly CheckResult[];
}

export interface ReviewsRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  reviewDecision: ReviewDecision | null;
  requestedChanges: readonly string[];
  unresolvedThreadCount: number | null;
  threadRefs: readonly string[];
  blockingAutomatedFindings: readonly string[];
}

export interface MergeabilityRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  mergeability: Mergeability | null;
  mergeStateStatus: string | null;
}

export interface PrLiveSnapshotRead {
  outcome: N7ReadOutcome;
  meta: N7ReadMeta;
  snapshot: PrLiveSnapshot | null;
  // Set when assembly produced a candidate object that n7Schemas.ts's own
  // validatePrLiveSnapshot rejected. Assembly never repairs or defaults a
  // malformed field to make validation pass — it surfaces the failure.
  validationError: string | null;
}

// ---------------------------------------------------------------------------
// Reader interface — READ-ONLY. No exported write method exists anywhere in
// this module: no createPullRequest, markReady, submitReview, mergePullRequest,
// closePullRequest, deleteBranch, push, or any GraphQL write operation.
// ---------------------------------------------------------------------------

export interface N7GithubReader {
  readPullRequestIdentity(selector: PrSelector, capturedAtIso: string): Promise<PrIdentityRead>;
  readPullRequestLiveSnapshot(selector: PrSelector, capturedAtIso: string): Promise<PrLiveSnapshotRead>;
  readChangedFiles(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<PagedFilesRead>;
  readChecksForHead(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<ChecksRead>;
  readReviewsAndThreads(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<ReviewsRead>;
  readMergeability(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<MergeabilityRead>;
}

// ---------------------------------------------------------------------------
// Narrow provider-payload guards (this module's own DTO contract — distinct
// from, and upstream of, n7Schemas.ts's wire-schema validators).
// ---------------------------------------------------------------------------

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function isNonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.length > 0;
}

interface ProviderPullRequestPayload {
  repository: { owner: string; name: string; url: string };
  number: number;
  url: string;
  title: string;
  state: PrProviderState;
  draft: boolean;
  baseRef: string;
  baseSha: string;
  headRef: string;
  headSha: string;
  commitCount: number;
  mergeability: Mergeability;
  mergeStateStatus: string;
}

function parseProviderPullRequestPayload(
  raw: unknown,
): { ok: true; value: ProviderPullRequestPayload } | { ok: false; reason: string } {
  if (!isPlainObject(raw)) return { ok: false, reason: "pull_request payload must be a plain object" };
  const repo = raw.repository;
  if (!isPlainObject(repo) || !isNonEmptyString(repo.owner) || !isNonEmptyString(repo.name) || !isNonEmptyString(repo.url)) {
    return { ok: false, reason: "pull_request payload: invalid repository" };
  }
  if (!isSafeCanonicalInteger(raw.number) || (raw.number as number) <= 0) {
    return { ok: false, reason: "pull_request payload: invalid PR number" };
  }
  if (!isNonEmptyString(raw.url)) return { ok: false, reason: "pull_request payload: invalid url" };
  if (typeof raw.title !== "string") return { ok: false, reason: "pull_request payload: invalid title" };
  if (!(PR_PROVIDER_STATES as readonly string[]).includes(raw.state as string)) {
    return { ok: false, reason: "pull_request payload: invalid state" };
  }
  if (typeof raw.draft !== "boolean") return { ok: false, reason: "pull_request payload: invalid draft" };
  if (!isNonEmptyString(raw.baseRef)) return { ok: false, reason: "pull_request payload: invalid baseRef" };
  if (typeof raw.baseSha !== "string") return { ok: false, reason: "pull_request payload: invalid baseSha" };
  if (!isNonEmptyString(raw.headRef)) return { ok: false, reason: "pull_request payload: invalid headRef" };
  if (typeof raw.headSha !== "string" || raw.headSha.length === 0) {
    return { ok: false, reason: "pull_request payload: invalid headSha" };
  }
  if (!isSafeCanonicalInteger(raw.commitCount) || (raw.commitCount as number) < 0) {
    return { ok: false, reason: "pull_request payload: invalid commitCount" };
  }
  if (!(MERGEABILITY_VALUES as readonly string[]).includes(raw.mergeability as string)) {
    return { ok: false, reason: "pull_request payload: invalid mergeability" };
  }
  if (typeof raw.mergeStateStatus !== "string") {
    return { ok: false, reason: "pull_request payload: invalid mergeStateStatus" };
  }
  return {
    ok: true,
    value: {
      repository: { owner: repo.owner, name: repo.name, url: repo.url },
      number: raw.number as number,
      url: raw.url,
      title: raw.title,
      state: raw.state as PrProviderState,
      draft: raw.draft,
      baseRef: raw.baseRef,
      baseSha: raw.baseSha as string,
      headRef: raw.headRef,
      headSha: raw.headSha,
      commitCount: raw.commitCount as number,
      mergeability: raw.mergeability as Mergeability,
      mergeStateStatus: raw.mergeStateStatus,
    },
  };
}

interface ProviderChangedFilesPayload {
  files: readonly ChangedFile[];
}

function parseProviderChangedFilesPayload(
  raw: unknown,
): { ok: true; value: ProviderChangedFilesPayload } | { ok: false; reason: string } {
  if (!isPlainObject(raw) || !Array.isArray(raw.files)) {
    return { ok: false, reason: "changed_files payload must be a plain object with a files array" };
  }
  const files: ChangedFile[] = [];
  for (const item of raw.files) {
    if (!isPlainObject(item) || !isNonEmptyString(item.filename) || !isNonEmptyString(item.status)) {
      return { ok: false, reason: "changed_files payload: invalid file item" };
    }
    if (!isSafeCanonicalInteger(item.additions) || (item.additions as number) < 0) {
      return { ok: false, reason: "changed_files payload: invalid additions" };
    }
    if (!isSafeCanonicalInteger(item.deletions) || (item.deletions as number) < 0) {
      return { ok: false, reason: "changed_files payload: invalid deletions" };
    }
    if (item.previousFilename !== null && item.previousFilename !== undefined && typeof item.previousFilename !== "string") {
      return { ok: false, reason: "changed_files payload: invalid previousFilename" };
    }
    files.push({
      filename: item.filename,
      status: item.status,
      additions: item.additions as number,
      deletions: item.deletions as number,
      previous_filename: (item.previousFilename as string | undefined) ?? null,
    });
  }
  return { ok: true, value: { files } };
}

interface ProviderChecksPayload {
  // The head this batch of checks was fetched/correlated against, per the
  // provider. Compared against the requested headSha for correlation.
  headSha: string;
  checks: readonly CheckResult[];
}

function parseProviderChecksPayload(raw: unknown): { ok: true; value: ProviderChecksPayload } | { ok: false; reason: string } {
  if (!isPlainObject(raw) || typeof raw.headSha !== "string" || raw.headSha.length === 0 || !Array.isArray(raw.checks)) {
    return { ok: false, reason: "checks payload must be a plain object with headSha and a checks array" };
  }
  const checks: CheckResult[] = [];
  for (const item of raw.checks) {
    if (
      !isPlainObject(item) ||
      !isNonEmptyString(item.provider) ||
      !isNonEmptyString(item.name) ||
      !isNonEmptyString(item.app) ||
      !(CHECK_STATUS_VALUES as readonly string[]).includes(item.status as string) ||
      typeof item.headSha !== "string" ||
      item.headSha.length === 0
    ) {
      return { ok: false, reason: "checks payload: invalid check item" };
    }
    if (item.conclusion !== null && !(CHECK_CONCLUSION_VALUES as readonly string[]).includes(item.conclusion as string)) {
      return { ok: false, reason: "checks payload: invalid conclusion" };
    }
    const provenance: Provenance = "GITHUB_LIVE";
    checks.push({
      provider: item.provider,
      name: item.name,
      app: item.app,
      status: item.status as CheckStatus,
      conclusion: (item.conclusion as CheckConclusion) ?? null,
      head_sha: item.headSha,
      started_at: (item.startedAt as string | undefined) ?? null,
      completed_at: (item.completedAt as string | undefined) ?? null,
      details_url: (item.detailsUrl as string | undefined) ?? null,
      provenance,
    });
  }
  return { ok: true, value: { headSha: raw.headSha, checks } };
}

interface ProviderReviewsPayload {
  // The head the provider states this review/thread data correlates to.
  // null when the provider cannot prove correlation to any specific head.
  observedHeadSha: string | null;
  reviewDecision: ReviewDecision;
  requestedChanges: readonly string[];
  unresolvedThreadCount: number;
  threadRefs: readonly string[];
  blockingAutomatedFindings: readonly string[];
}

function parseProviderReviewsPayload(
  raw: unknown,
): { ok: true; value: ProviderReviewsPayload } | { ok: false; reason: string } {
  if (!isPlainObject(raw)) return { ok: false, reason: "reviews_and_threads payload must be a plain object" };
  if (raw.observedHeadSha !== null && typeof raw.observedHeadSha !== "string") {
    return { ok: false, reason: "reviews_and_threads payload: invalid observedHeadSha" };
  }
  if (!(REVIEW_DECISION_VALUES as readonly string[]).includes(raw.reviewDecision as string)) {
    return { ok: false, reason: "reviews_and_threads payload: invalid reviewDecision" };
  }
  if (!Array.isArray(raw.requestedChanges) || !raw.requestedChanges.every((x) => typeof x === "string")) {
    return { ok: false, reason: "reviews_and_threads payload: invalid requestedChanges" };
  }
  if (!isSafeCanonicalInteger(raw.unresolvedThreadCount) || (raw.unresolvedThreadCount as number) < 0) {
    return { ok: false, reason: "reviews_and_threads payload: invalid unresolvedThreadCount" };
  }
  if (!Array.isArray(raw.threadRefs) || !raw.threadRefs.every((x) => typeof x === "string")) {
    return { ok: false, reason: "reviews_and_threads payload: invalid threadRefs" };
  }
  if (!Array.isArray(raw.blockingAutomatedFindings) || !raw.blockingAutomatedFindings.every((x) => typeof x === "string")) {
    return { ok: false, reason: "reviews_and_threads payload: invalid blockingAutomatedFindings" };
  }
  return {
    ok: true,
    value: {
      observedHeadSha: (raw.observedHeadSha as string | null) ?? null,
      reviewDecision: raw.reviewDecision as ReviewDecision,
      requestedChanges: raw.requestedChanges as readonly string[],
      unresolvedThreadCount: raw.unresolvedThreadCount as number,
      threadRefs: raw.threadRefs as readonly string[],
      blockingAutomatedFindings: raw.blockingAutomatedFindings as readonly string[],
    },
  };
}

interface ProviderMergeabilityPayload {
  headSha: string;
  mergeability: Mergeability;
  mergeStateStatus: string;
}

function parseProviderMergeabilityPayload(
  raw: unknown,
): { ok: true; value: ProviderMergeabilityPayload } | { ok: false; reason: string } {
  if (!isPlainObject(raw) || typeof raw.headSha !== "string" || raw.headSha.length === 0) {
    return { ok: false, reason: "mergeability payload must be a plain object with headSha" };
  }
  if (!(MERGEABILITY_VALUES as readonly string[]).includes(raw.mergeability as string)) {
    return { ok: false, reason: "mergeability payload: invalid mergeability" };
  }
  if (typeof raw.mergeStateStatus !== "string") {
    return { ok: false, reason: "mergeability payload: invalid mergeStateStatus" };
  }
  return {
    ok: true,
    value: { headSha: raw.headSha, mergeability: raw.mergeability as Mergeability, mergeStateStatus: raw.mergeStateStatus },
  };
}

// ---------------------------------------------------------------------------
// Result-envelope helpers for non-SUCCESS transport outcomes
// ---------------------------------------------------------------------------

function failedFromTransportResult(res: N7TransportResult, meta: N7ReadMeta): N7ReadMeta {
  switch (res.kind) {
    case "AUTH_FAILURE":
      return { ...meta, authFailure: res.errorClass };
    case "TIMEOUT":
      return { ...meta, timedOut: true };
    case "RATE_LIMITED":
      return { ...meta, rateLimit: res.rateLimit, unknownReason: "rate limited before any data was obtained" };
    case "PROVIDER_ERROR":
      return { ...meta, unknownReason: res.errorClass };
    default:
      return meta;
  }
}

// ---------------------------------------------------------------------------
// readPullRequestIdentity
// ---------------------------------------------------------------------------

async function readPullRequestIdentity(
  transport: N7Transport,
  selector: PrSelector,
  capturedAtIso: string,
): Promise<PrIdentityRead> {
  const meta = baseMeta(capturedAtIso, null);
  const res = await invokeTransport(transport, { operation: "pull_request", selector, pageToken: null });
  if (res.kind !== "SUCCESS") {
    return { outcome: "FAILED", meta: failedFromTransportResult(res, meta), identity: null };
  }
  const parsed = parseProviderPullRequestPayload(res.payload);
  if (!parsed.ok) {
    return { outcome: "FAILED", meta: { ...meta, unknownReason: parsed.reason }, identity: null };
  }
  const p = parsed.value;
  return {
    outcome: "COMPLETE",
    meta: { ...meta, headShaObserved: p.headSha, rateLimit: res.rateLimit, paginationComplete: true },
    identity: {
      repository: p.repository,
      prNumber: p.number,
      prUrl: p.url,
      title: p.title,
      state: p.state,
      draft: p.draft,
      baseRef: p.baseRef,
      baseSha: p.baseSha,
      headRef: p.headRef,
      headSha: p.headSha,
      commitCount: p.commitCount,
      mergeability: p.mergeability,
      mergeStateStatus: p.mergeStateStatus,
    },
  };
}

// ---------------------------------------------------------------------------
// Generic pagination loop for head-scoped, multi-page reads
// ---------------------------------------------------------------------------

interface PageLoopResult<TPayload> {
  // Successfully parsed payloads from every page fetched before either
  // completion or a stopping failure.
  pages: TPayload[];
  // Pagination genuinely completed (last page reported hasMorePages: false).
  complete: boolean;
  // Set when the loop stopped due to a non-SUCCESS transport result or a
  // malformed page, distinct from a clean "no more pages" stop.
  stoppedBy: N7TransportResult | { kind: "MALFORMED_PAGE"; reason: string } | null;
  lastRateLimit: RateLimitInfo | null;
}

async function fetchAllPages<TPayload>(
  transport: N7Transport,
  operation: N7TransportOperation,
  selector: PrSelector,
  headSha: string,
  parsePayload: (raw: unknown) => { ok: true; value: TPayload } | { ok: false; reason: string },
): Promise<PageLoopResult<TPayload>> {
  const pages: TPayload[] = [];
  let pageToken: string | null = null;
  let lastRateLimit: RateLimitInfo | null = null;

  // Bounded loop: a well-behaved transport eventually reports
  // hasMorePages:false. This module performs no retry; each iteration
  // fetches exactly the next declared page once.
  for (let guard = 0; guard < 10000; guard++) {
    const res = await invokeTransport(transport, { operation, selector, headSha, pageToken });
    if (res.kind !== "SUCCESS") {
      return { pages, complete: false, stoppedBy: res, lastRateLimit };
    }
    lastRateLimit = res.rateLimit;
    const parsed = parsePayload(res.payload);
    if (!parsed.ok) {
      return { pages, complete: false, stoppedBy: { kind: "MALFORMED_PAGE", reason: parsed.reason }, lastRateLimit };
    }
    pages.push(parsed.value);
    if (!res.hasMorePages) {
      return { pages, complete: true, stoppedBy: null, lastRateLimit };
    }
    pageToken = res.nextPageToken;
  }
  return { pages, complete: false, stoppedBy: { kind: "MALFORMED_PAGE", reason: "pagination did not terminate" }, lastRateLimit };
}

// ---------------------------------------------------------------------------
// readChangedFiles
// ---------------------------------------------------------------------------

async function readChangedFiles(
  transport: N7Transport,
  selector: PrSelector,
  headSha: string,
  capturedAtIso: string,
): Promise<PagedFilesRead> {
  const meta = baseMeta(capturedAtIso, headSha);
  const loop = await fetchAllPages(transport, "changed_files", selector, headSha, parseProviderChangedFilesPayload);
  const files = loop.pages.flatMap((p) => p.files);

  if (loop.complete) {
    return { outcome: "COMPLETE", meta: { ...meta, paginationComplete: true, rateLimit: loop.lastRateLimit }, files };
  }
  // Partial (or failed if literally zero pages were ever obtained).
  const partialMeta = enrichMetaFromStop(meta, loop);
  const outcome: N7ReadOutcome = loop.pages.length > 0 ? "PARTIAL" : "FAILED";
  return { outcome, meta: partialMeta, files };
}

function enrichMetaFromStop(
  meta: N7ReadMeta,
  loop: { stoppedBy: N7TransportResult | { kind: "MALFORMED_PAGE"; reason: string } | null; lastRateLimit: RateLimitInfo | null },
): N7ReadMeta {
  const withRateLimit = { ...meta, rateLimit: loop.lastRateLimit, paginationComplete: false };
  if (loop.stoppedBy === null) return withRateLimit;
  if (loop.stoppedBy.kind === "MALFORMED_PAGE") {
    return { ...withRateLimit, unknownReason: loop.stoppedBy.reason };
  }
  return failedFromTransportResult(loop.stoppedBy, withRateLimit);
}

// ---------------------------------------------------------------------------
// readChecksForHead — exact-head correlation enforced
// ---------------------------------------------------------------------------

async function readChecksForHead(
  transport: N7Transport,
  selector: PrSelector,
  headSha: string,
  capturedAtIso: string,
): Promise<ChecksRead> {
  const meta = baseMeta(capturedAtIso, headSha);
  const loop = await fetchAllPages(transport, "checks", selector, headSha, parseProviderChecksPayload);
  const checks = loop.pages.flatMap((p) => p.checks);
  // The provider's own declared correlation head for each fetched page.
  // Mixed-head batches (pages reporting different heads) or any page
  // reporting a head other than the one requested both break the
  // "definitely current-head" guarantee this read is meant to provide.
  const observedHeads = new Set(loop.pages.map((p) => p.headSha));
  const singleObservedHead = observedHeads.size === 1 ? [...observedHeads][0] : null;

  if (!loop.complete) {
    return { outcome: loop.pages.length > 0 ? "PARTIAL" : "FAILED", meta: enrichMetaFromStop(meta, loop), checks };
  }
  if (observedHeads.size !== 1 || singleObservedHead !== headSha) {
    // old_head_green_checks_do_not_clear_current_head /
    // mixed_head_checks_are_partial_or_unknown: correlation could not be
    // proven for the requested head, even though pagination itself
    // completed cleanly.
    return {
      outcome: "PARTIAL",
      meta: {
        ...meta,
        headShaObserved: singleObservedHead,
        paginationComplete: true,
        rateLimit: loop.lastRateLimit,
        unknownReason: `checks head correlation mismatch: requested ${headSha}, observed ${
          observedHeads.size === 0 ? "none" : [...observedHeads].join(",")
        }`,
      },
      checks,
    };
  }
  return {
    outcome: "COMPLETE",
    meta: { ...meta, headShaObserved: singleObservedHead, paginationComplete: true, rateLimit: loop.lastRateLimit },
    checks,
  };
}

// ---------------------------------------------------------------------------
// readReviewsAndThreads — exact-head correlation enforced
// ---------------------------------------------------------------------------

async function readReviewsAndThreads(
  transport: N7Transport,
  selector: PrSelector,
  headSha: string,
  capturedAtIso: string,
): Promise<ReviewsRead> {
  const meta = baseMeta(capturedAtIso, headSha);
  const loop = await fetchAllPages(transport, "reviews_and_threads", selector, headSha, parseProviderReviewsPayload);

  const emptyResult = {
    reviewDecision: null as ReviewDecision | null,
    requestedChanges: [] as readonly string[],
    unresolvedThreadCount: null as number | null,
    threadRefs: [] as readonly string[],
    blockingAutomatedFindings: [] as readonly string[],
  };

  if (loop.pages.length === 0) {
    return { outcome: loop.complete ? "PARTIAL" : "FAILED", meta: enrichMetaFromStop(meta, loop), ...emptyResult };
  }

  // Merge across pages: requested-changes/thread-refs/findings accumulate;
  // decision and unresolved count come from the most recent page (each
  // page in this provider's own contract restates the whole-PR-level
  // decision/count, not a per-page delta).
  const last = loop.pages[loop.pages.length - 1];
  const requestedChanges = loop.pages.flatMap((p) => p.requestedChanges);
  const threadRefs = loop.pages.flatMap((p) => p.threadRefs);
  const blockingAutomatedFindings = loop.pages.flatMap((p) => p.blockingAutomatedFindings);
  const observedHeads = new Set(loop.pages.map((p) => p.observedHeadSha));
  const singleObservedHead = observedHeads.size === 1 ? [...observedHeads][0] : null;

  const merged = {
    reviewDecision: last.reviewDecision,
    requestedChanges,
    unresolvedThreadCount: last.unresolvedThreadCount,
    threadRefs,
    blockingAutomatedFindings,
  };

  if (!loop.complete) {
    return { outcome: "PARTIAL", meta: enrichMetaFromStop(meta, loop), ...merged };
  }
  if (singleObservedHead === null || singleObservedHead !== headSha) {
    // review_fact_without_head_correlation_is_unknown /
    // missing_head_correlation_is_unknown
    return {
      outcome: "PARTIAL",
      meta: {
        ...meta,
        headShaObserved: singleObservedHead,
        paginationComplete: true,
        rateLimit: loop.lastRateLimit,
        unknownReason: "review data does not correlate to a single confirmed current head",
      },
      ...merged,
    };
  }
  return {
    outcome: "COMPLETE",
    meta: { ...meta, headShaObserved: singleObservedHead, paginationComplete: true, rateLimit: loop.lastRateLimit },
    ...merged,
  };
}

// ---------------------------------------------------------------------------
// readMergeability — exact-head correlation enforced
// ---------------------------------------------------------------------------

async function readMergeability(
  transport: N7Transport,
  selector: PrSelector,
  headSha: string,
  capturedAtIso: string,
): Promise<MergeabilityRead> {
  const meta = baseMeta(capturedAtIso, headSha);
  const res = await invokeTransport(transport, { operation: "mergeability", selector, headSha, pageToken: null });
  if (res.kind !== "SUCCESS") {
    return { outcome: "FAILED", meta: failedFromTransportResult(res, meta), mergeability: null, mergeStateStatus: null };
  }
  const parsed = parseProviderMergeabilityPayload(res.payload);
  if (!parsed.ok) {
    return { outcome: "FAILED", meta: { ...meta, unknownReason: parsed.reason }, mergeability: null, mergeStateStatus: null };
  }
  const p = parsed.value;
  if (p.headSha !== headSha) {
    return {
      outcome: "PARTIAL",
      meta: {
        ...meta,
        headShaObserved: p.headSha,
        paginationComplete: true,
        rateLimit: res.rateLimit,
        unknownReason: `mergeability head correlation mismatch: requested ${headSha}, observed ${p.headSha}`,
      },
      mergeability: null,
      mergeStateStatus: null,
    };
  }
  return {
    outcome: "COMPLETE",
    meta: { ...meta, headShaObserved: p.headSha, paginationComplete: true, rateLimit: res.rateLimit },
    mergeability: p.mergeability,
    mergeStateStatus: p.mergeStateStatus,
  };
}

// ---------------------------------------------------------------------------
// readPullRequestLiveSnapshot — orchestrates the five reads and assembles a
// validated n7.pr-live.v1 snapshot. Only attempts assembly when every
// sub-read is COMPLETE; otherwise surfaces PARTIAL/FAILED with snapshot:null
// rather than fabricating a snapshot from incomplete data.
// ---------------------------------------------------------------------------

function worstOutcome(outcomes: readonly N7ReadOutcome[]): N7ReadOutcome {
  if (outcomes.includes("FAILED")) return "FAILED";
  if (outcomes.includes("PARTIAL")) return "PARTIAL";
  return "COMPLETE";
}

async function readPullRequestLiveSnapshot(
  transport: N7Transport,
  selector: PrSelector,
  capturedAtIso: string,
): Promise<PrLiveSnapshotRead> {
  const identity = await readPullRequestIdentity(transport, selector, capturedAtIso);
  if (identity.outcome !== "COMPLETE" || identity.identity === null) {
    return { outcome: identity.outcome === "FAILED" ? "FAILED" : "PARTIAL", meta: identity.meta, snapshot: null, validationError: null };
  }

  const headSha = identity.identity.headSha;
  const [files, checks, reviews, mergeability] = await Promise.all([
    readChangedFiles(transport, selector, headSha, capturedAtIso),
    readChecksForHead(transport, selector, headSha, capturedAtIso),
    readReviewsAndThreads(transport, selector, headSha, capturedAtIso),
    readMergeability(transport, selector, headSha, capturedAtIso),
  ]);

  const overall = worstOutcome([files.outcome, checks.outcome, reviews.outcome, mergeability.outcome]);
  if (overall !== "COMPLETE") {
    const reasons = [files, checks, reviews, mergeability]
      .filter((r) => r.outcome !== "COMPLETE")
      .map((r) => r.meta.unknownReason ?? r.meta.authFailure ?? (r.meta.timedOut ? "timed out" : r.outcome))
      .join("; ");
    return {
      outcome: overall,
      meta: { ...identity.meta, headShaObserved: headSha, unknownReason: reasons || null },
      snapshot: null,
      validationError: null,
    };
  }

  const candidate = assembleSnapshot(selector, identity.identity, files.files, checks.checks, reviews, mergeability, capturedAtIso);
  const validated = validatePrLiveSnapshot(candidate);
  if (!validated.ok) {
    return {
      outcome: "FAILED",
      meta: { ...identity.meta, headShaObserved: headSha, unknownReason: validated.reason },
      snapshot: null,
      validationError: validated.reason,
    };
  }
  return {
    outcome: "COMPLETE",
    meta: { ...identity.meta, headShaObserved: headSha },
    snapshot: validated.value,
    validationError: null,
  };
}

function assembleSnapshot(
  selector: PrSelector,
  identity: PrIdentitySummary,
  files: readonly ChangedFile[],
  checks: readonly CheckResult[],
  reviews: ReviewsRead,
  mergeability: MergeabilityRead,
  capturedAtIso: string,
): unknown {
  const provenance: FieldProvenanceMap = {
    head_sha: "GITHUB_LIVE",
    base_sha: "GITHUB_LIVE",
    changed_files: "GITHUB_LIVE",
    checks: "GITHUB_LIVE",
    reviews: "GITHUB_LIVE",
    mergeability: "GITHUB_LIVE",
  };
  return {
    schema_version: N7_PR_LIVE_SCHEMA_VERSION,
    snapshot_id: `live_${capturedAtIso.replace(/[^0-9A-Za-z]/g, "")}_pr${identity.prNumber}_head${identity.headSha.slice(0, 8)}`,
    captured_at: capturedAtIso,
    captured_by: { source: "n7-github-reader", reader_version: "n7b-fake-transport.v1" },
    repository: {
      owner: identity.repository.owner,
      name: identity.repository.name,
      url: identity.repository.url,
      provider: "github",
    },
    pr_number: identity.prNumber,
    pr_url: identity.prUrl,
    title: identity.title,
    state: identity.state,
    draft: identity.draft,
    base_ref: identity.baseRef,
    base_sha: identity.baseSha,
    head_ref: identity.headRef,
    head_sha: identity.headSha,
    commit_count: identity.commitCount,
    changed_file_count: files.length,
    changed_files: files,
    mergeability: mergeability.mergeability ?? identity.mergeability,
    merge_state_status: mergeability.mergeStateStatus ?? identity.mergeStateStatus,
    checks,
    reviews: {
      review_decision: reviews.reviewDecision ?? "UNKNOWN",
      requested_changes: reviews.requestedChanges,
      unresolved_review_threads: {
        count: reviews.unresolvedThreadCount ?? 0,
        complete: true,
        thread_refs: reviews.threadRefs,
      },
      blocking_automated_findings: reviews.blockingAutomatedFindings,
    },
    pagination: {
      changed_files_complete: true,
      checks_complete: true,
      review_threads_complete: true,
    },
    source_identity: { api: "n7-fake-transport", request_id: "", etag: "", rate_limit_remaining: null },
    provenance,
    unknowns: [],
  };
}

// ---------------------------------------------------------------------------
// Factory — the ONLY way to obtain an N7GithubReader. There is no default
// export and no module-level singleton; a transport must always be supplied
// explicitly by the caller.
// ---------------------------------------------------------------------------

export function createN7GithubReader(transport: N7Transport): N7GithubReader {
  return {
    readPullRequestIdentity: (selector, capturedAtIso) => readPullRequestIdentity(transport, selector, capturedAtIso),
    readPullRequestLiveSnapshot: (selector, capturedAtIso) => readPullRequestLiveSnapshot(transport, selector, capturedAtIso),
    readChangedFiles: (selector, headSha, capturedAtIso) => readChangedFiles(transport, selector, headSha, capturedAtIso),
    readChecksForHead: (selector, headSha, capturedAtIso) => readChecksForHead(transport, selector, headSha, capturedAtIso),
    readReviewsAndThreads: (selector, headSha, capturedAtIso) => readReviewsAndThreads(transport, selector, headSha, capturedAtIso),
    readMergeability: (selector, headSha, capturedAtIso) => readMergeability(transport, selector, headSha, capturedAtIso),
  };
}
