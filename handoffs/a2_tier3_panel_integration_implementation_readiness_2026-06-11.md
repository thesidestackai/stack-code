# A2 Tier 3 Read-Only Status Panel Integration — Implementation-Readiness Handoff (Docs-Only)

> **Docs / design-only.** This handoff bounds a future, separately-approved lane that wires the
> already-merged read-only evidence-snapshot renderer into the live A2 IDE Extension Panel. It
> implements nothing, edits no source, runs no smoke/collector/orchestrator, and spawns no
> executor. It exists to define exact touch surfaces, the read-only UI behavior, the unchanged
> guard boundaries, the proof-of-safety tests, and the exact approval token required to begin.
>
> **This is not a write-capable executor lane. This is not an apply lane. This is not a live
> smoke lane. The next implementation may only surface read-only dry-run/status information.
> The webview must not spawn executor/create/write/apply. Any write-capable executor step
> requires a separate design and explicit approval.**

## 1. Executive Summary

Both halves of the Tier 3 read-only status panel are merged on `main`:

- the read-only collector (`rust/crates/a2-evidence-collector`, #126) emits a deterministic
  `a2-tier3-evidence-snapshot.v0`;
- the panel renderer (`ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts`, #128) is a
  pure, fail-closed, zero-control module that turns that snapshot into a read-only HTML fragment.

The gap: **nothing in the live extension acquires a snapshot or renders it.** The renderer is a
pure module that no live-path module (`extension.ts` / `render.ts` / `panel.ts`) calls yet.

The canonical PR #131 review (`docs/a2-tier3-panel-integration-review.md`, squash `fc09840`)
recommends shipping **Option A — operator-provided snapshot** first: the smallest guard surface,
zero new spawn capability. This handoff makes Option A concrete down to the file/line/symbol level
and defers Option B (helper-subcommand-mediated collector) to its own separate guard-reviewed lane.

## 2. Source of Truth

```text
docs/a2-tier3-panel-integration-review.md            (#131, squash fc09840) — canonical integration review
docs/a2-tier3-evidence-surface-contract.md           (#123)
docs/a2-tier3-evidence-collector-design.md           (#124)
docs/a2-tier3-status-panel-scope-card.md             (#127)
docs/a2-tier3-readonly-observability-closeout-20260610.md (#130)
rust/crates/a2-evidence-collector                    (#126, emits a2-tier3-evidence-snapshot.v0)
ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts (#128, the renderer to wire — read-only)
```

## 3. Current Merged State (verified read-only from `origin/main @ fc09840`)

```text
- panel.ts:        A2HarnessPanel.show(model: RenderModel) → renderHtml(model) → webview.html.
                   Webview has localResourceRoots: [] (no file content, no network). Its wiring
                   script binds ONLY existing button classes (.btn.helper / .btn.ui /
                   [data-copy-output]); it posts runSubcommand / uiAction / copyOutput only.
- extension.ts:    model(): RenderModel assembles the model from session state + build*View()
                   helpers (buildFoundationView / buildTier3View / buildExecutorDryRunView).
                   Live actions call runHelper(...) (read-only) and rerender().
- helperRunner.ts: the ONLY module allowed to spawn a process. Spawns ONLY the helper
                   (basename a2-ide-harness.sh, shell:false, array-argv) with an allowlisted
                   read-only subcommand. ALLOWED_SUBCOMMANDS = help, validate-input,
                   print-preview, find-artifacts, print-approval, print-apply-bundle,
                   print-apply, verify-final, audit-workspace. NEVER claw, NEVER a shell.
- render.ts:       RenderModel has optional, degrade-to-muted views (setup/nav/discovery/
                   timeline/foundation/tier3/executorDryRun). renderHtml() composes section
                   blocks; each *Block(view) renders a muted placeholder when its view is absent.
- scripts/run-guards.js (CI-enforced via .github/workflows/a2-harness-panel.yml, #129):
                   static-grep guards over src/*.ts. Forbids in live code: network/telemetry/
                   broker/ollama/:11434, fs.* (any), watchers, polling (setInterval/Timeout/
                   Immediate), SecretStorage, chain-write literals, approval-line composition,
                   and process spawning in any module EXCEPT helperRunner.ts (which itself may
                   not use exec/eval/spawnSync/shell:true).
- tier3EvidenceSnapshot.ts (#128): exports EVIDENCE_SNAPSHOT_SCHEMA, EvidenceSnapshotView,
                   viewFromSnapshot(snap), parseEvidenceSnapshot(raw: string),
                   renderEvidenceSnapshotHtml(view). Pure; no IO; fail-closed; zero controls.
                   NOT referenced by render.ts / extension.ts / panel.ts yet.
```

## 4. Implementation Goal

Surface the merged read-only evidence snapshot inside the live panel as a new read-only section,
via **Option A (operator-provided snapshot text)**, with zero new spawn capability and zero
execution controls. The operator runs the collector themselves (outside the panel); the panel
accepts the snapshot as text, parses it with the already-shipped `parseEvidenceSnapshot`, and
renders it with the already-shipped `renderEvidenceSnapshotHtml`.

Desired read-only UI behavior (carried verbatim from the goal context):

```text
show proposed executor plan / evidence status output
show plan/scope/per-step summary (via the snapshot's rows/subjects/caveats)
show would-create-worktree: no
show would-write-files: no
print external dry-run / collector command only (guidance text, never a control)
no executor spawn from webview
no create/write/apply/approve control
```

## 5. Non-Goals

```text
- no execution control of any kind (no Run / Apply / Approve / Create Worktree / Cleanup).
- no evidence gathering inside a guard-safe module; the snapshot is the SOLE input.
- no helper allowlist change in this lane (that is Option B's own scoped, guard-reviewed lane).
- no fs read in the panel for acquisition (the panel uses no fs; Option A acquires via text).
- no model / broker / runtime / network / Vault / :11434 wiring.
- no Tier 4 packaging.
- no autonomous / preapproval / fake-TTY approval path.
```

## 6. Approved Future Touch Surfaces (the next implementation lane may edit ONLY these)

```text
EDIT ide/vscode/a2-harness-panel/src/render.ts
  - add optional field to RenderModel:  evidenceSnapshot?: EvidenceSnapshotView | null
    (import the type from ./tier3EvidenceSnapshot)
  - add an evidenceSnapshotBlock(view) wrapper that:
      * when absent → renders a muted placeholder section
        (id "tier3-evidence-snapshot"-consistent; guidance text "no snapshot — run the
        read-only collector and paste its output", NOT a button), mirroring executorDryRunBlock.
      * when present → returns renderEvidenceSnapshotHtml(view) (the merged renderer's output).
  - insert ${evidenceSnapshotBlock(model.evidenceSnapshot)} into renderHtml() after
    ${executorDryRunBlock(model.executorDryRun)} (currently render.ts:626).

EDIT ide/vscode/a2-harness-panel/src/extension.ts
  - import parseEvidenceSnapshot (and EvidenceSnapshotView if needed) from ./tier3EvidenceSnapshot.
  - add a session field holding operator-provided snapshot text (default null).
  - add buildEvidenceSnapshotView(): returns null when no snapshot text is set; otherwise
    parseEvidenceSnapshot(session.<field>). NO fs, NO spawn.
  - add evidenceSnapshot: buildEvidenceSnapshotView() to the model() return (extension.ts:287-300).
  - add a read-only field-setter / input path (uiAction) so the operator can paste snapshot text;
    this sets a field only and triggers rerender() — it runs NO helper subcommand and spawns nothing.

ADD ide/vscode/a2-harness-panel/test/*.test.ts  (new tests only — see §10)
```

Reuse **as-is, unchanged**: `src/tier3EvidenceSnapshot.ts` (the merged renderer — do not edit it).

## 7. Forbidden Surfaces (the next implementation lane must NOT touch)

```text
- src/helperRunner.ts            (single spawn boundary; ALLOWED_SUBCOMMANDS/ALLOWED_FLAGS frozen
                                  under Option A)
- src/panel.ts                   (webview wiring; no new postMessage type / no control binding)
- src/tier3EvidenceSnapshot.ts   (merged pure renderer; reuse, do not modify)
- scripts/a2-ide-harness.sh      (helper script — Option B only, separate lane)
- scripts/run-guards.js          (guards stay as-is; the new code must PASS them, not relax them)
- .github/workflows/a2-harness-panel.yml (CI; no loosening)
- rust/** , schemas/** , services/** , hq/** , runtime, Dockerfiles, *.service, .vscode/
```

## 8. UI Behavior To Add

```text
- a new read-only section rendered by renderEvidenceSnapshotHtml() (id "tier3-evidence-snapshot"):
    Tier 3 status line; evidence rows (last proven run, evidence worktree, written file,
    approval-result, apply-bundle, checkpoint manifest, payload sha256, apply-result mode,
    control checkout, partial smoke worktrees); subjects; caveats; links; next-safe-action
    as DISPLAY-ONLY text.
- would-create-worktree: no / would-write-files: no — read-only facts, never controls.
- when no snapshot is set: a muted placeholder with guidance text only (no button/onclick).
- fail-closed: a bad/mismatched/unparseable snapshot renders ONLY the "unsupported snapshot"
  notice (already implemented in the renderer); never fabricates readiness.
- zero controls in the section: no button, onclick, data-subcommand, data-ui-action,
  command, or postMessage inside the snapshot section.
```

## 9. Renderer / Helper Boundaries (must remain unchanged)

```text
- single spawn point: helperRunner.ts stays the ONLY spawn; Option A adds NO spawn at all.
- snapshot-only: the section's SOLE input is operator-provided snapshot text; no guard-safe
  module gathers or re-derives any field.
- no fs in the panel: acquisition is text (paste), not a file read; the panel still imports no fs.
- fail-closed semantics owned by the merged renderer; the integration must not add its own
  fallback that fabricates values.
- approval stays a human-typed terminal step; the panel only ever PRINTS/show guidance.
```

## 10. Test Plan (tests-first; all under test/, new files only)

```text
[ ] render: with an EvidenceSnapshotView present, renderHtml() output includes the
    "tier3-evidence-snapshot" section and its rows; with evidenceSnapshot absent/null, the
    muted placeholder renders and contains NO control (no <button>, onclick, data-subcommand,
    data-ui-action, postMessage).
[ ] render: a fail-closed (unsupported) view renders ONLY the unsupported notice — no rows leak.
[ ] extension: setting operator-provided snapshot text → buildEvidenceSnapshotView() returns a
    parsed view; a bad/mismatched snapshot → the fail-closed view; no text → null.
[ ] guards: test/guards.test.ts still PASS (scripts/run-guards.js exits 0; "guards PASS") — the
    new render.ts/extension.ts code introduces no fs/spawn/network/:11434/watcher/polling/secret/
    chain-write/approval-compose.
[ ] no-control scan: the integrated snapshot section contains no execution control.
[ ] build/CI: tsc clean; npm test green; .github/workflows/a2-harness-panel.yml (#129) runs it.
```

## 11. Guard Scans (must run before commit in the implementation lane)

```text
- node scripts/run-guards.js  → exits 0, prints "a2-harness-panel guards PASS".
- grep the new section for forbidden tokens: 11434 | ollama | broker | fetch( | child_process |
  spawn( | fs\. | setInterval | setTimeout | SecretStorage | onclick | data-subcommand
  (expected: none in live code paths for the snapshot section; literals only in comments/strings).
- confirm git diff touches ONLY src/render.ts, src/extension.ts, and new test/ files.
```

## 12. STOP Gates (for the future implementation lane)

```text
- STOP before adding any execution control to the panel or the snapshot section.
- STOP before spawning anything (Option A adds no spawn; if a spawn is contemplated, it is
  Option B → a separate guard-reviewed lane).
- STOP before introducing fs/network/:11434/broker/ollama/watcher/polling/secret references.
- STOP before editing helperRunner.ts, panel.ts, tier3EvidenceSnapshot.ts, run-guards.js,
  the helper script, or CI.
- STOP before any model/broker/runtime/Vault/:11434 call.
- STOP before push/PR/merge — explicit operator approval required.
- The lane edits live IDE/panel code: it requires the exact approval token (§ draft prompt) to BEGIN.
```

## 13. Risk Assessment

```text
- Surface size: SMALL (Option A) — two edited files (render.ts, extension.ts) + new tests; the
  renderer is reused unchanged; no spawn/allowlist/CI change.
- Guard risk: LOW — CI guards (#129) statically forbid the dangerous patterns; the lane is
  designed to PASS them, not relax them. Main risk is accidentally importing fs or a control.
- Behavioral risk: LOW — fail-closed semantics already proven in the merged renderer (#128 tests).
- Trust risk: must not fabricate readiness; muted placeholder must carry guidance text, not a button.
- Out-of-scope creep risk: MEDIUM — resist sliding into Option B (helper allowlist) or any write
  control; both are explicitly forbidden here and require separate approval.
```

## 14. Recommended Implementation Lane

```text
Name: A2 Tier 3 Read-Only Status Panel Integration — Option A (operator-provided snapshot)
Objective: wire render.ts + extension.ts to render an operator-provided a2-tier3-evidence-snapshot.v0
           via the merged renderer; tests-first; CI-gated by #129; read-only; zero controls.
Approval: requires the exact token in the companion draft prompt before any edit.
Boundary: edits ONLY src/render.ts, src/extension.ts, and new test/ files; no spawn/allowlist/CI/
          helper/renderer change; no model/broker/runtime/Vault/:11434.
Exit: tests + guards + tsc green locally; local commit only; no push; PR gated on operator review.
```
