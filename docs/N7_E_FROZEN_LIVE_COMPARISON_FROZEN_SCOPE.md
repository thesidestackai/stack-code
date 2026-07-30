# N7-E Frozen/Live Comparison — Frozen Scope

> Docs-only scope. This document does not implement `n7Comparison.ts` or any
> other file. It calls no GitHub API, invokes no Claw/helper/broker/Ollama
> process, changes no approval grammar or token handling, opens no PR, stages
> nothing, commits nothing, and mutates no runtime state.
>
> This scope document does not authorize implementation.
> This scope document does not authorize execution.
> Future implementation requires a separate exact activation lane.
>
> Labeling convention used throughout this document: every factual claim is
> **VERIFIED** (confirmed by direct source/test inspection or a live probe
> in this session — the default, and the label for every claim in §2, §3,
> and §5 unless stated otherwise), **INFERRED** (a deterministic conclusion
> from verified facts, called out explicitly where it appears, e.g. the
> §11 precedence-ranking argument), or **UNKNOWN** (not established; none
> occur in this document — every question the original scope left open and
> still applies is instead carried forward unresolved in §4 by quoting the
> original document's own terminology, never asserted as fact).

---

## 1. Executive Summary

N7-E's one-sentence objective: give **base comparison** the same
always-visible, fixed-copy, runtime-safe presentation badge that **head
comparison** already received in the N7-D post-merge repair — closing the
last presentation-parity gap between the comparison dimensions the original
N7-E design named (head, base, CI, review), without duplicating any N7-A
derivation logic.

Implementation is not authorized by this document because: (1) the exact
remaining gap was not known until this audit reconciled the original 2026-07
N7-E design against the N7-D repairs merged in PRs #169 and #170 on
2026-07-27/28; (2) the original document's "likely files" list
(`n7Comparison.ts` + `n7View.ts` + `n7Comparison.test.ts`) is now stale and
would duplicate existing N7-A/N7-D logic if followed literally; and (3) per
this repository's standing convention, a frozen scope document and an
implementation lane are always separate, sequential steps.

## 2. Verified Base

All facts below are **VERIFIED** by direct inspection in this session unless
marked otherwise.

- repository: `thesidestackai/stack-code`
- base SHA (`origin/main`, this scope's worktree base): `302436733a532b61052b98a6c8caa0b77fabe3ca`
- PR #169 merge (N7-D initial Draft PR Card rendering): merged, ancestor of base SHA — **VERIFIED** via `git merge-base --is-ancestor`
- PR #170 merge (N7-D identity binding + head-comparison + requested-PR-number hardening): `302436733a532b61052b98a6c8caa0b77fabe3ca` — **VERIFIED**, this commit IS `origin/main`'s tip; zero commits exist after it
- original approved feature heads: `aa2763b93bf467ed8eed7d8dd0d93456d4ad5fd9` (PR #169), `776464586c06327ba1589845f87888c5701498a4` (PR #170) — **VERIFIED** against the two preserved worktrees' `HEAD`
- current relevant test totals (full suite, freshly re-run in an isolated worktree at this exact base): **1114 passing, 0 failing**
  - `test/n7Schemas.test.ts`: 65 tests
  - `test/n7State.test.ts`: 49 tests
  - `test/n7GithubReader.test.ts`: 41 tests
  - `test/n7EvidenceStore.test.ts`: 152 tests
  - `test/n7Render.test.ts`: 122 tests
- current exact N7-A through N7-D status: see §5 (Current-State Gap Audit) and the retained evidence file `n7-implemented-vs-remaining.md`

## 3. N7-D Closure

N7-D is administratively **closed** as of PR #170's merge. Complete:

- Draft PR Card rendering (PR #169): exact current/frozen identity display, index-only selectors, runtime-normalized severity, contextual escaping, no-write boundary.
- Identity binding repair (P1, PR #170): `assertN7PrCardIdentity()` fails closed on any repository/PR-number mismatch across requested/live/frozen, with 7 independently-reachable bounded reason codes; matching SHAs are never treated as identity proof.
- Explicit head-comparison projection and rendering (P2, PR #170): `N7PrCardComparisonView.headComparison` copied verbatim from N7-A; a dedicated, always-visible, fixed-copy `Head comparison: MATCH/DRIFT/UNKNOWN` row renders regardless of primary state (confirmed visible under `CI_FAILED` and `REVIEW_BLOCKED` by existing tests).
- Requested-PR-number runtime closure (post-merge finding, folded into PR #170): malformed `requestedPrNumber` values fail closed with `INVALID_REQUESTED_PR_NUMBER` before any snapshot-dependent branch.
- Requested-PR identity projection (post-merge finding, folded into PR #170): a validated `requestedPrNumber` is now preserved as the final `prNumber` fallback when no snapshot supplies one, without inventing clean authority.

What remains **explicitly unauthorized** as cleanup and is **not** addressed by this scope:

- Removing or archiving the two preserved implementation worktrees (`stack-code-n7d-pr-card-20260724_195452`, `stack-code-n7d-identity-head-20260728_142319`) — both confirmed to still exist, untouched, at this session's start.
- Deleting the local or remote branches backing PR #169 or PR #170.
- Any further N7-D source change. If a new N7-D-shaped defect is found, it requires its own bounded repair lane exactly as the prior four did — not this scope document.

## 4. Original N7-E Intent

Preserved verbatim in spirit from `docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md` §"Implementation Slices" → "N7-E: Frozen/Live Comparison":

- **Objective**: integrate live snapshot, frozen snapshot, and derived comparison badges.
- **Likely files** (original, now stale — see §14 for the reconciled list): `ide/vscode/a2-harness-panel/src/n7Comparison.ts`, `ide/vscode/a2-harness-panel/src/n7View.ts`, `ide/vscode/a2-harness-panel/test/n7Comparison.test.ts`.
- **Dependencies**: N7-A, N7-D.
- **Test gate**: drift precedence; stale refresh; old CI; base drift; review blockers.
- **Mutation risk**: low.
- **STOP gate**: old approval must never apply to a new head.
- **Exit criteria**: comparison rules produce the required states and next permitted actions.

Terminology preserved: Frozen/Live Comparison; live snapshot; frozen snapshot;
derived comparison badges; drift precedence; stale refresh; old CI; base
drift; review blockers; old approval never applies to new head.

## 5. Current-State Gap Audit

Full matrix retained at (session-scoped evidence, not part of this commit):
`/mnt/vast-data/tmp/stack-code-n7e-frozen-scope-20260729_172954/n7-implemented-vs-remaining.md`

Summary (24 requirements classified; full detail and file/test citations are
in the retained evidence file):

- **IMPLEMENTED (20)**: schema validation; live-snapshot adapter (interface + parsing, not wired to a live transport); frozen-snapshot persistence (module, not wired); identity binding; requested-PR identity; head comparison (derivation and badge); base comparison (derivation only); CI exact-head correlation (derivation and badge); old-head CI handling; review comparison (derivation and badge); mergeability (derivation + blocker); freshness/staleness (derivation + blocker + `FROZEN_STALE` state); 18-state precedence; "old approval never applies to new head"; next permitted action; display badges; unknown/partial handling; arbitrary-runtime rendering safety; PR mutation controls (correctly absent).
- **NOT_IMPLEMENTED — the genuine remaining gap (1)**: base comparison lacks the always-visible confirmation badge that head, CI, and review comparisons already have. It is currently visible only via the blockers list, i.e. only when it is `DRIFT` — never confirmed as `MATCH`.
- **OUT_OF_N7_E_SCOPE (2)**: a dedicated mergeability badge and a dedicated freshness badge distinct from existing timestamps/blockers — neither is required by the original document's "Always Visible Without Expansion" list.
- **NOT_IMPLEMENTED — explicitly out of scope (remaining rows)**: extension wiring, `.claw/n7` reads, live refresh execution, freeze-operation execution, timeline rendering, PR mutation — all excluded by the original document's own Non-Goals and by every prior N7 lane's STOP gate.

## 6. Exact N7-E Objective

Add one new field to the existing N7-D view model and one new rendered row
to the existing N7-D card, both following the exact pattern already proven
safe by the P2 head-comparison repair:

1. `N7PrCardComparisonView` gains `baseComparison: BaseComparison` (the
   existing N7-A type, imported — never redefined), copied directly from
   `input.derived.comparison.baseComparison`.
2. `render.ts` gains one runtime-safe normalizer,
   `normalizeN7BaseComparison(value: unknown)`, structurally identical to
   the existing `normalizeN7HeadComparison`, and one rendering function,
   `n7BaseComparisonBlock(baseComparison: unknown)`, structurally identical
   to `n7HeadComparisonBlock`.
3. The new row renders unconditionally, immediately after the existing head-
   comparison row, using fixed copy: `"Base comparison: MATCH — current base
   equals frozen base"` / `"DRIFT — current base differs from frozen base"`
   / `"UNKNOWN — current or frozen base is unavailable"`.

No other behavior changes. This is the entire N7-E objective.

## 7. Non-Goals

N7-E does not:

- perform GitHub network reads;
- perform evidence-store writes;
- perform freeze operations;
- execute live refresh;
- render the evidence timeline (N7-F);
- implement N7-F in any form;
- wire any N7 module into `extension.ts` (a separate, later, explicitly-authorized lane);
- add, remove, or alter any PR mutation control;
- add, remove, or alter any package-rung control;
- invoke Claw, helper, broker, or Ollama;
- change approval-token grammar or handling in any way;
- perform any cleanup of prior worktrees, branches, or PRs;
- add a mergeability badge or a freshness badge (see §5 — out of scope);
- re-derive, duplicate, or override any N7-A comparison or precedence value;
- introduce a second comparison state machine or a second identity validator.

## 8. Data Flow

```text
validated live snapshot (PrLiveSnapshot, from n7Schemas.ts — unchanged)
+ validated frozen snapshot (FrozenReviewSnapshot, from n7Schemas.ts — unchanged)
+ N7-A derived comparison (LiveFrozenComparison, from n7State.ts's
  deriveLiveFrozenComparison() — unchanged; baseComparison already exists
  in this structure today)
+ N7-D identity-safe card model (buildN7PrCardView() in n7View.ts — extended
  by exactly one new field copy)
→ N7-E comparison presentation (one new normalizer + one new render function
  in render.ts, following the existing head-comparison pattern exactly)
→ read-only badge row (data-testid="n7-base-comparison")
```

No value in this flow is rederived, recomputed from SHA strings, or
inferred from primary state. `input.derived.comparison.baseComparison` is
read exactly once, at the same call site and in the same manner as
`headComparison` already is.

## 9. Interface Contracts

### `N7PrCardComparisonView` (extended, in `n7View.ts`)

```ts
export interface N7PrCardComparisonView {
  primaryState: N7PrimaryState;
  severity: N7Severity;
  headComparison: HeadComparison;   // existing, unchanged
  baseComparison: BaseComparison;   // NEW — imported from n7State.ts, never redefined
  exactLabel: string;
  detail: string;
}
```

### `normalizeN7BaseComparison` (new, in `render.ts`)

Every externally constructible runtime field carries these four rules,
matching the existing `normalizeN7HeadComparison` exactly:

| Rule | Value |
|---|---|
| Accepted values | `"MATCH"`, `"DRIFT"`, `"UNKNOWN"` — exact-match `switch`, no `toLowerCase()`/`trim()`/coercion |
| Invalid default | Any other runtime value (including non-string, `null`, `undefined`, objects, arrays) maps to the fixed `"UNKNOWN"` result |
| Structural-attribute rule | The returned value never enters any `data-testid`, `class`, `id`, `style`, `href`, or `src` — it selects only which of three fixed copy strings is interpolated into the text-node body of `<p data-testid="n7-base-comparison">`, an already-hardcoded literal attribute |
| Visible-copy rule | Fixed copy only, always escaped via the existing shared `escapeHtml()`: `"MATCH — current base equals frozen base"` / `"DRIFT — current base differs from frozen base"` / `"UNKNOWN — current or frozen base is unavailable"` — never the raw input value, never a model-provided label |

```ts
interface TrustedN7BaseComparison {
  value: "MATCH" | "DRIFT" | "UNKNOWN";
  copy: string;
}

function normalizeN7BaseComparison(value: unknown): TrustedN7BaseComparison {
  switch (value) {
    case "MATCH":
      return { value: "MATCH", copy: "MATCH — current base equals frozen base" };
    case "DRIFT":
      return { value: "DRIFT", copy: "DRIFT — current base differs from frozen base" };
    case "UNKNOWN":
      return { value: "UNKNOWN", copy: "UNKNOWN — current or frozen base is unavailable" };
    default:
      return { value: "UNKNOWN", copy: "UNKNOWN — current or frozen base is unavailable" };
  }
}

function n7BaseComparisonBlock(baseComparison: unknown): string {
  const trusted = normalizeN7BaseComparison(baseComparison);
  return `  <p data-testid="n7-base-comparison">Base comparison: ${escapeHtml(trusted.copy)}</p>`;
}
```

## 10. Badge/Row Inventory

| Surface | Already exists (always-visible) | N7-E adds/restructures |
|---|---|---|
| Head comparison | YES — `data-testid="n7-head-comparison"`, N7-D P2 repair | none |
| Base comparison | NO — blocker-only visibility | **adds** `data-testid="n7-base-comparison"`, same pattern |
| CI correlation | YES — `data-testid="n7-ci"` / `n7-ci-summary` | none |
| Review comparison | YES — `data-testid="n7-review"` / `n7-review-summary` | none |
| Freshness | Partial — timestamps only (`n7-current-captured-at`, `n7-frozen-captured-at`); no dedicated FRESH/STALE badge | none (out of scope, §5) |
| Mergeability | Partial — blocker-only visibility (`MERGE_CONFLICT` code) | none (out of scope, §5) |
| Blocker summary | YES — `data-testid="n7-blockers"` / index-only rows | none |
| Overall primary state | YES — `n7-severity-prefix` / `n7-state-detail` | none |
| Next permitted action | YES — `data-testid="n7-next-action"` | none |

## 11. State and Precedence Ownership

N7-A (`src/n7State.ts`) remains the sole authority for every comparison
value and every precedence decision. N7-E adds no new precedence rule and
recomputes nothing.

Worked examples proving independent visibility under a higher-precedence
primary state (all confirmed live in this session against the current
implementation, before any N7-E change):

- With `headComparison = "DRIFT"` and `baseComparison = "DRIFT"`
  simultaneously, `primaryState` resolves to `BASE_DRIFT`'s higher-ranked
  sibling per the precedence table (`HEAD_DRIFT`, rank 7, before
  `BASE_DRIFT`, rank 8) — yet the blockers list (evaluated independently of
  which condition won precedence) already contains **both** `HEAD_DRIFT` and
  `BASE_DRIFT` entries. `buildBlockers()` in `n7View.ts` evaluates every
  comparison dimension unconditionally, never gating on which one is
  primary.
- Live-probed: with `baseComparison = "DRIFT"` and a failing required check
  simultaneously, the resulting `view.blockers` array contained
  `BASE_DRIFT`, `CI_FAILED`, and `STATE_BLOCKING_REASON` entries together,
  confirming blocker-level independence already holds today. N7-E's badge
  addition gives base comparison the same treatment in its **positive**
  case (`MATCH`) that it already effectively has in its negative case
  (`DRIFT`, via the blocker list).

## 12. Identity and Evidence Binding

N7-E inherits every N7-D identity rule unchanged — it does not touch
`assertN7PrCardIdentity()`, `normalizeRequestedPrNumber()`, or any of the 7
bounded `N7PrCardIdentityMismatchError` reason codes:

- repository identity (owner + name, exact string equality) is enforced
  before any card is constructed;
- PR-number identity (requested/live/frozen agreement) is enforced the same
  way;
- requested PR identity is preserved as a display fallback only when no
  snapshot supplies one, and only after passing identity validation;
- live/frozen agreement is required whenever both are present;
- matching head/base SHAs are never treated as identity proof — identity
  validation never inspects SHAs at all, and this remains true after N7-E;
- old approval cannot bind a new head — enforced by N7-A's `HEAD_DRIFT`
  precedence (rank 7), unaffected by N7-E.

## 13. Unknown and Partial Data

N7-E's new base-comparison badge fails closed identically to the existing
head-comparison badge for every listed condition:

- absent snapshots → `baseComparison` is already `"UNKNOWN"` from N7-A's
  `deriveBaseComparison()` (returns `"UNKNOWN"` when either `live` or
  `frozen` is `null`, or when either base SHA is an empty string) → renders
  `"UNKNOWN — current or frozen base is unavailable"`;
- incomplete review pagination → unaffected; N7-E does not touch review
  logic;
- missing CI correlation → unaffected; N7-E does not touch CI logic;
- stale live snapshot → unaffected; base comparison is independent of
  freshness, exactly as head comparison already is;
- missing frozen evidence → covered by the absent-snapshot case above;
- identity mismatch → `assertN7PrCardIdentity()` throws before
  `buildN7PrCardView()` constructs any view model at all, so no base-
  comparison badge (or any other field) is ever reached;
- arbitrary runtime view objects → `normalizeN7BaseComparison(value:
  unknown)` accepts any runtime value and always resolves to one of the
  three fixed literals, exactly like `normalizeN7HeadComparison`.

## 14. Exact Proposed Files

Reconciled against the original "likely files" list in §4 — **no new file
is proposed**. `n7Comparison.ts` provides no additional value: `n7State.ts`
already computes `baseComparison`, and `n7View.ts`/`render.ts` already carry
the exact projection/rendering pattern this slice extends. Forcing a new
module would duplicate existing logic and add an unjustified fourth file.

Exactly two production files and one test file — within the maximum
three-file budget, with no fourth file needed:

1. **`ide/vscode/a2-harness-panel/src/n7View.ts`** — add
   `baseComparison: BaseComparison` to `N7PrCardComparisonView`; import
   `BaseComparison` from `./n7State` alongside the existing
   `HeadComparison` import; copy `comparison.baseComparison` into the
   `comparison` object literal returned by `buildN7PrCardView()`, adjacent
   to the existing `headComparison: comparison.headComparison,` line.
2. **`ide/vscode/a2-harness-panel/src/render.ts`** — add
   `normalizeN7BaseComparison()` and `n7BaseComparisonBlock()` immediately
   after the existing `normalizeN7HeadComparison()`/`n7HeadComparisonBlock()`
   pair; splice `${n7BaseComparisonBlock(view.comparison.baseComparison)}`
   immediately after `${n7HeadComparisonBlock(view.comparison.headComparison)}`
   in `n7CardBlock()`'s returned template.
3. **`ide/vscode/a2-harness-panel/test/n7Render.test.ts`** — add the
   acceptance tests in §15, mirroring the existing "P2 explicit head
   comparison" describe block's structure and helper reuse
   (`renderWithN7`, `buildCard`, `renderArbitrary`, `extractN7Section`,
   `makeLive`, `makeFrozen`).

No package or lock change. No extension wiring in this or any authorized
follow-on lane unless separately authorized in a dedicated prompt.

## 15. Acceptance-Test Matrix

```text
head_match_remains_visible_when_ci_failed
head_match_remains_visible_when_review_blocked
head_drift_has_precedence_over_old_approval
base_drift_remains_independently_visible
old_head_ci_success_is_not_current_success
stale_live_snapshot_is_not_clean
review_blocker_remains_visible_under_other_primary_state
partial_review_data_is_unknown
missing_frozen_snapshot_does_not_claim_match
requested_pr_without_snapshots_remains_live_unchecked
identity_mismatch_never_returns_comparison_view
matching_sha_different_pr_never_matches
comparison_badges_do_not_authorize_write
invalid_prebuilt_badge_value_maps_to_unknown
comparison_values_do_not_enter_structural_attributes
```

The 15 identifiers above are normative **acceptance-behavior IDs**. They are
not a requirement to create 15 duplicate Mocha `it()` names. Each behavior
must be covered by the exact existing, new, or extended test mapping in the
table below. Rows marked `EXISTING_COVERAGE` require regression execution
only — no test-code change. Rows marked `NEW_TEST` or `EXTEND_EXISTING_TEST`
define the exact, and only, implementation-lane test changes.

Every mapping below was verified by direct inspection of
`ide/vscode/a2-harness-panel/test/n7Render.test.ts` and
`ide/vscode/a2-harness-panel/test/n7State.test.ts` at this scope's base SHA
— no cited test name is invented, and exact literal strings (including
non-snake-case descriptive titles and parenthetical suffixes) are quoted
exactly as they appear in source.

| # | Behavior ID | Status | Exact mapping |
|---|---|---|---|
| 1 | `head_match_remains_visible_when_ci_failed` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("ci_failed_primary_still_renders_head_match", ...)` |
| 2 | `head_match_remains_visible_when_review_blocked` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("review_blocked_primary_still_renders_head_match", ...)` |
| 3 | `head_drift_has_precedence_over_old_approval` | `EXISTING_COVERAGE` | `n7State.test.ts` — `it("head_drift_outranks_ci_success", ...)` and `it("current_head_change_invalidates_prior_merge_approval (HEAD_DRIFT)", ...)` (exact literal includes the `(HEAD_DRIFT)` suffix) |
| 4 | `base_drift_remains_independently_visible` | `NEW_TEST` | `n7Render.test.ts` — `it("base_drift_remains_independently_visible", ...)`: construct simultaneous base drift + a higher-precedence blocking condition (e.g. a failing required check); assert `view.blockers` contains a `BASE_DRIFT` entry AND the rendered `n7-base-comparison` row shows `DRIFT`, even though the primary state is `CI_FAILED` |
| 5 | `old_head_ci_success_is_not_current_success` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("old_head_ci_is_not_described_as_current_success", ...)` |
| 6 | `stale_live_snapshot_is_not_clean` | `EXISTING_COVERAGE` | `n7State.test.ts` — `it("FROZEN_STALE when a live refresh exists but is older than the freshness policy", ...)` (exact literal descriptive string, not snake_case) |
| 7 | `review_blocker_remains_visible_under_other_primary_state` | `EXTEND_EXISTING_TEST` | Extend `n7Render.test.ts`'s `it("review_blockers_are_visible", ...)` to additionally construct a simultaneous head-drift scenario and assert the `REVIEW_BLOCKED` blocker entry still appears in `view.blockers` even though `HEAD_DRIFT` is the resulting `primaryState` |
| 8 | `partial_review_data_is_unknown` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("partial_review_data_is_unknown_not_clear", ...)` |
| 9 | `missing_frozen_snapshot_does_not_claim_match` | `EXTEND_EXISTING_TEST` | Extend `n7Render.test.ts`'s `it("frozen_only_matching_identity_builds_card", ...)` (currently asserts only `assert.ok(view)`) to additionally assert `assert.notStrictEqual(view.comparison.primaryState, "FROZEN_MATCH")` |
| 10 | `requested_pr_without_snapshots_remains_live_unchecked` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("requested_pr_number_is_preserved_in_live_unchecked_model", ...)` (already asserts `primaryState === "LIVE_UNCHECKED"` exactly) |
| 11 | `identity_mismatch_never_returns_comparison_view` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("identity_mismatch_never_returns_frozen_match", ...)` and `it("identity_mismatch_never_returns_ready_clean", ...)`; both already assert the returned `view` itself is `undefined` after a caught throw. Since no view object is ever constructed, no field on it — present or future, including `baseComparison` — can ever be reached. No extension is meaningful or required. |
| 12 | `matching_sha_different_pr_never_matches` | `EXISTING_COVERAGE` | `n7Render.test.ts` — `it("same_heads_different_repository_fails_closed", ...)` and `it("same_heads_same_repository_different_pr_number_fails_closed", ...)` |
| 13 | `comparison_badges_do_not_authorize_write` | `EXISTING_COVERAGE` | `n7Render.test.ts`'s `describe("n7 render — no-write boundary", ...)` block (`n7_card_has_no_pr_mutation_controls`, `n7_card_has_no_package_rung_controls`, `n7_card_has_no_write_message_dispatch`, `n7_card_has_no_merge_ready_approve_or_rerun_action`) already call `extractN7Section(html)` and scan the **entire** card body for forbidden patterns — this already covers any row present in that section, including the future `n7-base-comparison` row, with zero test-code change |
| 14 | `invalid_prebuilt_badge_value_maps_to_unknown` | `NEW_TEST` | `n7Render.test.ts` — `it("invalid_prebuilt_base_comparison_maps_to_unknown", ...)`, mirroring the existing `invalid_prebuilt_head_comparison_maps_to_unknown` structure and adversarial payload set exactly (`"MATCH extra-class"`, `"\"><script>alert(1)</script>"`, `"match"`, `null`, `undefined`, `42`, `{}`, `[]`) |
| 15 | `comparison_values_do_not_enter_structural_attributes` | `NEW_TEST` | `n7Render.test.ts` — `it("base_comparison_is_not_color_only", ...)`, mirroring the existing `head_comparison_is_not_color_only` structure exactly |

Exact future test-change budget (not approximate):

- **`NEW_TEST` (3)**: `base_drift_remains_independently_visible`,
  `invalid_prebuilt_base_comparison_maps_to_unknown`,
  `base_comparison_is_not_color_only`.
- **`EXTEND_EXISTING_TEST` (2)**: `review_blockers_are_visible` (add a
  head-drift-simultaneous assertion); `frozen_only_matching_identity_builds_card`
  (add a `primaryState !== "FROZEN_MATCH"` assertion).
- **`EXISTING_COVERAGE` (10)**: rows 1, 2, 3, 5, 6, 8, 10, 11, 12, 13 —
  regression execution only, no test-code change, no duplicate alias tests.

Total exact test-code changes: 3 new `it()` blocks + 2 extended `it()`
blocks = 5.

## 16. Security and Accessibility

Frozen, inherited unchanged from the N7-D P2 pattern:

- fixed/index-only selectors: `data-testid="n7-base-comparison"` is itself
  a hardcoded literal (not index-based, since there is exactly one such row
  per card — matching `n7-head-comparison`'s own non-indexed pattern);
- runtime mapping to fixed literals: `normalizeN7BaseComparison` is the
  single place that inspects `comparison.baseComparison`'s runtime value;
- contextual escaping: the interpolated copy string is always passed
  through the existing shared `escapeHtml()`, exactly as
  `n7HeadComparisonBlock` already does;
- visible text independent of color: the fixed copy always begins with the
  literal word `MATCH`/`DRIFT`/`UNKNOWN`, never relying on a CSS class or
  color alone;
- heading/order requirement: the new row renders immediately after
  `n7-head-comparison` and before `n7-comparison-detail`, preserving the
  existing "after identities, before CI/review" ordering guarantee;
- no write/control surface: the new row is a single `<p>` element with no
  button, form, `data-ui-action`, or `postMessage` — identical in kind to
  every existing N7 card row.

## 17. Validation Plan

Repository-root commands (`scripts/check-harness-exec-safety.sh` and
`.github/scripts/check_doc_source_of_truth.py`) and panel-package commands
(`npm test`, `npx tsc`, `npx mocha`, `npm run lint`) resolve relative to
different roots. Neither group's success may depend on an implicit shell
working directory left over from the other group — each group explicitly
sets its own working directory before running.

```bash
REPO_ROOT=/mnt/vast-data/git-worktrees/<n7e-implementation-worktree>
PANEL_ROOT="$REPO_ROOT/ide/vscode/a2-harness-panel"

# --- panel-package commands: run from PANEL_ROOT ---------------------------
(
  cd "$PANEL_ROOT"

  # focused tests
  npx mocha --reporter min --recursive ./out-test/test --grep "P2 explicit head comparison"
  npx mocha --reporter min --recursive ./out-test/test --grep "base comparison"
  npx mocha --reporter min --recursive ./out-test/test --grep "n7 identity binding"
  npx mocha --reporter min --recursive ./out-test/test --grep "n7 render"

  # full panel suite
  npm test

  # source typecheck
  npx tsc -p . --noEmit

  # test typecheck
  npx tsc -p ./tsconfig.test.json --noEmit

  # panel guard
  npm run lint
)

# --- repository-root guards: run from REPO_ROOT -----------------------------
(
  cd "$REPO_ROOT"

  # harness execution-safety guard
  bash scripts/check-harness-exec-safety.sh

  # documentation source-of-truth guard
  python3 .github/scripts/check_doc_source_of_truth.py
)
```

Required minimums for the future implementation lane: all 1114 previously
passing tests remain passing; new base-comparison tests pass; final count
exceeds 1114; zero failures; both guards pass; `render.ts`/`n7View.ts`
diffs are scoped to exactly the additions described in §14.

## 18. STOP Gates

- no duplicate N7-A comparison logic — `baseComparison` is read from
  `input.derived.comparison.baseComparison` exactly once, never recomputed;
- no old approval on a new head — unaffected; N7-A's `HEAD_DRIFT` precedence
  is untouched;
- no identity mismatch rendered as usable — unaffected;
  `assertN7PrCardIdentity()` still throws before any card (including the
  new base-comparison field) is constructed;
- no old-head CI shown as current success — unaffected; N7-E does not touch
  CI logic;
- no partial data shown clean — the new badge's `UNKNOWN` default covers
  every non-`MATCH`/`DRIFT` runtime value;
- no model value in structural attributes — `normalizeN7BaseComparison`'s
  output is restricted to 3 fixed literals, matching the head-comparison
  pattern exactly;
- no PR/package mutation controls — the new row is a plain `<p>`, nothing
  else;
- no scope drift — implementation must touch exactly the 3 files in §14,
  no `n7Comparison.ts`, no `n7State.ts`/`n7Schemas.ts` change, no extension
  wiring.

## 19. Exit Criteria

N7-E is complete when:

1. `N7PrCardComparisonView.baseComparison` exists and is populated directly
   from N7-A's derived result.
2. A dedicated `data-testid="n7-base-comparison"` row renders unconditionally,
   with fixed MATCH/DRIFT/UNKNOWN copy, positioned after the existing head-
   comparison row.
3. All 15 acceptance behaviors in §15 are covered and passing through the
   exact existing, new, or extended test mappings recorded in the matrix.
   Only rows marked `NEW_TEST` or `EXTEND_EXISTING_TEST` require test-code
   changes (exactly 3 new `it()` blocks and 2 extended `it()` blocks, per
   §15's exact future test-change budget); duplicate alias tests for
   `EXISTING_COVERAGE` rows are not required. No acceptance behavior is
   satisfied only by prose.
4. The full panel suite exceeds its current 1114-passing baseline with zero
   failures.
5. Both repository guards (harness execution-safety, documentation
   source-of-truth) pass.
6. The implementation diff is scoped to exactly the 3 files named in §14 —
   verified via `git diff --name-only` against the base SHA recorded in §2.
7. No STOP gate in §18 is violated.

## 20. Exact Next Implementation Lane

- **Name**: N7-E Base Comparison Badge Parity — Implementation
- **Recommended tool**: Claude Code
- Not authorized by this document. A separate exact activation lane, quoting
  this document's §6, §9, §14, and §15 verbatim, is required before any
  code is written.
