# CLAUDE CODE PROMPT (DRAFT) — A2 Tier 3 Read-Only Status Panel Integration Implementation (Option A)

> **DRAFT — token-gated.** Do not execute this prompt until reviewed and merged, and until the
> operator supplies the exact approval token below. This lane edits live IDE/panel code.

## Approval Token (REQUIRED before any implementation)

The executing session MUST refuse to make any edit unless the operator's message contains, verbatim:

```text
APPROVED: Execute A2 Tier 3 read-only status panel integration implementation
```

If the token is absent, STOP and report `BLOCKED: missing approval token` — make no edits, no
worktree, no commit.

## Role

You are operating as a careful Stack-Code read-only-panel integration implementer.

Follow: OBSERVE → VERIFY TOKEN → CLEAN WORKTREE → TESTS-FIRST → IMPLEMENT (Option A only) → GUARD/BUILD/TEST → SCAN → COMMIT LOCAL → REPORT

## Objective

Wire the merged read-only evidence-snapshot renderer (`src/tier3EvidenceSnapshot.ts`, #128) into
the live A2 IDE Extension Panel via **Option A (operator-provided snapshot text)**: render an
operator-pasted `a2-tier3-evidence-snapshot.v0` as a new read-only section, with zero new spawn
capability and zero execution controls.

Canonical inputs:

```text
handoffs/a2_tier3_panel_integration_implementation_readiness_2026-06-11.md  (this lane's readiness spec)
docs/a2-tier3-panel-integration-review.md                                   (#131, fc09840)
ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts                     (#128, reuse unchanged)
```

## Hard Boundaries

Do NOT:

```text
edit any source outside the approved set (src/render.ts, src/extension.ts, new test/ files)
edit src/helperRunner.ts / src/panel.ts / src/tier3EvidenceSnapshot.ts
edit scripts/a2-ide-harness.sh / scripts/run-guards.js / CI workflow
edit rust / schemas / services / runtime / Dockerfiles / *.service / .vscode
add ANY execution control (Run / Apply / Approve / Create Worktree / Cleanup)
add a button / onclick / data-subcommand / data-ui-action / postMessage to the snapshot section
add fs / network / fetch / child_process / spawn / :11434 / ollama / broker / SecretStorage /
  setInterval / setTimeout / watcher reference in live code
broaden the helper spawn boundary or ALLOWED_SUBCOMMANDS (that is Option B — separate lane)
spawn the executor / collector from the webview or any guard-safe module
create worktrees from the panel / write files from the panel
gather evidence inside a guard-safe module (snapshot is the SOLE input; acquired as text)
call model / broker / /v1/chat/completions / /status/vram
touch runtime / Vault / secrets; print secrets
run live smoke / collector / orchestrator / validate-lane / apply-lane / preview / approval /
  apply-bundle / apply
push / open PR / merge (local commit only)
```

Do NOT run destructive commands:

```bash
git clean ; rm -rf ; find ... -delete ; find ... -exec rm ; git reset --hard
git add . ; git add -A ; git branch -D ; git worktree remove --force ; git fetch --prune
git push origin --delete
```

Allowed:

```text
fresh worktree from origin/main; exact branch/path collision checks; read repo/source read-only;
write ONLY src/render.ts + src/extension.ts + new test/ files; run node scripts/run-guards.js,
tsc, and npm test; exact-path staging; local commit only.
```

## Clean Worktree Setup

```text
Base:     origin/main (fast-forwarded, clean control checkout, verified)
Branch:   feat/a2-tier3-panel-integration-optionA-<YYYYMMDD>
Worktree: /mnt/vast-data/git-worktrees/stack-code-a2-tier3-panel-integration-optionA-<YYYYMMDD>
```

Do not edit `/home/suki/stack-code`. Verify the target branch/path do not already exist (collision
checks). Do not reuse or touch any retained review/readiness branch or worktree.

## Phase 0 — Preflight + Token

```text
- assert the approval token is present verbatim; else STOP BLOCKED.
- control checkout on main, clean (no staged/unstaged); fetch origin main; ff-only.
- collision checks: target branch and worktree path must not exist.
- worktree add -b <branch> <path> origin/main; cd into it.
```

## Phase 1 — Tests First

```text
- add test/ cases per readiness §10 BEFORE editing src:
  * render: section present with a view; muted no-control placeholder when absent;
    fail-closed view renders ONLY the unsupported notice.
  * extension: snapshot text set → parsed view; bad/mismatched → fail-closed; none → null.
  * guards: test/guards.test.ts still PASS.
  * no-control scan over the new section.
- run npm test; confirm the new render/extension tests FAIL for the right reason (not yet wired).
```

## Phase 2 — Implement (Option A only)

```text
- src/render.ts: add evidenceSnapshot?: EvidenceSnapshotView | null to RenderModel (import type
  from ./tier3EvidenceSnapshot); add evidenceSnapshotBlock(view) (muted placeholder when absent,
  else renderEvidenceSnapshotHtml(view)); insert into renderHtml() after executorDryRunBlock.
- src/extension.ts: import parseEvidenceSnapshot; add a session field for operator-provided
  snapshot text; add buildEvidenceSnapshotView() (null when unset, else parse); add
  evidenceSnapshot to model(); add a read-only field-setter (uiAction) to paste snapshot text
  (sets a field + rerender(); spawns nothing, runs no helper subcommand).
- NO fs, NO spawn, NO control, NO network anywhere in the new code.
```

## Phase 3 — Guard / Build / Test / Scan

```text
- node scripts/run-guards.js → exits 0, prints "a2-harness-panel guards PASS".
- tsc build clean; npm test green (all suites, including guards + new tests).
- grep the diff for: 11434 | ollama | broker | fetch( | child_process | spawn( | fs\. |
  setInterval | setTimeout | SecretStorage | onclick | data-subcommand — expect none in live code.
- confirm git diff --name-only is EXACTLY src/render.ts, src/extension.ts, and new test/ files.
```

## Phase 4 — Commit Locally Only

```text
- exact-path stage ONLY the approved files; git commit -m "feat(a2): wire read-only tier 3
  evidence snapshot into panel (Option A)". DO NOT push. DO NOT open a PR. DO NOT merge.
```

## Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_PANEL_INTEGRATION_OPTION_A_IMPLEMENTATION
APPROVAL TOKEN PRESENT: yes | no
BRANCH:
WORKTREE:
BASE:
COMMIT:
FILES CHANGED: (must be exactly src/render.ts, src/extension.ts, new test/ files)
TESTS:
  tests-first added:
  render section present/absent:
  fail-closed notice:
  extension parse/null/bad:
  guards PASS:
  no-control scan:
GUARD/BUILD:
  run-guards.js:
  tsc:
  npm test:
  forbidden-token grep:
BOUNDARIES:
  helperRunner.ts edited: (must be NO)
  panel.ts edited: (must be NO)
  tier3EvidenceSnapshot.ts edited: (must be NO)
  run-guards/CI/helper edited: (must be NO)
  spawn added: (must be NO)
  control added: (must be NO)
  fs/network/:11434 added: (must be NO)
SAFETY:
  model/broker call: NO
  runtime touched: NO
  Vault/secrets touched: NO
  raw 11434 app inference: NO
  live smoke/collector/orchestrator/validate-lane/apply-lane: NO
  push/PR/merge: NO
  destructive commands used: NO
STOP GATES HIT: none | details
NEXT BEST LANE:
  Name:
  Objective:
  Recommended tool:
  Why:
  Touched surfaces:
  Mutation risk:
  STOP gate:
  First prompt/command:
```

## Recommended Next Lane After This Implementation

```text
A2 Tier 3 Panel Integration — Push PR + CI verification (operator-approved), then OPTIONAL
Option B (helper print-tier3-evidence subcommand) as a separate guard-reviewed lane.
```
