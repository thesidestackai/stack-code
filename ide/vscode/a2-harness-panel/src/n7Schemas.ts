// N7-A — versioned schemas, canonical serialization, and hash-input rules (pure).
//
// Source of truth: docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md
// (Schemas §, Canonical Serialization and Hashing §, Canonical Number
// Encoding §).
//
// N7-A is schemas and pure computation ONLY:
//   - no GitHub calls, no panel rendering, no `.claw/n7` storage;
//   - no fs, no spawn, no network, no timers, no process.env reads;
//   - the only Node built-in used is `crypto`, for SHA-256 over an
//     already-canonicalized string (a pure, deterministic computation, not
//     I/O) — this mirrors the scope's explicit allowance that N7-A "may
//     construct canonical hash input and calculate SHA-256 only when this
//     can remain a pure deterministic function".
// PURE: no fs, no spawn, no network, no clock reads.
//
// Validators in this file are fail-closed and recursive: every required
// nested object/array item is validated explicitly. Missing or malformed
// required data is REJECTED, never silently replaced with "", 0, [], or a
// default object — an accepted value always reflects what the caller
// actually supplied.

import { createHash } from "crypto";

// ---------------------------------------------------------------------------
// Trust classification and provenance
// ---------------------------------------------------------------------------

export type TrustClassification =
  | "VERIFIED"
  | "INFERRED"
  | "OPERATOR_ASSERTED"
  | "UNKNOWN";

export const TRUST_CLASSIFICATIONS: readonly TrustClassification[] = [
  "VERIFIED",
  "INFERRED",
  "OPERATOR_ASSERTED",
  "UNKNOWN",
];

export type Provenance =
  | "GITHUB_LIVE"
  | "LOCAL_GIT"
  | "FROZEN_EVIDENCE"
  | "DERIVED_COMPARISON"
  | "OPERATOR_ASSERTION"
  | "UNKNOWN_NOT_CHECKED";

export const PROVENANCE_VALUES: readonly Provenance[] = [
  "GITHUB_LIVE",
  "LOCAL_GIT",
  "FROZEN_EVIDENCE",
  "DERIVED_COMPARISON",
  "OPERATOR_ASSERTION",
  "UNKNOWN_NOT_CHECKED",
];

function isTrustClassification(v: unknown): v is TrustClassification {
  return typeof v === "string" && (TRUST_CLASSIFICATIONS as readonly string[]).includes(v);
}

function isProvenance(v: unknown): v is Provenance {
  return typeof v === "string" && (PROVENANCE_VALUES as readonly string[]).includes(v);
}

// ---------------------------------------------------------------------------
// Schema version constants
// ---------------------------------------------------------------------------

export const N7_PR_LIVE_SCHEMA_VERSION = "n7.pr-live.v1" as const;
export const N7_PR_REVIEW_FREEZE_SCHEMA_VERSION = "n7.pr-review-freeze.v1" as const;
export const N7_TIMELINE_EVENT_SCHEMA_VERSION = "n7.timeline-event.v1" as const;

// ---------------------------------------------------------------------------
// Field provenance — per-field record of where a value came from
// ---------------------------------------------------------------------------

// A field-provenance map records the Provenance for each field name a
// schema chooses to attribute. Keys are field names; values are Provenance.
export type FieldProvenanceMap = Readonly<Record<string, Provenance>>;

function isFieldProvenanceMap(v: unknown): v is FieldProvenanceMap {
  if (v === null || typeof v !== "object" || Array.isArray(v)) {
    return false;
  }
  return Object.values(v as Record<string, unknown>).every(isProvenance);
}

function isNonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.length > 0;
}

function isStringArray(v: unknown): v is readonly string[] {
  return Array.isArray(v) && v.every((x) => typeof x === "string");
}

// A PR (or issue) number is a positive identity: GitHub numbers PRs
// starting at 1, and 0/negative values are never real PR identities. This
// is a distinct, stricter contract than the general nonnegative-safe-integer
// counts used elsewhere (commit_count, changed_file_count, etc.) — a PR
// number of 0 is not "an unusually small but valid count", it is an
// impossible identity and must be rejected, not silently accepted.
function parsePositiveSafeInteger(value: unknown, path: string): { ok: true; value: number } | { ok: false; reason: string } {
  if (!isSafeCanonicalInteger(value) || (value as number) <= 0) {
    return { ok: false, reason: `${path}: expected positive safe integer` };
  }
  return { ok: true, value: value as number };
}

// ---------------------------------------------------------------------------
// Strict RFC3339 UTC timestamp validation
// ---------------------------------------------------------------------------
//
// Every schema timestamp field in this module (captured_at, frozen_at,
// created_at, started_at, completed_at) is validated with this function.
// Supported lexical form ONLY:
//
//   YYYY-MM-DDTHH:MM:SSZ
//   YYYY-MM-DDTHH:MM:SS.fractionZ
//
// Rejected: space separator, numeric offsets (+00:00), a missing "Z",
// date-only values, out-of-range calendar/time components, and impossible
// calendar dates (e.g. 2026-02-30). This performs NO current-clock read:
// `Date.UTC(...)` and `new Date(explicitMs)` both take explicit numeric
// arguments derived only from the caller-supplied string — never
// `Date.now()` and never an argument-less `new Date()`.

const RFC3339_UTC_RE = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?Z$/;

export function validateRfc3339UtcTimestamp(
  value: unknown,
): { ok: true; epochMs: number } | { ok: false; reason: string } {
  if (typeof value !== "string") {
    return { ok: false, reason: "expected an RFC3339 UTC timestamp string" };
  }
  // String.prototype.match (not RegExp.prototype.exec) so the panel's
  // static guard, which naively greps for the literal token "exec(" as a
  // process-spawn signal, does not misfire on this pure regex match.
  const m = value.match(RFC3339_UTC_RE);
  if (!m) {
    return {
      ok: false,
      reason: "expected format YYYY-MM-DDTHH:MM:SS[.fraction]Z (uppercase T and Z, no offset, no space separator)",
    };
  }
  const year = Number(m[1]);
  const month = Number(m[2]);
  const day = Number(m[3]);
  const hour = Number(m[4]);
  const minute = Number(m[5]);
  const second = Number(m[6]);
  const fraction = m[7] ? Number(m[7]) : 0;

  if (month < 1 || month > 12) return { ok: false, reason: "month out of range" };
  if (day < 1 || day > 31) return { ok: false, reason: "day out of range" };
  if (hour > 23) return { ok: false, reason: "hour out of range" };
  if (minute > 59) return { ok: false, reason: "minute out of range" };
  if (second > 59) return { ok: false, reason: "second out of range" };

  const wholeSecondsMs = Date.UTC(year, month - 1, day, hour, minute, second, 0);
  const constructed = new Date(wholeSecondsMs);
  if (
    constructed.getUTCFullYear() !== year ||
    constructed.getUTCMonth() !== month - 1 ||
    constructed.getUTCDate() !== day ||
    constructed.getUTCHours() !== hour ||
    constructed.getUTCMinutes() !== minute ||
    constructed.getUTCSeconds() !== second
  ) {
    // Date.UTC silently normalizes impossible dates (e.g. Feb 30 -> Mar 2).
    // Comparing every round-tripped component against the input catches
    // that normalization instead of accepting it.
    return { ok: false, reason: "not a real calendar date/time" };
  }
  return { ok: true, epochMs: wholeSecondsMs + Math.round(fraction * 1000) };
}

// ---------------------------------------------------------------------------
// Git commit SHA / SHA-256 hex format validation
// ---------------------------------------------------------------------------
//
// Git commit SHAs are validated as 7-64 lowercase hex characters (covering
// abbreviated and full SHA-1, and full SHA-256 for repositories migrated to
// it). An empty string is accepted ONLY where the scope's own JSON examples
// use "" as the explicit "not yet known" placeholder for that exact field
// (head_sha, base_sha, workspace.git_head, ci_summary.head_sha); any
// NONEMPTY value that is not valid hex is always rejected. Fields that
// identify what was actually reviewed (FrozenReviewSnapshot.approved_head_sha,
// pr.head_sha on a timeline event) require a nonempty SHA — "unknown" is not
// an honest value for something that was, by definition, observed.

const GIT_SHA_RE = /^[0-9a-f]{7,64}$/;

function isGitShaOrEmpty(value: unknown): value is string {
  return typeof value === "string" && (value.length === 0 || GIT_SHA_RE.test(value));
}

function isNonEmptyGitSha(value: unknown): value is string {
  return typeof value === "string" && GIT_SHA_RE.test(value);
}

// Full SHA-256 hex digest: exactly 64 lowercase hex characters. Empty is
// accepted only for the frozen-record's own not-yet-computed hash fields;
// any nonempty value that is not exactly 64 hex characters is rejected.
const SHA256_HEX_RE = /^[0-9a-f]{64}$/;

function isSha256HexOrEmpty(value: unknown): value is string {
  return typeof value === "string" && (value.length === 0 || SHA256_HEX_RE.test(value));
}

// ---------------------------------------------------------------------------
// Evidence classification — fact / inference / operator-assertion / unknown
// ---------------------------------------------------------------------------

export interface EvidenceFact {
  id: string;
  classification: "VERIFIED";
  statement: string;
  provenance: Provenance;
  source_event_id: string | null;
  source_artifact_id: string | null;
  captured_at: string;
}

export interface EvidenceInference {
  id: string;
  classification: "INFERRED";
  statement: string;
  provenance: Provenance;
  supports: readonly string[];
  rule: string;
  captured_at: string;
}

export interface EvidenceOperatorAssertion {
  id: string;
  classification: "OPERATOR_ASSERTED";
  statement: string;
  provenance: "OPERATOR_ASSERTION";
  asserted_by: string;
  assertion_kind: string;
  promote_to_verified: false;
  captured_at: string;
}

export interface EvidenceUnknown {
  id: string;
  classification: "UNKNOWN";
  statement: string;
  provenance: Provenance;
  reason: string;
  blocks: readonly string[];
  captured_at: string;
}

export type EvidenceItem =
  | EvidenceFact
  | EvidenceInference
  | EvidenceOperatorAssertion
  | EvidenceUnknown;

export function validateEvidenceItem(raw: unknown): { ok: true; value: EvidenceItem } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: "evidence item must be a plain object" };
  }
  const o = raw as Record<string, unknown>;
  if (!isNonEmptyString(o.id)) {
    return { ok: false, reason: "evidence item missing string id" };
  }
  if (!isNonEmptyString(o.statement)) {
    return { ok: false, reason: "evidence item missing string statement" };
  }
  const capturedAt = validateRfc3339UtcTimestamp(o.captured_at);
  if (!capturedAt.ok) {
    return { ok: false, reason: `evidence item captured_at: ${capturedAt.reason}` };
  }
  if (!isTrustClassification(o.classification)) {
    return { ok: false, reason: "evidence item has invalid classification" };
  }
  switch (o.classification) {
    case "VERIFIED": {
      if (!isProvenance(o.provenance)) {
        return { ok: false, reason: "VERIFIED item has invalid provenance" };
      }
      if (o.source_event_id !== null && typeof o.source_event_id !== "string") {
        return { ok: false, reason: "VERIFIED item source_event_id must be string or null" };
      }
      if (o.source_artifact_id !== null && typeof o.source_artifact_id !== "string") {
        return { ok: false, reason: "VERIFIED item source_artifact_id must be string or null" };
      }
      const value: EvidenceFact = {
        id: o.id,
        classification: "VERIFIED",
        statement: o.statement,
        provenance: o.provenance,
        source_event_id: (o.source_event_id as string | null) ?? null,
        source_artifact_id: (o.source_artifact_id as string | null) ?? null,
        captured_at: o.captured_at as string,
      };
      return { ok: true, value };
    }
    case "INFERRED": {
      if (!isProvenance(o.provenance)) {
        return { ok: false, reason: "INFERRED item has invalid provenance" };
      }
      if (!isStringArray(o.supports) || o.supports.length === 0) {
        return { ok: false, reason: "INFERRED item requires non-empty supports[] of fact/event IDs" };
      }
      if (!isNonEmptyString(o.rule)) {
        return { ok: false, reason: "INFERRED item missing string rule" };
      }
      const value: EvidenceInference = {
        id: o.id,
        classification: "INFERRED",
        statement: o.statement,
        provenance: o.provenance,
        supports: o.supports,
        rule: o.rule,
        captured_at: o.captured_at as string,
      };
      return { ok: true, value };
    }
    case "OPERATOR_ASSERTED": {
      if (o.provenance !== "OPERATOR_ASSERTION") {
        return { ok: false, reason: "OPERATOR_ASSERTED item must carry provenance OPERATOR_ASSERTION" };
      }
      if (!isNonEmptyString(o.asserted_by)) {
        return { ok: false, reason: "OPERATOR_ASSERTED item missing string asserted_by" };
      }
      if (!isNonEmptyString(o.assertion_kind)) {
        return { ok: false, reason: "OPERATOR_ASSERTED item missing string assertion_kind" };
      }
      if (o.promote_to_verified !== false) {
        return { ok: false, reason: "OPERATOR_ASSERTED item must never set promote_to_verified to true" };
      }
      const value: EvidenceOperatorAssertion = {
        id: o.id,
        classification: "OPERATOR_ASSERTED",
        statement: o.statement,
        provenance: "OPERATOR_ASSERTION",
        asserted_by: o.asserted_by,
        assertion_kind: o.assertion_kind,
        promote_to_verified: false,
        captured_at: o.captured_at as string,
      };
      return { ok: true, value };
    }
    case "UNKNOWN": {
      if (!isProvenance(o.provenance)) {
        return { ok: false, reason: "UNKNOWN item has invalid provenance" };
      }
      if (!isNonEmptyString(o.reason)) {
        return { ok: false, reason: "UNKNOWN item missing string reason" };
      }
      if (!isStringArray(o.blocks)) {
        return { ok: false, reason: "UNKNOWN item requires blocks[] (may be empty)" };
      }
      const value: EvidenceUnknown = {
        id: o.id,
        classification: "UNKNOWN",
        statement: o.statement,
        provenance: o.provenance,
        reason: o.reason,
        blocks: o.blocks,
        captured_at: o.captured_at as string,
      };
      return { ok: true, value };
    }
  }
}

// An OPERATOR_ASSERTED item can never be promoted to VERIFIED. This is a
// structural guard, not merely a validator check, so callers who construct
// evidence items programmatically (not only via validateEvidenceItem) still
// cannot produce a promoted assertion.
export function assertOperatorAssertionNeverPromoted(item: EvidenceOperatorAssertion): void {
  if ((item.promote_to_verified as boolean) !== false) {
    throw new Error("operator assertion must never be promoted to verified");
  }
}

// ---------------------------------------------------------------------------
// Artifact reference
// ---------------------------------------------------------------------------

export interface ArtifactReference {
  artifact_id: string;
  kind: string;
  path: string;
  sha256: string;
  size_bytes: number;
  redaction: string;
}

export function validateArtifactReference(raw: unknown): { ok: true; value: ArtifactReference } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: "artifact reference must be a plain object" };
  }
  const o = raw as Record<string, unknown>;
  if (!isNonEmptyString(o.artifact_id)) return { ok: false, reason: "artifact reference missing artifact_id" };
  if (!isNonEmptyString(o.kind)) return { ok: false, reason: "artifact reference missing kind" };
  if (!isNonEmptyString(o.path)) return { ok: false, reason: "artifact reference missing path" };
  if (!isSha256HexOrEmpty(o.sha256)) {
    return { ok: false, reason: "artifact reference sha256 must be a 64-character lowercase hex digest or empty string" };
  }
  if (!isSafeCanonicalInteger(o.size_bytes) || (o.size_bytes as number) < 0) {
    return { ok: false, reason: "artifact reference size_bytes must be a nonnegative safe integer" };
  }
  if (!isNonEmptyString(o.redaction)) return { ok: false, reason: "artifact reference missing redaction" };
  return {
    ok: true,
    value: {
      artifact_id: o.artifact_id,
      kind: o.kind,
      path: o.path,
      sha256: o.sha256 as string,
      size_bytes: o.size_bytes as number,
      redaction: o.redaction,
    },
  };
}

// ---------------------------------------------------------------------------
// PR live snapshot — n7.pr-live.v1
// ---------------------------------------------------------------------------

export type PrProviderState = "OPEN" | "CLOSED" | "MERGED";
export const PR_PROVIDER_STATES: readonly PrProviderState[] = ["OPEN", "CLOSED", "MERGED"];

export type Mergeability = "MERGEABLE" | "CONFLICTING" | "UNKNOWN";
export const MERGEABILITY_VALUES: readonly Mergeability[] = ["MERGEABLE", "CONFLICTING", "UNKNOWN"];

export type CheckStatus = "QUEUED" | "IN_PROGRESS" | "COMPLETED";
export const CHECK_STATUS_VALUES: readonly CheckStatus[] = ["QUEUED", "IN_PROGRESS", "COMPLETED"];

export type CheckConclusion =
  | "SUCCESS"
  | "FAILURE"
  | "NEUTRAL"
  | "CANCELLED"
  | "TIMED_OUT"
  | "ACTION_REQUIRED"
  | "SKIPPED"
  | null;

export const CHECK_CONCLUSION_VALUES: readonly Exclude<CheckConclusion, null>[] = [
  "SUCCESS",
  "FAILURE",
  "NEUTRAL",
  "CANCELLED",
  "TIMED_OUT",
  "ACTION_REQUIRED",
  "SKIPPED",
];

export type ReviewDecision = "APPROVED" | "CHANGES_REQUESTED" | "REVIEW_REQUIRED" | "UNKNOWN";
export const REVIEW_DECISION_VALUES: readonly ReviewDecision[] = [
  "APPROVED",
  "CHANGES_REQUESTED",
  "REVIEW_REQUIRED",
  "UNKNOWN",
];

export interface ChangedFile {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
  previous_filename: string | null;
}

export interface CheckResult {
  provider: string;
  name: string;
  app: string;
  status: CheckStatus;
  conclusion: CheckConclusion;
  head_sha: string;
  started_at: string | null;
  completed_at: string | null;
  details_url: string | null;
  provenance: Provenance;
}

export interface PrLiveSnapshot {
  schema_version: typeof N7_PR_LIVE_SCHEMA_VERSION;
  snapshot_id: string;
  captured_at: string;
  captured_by: { source: string; reader_version: string };
  repository: { owner: string; name: string; url: string; provider: string };
  pr_number: number;
  pr_url: string;
  title: string;
  state: PrProviderState;
  draft: boolean;
  base_ref: string;
  base_sha: string;
  head_ref: string;
  head_sha: string;
  commit_count: number;
  changed_file_count: number;
  changed_files: readonly ChangedFile[];
  mergeability: Mergeability;
  merge_state_status: string;
  checks: readonly CheckResult[];
  reviews: {
    review_decision: ReviewDecision;
    requested_changes: readonly string[];
    unresolved_review_threads: { count: number; complete: boolean; thread_refs: readonly string[] };
    blocking_automated_findings: readonly string[];
  };
  pagination: {
    changed_files_complete: boolean;
    checks_complete: boolean;
    review_threads_complete: boolean;
  };
  source_identity: { api: string; request_id: string; etag: string; rate_limit_remaining: number | null };
  provenance: FieldProvenanceMap;
  unknowns: readonly EvidenceUnknown[];
}

function validateChangedFile(raw: unknown, path: string): { ok: true; value: ChangedFile } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: `${path}: expected a plain object` };
  }
  const o = raw as Record<string, unknown>;
  if (!isNonEmptyString(o.filename)) return { ok: false, reason: `${path}.filename: expected nonempty string` };
  if (!isNonEmptyString(o.status)) return { ok: false, reason: `${path}.status: expected nonempty string` };
  if (!isSafeCanonicalInteger(o.additions) || (o.additions as number) < 0) {
    return { ok: false, reason: `${path}.additions: expected a nonnegative safe integer` };
  }
  if (!isSafeCanonicalInteger(o.deletions) || (o.deletions as number) < 0) {
    return { ok: false, reason: `${path}.deletions: expected a nonnegative safe integer` };
  }
  if (o.previous_filename !== null && typeof o.previous_filename !== "string") {
    return { ok: false, reason: `${path}.previous_filename: expected string or null` };
  }
  return {
    ok: true,
    value: {
      filename: o.filename,
      status: o.status,
      additions: o.additions as number,
      deletions: o.deletions as number,
      previous_filename: (o.previous_filename as string | null) ?? null,
    },
  };
}

function validateCheckResult(raw: unknown, path: string): { ok: true; value: CheckResult } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: `${path}: expected a plain object` };
  }
  const o = raw as Record<string, unknown>;
  if (!isNonEmptyString(o.provider)) return { ok: false, reason: `${path}.provider: expected nonempty string` };
  if (!isNonEmptyString(o.name)) return { ok: false, reason: `${path}.name: expected nonempty string` };
  if (!isNonEmptyString(o.app)) return { ok: false, reason: `${path}.app: expected nonempty string` };
  if (!(CHECK_STATUS_VALUES as readonly string[]).includes(o.status as string)) {
    return { ok: false, reason: `${path}.status: expected one of ${CHECK_STATUS_VALUES.join(", ")}` };
  }
  if (o.conclusion !== null && !(CHECK_CONCLUSION_VALUES as readonly string[]).includes(o.conclusion as string)) {
    return { ok: false, reason: `${path}.conclusion: expected a valid conclusion or null` };
  }
  if (!isGitShaOrEmpty(o.head_sha)) {
    return { ok: false, reason: `${path}.head_sha: expected a git SHA or empty string` };
  }
  if (o.started_at !== null) {
    const t = validateRfc3339UtcTimestamp(o.started_at);
    if (!t.ok) return { ok: false, reason: `${path}.started_at: ${t.reason}` };
  }
  if (o.completed_at !== null) {
    const t = validateRfc3339UtcTimestamp(o.completed_at);
    if (!t.ok) return { ok: false, reason: `${path}.completed_at: ${t.reason}` };
  }
  if (o.details_url !== null && typeof o.details_url !== "string") {
    return { ok: false, reason: `${path}.details_url: expected string or null` };
  }
  if (!isProvenance(o.provenance)) return { ok: false, reason: `${path}.provenance: invalid provenance` };
  return {
    ok: true,
    value: {
      provider: o.provider,
      name: o.name,
      app: o.app,
      status: o.status as CheckStatus,
      conclusion: (o.conclusion as CheckConclusion) ?? null,
      head_sha: o.head_sha as string,
      started_at: (o.started_at as string | null) ?? null,
      completed_at: (o.completed_at as string | null) ?? null,
      details_url: (o.details_url as string | null) ?? null,
      provenance: o.provenance,
    },
  };
}

export function validatePrLiveSnapshot(raw: unknown): { ok: true; value: PrLiveSnapshot } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: "pr-live snapshot must be a plain object" };
  }
  const o = raw as Record<string, unknown>;
  if (o.schema_version !== N7_PR_LIVE_SCHEMA_VERSION) {
    return { ok: false, reason: `unsupported schema_version: ${String(o.schema_version)}` };
  }
  if (!isNonEmptyString(o.snapshot_id)) return { ok: false, reason: "snapshot_id: expected nonempty string" };

  const capturedAt = validateRfc3339UtcTimestamp(o.captured_at);
  if (!capturedAt.ok) return { ok: false, reason: `captured_at: ${capturedAt.reason}` };

  const capturedByRaw = o.captured_by;
  if (capturedByRaw === null || typeof capturedByRaw !== "object" || Array.isArray(capturedByRaw)) {
    return { ok: false, reason: "captured_by: expected a plain object" };
  }
  const cb = capturedByRaw as Record<string, unknown>;
  if (!isNonEmptyString(cb.source)) return { ok: false, reason: "captured_by.source: expected nonempty string" };
  if (typeof cb.reader_version !== "string") return { ok: false, reason: "captured_by.reader_version: expected string" };

  const repositoryRaw = o.repository;
  if (repositoryRaw === null || typeof repositoryRaw !== "object" || Array.isArray(repositoryRaw)) {
    return { ok: false, reason: "repository: expected a plain object" };
  }
  const repo = repositoryRaw as Record<string, unknown>;
  if (!isNonEmptyString(repo.owner)) return { ok: false, reason: "repository.owner: expected nonempty string" };
  if (!isNonEmptyString(repo.name)) return { ok: false, reason: "repository.name: expected nonempty string" };
  if (!isNonEmptyString(repo.url)) return { ok: false, reason: "repository.url: expected nonempty string" };
  if (!isNonEmptyString(repo.provider)) return { ok: false, reason: "repository.provider: expected nonempty string" };

  const prNumberResult = parsePositiveSafeInteger(o.pr_number, "pr_number");
  if (!prNumberResult.ok) return prNumberResult;
  if (!isNonEmptyString(o.pr_url)) return { ok: false, reason: "pr_url: expected nonempty string" };
  if (typeof o.title !== "string") return { ok: false, reason: "title: expected string" };
  if (!(PR_PROVIDER_STATES as readonly string[]).includes(o.state as string)) {
    return { ok: false, reason: "state: invalid" };
  }
  if (typeof o.draft !== "boolean") return { ok: false, reason: "draft: expected boolean" };
  if (!isNonEmptyString(o.base_ref)) return { ok: false, reason: "base_ref: expected nonempty string" };
  if (!isGitShaOrEmpty(o.base_sha)) return { ok: false, reason: "base_sha: expected a git SHA or empty string" };
  if (!isNonEmptyString(o.head_ref)) return { ok: false, reason: "head_ref: expected nonempty string" };
  if (!isGitShaOrEmpty(o.head_sha)) return { ok: false, reason: "head_sha: expected a git SHA or empty string" };

  if (!isSafeCanonicalInteger(o.commit_count) || (o.commit_count as number) < 0) {
    return { ok: false, reason: "commit_count: expected a nonnegative safe integer" };
  }
  if (!isSafeCanonicalInteger(o.changed_file_count) || (o.changed_file_count as number) < 0) {
    return { ok: false, reason: "changed_file_count: expected a nonnegative safe integer" };
  }

  if (!Array.isArray(o.changed_files)) return { ok: false, reason: "changed_files: expected an array" };
  const changedFiles: ChangedFile[] = [];
  for (let i = 0; i < o.changed_files.length; i++) {
    const r = validateChangedFile(o.changed_files[i], `changed_files[${i}]`);
    if (!r.ok) return r;
    changedFiles.push(r.value);
  }

  if (!(MERGEABILITY_VALUES as readonly string[]).includes(o.mergeability as string)) {
    return { ok: false, reason: "mergeability: invalid" };
  }
  if (typeof o.merge_state_status !== "string") return { ok: false, reason: "merge_state_status: expected string" };

  if (!Array.isArray(o.checks)) return { ok: false, reason: "checks: expected an array" };
  const checks: CheckResult[] = [];
  for (let i = 0; i < o.checks.length; i++) {
    const r = validateCheckResult(o.checks[i], `checks[${i}]`);
    if (!r.ok) return r;
    checks.push(r.value);
  }

  const reviewsRaw = o.reviews;
  if (reviewsRaw === null || typeof reviewsRaw !== "object" || Array.isArray(reviewsRaw)) {
    return { ok: false, reason: "reviews: expected a plain object" };
  }
  const rv = reviewsRaw as Record<string, unknown>;
  if (!(REVIEW_DECISION_VALUES as readonly string[]).includes(rv.review_decision as string)) {
    return { ok: false, reason: "reviews.review_decision: invalid" };
  }
  if (!isStringArray(rv.requested_changes)) {
    return { ok: false, reason: "reviews.requested_changes: expected a string array" };
  }
  const urtRaw = rv.unresolved_review_threads;
  if (urtRaw === null || typeof urtRaw !== "object" || Array.isArray(urtRaw)) {
    return { ok: false, reason: "reviews.unresolved_review_threads: expected a plain object" };
  }
  const urt = urtRaw as Record<string, unknown>;
  if (!isSafeCanonicalInteger(urt.count) || (urt.count as number) < 0) {
    return { ok: false, reason: "reviews.unresolved_review_threads.count: expected a nonnegative safe integer" };
  }
  if (typeof urt.complete !== "boolean") {
    return { ok: false, reason: "reviews.unresolved_review_threads.complete: expected boolean" };
  }
  if (!isStringArray(urt.thread_refs)) {
    return { ok: false, reason: "reviews.unresolved_review_threads.thread_refs: expected a string array" };
  }
  if (!isStringArray(rv.blocking_automated_findings)) {
    return { ok: false, reason: "reviews.blocking_automated_findings: expected a string array" };
  }

  const paginationRaw = o.pagination;
  if (paginationRaw === null || typeof paginationRaw !== "object" || Array.isArray(paginationRaw)) {
    return { ok: false, reason: "pagination: expected a plain object" };
  }
  const pagination = paginationRaw as Record<string, unknown>;
  if (typeof pagination.changed_files_complete !== "boolean") {
    return { ok: false, reason: "pagination.changed_files_complete: expected boolean" };
  }
  if (typeof pagination.checks_complete !== "boolean") {
    return { ok: false, reason: "pagination.checks_complete: expected boolean" };
  }
  if (typeof pagination.review_threads_complete !== "boolean") {
    return { ok: false, reason: "pagination.review_threads_complete: expected boolean" };
  }

  // Count coherence: when a collection is marked complete, its declared
  // total must match what was actually captured. When collection is
  // incomplete, the declared total may legitimately exceed the locally
  // captured array (more items exist than were paged in), so no coherence
  // check applies in that case.
  if (pagination.changed_files_complete === true && (o.changed_file_count as number) !== changedFiles.length) {
    return {
      ok: false,
      reason: `changed_file_count: expected ${changedFiles.length} (changed_files.length) when pagination.changed_files_complete is true, got ${o.changed_file_count as number}`,
    };
  }
  if (urt.complete === true && (urt.count as number) !== (urt.thread_refs as readonly string[]).length) {
    return {
      ok: false,
      reason: `reviews.unresolved_review_threads.count: expected ${(urt.thread_refs as readonly string[]).length} (thread_refs.length) when complete is true, got ${urt.count as number}`,
    };
  }

  const sourceIdentityRaw = o.source_identity;
  if (sourceIdentityRaw === null || typeof sourceIdentityRaw !== "object" || Array.isArray(sourceIdentityRaw)) {
    return { ok: false, reason: "source_identity: expected a plain object" };
  }
  const si = sourceIdentityRaw as Record<string, unknown>;
  if (typeof si.api !== "string") return { ok: false, reason: "source_identity.api: expected string" };
  if (typeof si.request_id !== "string") return { ok: false, reason: "source_identity.request_id: expected string" };
  if (typeof si.etag !== "string") return { ok: false, reason: "source_identity.etag: expected string" };
  if (si.rate_limit_remaining !== null && !isSafeCanonicalInteger(si.rate_limit_remaining)) {
    return { ok: false, reason: "source_identity.rate_limit_remaining: expected a safe integer or null" };
  }

  if (!isFieldProvenanceMap(o.provenance)) return { ok: false, reason: "provenance: invalid provenance map" };

  if (!Array.isArray(o.unknowns)) return { ok: false, reason: "unknowns: expected an array" };
  const unknowns: EvidenceUnknown[] = [];
  for (let i = 0; i < o.unknowns.length; i++) {
    const r = validateEvidenceItem(o.unknowns[i]);
    if (!r.ok) return { ok: false, reason: `unknowns[${i}]: ${r.reason}` };
    if (r.value.classification !== "UNKNOWN") {
      return { ok: false, reason: `unknowns[${i}]: expected classification UNKNOWN` };
    }
    unknowns.push(r.value);
  }

  const value: PrLiveSnapshot = {
    schema_version: N7_PR_LIVE_SCHEMA_VERSION,
    snapshot_id: o.snapshot_id as string,
    captured_at: o.captured_at as string,
    captured_by: { source: cb.source as string, reader_version: cb.reader_version as string },
    repository: {
      owner: repo.owner as string,
      name: repo.name as string,
      url: repo.url as string,
      provider: repo.provider as string,
    },
    pr_number: prNumberResult.value,
    pr_url: o.pr_url as string,
    title: o.title as string,
    state: o.state as PrProviderState,
    draft: o.draft as boolean,
    base_ref: o.base_ref as string,
    base_sha: o.base_sha as string,
    head_ref: o.head_ref as string,
    head_sha: o.head_sha as string,
    commit_count: o.commit_count as number,
    changed_file_count: o.changed_file_count as number,
    changed_files: changedFiles,
    mergeability: o.mergeability as Mergeability,
    merge_state_status: o.merge_state_status as string,
    checks,
    reviews: {
      review_decision: rv.review_decision as ReviewDecision,
      requested_changes: rv.requested_changes,
      unresolved_review_threads: {
        count: urt.count as number,
        complete: urt.complete as boolean,
        thread_refs: urt.thread_refs,
      },
      blocking_automated_findings: rv.blocking_automated_findings,
    },
    pagination: {
      changed_files_complete: pagination.changed_files_complete as boolean,
      checks_complete: pagination.checks_complete as boolean,
      review_threads_complete: pagination.review_threads_complete as boolean,
    },
    source_identity: {
      api: si.api as string,
      request_id: si.request_id as string,
      etag: si.etag as string,
      rate_limit_remaining: (si.rate_limit_remaining as number | null) ?? null,
    },
    provenance: o.provenance as FieldProvenanceMap,
    unknowns,
  };
  return { ok: true, value };
}

// ---------------------------------------------------------------------------
// Frozen review snapshot — n7.pr-review-freeze.v1
// ---------------------------------------------------------------------------

export interface FrozenReviewSnapshot {
  schema_version: typeof N7_PR_REVIEW_FREEZE_SCHEMA_VERSION;
  snapshot_id: string;
  frozen_at: string;
  repository: { owner: string; name: string };
  pr_number: number;
  pr_snapshot_ref: string;
  pr_snapshot_sha256: string;
  approved_head_sha: string;
  base_sha: string;
  changed_file_count: number;
  changed_filenames_sha256: string;
  ci_summary: { state: string; head_sha: string; check_identities: readonly string[] };
  review_summary: {
    decision: ReviewDecision;
    requested_changes_count: number;
    unresolved_threads_count: number;
    complete: boolean;
  };
  mergeability: string;
  source_api_identity: { api: string; request_id: string; etag: string };
  evidence_refs: readonly ArtifactReference[];
  operator_assertions: readonly EvidenceOperatorAssertion[];
  facts: readonly EvidenceFact[];
  inferences: readonly EvidenceInference[];
  unknowns: readonly EvidenceUnknown[];
}

export function validateFrozenReviewSnapshot(
  raw: unknown,
): { ok: true; value: FrozenReviewSnapshot } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: "frozen review snapshot must be a plain object" };
  }
  const o = raw as Record<string, unknown>;
  if (o.schema_version !== N7_PR_REVIEW_FREEZE_SCHEMA_VERSION) {
    return { ok: false, reason: `unsupported schema_version: ${String(o.schema_version)}` };
  }
  if (!isNonEmptyString(o.snapshot_id)) return { ok: false, reason: "snapshot_id: expected nonempty string" };

  const frozenAt = validateRfc3339UtcTimestamp(o.frozen_at);
  if (!frozenAt.ok) return { ok: false, reason: `frozen_at: ${frozenAt.reason}` };

  const repositoryRaw = o.repository;
  if (repositoryRaw === null || typeof repositoryRaw !== "object" || Array.isArray(repositoryRaw)) {
    return { ok: false, reason: "repository: expected a plain object" };
  }
  const repo = repositoryRaw as Record<string, unknown>;
  if (!isNonEmptyString(repo.owner)) return { ok: false, reason: "repository.owner: expected nonempty string" };
  if (!isNonEmptyString(repo.name)) return { ok: false, reason: "repository.name: expected nonempty string" };

  const prNumberResult = parsePositiveSafeInteger(o.pr_number, "pr_number");
  if (!prNumberResult.ok) return prNumberResult;
  if (!isNonEmptyString(o.pr_snapshot_ref)) return { ok: false, reason: "pr_snapshot_ref: expected nonempty string" };
  if (!isSha256HexOrEmpty(o.pr_snapshot_sha256)) {
    return { ok: false, reason: "pr_snapshot_sha256: expected a 64-character lowercase hex digest or empty string" };
  }
  if (!isNonEmptyGitSha(o.approved_head_sha)) {
    return { ok: false, reason: "approved_head_sha: expected a nonempty git SHA" };
  }
  if (!isGitShaOrEmpty(o.base_sha)) return { ok: false, reason: "base_sha: expected a git SHA or empty string" };

  if (!isSafeCanonicalInteger(o.changed_file_count) || (o.changed_file_count as number) < 0) {
    return { ok: false, reason: "changed_file_count: expected a nonnegative safe integer" };
  }
  if (!isSha256HexOrEmpty(o.changed_filenames_sha256)) {
    return { ok: false, reason: "changed_filenames_sha256: expected a 64-character lowercase hex digest or empty string" };
  }

  const ciSummaryRaw = o.ci_summary;
  if (ciSummaryRaw === null || typeof ciSummaryRaw !== "object" || Array.isArray(ciSummaryRaw)) {
    return { ok: false, reason: "ci_summary: expected a plain object" };
  }
  const ciSummary = ciSummaryRaw as Record<string, unknown>;
  if (!isNonEmptyString(ciSummary.state)) return { ok: false, reason: "ci_summary.state: expected nonempty string" };
  if (!isGitShaOrEmpty(ciSummary.head_sha)) {
    return { ok: false, reason: "ci_summary.head_sha: expected a git SHA or empty string" };
  }
  if (!isStringArray(ciSummary.check_identities)) {
    return { ok: false, reason: "ci_summary.check_identities: expected a string array" };
  }

  const reviewSummaryRaw = o.review_summary;
  if (reviewSummaryRaw === null || typeof reviewSummaryRaw !== "object" || Array.isArray(reviewSummaryRaw)) {
    return { ok: false, reason: "review_summary: expected a plain object" };
  }
  const reviewSummary = reviewSummaryRaw as Record<string, unknown>;
  if (!(REVIEW_DECISION_VALUES as readonly string[]).includes(reviewSummary.decision as string)) {
    return { ok: false, reason: "review_summary.decision: invalid" };
  }
  if (!isSafeCanonicalInteger(reviewSummary.requested_changes_count) || (reviewSummary.requested_changes_count as number) < 0) {
    return { ok: false, reason: "review_summary.requested_changes_count: expected a nonnegative safe integer" };
  }
  if (!isSafeCanonicalInteger(reviewSummary.unresolved_threads_count) || (reviewSummary.unresolved_threads_count as number) < 0) {
    return { ok: false, reason: "review_summary.unresolved_threads_count: expected a nonnegative safe integer" };
  }
  if (typeof reviewSummary.complete !== "boolean") {
    return { ok: false, reason: "review_summary.complete: expected boolean" };
  }

  if (!isNonEmptyString(o.mergeability)) return { ok: false, reason: "mergeability: expected nonempty string" };

  const sourceApiIdentityRaw = o.source_api_identity;
  if (sourceApiIdentityRaw === null || typeof sourceApiIdentityRaw !== "object" || Array.isArray(sourceApiIdentityRaw)) {
    return { ok: false, reason: "source_api_identity: expected a plain object" };
  }
  const sourceApiIdentity = sourceApiIdentityRaw as Record<string, unknown>;
  if (typeof sourceApiIdentity.api !== "string") return { ok: false, reason: "source_api_identity.api: expected string" };
  if (typeof sourceApiIdentity.request_id !== "string") {
    return { ok: false, reason: "source_api_identity.request_id: expected string" };
  }
  if (typeof sourceApiIdentity.etag !== "string") return { ok: false, reason: "source_api_identity.etag: expected string" };

  if (!Array.isArray(o.evidence_refs)) return { ok: false, reason: "evidence_refs: expected an array" };
  const evidenceRefs: ArtifactReference[] = [];
  for (let i = 0; i < o.evidence_refs.length; i++) {
    const r = validateArtifactReference(o.evidence_refs[i]);
    if (!r.ok) return { ok: false, reason: `evidence_refs[${i}]: ${r.reason}` };
    evidenceRefs.push(r.value);
  }

  if (!Array.isArray(o.operator_assertions)) return { ok: false, reason: "operator_assertions: expected an array" };
  const operatorAssertions: EvidenceOperatorAssertion[] = [];
  for (let i = 0; i < o.operator_assertions.length; i++) {
    const r = validateEvidenceItem(o.operator_assertions[i]);
    if (!r.ok) return { ok: false, reason: `operator_assertions[${i}]: ${r.reason}` };
    if (r.value.classification !== "OPERATOR_ASSERTED") {
      return { ok: false, reason: `operator_assertions[${i}]: expected classification OPERATOR_ASSERTED` };
    }
    operatorAssertions.push(r.value);
  }

  if (!Array.isArray(o.facts)) return { ok: false, reason: "facts: expected an array" };
  const facts: EvidenceFact[] = [];
  for (let i = 0; i < o.facts.length; i++) {
    const r = validateEvidenceItem(o.facts[i]);
    if (!r.ok) return { ok: false, reason: `facts[${i}]: ${r.reason}` };
    if (r.value.classification !== "VERIFIED") {
      return { ok: false, reason: `facts[${i}]: expected classification VERIFIED` };
    }
    facts.push(r.value);
  }

  if (!Array.isArray(o.inferences)) return { ok: false, reason: "inferences: expected an array" };
  const inferences: EvidenceInference[] = [];
  for (let i = 0; i < o.inferences.length; i++) {
    const r = validateEvidenceItem(o.inferences[i]);
    if (!r.ok) return { ok: false, reason: `inferences[${i}]: ${r.reason}` };
    if (r.value.classification !== "INFERRED") {
      return { ok: false, reason: `inferences[${i}]: expected classification INFERRED` };
    }
    inferences.push(r.value);
  }

  if (!Array.isArray(o.unknowns)) return { ok: false, reason: "unknowns: expected an array" };
  const unknowns: EvidenceUnknown[] = [];
  for (let i = 0; i < o.unknowns.length; i++) {
    const r = validateEvidenceItem(o.unknowns[i]);
    if (!r.ok) return { ok: false, reason: `unknowns[${i}]: ${r.reason}` };
    if (r.value.classification !== "UNKNOWN") {
      return { ok: false, reason: `unknowns[${i}]: expected classification UNKNOWN` };
    }
    unknowns.push(r.value);
  }

  const value: FrozenReviewSnapshot = {
    schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
    snapshot_id: o.snapshot_id as string,
    frozen_at: o.frozen_at as string,
    repository: { owner: repo.owner as string, name: repo.name as string },
    pr_number: prNumberResult.value,
    pr_snapshot_ref: o.pr_snapshot_ref as string,
    pr_snapshot_sha256: o.pr_snapshot_sha256 as string,
    approved_head_sha: o.approved_head_sha as string,
    base_sha: o.base_sha as string,
    changed_file_count: o.changed_file_count as number,
    changed_filenames_sha256: o.changed_filenames_sha256 as string,
    ci_summary: {
      state: ciSummary.state as string,
      head_sha: ciSummary.head_sha as string,
      check_identities: ciSummary.check_identities as readonly string[],
    },
    review_summary: {
      decision: reviewSummary.decision as ReviewDecision,
      requested_changes_count: reviewSummary.requested_changes_count as number,
      unresolved_threads_count: reviewSummary.unresolved_threads_count as number,
      complete: reviewSummary.complete as boolean,
    },
    mergeability: o.mergeability as string,
    source_api_identity: {
      api: sourceApiIdentity.api as string,
      request_id: sourceApiIdentity.request_id as string,
      etag: sourceApiIdentity.etag as string,
    },
    evidence_refs: evidenceRefs,
    operator_assertions: operatorAssertions,
    facts,
    inferences,
    unknowns,
  };
  return { ok: true, value };
}

// ---------------------------------------------------------------------------
// Timeline event — n7.timeline-event.v1
// ---------------------------------------------------------------------------

export type TimelineEventType =
  | "WORKSPACE_SNAPSHOT"
  | "PACKAGE_PLAN_STARTED"
  | "PACKAGE_PLAN_COMPLETED"
  | "PACKAGE_PLAN_FAILED"
  | "PACKAGE_COMMIT_PREVIEWED"
  | "PACKAGE_COMMIT_COMPLETED"
  | "PACKAGE_PUSH_COMPLETED"
  | "DRAFT_PR_CREATED"
  | "PR_LIVE_REFRESH"
  | "PR_REVIEW_FROZEN"
  | "HEAD_DRIFT_DETECTED"
  | "CI_STATE_CAPTURED"
  | "REVIEW_BLOCKER_DETECTED"
  | "MERGE_APPROVAL_CAPTURED"
  | "PR_MERGED"
  | "OPERATOR_NOTE";

export const TIMELINE_EVENT_TYPES: readonly TimelineEventType[] = [
  "WORKSPACE_SNAPSHOT",
  "PACKAGE_PLAN_STARTED",
  "PACKAGE_PLAN_COMPLETED",
  "PACKAGE_PLAN_FAILED",
  "PACKAGE_COMMIT_PREVIEWED",
  "PACKAGE_COMMIT_COMPLETED",
  "PACKAGE_PUSH_COMPLETED",
  "DRAFT_PR_CREATED",
  "PR_LIVE_REFRESH",
  "PR_REVIEW_FROZEN",
  "HEAD_DRIFT_DETECTED",
  "CI_STATE_CAPTURED",
  "REVIEW_BLOCKER_DETECTED",
  "MERGE_APPROVAL_CAPTURED",
  "PR_MERGED",
  "OPERATOR_NOTE",
];

export interface TimelineEvent {
  schema_version: typeof N7_TIMELINE_EVENT_SCHEMA_VERSION;
  event_id: string;
  sequence: number;
  previous_event_sha256: string | null;
  event_sha256: string | null;
  event_type: TimelineEventType;
  created_at: string;
  captured_by: { source: string; operator_id: string; tool_version: string };
  repository: { owner: string; name: string; remote_url_hash: string };
  workspace: { root: string; root_sha256: string; git_branch: string; git_head: string };
  pr: { number: number | null; head_sha: string | null };
  workflow_rung: string;
  operation: string;
  result: string;
  facts: readonly EvidenceFact[];
  inferences: readonly EvidenceInference[];
  unknowns: readonly EvidenceUnknown[];
  warnings: readonly string[];
  artifact_refs: readonly ArtifactReference[];
  next_permitted_action: string;
  blocking_reason: string | null;
}

export function validateTimelineEvent(raw: unknown): { ok: true; value: TimelineEvent } | { ok: false; reason: string } {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, reason: "timeline event must be a plain object" };
  }
  const o = raw as Record<string, unknown>;
  if (o.schema_version !== N7_TIMELINE_EVENT_SCHEMA_VERSION) {
    return { ok: false, reason: `unsupported schema_version: ${String(o.schema_version)}` };
  }
  if (!isNonEmptyString(o.event_id)) return { ok: false, reason: "event_id: expected nonempty string" };
  if (!isSafeCanonicalInteger(o.sequence) || (o.sequence as number) < 1) {
    return { ok: false, reason: "sequence: expected a positive safe integer" };
  }
  if (o.previous_event_sha256 !== null && !isSha256HexOrEmpty(o.previous_event_sha256)) {
    return {
      ok: false,
      reason: "previous_event_sha256: expected a 64-character lowercase hex digest, empty string, or null",
    };
  }
  if (o.event_sha256 !== null && !isSha256HexOrEmpty(o.event_sha256)) {
    return { ok: false, reason: "event_sha256: expected a 64-character lowercase hex digest, empty string, or null" };
  }
  if (!(TIMELINE_EVENT_TYPES as readonly string[]).includes(o.event_type as string)) {
    return { ok: false, reason: `event_type: unsupported value ${String(o.event_type)}` };
  }
  const createdAt = validateRfc3339UtcTimestamp(o.created_at);
  if (!createdAt.ok) return { ok: false, reason: `created_at: ${createdAt.reason}` };

  const capturedByRaw = o.captured_by;
  if (capturedByRaw === null || typeof capturedByRaw !== "object" || Array.isArray(capturedByRaw)) {
    return { ok: false, reason: "captured_by: expected a plain object" };
  }
  const cb = capturedByRaw as Record<string, unknown>;
  if (!isNonEmptyString(cb.source)) return { ok: false, reason: "captured_by.source: expected nonempty string" };
  if (!isNonEmptyString(cb.operator_id)) return { ok: false, reason: "captured_by.operator_id: expected nonempty string" };
  if (typeof cb.tool_version !== "string") return { ok: false, reason: "captured_by.tool_version: expected string" };

  const repositoryRaw = o.repository;
  if (repositoryRaw === null || typeof repositoryRaw !== "object" || Array.isArray(repositoryRaw)) {
    return { ok: false, reason: "repository: expected a plain object" };
  }
  const repo = repositoryRaw as Record<string, unknown>;
  if (!isNonEmptyString(repo.owner)) return { ok: false, reason: "repository.owner: expected nonempty string" };
  if (!isNonEmptyString(repo.name)) return { ok: false, reason: "repository.name: expected nonempty string" };
  if (typeof repo.remote_url_hash !== "string") return { ok: false, reason: "repository.remote_url_hash: expected string" };

  const workspaceRaw = o.workspace;
  if (workspaceRaw === null || typeof workspaceRaw !== "object" || Array.isArray(workspaceRaw)) {
    return { ok: false, reason: "workspace: expected a plain object" };
  }
  const workspace = workspaceRaw as Record<string, unknown>;
  if (!isNonEmptyString(workspace.root)) return { ok: false, reason: "workspace.root: expected nonempty string" };
  if (typeof workspace.root_sha256 !== "string") return { ok: false, reason: "workspace.root_sha256: expected string" };
  if (!isNonEmptyString(workspace.git_branch)) {
    return { ok: false, reason: "workspace.git_branch: expected nonempty string" };
  }
  if (!isGitShaOrEmpty(workspace.git_head)) {
    return { ok: false, reason: "workspace.git_head: expected a git SHA or empty string" };
  }

  const prRaw = o.pr;
  if (prRaw === null || typeof prRaw !== "object" || Array.isArray(prRaw)) {
    return { ok: false, reason: "pr: expected a plain object" };
  }
  const pr = prRaw as Record<string, unknown>;
  if (pr.number !== null) {
    const prNumberResult = parsePositiveSafeInteger(pr.number, "pr.number");
    if (!prNumberResult.ok) return prNumberResult;
  }
  if (pr.head_sha !== null && !isNonEmptyGitSha(pr.head_sha)) {
    return { ok: false, reason: "pr.head_sha: expected a nonempty git SHA or null" };
  }

  if (!isNonEmptyString(o.workflow_rung)) return { ok: false, reason: "workflow_rung: expected nonempty string" };
  if (!isNonEmptyString(o.operation)) return { ok: false, reason: "operation: expected nonempty string" };
  if (!isNonEmptyString(o.result)) return { ok: false, reason: "result: expected nonempty string" };

  if (!Array.isArray(o.facts)) return { ok: false, reason: "facts: expected an array" };
  const facts: EvidenceFact[] = [];
  for (let i = 0; i < o.facts.length; i++) {
    const r = validateEvidenceItem(o.facts[i]);
    if (!r.ok) return { ok: false, reason: `facts[${i}]: ${r.reason}` };
    if (r.value.classification !== "VERIFIED") {
      return { ok: false, reason: `facts[${i}]: expected classification VERIFIED` };
    }
    facts.push(r.value);
  }

  if (!Array.isArray(o.inferences)) return { ok: false, reason: "inferences: expected an array" };
  const inferences: EvidenceInference[] = [];
  for (let i = 0; i < o.inferences.length; i++) {
    const r = validateEvidenceItem(o.inferences[i]);
    if (!r.ok) return { ok: false, reason: `inferences[${i}]: ${r.reason}` };
    if (r.value.classification !== "INFERRED") {
      return { ok: false, reason: `inferences[${i}]: expected classification INFERRED` };
    }
    inferences.push(r.value);
  }

  if (!Array.isArray(o.unknowns)) return { ok: false, reason: "unknowns: expected an array" };
  const unknowns: EvidenceUnknown[] = [];
  for (let i = 0; i < o.unknowns.length; i++) {
    const r = validateEvidenceItem(o.unknowns[i]);
    if (!r.ok) return { ok: false, reason: `unknowns[${i}]: ${r.reason}` };
    if (r.value.classification !== "UNKNOWN") {
      return { ok: false, reason: `unknowns[${i}]: expected classification UNKNOWN` };
    }
    unknowns.push(r.value);
  }

  if (!isStringArray(o.warnings)) return { ok: false, reason: "warnings: expected a string array" };

  if (!Array.isArray(o.artifact_refs)) return { ok: false, reason: "artifact_refs: expected an array" };
  const artifactRefs: ArtifactReference[] = [];
  for (let i = 0; i < o.artifact_refs.length; i++) {
    const r = validateArtifactReference(o.artifact_refs[i]);
    if (!r.ok) return { ok: false, reason: `artifact_refs[${i}]: ${r.reason}` };
    artifactRefs.push(r.value);
  }

  if (!isNonEmptyString(o.next_permitted_action)) {
    return { ok: false, reason: "next_permitted_action: expected nonempty string" };
  }
  if (o.blocking_reason !== null && typeof o.blocking_reason !== "string") {
    return { ok: false, reason: "blocking_reason: expected string or null" };
  }

  const value: TimelineEvent = {
    schema_version: N7_TIMELINE_EVENT_SCHEMA_VERSION,
    event_id: o.event_id as string,
    sequence: o.sequence as number,
    previous_event_sha256: (o.previous_event_sha256 as string | null) ?? null,
    event_sha256: (o.event_sha256 as string | null) ?? null,
    event_type: o.event_type as TimelineEventType,
    created_at: o.created_at as string,
    captured_by: {
      source: cb.source as string,
      operator_id: cb.operator_id as string,
      tool_version: cb.tool_version as string,
    },
    repository: {
      owner: repo.owner as string,
      name: repo.name as string,
      remote_url_hash: repo.remote_url_hash as string,
    },
    workspace: {
      root: workspace.root as string,
      root_sha256: workspace.root_sha256 as string,
      git_branch: workspace.git_branch as string,
      git_head: workspace.git_head as string,
    },
    pr: { number: (pr.number as number | null) ?? null, head_sha: (pr.head_sha as string | null) ?? null },
    workflow_rung: o.workflow_rung as string,
    operation: o.operation as string,
    result: o.result as string,
    facts,
    inferences,
    unknowns,
    warnings: o.warnings as readonly string[],
    artifact_refs: artifactRefs,
    next_permitted_action: o.next_permitted_action as string,
    blocking_reason: (o.blocking_reason as string | null) ?? null,
  };
  return { ok: true, value };
}

// ---------------------------------------------------------------------------
// Canonical value model
// ---------------------------------------------------------------------------

// Hash-bearing N7 objects may contain only these JSON value shapes. See
// "Canonical Number Encoding": floats, NaN, +/-Infinity, -0, BigInt,
// undefined, functions, symbols, dates/class instances, and cycles are all
// rejected before any hash is computed.
export type CanonicalValue =
  | null
  | boolean
  | string
  | number
  | readonly CanonicalValue[]
  | { readonly [key: string]: CanonicalValue };

export const CANONICAL_MIN_SAFE_INTEGER = -9007199254740991;
export const CANONICAL_MAX_SAFE_INTEGER = 9007199254740991;

// Typed-value contract (honest, non-recovering): this module's canonical
// number encoding operates ONLY on already-parsed JavaScript `number`
// values, never on raw JSON source text. A safe integer value is serialized
// in minimal base-10 form (`String(n)`), and JavaScript numbers of that kind
// never round-trip through exponent notation in that range. But once a JSON
// literal like `1e3` has been parsed into a JS number, it is
// indistinguishable from the literal `1000` — `Number.isInteger(1e3)` is
// `true`, and there is no way for a function operating on the parsed value
// to know which lexical form produced it. This module therefore does NOT
// claim to detect or reject raw exponent-form JSON *source text* — that is
// a property of a JSON lexer/parser, which this module does not implement.
// If a future N7 slice ingests raw JSON text (e.g. from a file or network
// response) and must reject exponent-form numeric literals before they are
// ever parsed into JS numbers, that is a separate raw-text lexical
// validator layered in front of this typed-value contract, not a claim made
// by isSafeCanonicalInteger/canonicalizeValue below.
//
// A safe canonical integer: finite, integral, not negative zero, and inside
// the ±9007199254740991 range. This function alone decides "is this number
// permitted in a hash-bearing object" — validation always runs before any
// hash is computed; nothing is rounded, coerced, clamped, or truncated.
export function isSafeCanonicalInteger(v: unknown): v is number {
  if (typeof v !== "number") return false;
  if (!Number.isFinite(v)) return false; // rejects NaN, +Infinity, -Infinity
  if (!Number.isInteger(v)) return false; // rejects fractional values
  if (Object.is(v, -0)) return false; // rejects negative zero
  if (v < CANONICAL_MIN_SAFE_INTEGER || v > CANONICAL_MAX_SAFE_INTEGER) return false; // rejects out-of-range
  return true;
}

export class CanonicalizationError extends Error {
  readonly path: string;
  constructor(path: string, message: string) {
    super(`canonicalization failed at ${path || "<root>"}: ${message}`);
    this.path = path;
    this.name = "CanonicalizationError";
  }
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return false;
  const proto = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null;
}

// Validate an arbitrary JS value tree against the hash-bearing value
// contract and return it re-typed as CanonicalValue. Throws
// CanonicalizationError on the first disallowed value found (depth-first,
// left-to-right / key-sorted-independent order is irrelevant to *which*
// error fires first for a single-violation input; multiple violations each
// independently fail validation).
//
// Cycle detection walks the live ancestor chain (a value is only "cyclic"
// if it re-appears among its own ancestors — shared substructure that is
// NOT an ancestor of itself is legal and is visited more than once, which
// is fine since this function has no side effects across siblings).
export function canonicalizeValue(input: unknown, path = "", ancestors: Set<unknown> = new Set()): CanonicalValue {
  if (input === null) return null;
  if (typeof input === "boolean") return input;
  if (typeof input === "string") return input;
  if (typeof input === "number") {
    if (!isSafeCanonicalInteger(input)) {
      throw new CanonicalizationError(path, `disallowed numeric value: ${String(input)}`);
    }
    return input;
  }
  if (typeof input === "undefined") {
    throw new CanonicalizationError(path, "undefined is not permitted in a hash-bearing object");
  }
  if (typeof input === "bigint") {
    throw new CanonicalizationError(path, "BigInt is not permitted in a hash-bearing object");
  }
  if (typeof input === "function") {
    throw new CanonicalizationError(path, "function is not permitted in a hash-bearing object");
  }
  if (typeof input === "symbol") {
    throw new CanonicalizationError(path, "symbol is not permitted in a hash-bearing object");
  }
  if (typeof input === "object") {
    if (ancestors.has(input)) {
      throw new CanonicalizationError(path, "cyclic structure is not permitted in a hash-bearing object");
    }
    if (Array.isArray(input)) {
      const nextAncestors = new Set(ancestors);
      nextAncestors.add(input);
      const out: CanonicalValue[] = input.map((el, i) => canonicalizeValue(el, `${path}[${i}]`, nextAncestors));
      return out;
    }
    if (isPlainObject(input)) {
      const nextAncestors = new Set(ancestors);
      nextAncestors.add(input);
      const out: Record<string, CanonicalValue> = {};
      for (const key of Object.keys(input)) {
        out[key] = canonicalizeValue(input[key], `${path}.${key}`, nextAncestors);
      }
      return out;
    }
    // Date, Map, Set, RegExp, class instances, etc. — unsupported value type.
    throw new CanonicalizationError(path, "unsupported value type (only plain objects and arrays permitted)");
  }
  throw new CanonicalizationError(path, `unsupported value type: ${typeof input}`);
}

// Serialize an already-validated CanonicalValue as canonical JSON text:
// UTF-8, recursive lexicographic key ordering, array order preserved,
// minimal base-10 integers, no insignificant whitespace.
export function canonicalStringify(value: CanonicalValue): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number") {
    // isSafeCanonicalInteger was already enforced by canonicalizeValue; this
    // is a defensive re-check so canonicalStringify never silently formats
    // an invalid number even if called on a hand-built CanonicalValue.
    if (!isSafeCanonicalInteger(value)) {
      throw new CanonicalizationError("", `disallowed numeric value at stringify time: ${String(value)}`);
    }
    // String(n) for an integer within the safe range never produces an
    // exponent, leading zero, or decimal point in V8/Node. This describes
    // ONLY the shape of this function's own OUTPUT text — it makes no claim
    // about recovering how an upstream JSON source text originally wrote
    // the number (see the typed-value contract note above
    // isSafeCanonicalInteger).
    return String(value);
  }
  if (Array.isArray(value)) {
    return "[" + value.map((el) => canonicalStringify(el)).join(",") + "]";
  }
  const keys = Object.keys(value).sort();
  const parts = keys.map((k) => JSON.stringify(k) + ":" + canonicalStringify((value as Record<string, CanonicalValue>)[k]));
  return "{" + parts.join(",") + "}";
}

// Validate-then-serialize in one call: the required order is always
// "validation before hashing" — canonicalize() throws before any string is
// produced for an invalid input, so no hash is ever computed for it.
export function canonicalize(input: unknown): string {
  return canonicalStringify(canonicalizeValue(input));
}

export function sha256Hex(canonicalText: string): string {
  return createHash("sha256").update(canonicalText, "utf8").digest("hex");
}

// ---------------------------------------------------------------------------
// Hash rules
// ---------------------------------------------------------------------------

export function computeArtifactSha256(rawBytesUtf8: string): string {
  return createHash("sha256").update(rawBytesUtf8, "utf8").digest("hex");
}

export function computePrSnapshotSha256(snapshot: PrLiveSnapshot): string {
  return sha256Hex(canonicalize(snapshot));
}

// event_sha256 is omitted from the canonical object used to compute
// event_sha256 itself. previous_event_sha256 remains included.
export function timelineEventHashInput(event: TimelineEvent): Omit<TimelineEvent, "event_sha256"> {
  const { event_sha256: _omitted, ...rest } = event;
  return rest;
}

export function computeEventSha256(event: TimelineEvent): string {
  return sha256Hex(canonicalize(timelineEventHashInput(event)));
}
