# A2 IDE Extension Panel v1 — Operator-Ready Handoff — 2026-06-08

> Docs-only readiness handoff. It implements nothing, runs no A2 command, and makes no
> model/broker/runtime call. It records that the A2 IDE Extension Panel v1 is operator-ready for
> **print/validate-only** use after the operator GUI click-through confirmed the artifact-field UX
> polish.

---

## 1. Executive Summary

The **A2 IDE Extension Panel v1** is **operator-ready for print/validate-only use**. It is a visual,
button-driven VS Code / Cursor panel that drives the merged print/validate-only A2 IDE harness v0
helper (`scripts/a2-ide-harness.sh`) in read-only / print subcommands, renders helper stdout, and
copies the helper-printed commands.

The panel is **not a live apply engine**. It does **not** execute the A2 chain: it never spawns `claw`,
never runs preview / approval / apply-bundle / apply, never composes or captures the approval line. It
helps a non-terminal-first operator visually select fields and print/copy the exact commands to run
themselves, with approval staying real-terminal and human-typed.

Merged on `main`:

```text
PR #96  feat(a2): add IDE extension panel v1                377e25eb82c14ba9cce84f092e8c35bd8b402277
PR #98  fix(a2): expose panel artifact field controls       6267f427c4957d8104dd6a6c5916ff2c3c6df463
```

PR #98 closed the discoverability gap by adding visible controls for the artifact/hash fields that the
panel already tracked but could not previously set from the GUI.

---

## 2. What Is Now Operator-Ready

```text
Field-setter controls (set session fields only; never run a chain command):
  Select Workspace
  Select Plan
  Select Target
  Set After SHA
  Select Preview Bundle
  Select Generator Result
  Select Approval Result
  Set Approval Output
  Select Apply Bundle

Read-only / print actions:
  Validate Input
  Audit Workspace
  Find Artifacts
  Show/Copy Preview Command
  Show/Copy Approval Command
  Show/Copy Apply-Bundle Command
  Show/Copy Apply Command
  Verify Final Target
  Open Runbook
  Export Evidence Summary
```

There is intentionally **no** Run Preview / Run Approval / Run Apply-Bundle / Run Apply button.

---

## 3. What Was Proved

```text
Panel opens in the VS Code Extension Development Host.
Safety / Stop Gates banner renders (always on).
All 7 artifact-field controls render and are usable.
Target and after-sha can be set from the GUI.
Approval-output can be set from the GUI.
Validate Input returns exit 0.
Audit Workspace returns exit 0.
Find Artifacts returns exit 0.
Show/Copy Preview Command returns exit 0 and prints the command only (no execution).
Verify Final returns exit 0 with MATCH (and exit 3 with MISMATCH on a wrong hash).
Export Evidence Summary opens as an unsaved markdown document (no file written).
No Run-* buttons exist.
```

Headless pre-validation (same build) additionally confirmed: guards PASS, compile clean, 49/49 unit
tests passing, and the rendered panel HTML contains all 7 new field controls with no Run-* button
markup.

---

## 4. GUI Smoke Evidence

```text
Disposable workspace:
  /mnt/vast-data/tmp/a2-panel-field-controls-gui-smoke-ws-20260608

Demo target:
  /mnt/vast-data/tmp/a2-panel-field-controls-gui-smoke-ws-20260608/sample/demo_target.txt

After SHA (Verify Final match):
  14ba4688ebf366d55d42f09d8e16a81779f91a8202fe865f2f903ed3f085afd2

Terminal confirmation:
  target content remained "original demo target content"
  file list contained only:
    - handoff/plan.yaml
    - materialized/demo_target.after.txt
    - sample/demo_target.txt
    - .vscode/settings.json
  No .claw directory was created.
```

The smoke ran read-only / print subcommands only. The target was never modified and no `.claw`
artifacts were produced.

---

## 5. Safety Boundaries Preserved

```text
No live preview run.
No live approval run.
No live apply-bundle run.
No live apply run.
No approval line typed into the panel.
No approval phrase composed or captured by the panel.
No auto-approval.
No hidden apply.
No model call.
No broker call.
No runtime touch.
No target mutation.
No .claw artifact creation.
No permanent extension install (dev-host session only; close = rollback).
No user IDE settings modified outside the disposable workspace.
```

apply-bundle is the generator (writes no target); `claw plan apply` is the only command that writes the
target — and the panel only prints it. The single spawn boundary (the helper, allowlisted read-only /
print subcommands) is unchanged.

---

## 6. What Was Not Proved

Be explicit — this readiness is for print/validate-only use:

```text
A full artifact-backed GUI flow with REAL preview-bundle / approval-result / apply-bundle was NOT run.
The panel was NOT tested against a live disposable A2 preview/apply chain after artifact creation,
because the read-only smoke intentionally did not run preview/approval/apply.
The Show/Copy Approval, Show/Copy Apply-Bundle, and Show/Copy Apply buttons were exercised as
command-printing only (against placeholder artifact paths), not against real chain artifacts.
A live (even disposable) chain exercise would require a SEPARATELY approved disposable live-chain lane.
```

This handoff does NOT claim a full live preview / approval / apply GUI chain was run.

---

## 7. How To Use The Panel Safely

```text
Use the panel to select fields and to print/copy commands.
Run approval ONLY in a real terminal, and type the approval line yourself there.
Never type or paste the approval line into the panel.
Treat Show/Copy Apply as a printed command only — running it is your explicit terminal action.
Do not use real targets until a scoped lane includes backups and rollback.
Prefer a disposable workspace for any exploration.
```

---

## 8. Known Remaining Limitations

```text
The Preview Bundle / Generator Result / Approval Result / Apply Bundle controls require those artifacts
to already exist; the read-only smoke deliberately did not run preview, so these artifact fields were
not exercised against real artifacts.
Find Artifacts auto-population (Option B) remains optional future work — fields are set manually today.
The panel is not packaged as a .vsix; it loads via the Extension Development Host (or a dev symlink).
Cursor compatibility uses the same VS Code extension format but was not verified on the build host.
```

---

## 9. Future Optional Lanes

```text
1. Disposable live-chain GUI artifact-backed smoke — optional, separately token-gated.
2. Find Artifacts read-only auto-population (Option B) — optional UX improvement.
3. VSIX packaging / install flow — optional (adds a vsce dependency; its own lane).
4. Cleanup of the disposable GUI smoke worktree/workspace — separate housekeeping lane (see §10).
5. Push/merge the install-smoke scope + DRAFT prompt — optional docs debt (see §10).
```

---

## 10. Disposable Artifacts Left Behind

The GUI smoke disposable worktree and workspace may still exist and should be cleaned **only** in a
separate cleanup lane (not here):

```text
/mnt/vast-data/git-worktrees/stack-code-a2-panel-field-controls-gui-smoke-20260608
/mnt/vast-data/tmp/a2-panel-field-controls-gui-smoke-ws-20260608
```

Earlier leftover, separate from this handoff — do not mix in:

```text
Install-smoke scope card + DRAFT prompt at local commit 448d7ea remain on their own branch and unpushed.
```

---

## 11. Final Classification

```text
CLASSIFICATION: OPERATOR-READY (print/validate-only)
The A2 IDE Extension Panel v1 is ready for a non-terminal-first operator to visually select fields and
print/copy the A2-L2b chain commands safely. It is NOT a live apply engine and was NOT proved against a
real artifact-backed live chain. Approval remains real-terminal and human-typed; apply remains an
explicit, operator-run terminal step. Any live (even disposable) chain exercise is a separate,
explicitly-approved lane.
```
