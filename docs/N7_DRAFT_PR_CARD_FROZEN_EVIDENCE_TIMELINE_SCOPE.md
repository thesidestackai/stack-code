# N7 Draft PR Card and Frozen Evidence Timeline Scope

> Docs-only scope. This document designs the next read-only Stack-Code
> operator surface after the accepted N6 package-plan path. It implements
> nothing, calls no GitHub API, invokes no Claw/helper/broker/Ollama process,
> changes no approval grammar or token handling, opens no PR, stages nothing,
> commits nothing, and mutates no runtime state.
>
> This scope document does not authorize implementation.
> This scope document does not authorize execution.
> Future implementation requires a separate exact activation lane.

---

## Executive Summary

N6 proved the operator path:

```text
panel action
-> Stack-Code caller
-> canonical Claw selection
-> broker :11435
-> qwen3:14b
-> read-only package-plan completion
-> bounded .claw receipts
-> no forbidden downstream mutation
```

The next operator gap is not another execution button. It is a read-only,
evidence-integrity surface that makes the post-PR review boundary explicit:

```text
Draft PR Card
+ Frozen Evidence Timeline
+ Live Refresh
+ Approval Gate comparison
```

N7 must let an operator answer, without inference-by-hope:

```text
What exact PR am I reviewing?
What immutable head SHA was reviewed?
Which files changed?
What CI and review state applied to that exact head?
What evidence existed before and after each workflow rung?
Has any evidence, head, branch, or result changed since it was frozen?
What action is permitted next?
What action is explicitly not permitted?
```

The central design choice is separation:

- live mutable PR state is a current observation;
- frozen reviewed state is an immutable local evidence snapshot;
- comparison state is derived and must expose its inputs;
- next action guidance is read-only and must never advance a workflow rung.

The recommended first implementation slice is **N7-A: schemas and pure state
model**. It should add only versioned schemas, canonical serialization rules,
and pure live-vs-frozen state transitions. It must not call GitHub, render the
panel, write storage, invoke Claw, or run package rungs.

---

## Problem

The Stack-Code operator can now execute the bounded N6 package ladder with
sub-token-gated controls, but the panel still lacks a trust surface for the
PR-review phase. Current UI and evidence surfaces can show package rung output
and a session-local action timeline, but they do not preserve a durable,
hash-verifiable record of what exact PR head, CI, review, files, and evidence
were reviewed at the moment an operator froze review evidence.

The unsafe failure mode is subtle: the operator sees a green or familiar status
and assumes it still applies to the current PR head. N7 exists to prevent that.
Old approval for head `A` must never silently apply to new head `B`; green CI
for an obsolete commit must not clear a current head; missing review-thread data
must render as `UNKNOWN`, not "no blockers".

---

## Current State

Verified base for this scope:

```text
repository: /home/suki/stack-code
scope worktree: /mnt/vast-data/git-worktrees/stack-code-n7-pr-card-evidence-scope-20260717_105835
branch: docs/n7-pr-card-evidence-timeline-scope-20260717_105835
base: origin/main @ 4a4bbd8d09a3adf8a24ed2d476b2f0511539a137
```

The local `stack-code-northstar-gap-audit-20260715_122228` worktree exists but
is behind the verified `origin/main` by two commits, so this N7 scope uses the
fresh `origin/main` worktree as source of truth.

### Source Map

| Component | Current responsibility | Relevant file | Current trust boundary | Current limitation | N7 relevance |
|---|---|---|---|---|---|
| Northstar ladder | Pure read-only ladder from workspace through draft PR, evidence freeze, disposition, and human merge pending. | `ide/vscode/a2-harness-panel/src/northstarState.ts` | Signals only; no fs/spawn/network; human-gated states require observed signals. | `DRAFT_PR_OPEN` and `EVIDENCE_FROZEN` are booleans; no PR identity, frozen snapshot ID, head SHA, CI, or review state. | N7 should refine post-`DRAFT_PR_OPEN` with exact PR and evidence identity without weakening ladder gates. |
| Workspace status card | Honest workspace/branch/cleanliness/freshness display. | `ide/vscode/a2-harness-panel/src/workspaceStatus.ts` | Pure model; unknown when no guard-safe probe exists. | Git facts can be `unknown`; no PR metadata. | N7 should keep the same honesty rule for missing PR facts. |
| Setup status | Helper/workspace/plan/artifact status from one-shot probes. | `ide/vscode/a2-harness-panel/src/setupStatus.ts` | Helper probe plus parsed audit; no green-by-default. | Setup facts stop at local chain artifacts. | N7 should reuse the positive/negative/not-checked posture. |
| N5 readiness board | Read-only package-ladder readiness from N3/N4 state. | `ide/vscode/a2-harness-panel/src/n5ReadinessModel.ts`, `src/n5State.ts`, `src/n5View.ts` | Pure; labels `VERIFIED`, `MISSING`, `BLOCKED`, `EXECUTION_REQUIRED`; N5 never runs rungs. | Commit/push/PR are execution-required and not independently verified in N5. | N7 should consume observed package/PR events rather than infer them from readiness. |
| N6 execution state | Sub-token-gated per-rung state and output capture. | `ide/vscode/a2-harness-panel/src/n6State.ts`, `src/n6View.ts`, `src/extension.ts` | In-memory sub-tokens; helper dispatch only through `helperRunner.ts`; no auto-advance; failure clears token. | Captured outputs are session state; no durable frozen PR/evidence model. | N7 should record read-only observations after rungs without adding new package execution. |
| Helper runner | Single spawn boundary and argv allowlist. | `ide/vscode/a2-harness-panel/src/helperRunner.ts` | Array argv only; allowlisted subcommands/flags; refuses chain-write-shaped values. | `package-pr` is write-capable draft creation; no read-only GitHub adapter exists. | N7 must introduce a separate read-only GitHub reader boundary, not extend write-capable package-pr. |
| IDE harness shell | Bounded helper implementation for print/validate plus N6 package rungs. | `scripts/a2-ide-harness.sh` | Package-plan is preview-only; package-commit exact-path; package-push non-force; package-pr draft-only; no merge. | `package-pr` creates a draft PR; it does not build an operator review card or durable freeze. | N7 may read its output as evidence but must not invoke it. |
| Session timeline | Ordered session-local action lines. | `ide/vscode/a2-harness-panel/src/evidence.ts`, `src/render.ts` | Pure append-with-cap in memory; render-only; records printed vs run. | No timestamps, no hashes, no storage, capped at 200, and no artifact identity. | N7 needs a durable hash-linked append-only evidence timeline. |
| Tier-3 evidence snapshot | Parse/render `a2-tier3-evidence-snapshot.v0`. | `ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts` | Snapshot text is sole input; fail-closed on bad schema; unknowns visible. | Snapshot is Tier-3/local-chain evidence, not PR/CI/review state. | N7 should reuse the fail-closed schema-version posture. |
| Tier-4 PR packaging proof | Terminal-oriented proof that one real draft PR can be opened under separate gates. | `docs/a2-tier4-package-pr-live-smoke-readiness.md`, `handoffs/a2_tier4_lane_b_live_package_pr_smoke_closeout_2026-06-15.md`, `scripts/a2-tier3-write-orchestrator.sh` | Live package-pr proof is gated, draft-only, and terminal/operator controlled. | Evidence remains in handoffs/terminal output; not projected into panel as live/frozen review state. | N7 is the panel-side read-only review surface after a draft PR exists. |
| Static guards | Structural no-network/no-fs/no-spawn/no-write checks for panel and shell helper safety. | `ide/vscode/a2-harness-panel/scripts/run-guards.js`, `scripts/check-harness-exec-safety.sh` | Fail on forbidden source patterns and unsafe helper surfaces. | No N7 GitHub read-only/no-write guard exists yet. | N7 must add tests/guards proving write methods are unreachable from the reader. |
| Current tests | Unit/render/guard coverage for N5/N6, evidence timeline, Tier-3 snapshot, Northstar safety. | `ide/vscode/a2-harness-panel/test/*.test.ts` | Pure tests and static guards. | No tests for PR identity, exact-head CI correlation, live-vs-frozen drift, or hash chain verification. | N7 acceptance depends on new pure model, schema, hash, reader, render, and no-write tests. |

### Prior Gap Findings

Directly supported findings from current repository docs:

- The N1 Northstar gap scope requires `package-pr` to remain gated and requires
  draft PR verification to display independently verified PR state:
  URL, draft state, merged status, base/head, and review decision.
- The same N1 matrix calls for an in-product evidence timeline with per-step
  evidence records. Older wording placed this under Phase N6, but N6 later
  became the execution boundary.
- The N4 scope explicitly deferred PR card and evidence freeze/persistence.
- The N5 scope noted the draft PR card plus frozen evidence timeline was
  resequenced to a later N7+ lane.
- The N6 execution-boundary scope explicitly excluded N7+ behavior and listed
  evidence freeze to disk and PR lifecycle features as future candidates.
- Current `origin/main` includes N6 execution controls and N6A helper allowlist,
  but no read-only PR review card, no GitHub live reader, no exact-head CI
  correlation, and no persistent hash-linked timeline.

What N7 must solve:

- exact PR identity and exact head identity visibility;
- frozen reviewed state distinct from live mutable state;
- exact-head CI/review/file provenance;
- drift detection after freeze;
- append-only, hash-verifiable evidence events;
- read-only GitHub boundary with no write surface;
- STOP gates for unknown, stale, missing, or mismatched evidence;
- next-action guidance that does not auto-advance any rung.

What N7 must not solve:

- creating, editing, marking ready, approving, merging, closing, or deleting PRs;
- running package-plan, package-commit, package-push, or package-pr;
- altering approval grammar or token strings;
- invoking Claw, the helper, broker `:11435`, raw Ollama `:11434`, or runtime services;
- adding a database without a proven need;
- hiding missing GitHub data behind a clean status.

---

## Goals

1. Render a Draft PR Card that shows exact identities rather than generic state.
2. Render a Frozen Review Snapshot that remains visible after live refresh.
3. Render a Live vs Frozen Comparison that makes match/drift/blockers obvious.
4. Render a Frozen Evidence Timeline that is append-only from the UI contract
   and hash-verifiable from disk.
5. Classify every statement as `VERIFIED`, `INFERRED`,
   `OPERATOR_ASSERTED`, or `UNKNOWN`.
6. Correlate CI/check/review facts to an exact PR head SHA.
7. Treat missing data as `UNKNOWN` or `STOP`, never clean.
8. Keep all PR lifecycle writes unreachable in N7.
9. Preserve the approval-token and stop-gate philosophy.
10. Provide a surgical implementation sequence with pure models first.

---

## Non-Goals

N7 does not:

- implement panel code in this scope;
- call GitHub in this scope;
- create or mutate PRs;
- mark a PR ready for review;
- request reviewers;
- resolve review threads;
- rerun workflows;
- enable auto-merge;
- merge, close, or delete branches;
- run Claw or helper subcommands;
- alter N6 package rung behavior;
- change approval tokens or grammar;
- persist secrets, prompt text, authorization headers, cookies, or raw tokens;
- claim cryptographic immutability beyond a hash-linked local evidence contract.

---

## Trust Model

### Product Boundary

N7 separates four concepts that must not collapse into one mutable status object:

| Surface | Purpose | Source | Mutability |
|---|---|---|---|
| Draft PR Card | Compact operator projection of a PR and trust state. | Latest live snapshot plus frozen snapshot plus derived comparison. | Live projection may change after refresh. |
| Frozen Evidence Timeline | Historical event sequence and artifact references. | Local append-only evidence events. | Append-only from UI; prior events never rewritten. |
| Live Refresh | Explicit operator action to obtain current PR state. | Future read-only GitHub reader and local git observations. | Appends `PR_LIVE_REFRESH`; does not rewrite freeze. |
| Approval Gate | Comparison of frozen approved state to latest live state. | Derived from frozen snapshot and live snapshot. | Recomputed; never grants mutation authority. |

### Provenance Enum

Every card field and evidence item must carry one of:

```text
GITHUB_LIVE          captured from a live read-only GitHub response
LOCAL_GIT            captured from local git read-only commands or library calls
FROZEN_EVIDENCE      read from an existing frozen snapshot/event with verified hash
DERIVED_COMPARISON   deterministic comparison of cited facts
OPERATOR_ASSERTION   typed or selected by the operator; not independently verified
UNKNOWN_NOT_CHECKED  missing, partial, stale, inaccessible, or intentionally unqueried
```

No UI copy may imply live verification when the field came from
`FROZEN_EVIDENCE` or `OPERATOR_ASSERTION`.

### Evidence Classification

| Class | Rule | Examples |
|---|---|---|
| `VERIFIED` | Established by a captured source whose identity, timestamp, and hash/response metadata are recorded, and whose schema/version is supported. | GitHub live PR head SHA response; local `git rev-parse HEAD`; verified frozen event hash; artifact SHA-256 match. |
| `INFERRED` | Deterministic conclusion from cited `VERIFIED` or `UNKNOWN` facts. It must list supporting fact/event IDs. | `HEAD_DRIFT` because live head SHA differs from frozen reviewed head SHA. |
| `OPERATOR_ASSERTED` | Operator-entered note, decision, or external review assertion. It is preserved but never promoted to verified. | "Reviewed by Suki in browser"; "merge approval captured"; operator note. |
| `UNKNOWN` | Not checked, unavailable, partial, stale, schema-mismatched, unauthorized, rate-limited, or unsupported. | Review threads page unavailable; mergeability pending; check response missing head SHA. |

Contradictions are first-class:

- show both conflicting facts with timestamps and provenance;
- derive a `STOP` blocker until a newer verified source resolves the conflict;
- do not delete or edit the older event;
- newer evidence may supersede operational decisions, but old evidence remains
  visible and hash-verifiable.

Example classification from the N6 smoke process-monitor gap:

```text
Broker and receipt execution:
  VERIFIED

Intermediate PID chain in that specific run:
  UNKNOWN

Complete route conclusion using prior and current evidence:
  INFERRED with supporting event references
```

---

## Draft PR Card Contract

The Draft PR Card is a compact projection. It is not the source of truth. It
must cite live snapshot IDs, frozen snapshot IDs, and derived comparison IDs.

### Required Fields

| Field | Meaning | Required provenance | Unknown behavior |
|---|---|---|---|
| `repository` | Canonical repository owner/name and URL. | `GITHUB_LIVE`, `LOCAL_GIT`, or `FROZEN_EVIDENCE`. | Show `UNKNOWN`; no freeze. |
| `pr_number` | Numeric PR identifier. | `GITHUB_LIVE`, `FROZEN_EVIDENCE`, or `OPERATOR_ASSERTION` before refresh. | State `NO_PR` or `LIVE_UNCHECKED`. |
| `pr_url` | Canonical PR URL. | `GITHUB_LIVE`, `FROZEN_EVIDENCE`, or `OPERATOR_ASSERTION` before refresh. | No clickable card action beyond entering/refreshing PR. |
| `title` | PR title. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`, not blank clean. |
| `state` | Open/closed/merged or provider enum. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; mutation forbidden. |
| `draft_status` | Whether PR is draft. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; ready/merge guidance forbidden. |
| `base_branch` | Target branch name. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; freeze forbidden. |
| `base_sha` | Exact base commit SHA at capture. | `GITHUB_LIVE`, `LOCAL_GIT`, or `FROZEN_EVIDENCE`. | `UNKNOWN`; base drift cannot be cleared. |
| `head_branch` | Source branch/ref. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; no approval gate. |
| `current_head_sha` | Latest live PR head SHA from refresh. | `GITHUB_LIVE`; never inferred. | `UNKNOWN`; state `LIVE_UNCHECKED` or `LIVE_FETCH_FAILED`. |
| `frozen_reviewed_head_sha` | Head SHA reviewed when evidence was frozen. | `FROZEN_EVIDENCE`. | `UNKNOWN`; no approval applies. |
| `head_drift_state` | Match/drift/unchecked between live and frozen heads. | `DERIVED_COMPARISON`. | `UNKNOWN`; must refresh/freeze. |
| `commit_count` | Number of PR commits at capture. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; not zero. |
| `changed_file_count` | Number of changed files. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; no freeze until complete. |
| `changed_filenames` | Complete filename list for the exact head. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN/PARTIAL`; partial page is blocker. |
| `mergeability` | Provider mergeable flag. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; STOP. |
| `merge_state_status` | Detailed merge state, if provider supports it. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; STOP for merge guidance. |
| `ci_checks` | Check/suite/workflow statuses correlated to head SHA. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; not clean. |
| `review_decision` | Provider review decision. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; review blocker. |
| `requested_changes` | Reviews requiring changes. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; not "none". |
| `unresolved_review_threads` | Count and identities of unresolved threads. | `GITHUB_LIVE` or `FROZEN_EVIDENCE`. | `UNKNOWN`; blocker. |
| `blocking_automated_findings` | Bot/security/code scanning findings in scope. | `GITHUB_LIVE`, `FROZEN_EVIDENCE`, or `UNKNOWN_NOT_CHECKED`. | `UNKNOWN`; not clean. |
| `last_live_refresh_timestamp` | When live state was captured. | `GITHUB_LIVE`. | `never`; state `LIVE_UNCHECKED`. |
| `frozen_snapshot_timestamp` | When review evidence was frozen. | `FROZEN_EVIDENCE`. | `none`; no approved head. |
| `evidence_snapshot_id` | Visible frozen snapshot ID. | `FROZEN_EVIDENCE`. | `none`; no freeze exists. |
| `next_permitted_action` | Read-only action or human-only next step. | `DERIVED_COMPARISON`. | `Refresh Live PR State`. |
| `blocking_reason` | Primary blocker plus supporting IDs. | `DERIVED_COMPARISON`. | Must explain unknown source. |

### Card Invariants

- `current_head_sha` is live-only. A frozen snapshot cannot populate it.
- `frozen_reviewed_head_sha` is frozen-only. A live refresh cannot overwrite it.
- `head_drift_state` is derived only when both live and frozen heads are known.
- CI is clean only when each required check is correlated to `current_head_sha`.
- Review is clean only when review decision and thread pagination are complete.
- A frozen snapshot can support review of that head only; it cannot authorize
  a newer head.

---

## Frozen Evidence Timeline Contract

The timeline is a local, hash-linked, append-only evidence contract. It is not
cryptographically immutable in the PKI/notarized sense. A user with filesystem
access can edit files, but verification must detect edits, missing files,
reordered events, and hash mismatches.

### Event Envelope

Each event must contain:

```text
schema_version
event_id
event_type
created_at
captured_by identity/source
repository identity
workspace identity
branch
HEAD
PR number if present
PR head SHA if present
workflow rung
operation
result
evidence artifact references
artifact hashes
previous-event hash or chain reference
facts
inferences
unknowns
warnings
next permitted action
```

### Event Types

Required event types:

```text
WORKSPACE_SNAPSHOT
PACKAGE_PLAN_STARTED
PACKAGE_PLAN_COMPLETED
PACKAGE_PLAN_FAILED
PACKAGE_COMMIT_PREVIEWED
PACKAGE_COMMIT_COMPLETED
PACKAGE_PUSH_COMPLETED
DRAFT_PR_CREATED
PR_LIVE_REFRESH
PR_REVIEW_FROZEN
HEAD_DRIFT_DETECTED
CI_STATE_CAPTURED
REVIEW_BLOCKER_DETECTED
MERGE_APPROVAL_CAPTURED
PR_MERGED
OPERATOR_NOTE
```

Implementation may add event types only by versioning the schema or extending a
closed enum in a dedicated scope. Unknown event types must render as `UNKNOWN`
with raw metadata available only after redaction.

### Timeline Rules

- UI append means append a new event file; never edit prior event files.
- Refreshing live PR state appends `PR_LIVE_REFRESH`.
- Freezing review evidence appends `PR_REVIEW_FROZEN`.
- Detecting drift appends `HEAD_DRIFT_DETECTED` and updates the derived card
  projection; it does not rewrite the old freeze.
- Operator notes append `OPERATOR_NOTE` and remain `OPERATOR_ASSERTED`.
- Events may reference artifacts by hash; they must not embed secrets, private
  prompt text, auth headers, cookies, or raw tokens.
- Review/comment bodies should be omitted or redacted by default. Store IDs,
  URLs, authors, states, counts, and blocker summaries unless the operator
  explicitly chooses a future redacted export mode.

### Live Refresh Contract

`Refresh Live PR State` is an explicit read-only operator action. It obtains
the latest observable PR state and appends a `PR_LIVE_REFRESH` event. It may
update the in-memory live projection shown by the card, but it must not modify,
replace, or delete any frozen snapshot or earlier event.

Refresh records:

- PR identity and provider source identity;
- current head SHA and base SHA;
- current changed-file count and complete filename list, or a partial marker;
- current CI/check state with exact-head correlation;
- current review decision, requested changes, and review-thread completeness;
- current mergeability/merge-state status;
- capture timestamp;
- unknowns, warnings, and partial-result reasons;
- sanitized raw/canonical artifact hashes when artifacts are persisted.

### Freeze Review Evidence Contract

`Freeze Review Evidence` is separate from `Refresh Live PR State`. It is
enabled only from a complete live snapshot that is not blocked by unknown PR
identity, incomplete pagination, missing exact-head correlation, or evidence
chain failure.

A frozen review snapshot records:

- exact current PR head SHA;
- base SHA;
- changed filenames and count;
- CI results, check identities, conclusions, and the head SHA they apply to;
- review state, requested changes, unresolved thread count, and completeness;
- mergeability and merge-state status;
- captured timestamp;
- source/API result identity such as request ID/ETag when available;
- artifact hashes;
- operator-visible snapshot ID.

After freezing:

- live PR state may change;
- the frozen snapshot remains visible;
- drift becomes explicit;
- the operator cannot treat old approval as applying to a new head;
- a new freeze appends a new event and snapshot rather than overwriting the old
  one.

### Approval Gate Contract

The N7 approval gate is a read-only comparison, not a mutation gate. It compares
a frozen approved state with the latest live state and derives whether the old
approval is still applicable to the current head.

Rules:

- if frozen reviewed head and current head differ, old approval is invalid;
- if CI or review data does not correlate to current head, approval is unknown;
- if requested changes or unresolved threads exist, approval is blocked;
- if base drift requires policy, approval is blocked until the policy decision
  is recorded and/or a new freeze exists;
- N7 never performs merge, approval, ready-for-review, or branch operations.

---

## State Machine

N7 uses a finite primary state plus badges. A card may show secondary badges
such as `FROZEN_MATCH`, but the primary state is chosen by precedence.

### Severity Labels

```text
OK          review evidence currently matches and no blockers are known
INFO        neutral or terminal information
WARN        operator attention required before relying on state
STOP        no approval or mutation guidance may proceed
TERMINAL    PR is merged or closed
UNKNOWN     the surface lacks enough verified data
```

### Precedence

When multiple conditions hold, choose the first matching primary state:

1. `MERGED`
2. `CLOSED_UNMERGED`
3. evidence chain verification failure, artifact missing, or hash mismatch
   (render as STOP gate with primary `UNKNOWN`)
4. `NO_PR`
5. `LIVE_FETCH_FAILED`
6. `LIVE_UNCHECKED` or `FROZEN_STALE`
7. `HEAD_DRIFT`
8. `BASE_DRIFT`
9. `MERGE_CONFLICT`
10. `REVIEW_BLOCKED`
11. `CI_FAILED`
12. `CI_PENDING`
13. `DRAFT_BLOCKED`
14. `READY_BLOCKED`
15. `DRAFT_CLEAN`
16. `READY_CLEAN`
17. `FROZEN_MATCH`
18. `UNKNOWN`

`HEAD_DRIFT` outranks CI success because CI may apply to an obsolete head.
Review blockers outrank green CI because approval cannot be inferred from
passing checks.

### State Definitions

| State | Entry conditions | Badge | Severity | Permitted next actions | Forbidden next actions | Required evidence | Recovery action | New freeze required |
|---|---|---|---|---|---|---|---|---|
| `NO_PR` | No PR number/URL is known. | No PR | WARN | Enter/select PR identity; inspect package-pr output. | Freeze, approve, merge, mark ready. | None. | Provide PR identity then refresh. | No. |
| `LIVE_UNCHECKED` | PR identity is known but no current live snapshot exists. | Live unchecked | WARN | `Refresh Live PR State`. | Treat any frozen state as current. | PR identity from operator or prior event. | Refresh live state. | Not yet. |
| `LIVE_FETCH_FAILED` | Read-only fetch attempted and failed. | Live fetch failed | STOP | Retry refresh; inspect auth/rate-limit/partial errors. | Freeze, approve, merge, mark ready. | Failed read event with error class. | Fix read-only access and refresh. | Not until successful refresh. |
| `DRAFT_CLEAN` | Live PR is draft; current data complete; no CI/review/merge blockers except draft posture; head comparison is match or no freeze yet. | Draft clean | OK | Review draft, freeze evidence, refresh. | Merge, mark ready, approve via N7. | Live PR snapshot; complete files/checks/review facts. | Freeze or continue human review. | Yes if no current freeze. |
| `DRAFT_BLOCKED` | Live PR is draft and at least one blocker/unknown exists. | Draft blocked | STOP | Refresh, inspect blockers, append operator note. | Freeze as clean, merge, mark ready. | Live snapshot plus blocker evidence. | Resolve outside N7, then refresh. | Yes after blocker resolution. |
| `READY_CLEAN` | Live PR is not draft; current head equals frozen reviewed head; required CI success; review approved/no unresolved blockers; mergeability clean. | Ready clean | OK | Human-only merge decision outside N7; capture merge approval evidence as assertion/fact if applicable; refresh. | N7 merge/approve/ready write. | Live snapshot, frozen snapshot, CI/review/mergeability evidence. | Human review/merge lane. | No if current freeze matches. |
| `READY_BLOCKED` | Live PR is not draft but has any blocker not covered by more specific state. | Ready blocked | STOP | Refresh; inspect blockers. | Merge/approve guidance. | Live snapshot and blocker evidence. | Resolve externally and refresh. | Yes if head/evidence changed. |
| `HEAD_DRIFT` | `current_head_sha` and `frozen_reviewed_head_sha` are both known and differ. | Head drift | STOP | Refresh; freeze new review evidence for current head. | Treat old approval/freeze as applying to current head. | Live snapshot, frozen snapshot, derived comparison. | Re-review and `Freeze Review Evidence`. | Yes. |
| `BASE_DRIFT` | Live base SHA differs from frozen base SHA or local expected base, and policy does not explicitly allow it. | Base drift | STOP | Refresh; inspect base policy; append operator note. | Merge/approval guidance. | Live/frozen base SHA comparison. | Rebase/update policy outside N7, then refresh/freeze. | Usually yes. |
| `CI_PENDING` | Required CI/checks for current head are queued, in progress, neutral-with-policy-needed, missing, or incomplete. | CI pending | WARN | Refresh after checks complete. | Clear review/merge gate. | Check identities and head correlation. | Wait/refresh. | If CI state changes, append event; freeze if reviewing final state. |
| `CI_FAILED` | Any required check for current head failed/cancelled/timed out/action-required. | CI failed | STOP | Inspect checks; refresh after fix. | Merge/approval guidance. | Check run/suite evidence with `head_sha=current_head_sha`. | Fix externally and refresh. | Yes after new head or final CI changes. |
| `REVIEW_BLOCKED` | Requested changes, unresolved review threads, missing thread page, blocking automated findings, or review decision not approved when required. | Review blocked | STOP | Inspect blockers; refresh after resolution. | Treat green CI as sufficient. | Review decision, review/thread pagination, blocker refs. | Resolve externally and refresh. | Yes if review state changes. |
| `MERGE_CONFLICT` | Mergeability or merge-state status says conflict/dirty/blocked by branch protection. | Merge conflict | STOP | Refresh; inspect mergeability. | Merge/approval guidance. | Live mergeability evidence. | Resolve externally; refresh. | Yes if head/base changes. |
| `FROZEN_MATCH` | Frozen snapshot exists; live current head equals frozen reviewed head; no higher-priority state. | Frozen match | OK | Continue review; capture human-only decision if appropriate. | Any write from N7. | Live snapshot, frozen snapshot, comparison event. | Keep refreshing until final human decision. | No. |
| `FROZEN_STALE` | Frozen snapshot exists but live refresh is older than freshness threshold, or no live refresh after freeze. | Frozen stale | WARN | Refresh live PR state. | Rely on old freeze for current decision. | Frozen snapshot timestamp and freshness policy. | Refresh. | Maybe after refresh if state changed. |
| `MERGED` | Live or frozen terminal state says PR merged. | Merged | TERMINAL | Inspect evidence; append note. | Any PR mutation from N7. | Live/frozen PR state; merge commit SHA if available. | None; close lane. | No. |
| `CLOSED_UNMERGED` | Live or frozen terminal state says PR closed without merge. | Closed unmerged | TERMINAL | Inspect evidence; append note. | Merge/approval guidance. | Live/frozen PR state. | None or new PR outside N7. | No. |
| `UNKNOWN` | Unsupported schema, incomplete comparison, partial source, internal inconsistency, or unclassified condition. | Unknown | UNKNOWN/STOP | Inspect evidence; refresh; verify hash chain. | Clean/ready guidance. | Unknown reason and source IDs. | Fix data/source and refresh. | Usually yes. |

---

## Evidence Classification

Every fact, inference, assertion, and unknown uses a common item shape:

```json
{
  "id": "fact_...",
  "classification": "VERIFIED",
  "statement": "PR #123 current head is abc123...",
  "provenance": "GITHUB_LIVE",
  "source_event_id": "evt_...",
  "source_artifact_id": "artifact_...",
  "captured_at": "2026-07-17T17:58:35Z"
}
```

Inference items additionally require:

```json
{
  "classification": "INFERRED",
  "supports": ["fact_live_head", "fact_frozen_head"],
  "rule": "current_head_sha != frozen_reviewed_head_sha => HEAD_DRIFT"
}
```

Operator assertions additionally require:

```json
{
  "classification": "OPERATOR_ASSERTED",
  "asserted_by": "operator:<local-user-or-configured-id>",
  "assertion_kind": "review_note",
  "promote_to_verified": false
}
```

Unknowns require:

```json
{
  "classification": "UNKNOWN",
  "reason": "reviewThreads page 2 was not fetched due to rate limit",
  "blocks": ["review_clean", "merge_guidance"]
}
```

---

## Schemas

These are implementation-ready sketches, not final JSON Schema files. N7-A
should turn them into typed structures and focused validation tests before any
GitHub or panel integration exists.

### Canonical Serialization and Hashing

Canonical serialization rules:

```text
- UTF-8 JSON.
- Objects sorted lexicographically by key at every level.
- No insignificant whitespace.
- Arrays preserve logical order.
- Timestamps are RFC3339 UTC strings with `Z`.
- Unknown optional fields are omitted unless the schema explicitly requires null.
- Hash input is the canonical byte sequence plus trailing LF only if specified by the schema.
```

### Canonical Number Encoding

Number encoding is normative. It is not left to implementation-dependent
JSON serializer defaults.

Permitted numeric representation:

```text
Hash-bearing N7 objects may contain only JSON integers.

Floating-point values are prohibited in hash-bearing objects, including:
  fractional values
  exponent-form values
  NaN
  positive or negative infinity
  negative zero

Schema fields that require time fractions or decimal measurements must
encode them as integer units, with the unit named in the field, e.g.:
  duration_ms
  size_bytes
  sequence_number

Binary floating-point values must never appear in a hashed object.
```

Range:

```text
Integers must be inside the JavaScript safe-integer range:
  -9007199254740991 through 9007199254740991 (inclusive).

Individual schemas should narrow fields to nonnegative integers where
appropriate (for example: sequence, size_bytes, duration_ms).

An out-of-range value is rejected before canonical serialization or
hashing. It is never clamped or truncated into range.
```

Canonical integer text:

```text
Integers serialize as minimal base-10 ASCII: 0, 1, 42, -1.

Rules:
  - no leading plus sign;
  - no leading zeros except the single value 0;
  - no decimal point;
  - no exponent;
  - no surrounding whitespace;
  - -0 is invalid (must be encoded/accepted as 0, and any literal "-0"
    input is rejected, never normalized silently).
```

Hash failure behavior:

```text
Any disallowed or noncanonical numeric value (float, out-of-range
integer, non-minimal text such as "01" or "1.0", or "-0") must produce a
validation failure before hashing.

It must not be:
  - rounded;
  - coerced;
  - silently converted;
  - hashed using implementation-dependent formatting.

A canonicalization/hash attempt over an invalid numeric value is itself
a bug; validation must run first and refuse the value.
```

Self-field handling (single authoritative statement; see also "Hash
rules" immediately below, which restates this without contradiction):

```text
event_sha256 is omitted from the canonical object used to compute
event_sha256 itself. previous_event_sha256 remains included in that
same canonical object.
```

Hash rules:

```text
artifact_sha256
  SHA-256 over raw artifact bytes.

pr_snapshot_sha256
  SHA-256 over canonical `n7.pr-live.v1` snapshot bytes.

event_sha256
  SHA-256 over canonical `n7.timeline-event.v1` event bytes with
  `event_sha256` omitted. `previous_event_sha256` is included.

chain verification
  Sort events by monotonic sequence/created_at plus event_id tie-breaker,
  recompute each event hash, verify every `previous_event_sha256` equals the
  prior event's recomputed `event_sha256`, verify referenced artifact hashes.
```

Do not add signatures, PKI, or remote notarization in N7 unless a later scope
proves local hash chaining is insufficient.

### PR Live Snapshot

```json
{
  "schema_version": "n7.pr-live.v1",
  "snapshot_id": "live_20260717T175835Z_pr123_headabcdef0",
  "captured_at": "2026-07-17T17:58:35Z",
  "captured_by": {
    "source": "github-reader",
    "reader_version": "n7-reader.v1"
  },
  "repository": {
    "owner": "thesidestackai",
    "name": "stack-code",
    "url": "https://github.com/thesidestackai/stack-code",
    "provider": "github"
  },
  "pr_number": 123,
  "pr_url": "https://github.com/thesidestackai/stack-code/pull/123",
  "title": "",
  "state": "OPEN",
  "draft": true,
  "base_ref": "main",
  "base_sha": "",
  "head_ref": "docs/example",
  "head_sha": "",
  "commit_count": 0,
  "changed_file_count": 0,
  "changed_files": [
    {
      "filename": "docs/example.md",
      "status": "modified",
      "additions": 0,
      "deletions": 0,
      "previous_filename": null
    }
  ],
  "mergeability": "MERGEABLE",
  "merge_state_status": "CLEAN",
  "checks": [
    {
      "provider": "github",
      "name": "test",
      "app": "github-actions",
      "status": "COMPLETED",
      "conclusion": "SUCCESS",
      "head_sha": "",
      "started_at": "",
      "completed_at": "",
      "details_url": "",
      "provenance": "GITHUB_LIVE"
    }
  ],
  "reviews": {
    "review_decision": "APPROVED",
    "requested_changes": [],
    "unresolved_review_threads": {
      "count": 0,
      "complete": true,
      "thread_refs": []
    },
    "blocking_automated_findings": []
  },
  "pagination": {
    "changed_files_complete": true,
    "checks_complete": true,
    "review_threads_complete": true
  },
  "source_identity": {
    "api": "github",
    "request_id": "",
    "etag": "",
    "rate_limit_remaining": null
  },
  "provenance": {
    "head_sha": "GITHUB_LIVE",
    "changed_files": "GITHUB_LIVE",
    "checks": "GITHUB_LIVE",
    "reviews": "GITHUB_LIVE"
  },
  "unknowns": []
}
```

### Frozen Review Snapshot

```json
{
  "schema_version": "n7.pr-review-freeze.v1",
  "snapshot_id": "freeze_pr123_headabcdef0_20260717T175900Z",
  "frozen_at": "2026-07-17T17:59:00Z",
  "repository": {
    "owner": "thesidestackai",
    "name": "stack-code"
  },
  "pr_number": 123,
  "pr_snapshot_ref": "artifact_pr_live_snapshot",
  "pr_snapshot_sha256": "",
  "approved_head_sha": "",
  "base_sha": "",
  "changed_file_count": 0,
  "changed_filenames_sha256": "",
  "ci_summary": {
    "state": "SUCCESS",
    "head_sha": "",
    "check_identities": []
  },
  "review_summary": {
    "decision": "APPROVED",
    "requested_changes_count": 0,
    "unresolved_threads_count": 0,
    "complete": true
  },
  "mergeability": "",
  "source_api_identity": {
    "api": "github",
    "request_id": "",
    "etag": ""
  },
  "evidence_refs": [
    {
      "artifact_id": "artifact_pr_live_snapshot",
      "kind": "pr-live-snapshot",
      "path": ".claw/n7/pr-123/artifacts/live_20260717T175835Z.json",
      "sha256": ""
    }
  ],
  "operator_assertions": [],
  "facts": [],
  "inferences": [],
  "unknowns": []
}
```

### Timeline Event

```json
{
  "schema_version": "n7.timeline-event.v1",
  "event_id": "evt_20260717T175900Z_pr_review_frozen_01",
  "sequence": 1,
  "previous_event_sha256": "",
  "event_sha256": "",
  "event_type": "PR_REVIEW_FROZEN",
  "created_at": "2026-07-17T17:59:00Z",
  "captured_by": {
    "source": "a2-harness-panel",
    "operator_id": "operator:local",
    "tool_version": "n7"
  },
  "repository": {
    "owner": "thesidestackai",
    "name": "stack-code",
    "remote_url_hash": ""
  },
  "workspace": {
    "root": "/mnt/vast-data/git-worktrees/example",
    "root_sha256": "",
    "git_branch": "",
    "git_head": ""
  },
  "pr": {
    "number": 123,
    "head_sha": ""
  },
  "workflow_rung": "draft-pr-review",
  "operation": "Freeze Review Evidence",
  "result": "OK",
  "facts": [],
  "inferences": [],
  "unknowns": [],
  "warnings": [],
  "artifact_refs": [
    {
      "artifact_id": "artifact_pr_live_snapshot",
      "kind": "pr-live-snapshot",
      "path": ".claw/n7/pr-123/artifacts/live_20260717T175835Z.json",
      "sha256": "",
      "size_bytes": 0,
      "redaction": "no-secrets"
    }
  ],
  "next_permitted_action": "ReviewDisposition",
  "blocking_reason": null
}
```

---

## Storage Decision

### Options Compared

| Storage model | Durability | Portability | Source-control impact | Secret risk | Concurrent access | Cleanup ownership | Fit |
|---|---|---|---|---|---|---|---|
| Workspace-local ignored `.claw` evidence | Good for local operator workflow; survives panel restart. | Moves with workspace if copied. | Ignored/untracked; no repo noise. | Medium; must redact and never store tokens/prompts. | Needs append lock or atomic create. | Operator/workspace owner. | Best primary model. |
| Repository-tracked evidence | High in git history. | High. | Pollutes source history; hard to remove mistakes. | High; accidental secret permanence. | Normal git conflicts. | Repo maintainers. | Not recommended for N7. |
| External operator-state directory | Good if backed up. | Lower; separated from workspace evidence. | None. | Medium; centralizes sensitive metadata. | Needs global locking/namespacing. | Operator profile owner. | Possible later export/cache, not primary. |
| Database-backed state | Potentially high. | Low without migration. | None. | Medium/high; auth and backup complexity. | Good if designed. | Service owner. | Over-designed for N7; not justified. |

### Recommendation

Use workspace-local ignored `.claw` evidence as the primary model:

```text
.claw/n7/
  pr-<number>/
    artifacts/
      live_<timestamp>_<head>.json
      freeze_<timestamp>_<head>.json
    events/
      000001_<event_id>.json
      000002_<event_id>.json
    chain.json
```

Rules:

- `.claw/n7/**` is local operator state, not source code.
- Do not add `.claw/n7/**` to git.
- Use atomic create for new events; never rewrite prior event files.
- `chain.json` may be a derived index/cache. If present, it is rebuildable from
  event files and cannot be the sole source of truth.
- Store sanitized snapshots and hashes, not secrets, auth headers, prompt text,
  or full private review bodies.
- Retention is operator-owned. N7 must not auto-delete evidence. The safe
  default is: **retain indefinitely until explicit operator-directed export
  or disposal; no automatic deletion.** (See Open Questions for the
  separate, still-open question of when a dedicated cleanup lane should be
  offered.)
- Missing old evidence is a STOP/UNKNOWN condition, not silently ignored.
- This aligns with existing `.claw` receipts while adding explicit hash-chain
  verification.

`.claw/n7` evidence is **ignored workspace-local evidence**. It is
**tamper-evident** when its hash chain verifies (a modification is
detectable), but it is **not durable merely because it exists locally**:
nothing in this scope makes it immutable, backed up, or safe from disk
loss, accidental deletion, or worktree removal.

### Worktree Removal and Disk-Loss Risk

```text
Removing a worktree may remove its .claw/n7 evidence.
Ignored evidence is not protected by Git.
Disk loss or broad external cleanup may destroy it.
```

No automatic worktree removal or evidence cleanup is permitted while
retained N7 evidence is required by an open review lane. Before a worktree
holding `.claw/n7` evidence is removed, the operator must explicitly:

```text
export, archive, or acknowledge disposal of retained evidence.
```

This scope must not claim durability across worktree deletion, and no N7
implementation may silently treat `.claw/n7` as safe to discard.

### Branch Switching

Evidence is not silently reassigned when the worktree branch or HEAD
changes. Each event must retain:

```text
repository identity
worktree identity or path
branch at capture
HEAD at capture
PR identity when applicable
```

A later branch or HEAD change must produce new evidence and an explicit
drift state (see `HEAD_DRIFT` / `BASE_DRIFT`). Old evidence remains
historical and must never be relabeled as belonging to the new branch or
HEAD.

### Permissions and Path Safety

Future storage defaults:

```text
.claw/n7 directory:                owner-only access, mode 0700 where supported.
event and artifact metadata files: owner read/write only, mode 0600 where supported.
```

The implementation must:

```text
reject symlink traversal for event-store paths;
avoid following a replaced event directory or event file symlink;
avoid writing outside the verified workspace evidence root.
```

When the platform cannot enforce these permissions (for example, a
filesystem without POSIX mode bits), the implementation must record that
as an explicit warning rather than silently claiming protection it cannot
provide.

### Exclusive Writer Contract

Define one writer per evidence chain. A future append must acquire an
exclusive chain-level writer lock before determining:

```text
next event ID
previous event hash
output filename
```

On lock contention:

```text
fail closed;
do not append;
do not retry in a tight loop;
do not report success.
```

The implementation must not automatically break a possibly stale lock. A
stale-lock decision requires explicit inspection and operator policy; it is
out of scope for N7 to auto-resolve.

### Atomic Event Creation

Write sequence for a new event:

```text
1. validate event
2. canonicalize and hash event
3. write a uniquely named temporary file in the same directory
4. flush the file
5. atomically rename it to its final event filename
6. flush directory metadata where supported
7. verify the final event by reading it back
```

An event becomes accepted only after the final-path read verification
succeeds. Steps 3-5 must use a temporary name that cannot collide with a
concurrently-writing process and must never be the final event filename
until the rename completes.

### Partial-Write Recovery

Temporary or incomplete files:

```text
must not be interpreted as accepted timeline events;
must not advance the previous-event hash;
must remain visible as recovery warnings;
must not be automatically deleted.
```

The reader must distinguish:

```text
accepted event
orphan temporary artifact
invalid final event
broken chain
```

An invalid final event or broken chain blocks further append until the
condition is resolved by explicit operator action; N7 does not
self-repair a broken chain.

### Verification on Read

Every read of the evidence chain must verify:

```text
supported schema version
event ID uniqueness
event ordering
canonical serialization validity
event_sha256
previous_event_sha256 linkage
required repository/workspace identity
artifact-reference hashes when the artifact is available
```

Any failure must produce an **integrity STOP**. It must never degrade to a
clean or verified state; a chain that fails verification is treated as
`UNKNOWN`/STOP exactly as described in the STOP Gates table
(`Evidence chain verification fails`), not as "verified with warnings."

---

## GitHub Read-Only Boundary

N7 must define a separate read-only reader interface. It must not reuse the
write-capable package-pr command path for refresh.

### Permitted Future Reads

```text
read PR metadata
read exact head
read changed filenames
read CI/check state
read review state
read comments/threads metadata
read mergeability
```

### Forbidden N7 Actions

```text
create PR
edit PR
mark ready
request reviewers
resolve threads
rerun workflow
enable auto-merge
merge
close
delete branch
approve review
submit review
push
force push
```

### Interface Boundary

The implementation should expose a narrow interface similar to:

```ts
interface N7GithubReader {
  readPullRequestIdentity(input: PrSelector): Promise<PrIdentityRead>;
  readPullRequestLiveSnapshot(input: PrSelector): Promise<PrLiveSnapshotRead>;
  readChangedFiles(input: PrSelector, headSha: string): Promise<PagedFilesRead>;
  readChecksForHead(input: PrSelector, headSha: string): Promise<ChecksRead>;
  readReviewsAndThreads(input: PrSelector, headSha: string): Promise<ReviewsRead>;
  readMergeability(input: PrSelector, headSha: string): Promise<MergeabilityRead>;
}
```

There must be no exported write methods. Tests must prove that write-shaped
methods, GraphQL mutations, and write CLI verbs are unreachable from this
reader.

### Operational Read Rules

Timeouts:

- each read has a bounded timeout;
- timeout yields partial/unknown data with a `LIVE_FETCH_FAILED` or
  `UNKNOWN` fact, never stale clean state.

Pagination:

- changed files, checks, review threads, and comments must declare completion;
- incomplete pagination blocks clean review state;
- partial pages may be displayed with `UNKNOWN/PARTIAL`, but not treated as
  absence of blockers.

Rate limits:

- record rate-limit class and retry-after metadata when available;
- no automatic retry storm;
- stale cache may be displayed only as stale/frozen, not live.

Authentication failure:

- render `GitHub authentication unavailable`;
- do not print tokens, scopes, or credential material;
- allow only read-only recovery guidance.

Partial results:

- each missing field becomes an `UNKNOWN` item with a reason;
- a card with partial review or checks is blocked for approval/merge guidance.

Stale cache:

- cache can speed rendering but cannot replace explicit live refresh;
- cache age is visible;
- stale cache cannot clear drift or blockers.

Exact-head correlation:

- every CI/check/review fact must cite the PR head SHA it applies to;
- if the API cannot prove correlation, the fact is `UNKNOWN`;
- checks for old head do not clear current head.

---

## Panel UX

### Required Sections

```text
Draft PR Card
Frozen Review Snapshot
Live vs Frozen Comparison
Evidence Timeline
Blockers
Next Permitted Action
Evidence Details
```

### Always Visible Without Expansion

The first viewport of the N7 section must show:

```text
Current head
Frozen approved head
MATCH / DRIFT / UNKNOWN
CI success/pending/failure/unknown
Review blockers
Last live refresh
Last frozen snapshot
Next permitted action
```

### Copy Rules

Do not show generic-only labels such as:

```text
configured
ready
good
clean
```

unless the exact identity and evidence are adjacent. Acceptable copy:

```text
Current head: abcdef123 (live, refreshed 2026-07-17T17:58:35Z)
Frozen reviewed head: abcdef123 (freeze_pr123_headabcdef0_20260717T175900Z)
MATCH: current head equals frozen reviewed head
CI: SUCCESS for abcdef123 (5/5 checks)
Review: BLOCKED - 2 unresolved threads
Next: Resolve review blockers outside N7, then Refresh Live PR State
```

### Accessible Severity Labels

Use text labels plus color/icon affordances:

```text
OK: Frozen match
WARN: Stale live data
STOP: Head drift
UNKNOWN: Review threads incomplete
TERMINAL: Merged
```

Color must never be the only signal. Every badge needs an accessible text
label and supporting detail.

### Evidence Details

Expanded evidence details should show:

- source event ID;
- source artifact ID/path;
- artifact hash;
- captured timestamp;
- provenance;
- classification;
- supporting facts for inferences;
- redaction status;
- unknown reason.

---

## STOP Gates

| STOP gate | Exact condition | UI state | Operator message | Allowed read-only recovery | Forbidden mutation |
|---|---|---|---|---|---|
| Head changed after freeze | `current_head_sha != frozen_reviewed_head_sha`. | `HEAD_DRIFT`, STOP. | "Current head differs from frozen reviewed head. Old approval does not apply." | Refresh; freeze new review evidence after review. | Merge/approve/mark ready using old freeze. |
| CI not correlated to current head | Any required check lacks `head_sha=current_head_sha` or equivalent proof. | `CI_PENDING` or `UNKNOWN`, STOP/WARN. | "CI result is not proven for current head." | Refresh checks; inspect check source. | Treat old green CI as current. |
| PR data incomplete | Required live fields or pagination incomplete. | `UNKNOWN`, STOP. | "PR data is incomplete; clean state cannot be proven." | Retry refresh; inspect partial errors. | Freeze as clean; merge guidance. |
| Review threads unresolved | Any unresolved thread count > 0. | `REVIEW_BLOCKED`, STOP. | "Unresolved review threads remain." | Refresh after resolution; inspect details. | Merge guidance. |
| Requested changes present | Any latest non-dismissed review requests changes. | `REVIEW_BLOCKED`, STOP. | "Requested changes are present." | Refresh after new review. | Treat approvals/CI as sufficient. |
| Mergeability unknown | Mergeability is unknown/pending/unavailable. | `UNKNOWN` or `MERGE_CONFLICT`, STOP. | "Mergeability is not proven." | Refresh later; inspect provider state. | Merge guidance. |
| Base drift requires policy | Live base SHA differs from frozen base SHA and no policy permits it. | `BASE_DRIFT`, STOP. | "Base changed since freeze; policy decision required." | Refresh; re-freeze after review or record policy assertion. | Apply old approval to new base. |
| Evidence chain verification fails | Recomputed event hash or previous-event hash mismatch. | `UNKNOWN`, STOP. | "Evidence chain verification failed." | Inspect event files; restore from trusted backup; append note only after verification. | Rely on corrupted evidence. |
| Artifact missing/hash mismatch | Referenced artifact missing or SHA-256 mismatch. | `UNKNOWN`, STOP. | "Referenced evidence artifact is missing or changed." | Recompute/inspect; refresh/freeze new evidence if appropriate. | Treat missing artifact as absent blocker-free. |
| GitHub auth unavailable | Reader cannot authenticate for required read scope. | `LIVE_FETCH_FAILED`, STOP. | "GitHub authentication unavailable for read-only refresh." | Fix auth externally; retry. | Any PR write workaround. |
| Live refresh stale | `now - last_live_refresh > freshness_threshold`. | `FROZEN_STALE`, WARN/STOP. | "Live PR state is stale; refresh before decision." | Refresh live state. | Use stale live state for approval. |
| Secret detected in evidence | Evidence artifact contains token/header/private key pattern. | `UNKNOWN`, STOP. | "Potential secret detected in evidence; do not persist/export." | Redact via separately scoped remediation; do not publish. | Commit/push evidence. |
| No automatic rung advancement | Any N7 event would transition package rung state without observed event. | STOP. | "N7 cannot auto-advance workflow rungs." | Append observation only after verified source. | Package/PR/merge action. |

---

## Testing

### Required Test Matrix

| Category | Required examples |
|---|---|
| Pure state-model tests | `current_head_equal_to_frozen_head_is_match`, `current_head_change_invalidates_prior_merge_approval`, `head_drift_outranks_green_ci`, `review_blocked_outranks_ready_clean`, `terminal_merged_outranks_stale_refresh`. |
| Schema validation | Reject unknown schema versions; reject missing required IDs; reject invalid provenance enum; accept complete v1 snapshots. |
| Canonical serialization/hash tests | Stable sorted-key hashes; changing whitespace does not change canonical hash; changing a fact changes hash; `event_sha256` excludes itself. |
| Append-only timeline tests | `refresh_appends_event_without_rewriting_freeze`, prior event file content is unchanged, sequence gap is detected, duplicate event ID is refused. |
| Live-vs-frozen drift tests | `current_head_change_invalidates_prior_merge_approval`, base SHA drift blocks, missing frozen head yields unknown not match. |
| Exact-head CI correlation tests | `green_ci_for_old_head_does_not_clear_current_head`, missing head correlation blocks clean, mixed-head checks derive STOP. |
| Partial GitHub response tests | Missing changed-files page is partial; missing review page is not no blockers; timeout creates unknowns. |
| Authentication failure tests | Auth failure renders `LIVE_FETCH_FAILED`; no token printed; recovery is refresh after auth only. |
| Rate-limit tests | Rate-limited review threads become unknown; retry-after displayed; stale cache not clean. |
| Pagination tests | All changed files fetched; thread pages complete; incomplete cursor blocks clean. |
| Review-blocker tests | Requested changes block; unresolved thread blocks; bot finding blocks; dismissed review is handled by explicit source rule. |
| Mergeability unknown tests | Unknown/pending mergeability blocks; conflict maps to `MERGE_CONFLICT`. |
| Artifact missing/hash mismatch tests | Missing freeze artifact fails chain; modified prior event detected; artifact SHA mismatch blocks. |
| Panel rendering tests | Exact heads visible; current vs frozen visually distinct; blockers visible above details; generic "ready" absent without evidence. |
| Accessibility tests | Severity text labels present; color not sole signal; headings and expanded details navigable. |
| No-write GitHub guard tests | `github_write_method_is_unreachable_from_n7_reader`; GraphQL mutation strings refused; CLI verbs `create/edit/ready/merge/close/review --approve` absent. |
| No automatic workflow-rung advancement tests | Refresh and freeze do not mark package rungs done; N7 never dispatches helper; no state transition without observed source event. |
| Number-encoding tests | `canonical_integer_serialization_is_minimal_base10`, `floating_point_value_is_rejected_before_hashing`, `negative_zero_is_rejected`, `integer_above_safe_range_is_rejected`, `event_hash_omits_only_event_sha256`. |
| Concurrency tests | `concurrent_append_second_writer_fails_closed`, `concurrent_append_does_not_duplicate_event_id`, `concurrent_append_does_not_fork_previous_hash`, `writer_lock_contention_does_not_report_success`. |
| Interrupted/partial-write tests | `interrupted_temp_write_does_not_advance_chain`, `orphan_temp_file_is_reported_and_preserved`, `partial_final_event_blocks_append`, `read_verification_rejects_modified_event`, `failed_readback_does_not_accept_event`. |
| Storage-identity tests | `branch_change_does_not_relabel_old_events`, `head_change_requires_new_event_and_drift_state`, `worktree_local_store_is_not_claimed_durable_after_removal`, `symlinked_event_path_is_rejected`, `unsupported_permission_enforcement_is_reported`. |

Named tests that must exist by N7 completion:

```text
current_head_equal_to_frozen_head_is_match
current_head_change_invalidates_prior_merge_approval
green_ci_for_old_head_does_not_clear_current_head
refresh_appends_event_without_rewriting_freeze
missing_review_page_is_not_reported_as_no_blockers
github_write_method_is_unreachable_from_n7_reader
timeline_hash_chain_detects_modified_prior_event
head_drift_outranks_ci_success
requested_changes_block_ready_clean
unknown_mergeability_blocks_merge_guidance
frozen_snapshot_visible_after_live_refresh
operator_assertion_never_promotes_to_verified
canonical_integer_serialization_is_minimal_base10
floating_point_value_is_rejected_before_hashing
negative_zero_is_rejected
integer_above_safe_range_is_rejected
event_hash_omits_only_event_sha256
concurrent_append_second_writer_fails_closed
concurrent_append_does_not_duplicate_event_id
concurrent_append_does_not_fork_previous_hash
writer_lock_contention_does_not_report_success
interrupted_temp_write_does_not_advance_chain
orphan_temp_file_is_reported_and_preserved
partial_final_event_blocks_append
read_verification_rejects_modified_event
failed_readback_does_not_accept_event
branch_change_does_not_relabel_old_events
head_change_requires_new_event_and_drift_state
worktree_local_store_is_not_claimed_durable_after_removal
symlinked_event_path_is_rejected
unsupported_permission_enforcement_is_reported
```

### Storage, Concurrency, and Number-Encoding Test Contracts

Each test below must define a precondition, an operation, an expected
state/result, the expected evidence effect, and a forbidden side effect.
Titles alone are not sufficient specification.

| Test | Precondition | Operation | Expected state/result | Expected evidence effect | Forbidden side effect |
|---|---|---|---|---|---|
| `canonical_integer_serialization_is_minimal_base10` | A valid in-range integer field (e.g. `sequence: 1`). | Canonicalize and serialize the object. | Serialized text is minimal base-10 (`1`, not `01` or `1.0`). | Hash is computed only over the minimal-base-10 form. | Leading zeros, decimal points, or exponents in hashed output. |
| `floating_point_value_is_rejected_before_hashing` | A hash-bearing object with a fractional/exponent/NaN/Infinity numeric field. | Attempt canonicalization/hash. | Validation fails before any hash is computed. | No event/hash is produced or accepted. | Rounding, truncation, or silent coercion to an integer. |
| `negative_zero_is_rejected` | A hash-bearing object with a literal `-0` numeric field. | Attempt canonicalization/hash. | Validation fails; `-0` is never silently normalized to `0` and accepted. | No event/hash is produced from the invalid input. | Treating `-0` and `0` as interchangeably valid input. |
| `integer_above_safe_range_is_rejected` | A hash-bearing object with an integer outside ±9007199254740991. | Attempt canonicalization/hash. | Validation fails before hashing. | No event/hash is produced. | Clamping, truncating, or wrapping the value into range. |
| `event_hash_omits_only_event_sha256` | A fully populated timeline event with both `event_sha256` and `previous_event_sha256` set. | Compute `event_sha256` over the canonical object. | Only `event_sha256` is excluded from the hashed object; `previous_event_sha256` is included. | Recomputed hash matches only when `previous_event_sha256` is part of the input. | Excluding `previous_event_sha256`, or including a stale/placeholder `event_sha256`. |
| `concurrent_append_second_writer_fails_closed` | Writer A holds the chain-level writer lock and has not released it. | Writer B attempts to append a new event. | Writer B's append fails closed (no event written). | No new event file exists from writer B's attempt. | Writer B silently succeeding, blocking indefinitely, or retrying in a tight loop. |
| `concurrent_append_does_not_duplicate_event_id` | Two writers race for the same next sequence/event ID. | Both attempt to append near-simultaneously. | Only the lock-holding writer's event is accepted; the other fails closed. | Exactly one event exists for that sequence number. | Two accepted events sharing one event ID or sequence number. |
| `concurrent_append_does_not_fork_previous_hash` | Two writers race to append after the same prior event. | Both attempt to append near-simultaneously. | Only one successor event is accepted for that prior event's hash. | The chain remains a single linear sequence, not a fork. | Two accepted events both citing the same `previous_event_sha256`. |
| `writer_lock_contention_does_not_report_success` | The chain-level writer lock is held by another process. | A second process attempts to acquire the lock and append. | The second process reports failure/contention, not success. | No event file is created by the contending process. | A success result returned without an actual accepted event. |
| `interrupted_temp_write_does_not_advance_chain` | A temporary event file write is interrupted mid-write (simulated truncation/crash). | Reader/verifier inspects the chain. | The chain's last accepted event is unchanged; the temp file is not treated as accepted. | `previous_event_sha256` chain state is unaffected by the interrupted write. | Treating the incomplete temp file as the new chain head. |
| `orphan_temp_file_is_reported_and_preserved` | An orphaned temporary file exists from a prior interrupted write. | Reader/verifier scans the event directory. | Orphan is surfaced as a recovery warning. | Orphan file is neither deleted nor promoted to an accepted event. | Automatic deletion or automatic promotion of the orphan file. |
| `partial_final_event_blocks_append` | A final-path event file exists but fails schema/canonicalization/readback validation (e.g. truncated JSON). | Attempt a new append. | Append is blocked until the partial/invalid final event is resolved. | No new event is appended on top of an unverified predecessor. | Appending past an unverified/partial event as if it were valid. |
| `read_verification_rejects_modified_event` | An accepted event file's bytes are modified after acceptance (tamper simulation). | Reader/verifier recomputes the event's hash. | Recomputed hash does not match; verification fails with an integrity STOP. | Chain is reported as broken/unverified, not silently accepted. | Accepting the modified bytes as if unchanged. |
| `failed_readback_does_not_accept_event` | A write completes rename, but the final-path readback (write-sequence step 7) fails or mismatches. | Writer performs the write sequence including readback verification. | The event is not considered accepted. | No successor event may cite this event as `previous_event_sha256`. | Treating a rename-only success (without readback verification) as acceptance. |
| `branch_change_does_not_relabel_old_events` | Existing accepted events captured on branch A. | Worktree branch is switched to branch B; evidence chain is read. | Old events still show branch A as their captured branch. | No historical event's `workspace.git_branch` is rewritten. | Silently relabeling prior events to branch B. |
| `head_change_requires_new_event_and_drift_state` | Existing accepted events captured at HEAD X; worktree HEAD advances to HEAD Y. | Panel/reader observes the new HEAD. | A new event is required to reflect HEAD Y; drift state (`HEAD_DRIFT`) is derived rather than silently updating old evidence. | Old events remain associated with HEAD X. | Mutating an existing event's `workspace.git_head` in place. |
| `worktree_local_store_is_not_claimed_durable_after_removal` | A worktree containing `.claw/n7` evidence is removed (simulated via a disposable fixture worktree, not a real operator worktree). | Attempt to read evidence after removal. | Evidence is unavailable; this is the expected, documented outcome, not a defect. | No claim anywhere in surfaced state implies the evidence survived removal. | Documentation or UI text implying `.claw/n7` evidence is durable across worktree deletion. |
| `symlinked_event_path_is_rejected` | An event-store path component is replaced with a symlink pointing outside the verified evidence root. | Reader/writer resolves the path. | The symlinked path is rejected; no read or write occurs through it. | No event is read from or written to a location outside the verified root. | Following the symlink and silently operating outside the evidence root. |
| `unsupported_permission_enforcement_is_reported` | Running on a filesystem/platform that cannot enforce 0700/0600 mode bits. | Writer initializes or appends to the store. | An explicit warning is recorded/surfaced. | The warning is visible in evidence/state, not swallowed. | Silently claiming permission protection that the platform cannot provide. |

---

## Acceptance Criteria

An N7 implementation is acceptable only when all twelve criteria hold:

1. The PR card shows exact identities, not generic status.
2. Live and frozen state are visually distinct.
3. Head drift invalidates prior approval.
4. CI is correlated to exact head SHA.
5. Missing data is represented as unknown, never clean.
6. Review blockers are not hidden by green CI.
7. Frozen evidence is append-only and hash-verifiable.
8. Old evidence remains visible after refresh.
9. No GitHub write operation is reachable.
10. No workflow rung auto-advances.
11. No secret values enter evidence artifacts.
12. All required state, schema, hash, and rendering tests pass.

Blocking criteria:

- any GitHub write surface in N7 code;
- any live/frozen merge into one mutable status object;
- any clean state from partial data;
- any old-head CI clearing current-head state;
- any evidence rewrite instead of append;
- any secret/token persisted in evidence;
- any non-integer numeric value entering a hash-bearing object;
- any integer exceeding the safe-integer range entering a hash-bearing object;
- any state where the evidence chain cannot be verified but is not reported
  as an integrity STOP;
- any append proceeding while another writer owns the chain lock;
- any temporary or partial event left unresolved without blocking further
  append;
- any event accepted despite a failed final-path readback verification;
- any storage path resolving through a prohibited symlink;
- any worktree removal proceeding while retained N7 evidence has not had
  its disposal explicitly acknowledged by the operator.

---

## Implementation Slices

### N7-A: Schemas and Pure State Model

Objective:

- implement only versioned TypeScript model types, validators, canonical
  serialization/hash helpers, and pure live-vs-frozen state derivation.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7Schemas.ts
ide/vscode/a2-harness-panel/src/n7State.ts
ide/vscode/a2-harness-panel/test/n7Schemas.test.ts
ide/vscode/a2-harness-panel/test/n7State.test.ts
```

Dependencies:

- current N5/N6 state style and existing test harness;
- canonical number encoding is fully specified by this scope (see
  "Canonical Number Encoding") and is no longer an open hashing ambiguity
  for N7-A.

Test gate:

- pure state/schema/hash tests only, including the number-encoding test
  contracts (`canonical_integer_serialization_is_minimal_base10`,
  `floating_point_value_is_rejected_before_hashing`,
  `negative_zero_is_rejected`, `integer_above_safe_range_is_rejected`,
  `event_hash_omits_only_event_sha256`).

Mutation risk:

- low; no network, no panel rendering, no storage writes.

STOP gate:

- stop if any GitHub, fs storage, helper, Claw, or panel integration appears.

Exit criteria:

- all required pure state examples pass; hash canonicalization is deterministic.

### N7-B: Read-Only GitHub Adapter

Objective:

- add a read-only adapter interface and a fake-backed implementation boundary
  that can produce `n7.pr-live.v1` snapshots.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7GithubReader.ts
ide/vscode/a2-harness-panel/test/n7GithubReader.test.ts
ide/vscode/a2-harness-panel/scripts/run-guards.js
```

Dependencies:

- N7-A schemas.

Test gate:

- no-write guard, auth failure, partial response, pagination, rate-limit,
  exact-head correlation tests.

Mutation risk:

- medium; introduces provider boundary.

STOP gate:

- no mutation strings, write verbs, PR creation/edit/merge/ready/review approve,
  or helper/package-pr reuse.

Exit criteria:

- fake reader covers complete/partial/failure cases; no live GitHub in tests.

### N7-C: Append-Only Evidence Store

Objective:

- implement local `.claw/n7` append-only event/artifact writer and verifier.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7EvidenceStore.ts
ide/vscode/a2-harness-panel/test/n7EvidenceStore.test.ts
```

Dependencies:

- N7-A canonical hashes;
- the exclusive-writer, atomic-write, partial-write-recovery, and
  verification-on-read contracts are fully specified by this scope (see
  "Storage Failure and Concurrency Contract") and are no longer open
  storage-tradeoff questions for N7-C.

Test gate:

- append-only, chain verification, missing artifact, hash mismatch, atomic
  create behavior, plus the concurrency, interrupted/partial-write, and
  storage-identity test contracts (`concurrent_append_second_writer_fails_closed`,
  `interrupted_temp_write_does_not_advance_chain`,
  `orphan_temp_file_is_reported_and_preserved`,
  `partial_final_event_blocks_append`,
  `branch_change_does_not_relabel_old_events`,
  `worktree_local_store_is_not_claimed_durable_after_removal`,
  `symlinked_event_path_is_rejected`,
  `unsupported_permission_enforcement_is_reported`, and the remaining
  named tests in "Storage, Concurrency, and Number-Encoding Test
  Contracts").

Mutation risk:

- medium; local file writes under ignored `.claw/n7`.

STOP gate:

- no writes outside `.claw/n7`; no repo-tracked evidence; no secret storage;
  no append while the chain-level writer lock is held by another writer.

Exit criteria:

- old events remain unchanged; verifier detects tamper/missing artifacts;
  concurrent and interrupted-write test contracts pass.

### N7-D: PR Card Rendering

Objective:

- render Draft PR Card and visible exact identities from pure N7 view model.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7View.ts
ide/vscode/a2-harness-panel/src/render.ts
ide/vscode/a2-harness-panel/test/n7Render.test.ts
```

Dependencies:

- N7-A; may use fake snapshots.

Test gate:

- exact identities visible, generic-status avoidance, accessibility labels.

Mutation risk:

- low/medium; UI only.

STOP gate:

- no controls that mutate PRs or package rungs.

Exit criteria:

- current head, frozen head, match/drift, CI/review/blockers, refresh/freeze
  timestamps visible.

### N7-E: Frozen/Live Comparison

Objective:

- integrate live snapshot, frozen snapshot, and derived comparison badges.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7Comparison.ts
ide/vscode/a2-harness-panel/src/n7View.ts
ide/vscode/a2-harness-panel/test/n7Comparison.test.ts
```

Dependencies:

- N7-A, N7-D.

Test gate:

- drift precedence, stale refresh, old CI, base drift, review blockers.

Mutation risk:

- low.

STOP gate:

- old approval must never apply to new head.

Exit criteria:

- comparison rules produce required states and next permitted actions.

### N7-F: Timeline Rendering

Objective:

- render hash-linked evidence timeline and evidence details.

Likely files:

```text
ide/vscode/a2-harness-panel/src/n7TimelineView.ts
ide/vscode/a2-harness-panel/src/render.ts
ide/vscode/a2-harness-panel/test/n7TimelineRender.test.ts
```

Dependencies:

- N7-A and optionally N7-C.

Test gate:

- timeline order, event classification, artifact refs, hash status,
  contradiction display, accessibility.

Mutation risk:

- low/medium.

STOP gate:

- do not hide hash verification failures in collapsed details.

Exit criteria:

- evidence event list distinguishes facts/inferences/assertions/unknowns.

### N7-G: Integrated Read-Only Smoke

Objective:

- exercise panel flow with fake or fixture GitHub responses and local `.claw/n7`
  evidence, without live network or package execution.

Likely files:

```text
ide/vscode/a2-harness-panel/test/n7IntegratedReadOnly.test.ts
test/fixtures/n7/*
```

Dependencies:

- N7-A through N7-F.

Test gate:

- full read-only smoke: refresh fixture, freeze, refresh drift, verify old
  freeze remains visible.

Mutation risk:

- low if fixture-only; medium if `.claw` temp writes are used.

STOP gate:

- no live GitHub, no helper, no Claw, no workflow rung advancement.

Exit criteria:

- integrated flow passes with deterministic fixtures and all guards green.

---

## Open Questions

Resolved by this scope pass (no longer open for N7-A or N7-C): canonical
number encoding (see "Canonical Number Encoding"), the exclusive-writer/
append contract, partial-write recovery, and verification-on-read behavior
(see "Storage Failure and Concurrency Contract"). These are policy- and
implementation-ready; the questions below are the remaining genuinely open
choices.

1. What freshness threshold should block live/frozen decisions: 5 minutes,
   15 minutes, or operator-configurable?
2. Should the first GitHub reader use `gh` JSON output, GraphQL, REST, or an
   injectable abstraction with CLI/API adapters later?
3. What exact local operator identity should `captured_by` use without leaking
   hostnames or account secrets?
4. Should review comment bodies always be redacted, or should N7 allow an
   explicit redacted excerpt mode later?
5. What is the base-drift policy for documentation-only PRs versus code PRs?
6. **Retention has a safe default and is not blocking**: `.claw/n7` evidence
   is retained indefinitely until explicit operator-directed export or
   disposal, with no automatic deletion (see "Storage Decision"). The
   remaining open question is narrower: at what retention age or evidence
   volume, if any, should a *separate* cleanup lane be recommended to the
   operator (as an offer, never an automatic action)?
7. Should chain verification run on panel open, on refresh, or both? (The
   mechanics of verification-on-read are now defined; only the triggering
   UI moment remains open.)

---

## Recommended First Build Slice

Start with **N7-A: Schemas and Pure State Model**.

Boundaries:

- no GitHub calls;
- no panel rendering;
- no storage writes;
- no helper/Claw/broker/Ollama;
- no runtime state;
- no workflow rung advancement.

The first command in the implementation lane should be:

```text
Inspect docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md and the existing pure model patterns in ide/vscode/a2-harness-panel/src/n5State.ts, src/n6State.ts, and test/n6State.test.ts. Identify the smallest module boundary for versioned N7 schemas, canonical serialization, and pure live-vs-frozen state transitions only.
```
