import * as assert from "assert";
import { EvidenceUnknown, FrozenReviewSnapshot, N7_PR_LIVE_SCHEMA_VERSION, N7_PR_REVIEW_FREEZE_SCHEMA_VERSION, PrLiveSnapshot } from "../src/n7Schemas";
import { CiRequirementPolicy, N7DerivationInput, deriveN7PrimaryState } from "../src/n7State";
import { N7PrCardInput, N7PrCardViewModel, buildN7PrCardView } from "../src/n7View";
import { RenderModel, emptyInputs, renderHtml } from "../src/render";

// A minimal, valid, hand-built N7PrCardViewModel — deliberately NOT produced
// by buildN7PrCardView. render.ts accepts the exported interface, and any
// caller (not only the trusted builder) can construct one this way, so the
// renderer must be safe against arbitrary prebuilt models, not only
// builder-produced ones.
function arbitraryModel(overrides: Partial<N7PrCardViewModel> = {}): N7PrCardViewModel {
  const base: N7PrCardViewModel = {
    repository: { owner: "o", name: "n" },
    prNumber: 1,
    prTitle: null,
    current: { headSha: null, baseSha: null, capturedAt: null, statusLabel: "s" },
    frozen: { headSha: null, baseSha: null, capturedAt: null, statusLabel: "s", freezeId: null },
    comparison: { primaryState: "UNKNOWN", severity: "UNKNOWN", exactLabel: "UNKNOWN: x", detail: "d" },
    ci: { state: "NOT_REQUIRED", headSha: null, summary: "s" },
    review: { state: "UNKNOWN", summary: "s", unresolvedThreadCount: null },
    blockers: [],
    nextPermittedAction: { action: "STOP_UNKNOWN_DATA", label: "L", detail: "D" },
    unknowns: [],
  };
  return { ...base, ...overrides };
}

function renderArbitrary(overrides: Partial<N7PrCardViewModel> = {}): string {
  return renderHtml({ inputs: emptyInputs(), output: null, notice: null, n7: arbitraryModel(overrides) });
}

// ---------------------------------------------------------------------------
// Fixtures — same shape and defaults as n7State.test.ts's own makeLive/
// makeFrozen (not imported: that file does not export them, and each N7
// render test file builds its own minimal fixtures, matching every other
// n*Render.test.ts convention in this package).
// ---------------------------------------------------------------------------

const NOW = "2026-07-24T18:00:00Z";
const FRESH_MS = 15 * 60 * 1000;

function makeLive(overrides: Partial<PrLiveSnapshot> = {}): PrLiveSnapshot {
  const base: PrLiveSnapshot = {
    schema_version: N7_PR_LIVE_SCHEMA_VERSION,
    snapshot_id: "live_1",
    captured_at: "2026-07-24T17:58:35Z",
    captured_by: { source: "github-reader", reader_version: "n7-reader.v1" },
    repository: { owner: "thesidestackai", name: "stack-code", url: "https://example.invalid/repo", provider: "github" },
    pr_number: 168,
    pr_url: "https://example.invalid/repo/pull/168",
    title: "Example PR",
    state: "OPEN",
    draft: false,
    base_ref: "main",
    base_sha: "base0001base0001base0001base0001base0001",
    head_ref: "feat/example",
    head_sha: "head0001head0001head0001head0001head0001",
    commit_count: 1,
    changed_file_count: 1,
    changed_files: [{ filename: "src/example.ts", status: "modified", additions: 1, deletions: 0, previous_filename: null }],
    mergeability: "MERGEABLE",
    merge_state_status: "CLEAN",
    checks: [
      {
        provider: "github",
        name: "test",
        app: "github-actions",
        status: "COMPLETED",
        conclusion: "SUCCESS",
        head_sha: "head0001head0001head0001head0001head0001",
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
    frozen_at: "2026-07-24T17:59:00Z",
    repository: { owner: "thesidestackai", name: "stack-code" },
    pr_number: 168,
    pr_snapshot_ref: "artifact_pr_live_snapshot",
    pr_snapshot_sha256: "abc",
    approved_head_sha: "head0001head0001head0001head0001head0001",
    base_sha: "base0001base0001base0001base0001base0001",
    changed_file_count: 1,
    changed_filenames_sha256: "def",
    ci_summary: { state: "SUCCESS", head_sha: "head0001head0001head0001head0001head0001", check_identities: [] },
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

const REQUIRED_POLICY: CiRequirementPolicy = { kind: "REQUIRED", requiredCheckNames: ["test"] };
const NOT_REQUIRED_POLICY: CiRequirementPolicy = { kind: "NOT_REQUIRED" };

function makeDerivationInput(overrides: Partial<N7DerivationInput> = {}): N7DerivationInput {
  return {
    prIdentityKnown: true,
    live: makeLive(),
    liveFetchFailed: false,
    frozen: makeFrozen(),
    ciRequirementPolicy: REQUIRED_POLICY,
    evidenceChainIntegrity: "NOT_CHECKED",
    nowIso: NOW,
    freshnessThresholdMs: FRESH_MS,
    ...overrides,
  };
}

function makeCardInput(overrides: Partial<N7DerivationInput> = {}): N7PrCardInput {
  const derivationInput = makeDerivationInput(overrides);
  const derived = deriveN7PrimaryState(derivationInput);
  return {
    repository: { owner: "thesidestackai", name: "stack-code" },
    live: derivationInput.live,
    frozen: derivationInput.frozen,
    derived,
  };
}

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function renderWithN7(overrides: Partial<N7DerivationInput> = {}, cardInputOverrides: Partial<N7PrCardInput> = {}): string {
  const input = { ...makeCardInput(overrides), ...cardInputOverrides };
  return renderHtml({ ...baseModel(), n7: buildN7PrCardView(input) });
}

// Extracts exactly the N7 section's own HTML (from its opening <section
// class="n7-pr-card" to the matching top-level </section>), for tests that
// must scope their assertions to the N7 card and not incidentally match
// unrelated sections (e.g. N5's own "MERGEABLE"/"package-plan" text).
function extractN7Section(html: string): string {
  const start = html.indexOf('<section class="n7-pr-card');
  assert.ok(start >= 0, "test sanity: n7 section must be present");
  const end = html.indexOf("</body>", start);
  assert.ok(end >= 0, "test sanity: body close must follow the n7 section");
  return html.slice(start, end);
}

// ---------------------------------------------------------------------------
// Exact identity tests
// ---------------------------------------------------------------------------

describe("n7 render — exact identity", () => {
  it("current_head_is_visible_in_full", () => {
    const html = renderWithN7();
    assert.ok(html.includes("head0001head0001head0001head0001head0001"));
  });

  it("frozen_reviewed_head_is_visible_in_full", () => {
    const html = renderWithN7();
    // Same value in this fixture (MATCH case) but must appear specifically
    // in the frozen identity block, not only the current one.
    const section = extractN7Section(html);
    const frozenStart = section.indexOf('data-testid="n7-frozen-identity"');
    const frozenBlock = section.slice(frozenStart, section.indexOf("</section>", frozenStart));
    assert.ok(frozenBlock.includes("head0001head0001head0001head0001head0001"));
  });

  it("current_and_frozen_heads_are_visually_distinct", () => {
    const html = renderWithN7({
      live: makeLive({ head_sha: "currentHeadDistinctValue0000000000000001" }),
      frozen: makeFrozen({ approved_head_sha: "frozenHeadDistinctValue0000000000000002" }),
    });
    const section = extractN7Section(html);
    const currentStart = section.indexOf('data-testid="n7-current-head"');
    const frozenStart = section.indexOf('data-testid="n7-frozen-head"');
    assert.ok(currentStart >= 0 && frozenStart >= 0);
    assert.ok(section.includes("currentHeadDistinctValue0000000000000001"));
    assert.ok(section.includes("frozenHeadDistinctValue0000000000000002"));
  });

  it("repository_and_pr_identity_are_visible", () => {
    const html = renderWithN7();
    assert.ok(html.includes("thesidestackai"));
    assert.ok(html.includes("stack-code"));
    assert.ok(html.includes("168"));
  });

  it("current_and_frozen_base_sha_are_visible_when_present", () => {
    const html = renderWithN7();
    assert.ok(html.includes("base0001base0001base0001base0001base0001"));
  });

  it("refresh_and_freeze_timestamps_are_visible", () => {
    const html = renderWithN7();
    assert.ok(html.includes("2026-07-24T17:58:35Z"));
    assert.ok(html.includes("2026-07-24T17:59:00Z"));
  });

  it("freeze_identity_is_visible", () => {
    const html = renderWithN7();
    assert.ok(html.includes("freeze_1"));
  });
});

// ---------------------------------------------------------------------------
// State tests
// ---------------------------------------------------------------------------

describe("n7 render — state", () => {
  it("match_state_explains_exact_identity_match", () => {
    const html = renderWithN7({ ciRequirementPolicy: NOT_REQUIRED_POLICY, live: makeLive({ checks: [] }) });
    assert.ok(html.includes("MATCH: current head equals frozen reviewed head"));
  });

  it("head_drift_is_stop_labeled", () => {
    const html = renderWithN7({ live: makeLive({ head_sha: "differentHead00000000000000000000000001" }) });
    assert.ok(html.includes("STOP: current head differs from frozen reviewed head"));
  });

  it("unknown_head_is_not_rendered_as_clean", () => {
    const html = renderWithN7({ live: null, frozen: null, prIdentityKnown: false });
    assert.ok(html.includes("Unknown — current head was not available"));
    assert.ok(!html.includes("OK:"), "an unknown/no-PR state must never render an OK severity label");
  });

  it("ci_success_names_the_correlated_head", () => {
    const html = renderWithN7();
    assert.ok(html.includes("SUCCESS for head0001head0001head0001head0001head0001"));
  });

  it("old_head_ci_is_not_described_as_current_success", () => {
    const html = renderWithN7({
      live: makeLive({
        checks: [
          {
            provider: "github",
            name: "test",
            app: "github-actions",
            status: "COMPLETED",
            conclusion: "SUCCESS",
            head_sha: "oldHead000000000000000000000000000000001",
            started_at: null,
            completed_at: null,
            details_url: null,
            provenance: "GITHUB_LIVE",
          },
        ],
      }),
    });
    const section = extractN7Section(html);
    const ciStart = section.indexOf('data-testid="n7-ci"');
    const ciBlock = section.slice(ciStart, section.indexOf("</section>", ciStart));
    assert.ok(!ciBlock.includes("SUCCESS"), "a check correlated only to an old head must never be described as current-head success");
  });

  it("review_blockers_are_visible", () => {
    const html = renderWithN7({ live: makeLive({ reviews: { review_decision: "CHANGES_REQUESTED", requested_changes: ["alice"], unresolved_review_threads: { count: 2, complete: true, thread_refs: [] }, blocking_automated_findings: [] } }) });
    assert.ok(html.includes("REVIEW_BLOCKED"));
    assert.ok(html.includes("unresolved thread"));
  });

  it("partial_review_data_is_unknown_not_clear", () => {
    const html = renderWithN7({ live: makeLive({ pagination: { changed_files_complete: true, checks_complete: true, review_threads_complete: false } }) });
    const section = extractN7Section(html);
    const reviewStart = section.indexOf('data-testid="n7-review"');
    const reviewBlock = section.slice(reviewStart, section.indexOf("</section>", reviewStart));
    assert.ok(reviewBlock.includes("UNKNOWN"));
    assert.ok(!reviewBlock.includes("CLEAN"));
  });

  it("terminal_state_has_terminal_text_label", () => {
    const html = renderWithN7({ live: makeLive({ state: "MERGED" }) });
    assert.ok(html.includes("TERMINAL: PR is merged"));
  });
});

// ---------------------------------------------------------------------------
// Ordering tests
// ---------------------------------------------------------------------------

describe("n7 render — ordering", () => {
  it("blockers_render_before_optional_details", () => {
    const html = renderWithN7({
      live: makeLive({ head_sha: "differentHead00000000000000000000000001", unknowns: [{ id: "u1", classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason: "some unknown reason", blocks: [], captured_at: "2026-07-24T17:58:35Z" }] }),
    });
    const section = extractN7Section(html);
    const blockersIdx = section.indexOf('data-testid="n7-blockers"');
    const unknownsIdx = section.indexOf('data-testid="n7-unknowns"');
    assert.ok(blockersIdx >= 0 && unknownsIdx >= 0);
    assert.ok(blockersIdx < unknownsIdx);
  });

  it("next_permitted_action_is_visible_without_expansion", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(section.includes('data-testid="n7-next-action"'));
    assert.ok(!/<details>[\s\S]*n7-next-action/.test(section), "next permitted action must not be hidden inside a <details> expansion");
  });

  it("current_and_frozen_identity_render_before_secondary_details", () => {
    const html = renderWithN7({
      live: makeLive({ unknowns: [{ id: "u1", classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason: "reason text", blocks: [], captured_at: "2026-07-24T17:58:35Z" }] }),
    });
    const section = extractN7Section(html);
    const currentIdx = section.indexOf('data-testid="n7-current-identity"');
    const unknownsIdx = section.indexOf('data-testid="n7-unknowns"');
    assert.ok(currentIdx >= 0 && unknownsIdx >= 0);
    assert.ok(currentIdx < unknownsIdx);
  });
});

// ---------------------------------------------------------------------------
// Accessibility tests
// ---------------------------------------------------------------------------

describe("n7 render — accessibility", () => {
  it("severity_text_is_present_without_relying_on_color", () => {
    const html = renderWithN7({ live: makeLive({ head_sha: "differentHead00000000000000000000000001" }) });
    const section = extractN7Section(html);
    assert.ok(section.includes("STOP:"), "severity text must be present as literal readable text, not only a CSS class");
  });

  it("headings_have_semantic_structure", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(/<h3>/.test(section));
    assert.ok(/<h4>/.test(section));
  });

  it("current_and_frozen_sections_have_distinct_labels", () => {
    const html = renderWithN7();
    assert.ok(html.includes("Current / Live PR State"));
    assert.ok(html.includes("Frozen Reviewed Evidence"));
  });

  it("unknown_reason_is_visible", () => {
    const html = renderWithN7({
      live: makeLive({ unknowns: [{ id: "u1", classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason: "a specific unknown reason text", blocks: [], captured_at: "2026-07-24T17:58:35Z" }] }),
    });
    assert.ok(html.includes("a specific unknown reason text"));
  });
});

// ---------------------------------------------------------------------------
// Escaping tests
// ---------------------------------------------------------------------------

const SCRIPT_PAYLOAD = "<script>alert(1)</script>";
const IMG_PAYLOAD = '<img src=x onerror=alert(1)>';
const SVG_PAYLOAD = `"'><svg onload=alert(1)>`;
const JAVASCRIPT_URI_PAYLOAD = "javascript:alert(1)";
const ENTITY_PAYLOAD = "&amp;&lt;&gt;&quot;&#39;";
const ALL_PAYLOADS = [SCRIPT_PAYLOAD, IMG_PAYLOAD, SVG_PAYLOAD, JAVASCRIPT_URI_PAYLOAD, ENTITY_PAYLOAD];

// Mirrors render.ts's own private escapeHtml() exactly, so tests can assert
// the precise expected escaped form is visible — not merely that the raw
// payload is absent.
function expectedEscapedText(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// A payload is "neutralized" when its structural HTML characters (< and >)
// have been converted to entities, so it can never be parsed as a live tag
// or attribute — even though the inert escaped text may still legitimately
// contain substrings like "onerror=" as plain, non-executing text. The
// correct check is therefore for the absence of a live tag/live URI/live
// selector, not a blanket ban on substrings that also occur inside safely-
// escaped text.
function assertNoInjection(html: string, payload: string): void {
  // Only meaningful when escaping actually transforms the payload. A
  // payload with no HTML metacharacters (e.g. a bare "javascript:" URI) is
  // inherently unchanged by escapeHtml() and is safe to appear verbatim as
  // plain, non-executing text — the check below for a live href/src is what
  // actually matters for that payload shape.
  if (expectedEscapedText(payload) !== payload) {
    assert.ok(!html.includes(payload), `raw payload must not appear unescaped: ${payload}`);
  }
  assert.ok(!/<script\b/i.test(html), "no live <script> tag");
  assert.ok(!/<img\b/i.test(html), "no live <img> tag");
  assert.ok(!/<svg\b/i.test(html), "no live <svg> tag");
  assert.ok(!/\bhref="javascript:/i.test(html), "no javascript: URI in a live href");
  assert.ok(!/\bsrc="javascript:/i.test(html), "no javascript: URI in a live src");
  for (const m of html.matchAll(/data-testid="([^"]*)"/g)) {
    assert.ok(!m[1].includes(payload), `data-testid must not contain the payload: ${m[1]}`);
  }
  for (const m of html.matchAll(/\s(?:id|class|style)="([^"]*)"/g)) {
    assert.ok(!m[1].includes(payload), `id/class/style must not contain the payload: ${m[1]}`);
  }
}

describe("n7 render — HTML escaping", () => {
  it("repository_identity_is_html_escaped", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({}, { repository: { owner: payload, name: payload } });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)), "escaped form must remain visible as text");
    }
  });

  it("pr_title_is_html_escaped", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({ live: makeLive({ title: payload }) });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)), "escaped form must remain visible as text");
    }
  });

  it("blocker_text_is_html_escaped", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({
        ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: [payload] },
        live: makeLive({ checks: [] }),
      });
      assertNoInjection(html, payload);
    }
  });

  it("unknown_reason_is_html_escaped", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({
        live: makeLive({ unknowns: [{ id: "u1", classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason: payload, blocks: [], captured_at: "2026-07-24T17:58:35Z" }] }),
      });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)), "escaped form must remain visible as text");
    }
  });

  it("unknown_field_id_is_html_escaped_and_never_enters_an_attribute", () => {
    // The most important new case: EvidenceUnknown.id is provider/model-
    // supplied and (via n7View.ts's "live."/"frozen." + id concatenation)
    // used to be reflected directly into a data-testid attribute before
    // this repair. It must now appear only as escaped visible text.
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({
        live: makeLive({ unknowns: [{ id: payload, classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason: "r", blocks: [], captured_at: "2026-07-24T17:58:35Z" }] }),
      });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(`live.${payload}`)), "field text (live.<id>) must remain visible, fully escaped");
      for (const m of html.matchAll(/data-testid="([^"]*)"/g)) {
        assert.ok(!m[1].includes(payload), `data-testid must never contain the provider-supplied id: ${m[1]}`);
      }
    }
  });

  it("entity_payload_is_safely_re_escaped_not_interpreted_as_trusted_markup", () => {
    const html = renderWithN7({ live: makeLive({ title: ENTITY_PAYLOAD }) });
    const section = extractN7Section(html);
    assert.ok(
      section.includes("&amp;amp;&amp;lt;&amp;gt;&amp;quot;&amp;#39;"),
      "a pre-escaped-looking payload must be re-escaped (& itself escaped), never treated as already-trusted markup",
    );
  });

  it("next_action_text_is_html_escaped", () => {
    // Next-action copy itself is fixed internal text (never externally
    // sourced), but the surrounding N7 region must still remain fully
    // escaped end-to-end when a payload is introduced via a field that
    // legitimately flows into the same region (ciReason, via an attacker-
    // influenceable required-check name) — proving the escaping guarantee
    // holds throughout the section, not only for specific fields.
    const html = renderWithN7({
      ciRequirementPolicy: { kind: "REQUIRED", requiredCheckNames: [SCRIPT_PAYLOAD] },
      live: makeLive({ checks: [] }),
    });
    const section = extractN7Section(html);
    assertNoInjection(section, SCRIPT_PAYLOAD);
    assert.ok(section.includes('data-testid="n7-next-action"'));
  });

  it("timestamp_and_evidence_identity_are_html_escaped", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderWithN7({
        live: makeLive({ captured_at: payload }),
        frozen: makeFrozen({ frozen_at: payload, snapshot_id: payload }),
      });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)), "escaped form must remain visible as text");
    }
  });
});

// ---------------------------------------------------------------------------
// Index-only structural selectors (N7-D structural identifier closure
// repair). No selector is ever derived by inspecting model string content
// — not blocker.code, not unknown.field, not even a startsWith prefix check
// on unknown.field. Only the array's own iteration index is used.
// ---------------------------------------------------------------------------

function unknownItem(id: string, reason: string): EvidenceUnknown {
  return { id, classification: "UNKNOWN", statement: "s", provenance: "GITHUB_LIVE", reason, blocks: [], captured_at: "2026-07-24T17:58:35Z" };
}

function renderWithUnknowns(liveUnknowns: EvidenceUnknown[], frozenUnknowns: EvidenceUnknown[] = []): string {
  return renderWithN7({
    ciRequirementPolicy: NOT_REQUIRED_POLICY,
    live: makeLive({ checks: [], unknowns: liveUnknowns }),
    frozen: makeFrozen({ unknowns: frozenUnknowns }),
  });
}

function extractDataTestids(html: string): string[] {
  return [...html.matchAll(/data-testid="([^"]*)"/g)].map((m) => m[1]);
}

describe("n7 render — index-only unknown-row selectors (builder path)", () => {
  it("unknown rows are indexed from 0 regardless of live/frozen origin", () => {
    const html = renderWithUnknowns([unknownItem("a", "reason a")]);
    const section = extractN7Section(html);
    assert.ok(section.includes('data-testid="n7-unknown-0"'));
  });

  it("multiple unknown rows receive a single flat monotonic index", () => {
    const html = renderWithUnknowns(
      [unknownItem("a", "ra"), unknownItem("b", "rb"), unknownItem("c", "rc")],
      [unknownItem("d", "rd")],
    );
    const section = extractN7Section(html);
    const testids = extractDataTestids(section).filter((t) => /^n7-unknown-\d+$/.test(t));
    assert.deepStrictEqual(testids, ["n7-unknown-0", "n7-unknown-1", "n7-unknown-2", "n7-unknown-3"]);
  });

  it("field and reason child selectors are indexed, never content-derived", () => {
    const html = renderWithUnknowns([unknownItem("a", "reason a")]);
    const section = extractN7Section(html);
    assert.ok(section.includes('data-testid="n7-unknown-field-0"'));
    assert.ok(section.includes('data-testid="n7-unknown-reason-0"'));
  });
});

// ---------------------------------------------------------------------------
// Phase 7 — direct arbitrary-prebuilt-model tests (bypass buildN7PrCardView)
// ---------------------------------------------------------------------------

describe("n7 render — arbitrary prebuilt model structural boundary", () => {
  it("arbitrary_prebuilt_blocker_code_cannot_change_data_testid", () => {
    const html = renderArbitrary({ blockers: [{ code: "bad code <x> \"y'z", label: "L", detail: "D" }] });
    const section = extractN7Section(html);
    assert.ok(section.includes('data-testid="n7-blocker-0"'));
    for (const t of extractDataTestids(section)) {
      assert.ok(!t.includes("bad code"), `data-testid must not contain the blocker code: ${t}`);
    }
  });

  it("arbitrary_prebuilt_unknown_field_cannot_choose_selector_prefix", () => {
    const html = renderArbitrary({
      unknowns: [
        { field: "live.attacker-controlled", reason: "r1" },
        { field: "frozen.attacker-controlled", reason: "r2" },
        { field: "general.attacker-controlled", reason: "r3" },
      ],
    });
    const section = extractN7Section(html);
    const testids = extractDataTestids(section).filter((t) => /^n7-unknown-\d+$/.test(t));
    assert.deepStrictEqual(testids, ["n7-unknown-0", "n7-unknown-1", "n7-unknown-2"]);
    // None of the model-chosen "live"/"frozen"/"general" words leaked into
    // any selector as a scope prefix.
    for (const t of extractDataTestids(section)) {
      assert.ok(!/^n7-unknown-(live|frozen|general)-/.test(t), `no model-chosen scope prefix permitted: ${t}`);
    }
  });

  it("arbitrary_prebuilt_unknown_field_cannot_enter_any_attribute", () => {
    const payload = '"><img src=x onerror=alert(1)>';
    const html = renderArbitrary({ unknowns: [{ field: payload, reason: "r" }] });
    const section = extractN7Section(html);
    for (const m of section.matchAll(/\s[a-zA-Z-]+="([^"]*)"/g)) {
      assert.ok(!m[1].includes(payload), `attribute must not contain the unknown field: ${m[1]}`);
    }
  });

  it("arbitrary_prebuilt_unknown_reason_cannot_enter_any_attribute", () => {
    const payload = '"><img src=x onerror=alert(1)>';
    const html = renderArbitrary({ unknowns: [{ field: "f", reason: payload }] });
    const section = extractN7Section(html);
    for (const m of section.matchAll(/\s[a-zA-Z-]+="([^"]*)"/g)) {
      assert.ok(!m[1].includes(payload), `attribute must not contain the unknown reason: ${m[1]}`);
    }
  });

  it("duplicate_blocker_codes_receive_unique_indexed_selectors", () => {
    const html = renderArbitrary({
      blockers: [
        { code: "SAME_CODE", label: "L1", detail: "D1" },
        { code: "SAME_CODE", label: "L2", detail: "D2" },
        { code: "SAME_CODE", label: "L3", detail: "D3" },
      ],
    });
    const section = extractN7Section(html);
    const testids = extractDataTestids(section).filter((t) => /^n7-blocker-\d+$/.test(t));
    assert.deepStrictEqual(testids, ["n7-blocker-0", "n7-blocker-1", "n7-blocker-2"]);
    assert.strictEqual(new Set(testids).size, 3, "duplicate content must not collapse into duplicate/colliding selectors");
  });

  it("duplicate_unknown_fields_receive_unique_indexed_selectors", () => {
    const html = renderArbitrary({
      unknowns: [
        { field: "SAME_FIELD", reason: "r1" },
        { field: "SAME_FIELD", reason: "r2" },
      ],
    });
    const section = extractN7Section(html);
    const testids = extractDataTestids(section).filter((t) => /^n7-unknown-\d+$/.test(t));
    assert.deepStrictEqual(testids, ["n7-unknown-0", "n7-unknown-1"]);
  });

  it("blocker_code_remains_visible_as_escaped_text", () => {
    const html = renderArbitrary({ blockers: [{ code: SCRIPT_PAYLOAD, label: "L", detail: "D" }] });
    const section = extractN7Section(html);
    assert.ok(section.includes(expectedEscapedText(SCRIPT_PAYLOAD)));
  });

  it("unknown_field_remains_visible_as_escaped_text", () => {
    const html = renderArbitrary({ unknowns: [{ field: SCRIPT_PAYLOAD, reason: "r" }] });
    const section = extractN7Section(html);
    assert.ok(section.includes(expectedEscapedText(SCRIPT_PAYLOAD)));
  });

  it("selector_output_is_identical_for_same_list_lengths_regardless_of_content", () => {
    const htmlA = renderArbitrary({
      blockers: [
        { code: "AAA", label: "one", detail: "alpha" },
        { code: "BBB", label: "two", detail: "beta" },
      ],
      unknowns: [
        { field: "field-one", reason: "reason-one" },
        { field: "field-two", reason: "reason-two" },
        { field: "field-three", reason: "reason-three" },
      ],
    });
    const htmlB = renderArbitrary({
      blockers: [
        { code: SCRIPT_PAYLOAD, label: IMG_PAYLOAD, detail: SVG_PAYLOAD },
        { code: "totally different <>&\"'", label: "zzz", detail: "yyy" },
      ],
      unknowns: [
        { field: JAVASCRIPT_URI_PAYLOAD, reason: ENTITY_PAYLOAD },
        { field: "xyz", reason: "abc" },
        { field: '"><svg onload=alert(1)>', reason: "another" },
      ],
    });
    const testidsA = extractDataTestids(extractN7Section(htmlA)).filter((t) => /^n7-(blocker|unknown)(-[a-z]+)?-\d+$/.test(t));
    const testidsB = extractDataTestids(extractN7Section(htmlB)).filter((t) => /^n7-(blocker|unknown)(-[a-z]+)?-\d+$/.test(t));
    assert.deepStrictEqual(testidsA, testidsB, "selector lists must be identical when only content, not row count, differs");
  });
});

// ---------------------------------------------------------------------------
// Phase 8 — structural identifier extraction test
// ---------------------------------------------------------------------------

describe("n7 render — structural identifier extraction", () => {
  it("every N7 data-testid matches the closed trusted-literal/index pattern", () => {
    const html = renderArbitrary({
      blockers: [
        { code: SCRIPT_PAYLOAD, label: "L", detail: "D" },
        { code: IMG_PAYLOAD, label: "L", detail: "D" },
      ],
      unknowns: [
        { field: SVG_PAYLOAD, reason: "r" },
        { field: JAVASCRIPT_URI_PAYLOAD, reason: "r" },
        { field: ENTITY_PAYLOAD, reason: "r" },
      ],
    });
    const section = extractN7Section(html);
    const testids = extractDataTestids(section);
    assert.ok(testids.length > 0);
    for (const t of testids) {
      assert.ok(/^n7-[a-z]+(?:-[a-z]+)*(?:-\d+)?$/.test(t), `data-testid must be lowercase trusted literals/hyphens/index only: ${t}`);
    }
    assert.strictEqual(new Set(testids).size, testids.length, "no duplicate selectors");
    // Exact expected values, not just the broad pattern.
    assert.deepStrictEqual(
      testids.filter((t) => /^n7-blocker-\d+$/.test(t)),
      ["n7-blocker-0", "n7-blocker-1"],
    );
    assert.deepStrictEqual(
      testids.filter((t) => /^n7-unknown-\d+$/.test(t)),
      ["n7-unknown-0", "n7-unknown-1", "n7-unknown-2"],
    );
  });

  it("no id/class/style/href/src attribute contains an arbitrary model string", () => {
    const payload = SCRIPT_PAYLOAD;
    const html = renderArbitrary({
      blockers: [{ code: payload, label: payload, detail: payload }],
      unknowns: [{ field: payload, reason: payload }],
      prTitle: payload,
      repository: { owner: payload, name: payload },
    });
    const section = extractN7Section(html);
    for (const m of section.matchAll(/\s(?:id|class|style|href|src)="([^"]*)"/g)) {
      assert.ok(!m[1].includes(payload), `structural attribute must not contain the model string: ${m[1]}`);
    }
  });

  it("content changes do not change selectors when list shape is unchanged", () => {
    const shapeA = renderArbitrary({ blockers: [{ code: "x1", label: "y1", detail: "z1" }] });
    const shapeB = renderArbitrary({ blockers: [{ code: "x2-completely-different", label: "y2", detail: "z2" }] });
    const testidsA = extractDataTestids(extractN7Section(shapeA)).filter((t) => t.startsWith("n7-blocker-"));
    const testidsB = extractDataTestids(extractN7Section(shapeB)).filter((t) => t.startsWith("n7-blocker-"));
    assert.deepStrictEqual(testidsA, testidsB);
  });
});

// ---------------------------------------------------------------------------
// Phase 9 — complete direct payload matrix applied to a prebuilt model
// ---------------------------------------------------------------------------

describe("n7 render — direct payload matrix on prebuilt model", () => {
  it("all payloads are neutralized across every direct field", () => {
    for (const payload of ALL_PAYLOADS) {
      const html = renderArbitrary({
        blockers: [{ code: payload, label: payload, detail: payload }],
        unknowns: [{ field: payload, reason: payload }],
        nextPermittedAction: { action: "STOP_UNKNOWN_DATA", label: payload, detail: payload },
        repository: { owner: payload, name: payload },
        prTitle: payload,
        current: { headSha: payload, baseSha: payload, capturedAt: payload, statusLabel: payload },
        frozen: { headSha: payload, baseSha: payload, capturedAt: payload, statusLabel: payload, freezeId: payload },
      });
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)), `escaped payload must remain visible for: ${payload}`);
    }
  });

  it("quote payload cannot break out of any N7 attribute", () => {
    const html = renderArbitrary({
      blockers: [{ code: SVG_PAYLOAD, label: SVG_PAYLOAD, detail: SVG_PAYLOAD }],
      prTitle: SVG_PAYLOAD,
    });
    const section = extractN7Section(html);
    // A successful attribute breakout would produce a live <svg tag; absence
    // of one (already checked by assertNoInjection) plus a well-formed
    // section (still parses as balanced tags) demonstrates containment.
    assertNoInjection(section, SVG_PAYLOAD);
    assert.ok(/<section class="n7-pr-card/.test(section));
  });

  it("entity payload is re-escaped, not treated as trusted markup, on direct fields", () => {
    const html = renderArbitrary({ prTitle: ENTITY_PAYLOAD, blockers: [{ code: "c", label: ENTITY_PAYLOAD, detail: "d" }] });
    const section = extractN7Section(html);
    assert.ok(section.includes(expectedEscapedText(ENTITY_PAYLOAD)));
  });
});

// ---------------------------------------------------------------------------
// Severity structural-class closure repair. comparison.severity is a
// TypeScript-union-only field — an arbitrary prebuilt model can set it to
// anything. normalizeN7Severity() in render.ts is the single place that
// inspects it: exact-match only against the five known literals, never
// toLowerCase()/trim()/concatenation. Anything else — including non-string
// values — must collapse to the fixed UNKNOWN class and label, and must
// never introduce an extra CSS class token or crash the renderer.
// ---------------------------------------------------------------------------

function cardClassAttr(html: string): string {
  const m = html.match(/<section class="([^"]*)" data-testid="n7-pr-card"/);
  assert.ok(m, "n7-pr-card section must be present");
  return m![1];
}

// The authoritative severity prefix and the model's (untrusted) secondary
// exactLabel text are rendered in two structurally distinct spans — see the
// "authoritative severity label closure" describe block below for the tests
// that specifically exercise this separation.
function severityPrefixText(html: string): string {
  const m = html.match(/data-testid="n7-severity-prefix">([^<]*)</);
  assert.ok(m, "n7-severity-prefix span must be present");
  return m![1];
}

function stateDetailText(html: string): string {
  const m = html.match(/data-testid="n7-state-detail">([^<]*)</);
  assert.ok(m, "n7-state-detail span must be present");
  return m![1];
}

const VALID_SEVERITIES: Array<{ severity: string; suffix: string }> = [
  { severity: "OK", suffix: "ok" },
  { severity: "WARN", suffix: "warn" },
  { severity: "STOP", suffix: "stop" },
  { severity: "UNKNOWN", suffix: "unknown" },
  { severity: "TERMINAL", suffix: "terminal" },
];

describe("n7 render — severity structural-class closure (valid severities)", () => {
  for (const { severity, suffix } of VALID_SEVERITIES) {
    it(`severity_${severity}_maps_to_fixed_${suffix}_class_and_label`, () => {
      const html = renderArbitrary({
        comparison: { primaryState: "UNKNOWN", severity: severity as N7PrCardViewModel["comparison"]["severity"], exactLabel: `${severity}: test label`, detail: "d" },
      });
      assert.strictEqual(cardClassAttr(html), `n7-pr-card n7-severity-${suffix}`);
      assert.strictEqual(severityPrefixText(html), `${severity}:`);
      assert.strictEqual(stateDetailText(html), `${severity}: test label`);
    });
  }
});

const INVALID_SEVERITIES: Array<{ name: string; value: unknown }> = [
  { name: "bad class injected", value: "bad class injected" },
  { name: "quote+onclick", value: '" onclick="alert(1)' },
  { name: "path traversal", value: "../../../" },
  { name: "OK extra-class", value: "OK extra-class" },
  { name: "lowercase ok", value: "ok" },
  { name: "mixed case Stop", value: "Stop" },
  { name: "trailing whitespace STOP ", value: "STOP " },
  { name: "script tag", value: "<script>alert(1)</script>" },
  { name: "javascript uri", value: "javascript:alert(1)" },
  { name: "empty string", value: "" },
  { name: "null", value: null },
  { name: "undefined", value: undefined },
  { name: "number", value: 42 },
  { name: "object", value: {} },
  { name: "array", value: [] },
];

describe("n7 render — severity structural-class closure (invalid severities)", () => {
  for (const { name, value } of INVALID_SEVERITIES) {
    it(`invalid severity (${name}) collapses to the fixed unknown class and label`, () => {
      const html = renderArbitrary({
        comparison: { primaryState: "UNKNOWN", severity: value as N7PrCardViewModel["comparison"]["severity"], exactLabel: "OK: fake success claim to prove non-authoritative", detail: "d" },
      });
      const classAttr = cardClassAttr(html);
      assert.strictEqual(classAttr, "n7-pr-card n7-severity-unknown", `class token count/content must be exactly fixed for: ${name}`);
      assert.strictEqual(severityPrefixText(html), "UNKNOWN:");
      assert.strictEqual(stateDetailText(html), "severity value is not a recognized N7 severity");
      if (typeof value === "string" && value.length > 0) {
        assert.ok(!classAttr.includes(value), `no input substring may appear in the class attribute: ${name}`);
      }
      assert.ok(!html.includes("onclick="), "no event-handler attribute");
      assert.ok(!/<script\b/i.test(html), "no live script tag");
      assert.ok(!html.includes("fake success"), "the model's own exactLabel must not be trusted when severity is invalid");
    });
  }

  it("no invalid severity throws during render", () => {
    for (const { value } of INVALID_SEVERITIES) {
      assert.doesNotThrow(() => {
        renderArbitrary({ comparison: { primaryState: "UNKNOWN", severity: value as N7PrCardViewModel["comparison"]["severity"], exactLabel: "x", detail: "d" } });
      });
    }
  });
});

describe("n7 render — severity content-independence", () => {
  it("all structural attributes are identical across every invalid severity input", () => {
    const renders = INVALID_SEVERITIES.map(({ value }) =>
      renderArbitrary({ comparison: { primaryState: "UNKNOWN", severity: value as N7PrCardViewModel["comparison"]["severity"], exactLabel: `label for ${String(value)}`, detail: "d" } }),
    );
    const classAttrs = renders.map(cardClassAttr);
    const prefixTexts = renders.map(severityPrefixText);
    const detailTexts = renders.map(stateDetailText);
    for (const c of classAttrs) {
      assert.strictEqual(c, classAttrs[0], "class attribute must be identical regardless of invalid severity content");
    }
    for (const p of prefixTexts) {
      assert.strictEqual(p, "UNKNOWN:");
    }
    for (const d of detailTexts) {
      assert.strictEqual(d, "severity value is not a recognized N7 severity");
    }
  });
});

// ---------------------------------------------------------------------------
// Authoritative severity label closure. comparison.exactLabel is an
// independent, untrusted string field — a prebuilt model can pair a VALID
// (recognized) severity with a contradictory exactLabel (e.g. severity STOP
// + exactLabel "OK: Safe to merge"). The authoritative prefix must always
// come only from the normalized severity, never from exactLabel, regardless
// of whether severity happens to validate.
// ---------------------------------------------------------------------------

const VALID_SEVERITY_MISMATCH_CASES: Array<{ testName: string; severity: string; suffix: string; spoofedExactLabel: string }> = [
  { testName: "valid_ok_severity_cannot_display_stop_as_authoritative_prefix", severity: "OK", suffix: "ok", spoofedExactLabel: "STOP: Review is blocked" },
  { testName: "valid_stop_severity_cannot_display_ok_as_authoritative_prefix", severity: "STOP", suffix: "stop", spoofedExactLabel: "OK: Safe to merge" },
  { testName: "valid_warn_severity_cannot_display_terminal_as_authoritative_prefix", severity: "WARN", suffix: "warn", spoofedExactLabel: "TERMINAL: Workflow completed" },
  { testName: "valid_terminal_severity_cannot_display_unknown_as_authoritative_prefix", severity: "TERMINAL", suffix: "terminal", spoofedExactLabel: "UNKNOWN: Continue automatically" },
  { testName: "valid_unknown_severity_cannot_display_ok_as_authoritative_prefix", severity: "UNKNOWN", suffix: "unknown", spoofedExactLabel: "OK: Everything is clean" },
];

describe("n7 render — authoritative severity label closure", () => {
  for (const { testName, severity, suffix, spoofedExactLabel } of VALID_SEVERITY_MISMATCH_CASES) {
    it(testName, () => {
      const html = renderArbitrary({
        comparison: { primaryState: "UNKNOWN", severity: severity as N7PrCardViewModel["comparison"]["severity"], exactLabel: spoofedExactLabel, detail: "d" },
      });
      // Exact fixed CSS class.
      assert.strictEqual(cardClassAttr(html), `n7-pr-card n7-severity-${suffix}`);
      // Exact fixed authoritative prefix — matches the TRUE severity, never
      // the contradictory spoofed exactLabel's own leading word.
      assert.strictEqual(severityPrefixText(html), `${severity}:`);
      // The model's exactLabel does not replace the prefix: it is still
      // present, but only inside the separate, non-authoritative detail
      // span — escaped, and clearly not occupying the prefix position.
      assert.strictEqual(stateDetailText(html), spoofedExactLabel);
      const section = extractN7Section(html);
      const prefixIdx = section.indexOf('data-testid="n7-severity-prefix"');
      const detailIdx = section.indexOf('data-testid="n7-state-detail"');
      assert.ok(prefixIdx >= 0 && detailIdx >= 0 && prefixIdx < detailIdx, "prefix must be a distinct element preceding the secondary detail element");
    });
  }

  it("payload exactLabel is escaped and cannot appear as the authoritative prefix", () => {
    for (const payload of [SCRIPT_PAYLOAD, IMG_PAYLOAD, SVG_PAYLOAD]) {
      const html = renderArbitrary({
        comparison: { primaryState: "UNKNOWN", severity: "OK" as N7PrCardViewModel["comparison"]["severity"], exactLabel: payload, detail: "d" },
      });
      assert.strictEqual(severityPrefixText(html), "OK:");
      assertNoInjection(html, payload);
      assert.ok(html.includes(expectedEscapedText(payload)));
    }
  });

  it("accessibility: the severity-prefix element always matches the normalized class, independent of exactLabel", () => {
    for (const { severity, suffix, spoofedExactLabel } of VALID_SEVERITY_MISMATCH_CASES) {
      const html = renderArbitrary({
        comparison: { primaryState: "UNKNOWN", severity: severity as N7PrCardViewModel["comparison"]["severity"], exactLabel: spoofedExactLabel, detail: "d" },
      });
      const classAttr = cardClassAttr(html);
      assert.ok(classAttr.includes(`n7-severity-${suffix}`));
      assert.strictEqual(severityPrefixText(html), `${severity}:`, "prefix text must agree with the class for every case, regardless of exactLabel content");
    }
  });

  it("authoritative_severity_output_depends_only_on_normalized_severity", () => {
    // Render the SAME valid severity with several completely different
    // (and mutually contradictory) exactLabel values — the class and
    // authoritative prefix must be identical every time; only the escaped
    // secondary detail text may vary.
    const contradictoryLabels = ["OK: all good", "STOP: actually blocked", "<script>x</script>", "", "TERMINAL: done"];
    const renders = contradictoryLabels.map((exactLabel) =>
      renderArbitrary({ comparison: { primaryState: "UNKNOWN", severity: "WARN" as N7PrCardViewModel["comparison"]["severity"], exactLabel, detail: "d" } }),
    );
    const classAttrs = renders.map(cardClassAttr);
    const prefixes = renders.map(severityPrefixText);
    for (const c of classAttrs) {
      assert.strictEqual(c, "n7-pr-card n7-severity-warn");
    }
    for (const p of prefixes) {
      assert.strictEqual(p, "WARN:");
    }
    const detailTexts = renders.map(stateDetailText);
    assert.strictEqual(detailTexts[0], "OK: all good");
    assert.strictEqual(detailTexts[1], "STOP: actually blocked");
  });

  it("invalid severity ignores a spoofed clean-looking exactLabel entirely", () => {
    const html = renderArbitrary({
      comparison: { primaryState: "UNKNOWN", severity: "totally invalid" as N7PrCardViewModel["comparison"]["severity"], exactLabel: "OK: Everything is clean and safe to merge", detail: "d" },
    });
    assert.strictEqual(severityPrefixText(html), "UNKNOWN:");
    assert.strictEqual(stateDetailText(html), "severity value is not a recognized N7 severity");
    assert.ok(!html.includes("safe to merge"), "an invalid severity's spoofed exactLabel must never surface anywhere");
  });
});

// ---------------------------------------------------------------------------
// No-write tests
// ---------------------------------------------------------------------------

describe("n7 render — no-write boundary", () => {
  it("n7_card_has_no_pr_mutation_controls", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!/<button\b/.test(section));
    assert.ok(!/<form\b/.test(section));
  });

  it("n7_card_has_no_package_rung_controls", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!section.includes("data-ui-action"));
    assert.ok(!/package-plan|package-commit|package-push|package-pr/.test(section));
  });

  it("n7_card_has_no_write_message_dispatch", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!section.includes("postMessage"));
    assert.ok(!section.includes("data-subcommand"));
  });

  it("n7_card_has_no_merge_ready_approve_or_rerun_action", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!/<button[^>]*>(merge|ready|approve|rerun)/i.test(section));
    assert.ok(!section.includes("data-ui-action"));
  });
});

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

describe("n7 render — regressions", () => {
  it("rendering without N7 data remains unchanged (muted placeholder only)", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="n7-pr-card"'));
    assert.ok(html.includes('data-testid="n7-pr-card-empty"'));
    assert.ok(!html.includes('data-testid="n7-current-identity"'));
  });

  it("N5/N6 and prior sections continue rendering alongside N7", () => {
    const html = renderHtml({
      ...baseModel(),
      n5: {
        state: "N5_PACKAGE_PLAN_READY",
        stepLabel: "step",
        isBlocked: false,
        n4State: "N4_EVIDENCE_READY",
        n4StepLabel: "step",
        taskSummary: "task",
        riskLevel: "SOURCE_EDIT",
        ladder: [],
      },
      n7: buildN7PrCardView(makeCardInput()),
    });
    assert.ok(html.includes('data-testid="n5-readiness-board"'));
    assert.ok(html.includes('data-testid="safety-gates"'));
    assert.ok(html.includes('data-testid="n7-pr-card"'));
  });
});

// ---------------------------------------------------------------------------
// Generic-status avoidance tests (Phase 10)
// ---------------------------------------------------------------------------

describe("n7 render — generic status avoidance", () => {
  it("does not render a bare unqualified 'Ready' label", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!/(^|[^A-Za-z_])Ready([^A-Za-z_]|$)/.test(section), "a bare 'Ready' with no adjacent evidence must not appear");
    // READY_CLEAN (an evidence-backed enum name) is acceptable when it
    // appears alongside its exact-label sentence.
  });

  it("does not render a bare unqualified 'clean' label without adjacent evidence", () => {
    const html = renderWithN7({ ciRequirementPolicy: NOT_REQUIRED_POLICY, live: makeLive({ checks: [] }) });
    const section = extractN7Section(html);
    // "CLEAN" (N7-A's own review-state enum) is acceptable; a bare
    // lowercase "clean" with no state/evidence context is not used anywhere
    // in this renderer's copy.
    assert.ok(!/\bclean\b/.test(section));
  });

  it("does not render a bare unqualified 'configured' or 'good' label", () => {
    const html = renderWithN7();
    const section = extractN7Section(html);
    assert.ok(!/\bconfigured\b/i.test(section));
    assert.ok(!/\bgood\b/i.test(section));
  });

  it("every rendered severity word is immediately followed by its exact-label sentence", () => {
    const html = renderWithN7({ live: makeLive({ head_sha: "differentHead00000000000000000000000001" }) });
    const section = extractN7Section(html);
    assert.ok(/STOP: current head differs from frozen reviewed head/.test(section));
  });
});
