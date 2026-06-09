# A2 Tier 3 Mutation Executor v0 (dry-run) — Implementation Report — 2026-06-08

> First executor lane. It adds the **plan / dry-run only** layer: a pure dry-run model and a read-only
> "Proposed Executor Plan" panel section. It creates NO disposable worktree, writes NO file, places NO
> executor inside the panel, and adds NO write/create/agent-run/apply/approve control. The panel
> remains read-only; the single spawn boundary is unchanged.

---

## 1. Executive Summary

Tier 3 Mutation Executor v0 implements the executor's first lane as a non-mutating dry-run, built on
the merged Tier 3 Foundation v0 + executor design scope (`docs/a2-tier3-mutation-executor-design-scope.md`,
PR #109). It adds one pure model module (`executorDryRun.ts`) and one read-only panel section
("Proposed Executor Plan"), plus tests. Given an operator-approved lane, the dry-run validates it
against the Foundation v0 models and computes exactly what an external executor WOULD do — while
creating nothing and writing nothing. The panel PRINTS the external dry-run command (operator-run) and
renders the result/evidence; it never spawns the executor.

Build is green: guards PASS (21 src files audited), `tsc` compile clean, `235 passing` unit tests
(up from 218; 17 new), all headless — no worktree creation, no writes, no chain, no model/broker/
runtime call.

---

## 2. Files Changed

New source module (pure, no IO):

```text
src/executorDryRun.ts          dry-run plan model: computeDryRun classifies what the executor WOULD do
                               (readiness + plan + scope + per-step would-accept/would-reject);
                               wouldCreateWorktree/wouldWriteFiles always false; reuses the Foundation
                               v0 models; performs no IO.
```

New tests:

```text
test/executorDryRun.test.ts        dry-run never creates/writes; readiness gate; denials win; exact-path
test/executorDryRunRender.test.ts  read-only section present; would-create/write no; no action control
```

Edited (additive only):

```text
src/render.ts                            + ExecutorDryRunView type + read-only "Proposed Executor Plan" block
src/extension.ts                         + buildExecutorDryRunView() (default unapproved/not-ready)
docs/runbooks/a2-ide-extension-panel.md  + Tier 3 Mutation Executor v0 (dry-run) section
handoffs/a2_tier3_mutation_executor_v0_dryrun_implementation_report_2026-06-08.md (this report)
```

Not touched: `scripts/a2-ide-harness.sh` (helper), Foundation v0 module behavior, Rust, schemas, CI.
No external executor script was added (the design requires separate approval for any such file; v0
only PRINTS a describe-only command string). The single spawn boundary (`helperRunner.ts`) is unchanged.

---

## 3. What Was Added

```text
- A pure dry-run plan model (executorDryRun.ts):
    * computeDryRun(approvedLane, facts?) -> DryRunResult.
    * Reuses tier3Readiness (overall ready/not-ready), disposableWorktreePlan (plan validation),
      mutationScope (validateDeclaredSet), safeMutationPolicy (evaluateTier3Command / evaluateTier3Write).
    * Per proposed write: would-accept only if in the declared set, inside the worktree, not under the
      control checkout; else would-reject.
    * Per proposed command: would-accept only if non-denied AND on the Tier-3 allowlist; denials win.
    * wouldCreateWorktree and wouldWriteFiles are ALWAYS false (dry-run).
    * printedCommand is a describe-only external command string (operator-run; no creation/writes).
- A read-only "Proposed Executor Plan" panel section that PRINTS the dry-run command and renders the
  result + evidence; it adds no executor spawn, no worktree-creation control, and no write button.
- Default wiring: v0 loads no approved lane, so the section renders not-ready and prints the command.
```

---

## 4. What Remains Blocked (no mutation in v0)

```text
- No worktree creation. No file write by the executor or the panel.
- No executor inside the panel; the panel never spawns the executor.
- No create / write / agent-run / agent-execute / apply / approve control.
- No live A2 chain. No model / broker / runtime / service call. No raw :11434 inference.
- No new process spawn boundary, no fs use, no network egress, no watcher / polling / timer.
- An actual write-capable executor step is a separate, explicitly-approved later lane.
```

---

## 5. Safety Confirmation

```text
disposable worktree created              : NO
file written by executor/panel           : NO
executor placed in panel                 : NO (external; describe-only command printed; never spawned)
create/write/agent-run/apply/approve control added : NO
network/broker/model/runtime/secret/:11434 added   : NO
fs/spawn added to panel outside helperRunner       : NO (guards confirm; single spawn boundary intact)
helper script touched                    : NO
external executor script added           : NO (would require separate approval)
.claw artifacts modified                 : NO
real target writes                       : NO
install-smoke 448d7ea touched            : NO
destructive commands used                : NONE
```

---

## 6. Tests / Guards / Build Results

```text
npm install --ignore-scripts : OK
guards (run-guards.js)        : PASS (21 src files audited; single spawn boundary intact)
compile (tsc -p .)            : clean
unit tests (mocha)            : 235 passing (17 new; previously 218)
```

New coverage:

```text
- executorDryRun: wouldCreateWorktree/wouldWriteFiles always false; printed command is describe-only;
  not-ready by default; ready only when readiness facts + plan + scope + approval all hold;
  declared-in-worktree write would-accept; outside-set / control-checkout write would-reject;
  denied-registry command would-reject (denials win); approved validation would-accept;
  non-allowlisted command would-reject; summary never claims a creation/write happened.
- executorDryRun render: section present (command + result + evidence); would-create/write render no;
  muted placeholder when absent; NO executor/create/write/apply/approve action control; field-setter
  ordering invariant preserved.
```

---

## 7. Next Recommended Lane

```text
Tier 3 Mutation Executor v0 (dry-run) Review / Push PR
```

Review/push lane: review the dry-run executor + the read-only "Proposed Executor Plan" view, push the
branch, open a PR for operator review. Do not design or implement an actual write-capable executor
step until this dry-run lane is merged AND a separate, explicitly-approved write-capable lane is
opened; the panel stays read-only.
