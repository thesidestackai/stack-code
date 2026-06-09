# A2 Tier 3 Foundation v0 Headless / GUI Smoke Scope

> Docs-only handoff. It records headless validation evidence for the merged Tier 3 Foundation v0
> (the read-only readiness/state/render layer for the disposable worktree mutation path) from clean
> `origin/main`, and scopes a safe operator GUI smoke. It runs no live A2 workflow, makes no
> model/broker/runtime call, enables no mutation, creates no worktree, and writes no `.claw` artifact.

---

## 1. Executive Summary

Tier 3 Foundation v0 is merged on `main` at `6efc29a` (PR #107). This lane validates, from a clean
worktree off `origin/main`, that the panel package builds, passes static guards, compiles, and passes
its unit tests — and that it exposes the eight read-only Tier 3 control-plane sections without
introducing any mutation executor, worktree-creation control, or write capability. It then prepares a
disposable demo workspace and an operator GUI smoke checklist. No mutation is enabled; the GUI smoke
itself is a separate operator step.

Headless result: `npm install --ignore-scripts` OK, guards PASS (20 src files), `tsc` compile clean,
`218 passing` unit tests.

---

## 2. Scope

```text
In scope (this lane):
- Headless build/guards/compile/test of the merged Tier 3 Foundation v0 from origin/main.
- Read-only source/render/safety inspection.
- A disposable demo workspace for a future GUI smoke.
- An operator GUI smoke checklist.
- This docs-only handoff.

Out of scope (this lane):
- No mutation, no mutation executor, no worktree creation, no file write by the panel.
- No live A2 workflow; no preview/approval/apply-bundle/apply.
- No model/broker/runtime call; no raw :11434 inference.
- No .claw artifact creation; no real target writes.
- No GUI launch (the command is printed for the operator, not executed here).
```

---

## 3. Source of Truth

```text
Tier 3 design scope (origin/main):  docs/a2-tier3-disposable-worktree-mutation-scope.md  (PR #106, 4bcd8a2)
Tier 3 Foundation v0 (origin/main):
  PR #107  feat(a2): add tier 3 disposable mutation foundation   6efc29a33cf0593dd827260f556e696f7ec530a1

Package:           ide/vscode/a2-harness-panel/
Helper (spawned):  scripts/a2-ide-harness.sh  (print/validate only; unchanged by Tier 3 v0)
Runbook:           docs/runbooks/a2-ide-extension-panel.md  (Tier 3 Foundation v0 section)
Implementation report:
  handoffs/a2_tier3_disposable_worktree_mutation_foundation_implementation_report_2026-06-08.md
New Tier 3 modules:
  src/tier3Readiness.ts, src/disposableWorktreePlan.ts, src/mutationScope.ts,
  src/safeMutationPolicy.ts, src/mutationEvidence.ts
```

---

## 4. Headless Validation Evidence

Run from a fresh worktree at `origin/main` (`6efc29a`), package dir `ide/vscode/a2-harness-panel`:

```text
npm install --ignore-scripts : OK
npm run lint (run-guards.js)  : PASS (20 src files audited; single spawn boundary intact)
npm run compile (tsc -p .)    : clean
npm test (mocha)              : 218 passing
```

`run-guards.js` is the authoritative structural guard: it strips comments and string literals before
checking, and confirms no network/telemetry/broker/`ollama`/`:11434` egress, no `fs`, no
watcher/polling/timer, no secret-storage API, no chain-write literal in live code, and that only
`helperRunner.ts` may spawn a process.

---

## 5. Tier 3 Concept Evidence

The eight read-only Tier 3 sections are present and exercised by unit tests:

```text
Tier 3 Readiness            — honest tri-state (control-checkout-clean / origin-main / worktree-path-free
                              / branch-name-free / operator-approved / plan-valid / declared-scope /
                              denied-registry); overall ready/not-ready; not-ready by default.
Disposable Worktree Plan    — intended path + mutation branch + base; validated only, never created.
Declared Touched Files      — the exact declared path set (empty in v0); mutation limited to it.
Mutation Approval Gate      — operator-approved? (no in v0); read-only until explicit per-lane approval.
Diff Summary                — placeholder (a diff would be computed in the disposable worktree first).
Validation Results          — placeholder (only explicitly-approved validation would run in the worktree).
Rollback / Abandon Worktree — rollback prefers abandoning the disposable worktree; never force-remove/delete.
Mutation Evidence Ledger    — session-local, read-only; checkpoint/print steps marked printed-not-run.
```

Honesty: Tier 3 readiness renders `not-checked` and overall `not-ready` by default (no guard-safe
probe wired in v0); git/worktree state is never fabricated. A dirty control checkout is a hard block.
The safe-mutation policy is classification only — denials win over the Tier-3 allowlist, and writes
are limited to the declared exact-path set inside the disposable worktree.

---

## 6. Safety Review

```text
mutation by the panel                : none (no mutation lane is active)
mutation executor                    : none
worktree creation by the panel       : none
file write by the panel              : none
agent-run / agent-execute / apply / approve control : none
live A2 workflow run                 : no
preview / approval / apply-bundle / apply : none run
model / broker call                  : no
runtime touched                      : no
raw :11434 app inference             : no
new process spawn boundary           : no (helperRunner only; guards PASS)
network egress / watcher / polling / timer : no
helper script touched                : no
.claw artifacts created              : no
real target writes                   : no
```

Scan-disposition note (transparency): broad literal scans match benign baseline content (e.g. the
pre-existing read-only `openRunbook` ui-action; denied-registry pattern string literals; doc comments
mentioning `fs`/`spawn` in negation). These are not new risk. The authoritative structural guard
(`run-guards.js`) passes, and the only `data-ui-action` in src is the template
`data-ui-action="${escapeHtml(b.action)}"` — no mutation/create/executor control exists. This lane
changed no source.

---

## 7. Demo Workspace Prepared

A disposable demo workspace was staged for a future GUI smoke (no `.claw` created):

```text
Workspace : /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608
Plan      : /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608/handoff/plan.yaml
Target    : /mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608/sample/demo_target.txt
After SHA : 884eb82629be8c80e651d102cdce890a8a351411e425d4d4c480b8e206dc5edd
Helper    : /mnt/vast-data/git-worktrees/stack-code-a2-tier3-foundation-v0-smoke-scope-20260608/scripts/a2-ide-harness.sh
.claw     : not present (NO_CLAW_ARTIFACTS_CREATED)
```

---

## 8. Operator GUI Smoke Checklist

Operator GUI smoke command (printed only; NOT launched by this lane):

```text
code --extensionDevelopmentPath="/mnt/vast-data/git-worktrees/stack-code-a2-tier3-foundation-v0-smoke-scope-20260608/ide/vscode/a2-harness-panel" \
     --new-window "/mnt/vast-data/tmp/a2-tier3-foundation-v0-gui-smoke-ws-20260608"
```

Confirm in the rendered panel:

```text
Panel opens.
Safety / Stop Gates banner visible.
Workspace status renders (Foundation v0 sections still render).
Tier 3 — Disposable Worktree Mutation (Foundation v0, read-only) section renders, with:
  Tier 3 Readiness section renders; control-checkout/origin/worktree/branch/operator-approved show not-checked.
  Overall shows not-ready.
  Disposable Worktree Plan shows creation: not performed (plan only); plan valid: no.
  Declared Touched Files shows (none declared).
  Mutation Approval Gate shows operator approved: no.
  Diff Summary and Validation Results show their read-only placeholders.
  Rollback / Abandon Worktree guidance renders.
  Mutation Evidence Ledger shows (no Tier 3 mutation-lane gestures recorded yet).
Tier 3 readiness is honest not-checked / not-ready (never fabricated green).
A dirty control checkout would render a hard block (not exercised in v0; honest not-checked otherwise).
No mutation / executor / worktree-creation / apply / approve / agent-run control appears.
No .claw directory appears.
Target remains unchanged.
```

---

## 9. What Is Still Not Proven

```text
A rendered GUI click-through of the Tier 3 sections was NOT performed in this lane (headless only).
The operator GUI smoke (Section 8) is a separate operator step.
No artifact-backed live chain was run; Tier 3 Foundation v0 enables none.
No disposable worktree was created and no scoped write was applied — Tier 3 v0 is a read-only control
  plane only; an actual mutation executor / worktree-creation control is a separate, explicitly-approved
  lane that is not designed or implemented here.
No guard-safe Tier 3 probe is wired; readiness remains not-checked by design in v0.
```

---

## 10. STOP Gates

```text
STOP if a future lane attempts an actual mutation executor or worktree-creation control before this
  GUI smoke passes or the operator explicitly skips it, AND a separate explicitly-approved lane is opened.
STOP if any change adds a mutation/create/executable control, a new spawn boundary, or a
  runtime/model/broker/:11434 call.
STOP if a .claw artifact or a target write appears during the read-only GUI smoke.
STOP if Tier 3 readiness renders a fabricated value instead of not-checked when unprobed.
STOP if guards or unit tests fail from main.
```

---

## 11. Recommended Next Lane

```text
Name        : Tier 3 Foundation v0 Operator GUI Smoke
Objective   : the operator runs the Section 8 command and confirms the checklist in a rendered panel,
              then records read-only GUI evidence.
Tool        : operator (VS Code Extension Development Host) + Claude Code for the evidence handoff.
Why         : headless validation passes; a rendered confirmation of the eight Tier 3 sections + honest
              not-checked/not-ready readiness is the remaining read-only proof before any lane that
              designs an actual mutation executor or worktree-creation control.
Mutation    : none (read-only GUI smoke).
STOP gate   : do not design or implement an actual mutation executor / worktree-creation control until
              this GUI smoke passes or the operator explicitly skips it; never run live A2 / apply /
              approve / mutation in the smoke.
```
