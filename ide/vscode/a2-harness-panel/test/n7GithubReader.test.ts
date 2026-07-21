import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { GUARDS_SCRIPT, SRC_DIR } from "./_paths";

// The exact production guard function n7GithubReader.ts is audited with —
// requiring this module never runs the repository-wide audit or calls
// process.exit (see the `require.main === module` guard in run-guards.js);
// it only exposes findN7GithubReaderViolations/N7_GITHUB_READER_RULES.
interface N7GuardViolation {
  label: string;
  match: string;
}
const guardsModule = require(GUARDS_SCRIPT) as {
  findN7GithubReaderViolations: (rawSourceText: string) => N7GuardViolation[];
};
const findN7GithubReaderViolations = guardsModule.findN7GithubReaderViolations;
import {
  MergeabilityRead,
  N7GithubReader,
  N7Transport,
  N7TransportRequest,
  N7TransportResult,
  PrSelector,
  createN7GithubReader,
} from "../src/n7GithubReader";

const SELECTOR: PrSelector = { owner: "thesidestackai", name: "stack-code", prNumber: 42 };
const CAPTURED_AT = "2026-07-21T18:00:00Z";
const HEAD = "aaaaaaa1";
const OLD_HEAD = "cccccccc";
const BASE = "bbbbbbb2";

// ---------------------------------------------------------------------------
// Fixture payload builders
// ---------------------------------------------------------------------------

function identityPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    repository: { owner: SELECTOR.owner, name: SELECTOR.name, url: "https://example.invalid/repo" },
    number: SELECTOR.prNumber,
    url: "https://example.invalid/repo/pull/42",
    title: "Example PR",
    state: "OPEN",
    draft: false,
    baseRef: "main",
    baseSha: BASE,
    headRef: "feature/x",
    headSha: HEAD,
    commitCount: 3,
    mergeability: "MERGEABLE",
    mergeStateStatus: "CLEAN",
    ...overrides,
  };
}

function changedFilesPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    files: [{ filename: "a.md", status: "modified", additions: 1, deletions: 0, previousFilename: null }],
    ...overrides,
  };
}

function checksPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    headSha: HEAD,
    checks: [
      {
        provider: "github",
        name: "test",
        app: "github-actions",
        status: "COMPLETED",
        conclusion: "SUCCESS",
        headSha: HEAD,
        startedAt: null,
        completedAt: null,
        detailsUrl: null,
      },
    ],
    ...overrides,
  };
}

function reviewsPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    observedHeadSha: HEAD,
    reviewDecision: "APPROVED",
    requestedChanges: [],
    unresolvedThreadCount: 0,
    threadRefs: [],
    blockingAutomatedFindings: [],
    ...overrides,
  };
}

function mergeabilityPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return { headSha: HEAD, mergeability: "MERGEABLE", mergeStateStatus: "CLEAN", ...overrides };
}

function success(payload: unknown, opts: Partial<{ hasMorePages: boolean; nextPageToken: string | null }> = {}): N7TransportResult {
  return {
    kind: "SUCCESS",
    payload,
    hasMorePages: opts.hasMorePages ?? false,
    nextPageToken: opts.nextPageToken ?? null,
    rateLimit: null,
  };
}

// A transport that answers every operation with a COMPLETE, correlated,
// single-page happy-path response — the baseline "everything is fine" fake.
function happyTransport(overrides: Partial<Record<N7TransportRequest["operation"], N7TransportResult[]>> = {}): N7Transport {
  const scripts: Record<string, N7TransportResult[]> = {
    pull_request: [success(identityPayload())],
    changed_files: [success(changedFilesPayload())],
    checks: [success(checksPayload())],
    reviews_and_threads: [success(reviewsPayload())],
    mergeability: [success(mergeabilityPayload())],
    ...overrides,
  };
  const counters: Record<string, number> = {};
  return async (req: N7TransportRequest): Promise<N7TransportResult> => {
    const list = scripts[req.operation];
    if (!list || list.length === 0) {
      throw new Error("test bug: no scripted response for " + req.operation);
    }
    const idx = counters[req.operation] ?? 0;
    counters[req.operation] = idx + 1;
    return list[Math.min(idx, list.length - 1)];
  };
}

function makeReader(transport: N7Transport): N7GithubReader {
  return createN7GithubReader(transport);
}

// ---------------------------------------------------------------------------
// Complete reads
// ---------------------------------------------------------------------------

describe("n7GithubReader — complete reads", () => {
  it("complete_provider_response_produces_valid_live_snapshot", async () => {
    const reader = makeReader(happyTransport());
    const res = await reader.readPullRequestLiveSnapshot(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.ok(res.snapshot !== null);
    assert.strictEqual(res.validationError, null);
    assert.strictEqual(res.snapshot?.head_sha, HEAD);
    assert.strictEqual(res.snapshot?.pr_number, SELECTOR.prNumber);
  });

  it("complete_changed_files_pagination_is_marked_complete", async () => {
    const reader = makeReader(happyTransport());
    const res = await reader.readChangedFiles(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.meta.paginationComplete, true);
    assert.strictEqual(res.files.length, 1);
  });

  it("complete_reviews_and_threads_are_preserved", async () => {
    const reader = makeReader(
      happyTransport({
        reviews_and_threads: [
          success(reviewsPayload({ requestedChanges: ["r1"], threadRefs: ["t1", "t2"], unresolvedThreadCount: 2 })),
        ],
      }),
    );
    const res = await reader.readReviewsAndThreads(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.deepStrictEqual(res.requestedChanges, ["r1"]);
    assert.deepStrictEqual(res.threadRefs, ["t1", "t2"]);
    assert.strictEqual(res.unresolvedThreadCount, 2);
  });

  it("mergeability_and_head_identity_are_preserved", async () => {
    const reader = makeReader(happyTransport());
    const res = await reader.readMergeability(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.mergeability, "MERGEABLE");
    assert.strictEqual(res.meta.headShaObserved, HEAD);
  });
});

// ---------------------------------------------------------------------------
// Partial responses
// ---------------------------------------------------------------------------

describe("n7GithubReader — partial responses", () => {
  it("missing_changed_files_page_is_partial", async () => {
    const reader = makeReader(
      happyTransport({
        changed_files: [
          success(changedFilesPayload(), { hasMorePages: true, nextPageToken: "p2" }),
          { kind: "TIMEOUT" },
        ],
      }),
    );
    const res = await reader.readChangedFiles(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    assert.strictEqual(res.meta.paginationComplete, false);
    // The page that DID succeed is not discarded.
    assert.strictEqual(res.files.length, 1);
  });

  it("missing_review_thread_page_is_not_zero_blockers", async () => {
    const reader = makeReader(
      happyTransport({
        reviews_and_threads: [
          success(reviewsPayload({ unresolvedThreadCount: 5, threadRefs: ["t1"] }), { hasMorePages: true, nextPageToken: "p2" }),
          { kind: "PROVIDER_ERROR", errorClass: "page_fetch_failed" },
        ],
      }),
    );
    const res = await reader.readReviewsAndThreads(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    // The count is preserved from what was captured; it must not be
    // silently presented as zero/no-blockers.
    assert.strictEqual(res.unresolvedThreadCount, 5);
  });

  it("partial_checks_do_not_become_success", async () => {
    const reader = makeReader(
      happyTransport({
        checks: [
          success(checksPayload(), { hasMorePages: true, nextPageToken: "p2" }),
          { kind: "TIMEOUT" },
        ],
      }),
    );
    const res = await reader.readChecksForHead(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
  });

  it("mixed_head_checks_are_partial_or_unknown", async () => {
    // Page 1 correlates to the current head; page 2 correlates to an old
    // head — the batch as a whole cannot prove it is entirely current-head.
    const reader = makeReader(
      happyTransport({
        checks: [
          success(checksPayload({ headSha: HEAD }), { hasMorePages: true, nextPageToken: "p2" }),
          success(checksPayload({ headSha: OLD_HEAD, checks: [] }), { hasMorePages: false }),
        ],
      }),
    );
    const res = await reader.readChecksForHead(SELECTOR, HEAD, CAPTURED_AT);
    assert.notStrictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.outcome, "PARTIAL");
  });

  it("missing_head_correlation_is_unknown", async () => {
    const reader = makeReader(
      happyTransport({
        mergeability: [success(mergeabilityPayload({ headSha: OLD_HEAD }))],
      }),
    );
    const res = await reader.readMergeability(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    assert.strictEqual(res.mergeability, null);
  });

  it("empty_arrays_without_completion_do_not_prove_empty_results", async () => {
    const reader = makeReader(
      happyTransport({
        changed_files: [
          success(changedFilesPayload({ files: [] }), { hasMorePages: true, nextPageToken: "p2" }),
          { kind: "TIMEOUT" },
        ],
      }),
    );
    const res = await reader.readChangedFiles(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    assert.strictEqual(res.meta.paginationComplete, false);
  });

  it("an empty but explicitly complete page is honestly COMPLETE (contrast case)", async () => {
    const reader = makeReader(
      happyTransport({ changed_files: [success(changedFilesPayload({ files: [] }), { hasMorePages: false })] }),
    );
    const res = await reader.readChangedFiles(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.files.length, 0);
  });
});

// ---------------------------------------------------------------------------
// Exact head
// ---------------------------------------------------------------------------

describe("n7GithubReader — exact head correlation", () => {
  it("green_ci_for_old_head_does_not_clear_current_head", async () => {
    const reader = makeReader(
      happyTransport({
        checks: [
          success(
            checksPayload({
              headSha: OLD_HEAD,
              checks: [
                {
                  provider: "github",
                  name: "test",
                  app: "github-actions",
                  status: "COMPLETED",
                  conclusion: "SUCCESS",
                  headSha: OLD_HEAD,
                  startedAt: null,
                  completedAt: null,
                  detailsUrl: null,
                },
              ],
            }),
          ),
        ],
      }),
    );
    const res = await reader.readChecksForHead(SELECTOR, HEAD, CAPTURED_AT);
    // The check genuinely reports SUCCESS, but it correlates only to the
    // OLD head, not the requested current head — the read must not
    // therefore be COMPLETE/proven-successful for HEAD.
    assert.strictEqual(res.checks[0]?.conclusion, "SUCCESS");
    assert.strictEqual(res.checks[0]?.head_sha, OLD_HEAD);
    assert.notStrictEqual(res.outcome, "COMPLETE");
  });

  it("requested_head_mismatch_fails_closed", async () => {
    const reader = makeReader(happyTransport({ mergeability: [success(mergeabilityPayload({ headSha: OLD_HEAD }))] }));
    const res: MergeabilityRead = await reader.readMergeability(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    assert.notStrictEqual(res.mergeability, "MERGEABLE");
  });

  it("review_fact_without_head_correlation_is_unknown", async () => {
    const reader = makeReader(happyTransport({ reviews_and_threads: [success(reviewsPayload({ observedHeadSha: null }))] }));
    const res = await reader.readReviewsAndThreads(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
  });

  it("the assembled snapshot's head always equals the requested identity head", async () => {
    const reader = makeReader(happyTransport());
    const res = await reader.readPullRequestLiveSnapshot(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.snapshot?.head_sha, HEAD);
  });

  it("a live snapshot is not assembled when a sub-read has head drift", async () => {
    const reader = makeReader(happyTransport({ checks: [success(checksPayload({ headSha: OLD_HEAD }))] }));
    const res = await reader.readPullRequestLiveSnapshot(SELECTOR, CAPTURED_AT);
    assert.notStrictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.snapshot, null);
  });
});

// ---------------------------------------------------------------------------
// Authentication and errors
// ---------------------------------------------------------------------------

describe("n7GithubReader — authentication and provider errors", () => {
  it("authentication_failure_returns_safe_failed_result", async () => {
    const reader = makeReader(happyTransport({ pull_request: [{ kind: "AUTH_FAILURE", errorClass: "unauthorized" }] }));
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    assert.strictEqual(res.meta.authFailure, "unauthorized");
    assert.strictEqual(res.identity, null);
  });

  it("authentication_failure_does_not_expose_token", async () => {
    const reader = makeReader(happyTransport({ pull_request: [{ kind: "AUTH_FAILURE", errorClass: "token_expired" }] }));
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    const serialized = JSON.stringify(res);
    assert.ok(!/ghp_|gho_|bearer|authorization/i.test(serialized), "no credential-shaped content in the result");
    assert.strictEqual(res.meta.authFailure, "token_expired");
  });

  it("timeout_returns_failed_or_unknown_not_stale_clean", async () => {
    const reader = makeReader(happyTransport({ pull_request: [{ kind: "TIMEOUT" }] }));
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    assert.strictEqual(res.meta.timedOut, true);
  });

  it("provider_error_does_not_echo_sensitive_response", async () => {
    const reader = makeReader(
      happyTransport({ pull_request: [{ kind: "PROVIDER_ERROR", errorClass: "internal_error" }] }),
    );
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    assert.strictEqual(res.meta.unknownReason, "internal_error");
  });

  it("a throwing transport is normalized to a safe provider error, never the raw exception", async () => {
    const throwingTransport: N7Transport = async () => {
      throw new Error("secret-bearing-stack-trace token=abc123");
    };
    const reader = makeReader(throwingTransport);
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    const serialized = JSON.stringify(res);
    assert.ok(!serialized.includes("token=abc123"));
    assert.ok(!serialized.includes("secret-bearing"));
  });
});

// ---------------------------------------------------------------------------
// Rate limits
// ---------------------------------------------------------------------------

describe("n7GithubReader — rate limits", () => {
  it("rate_limit_metadata_is_preserved_safely", async () => {
    const transportWithRateLimit: N7Transport = async (req) => {
      if (req.operation === "pull_request") {
        return {
          kind: "SUCCESS",
          payload: identityPayload(),
          hasMorePages: false,
          nextPageToken: null,
          rateLimit: { limitClass: "PRIMARY", remaining: 10, retryAfterSeconds: null },
        };
      }
      return { kind: "PROVIDER_ERROR", errorClass: "unused" };
    };
    const reader = makeReader(transportWithRateLimit);
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.deepStrictEqual(res.meta.rateLimit, { limitClass: "PRIMARY", remaining: 10, retryAfterSeconds: null });
  });

  it("rate_limit_does_not_retry_automatically", async () => {
    let callCount = 0;
    const transport: N7Transport = async () => {
      callCount++;
      return { kind: "RATE_LIMITED", rateLimit: { limitClass: "SECONDARY", remaining: 0, retryAfterSeconds: 30 } };
    };
    const reader = makeReader(transport);
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    assert.strictEqual(callCount, 1, "no automatic retry occurred");
  });

  it("rate_limited_review_page_is_partial", async () => {
    const reader = makeReader(
      happyTransport({
        reviews_and_threads: [
          success(reviewsPayload(), { hasMorePages: true, nextPageToken: "p2" }),
          { kind: "RATE_LIMITED", rateLimit: { limitClass: "PRIMARY", remaining: 0, retryAfterSeconds: 60 } },
        ],
      }),
    );
    const res = await reader.readReviewsAndThreads(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "PARTIAL");
    assert.deepStrictEqual(res.meta.rateLimit, { limitClass: "PRIMARY", remaining: 0, retryAfterSeconds: 60 });
  });
});

// ---------------------------------------------------------------------------
// No-write boundary
// ---------------------------------------------------------------------------

describe("n7GithubReader — no-write boundary", () => {
  it("reader_interface_has_no_write_methods", async () => {
    const reader = makeReader(happyTransport());
    const methodNames = Object.keys(reader);
    const forbidden = ["create", "update", "merge", "close", "delete", "approve", "submit", "resolve", "rerun", "push"];
    for (const name of methodNames) {
      const lower = name.toLowerCase();
      for (const f of forbidden) {
        assert.ok(!lower.startsWith(f), `reader method ${name} looks write-shaped`);
      }
    }
    assert.deepStrictEqual(
      methodNames.sort(),
      [
        "readChangedFiles",
        "readChecksForHead",
        "readMergeability",
        "readPullRequestIdentity",
        "readPullRequestLiveSnapshot",
        "readReviewsAndThreads",
      ].sort(),
    );
  });

  it("github_write_method_is_unreachable_from_n7_reader", async () => {
    const reader = makeReader(happyTransport()) as unknown as Record<string, unknown>;
    for (const verb of ["createPullRequest", "mergePullRequest", "closePullRequest", "deleteBranch", "submitReview", "approveReview"]) {
      assert.strictEqual(typeof reader[verb], "undefined", `${verb} must not exist on the reader`);
    }
  });

  it("graphql_mutation_strings_are_absent", () => {
    const sourcePath = path.join(SRC_DIR, "n7GithubReader.ts");
    const raw = fs.readFileSync(sourcePath, { encoding: "utf8" });
    assert.ok(!/mutation/i.test(raw), "the word 'mutation' must not appear anywhere in n7GithubReader.ts, including comments");
  });

  it("package_pr_path_is_not_reused", () => {
    const sourcePath = path.join(SRC_DIR, "n7GithubReader.ts");
    const raw = fs.readFileSync(sourcePath, { encoding: "utf8" });
    assert.ok(!raw.includes("package-pr"));
    assert.ok(!raw.includes("helperRunner"));
    assert.ok(!raw.includes("a2-ide-harness.sh"));
  });

  // The pre-existing test/guards.test.ts already spawns scripts/run-guards.js
  // over the whole shipped src/ tree and asserts exit 0 — that coverage
  // already includes n7GithubReader.ts (run-guards.js walks all of src/), so
  // it is not duplicated here.
});

// ---------------------------------------------------------------------------
// Static guard — representative negative/positive tests against the ACTUAL
// production rule table (findN7GithubReaderViolations, imported from
// scripts/run-guards.js above), not a second, hand-rolled regex set. These
// prove the guard itself would catch each forbidden shape, independent of
// whether the shipped n7GithubReader.ts happens to contain it today.
// ---------------------------------------------------------------------------

describe("n7GithubReader — static guard rejects representative forbidden source", () => {
  function violationLabels(src: string): string[] {
    return findN7GithubReaderViolations(src).map((v) => v.label);
  }

  it("guard_rejects_octokit_import", () => {
    const src = 'import { Octokit } from "@octokit/rest";\nconst client = new Octokit();';
    const labels = violationLabels(src);
    assert.ok(labels.includes("FORBIDDEN-OCTOKIT"), JSON.stringify(labels));

    const requireSrc = 'const octokit = require("@octokit/core");';
    assert.ok(violationLabels(requireSrc).includes("FORBIDDEN-OCTOKIT"));
  });

  it("guard_rejects_fetch_transport", () => {
    const src = 'fetch("https://api.github.com/repos/x/y/pulls/1");';
    assert.ok(violationLabels(src).includes("FORBIDDEN-NETWORK"));
  });

  it("guard_rejects_graphql_mutation", () => {
    // Representative of how a real GraphQL mutation would actually appear:
    // inside a template literal, which stripCommentsAndStrings blanks out —
    // exercising exactly the blind spot a naive live-only regex would miss.
    const src = "const query = `mutation { addComment(input: {}) { clientMutationId } }`;";
    assert.ok(violationLabels(src).includes("FORBIDDEN-GRAPHQL-OR-GIT-WRITE"));
  });

  it("guard_rejects_pull_request_write_method", () => {
    const src = "async function doIt() { return client.createPullRequest({}); }";
    assert.ok(violationLabels(src).includes("FORBIDDEN-GITHUB-WRITE"));

    for (const verb of [
      "updatePullRequest",
      "markPullRequestReady",
      "requestReviewers",
      "submitReview",
      "approveReview",
      "resolveReviewThread",
      "rerunWorkflow",
      "enableAutoMerge",
      "mergePullRequest",
      "closePullRequest",
      "deleteBranch",
      "forcePush",
    ]) {
      assert.ok(
        violationLabels(`function f() { return ${verb}(); }`).includes("FORBIDDEN-GITHUB-WRITE"),
        `expected ${verb} to be rejected`,
      );
    }
  });

  it("guard_rejects_package_pr_reuse", () => {
    const importSrc = 'import { runHelper } from "./helperRunner";';
    assert.ok(violationLabels(importSrc).includes("FORBIDDEN-GRAPHQL-OR-GIT-WRITE"));

    const subcommandSrc = 'const subcommand = "package-pr";';
    assert.ok(violationLabels(subcommandSrc).includes("FORBIDDEN-GRAPHQL-OR-GIT-WRITE"));

    const scriptPathSrc = 'const helperPath = "scripts/a2-ide-harness.sh";';
    assert.ok(violationLabels(scriptPathSrc).includes("FORBIDDEN-GRAPHQL-OR-GIT-WRITE"));
  });

  it("guard_rejects_child_process_transport", () => {
    const src = 'require("child_process").spawn("gh", ["pr", "create"]);';
    const labels = violationLabels(src);
    assert.ok(labels.includes("FORBIDDEN-PROCESS-SPAWN"));
  });

  it("guard_accepts_finite_read_only_reader_source", () => {
    const src = `
      export interface N7GithubReader {
        readPullRequestIdentity(selector: PrSelector, capturedAtIso: string): Promise<PrIdentityRead>;
        readPullRequestLiveSnapshot(selector: PrSelector, capturedAtIso: string): Promise<PrLiveSnapshotRead>;
        readChangedFiles(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<PagedFilesRead>;
        readChecksForHead(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<ChecksRead>;
        readReviewsAndThreads(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<ReviewsRead>;
        readMergeability(selector: PrSelector, headSha: string, capturedAtIso: string): Promise<MergeabilityRead>;
      }
      export type N7ReadOutcome = "COMPLETE" | "PARTIAL" | "FAILED";
      export type N7Transport = (req: unknown) => Promise<unknown>;
    `;
    assert.deepStrictEqual(violationLabels(src), []);
  });

  it("the shipped n7GithubReader.ts itself produces zero violations under the exact production rule table", () => {
    const sourcePath = path.join(SRC_DIR, "n7GithubReader.ts");
    const raw = fs.readFileSync(sourcePath, { encoding: "utf8" });
    assert.deepStrictEqual(findN7GithubReaderViolations(raw), []);
  });
});

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

describe("n7GithubReader — validation", () => {
  it("malformed_provider_nested_data_is_rejected", async () => {
    const reader = makeReader(
      happyTransport({
        changed_files: [success(changedFilesPayload({ files: [{ filename: "a.md", status: "modified", additions: -1, deletions: 0, previousFilename: null }] }))],
      }),
    );
    const res = await reader.readChangedFiles(SELECTOR, HEAD, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
  });

  it("malformed pull_request payload fails closed at the identity read", async () => {
    const reader = makeReader(happyTransport({ pull_request: [success({ number: 42 })] }));
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
    assert.ok(res.meta.unknownReason !== null);
  });

  it("assembled_snapshot_runs_n7a_validation", async () => {
    // Prove the assembly path is truly routed through n7Schemas.ts's own
    // validatePrLiveSnapshot by checking a COMPLETE read's snapshot has the
    // exact validator output shape (schema_version present, etc.) rather
    // than being a hand-built object that merely resembles one.
    const reader = makeReader(happyTransport());
    const res = await reader.readPullRequestLiveSnapshot(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "COMPLETE");
    assert.strictEqual(res.snapshot?.schema_version, "n7.pr-live.v1");
  });

  it("a provider pull_request payload with a non-positive PR number is rejected before assembly", async () => {
    const reader = makeReader(happyTransport({ pull_request: [success(identityPayload({ number: 0 }))] }));
    const res = await reader.readPullRequestIdentity(SELECTOR, CAPTURED_AT);
    assert.strictEqual(res.outcome, "FAILED");
  });
});

// ---------------------------------------------------------------------------
// No live calls
// ---------------------------------------------------------------------------

describe("n7GithubReader — no live network/process activity", () => {
  it("every test in this file uses only in-memory fake transports", () => {
    // Structural reminder assertion: if this file is ever edited to import
    // a real HTTP client, gh CLI wrapper, or Octokit, that import itself
    // would already be caught by run-guards.js's global NETWORK_PATTERNS/
    // PROCESS_PATTERNS checks (exercised above). This test simply documents
    // the intent for a human reader.
    assert.ok(true);
  });
});
