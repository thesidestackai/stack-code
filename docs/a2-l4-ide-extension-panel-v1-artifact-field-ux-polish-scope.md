# A2-L4 IDE Extension Panel v1 — Artifact Field UX Polish Scope (Docs-Only) — 2026-06-07

> Docs-only scope card. It implements NOTHING: no extension/render/panel/button edit, no helper change,
> no Rust/runtime/schema change, no preview/approval/apply-bundle/apply run, no model/broker/runtime
> call. It scopes a small FUTURE GUI polish patch surfaced by the operator GUI smoke of the merged v1
> panel.

---

## 1. Executive Summary

The A2 IDE Extension Panel v1 is merged on `main` (`377e25e`,
[`ide/vscode/a2-harness-panel/`](../ide/vscode/a2-harness-panel/)). Headless and structural smoke passed
(guards, compile, 41 tests, rendered-HTML structure, button→subcommand behavior), and the operator GUI
smoke confirmed the panel opens, the Safety / Stop Gates banner shows, the early safe buttons work, and
no Run-\* buttons exist. The smoke surfaced one **UX discoverability gap**: later-stage buttons
(Verify Final, Show/Copy Approval, Show/Copy Apply) require artifact fields (`target`, `after-sha`,
`preview-bundle`, `approval-output`, `apply-bundle`, …) that the **visible UI provides no control to
set** — only Select Workspace and Select Plan are exposed. The panel's missing-field refusals are
correct and safe; they are simply not actionable from the GUI. This card scopes a small, safe polish to
close that gap. It performs **no implementation**.

---

## 2. GUI Smoke Finding

```text
Panel opened:                              YES
Safety / Stop Gates banner visible:        YES
Workspace selected:                        YES
Plan selected:                             YES
Dangerous Run buttons absent:              YES
Validate Input:                            PASS
Audit Workspace:                           PASS
Find Artifacts:                            PASS
Show/Copy Preview Command:                 PASS
Live preview / approval / apply-bundle / apply run: NO
Target unchanged:                          YES
No .claw artifacts created:                YES

Blocked-but-safe (the finding):
  Verify Final         blocked — "Set these first for verify-final: target, after-sha"
  Show/Copy Approval   blocked — needs preview-bundle + approval-output (no visible control)
  Show/Copy Apply      blocked — needs apply-bundle (no visible control)
```

The refusals are the panel's `optionsFor()` missing-field guard working as designed (it sets a notice
and re-renders without spawning the helper). The issue is purely **discoverability**: there is no way to
supply those fields from the visible UI.

---

## 3. Source of Truth

```text
Merged v1 (origin/main @ 377e25e):
  ide/vscode/a2-harness-panel/src/buttons.ts      PANEL_BUTTONS catalog (UI + helper buttons)
  ide/vscode/a2-harness-panel/src/extension.ts    handleUiAction(...) + optionsFor(...) missing-field guard
  ide/vscode/a2-harness-panel/src/render.ts        inputRow(...) field display + button rendering
  ide/vscode/a2-harness-panel/src/panel.ts         webview wiring (uiAction / runSubcommand / copyOutput)
  ide/vscode/a2-harness-panel/src/helperRunner.ts  ALLOWED_FLAGS per subcommand (the fields each needs)
  docs/runbooks/a2-ide-extension-panel.md          v1 operator runbook
  handoffs/a2_ide_extension_panel_implementation_report_2026-06-07.md
Operator GUI smoke evidence (preceding lane).
```

Exact code evidence (read read-only this lane):

```text
extension.ts handleUiAction WIRES 9 actions (≈L154-L185):
  selectWorkspace, selectPlan, selectPreviewBundle, selectGeneratorResult, selectApprovalResult,
  selectApprovalOutput, selectApplyBundle, selectTarget, setAfterSha
buttons.ts PANEL_BUTTONS RENDERS only 2 of those as UI buttons (+ openRunbook, exportEvidence):
  select-workspace (selectWorkspace), select-plan (selectPlan)
render.ts DISPLAYS all 9 input rows (workspace, plan, preview-bundle, generator-result,
  approval-result, approval-output, apply-bundle, target, after-sha) as "(not set)" when unset.
```

---

## 4. What Worked

```text
- Panel opens via "A2 Harness: Open Panel"; Safety / Stop Gates banner always visible.
- Select Workspace + Select Plan set their fields; selected paths display correctly.
- Early helper buttons (validate-input, audit-workspace, find-artifacts, print-preview) render stdout.
- Missing-field refusal is SAFE: later buttons do not spawn the helper without their required fields;
  they show a notice ("Set these first for <sub>: <flags>") and re-render. No live A2 ran; target
  unchanged; no .claw created.
- The handlers for the unreachable fields ALREADY EXIST in extension.ts — only the buttons are missing.
```

---

## 5. What Was Blocked / Confusing

```text
- Verify Final cannot run: target + after-sha have no visible setter.
- Show/Copy Approval cannot run: preview-bundle + approval-output have no visible setter.
- Show/Copy Apply cannot run: apply-bundle has no visible setter.
- The fields are SHOWN (as "(not set)") but there is no control to set them, so the operator sees the
  requirement but cannot satisfy it from the GUI. The notice names the flags but not how to set them.
```

---

## 6. Root Cause

```text
- The panel tracks all artifact fields in session state and DISPLAYS them (render.ts inputRow x9).
- extension.ts handleUiAction already implements setters for all 9 fields (selectTarget, setAfterSha,
  selectPreviewBundle, selectGeneratorResult, selectApprovalResult, selectApprovalOutput,
  selectApplyBundle) via showInputBox.
- buttons.ts PANEL_BUTTONS only EXPOSES select-workspace and select-plan as UI buttons; the other 7
  select/set actions have working handlers but NO button to trigger them.
- panel.ts wires '.btn.ui[data-ui-action]' clicks generically, so adding the missing buttons to the
  catalog is sufficient to reach the existing handlers — no new handler logic is required for Option A.
- Net: safe refusal works; the gap is missing UI controls (discoverability), not a safety defect.
```

---

## 7. Recommended UX Polish

Recommended: the **hybrid (Option C)** — add the missing explicit controls, and let Find Artifacts
populate discoverable fields read-only.

```text
1. Add explicit UI buttons (catalog entries) for the 7 currently-unreachable fields, matching the
   existing Select Workspace / Select Plan pattern and reusing the EXISTING handlers:
     Select Target, Set After SHA, Select Preview Bundle, Select Preview Generator Result,
     Select Approval Result, Set Approval Output, Select Apply Bundle.
   (Option A — smallest, safest; no new handler logic, just catalog + render coverage.)
2. Improve Find Artifacts so it can POPULATE discovered fields read-only from the workspace + .claw:
     preview-bundle.json, preview-generator-result.json, approval-result.json, apply-bundle.json,
     and (where unambiguous) the target path from plan.yaml's write_target/after_file.
   (Option B — better UX; every populated value MUST be displayed before any button uses it, and the
   operator can override; no value is auto-consumed silently.)
3. Keep all buttons print/validate-only; keep Find Artifacts read-only.
4. Keep live A2 execution forbidden; never compose the approval line; never auto-populate after-sha
   from a source that would imply the chain ran.

Option A pros: obvious, matches existing pattern, safest for v1. Cons: more buttons; manual paths.
Option B pros: less manual path handling; aligns with find-artifacts. Cons: needs careful path/hash
rules and must NEVER infer an unsafe target path without operator visibility.
Recommendation: ship Option A first (tiny), then layer Option B's read-only population behind it.
```

---

## 8. Candidate Implementation Surface

```text
ide/vscode/a2-harness-panel/src/buttons.ts        add the 7 UI select/set buttons to PANEL_BUTTONS
ide/vscode/a2-harness-panel/src/render.ts         render the new buttons (group them near the inputs)
ide/vscode/a2-harness-panel/src/panel.ts          (likely no change — generic ui-action wiring already)
ide/vscode/a2-harness-panel/src/extension.ts      (Option B only) Find Artifacts read-only field population
ide/vscode/a2-harness-panel/test/buttons.test.ts  assert the new buttons exist + map to existing actions
ide/vscode/a2-harness-panel/test/render.test.ts   assert new controls render; missing-field notice stays
ide/vscode/a2-harness-panel/test/manifest.test.ts (only if a command surface changes — likely not)
ide/vscode/a2-harness-panel/test/guards.test.ts   guards still PASS
docs/runbooks/a2-ide-extension-panel.md           document the new field controls
handoffs/a2_ide_extension_panel_artifact_field_ux_polish_report_2026-06-07.md   future report

Do NOT touch: ide/vscode/claw-status-panel/, rust/, runtime/, schemas/. Do NOT change the helper
script unless discovery proves Option B field-discovery needs a helper-output contract change — and if
so, escalate that as its own scope, do not inline it.
```

---

## 9. Non-Goals

```text
- NOT implementing the polish in this lane (scope only).
- NOT adding any Run-* execution button or any live A2 action.
- NOT composing or capturing the approval line in the panel.
- NOT auto-populating after-sha in a way that implies the chain ran.
- NOT silently consuming any discovered path without displaying it first.
- NOT touching claw-status-panel, Rust, runtime, or schemas.
- NOT a full UI redesign; this is a targeted field-control gap fix.
```

---

## 10. Safety Boundaries

```text
- The panel stays print/validate-only: new controls only SET session fields (via showInputBox) or
  POPULATE them read-only from find-artifacts; they never spawn claw or any chain-write command.
- Missing-field refusal remains: a button still refuses (notice + re-render) until its fields are set.
- No auto-approval, no hidden apply, no approval-line composition, no webview approval capture.
- No model / broker / runtime / :11434 / Vault call. No fs writes from the panel (existing guards hold).
- Option B population is READ-ONLY and operator-visible: every discovered value is displayed and
  overridable before any button uses it; unsafe/ambiguous target paths are shown, never silently used.
- The single spawn boundary (helperRunner.ts, allowlisted subcommands) is unchanged.
```

---

## 11. Validation Plan

A future implementation must demonstrate (offline; fixtures only; no live A2):

```text
- npm run lint (guards) PASS; npm run compile clean; npm test green.
- rendered HTML includes the new field controls (Select Target, Set After SHA, Select Preview Bundle,
  Select Preview Generator Result, Select Approval Result, Set Approval Output, Select Apply Bundle).
- each new UI button maps to an existing handleUiAction case (no new spawn path).
- missing-field notices still appear until fields are set (refusal preserved).
- buttons still run only read-only/print helper subcommands; no live A2 command is executed.
- no approval-line composition; no hidden apply; no auto-approval (run-guards still PASS).
- Option B (if included): find-artifacts population is read-only, displays every discovered path before
  use, and never infers an unsafe target path without operator visibility.
- a follow-up GUI smoke can COMPLETE Verify Final (set target + after-sha, MISMATCH then MATCH) and
  reach print-approval / print-apply field entry — all without running the live chain.
- existing claw-status-panel + all other tests still pass unchanged.
```

---

## 12. STOP Conditions

```text
- live preview / approval / apply-bundle / apply requested or run
- the panel directly executes `claw plan ...`
- auto-approval introduced
- hidden apply introduced
- the approval phrase is captured from the webview or composed in panel source
- a model / broker / runtime / :11434 / Vault call is introduced
- target mutation introduced
- .claw mutation introduced
- unsafe path inference without operator visibility (Option B)
- claw-status-panel touched
- the single helperRunner spawn boundary is widened beyond the allowlist
```

---

## 13. Future Lanes

```text
1. A2 IDE Extension Panel v1 Artifact Field UX Polish Scope Review / Push PR (this card).
2. A2 IDE Extension Panel v1 Artifact Field UX Polish Scope exact-head merge gate.
3. (implementation) A2 IDE Extension Panel v1 Artifact Field UX Polish — Option A buttons first, then
   optional Option B read-only find-artifacts population, validated per §11; panel stays
   print/validate-only.
4. (operator) Re-run the GUI smoke to completion (Verify Final MISMATCH+MATCH; reach approval/apply
   field entry) on a disposable workspace; no live chain.
```

---

## 14. Final Recommendation

```text
Proceed with a small, print/validate-only polish: add the 7 missing field controls (reusing the
existing extension.ts handlers — Option A, smallest and safest), then optionally layer read-only
find-artifacts field population (Option B) behind it. Keep every button print/validate-only, keep live
A2 execution forbidden, keep the missing-field refusal, and never compose/capture the approval line or
infer an unsafe target path silently. Review and merge this scope before implementing.
```
