# A2 IDE Extension Panel — Workspace-First UX — Implementation Report — 2026-06-08

> Implementation of the merged workspace-first UX scope
> ([`docs/a2-l4-ide-extension-panel-workspace-first-ux-scope.md`](../docs/a2-l4-ide-extension-panel-workspace-first-ux-scope.md))
> and its token-gated prompt
> ([`handoffs/a2_ide_extension_panel_workspace_first_ux_implementation_prompt_DRAFT_2026-06-08.md`](a2_ide_extension_panel_workspace_first_ux_implementation_prompt_DRAFT_2026-06-08.md)).
> Executed under the exact approval token. The panel remains **print/validate-only**: no live apply, no
> auto-approval, no hidden apply, approval stays real-terminal and human-typed.

---

## 1. Summary

The A2 IDE Extension Panel now opens **workspace-first**: on open (and on an explicit **Refresh Workspace
Status** gesture) it runs a single read-only inspection and shows, without the operator typing a path:

- a **Workspace status** section (helper path, claw binary, workspace root, plan, target, after_sha,
  preview/approval/apply artifacts, final verification),
- a **Next safe step** recommendation driven by a read-only state machine,
- a **Discovered (read-only)** section listing plan/artifact candidates (shown before use),
- an **Evidence timeline** of the session's safe actions, folded into the unsaved evidence summary.

No mutating capability was added. The single spawn boundary (`helperRunner.ts`) is unchanged in shape.

## 2. Safe architecture (why it stays read-only)

The package's static guards (`scripts/run-guards.js`) structurally forbid `fs` in panel source, any
network/`:11434`/watcher/timer, and process spawning outside `helperRunner.ts`. The workspace-first layer
therefore derives everything through capabilities that already exist and are already safe:

- **Setup status + chain state + artifact paths** ← parsed from the helper's existing read-only
  `audit-workspace` (and `help`) stdout. The panel never re-walks `.claw` itself and never spawns `claw`.
- **Plan discovery** ← `vscode.workspace.findFiles('**/plan.yaml', …)` — a one-shot editor index search,
  not node `fs`, not a watcher.
- **claw binary** is reported as `configured`/`unknown` (a path parsed from the helper's usage output),
  never as "found": verifying the binary would require `fs`/spawn, both forbidden. The operator runs claw
  themselves at a real terminal; the panel never needs it.

Detection is one-shot per gesture (open + Refresh). There is no watcher, no polling, and no timer.

## 3. Files changed

```text
ide/vscode/a2-harness-panel/src/discovery.ts        (new) read-only parsers (audit/find/help) + candidate selection
ide/vscode/a2-harness-panel/src/setupStatus.ts      (new) pure setup-status model
ide/vscode/a2-harness-panel/src/stateMachine.ts     (new) read-only next-step state machine + safety guard
ide/vscode/a2-harness-panel/src/evidence.ts         (new) pure evidence-timeline model/formatter
ide/vscode/a2-harness-panel/src/buttons.ts          (edit) + "Refresh Workspace Status" workflow button
ide/vscode/a2-harness-panel/src/render.ts           (edit) + setup/next-step/discovery/timeline sections
ide/vscode/a2-harness-panel/src/extension.ts        (edit) read-only refresh wiring + auto-populate + timeline
ide/vscode/a2-harness-panel/test/discovery.test.ts  (new)
ide/vscode/a2-harness-panel/test/setupStatus.test.ts(new)
ide/vscode/a2-harness-panel/test/stateMachine.test.ts(new)
ide/vscode/a2-harness-panel/test/evidence.test.ts   (new)
ide/vscode/a2-harness-panel/test/buttons.test.ts    (edit) Refresh button assertions
ide/vscode/a2-harness-panel/test/render.test.ts     (edit) workspace-first section assertions
docs/runbooks/a2-ide-extension-panel.md             (edit) workspace-first docs + Refresh row
handoffs/a2_ide_extension_panel_workspace_first_ux_implementation_report_2026-06-08.md  (new, this file)
```

## 4. What shipped

- **setup-status detector** (`setupStatus.ts`): pure `computeSetupStatus(probe)` → honest tri-states;
  claw is `configured`/`unknown`, never claimed found.
- **artifact discovery (Option B)** (`discovery.ts` + `extension.ts`): parses `audit-workspace` /
  `find-artifacts`; `selectCandidate` enforces exactly-one→auto / zero→none / many→select-needed; plan
  discovery via `findFiles`; auto-fill only the single unambiguous candidate into an empty field, always
  shown in the field table + discovery section.
- **next-step state machine** (`stateMachine.ts`): the 13 scoped states → exactly one safe step;
  `assertSafe` + tests prove it can never recommend a `Run-*`/chain executor.
- **visual diff / preview readiness**: implemented as a read-only **preview-readiness status** (artifact
  presence + chain state surfaced in setup status / next step). A byte-level before/after content diff is
  intentionally **not** implemented — rendering artifact file contents would require `fs`, which the
  guards forbid; this is recorded as a deliberate, safety-preserving limitation, not an oversight.
- **evidence timeline** (`evidence.ts` + `extension.ts`): ordered, session-local, bounded; rendered in
  the panel and folded into the unsaved evidence summary; print steps recorded as printed-not-run; writes
  no file.
- **Refresh Workspace Status** workflow button + auto-refresh on open.

## 5. Validation

```text
npm run compile : PASS (tsc -p ., clean)
npm run lint    : PASS (a2-harness-panel guards PASS — 10 src files audited)
npm test        : PASS (113 passing; was 49 — +64 new across discovery/setupStatus/stateMachine/
                  evidence/render/buttons)
```

Source safety scans over `src/` (all hits are prohibition/safety-banner/allowlist context only):

```text
Run-* execution button / live executor        : none (only FORBIDDEN list + safety banner text)
claw plan run/approve/apply-bundle/apply (live): none (only CHAIN_WRITE_FRAGMENTS refusal + banner text)
model / broker / :11434 / network             : none (only safety banner + evidence attestation)
fs. / readFileSync / watcher / setInterval / setTimeout : none
approval-phrase composition                   : none
spawn( / child_process                        : helperRunner.ts only
```

## 6. Safety attestations

```text
live preview / approval / apply-bundle / apply run : no
claw spawned                                       : no (only the helper is spawned)
model / broker / runtime / :11434 call             : no
approval phrase captured/composed in webview       : no
Run-* button added                                 : no (a read-only "Refresh Workspace Status" was added)
.claw artifact written / target modified           : no
filesystem watcher / polling / timer added         : no
node `fs` used in panel source                     : no (guard-enforced)
helperRunner spawn behavior broadened              : no (same argv-bounded, helper-basename-only shape)
runtime / Rust / schemas / services / HQ touched   : no
```

## 7. Stop conditions (none hit)

No discovery required unsafe inference (zero/many candidates → select-needed, never guessed). No detection
required spawning claw or writing a file. No approval phrase was routed through the webview. Preserving
workspace-first UX required no watcher/polling. The helper's read-only output was sufficient for chain
state — the extension never re-derives chain semantics.

## 8. Posture / next lane

This lane committed locally only (no push, no PR). Recommended next lane: review → push → PR for the
workspace-first implementation, then a separately-approved **disposable live-chain GUI artifact-backed
smoke** (its own token-gated lane) to exercise discovery/preview-readiness against real artifacts. The
panel remains print/validate-only; any live (even disposable) chain exercise is a separate, explicitly
approved lane.
