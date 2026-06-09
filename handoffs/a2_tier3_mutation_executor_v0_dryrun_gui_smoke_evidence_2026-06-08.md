# A2 Tier 3 Mutation Executor v0 (dry-run) GUI Smoke Evidence

> Docs-only evidence handoff. It records that the merged Tier 3 Mutation Executor v0 (the plan/dry-run
> -only layer + read-only "Proposed Executor Plan" panel section) passed a **read-only** operator GUI
> smoke. It runs no live A2 workflow, makes no model/broker/runtime call, creates no disposable
> worktree, writes no file, and writes no `.claw` artifact. The panel stays read-only; the executor is
> external and is never spawned by the panel.

---

## 1. Executive Summary

Tier 3 Mutation Executor v0 (dry-run) GUI smoke **passed in read-only mode**. In the VS Code Extension
Development Host, the A2 Harness Panel opened, the existing read-only workspace flow worked
(validate-input exit 0, print-preview printed-only exit 0), and the read-only "Proposed Executor Plan"
(dry-run) section rendered. It showed `wouldCreateWorktree: no` and `wouldWriteFiles: no`, rendered the
dry-run result/evidence, and exposed no executor / create / write / apply / approve / live-A2 control.
Verify Final returned MATCH; the demo target was not modified and no `.claw` directory appeared.

This does **not** prove an artifact-backed live chain or any actual mutation — v0 is dry-run /
classification only. It is a read-only control-plane GUI smoke.

---

## 2. Scope

```text
Read-only operator GUI smoke + terminal safety verification.
No disposable worktree creation. No file write by the executor or the panel.
No executor inside the panel; the panel never spawns the executor.
No live preview/approval/apply-bundle/apply. No target write. No .claw write.
No write-capable executor step. No model/broker/runtime call. No raw :11434 inference.
```

---

## 3. Environment

```text
Tier 3 Mutation Executor v0 (dry-run) merge (origin/main):
  PR #110  feat(a2): add tier 3 mutation executor dry-run   8795d0b1239ec460d698152868061588dc751f7c
Tier 3 mutation executor design scope (origin/main):
  PR #109  docs(a2): scope tier 3 mutation executor          90c83a4f003b37dd9a902c64383a7e1712b7e22d

smoke-scope worktree (branch docs/a2-tier3-executor-dryrun-smoke-scope-20260608 @ 26ebf96):
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-executor-dryrun-smoke-scope-20260608

demo workspace:
  /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608
demo plan:
  /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608/handoff/plan.yaml
demo target:
  /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608/sample/demo_target.txt
helper (only binary the panel spawns; unchanged):
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-executor-dryrun-smoke-scope-20260608/scripts/a2-ide-harness.sh
```

Headless corroboration (this lane, before the GUI smoke): `npm install --ignore-scripts` OK, guards
PASS (21 src files audited), `tsc` compile clean, `235 passing` unit tests.

### Evidence provenance (read this first)

- **Operator GUI click-through (operator-reported)**: the rendered-panel states in §4–§6 were confirmed
  by the operator in the VS Code Extension Development Host. This docs lane did not itself launch a GUI
  window.
- **Headless + terminal-side, independently reproduced in this lane**: package build (guards + compile
  + 235 passing tests) and the post-smoke terminal safety checks (§7) were run directly against the
  disposable demo workspace, corroborating the operator report.

---

## 4. Operator GUI Evidence

Operator-reported from the Extension Development Host, corroborated by the headless build and the
terminal safety checks:

```text
A2 Harness Panel opened.
Safety / Stop Gates banner visible.
Workspace status rendered:
  helper found; claw configured; workspace root detected; plan.yaml found.
Validate Input   : exit 0
Show/Copy Preview: print-preview exit 0; command printed only (not run)
```

---

## 5. Proposed Executor Plan (dry-run) Section Evidence

The read-only dry-run section rendered:

```text
Proposed Executor Plan (Tier 3 Mutation Executor v0 — dry-run, read-only) : rendered
  wouldCreateWorktree : no
  wouldWriteFiles     : no
  dry-run result      : rendered (readiness / plan / scope / per-step)
  dry-run evidence    : rendered (printed-not-run)
  external dry-run command : printed only (operator-run; describe-only)
No executor / create-worktree / write / apply / approve / agent-run / live-A2 control appeared.
```

The Tier 3 Foundation v0 sections and Foundation v0 sections also still render.

---

## 6. Verify Final Evidence

```text
Operator set target  : /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608/sample/demo_target.txt
Operator set after SHA: 2a69e352d2f2524c9441a87589af3a77e4466547554f3f39347bf59762e63929
Verify Final          : exit 0 — MATCH
```

---

## 7. Terminal Safety Evidence

Independently re-verified in this lane after the GUI smoke (terminal-side):

```text
target content:
original tier3 executor dry-run demo target content

target SHA (unchanged):
2a69e352d2f2524c9441a87589af3a77e4466547554f3f39347bf59762e63929

workspace files (unchanged):
handoff/plan.yaml
materialized/demo_target.after.txt
sample/demo_target.txt
.vscode/settings.json

.claw directory: NO_CLAW_AFTER_SMOKE (none appeared)
source worktree: clean (no tracked changes)
```

---

## 8. Safety Boundaries Preserved

```text
No executor / create-worktree / write control.
No Apply / Approve / live A2 control.
No agent-run / agent-execute control.
No hidden command execution observed.
Show/Copy Preview remained print-only.
The dry-run section showed wouldCreateWorktree: no and wouldWriteFiles: no.
No disposable worktree was created. No file was written by the executor or the panel.
The executor stayed external; the panel never spawned it.
No live preview / approval / apply-bundle / apply ran.
No model call. No broker call. No runtime touch. No raw :11434 inference.
No target write. No .claw artifact creation.
No write-capable executor step beyond the merged dry-run.
No permanent extension install (Extension Development Host session only).
```

---

## 9. What Was Not Proved

```text
No artifact-backed live chain was run.
No disposable worktree was created and no scoped write was applied.
No write-capable executor step was designed or implemented.
No real target write was attempted; v0 is dry-run / classification only.
A guard-safe Tier 3 probe is not yet wired; readiness remains not-checked / not-ready by design.
```

---

## 10. Remaining Artifacts

```text
The smoke-scope worktree and demo workspace remain in place until a separate cleanup lane:
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-executor-dryrun-smoke-scope-20260608
  /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608

Older smoke/demo artifacts remain out of scope and untouched:
  /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608
  /mnt/vast-data/git-worktrees/stack-code-a2-workspace-first-gui-smoke-20260608
  /mnt/vast-data/git-worktrees/stack-code-a2-panel-field-controls-gui-smoke-20260608

install-smoke scope + DRAFT prompt local commit 448d7ea remains untouched.
```

---

## 11. Final Classification

```text
CLASSIFICATION: PASS (read-only Tier 3 Mutation Executor v0 dry-run GUI smoke)
The dry-run control plane is operator-usable and legible: the Proposed Executor Plan section renders,
wouldCreateWorktree and wouldWriteFiles are no, and no executor / create / write / apply / approve /
live-A2 control exists. Verify Final returned MATCH; the target remained unchanged and no .claw
appeared. No disposable worktree was created and no scoped write was applied; v0 is dry-run /
classification only. Any write-capable executor step is a separate, explicitly-approved lane.
```

---

## 12. Recommended Next Lane

```text
Name        : Tier 3 Mutation Executor v0 (dry-run) Smoke Evidence Review / Push PR
Objective   : review this GUI smoke evidence handoff (with the smoke-scope handoff), then push the
              branch and open a docs-only PR for operator review.
Tool        : Claude Code (review/push lane) + operator review.
Why         : the GUI smoke evidence must be recorded and reviewed before any separately-approved
              write-capable executor step.
Mutation    : none (docs-only).
STOP gate   : do not design or implement a write-capable executor step until this evidence is reviewed,
              unless the operator explicitly skips that docs gate.
```
