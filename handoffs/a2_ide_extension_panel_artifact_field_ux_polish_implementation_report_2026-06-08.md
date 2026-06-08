# A2 IDE Extension Panel v1 — Artifact Field UX Polish Implementation Report — 2026-06-08

> Implementation closeout for the artifact-field UX polish scoped in
> docs/a2-l4-ide-extension-panel-v1-artifact-field-ux-polish-scope.md. The build lane ran NO A2 command
> (no preview/approval/apply-bundle/apply), made no model/broker/runtime call, modified no target or
> .claw artifact, and did not touch claw-status-panel, Rust, schemas, runtime, or the helper script.
> The panel remains print/validate-only; no execution button was added.

---

## 1. Approval

```text
Operator token (affirmative standalone line):
  APPROVED: Execute A2 IDE extension panel artifact field UX polish implementation
Gate: satisfied. Implementation proceeded under the merged polish scope (origin/main @ e8cad2a).
```

---

## 2. Source of Truth

```text
docs/a2-l4-ide-extension-panel-v1-artifact-field-ux-polish-scope.md   (merged @ e8cad2a)
ide/vscode/a2-harness-panel/                                          (merged panel @ 377e25e)
```

---

## 3. Root Cause (confirmed in discovery)

```text
- extension.ts handleUiAction already implements 9 select/set field handlers (selectWorkspace,
  selectPlan, selectTarget, setAfterSha, selectPreviewBundle, selectGeneratorResult,
  selectApprovalResult, selectApprovalOutput, selectApplyBundle).
- panel.ts wires '.btn.ui[data-ui-action]' clicks generically.
- buttons.ts PANEL_BUTTONS exposed only 2 of those (Select Workspace, Select Plan) as visible buttons.
- render.ts displayed all 9 fields as "(not set)" but had no control to set 7 of them.
=> The 7 missing fields were unreachable from the GUI. Discoverability gap, not a safety defect.
```

---

## 4. What Changed (Option A — smallest, safest)

```text
ide/vscode/a2-harness-panel/src/buttons.ts     PATCHED
  - added 7 field-setter UI buttons reusing the EXISTING handlers:
      Select Target, Set After SHA, Select Preview Bundle, Select Generator Result,
      Select Approval Result, Set Approval Output, Select Apply Bundle.
  - added FIELD_SETTER_ACTIONS set + isFieldSetterAction() + fieldSetterButtons() + workflowUiButtons()
    so render can group field setters next to the fields and workflow actions in Actions.
ide/vscode/a2-harness-panel/src/render.ts      PATCHED
  - field-setter controls now render inside the inputs section, directly under the field table.
  - Actions section renders helper buttons + workflow UI buttons (Open Runbook, Export Evidence).
  - added a "set fields only — never run a chain command" caption.
ide/vscode/a2-harness-panel/test/buttons.test.ts  PATCHED
  - new describe block: all 9 field setters present + map to handled actions; the 7 formerly-unreachable
    fields each have a control; no field-setter/workflow button is a Run-*; workflow UI buttons are
    exactly Open Runbook + Export Evidence.
ide/vscode/a2-harness-panel/test/render.test.ts   PATCHED
  - asserts the field-setter container + all 7 new controls render, inside the inputs section.
docs/runbooks/a2-ide-extension-panel.md           PATCHED
  - documents the field-setter controls (one per field) and that they set fields only.
handoffs/a2_ide_extension_panel_artifact_field_ux_polish_implementation_report_2026-06-08.md  NEW (this).

NOT changed: src/extension.ts (handlers already existed), src/panel.ts (generic wiring already existed),
src/helperRunner.ts (spawn boundary unchanged), test/manifest.test.ts, test/guards.test.ts,
scripts/run-guards.js. Option B (find-artifacts field population) was NOT implemented — Option A fully
closes the discoverability gap with the lowest risk; Option B remains a future lane.
```

---

## 5. Option B Decision

```text
Option B (Find Artifacts auto-populates discovered fields read-only) was DEFERRED, not implemented.
Rationale: Option A (visible field-setter controls reusing existing handlers) fully resolves the GUI
discoverability finding with a minimal, well-tested change and no new spawn/behavior surface. Option B
would add path/hash inference rules and is better scoped + reviewed on its own. Per the scope's "do
Option A first; stop if Option B risks broadening behavior", Option B was stopped before starting.
```

---

## 6. Safety Boundaries Preserved

```text
- New controls SET session fields only (via the existing showInputBox handlers); they spawn nothing.
- No Run Preview / Run Approval / Run Apply-Bundle / Run Apply button (asserted absent by tests).
- helperRunner spawn boundary unchanged: the only binary spawned is still the helper with an
  allowlisted read-only/print subcommand; no claw, no chain-write command.
- No approval-line composition; no webview approval capture; approval stays real-terminal/human.
- apply-bundle = generator; plan apply = only target writer (printed only, never executed).
- No auto-approval, no hidden apply. No model/broker/runtime/:11434/Vault call. No fs writes from the
  panel. No watcher/polling. claw-status-panel untouched.
```

---

## 7. Validation Results

```text
npm run compile (tsc -p .):   PASS (clean)
npm test (mocha):             PASS — 49 passing (helper runner, buttons incl. field-setter polish,
                              render incl. field-setter controls, clipboard, guards, manifest)
npm run lint (run-guards.js): PASS ("a2-harness-panel guards PASS")
forbidden surface guard:      NO_FORBIDDEN_SURFACE (no rust/schemas/runtime/services/hq/
                              claw-status-panel/helper/.vscode/tasks.json touched)
direct live A2 scan:          only deny-list constants / prohibition text / refusal tests
unsafe shortcut scan:         only guard-pattern regexes / prohibition text / the single child_process
                              import in helperRunner.ts (unchanged)
required control scan:        all 7 new controls present in src + tests + runbook
git diff --check:             clean
```

---

## 8. Build-Lane Safety Attestation

```text
preview run: NO | approval run: NO | apply-bundle run: NO | apply run: NO
model/broker call: NO | runtime touched: NO | target modified: NO | .claw artifacts modified: NO
auto-approval: NO | hidden apply: NO | approval phrase composed/captured: NO
claw-status-panel touched: NO | Rust/schemas/runtime touched: NO | helper script touched: NO
spawn boundary broadened: NO
```

---

## 9. Next Lane

```text
A2 IDE Extension Panel Artifact Field UX Polish Review / Push PR (buttons + render + tests + runbook +
this report). Then a follow-up operator GUI smoke can COMPLETE Verify Final (set target + after-sha,
MISMATCH then MATCH) and reach the approval/apply field entry — all on a disposable workspace, no live
chain. Do not run the live A2 workflow from the build or review lane.
```
