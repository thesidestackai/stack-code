# A2 Local Coding Agent Foundation v0 GUI Smoke Evidence

> Docs-only evidence handoff. It records that the merged A2 Local Coding Agent Foundation v0 panel
> passed a **read-only** operator GUI smoke. It runs no live A2 workflow, makes no model/broker/runtime
> call, starts no Tier 3 work, and writes no `.claw` artifact. The panel remains read-only /
> print-validate; the Foundation v0 sections are status-only.

---

## 1. Executive Summary

A2 Local Coding Agent Foundation v0 GUI smoke **passed in read-only mode**. In the VS Code Extension
Development Host, the A2 Harness Panel opened, the existing read-only workspace flow worked
(validate-input exit 0, print-preview printed-only exit 0), and the five new Foundation v0
control-plane sections rendered: **Permission Tier, Agent Readiness, Denied Command Registry, Agent
Evidence Ledger, Proposed Next Agent Lane**. Repo/git readiness rendered honestly as `not-checked`
(no guard-safe git probe is wired in v0). No executable agent / apply / approve control appeared.
Verify Final returned MATCH. The demo target was not modified and no `.claw` directory appeared.

This does **not** prove an artifact-backed live preview/approval/apply chain — Foundation v0 enables
none. It is a read-only control-plane GUI smoke only.

---

## 2. Scope

```text
Read-only operator GUI smoke + terminal safety verification.
No live preview. No live approval. No live apply-bundle. No live apply.
No target write. No .claw write.
No Tier 3 / Tier 4 work. No model/broker/runtime call. No raw :11434 inference.
```

---

## 3. Environment

```text
Foundation v0 merge (origin/main):
  PR #104  feat(a2): add local coding agent foundation   9e8781674ca38044210d5c615f4a6bce5ddd2a4b

smoke-scope worktree (branch docs/a2-local-coding-agent-foundation-v0-smoke-scope-20260608 @ 043a70b):
  /mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-smoke-scope-20260608

demo workspace:
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608
demo plan:
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608/handoff/plan.yaml
demo target:
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
helper (only binary the panel spawns; unchanged by v0):
  /mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-smoke-scope-20260608/scripts/a2-ide-harness.sh
```

Headless corroboration (this lane, before the GUI smoke): `npm install --ignore-scripts` OK, guards
PASS (15 src files audited), `tsc` compile clean, `166 passing` unit tests.

### Evidence provenance (read this first)

- **Operator GUI click-through (operator-reported, screenshots)**: the rendered-panel states in §4–§6
  were confirmed by the operator in the VS Code Extension Development Host. This docs lane did not
  itself launch a GUI window.
- **Headless + terminal-side, independently reproduced in this lane**: package build (guards + compile
  + 166 passing tests) and the post-smoke terminal safety checks (§7) were run directly against the
  disposable demo workspace, corroborating the operator report.

---

## 4. Operator GUI Evidence

Operator-reported from the Extension Development Host, corroborated by the headless build and the
terminal safety checks:

```text
A2 Harness Panel opened.
Safety / Stop Gates banner visible.
Workspace status rendered:
  helper path     : found
  claw binary     : configured
  workspace root  : detected
  plan.yaml       : found / auto-selected
Validate Input   : exit 0
Show/Copy Preview: print-preview exit 0; command printed only (not run)
Evidence timeline: updated with read-only events
```

---

## 5. Foundation v0 Section Evidence

All five new read-only control-plane sections rendered:

```text
Permission Tier            : rendered; current effective tier = Tier 2 (read-only)
Agent Readiness            : rendered; repo detected / git branch / dirty checkout / staged changes /
                             unstaged changes / untracked files all = not-checked
                             (stated as honest not-checked because no guard-safe git probe is wired in v0)
                             denied registry loaded = yes; safe executor mode = print-validate-only
Denied Command Registry    : rendered
Agent Evidence Ledger      : rendered (read-only; printed-not-run semantics)
Proposed Next Agent Lane   : rendered; mutation flag shows no (no mutation lane is enabled in v0)
```

Proposed Next Agent Lane "still blocked in v0" list confirmed: file editing, PR creation, branch
deletion, live A2 chain execution, runtime/model/broker/service actions, and hidden command execution.

Repo/git readiness behavior: rendered honestly as `not-checked` (no guard-safe git probe wired in v0);
the panel did not falsely claim a clean/dirty git state.

---

## 6. Verify Final Evidence

```text
Operator set target  : /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
Operator set after SHA: da6de659cb36738fffff8859f061add946843d3fd78bb9bbf252d6d3fbc610b7
Verify Final          : exit 0 — MATCH
```

---

## 7. Terminal Safety Evidence

Independently re-verified in this lane after the GUI smoke (terminal-side):

```text
target content:
original foundation v0 demo target content

target SHA (unchanged):
da6de659cb36738fffff8859f061add946843d3fd78bb9bbf252d6d3fbc610b7

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
No Run Agent control.
No Execute Agent control.
No Apply Now control.
No Approve Now control.
No live A2 execution control.
No hidden command execution observed.
Show/Copy Preview remained print-only.
Evidence ledger/timeline remained read-only / printed-not-run.
No live preview / approval / apply-bundle / apply ran.
No model call. No broker call. No runtime touch. No raw :11434 inference.
No target mutation. No .claw artifact creation.
No Tier 3 work started. No Tier 4 work started.
No permanent extension install (Extension Development Host session only).
```

---

## 9. What Was Not Proved

```text
No artifact-backed live chain was run.
No Tier 3 disposable worktree mutation was designed or implemented.
No PR packaging was implemented.
No real target write was attempted; this is a read-only control plane only.
A guard-safe git probe is not yet wired; git readiness remains not-checked by design in v0.
```

---

## 10. Remaining Artifacts

```text
The smoke-scope worktree and demo workspace remain in place until a separate cleanup lane:
  /mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-smoke-scope-20260608
  /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608

Older GUI-smoke worktrees/workspaces remain out of scope and untouched:
  /mnt/vast-data/git-worktrees/stack-code-a2-workspace-first-gui-smoke-20260608
  /mnt/vast-data/tmp/a2-workspace-first-gui-smoke-ws-20260608
  /mnt/vast-data/git-worktrees/stack-code-a2-panel-field-controls-gui-smoke-20260608
  /mnt/vast-data/tmp/a2-panel-field-controls-gui-smoke-ws-20260608

install-smoke scope + DRAFT prompt local commit 448d7ea remains untouched.
```

---

## 11. Final Classification

```text
CLASSIFICATION: PASS (read-only Foundation v0 GUI smoke)
The Foundation v0 control plane is operator-usable and legible: the five new sections render, the
current effective tier is read-only (Tier 2), git readiness is honest not-checked, the denied-command
registry is loaded, and no executable agent/apply/approve control exists. Verify Final returned MATCH;
the target remained unchanged and no .claw appeared. No artifact-backed live chain was run; Foundation
v0 enables none. Any mutation (even disposable) is a separate, explicitly-approved Tier 3 lane.
```

---

## 12. Recommended Next Lane

```text
Name        : A2 Foundation v0 Smoke Evidence Review / Push PR
Objective   : review this GUI smoke evidence handoff (with the smoke-scope handoff), then push the
              branch and open a docs-only PR for operator review.
Tool        : Claude Code (review/push lane) + operator review.
Why         : the GUI smoke evidence must be recorded and reviewed before any Tier 3 mutation design.
Mutation    : none (docs-only).
STOP gate   : do not begin Tier 3 (disposable worktree mutation) design until this GUI smoke evidence
              is reviewed, unless the operator explicitly skips that docs gate.
```
