import * as assert from "assert";
import {
  CanonicalizationError,
  N7_PR_LIVE_SCHEMA_VERSION,
  N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
  N7_TIMELINE_EVENT_SCHEMA_VERSION,
  PrLiveSnapshot,
  TimelineEvent,
  assertOperatorAssertionNeverPromoted,
  canonicalStringify,
  canonicalize,
  canonicalizeValue,
  computeArtifactSha256,
  computeEventSha256,
  computePrSnapshotSha256,
  isSafeCanonicalInteger,
  timelineEventHashInput,
  validateEvidenceItem,
  validateFrozenReviewSnapshot,
  validatePrLiveSnapshot,
  validateRfc3339UtcTimestamp,
  validateTimelineEvent,
} from "../src/n7Schemas";

function makePrLiveSnapshot(overrides: Partial<PrLiveSnapshot> = {}): PrLiveSnapshot {
  const base: PrLiveSnapshot = {
    schema_version: N7_PR_LIVE_SCHEMA_VERSION,
    snapshot_id: "live_20260717T175835Z_pr123_headabcdef0",
    captured_at: "2026-07-17T17:58:35Z",
    captured_by: { source: "github-reader", reader_version: "n7-reader.v1" },
    repository: { owner: "thesidestackai", name: "stack-code", url: "https://example.invalid/repo", provider: "github" },
    pr_number: 123,
    pr_url: "https://example.invalid/repo/pull/123",
    title: "Example PR",
    state: "OPEN",
    draft: true,
    base_ref: "main",
    base_sha: "bbbbbbb",
    head_ref: "docs/example",
    head_sha: "aaaaaaa",
    commit_count: 1,
    changed_file_count: 1,
    changed_files: [{ filename: "docs/example.md", status: "modified", additions: 1, deletions: 0, previous_filename: null }],
    mergeability: "MERGEABLE",
    merge_state_status: "CLEAN",
    checks: [],
    reviews: {
      review_decision: "APPROVED",
      requested_changes: [],
      unresolved_review_threads: { count: 0, complete: true, thread_refs: [] },
      blocking_automated_findings: [],
    },
    pagination: { changed_files_complete: true, checks_complete: true, review_threads_complete: true },
    source_identity: { api: "github", request_id: "req1", etag: "etag1", rate_limit_remaining: 100 },
    provenance: { head_sha: "GITHUB_LIVE" },
    unknowns: [],
  };
  return { ...base, ...overrides };
}

function makeTimelineEvent(overrides: Partial<TimelineEvent> = {}): TimelineEvent {
  const base: TimelineEvent = {
    schema_version: N7_TIMELINE_EVENT_SCHEMA_VERSION,
    event_id: "evt_20260717T175900Z_pr_review_frozen_01",
    sequence: 1,
    previous_event_sha256: null,
    event_sha256: null,
    event_type: "PR_REVIEW_FROZEN",
    created_at: "2026-07-17T17:59:00Z",
    captured_by: { source: "a2-harness-panel", operator_id: "operator:local", tool_version: "n7" },
    repository: { owner: "thesidestackai", name: "stack-code", remote_url_hash: "" },
    workspace: { root: "/example/worktree", root_sha256: "", git_branch: "docs/example", git_head: "ccccccc" },
    pr: { number: 123, head_sha: "ddddddd" },
    workflow_rung: "draft-pr-review",
    operation: "Freeze Review Evidence",
    result: "OK",
    facts: [],
    inferences: [],
    unknowns: [],
    warnings: [],
    artifact_refs: [],
    next_permitted_action: "ReviewDisposition",
    blocking_reason: null,
  };
  return { ...base, ...overrides };
}

describe("n7Schemas — canonical number encoding", () => {
  it("canonical_integer_serialization_is_minimal_base10", () => {
    assert.strictEqual(canonicalStringify(1), "1");
    assert.strictEqual(canonicalStringify(0), "0");
    assert.strictEqual(canonicalStringify(-1), "-1");
    assert.strictEqual(canonicalStringify(42), "42");
    assert.strictEqual(canonicalStringify(-42), "-42");
    // No leading zeros, no plus sign, no decimal point, no exponent.
    assert.ok(!/^0\d/.test(canonicalStringify(0)));
    assert.ok(!canonicalStringify(1).startsWith("+"));
    assert.ok(!canonicalStringify(1).includes("."));
    assert.ok(!canonicalStringify(1).toLowerCase().includes("e"));
  });

  it("floating_point_value_is_rejected_before_hashing", () => {
    assert.throws(() => canonicalizeValue({ sequence: 1.5 }), CanonicalizationError);
    assert.throws(() => canonicalizeValue(1.5), CanonicalizationError);
    assert.throws(() => canonicalizeValue(NaN), CanonicalizationError);
    assert.throws(() => canonicalizeValue(Infinity), CanonicalizationError);
    assert.throws(() => canonicalizeValue(-Infinity), CanonicalizationError);
    // No hash is ever produced: canonicalize() throws before sha256Hex runs.
    assert.throws(() => canonicalize({ duration_ms: 1.25 }), CanonicalizationError);
  });

  it("negative_zero_is_rejected", () => {
    assert.strictEqual(isSafeCanonicalInteger(-0), false);
    assert.strictEqual(isSafeCanonicalInteger(0), true);
    assert.throws(() => canonicalizeValue(-0), CanonicalizationError);
    assert.throws(() => canonicalize({ size_bytes: -0 }), CanonicalizationError);
  });

  it("integer_above_safe_range_is_rejected", () => {
    assert.strictEqual(isSafeCanonicalInteger(9007199254740991), true);
    assert.strictEqual(isSafeCanonicalInteger(-9007199254740991), true);
    assert.strictEqual(isSafeCanonicalInteger(9007199254740992), false);
    assert.strictEqual(isSafeCanonicalInteger(-9007199254740992), false);
    assert.throws(() => canonicalizeValue(9007199254740992), CanonicalizationError);
    // Not clamped or truncated: the thrown error, not a silently adjusted
    // value, is the only outcome.
    let threw = false;
    try {
      canonicalize({ sequence_number: Number.MAX_SAFE_INTEGER + 100 });
    } catch (e) {
      threw = true;
      assert.ok(e instanceof CanonicalizationError);
    }
    assert.ok(threw);
  });

  it("BigInt, undefined, functions, and symbols are rejected", () => {
    assert.throws(() => canonicalizeValue(10n), CanonicalizationError);
    assert.throws(() => canonicalizeValue(undefined), CanonicalizationError);
    assert.throws(() => canonicalizeValue(() => 1), CanonicalizationError);
    assert.throws(() => canonicalizeValue(Symbol("x")), CanonicalizationError);
  });

  it("unsupported_value_type_is_rejected", () => {
    assert.throws(() => canonicalizeValue(new Date()), CanonicalizationError);
    assert.throws(() => canonicalizeValue(new Map()), CanonicalizationError);
    class Custom {}
    assert.throws(() => canonicalizeValue(new Custom()), CanonicalizationError);
  });

  it("cyclic_object_is_rejected", () => {
    const obj: Record<string, unknown> = { a: 1 };
    obj.self = obj;
    assert.throws(() => canonicalizeValue(obj), CanonicalizationError);

    const arr: unknown[] = [1, 2];
    arr.push(arr);
    assert.throws(() => canonicalizeValue(arr), CanonicalizationError);

    // Shared (non-cyclic) substructure referenced twice is legal.
    const shared = { x: 1 };
    const notCyclic = { a: shared, b: shared };
    assert.doesNotThrow(() => canonicalizeValue(notCyclic));
  });
});

describe("n7Schemas — canonical serialization structure", () => {
  it("recursive_object_keys_are_canonical", () => {
    const a = { b: 1, a: { d: 2, c: 3 } };
    const b = { a: { c: 3, d: 2 }, b: 1 };
    assert.strictEqual(canonicalize(a), canonicalize(b));
    assert.strictEqual(canonicalize(a), '{"a":{"c":3,"d":2},"b":1}');
  });

  it("arrays preserve logical order (not sorted)", () => {
    assert.strictEqual(canonicalize([3, 1, 2]), "[3,1,2]");
    assert.notStrictEqual(canonicalize([3, 1, 2]), canonicalize([1, 2, 3]));
  });

  it("has no insignificant whitespace", () => {
    const text = canonicalize({ a: 1, b: [1, 2] });
    assert.ok(!/\s/.test(text));
  });
});

describe("n7Schemas — hash rules", () => {
  it("event_hash_omits_only_event_sha256", () => {
    const event = makeTimelineEvent({ previous_event_sha256: "prevhash123", event_sha256: "stalehash" });
    const hashInput = timelineEventHashInput(event);
    assert.ok(!("event_sha256" in hashInput));
    assert.strictEqual((hashInput as unknown as { previous_event_sha256: string }).previous_event_sha256, "prevhash123");

    const hash1 = computeEventSha256(event);
    // Changing only the stale event_sha256 field must not change the
    // recomputed hash (it is excluded from the hash input).
    const eventWithDifferentStaleHash = { ...event, event_sha256: "totally-different-stale-value" };
    const hash2 = computeEventSha256(eventWithDifferentStaleHash);
    assert.strictEqual(hash1, hash2);

    // Changing previous_event_sha256 DOES change the recomputed hash.
    const eventWithDifferentPrev = { ...event, previous_event_sha256: "some-other-prev-hash" };
    const hash3 = computeEventSha256(eventWithDifferentPrev);
    assert.notStrictEqual(hash1, hash3);
  });

  it("changing a fact changes the hash", () => {
    const s1 = makePrLiveSnapshot({ head_sha: "aaa111" });
    const s2 = makePrLiveSnapshot({ head_sha: "bbb222" });
    assert.notStrictEqual(computePrSnapshotSha256(s1), computePrSnapshotSha256(s2));
  });

  it("stable sorted-key hashes are unaffected by input key order", () => {
    const s1 = makePrLiveSnapshot();
    const reordered = JSON.parse(JSON.stringify(s1));
    // Rebuild with reversed key insertion order at the top level.
    const reversedKeys = Object.keys(reordered).reverse();
    const rebuilt: Record<string, unknown> = {};
    for (const k of reversedKeys) rebuilt[k] = reordered[k];
    assert.strictEqual(canonicalize(rebuilt), canonicalize(s1));
  });

  it("computeArtifactSha256 is deterministic and content-sensitive", () => {
    const h1 = computeArtifactSha256("hello world");
    const h2 = computeArtifactSha256("hello world");
    const h3 = computeArtifactSha256("hello world!");
    assert.strictEqual(h1, h2);
    assert.notStrictEqual(h1, h3);
    assert.strictEqual(h1.length, 64);
  });
});

describe("n7Schemas — schema validators", () => {
  it("accepts a complete pr-live.v1 snapshot", () => {
    const res = validatePrLiveSnapshot(makePrLiveSnapshot());
    assert.strictEqual(res.ok, true);
  });

  it("rejects unknown schema versions", () => {
    const bad = { ...makePrLiveSnapshot(), schema_version: "n7.pr-live.v2" };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);

    const badFrozen = validateFrozenReviewSnapshot({ schema_version: "not-a-real-version" });
    assert.strictEqual(badFrozen.ok, false);

    const badEvent = validateTimelineEvent({ schema_version: "not-a-real-version" });
    assert.strictEqual(badEvent.ok, false);
  });

  it("rejects missing required IDs", () => {
    const noSnapshotId = { ...makePrLiveSnapshot() } as Record<string, unknown>;
    delete noSnapshotId.snapshot_id;
    assert.strictEqual(validatePrLiveSnapshot(noSnapshotId).ok, false);

    const noEventId = { ...makeTimelineEvent() } as Record<string, unknown>;
    delete noEventId.event_id;
    assert.strictEqual(validateTimelineEvent(noEventId).ok, false);
  });

  it("rejects invalid provenance enum", () => {
    const bad = { ...makePrLiveSnapshot(), provenance: { head_sha: "NOT_A_REAL_PROVENANCE" } };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
  });

  it("accepts a complete frozen review snapshot", () => {
    const frozen = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_pr123_headabcdef0_20260717T175900Z",
      frozen_at: "2026-07-17T17:59:00Z",
      repository: { owner: "thesidestackai", name: "stack-code" },
      pr_number: 123,
      pr_snapshot_ref: "artifact_pr_live_snapshot",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "fffffff",
      changed_file_count: 1,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "eeeeeee", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "github", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      facts: [],
      inferences: [],
      unknowns: [],
    };
    assert.strictEqual(validateFrozenReviewSnapshot(frozen).ok, true);
  });

  it("accepts a complete timeline event", () => {
    assert.strictEqual(validateTimelineEvent(makeTimelineEvent()).ok, true);
  });

  it("rejects an unsupported timeline event_type", () => {
    const bad = { ...makeTimelineEvent(), event_type: "SOMETHING_MADE_UP" };
    assert.strictEqual(validateTimelineEvent(bad).ok, false);
  });
});

describe("n7Schemas — evidence classification", () => {
  it("accepts a VERIFIED fact with recorded provenance", () => {
    const res = validateEvidenceItem({
      id: "fact_1",
      classification: "VERIFIED",
      statement: "PR #123 current head is abc123",
      provenance: "GITHUB_LIVE",
      source_event_id: "evt_1",
      source_artifact_id: null,
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(res.ok, true);
  });

  it("missing_source_is_unknown: a fact with no recorded provenance cannot be VERIFIED", () => {
    const res = validateEvidenceItem({
      id: "fact_2",
      classification: "VERIFIED",
      statement: "some claim",
      // provenance intentionally omitted (no recorded source)
      source_event_id: null,
      source_artifact_id: null,
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(res.ok, false);

    // The honest representation of "no recorded source" is an UNKNOWN item.
    const unknownRes = validateEvidenceItem({
      id: "unk_1",
      classification: "UNKNOWN",
      statement: "some claim",
      provenance: "UNKNOWN_NOT_CHECKED",
      reason: "no source was captured",
      blocks: ["review_clean"],
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(unknownRes.ok, true);
  });

  it("inference_requires_supporting_fact_references", () => {
    const noSupports = validateEvidenceItem({
      id: "inf_1",
      classification: "INFERRED",
      statement: "HEAD_DRIFT because heads differ",
      provenance: "DERIVED_COMPARISON",
      supports: [],
      rule: "current_head_sha != frozen_reviewed_head_sha => HEAD_DRIFT",
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(noSupports.ok, false);

    const withSupports = validateEvidenceItem({
      id: "inf_2",
      classification: "INFERRED",
      statement: "HEAD_DRIFT because heads differ",
      provenance: "DERIVED_COMPARISON",
      supports: ["fact_live_head", "fact_frozen_head"],
      rule: "current_head_sha != frozen_reviewed_head_sha => HEAD_DRIFT",
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(withSupports.ok, true);
  });

  it("operator_assertion_never_promotes_to_verified", () => {
    const res = validateEvidenceItem({
      id: "assert_1",
      classification: "OPERATOR_ASSERTED",
      statement: "Reviewed by operator in browser",
      provenance: "OPERATOR_ASSERTION",
      asserted_by: "operator:local",
      assertion_kind: "review_note",
      promote_to_verified: false,
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(res.ok, true);
    if (res.ok && res.value.classification === "OPERATOR_ASSERTED") {
      const item = res.value;
      assert.strictEqual(item.promote_to_verified, false);
      assert.doesNotThrow(() => assertOperatorAssertionNeverPromoted(item));
    }

    // An attempt to set promote_to_verified: true is refused by the
    // validator, not silently accepted.
    const attemptedPromotion = validateEvidenceItem({
      id: "assert_2",
      classification: "OPERATOR_ASSERTED",
      statement: "Reviewed by operator in browser",
      provenance: "OPERATOR_ASSERTION",
      asserted_by: "operator:local",
      assertion_kind: "review_note",
      promote_to_verified: true,
      captured_at: "2026-07-17T17:58:35Z",
    });
    assert.strictEqual(attemptedPromotion.ok, false);
  });
});

describe("n7Schemas — strict RFC3339 UTC Z timestamp validation", () => {
  it("rfc3339_utc_z_accepts_whole_seconds", () => {
    const r = validateRfc3339UtcTimestamp("2026-07-20T10:00:00Z");
    assert.strictEqual(r.ok, true);
  });

  it("rfc3339_utc_z_accepts_fractional_seconds", () => {
    const r = validateRfc3339UtcTimestamp("2026-07-20T10:00:00.123Z");
    assert.strictEqual(r.ok, true);
    if (r.ok) {
      const whole = validateRfc3339UtcTimestamp("2026-07-20T10:00:00Z");
      assert.ok(whole.ok);
      if (whole.ok) {
        assert.strictEqual(r.epochMs, whole.epochMs + 123);
      }
    }
  });

  it("rfc3339_utc_z_rejects_offset", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20T10:00:00+00:00").ok, false);
  });

  it("rfc3339_utc_z_rejects_space_separator", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20 10:00:00Z").ok, false);
  });

  it("rfc3339_utc_z_rejects_missing_z", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20T10:00:00").ok, false);
  });

  it("rfc3339_utc_z_rejects_impossible_date", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-02-30T10:00:00Z").ok, false);
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-13-01T10:00:00Z").ok, false);
  });

  it("rfc3339_utc_z_rejects_invalid_time", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20T25:00:00Z").ok, false);
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20T10:60:00Z").ok, false);
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20T10:00:60Z").ok, false);
  });

  it("rejects a non-date string and a date-only value", () => {
    assert.strictEqual(validateRfc3339UtcTimestamp("not-a-date").ok, false);
    assert.strictEqual(validateRfc3339UtcTimestamp("2026-07-20").ok, false);
  });

  it("every schema timestamp field is validated with the strict contract", () => {
    const badCapturedAt = { ...makePrLiveSnapshot(), captured_at: "2026-07-20 10:00:00Z" };
    assert.strictEqual(validatePrLiveSnapshot(badCapturedAt).ok, false);

    const badFrozenAt = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_1",
      frozen_at: "2026-07-20T10:00:00+00:00",
      repository: { owner: "o", name: "n" },
      pr_number: 1,
      pr_snapshot_ref: "ref",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "",
      changed_file_count: 0,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      facts: [],
      inferences: [],
      unknowns: [],
    };
    assert.strictEqual(validateFrozenReviewSnapshot(badFrozenAt).ok, false);

    const badCreatedAt = { ...makeTimelineEvent(), created_at: "not-a-date" };
    assert.strictEqual(validateTimelineEvent(badCreatedAt).ok, false);
  });
});

describe("n7Schemas — honest canonicalization typed-value contract", () => {
  it("documents (rather than falsely claims to reject) that exponent-form and decimal-form numeric literals are indistinguishable once parsed", () => {
    // 1e3 and 1000 are the SAME JavaScript number once JSON.parse (or any
    // JS literal) has produced it. This module operates only on already-
    // parsed values, so it canonicalizes both identically — it does NOT,
    // and cannot, detect that one might have been written as "1e3" in some
    // upstream JSON source text. See the typed-value contract comment in
    // n7Schemas.ts above isSafeCanonicalInteger.
    // eslint-disable-next-line no-loss-of-precision
    const fromExponentLiteral = 1e3;
    const fromDecimalLiteral = 1000;
    assert.strictEqual(fromExponentLiteral, fromDecimalLiteral);
    assert.strictEqual(canonicalStringify(fromExponentLiteral), canonicalStringify(fromDecimalLiteral));
    assert.strictEqual(canonicalStringify(fromExponentLiteral), "1000");
  });
});

describe("n7Schemas — deep validation is fail-closed with no silent defaults", () => {
  it("live_snapshot_rejects_missing_captured_by", () => {
    const bad = { ...makePrLiveSnapshot() } as Record<string, unknown>;
    delete bad.captured_by;
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("captured_by"));
  });

  it("live_snapshot_rejects_malformed_repository", () => {
    const bad = { ...makePrLiveSnapshot(), repository: { owner: "", name: "stack-code", url: "u", provider: "github" } };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("repository.owner"));
  });

  it("live_snapshot_rejects_invalid_changed_file_item", () => {
    const bad = {
      ...makePrLiveSnapshot(),
      changed_files: [{ filename: "a.md", status: "modified", additions: -1, deletions: 0, previous_filename: null }],
    };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.includes("changed_files[0]"));
  });

  it("live_snapshot_rejects_invalid_check_item", () => {
    const bad = {
      ...makePrLiveSnapshot(),
      checks: [
        {
          provider: "github",
          name: "test",
          app: "github-actions",
          status: "NOT_A_REAL_STATUS",
          conclusion: null,
          head_sha: "",
          started_at: null,
          completed_at: null,
          details_url: null,
          provenance: "GITHUB_LIVE",
        },
      ],
    };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.includes("checks[0]"));
  });

  it("live_snapshot_rejects_incomplete_review_pagination (malformed, not merely false)", () => {
    const bad = {
      ...makePrLiveSnapshot(),
      pagination: { changed_files_complete: true, checks_complete: true, review_threads_complete: "not-a-boolean" },
    };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("pagination.review_threads_complete"));
  });

  it("live_snapshot_rejects_malformed_source_identity", () => {
    const bad = { ...makePrLiveSnapshot(), source_identity: { api: "github", request_id: "", etag: "", rate_limit_remaining: 1.5 } };
    const res = validatePrLiveSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("source_identity.rate_limit_remaining"));
  });

  it("live_snapshot_rejects_a_head_sha_that_is_not_valid_hex", () => {
    const bad = { ...makePrLiveSnapshot(), head_sha: "not-hex!" };
    assert.strictEqual(validatePrLiveSnapshot(bad).ok, false);
  });

  it("frozen_snapshot_rejects_malformed_fact", () => {
    const bad = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_1",
      frozen_at: "2026-07-20T10:00:00Z",
      repository: { owner: "o", name: "n" },
      pr_number: 1,
      pr_snapshot_ref: "ref",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "",
      changed_file_count: 0,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      facts: [{ id: "f1", classification: "VERIFIED" /* missing statement/provenance/captured_at */ }],
      inferences: [],
      unknowns: [],
    };
    const res = validateFrozenReviewSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("facts[0]"));
  });

  it("frozen_snapshot_rejects_inference_without_supports", () => {
    const bad = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_1",
      frozen_at: "2026-07-20T10:00:00Z",
      repository: { owner: "o", name: "n" },
      pr_number: 1,
      pr_snapshot_ref: "ref",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "",
      changed_file_count: 0,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      facts: [],
      inferences: [
        {
          id: "i1",
          classification: "INFERRED",
          statement: "s",
          provenance: "DERIVED_COMPARISON",
          supports: [],
          rule: "r",
          captured_at: "2026-07-20T10:00:00Z",
        },
      ],
      unknowns: [],
    };
    const res = validateFrozenReviewSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("inferences[0]"));
  });

  it("frozen_snapshot_rejects_malformed_unknown", () => {
    const bad = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_1",
      frozen_at: "2026-07-20T10:00:00Z",
      repository: { owner: "o", name: "n" },
      pr_number: 1,
      pr_snapshot_ref: "ref",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "",
      changed_file_count: 0,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      facts: [],
      inferences: [],
      unknowns: [{ id: "u1", classification: "UNKNOWN", statement: "s" /* missing reason/blocks/captured_at/provenance */ }],
    };
    const res = validateFrozenReviewSnapshot(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("unknowns[0]"));
  });

  it("frozen_snapshot_does_not_default_missing_arrays", () => {
    const bad = {
      schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
      snapshot_id: "freeze_1",
      frozen_at: "2026-07-20T10:00:00Z",
      repository: { owner: "o", name: "n" },
      pr_number: 1,
      pr_snapshot_ref: "ref",
      pr_snapshot_sha256: "",
      approved_head_sha: "eeeeeee",
      base_sha: "",
      changed_file_count: 0,
      changed_filenames_sha256: "",
      ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
      review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
      mergeability: "MERGEABLE",
      source_api_identity: { api: "", request_id: "", etag: "" },
      evidence_refs: [],
      operator_assertions: [],
      // facts / inferences / unknowns intentionally omitted entirely
    };
    const res = validateFrozenReviewSnapshot(bad);
    assert.strictEqual(res.ok, false, "a missing required array must be rejected, not silently defaulted to []");
  });

  it("timeline_event_rejects_malformed_artifact_reference", () => {
    const bad = {
      ...makeTimelineEvent(),
      artifact_refs: [{ artifact_id: "a1", kind: "pr-live-snapshot", path: "p", sha256: "not-64-hex", size_bytes: 0, redaction: "no-secrets" }],
    };
    const res = validateTimelineEvent(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("artifact_refs[0]"));
  });

  it("timeline_event_rejects_invalid_nested_fact", () => {
    const bad = {
      ...makeTimelineEvent(),
      facts: [{ id: "f1", classification: "VERIFIED", statement: "s", provenance: "GITHUB_LIVE" /* missing captured_at */ }],
    };
    const res = validateTimelineEvent(bad);
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("facts[0]"));
  });

  it("timeline_event_rejects_missing_required_identity", () => {
    const bad = { ...makeTimelineEvent() } as Record<string, unknown>;
    delete bad.repository;
    assert.strictEqual(validateTimelineEvent(bad).ok, false);

    const bad2 = { ...makeTimelineEvent() } as Record<string, unknown>;
    delete bad2.workspace;
    assert.strictEqual(validateTimelineEvent(bad2).ok, false);
  });

  it("timeline_event_does_not_default_missing_nested_values", () => {
    const bad = { ...makeTimelineEvent() } as Record<string, unknown>;
    delete bad.captured_by;
    const res = validateTimelineEvent(bad);
    assert.strictEqual(res.ok, false, "a missing required nested object must be rejected, not silently defaulted");
  });
});

function makeFrozenReviewSnapshot(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const base: Record<string, unknown> = {
    schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
    snapshot_id: "freeze_1",
    frozen_at: "2026-07-20T10:00:00Z",
    repository: { owner: "thesidestackai", name: "stack-code" },
    pr_number: 123,
    pr_snapshot_ref: "artifact_pr_live_snapshot",
    pr_snapshot_sha256: "",
    approved_head_sha: "eeeeeee",
    base_sha: "",
    changed_file_count: 0,
    changed_filenames_sha256: "",
    ci_summary: { state: "SUCCESS", head_sha: "", check_identities: [] },
    review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
    mergeability: "MERGEABLE",
    source_api_identity: { api: "", request_id: "", etag: "" },
    evidence_refs: [],
    operator_assertions: [],
    facts: [],
    inferences: [],
    unknowns: [],
  };
  return { ...base, ...overrides };
}

describe("n7Schemas — positive PR-number identity", () => {
  it("live_snapshot_rejects_zero_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: 0 });
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.strictEqual(res.reason, "pr_number: expected positive safe integer");
  });

  it("live_snapshot_rejects_negative_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: -1 });
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.strictEqual(res.reason, "pr_number: expected positive safe integer");
  });

  it("live_snapshot_rejects_fractional_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: 1.5 });
    assert.strictEqual(res.ok, false);
  });

  it("live_snapshot_rejects_unsafe_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: Number.MAX_SAFE_INTEGER + 100 });
    assert.strictEqual(res.ok, false);
  });

  it("live_snapshot_rejects_numeric_string_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: "123" });
    assert.strictEqual(res.ok, false);
  });

  it("live_snapshot_accepts_a_positive_pr_number", () => {
    const res = validatePrLiveSnapshot({ ...makePrLiveSnapshot(), pr_number: 1 });
    assert.strictEqual(res.ok, true);
  });

  it("frozen_snapshot_rejects_zero_pr_number", () => {
    const res = validateFrozenReviewSnapshot(makeFrozenReviewSnapshot({ pr_number: 0 }));
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.strictEqual(res.reason, "pr_number: expected positive safe integer");
  });

  it("frozen_snapshot_rejects_negative_pr_number", () => {
    const res = validateFrozenReviewSnapshot(makeFrozenReviewSnapshot({ pr_number: -5 }));
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.strictEqual(res.reason, "pr_number: expected positive safe integer");
  });

  it("timeline_event_rejects_nonpositive_pr_number_when_pr_identity_present", () => {
    const zero = validateTimelineEvent({ ...makeTimelineEvent(), pr: { number: 0, head_sha: "ddddddd" } });
    assert.strictEqual(zero.ok, false);
    if (!zero.ok) assert.strictEqual(zero.reason, "pr.number: expected positive safe integer");

    const negative = validateTimelineEvent({ ...makeTimelineEvent(), pr: { number: -1, head_sha: "ddddddd" } });
    assert.strictEqual(negative.ok, false);
  });

  it("timeline_event_still_accepts_null_pr_number (no PR identity present)", () => {
    const res = validateTimelineEvent({ ...makeTimelineEvent(), pr: { number: null, head_sha: null } });
    assert.strictEqual(res.ok, true);
  });
});

describe("n7Schemas — bounded numeric and count-coherence invariants", () => {
  it("count_fields_reject_negative_values", () => {
    assert.strictEqual(validatePrLiveSnapshot({ ...makePrLiveSnapshot(), commit_count: -1 }).ok, false);
    assert.strictEqual(
      validatePrLiveSnapshot({
        ...makePrLiveSnapshot(),
        changed_files: [{ filename: "a.md", status: "modified", additions: -1, deletions: 0, previous_filename: null }],
      }).ok,
      false,
    );
    assert.strictEqual(
      validatePrLiveSnapshot({
        ...makePrLiveSnapshot(),
        reviews: {
          review_decision: "APPROVED",
          requested_changes: [],
          unresolved_review_threads: { count: -1, complete: false, thread_refs: [] },
          blocking_automated_findings: [],
        },
      }).ok,
      false,
    );
  });

  it("complete_changed_files_require_matching_count", () => {
    const res = validatePrLiveSnapshot({
      ...makePrLiveSnapshot(),
      changed_file_count: 5, // does not match changed_files.length (1)
      pagination: { changed_files_complete: true, checks_complete: true, review_threads_complete: true },
    });
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("changed_file_count"));
  });

  it("incomplete_changed_files_allow_total_larger_than_captured_array", () => {
    const res = validatePrLiveSnapshot({
      ...makePrLiveSnapshot(),
      changed_file_count: 50, // more files exist than were paged in locally
      pagination: { changed_files_complete: false, checks_complete: true, review_threads_complete: true },
    });
    assert.strictEqual(res.ok, true);
  });

  it("complete_review_threads_require_matching_count", () => {
    const res = validatePrLiveSnapshot({
      ...makePrLiveSnapshot(),
      reviews: {
        review_decision: "APPROVED",
        requested_changes: [],
        unresolved_review_threads: { count: 3, complete: true, thread_refs: ["t1"] }, // 3 != 1
        blocking_automated_findings: [],
      },
    });
    assert.strictEqual(res.ok, false);
    if (!res.ok) assert.ok(res.reason.startsWith("reviews.unresolved_review_threads.count"));
  });

  it("incomplete review threads allow a declared count larger than the captured thread_refs", () => {
    const res = validatePrLiveSnapshot({
      ...makePrLiveSnapshot(),
      reviews: {
        review_decision: "APPROVED",
        requested_changes: [],
        unresolved_review_threads: { count: 3, complete: false, thread_refs: ["t1"] },
        blocking_automated_findings: [],
      },
    });
    assert.strictEqual(res.ok, true);
  });
});
