# A2 Workspace-First UX — Operator Smoke Evidence — 2026-06-08

> Docs-only evidence handoff. It records that the merged A2 IDE Extension Panel **workspace-first UX**
> passed a **read-only / print-validate** smoke. It implements nothing, runs no A2 command, and makes no
> model/broker/runtime call. The panel remains print/validate-only; approval stays real-terminal and
> human-typed.

---

## 1. Executive Summary

The A2 IDE Extension Panel **workspace-first UX** passed a read-only operator GUI smoke. The panel now
supports the intended flow: **open workspace → inspect setup status → discover plan/artifacts → show next
safe step → print/validate commands → export evidence**.

This does **not** prove a live preview/approval/apply GUI chain. It is a read-only / print-validate smoke
only.

### Evidence provenance (read this first)

This handoff records two complementary layers of evidence, with honest attribution:

- **Headless, independently reproduced in the staging lane** (this session): package build (guards +
  compile + `113 passing` tests), and the helper's read-only/print subcommands run directly against the
  disposable demo workspace — including `verify-final` MISMATCH→exit 3 and MATCH→exit 0, target unchanged,
  no `.claw` created.
- **Operator GUI click-through (operator-reported)**: the rendered-panel states below (Workspace status
  rows, Next safe step, Evidence timeline) were confirmed by the operator in the VS Code Extension
  Development Host. This docs lane did **not** itself launch a GUI window; it records the operator's
  click-through results, which are corroborated by — and consistent with — the headless evidence and the
  unit tests.

---

## 2. Scope of This Smoke

```text
Read-only / print-validate only.
No live preview.
No live approval.
No live apply-bundle.
No live apply.
No target write.
No .claw write.
```

---

## 3. Source of Truth

```text
Workspace-first UX merge (on origin/main):
  PR #101  feat(a2): add workspace-first panel UX   affedf999ef69d26ad8b32c4a22d6357f4a08e2b

Implementation report:
  handoffs/a2_ide_extension_panel_workspace_first_ux_implementation_report_2026-06-08.md
Operator-ready (v1) handoff:
  handoffs/a2_ide_extension_panel_v1_operator_ready_handoff_2026-06-08.md
Runbook:
  docs/runbooks/a2-ide-extension-panel.md
Package:
  ide/vscode/a2-harness-panel/
Helper (print/validate-only; the only binary the panel spawns):
  scripts/a2-ide-harness.sh
```

---

## 4. Environment

```text
smoke worktree:
/mnt/vast-data/git-worktrees/stack-code-a2-workspace-first-gui-smoke-20260608   (branch smoke/a2-workspace-first-gui-smoke-20260608 @ affedf9)

demo workspace:
/mnt/vast-data/tmp/a2-workspace-first-gui-smoke-ws-20260608

demo plan:
/mnt/vast-data/tmp/a2-workspace-first-gui-smoke-ws-20260608/handoff/plan.yaml

demo target:
/mnt/vast-data/tmp/a2-workspace-first-gui-smoke-ws-20260608/sample/demo_target.txt
```

Package build (headless, this session): `npm install --ignore-scripts` OK, guards PASS (10 src files
audited), `tsc` compile clean, `113 passing` unit tests (incl. the state-machine guard that it can never
recommend a chain executor).

---

## 5. Workspace-First UX Evidence

Operator-reported from the GUI click-through, corroborated by the headless smoke and unit tests:

```text
Panel opens.
Safety / Stop Gates banner is visible.
Workspace status renders.
  helper path        : found
  claw binary        : configured        (panel parses the helper `current:` path; never verifies/runs claw)
  workspace root     : detected
  plan.yaml          : found / auto-selected (single candidate handoff/plan.yaml)
  target             : initially unknown, later set
  after_sha          : initially unknown, later set
  preview bundle     : not-found
  approval result    : not-found
  apply bundle       : not-found
  final verification : not-checked, later MATCH
Next safe step renders.
  State after validate/refresh : NO_PREVIEW_ARTIFACTS
  Recommended button           : Show/Copy Preview Command
Discovered (read-only) section : plan.yaml auto-selected; no .claw artifacts (not-started).
Evidence timeline records read-only actions (printed-not-run).
```

Headless corroboration (this session, run directly against the demo workspace):

```text
help            : print/validate-only banner; emits `current:` claw path the panel parses.
validate-input  : exit 0 — OK (next step: print-preview); relative after_file flagged.
audit-workspace : exit 0 — chain state: not-started; no .claw directory.
find-artifacts  : exit 0 — no .claw yet; next-step hint.
print-preview   : prints the claw command only (no execution).
print-approval  : prints the REAL-terminal command only; warns artifacts absent.
print-apply-bundle / print-apply : print generator/executor commands only.
```

---

## 6. Read-Only Helper Evidence

Every panel action maps to one allowlisted read-only/print helper subcommand; the panel spawns only the
helper (basename `a2-ide-harness.sh`), never `claw`. The `Show/Copy …` buttons print/copy commands for the
operator to run themselves at a real terminal. No Run Preview / Run Approval / Run Apply-Bundle / Run Apply
button exists (confirmed in source/built output: `NO_RENDERED_RUN_BUTTONS`).

---

## 7. Verify Final Evidence

```text
Verify Final with a wrong SHA returned MISMATCH (exit 3).
Verify Final with the correct SHA returned MATCH (exit 0).
Target remained unchanged.
```

Known correct target SHA:

```text
14ba4688ebf366d55d42f09d8e16a81779f91a8202fe865f2f903ed3f085afd2
```

---

## 8. Evidence Timeline Evidence

The panel's read-only, session-local evidence timeline recorded the smoke's safe actions in order
(workspace detection, field sets, read-only helper subcommands with exit codes), with print steps recorded
as **printed-not-run**. The exported evidence summary opens as an unsaved untitled markdown document — the
panel writes no file.

---

## 9. Terminal Safety Evidence

```text
target content:
original demo target content

files present:
handoff/plan.yaml
materialized/demo_target.after.txt
sample/demo_target.txt
.vscode/settings.json

No .claw directory appeared.
```

(The `.vscode/settings.json` holds only `a2HarnessPanel.helperPath` pointing at the disposable smoke
worktree's helper; it lives inside the disposable workspace.)

---

## 10. Safety Boundaries Preserved

```text
No live preview run.
No live approval run.
No live apply-bundle run.
No live apply run.
No approval line typed.
No approval phrase composed/captured by the webview.
No model call.
No broker call.
No runtime touch.
No target mutation.
No .claw artifact creation.
No permanent extension install (Extension Development Host session only).
No user IDE settings modified outside the disposable workspace.
```

---

## 11. What Was Not Proved

Be explicit — this is a read-only / print-validate smoke:

```text
A full artifact-backed GUI flow was NOT run.
Real preview-bundle / preview-generator-result / approval-result / apply-bundle controls remain untested
  against generated artifacts (the read-only smoke deliberately did not run preview, so no real chain
  artifacts existed to exercise those fields against).
A disposable live-chain GUI smoke would require a SEPARATE, explicitly token-gated lane.
This handoff does NOT claim a live preview / approval / apply GUI chain was run.
```

---

## 12. Disposable Artifacts Left Behind

The smoke worktree and demo workspace are intentionally **left in place** until a separate cleanup
decision (do not clean here):

```text
/mnt/vast-data/git-worktrees/stack-code-a2-workspace-first-gui-smoke-20260608
/mnt/vast-data/tmp/a2-workspace-first-gui-smoke-ws-20260608
```

Prior, separate leftovers remain out of scope and untouched:

```text
/mnt/vast-data/git-worktrees/stack-code-a2-panel-field-controls-gui-smoke-20260608
/mnt/vast-data/tmp/a2-panel-field-controls-gui-smoke-ws-20260608
install-smoke scope + DRAFT prompt local commit 448d7ea
```

---

## 13. Recommended Next Lanes

```text
1. A2 Workspace-First UX Smoke Evidence Handoff Review / Push PR — docs-only.
2. Disposable smoke cleanup — a separate lane that removes the smoke worktree + demo workspace after this
   evidence is accepted (safe, non-force).
3. (Only if explicitly requested later) Disposable live-chain artifact-backed GUI smoke — separately
   token-gated; exercises real preview/approval/apply-bundle artifacts in a disposable workspace.
```

---

## 14. Final Classification

```text
CLASSIFICATION: PASS (read-only / print-validate workspace-first UX smoke)
The workspace-first UX is operator-usable for the open-workspace → inspect setup → discover →
next-safe-step → print/validate flow. It was NOT proved against a real artifact-backed live chain.
Approval remains real-terminal and human-typed; apply remains an explicit, operator-run terminal step.
Any live (even disposable) chain exercise is a separate, explicitly-approved lane.
```
