# A2 Tier 3 Foundation v0 GUI Smoke Evidence

> Docs-only evidence handoff. It records that the merged Tier 3 Foundation v0 panel (the read-only
> readiness/state/render layer for the disposable worktree mutation path) passed a **read-only**
> operator GUI smoke. It runs no live A2 workflow, makes no model/broker/runtime call, enables no
> mutation, creates no worktree, writes no `.claw` artifact, and modifies no target.

---

## 1. Executive Summary

Tier 3 Foundation v0 GUI smoke **passed in read-only mode**. In the VS Code Extension Development
Host, the A2 Harness Panel opened, the existing read-only workspace flow worked (validate-input exit
0, print-preview printed-only exit 0), and the eight Tier 3 control-plane sections rendered. Tier 3
readiness rendered **not-ready** with control-checkout / origin-main / worktree-path / branch-name
showing **not-checked** honestly; plan valid: no; operator approved: no; declared scope present: no;
the mutation lane is not active. No mutation executor, worktree-creation, apply, approve, or live-A2
control appeared. Verify Final returned MATCH; the demo target was not modified and no `.claw`
directory appeared.

This does **not** prove an artifact-backed live chain or any actual mutation — Tier 3 Foundation v0
enables none. It is a read-only control-plane GUI smoke only.

---

## 2. Scope

```text
Read-only operator GUI smoke + terminal safety verification.
No mutation. No mutation executor. No worktree creation. No file write by the panel.
No live preview/approval/apply-bundle/apply. No target write. No .claw write.
No Tier 3 implementation beyond the merged read-only foundation. No model/broker/runtime call.
No raw :11434 inference.
```

---

## 3. Environment

```text
Tier 3 Foundation v0 merge (origin/main):
  PR #107  feat(a2): add tier 3 disposable mutation foundation   6efc29a33cf0593dd827260f556e696f7ec530a1
Tier 3 design scope (origin/main):
  PR #106  docs(a2): scope tier 3 disposable mutation            4bcd8a21b7f721382aa9c4549b9432cb2be18c3a

smoke-scope worktree (branch docs/a2-tier3-foundation-v0-smoke-scope-20260608 @ 943d638):
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-foundation-v0-smoke-scope-20260608

demo workspace:
  /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608
demo plan:
  /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608/handoff/plan.yaml
demo target:
  /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
helper (only binary the panel spawns; unchanged by Tier 3 v0):
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-foundation-v0-smoke-scope-20260608/scripts/a2-ide-harness.sh
```

Headless corroboration (this lane, before the GUI smoke): `npm install --ignore-scripts` OK, guards
PASS (20 src files audited), `tsc` compile clean, `218 passing` unit tests.

### Evidence provenance (read this first)

- **Operator GUI click-through (operator-reported)**: the rendered-panel states in §4–§6 were confirmed
  by the operator in the VS Code Extension Development Host. This docs lane did not itself launch a GUI
  window.
- **Headless + terminal-side, independently reproduced in this lane**: package build (guards + compile
  + 218 passing tests) and the post-smoke terminal safety checks (§7) were run directly against the
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

## 5. Tier 3 Section Evidence

All eight read-only Tier 3 sections rendered:

```text
Tier 3 Readiness            : rendered; overall not-ready
                              control-checkout / origin-main / worktree-path / branch-name = not-checked
                              (honest; no guard-safe Tier 3 probe wired in v0)
                              plan valid: no; operator approved: no; declared scope present: no
Disposable Worktree Plan    : rendered (plan only; creation not performed)
Declared Touched Files      : rendered (none declared)
Mutation Approval Gate      : rendered; operator approved: no
Diff Summary                : rendered (read-only placeholder)
Validation Results          : rendered (read-only placeholder)
Rollback / Abandon Worktree : rendered (prefers abandoning the disposable worktree; never force)
Mutation Evidence Ledger    : rendered (no Tier 3 mutation-lane gestures recorded)
```

Mutation lane active: no. The Foundation v0 sections also still render.

---

## 6. Verify Final Evidence

```text
Operator set target  : /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
Operator set after SHA: 884eb82629be8c80e651d102cdce890a8a351411e425d4d4c480b8e206dc5edd
Verify Final          : exit 0 — MATCH
```

---

## 7. Terminal Safety Evidence

Independently re-verified in this lane after the GUI smoke (terminal-side):

```text
target content:
original tier3 foundation v0 demo target content

target SHA (unchanged):
884eb82629be8c80e651d102cdce890a8a351411e425d4d4c480b8e206dc5edd

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
No mutation executor control.
No worktree-creation control.
No file write by the panel.
No Apply / Approve / live A2 control.
No agent-run / agent-execute control.
No hidden command execution observed.
Show/Copy Preview remained print-only.
Mutation evidence ledger remained read-only / printed-not-run.
No live preview / approval / apply-bundle / apply ran.
No model call. No broker call. No runtime touch. No raw :11434 inference.
No target write. No .claw artifact creation.
No Tier 3 implementation beyond the merged read-only foundation.
No permanent extension install (Extension Development Host session only).
```

---

## 9. What Was Not Proved

```text
No artifact-backed live chain was run.
No disposable worktree was created and no scoped write was applied.
No actual mutation executor / worktree-creation control was designed or implemented.
No real target write was attempted; Tier 3 v0 is a read-only control plane only.
A guard-safe Tier 3 probe is not yet wired; readiness remains not-checked by design in v0.
```

---

## 10. Remaining Artifacts

```text
The smoke-scope worktree and demo workspace remain in place until a separate cleanup lane:
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-foundation-v0-smoke-scope-20260608
  /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608

Older smoke/demo artifacts remain out of scope and untouched:
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608
  /mnt/vast-data/git-worktrees/stack-code-a2-workspace-first-gui-smoke-20260608
  /mnt/vast-data/git-worktrees/stack-code-a2-panel-field-controls-gui-smoke-20260608

install-smoke scope + DRAFT prompt local commit 448d7ea remains untouched.
```

---

## 11. Final Classification

```text
CLASSIFICATION: PASS (read-only Tier 3 Foundation v0 GUI smoke)
The Tier 3 control plane is operator-usable and legible: the eight sections render, readiness is
honest not-checked / not-ready, plan is not created, operator has not approved, and no mutation /
executor / worktree-creation / apply / approve control exists. Verify Final returned MATCH; the target
remained unchanged and no .claw appeared. No actual mutation, worktree creation, or live chain was
run; Tier 3 v0 enables none. Any actual mutation executor or worktree-creation control is a separate,
explicitly-approved lane.
```

---

## 12. Recommended Next Lane

```text
Name        : Tier 3 Foundation v0 Smoke Evidence Review / Push PR
Objective   : review this GUI smoke evidence handoff (with the smoke-scope handoff), then push the
              branch and open a docs-only PR for operator review.
Tool        : Claude Code (review/push lane) + operator review.
Why         : the GUI smoke evidence must be recorded and reviewed before any lane that designs an
              actual mutation executor or worktree-creation control.
Mutation    : none (docs-only).
STOP gate   : do not design or implement an actual mutation executor / worktree-creation control until
              this evidence is reviewed, unless the operator explicitly skips that docs gate.
```
