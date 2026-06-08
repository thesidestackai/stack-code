# A2 Local Coding Agent Foundation v0 Headless / GUI Smoke Scope

> Docs-only handoff. It records headless validation evidence for the merged A2 Local Coding Agent
> Foundation v0 (from clean `origin/main`) and scopes a safe operator GUI smoke. It runs no live A2
> workflow, makes no model/broker/runtime call, starts no Tier 3 work, and writes no `.claw` artifact.

---

## 1. Executive Summary

A2 Local Coding Agent Foundation v0 is merged on `main` at `9e87816` (PR #104). This lane validates,
from a clean worktree off `origin/main`, that the foundation package builds, passes static guards,
compiles, and passes its unit tests — and that it exposes the five new read-only control-plane
sections without introducing any executable agent action or mutation capability. It then prepares a
disposable demo workspace and a checklist for a future operator GUI smoke. No live A2 chain is run
here; the GUI smoke itself is a separate operator step.

Headless result: `npm install --ignore-scripts` OK, guards PASS (15 src files), `tsc` compile clean,
`166 passing` unit tests.

---

## 2. Scope

```text
In scope (this lane):
- Headless build/guards/compile/test of the merged foundation from origin/main.
- Read-only source/render/safety inspection.
- A disposable demo workspace for a future GUI smoke.
- An operator GUI smoke checklist.
- This docs-only handoff.

Out of scope (this lane):
- No live A2 workflow; no preview/approval/apply-bundle/apply.
- No Tier 3 work; no Tier 4 work.
- No model/broker/runtime call; no raw :11434 inference.
- No .claw artifact creation; no target mutation.
- No GUI launch (the command is printed for the operator, not executed here).
```

---

## 3. Source of Truth

```text
Foundation v0 merge (origin/main):
  PR #104  feat(a2): add local coding agent foundation   9e8781674ca38044210d5c615f4a6bce5ddd2a4b

Package:           ide/vscode/a2-harness-panel/
Helper (spawned):  scripts/a2-ide-harness.sh  (print/validate only; unchanged by v0)
Runbook:           docs/runbooks/a2-ide-extension-panel.md
Implementation report:
  handoffs/a2_local_coding_agent_foundation_v0_implementation_report_2026-06-08.md
New foundation modules:
  src/permissionTiers.ts, src/deniedCommands.ts, src/agentSession.ts,
  src/agentEvidence.ts, src/agentReadiness.ts
```

---

## 4. Headless Validation Evidence

Run from a fresh worktree at `origin/main` (`9e87816`), package dir `ide/vscode/a2-harness-panel`:

```text
npm install --ignore-scripts : OK
npm run lint (run-guards.js)  : PASS (15 src files audited; single spawn boundary intact)
npm run compile (tsc -p .)    : clean
npm test (mocha)              : 166 passing
```

`run-guards.js` is the authoritative structural guard: it strips comments and string literals before
checking, and confirms no network/telemetry/broker/`ollama`/`:11434` egress, no `fs`, no watcher/
polling/timer, no secret-storage API, no chain-write literal in live code, and that only
`helperRunner.ts` may spawn a process.

---

## 5. Foundation v0 Concept Evidence

The five read-only control-plane sections are present and exercised by unit tests:

```text
Agent Readiness            — honest tri-state (workspace/repo/git/dirty/staged/unstaged/untracked,
                             current tier, denied-registry loaded, safe-executor mode).
Permission Tier            — Tier 0–5 model; Tier 5 denied by default; Tiers 3–4 require explicit
                             approval; effective tier is read-only (Tier 0–2) and guarded.
Denied Command Registry    — global denied families; denials win over any allowlist.
Agent Evidence Ledger      — session-local, render-only; print-only steps marked printed-not-run.
Proposed Next Agent Lane   — shows what comes next and that no mutation lane is enabled in v0.
```

Repo/git readiness behavior: v0 wires no guard-safe git probe (panel forbids fs/spawn/watcher/timer),
so git/dirty dimensions render honestly as `not-checked` with a stated reason — never fabricated
green. Dirty-checkout warning fires only on a real dirty fact; `not-checked` raises no false warning
and implies no false all-clear.

---

## 6. Safety Review

```text
Tier 3 work started                : no
Tier 4 work started                : no
live A2 workflow run               : no
preview / approval / apply-bundle / apply : none run
model / broker call                : no
runtime touched                    : no
raw :11434 app inference           : no (every :11434 occurrence is prohibition/denial/tier/test text)
new process spawn boundary         : no (helperRunner.ts remains the only spawn boundary; guards PASS)
network egress                     : no
watcher / polling / timer          : no
helper script touched              : no
.claw artifacts created            : no
target modified                    : no
mutation capability                : none (no file-editing, no PR-packaging, no branch-deletion control)
executable agent control           : none (no agent-run/agent-execute/apply/approve button)
```

Scan-disposition note (transparency): broad literal scans match benign baseline content — e.g. the
pre-existing read-only `openRunbook` ui-action (the substring "Run" inside "Runbook"), denied-command
registry pattern string literals (the very tokens the registry DENIES), and doc comments mentioning
"spawn"/"child_process" in negation. These are not new risk. The authoritative structural guard
(`run-guards.js`) passes, and the intent property — no NEW executable control and no new execution/
runtime/network surface — holds. This lane changed no source.

---

## 7. Demo Workspace Prepared

A disposable demo workspace was staged for a future GUI smoke (no `.claw` created):

```text
Workspace : /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608
Plan      : /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608/handoff/plan.yaml
Target    : /mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
After SHA : da6de659cb36738fffff8859f061add946843d3fd78bb9bbf252d6d3fbc610b7
Helper    : /mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-smoke-scope-20260608/scripts/a2-ide-harness.sh
.claw     : not present (NO_CLAW_ARTIFACTS_CREATED)
```

---

## 8. Operator GUI Smoke Checklist

Operator GUI smoke command (printed only; NOT launched by this lane):

```text
code --extensionDevelopmentPath="/mnt/vast-data/git-worktrees/stack-code-a2-local-coding-agent-foundation-v0-smoke-scope-20260608/ide/vscode/a2-harness-panel" \
     --new-window "/mnt/vast-data/tmp/a2-local-coding-agent-foundation-v0-gui-smoke-ws-20260608"
```

Confirm in the rendered panel:

```text
Panel opens.
Safety / Stop Gates banner visible.
Workspace status renders.
Agent Readiness section renders.
Permission Tier section renders (current effective tier read-only; Tier 5 denied by default).
Denied Command Registry section renders (denials win over allowlists).
Agent Evidence Ledger section renders (printed-not-run markers).
Proposed Next Agent Lane section renders (states no mutation lane is enabled in v0).
Git readiness is honest not-checked when no guard-safe probe is wired.
Dirty checkout does not falsely claim clean when not checked.
No agent-run / agent-execute button exists.
No apply / approve / live A2 chain button exists.
Show/Copy Preview remains print-only.
Verify Final MATCH works after target + after SHA are set (read-only hash check).
Evidence timeline remains read-only / printed-not-run.
No .claw directory appears.
Target remains unchanged.
```

---

## 9. What Is Still Not Proven

```text
A rendered GUI click-through of the five new sections was NOT performed in this lane (headless only).
The operator GUI smoke (Section 8) is a separate operator step.
No artifact-backed live preview/approval/apply chain was run; Foundation v0 enables none.
The foundation is not validated against real targets — it is a read-only control plane only.
Tier 3 (disposable worktree mutation) remains designed-only in the scope doc; nothing is enabled.
```

---

## 10. STOP Gates

```text
STOP if a future lane attempts Tier 3 mutation before this GUI smoke passes or the operator skips it.
STOP if any change adds an executable agent/apply/approve control, a new spawn boundary, or a
  runtime/model/broker/:11434 call.
STOP if a .claw artifact or a target write appears during the read-only GUI smoke.
STOP if git/dirty readiness renders a fabricated value instead of not-checked when unprobed.
STOP if guards or unit tests fail from main.
```

---

## 11. Recommended Next Lane

```text
Name        : Foundation v0 Operator GUI Smoke
Objective   : the operator runs the Section 8 command and confirms the checklist in a rendered panel,
              then records read-only GUI evidence.
Tool        : operator (VS Code Extension Development Host) + Claude Code for the evidence handoff.
Why         : headless validation passes; a rendered confirmation of the five sections + honest
              not-checked readiness is the remaining read-only proof before any Tier 3 design.
Mutation    : none (read-only GUI smoke).
STOP gate   : do not begin Tier 3 (disposable worktree mutation) design until this GUI smoke passes
              or the operator explicitly skips it; never run live A2 / apply / approve in the smoke.
```
