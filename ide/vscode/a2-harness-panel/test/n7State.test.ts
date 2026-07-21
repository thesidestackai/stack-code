import * as assert from "assert";
import { FrozenReviewSnapshot, N7_PR_LIVE_SCHEMA_VERSION, N7_PR_REVIEW_FREEZE_SCHEMA_VERSION, PrLiveSnapshot } from "../src/n7Schemas";
import {
  CiRequirementPolicy,
  N7DerivationInput,
  N7_NEXT_ACTIONS,
  N7_PRIMARY_STATES,
  assertNextActionNeverAuthorizesWrite,
  deriveLiveFrozenComparison,
  deriveN7PrimaryState,
  deriveNextPermittedAction,
  severityForPrimaryState,
} from "../src/n7State";

const NOW = "2026-07-17T18:00:00Z";
const FRESH_MS = 15 * 60 * 1000; // 15 minutes

function makeLive(overrides: Partial<PrLiveSnapshot> = {}): PrLiveSnapshot {
  const base: PrLiveSnapshot = {
    schema_version: N7_PR_LIVE_SCHEMA_VERSION,
    snapshot_id: "live_1",
    captured_at: "2026-07-17T17:58:35Z",
    captured_by: { source: "github-reader", reader_version: "n7-reader.v1" },
    repository: { owner: "thesidestackai", name: "stack-code", url: "https://example.invalid/repo", provider: "github" },
    pr_number: 123,
    pr_url: "https://example.invalid/repo/pull/123",
    title: "Example PR",
    state: "OPEN",
    draft: false,
    base_ref: "main",
    base_sha: "base0001",
    head_ref: "docs/example",
    head_sha: "head0001",
    commit_count: 1,
    changed_file_count: 1,
    changed_files: [{ filename: "docs/example.md", status: "modified", additions: 1, deletions: 0, previous_filename: null }],
    mergeability: "MERGEABLE",
    merge_state_status: "CLEAN",
    checks: [
      {
        provider: "github",
        name: "test",
        app: "github-actions",
        status: "COMPLETED",
        conclusion: "SUCCESS",
        head_sha: "head0001",
        started_at: null,
        completed_at: null,
        details_url: null,
        provenance: "GITHUB_LIVE",
      },
    ],
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

function makeFrozen(overrides: Partial<FrozenReviewSnapshot> = {}): FrozenReviewSnapshot {
  const base: FrozenReviewSnapshot = {
    schema_version: N7_PR_REVIEW_FREEZE_SCHEMA_VERSION,
    snapshot_id: "freeze_1",
    frozen_at: "2026-07-17T17:59:00Z",
    repository: { owner: "thesidestackai", name: "stack-code" },
    pr_number: 123,
    pr_snapshot_ref: "artifact_pr_live_snapshot",
    pr_snapshot_sha256: "abc",
    approved_head_sha: "head0001",
    base_sha: "base0001",
    changed_file_count: 1,
    changed_filenames_sha256: "def",
    ci_summary: { state: "SUCCESS", head_sha: "head0001", check_identities: [] },
    review_summary: { decision: "APPROVED", requested_changes_count: 0, unresolved_threads_count: 0, complete: true },
    mergeability: "MERGEABLE",
    source_api_identity: { api: "github", request_id: "", etag: "" },
    evidence_refs: [],
    operator_assertions: [],
    facts: [],
    inferences: [],
    unknowns: [],
  };
  return { ...base, ...overrides };
}

// Matches makeLive()'s default single check (name "test", head_sha
// "head0001", COMPLETED/SUCCESS) so existing non-CI-focused tests keep
// resolving CI as SUCCESS without needing to think about CI policy. Tests
// that specifically exercise CI-policy semantics override this explicitly.
const REQUIRED_TEST_CHECK_POLICY: CiRequirementPolicy = { kind: "REQUIRED", requiredCheckNames: ["test"] };

function baseInput(overrides: Partial<N7DerivationInput> = {}): N7DerivationInput {
  return {
    prIdentityKnown: true,
    live: makeLive(),
    liveFetchFailed: false,
    frozen: makeFrozen(),
    ciRequirementPolicy: REQUIRED_TEST_CHECK_POLICY,
    evidenceChainIntegrity: "NOT_CHECKED",
    nowIso: NOW,
    freshnessThresholdMs: FRESH_MS,
    ...overrides,
  };
}

describe("n7State — exactly 18 primary states, no nineteenth state", () => {
  it("exports exactly the required 18 distinct states", () => {
    const required = [
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
    assert.strictEqual(N7_PRIMARY_STATES.length, 18);
    const unique = new Set(N7_PRIMARY_STATES);
    assert.strictEqual(unique.size, 18);
    for (const s of required) {
      assert.ok(N7_PRIMARY_STATES.includes(s as never), `missing state: ${s}`);
    }
  });
});

describe("n7State — pure live-vs-frozen comparison", () => {
  it("current_head_equal_to_frozen_head_is_match", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({ head_sha: "abc" }),
      frozen: makeFrozen({ approved_head_sha: "abc" }),
      ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.strictEqual(cmp.headComparison, "MATCH");
  });

  it("missing frozen head yields unknown, not match", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({ head_sha: "abc" }),
      frozen: null,
      ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.strictEqual(cmp.headComparison, "UNKNOWN");
  });

  it("base_change_produces_base_drift", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({ base_sha: "new-base" }),
      frozen: makeFrozen({ base_sha: "old-base" }),
      ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.strictEqual(cmp.baseComparison, "DRIFT");
  });

  it("green_ci_for_old_head_does_not_clear_current_head", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({
        head_sha: "newhead",
        checks: [
          {
            provider: "github",
            name: "test",
            app: "github-actions",
            status: "COMPLETED",
            conclusion: "SUCCESS",
            head_sha: "oldhead", // correlates to a stale head, not current
            started_at: null,
            completed_at: null,
            details_url: null,
            provenance: "GITHUB_LIVE",
          },
        ],
      }),
      frozen: null,
      // REQUIRED so the "test" check's old-head correlation is actually
      // exercised: a check for an old head must not count toward the
      // required check, so it is reported missing (PENDING), not silently
      // treated as proof of success.
      ciRequirementPolicy: REQUIRED_TEST_CHECK_POLICY,
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.notStrictEqual(cmp.ci, "SUCCESS");
    assert.strictEqual(cmp.ci, "PENDING");
  });

  it("missing_review_page_is_not_reported_as_no_blockers", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({ pagination: { changed_files_complete: true, checks_complete: true, review_threads_complete: false } }),
      frozen: null,
      ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.notStrictEqual(cmp.review, "CLEAN");
  });

  it("unresolved thread produces a blocked review comparison", () => {
    const cmp = deriveLiveFrozenComparison({
      live: makeLive({
        reviews: {
          review_decision: "APPROVED",
          requested_changes: [],
          unresolved_review_threads: { count: 2, complete: true, thread_refs: ["t1", "t2"] },
          blocking_automated_findings: [],
        },
      }),
      frozen: null,
      ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      nowIso: NOW,
      freshnessThresholdMs: FRESH_MS,
    });
    assert.strictEqual(cmp.review, "BLOCKED");
  });

  it("does not mutate its inputs", () => {
    const live = Object.freeze(makeLive());
    const frozen = Object.freeze(makeFrozen());
    assert.doesNotThrow(() =>
      deriveLiveFrozenComparison({
        live,
        frozen,
        ciRequirementPolicy: REQUIRED_TEST_CHECK_POLICY,
        nowIso: NOW,
        freshnessThresholdMs: FRESH_MS,
      }),
    );
  });
});

describe("n7State — 18-state precedence and safety relationships", () => {
  it("current_head_change_invalidates_prior_merge_approval (HEAD_DRIFT)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ head_sha: "newhead", draft: false }),
        frozen: makeFrozen({ approved_head_sha: "oldhead" }),
      }),
    );
    assert.strictEqual(out.primaryState, "HEAD_DRIFT");
  });

  it("head_drift_outranks_ci_success", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          head_sha: "newhead",
          draft: false,
          // CI genuinely succeeds for the *current* head — proving drift
          // still outranks a real green CI, not merely an uncorrelated one.
          checks: [
            {
              provider: "github",
              name: "test",
              app: "github-actions",
              status: "COMPLETED",
              conclusion: "SUCCESS",
              head_sha: "newhead",
              started_at: null,
              completed_at: null,
              details_url: null,
              provenance: "GITHUB_LIVE",
            },
          ],
        }),
        frozen: makeFrozen({ approved_head_sha: "oldhead" }),
      }),
    );
    assert.strictEqual(out.primaryState, "HEAD_DRIFT");
    assert.strictEqual(out.comparison.ci, "SUCCESS");
  });

  it("base_drift outranks a matching head with clean CI/review", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ head_sha: "head0001", base_sha: "newbase", draft: false }),
        frozen: makeFrozen({ approved_head_sha: "head0001", base_sha: "oldbase" }),
      }),
    );
    assert.strictEqual(out.primaryState, "BASE_DRIFT");
  });

  it("merge_conflict_outranks_green_ci", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ mergeability: "CONFLICTING", draft: false }),
      }),
    );
    assert.strictEqual(out.primaryState, "MERGE_CONFLICT");
  });

  it("review_blocked outranks ready-clean", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          reviews: {
            review_decision: "CHANGES_REQUESTED",
            requested_changes: ["reviewer1"],
            unresolved_review_threads: { count: 0, complete: true, thread_refs: [] },
            blocking_automated_findings: [],
          },
        }),
      }),
    );
    assert.strictEqual(out.primaryState, "REVIEW_BLOCKED");
  });

  it("requested_changes_block_ready_clean", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          reviews: {
            review_decision: "APPROVED",
            requested_changes: ["reviewer1"],
            unresolved_review_threads: { count: 0, complete: true, thread_refs: [] },
            blocking_automated_findings: [],
          },
        }),
      }),
    );
    assert.strictEqual(out.primaryState, "REVIEW_BLOCKED");
  });

  it("CI failure/pending can never become clean (CI_FAILED)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          checks: [
            {
              provider: "github",
              name: "test",
              app: "github-actions",
              status: "COMPLETED",
              conclusion: "FAILURE",
              head_sha: "head0001",
              started_at: null,
              completed_at: null,
              details_url: null,
              provenance: "GITHUB_LIVE",
            },
          ],
        }),
      }),
    );
    assert.strictEqual(out.primaryState, "CI_FAILED");
  });

  it("CI failure/pending can never become clean (CI_PENDING)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          checks: [
            {
              provider: "github",
              name: "test",
              app: "github-actions",
              status: "IN_PROGRESS",
              conclusion: null,
              head_sha: "head0001",
              started_at: null,
              completed_at: null,
              details_url: null,
              provenance: "GITHUB_LIVE",
            },
          ],
        }),
      }),
    );
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("unknown_mergeability_blocks_merge_guidance (READY_BLOCKED)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ mergeability: "UNKNOWN", draft: false }),
      }),
    );
    assert.strictEqual(out.primaryState, "READY_BLOCKED");
  });

  it("unknown_mergeability_blocks_merge_guidance (DRAFT_BLOCKED)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ mergeability: "UNKNOWN", draft: true }),
        frozen: null,
      }),
    );
    assert.strictEqual(out.primaryState, "DRAFT_BLOCKED");
  });

  it("partial data (incomplete changed-files pagination) cannot become clean", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, pagination: { changed_files_complete: false, checks_complete: true, review_threads_complete: true } }),
      }),
    );
    assert.strictEqual(out.primaryState, "READY_BLOCKED");
  });

  it("DRAFT_CLEAN permits no freeze yet", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: true }),
        frozen: null,
      }),
    );
    assert.strictEqual(out.primaryState, "DRAFT_CLEAN");
  });

  it("READY_CLEAN requires a matching freeze plus proven CI success", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001" }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
      }),
    );
    assert.strictEqual(out.primaryState, "READY_CLEAN");
  });

  it("READY_BLOCKED when non-draft PR has no matching freeze at all", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false }),
        frozen: null,
      }),
    );
    assert.strictEqual(out.primaryState, "READY_BLOCKED");
  });

  it("FROZEN_MATCH when head matches and nothing is blocked under an explicit verified NOT_REQUIRED CI policy", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001", checks: [] }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      }),
    );
    assert.strictEqual(out.primaryState, "FROZEN_MATCH");
  });

  it("frozen_snapshot_visible_after_live_refresh: a new live snapshot does not overwrite/erase the frozen snapshot object", () => {
    const frozen = makeFrozen({ approved_head_sha: "head0001" });
    const before = JSON.stringify(frozen);
    deriveN7PrimaryState(baseInput({ live: makeLive({ head_sha: "newhead", draft: false }), frozen }));
    assert.strictEqual(JSON.stringify(frozen), before, "frozen snapshot input must never be mutated by derivation");
  });

  it("integrity_failure_outranks_clean_state", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001" }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        evidenceChainIntegrity: "FAILED",
      }),
    );
    assert.strictEqual(out.primaryState, "UNKNOWN");
    assert.strictEqual(out.severity, "UNKNOWN");
  });

  it("evidenceChainIntegrity NOT_CHECKED does not block an otherwise-clean state", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001" }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        evidenceChainIntegrity: "NOT_CHECKED",
      }),
    );
    assert.strictEqual(out.primaryState, "READY_CLEAN");
  });

  it("terminal_merged_state_is_terminal", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ state: "MERGED", head_sha: "newhead", draft: false }),
        frozen: makeFrozen({ approved_head_sha: "oldhead" }), // even with drift present
        evidenceChainIntegrity: "FAILED", // even with integrity failure present
      }),
    );
    assert.strictEqual(out.primaryState, "MERGED");
    assert.strictEqual(out.severity, "TERMINAL");
  });

  it("closed_unmerged_state_is_terminal", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ state: "CLOSED", draft: false }),
      }),
    );
    assert.strictEqual(out.primaryState, "CLOSED_UNMERGED");
    assert.strictEqual(out.severity, "TERMINAL");
  });

  it("NO_PR when PR identity is not known", () => {
    const out = deriveN7PrimaryState(baseInput({ prIdentityKnown: false, live: null, frozen: null }));
    assert.strictEqual(out.primaryState, "NO_PR");
  });

  it("LIVE_UNCHECKED when identity known but no live snapshot exists", () => {
    const out = deriveN7PrimaryState(baseInput({ live: null, frozen: null }));
    assert.strictEqual(out.primaryState, "LIVE_UNCHECKED");
  });

  it("LIVE_FETCH_FAILED when the last refresh attempt failed", () => {
    const out = deriveN7PrimaryState(baseInput({ liveFetchFailed: true }));
    assert.strictEqual(out.primaryState, "LIVE_FETCH_FAILED");
  });

  it("FROZEN_STALE when a live refresh exists but is older than the freshness policy", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ captured_at: "2026-07-17T17:00:00Z" }), // 1h before NOW
        freshnessThresholdMs: 5 * 60 * 1000, // 5-minute freshness policy
      }),
    );
    assert.strictEqual(out.primaryState, "FROZEN_STALE");
  });
});

describe("n7State — explicit CI-requirement policy (never inferred from checks.length)", () => {
  it("empty_checks_alone_do_not_prove_no_ci_policy", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001", checks: [] }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "UNKNOWN" },
      }),
    );
    assert.notStrictEqual(out.primaryState, "FROZEN_MATCH");
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("empty_checks_with_unknown_ci_policy_is_not_clean", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: true, checks: [] }),
        frozen: null,
        ciRequirementPolicy: { kind: "UNKNOWN" },
      }),
    );
    assert.notStrictEqual(out.severity, "OK");
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("required_ci_with_no_observed_checks_is_ci_pending", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, checks: [] }),
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: ["test"] },
      }),
    );
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("required_ci_with_missing_named_check_is_ci_pending", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false }), // has "test", not "lint"
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: ["test", "lint"] },
      }),
    );
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("required_ci_with_partial_check_pagination_is_ci_pending", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          pagination: { changed_files_complete: true, checks_complete: false, review_threads_complete: true },
        }),
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: ["test"] },
      }),
    );
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("required_ci_success_must_correlate_to_current_head", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001" }), // default check head_sha is "head0001"
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: ["test"] },
      }),
    );
    assert.strictEqual(out.comparison.ci, "SUCCESS");
    assert.strictEqual(out.primaryState, "READY_CLEAN");
  });

  it("old_head_green_checks_do_not_clear_current_head", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          head_sha: "newhead",
          checks: [
            {
              provider: "github",
              name: "test",
              app: "github-actions",
              status: "COMPLETED",
              conclusion: "SUCCESS",
              head_sha: "head0001", // stale head, not "newhead"
              started_at: null,
              completed_at: null,
              details_url: null,
              provenance: "GITHUB_LIVE",
            },
          ],
        }),
        frozen: makeFrozen({ approved_head_sha: "newhead" }),
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: ["test"] },
      }),
    );
    assert.notStrictEqual(out.primaryState, "READY_CLEAN");
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });

  it("explicit_verified_no_ci_policy_can_reach_frozen_match", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001", checks: [] }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      }),
    );
    assert.strictEqual(out.primaryState, "FROZEN_MATCH");
  });

  it("ready_clean_requires_explicit_required_ci_success (NOT_REQUIRED alone is insufficient)", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false, head_sha: "head0001", checks: [] }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      }),
    );
    assert.notStrictEqual(out.primaryState, "READY_CLEAN");
    assert.strictEqual(out.primaryState, "FROZEN_MATCH");
  });

  it("a NOT_REQUIRED policy still honors an observed failing check", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({
          draft: false,
          head_sha: "head0001",
          checks: [
            {
              provider: "github",
              name: "optional-lint",
              app: "github-actions",
              status: "COMPLETED",
              conclusion: "FAILURE",
              head_sha: "head0001",
              started_at: null,
              completed_at: null,
              details_url: null,
              provenance: "GITHUB_LIVE",
            },
          ],
        }),
        frozen: makeFrozen({ approved_head_sha: "head0001" }),
        ciRequirementPolicy: { kind: "NOT_REQUIRED" },
      }),
    );
    assert.strictEqual(out.primaryState, "CI_FAILED");
  });

  it("an empty REQUIRED requiredCheckNames list is fail-closed, not silently NOT_REQUIRED", () => {
    const out = deriveN7PrimaryState(
      baseInput({
        live: makeLive({ draft: false }),
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: [] },
      }),
    );
    assert.strictEqual(out.primaryState, "CI_PENDING");
  });
});

describe("n7State — next-permitted-action guidance never authorizes a write", () => {
  it("refresh_guidance_does_not_authorize_write", () => {
    const action = deriveNextPermittedAction("LIVE_UNCHECKED");
    assert.strictEqual(action, "REFRESH_LIVE_STATE");
    assert.doesNotThrow(() => assertNextActionNeverAuthorizesWrite(action));
  });

  it("freeze_guidance_does_not_advance_package_rung", () => {
    const action = deriveNextPermittedAction("DRAFT_CLEAN");
    assert.strictEqual(action, "FREEZE_REVIEW_EVIDENCE");
    assert.doesNotThrow(() => assertNextActionNeverAuthorizesWrite(action));
    // FREEZE_REVIEW_EVIDENCE is local evidence capture, not any package-*
    // rung; assert it is not literally a package rung name.
    assert.ok(!/PACKAGE/.test(action));
  });

  it("no_state_returns_merge_execution_authority: every state's action is write-safe", () => {
    for (const state of N7_PRIMARY_STATES) {
      const action = deriveNextPermittedAction(state);
      assert.ok(N7_NEXT_ACTIONS.includes(action), `unexpected action for ${state}: ${action}`);
      assert.doesNotThrow(() => assertNextActionNeverAuthorizesWrite(action), `action for ${state} must be write-safe`);
    }
  });

  it("terminal states resolve to NO_ACTION_TERMINAL", () => {
    assert.strictEqual(deriveNextPermittedAction("MERGED"), "NO_ACTION_TERMINAL");
    assert.strictEqual(deriveNextPermittedAction("CLOSED_UNMERGED"), "NO_ACTION_TERMINAL");
  });

  it("UNKNOWN resolves to STOP_UNKNOWN_DATA", () => {
    assert.strictEqual(deriveNextPermittedAction("UNKNOWN"), "STOP_UNKNOWN_DATA");
  });

  it("severityForPrimaryState never reports OK for a STOP-only state", () => {
    assert.strictEqual(severityForPrimaryState("HEAD_DRIFT"), "STOP");
    assert.strictEqual(severityForPrimaryState("REVIEW_BLOCKED"), "STOP");
    assert.strictEqual(severityForPrimaryState("MERGE_CONFLICT"), "STOP");
    assert.strictEqual(severityForPrimaryState("CI_FAILED"), "STOP");
  });
});
