# A2 IDE Extension Panel — v1 Implementation Report — 2026-06-07

> Implementation closeout for the v1 visual VS Code / Cursor panel that wraps the print/validate-only
> A2 IDE harness v0. The build lane ran NO A2 command (no preview / approval / apply-bundle / apply),
> made no model/broker/runtime call, modified no target or `.claw` artifact, and did not touch the
> existing `claw-status-panel` package, Rust, schemas, or runtime.

---

## 1. Approval

```text
Operator token (affirmative standalone line): APPROVED: Execute A2 IDE extension panel implementation
Gate: satisfied. Implementation proceeded under the merged scope + DRAFT prompt (origin/main @ 4d8845a).
```

---

## 2. Source of Truth

```text
docs/a2-l4-ide-extension-panel-scope.md                                  (merged @ 4d8845a)
handoffs/a2_ide_extension_panel_implementation_prompt_DRAFT_2026-06-07.md (merged @ 4d8845a)
scripts/a2-ide-harness.sh                                                (v0 helper; the execution boundary)
ide/vscode/claw-status-panel/                                           (convention re-derived, NOT modified)
```

---

## 3. Discovery Findings

```text
- Package manager / build: npm; tsc -p . -> out/; tests = tsc -p tsconfig.test.json -> out-test/ then
  mocha ./out-test/test; lint = node scripts/run-guards.js. No eslint/prettier.
- Extension source under src/ (commonjs, ES2020, strict). main = ./out/extension.js.
- claw-status-panel is the A2-L3 status observer; its guards FORBID chain-write command references and
  direct .claw reads, so it cannot host workflow buttons -> a SEPARATE sibling package is required.
- A sibling package re-derives the argv-bounded-subprocess + static-guard + mocha patterns WITHOUT a
  dependency on claw-status-panel.
- The v0 helper is mode 100755 with a `#!/usr/bin/env bash` shebang -> it can be spawned directly.
```

---

## 4. Implementation Surface (all NEW)

```text
ide/vscode/a2-harness-panel/package.json            extension manifest (one command, helperPath config)
ide/vscode/a2-harness-panel/tsconfig.json
ide/vscode/a2-harness-panel/tsconfig.test.json
ide/vscode/a2-harness-panel/.mocharc.json
ide/vscode/a2-harness-panel/.gitignore
ide/vscode/a2-harness-panel/.vscodeignore
ide/vscode/a2-harness-panel/README.md
ide/vscode/a2-harness-panel/src/helperRunner.ts     the SINGLE process-spawn boundary (argv-bounded)
ide/vscode/a2-harness-panel/src/buttons.ts          safe button catalog (no Run-* buttons)
ide/vscode/a2-harness-panel/src/render.ts           pure HTML render + always-on Safety/Stop Gates
ide/vscode/a2-harness-panel/src/panel.ts            webview lifecycle + click wiring
ide/vscode/a2-harness-panel/src/extension.ts        activation, command, message handling
ide/vscode/a2-harness-panel/src/clipboard.ts        single-field verbatim copy
ide/vscode/a2-harness-panel/scripts/run-guards.js   static-grep guards
ide/vscode/a2-harness-panel/test/_paths.ts
ide/vscode/a2-harness-panel/test/helper_runner.test.ts
ide/vscode/a2-harness-panel/test/buttons.test.ts
ide/vscode/a2-harness-panel/test/render.test.ts
ide/vscode/a2-harness-panel/test/clipboard.test.ts
ide/vscode/a2-harness-panel/test/guards.test.ts
ide/vscode/a2-harness-panel/test/manifest.test.ts
ide/vscode/a2-harness-panel/test/fixtures/audit_workspace_preview_ready.txt
docs/runbooks/a2-ide-extension-panel.md             operator runbook (v1)
handoffs/a2_ide_extension_panel_implementation_report_2026-06-07.md  (this report)

NOT touched: ide/vscode/claw-status-panel/, rust/, schemas/, runtime/, scripts/a2-ide-harness.sh,
.vscode/tasks.json. (node_modules/ and out/ are gitignored and not committed.)
```

---

## 5. What the Panel Does

```text
- One command: "A2 Harness: Open Panel" opens a webview with: an always-on Safety/Stop Gates banner,
  a Workspace/Plan/Artifact selection table, an Actions section of safe buttons, and a Helper output
  area that renders the helper's stdout verbatim with a single Copy button.
- Helper buttons map 1:1 to read-only/print subcommands: validate-input, audit-workspace,
  find-artifacts, print-preview, print-approval, print-apply-bundle, print-apply, verify-final.
- print-* buttons are labelled "Show/Copy ... Command": they display and let the operator copy the
  helper-printed command to run themselves at a real terminal.
- Open Runbook opens docs/runbooks/a2-ide-harness-workflow.md; Export Evidence Summary opens an
  UNSAVED markdown document (the panel writes no file).
```

---

## 6. Safety Model Preserved

```text
- PANEL-SPAWN-BOUNDED: the only binary spawned is the helper (basename must equal a2-ide-harness.sh),
  with an allowlisted subcommand + per-subcommand allowlisted flags, NO shell. Enforced by
  helperRunner.ts (HelperRunnerRefusal) + run-guards (only helperRunner may spawn) + tests.
- PANEL-NO-CHAIN-WRITE: the panel never spawns `claw` or `claw plan run/approve/apply-bundle/apply`.
  Those strings appear only in the helper's rendered stdout and in deny-list/prohibition text.
- PANEL-NO-APPROVAL-COMPOSE: the panel never composes `apply <step-id> <preview_sha256>` and never
  captures it from the webview. Approval stays real-terminal, human-typed. Enforced by guard + tests.
- NO Run Preview / Run Approval / Run Apply-Bundle / Run Apply button (asserted absent by tests).
- apply-bundle = generator (writes no target); `claw plan apply` = the only target writer, run once by
  the operator. The panel only prints these commands.
- No auto-approval, no hidden apply. No model/broker/runtime/:11434 call. No secrets. No `fs` access
  from the panel (the helper does the read-only .claw inspection). No watcher/polling/auto-refresh.
```

---

## 7. Validation Results

```text
node scripts/run-guards.js            PASS ("a2-harness-panel guards PASS (6 src files audited)")
npm install --ignore-scripts          OK (76 packages; mocha/typescript/@types only, devDependencies)
npm run compile (tsc -p .)            PASS (clean)
npm test (mocha)                      PASS — 41 passing:
    helperRunner argv/allowlist/binary-boundary/value-refusals/spawn-injection (20)
    buttons allowlist + dangerous-button-absence + required-buttons (8)
    render structure/output/inputs incl. always-on Safety section + HTML escaping (9)
    clipboard single-field verbatim (3)
    manifest one-command/no-broad-activation/helperPath-only (5 — counted within above groups)
    guards static-grep exits 0 (1)
forbidden top-level surface scan      NO_FORBIDDEN_SURFACE (only ide/vscode/a2-harness-panel touched)
live-A2 execution scan (src)          only prohibition text / comments / deny-list constants
process/network scan (src)            only the single child_process import in helperRunner.ts (the
                                      designated spawn boundary); no exec/eval/spawnSync/shell:true,
                                      no fetch/http/https/ws/net, no :11434/broker/secret in live code
git diff --check                      clean
```

---

## 8. Build-Lane Safety Attestation

```text
preview run:                 NO
approval run:                NO
apply-bundle run:            NO
apply run:                   NO
model/broker call:           NO
runtime touched:             NO
target modified:             NO
.claw artifacts modified/deleted: NO
auto-approval:               NO
hidden apply:                NO
dangerous (Run-*) buttons:   NONE
direct claw spawn:           NO
claw-status-panel modified:  NO
Rust / schema / runtime:     NO
```

---

## 9. Remaining Limitations / Future v2

```text
- No live VS Code host test (extension.ts is type-checked but not executed in this offline lane);
  unit coverage is at the module level (helperRunner/buttons/render/clipboard/guards/manifest).
- Not packaged as a .vsix; a packaging/marketplace lane is out of scope.
- Inputs are entered via showInputBox; a future v2 could add native file pickers + an in-panel
  diff viewer, each behind its own scope lane, still with no execution buttons.
- Dangerous execution buttons remain intentionally excluded; revisiting them requires a separate,
  independently-reviewed safe-gating design (approval stays real-terminal; apply stays operator-run).
```

---

## 10. Next Lane

```text
A2 IDE Extension Panel v1 Review / Push PR (the new package + runbook + this report).
Do not run the live A2 workflow from the build or review lane; live exercise is operator-driven at a
real terminal.
```
