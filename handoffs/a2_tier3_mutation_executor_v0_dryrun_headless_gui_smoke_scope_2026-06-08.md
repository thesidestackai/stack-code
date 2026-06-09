# A2 Tier 3 Mutation Executor v0 (dry-run) Headless / GUI Smoke Scope

> Docs-only handoff. It records headless validation evidence for the merged Tier 3 Mutation Executor
> v0 (the plan/dry-run-only layer + read-only "Proposed Executor Plan" panel section) from clean
> `origin/main`, and scopes a safe operator GUI smoke. It runs no live A2 workflow, makes no
> model/broker/runtime call, creates no disposable worktree, writes no file, and writes no `.claw`
> artifact. The panel stays read-only; the executor is external and is never spawned by the panel.

---

## 1. Executive Summary

Tier 3 Mutation Executor v0 (dry-run) is merged on `main` at `8795d0b` (PR #110). This lane validates,
from a clean worktree off `origin/main`, that the panel package builds, passes static guards,
compiles, and passes its unit tests — and that it exposes the read-only "Proposed Executor Plan"
section without introducing any executor spawn, worktree-creation control, or write capability. It
then prepares a disposable demo workspace and an operator GUI smoke checklist. No mutation is enabled;
the GUI smoke itself is a separate operator step.

Headless result: `npm install --ignore-scripts` OK, guards PASS (21 src files), `tsc` compile clean,
`235 passing` unit tests.

---

## 2. Scope

```text
In scope (this lane):
- Headless build/guards/compile/test of the merged dry-run executor from origin/main.
- Read-only source/render/safety inspection.
- A disposable demo workspace for a future GUI smoke.
- An operator GUI smoke checklist.
- This docs-only handoff.

Out of scope (this lane):
- No disposable worktree creation; no file write by the executor or the panel.
- No executor inside the panel; the panel never spawns the executor.
- No live A2 workflow; no preview/approval/apply-bundle/apply.
- No model/broker/runtime call; no raw :11434 inference.
- No .claw artifact creation; no real target writes.
- No GUI launch (the command is printed for the operator, not executed here).
```

---

## 3. Source of Truth

```text
Tier 3 mutation executor design scope (origin/main): docs/a2-tier3-mutation-executor-design-scope.md (PR #109, 90c83a4)
Tier 3 Mutation Executor v0 (dry-run) (origin/main):
  PR #110  feat(a2): add tier 3 mutation executor dry-run   8795d0b1239ec460d698152868061588dc751f7c

Package:           ide/vscode/a2-harness-panel/
Helper (spawned):  scripts/a2-ide-harness.sh  (print/validate only; unchanged)
Runbook:           docs/runbooks/a2-ide-extension-panel.md  (Tier 3 Mutation Executor v0 section)
Implementation report:
  handoffs/a2_tier3_mutation_executor_v0_dryrun_implementation_report_2026-06-08.md
New module:
  src/executorDryRun.ts  (pure dry-run plan model; computeDryRun classifies; creates/writes nothing)
```

---

## 4. Headless Validation Evidence

Run from a fresh worktree at `origin/main` (`8795d0b`), package dir `ide/vscode/a2-harness-panel`:

```text
npm install --ignore-scripts : OK
npm run lint (run-guards.js)  : PASS (21 src files audited; single spawn boundary intact)
npm run compile (tsc -p .)    : clean
npm test (mocha)              : 235 passing
```

`run-guards.js` is the authoritative structural guard: it strips comments and string literals before
checking, and confirms no network/telemetry/broker/`ollama`/`:11434` egress, no `fs`, no
watcher/polling/timer, no secret-storage API, no chain-write literal in live code, and that only
`helperRunner.ts` may spawn a process. The new `executorDryRun.ts` is pure: no genuine fs/git write
call (the only "write" tokens are the boolean field `wouldWriteFiles`, always false).

---

## 5. Dry-Run Concept Evidence

The read-only "Proposed Executor Plan" section + the pure dry-run model are present and tested:

```text
computeDryRun(approvedLane, facts?) -> DryRunResult:
  - reuses tier3Readiness (not-ready by default), disposableWorktreePlan (plan validation),
    mutationScope (exact-path / control-checkout reject), safeMutationPolicy (denials win).
  - per proposed write: would-accept only if declared + in-worktree + not under the control checkout.
  - per proposed command: would-accept only if non-denied AND on the Tier-3 allowlist (denials win).
  - wouldCreateWorktree and wouldWriteFiles are ALWAYS false.
  - printedCommand is a describe-only external command (operator-run; NO creation, NO writes).
Proposed Executor Plan section: PRINTS the external dry-run command + renders the result + evidence;
  no executor spawn, no worktree-creation control, no write button.
```

---

## 6. Safety Review

```text
disposable worktree creation         : none
file written by executor/panel       : no
executor inside the panel            : no (external; describe-only command printed; never spawned)
create / write / agent-run / apply / approve control : none
live A2 workflow run                 : no
model / broker call                  : no
runtime touched                      : no
raw :11434 app inference             : no
new process spawn boundary           : no (helperRunner only; guards PASS)
network egress / watcher / polling / timer : no
helper script touched                : no
external executor script added       : no
.claw artifacts created              : no
real target writes                   : no
```

Scan-disposition note (transparency): broad literal scans match benign content (e.g. the pre-existing
read-only `openRunbook` ui-action; the boolean field `wouldWriteFiles`; denied-registry pattern string
literals; doc comments in negation). These are not new risk. `run-guards.js` (authoritative) passes,
and the only `data-ui-action` in src is the template `data-ui-action="${escapeHtml(b.action)}"`. This
lane changed no source.

---

## 7. Demo Workspace Prepared

A disposable demo workspace was staged for a future GUI smoke (no `.claw` created):

```text
Workspace : /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608
Plan      : /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608/handoff/plan.yaml
Target    : /mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608/sample/demo_target.txt
After SHA : 2a69e352d2f2524c9441a87589af3a77e4466547554f3f39347bf59762e63929
Helper    : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-executor-dryrun-smoke-scope-20260608/scripts/a2-ide-harness.sh
.claw     : not present (NO_CLAW_ARTIFACTS_CREATED)
```

---

## 8. Operator GUI Smoke Checklist

Operator GUI smoke command (printed only; NOT launched by this lane):

```text
code --extensionDevelopmentPath="/mnt/vast-data/git-worktrees/stack-code-a2-tier3-executor-dryrun-smoke-scope-20260608/ide/vscode/a2-harness-panel" \
     --new-window "/mnt/vast-data/tmp/a2-tier3-executor-dryrun-gui-smoke-ws-20260608"
```

Confirm in the rendered panel:

```text
Panel opens.
Safety / Stop Gates banner visible.
Workspace status renders; Tier 3 Foundation v0 sections still render.
Proposed Executor Plan (Tier 3 Mutation Executor v0 — dry-run, read-only) section renders, with:
  the external dry-run command printed (operator-run; describe-only).
  would create worktree: no  /  would write files: no.
  dry-run result lines (ready / readiness / plan valid / scope problems / steps).
  dry-run evidence (printed-not-run).
No executor / create-worktree / apply / approve / agent-run control appears.
Show/Copy Preview remains print-only.
No .claw directory appears.
Target remains unchanged.
```

---

## 9. What Is Still Not Proven

```text
A rendered GUI click-through of the Proposed Executor Plan section was NOT performed in this lane
  (headless only). The operator GUI smoke (Section 8) is a separate operator step.
No disposable worktree was created and no scoped write was applied — v0 is dry-run/classification only.
No actual write-capable executor step was designed or implemented.
No guard-safe Tier 3 probe is wired; readiness remains not-checked / not-ready by design in v0.
```

---

## 10. STOP Gates

```text
STOP if a future lane attempts an actual write-capable executor step before this GUI smoke passes or
  the operator explicitly skips it, AND a separate explicitly-approved lane is opened.
STOP if any change adds an executor-spawn / worktree-creation / write control, a new spawn boundary,
  or a runtime/model/broker/:11434 call.
STOP if a .claw artifact or a target write appears during the read-only GUI smoke.
STOP if the dry-run renders would-create-worktree or would-write-files as anything other than no.
STOP if guards or unit tests fail from main.
```

---

## 11. Recommended Next Lane

```text
Name        : Tier 3 Mutation Executor v0 (dry-run) Operator GUI Smoke
Objective   : the operator runs the Section 8 command and confirms the checklist in a rendered panel,
              then records read-only GUI evidence.
Tool        : operator (VS Code Extension Development Host) + Claude Code for the evidence handoff.
Why         : headless validation passes; a rendered confirmation of the Proposed Executor Plan section
              is the remaining read-only proof before any separately-approved write-capable executor step.
Mutation    : none (read-only GUI smoke).
STOP gate   : do not design or implement a write-capable executor step until this GUI smoke passes or
              the operator explicitly skips it; never create a worktree, write a file, or run live A2
              in the smoke; the panel stays read-only.
```
