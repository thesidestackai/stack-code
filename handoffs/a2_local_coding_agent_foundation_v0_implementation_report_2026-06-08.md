# A2 Local Coding Agent Foundation v0 — Implementation Report — 2026-06-08

> First source-touching lane toward the local coding-agent cockpit. It adds the **foundation
> UI/state model only**. It adds no mutation capability, no autonomous editing, no live A2 chain
> execution, no PR packaging, and no runtime/model/service control. The panel remains read-only /
> print-validate; the single spawn boundary is unchanged.

---

## 1. Executive Summary

A2 Local Coding Agent Foundation v0 adds a read-only **permissioned control plane** to the A2 IDE
Extension Panel: a permission tier model (Tier 0–5), an agent session model, a global denied-command
registry, an agent evidence ledger, and a guard-safe agent-readiness view — surfaced as five new
status-only panel sections. It enables **no** new capability: there is no agent-run/agent-execute
action control, no file editing by the panel, no PR packaging, and no chain execution. It is the
legibility/permission foundation the merged scope package
(`docs/a2-local-coding-agent-foundation-scope.md`, PR #103) called for.

Build is green: guards PASS (15 src files audited), `tsc` compile clean, `166 passing` unit tests
(up from 113; 53 new), all headless — no GUI, no chain, no model/broker/runtime call.

---

## 2. Files Changed

New source modules (pure, no IO):

```text
ide/vscode/a2-harness-panel/src/permissionTiers.ts   Tier 0–5 model + safe-effective-tier guard
ide/vscode/a2-harness-panel/src/deniedCommands.ts    global denied-command registry; denials win
ide/vscode/a2-harness-panel/src/agentSession.ts      non-persistent session model; no-secret guarantee
ide/vscode/a2-harness-panel/src/agentEvidence.ts     agent evidence ledger shape/format; printed-not-run
ide/vscode/a2-harness-panel/src/agentReadiness.ts    guard-safe readiness; not-checked when no probe
```

New tests:

```text
ide/vscode/a2-harness-panel/test/permissionTiers.test.ts
ide/vscode/a2-harness-panel/test/deniedCommands.test.ts
ide/vscode/a2-harness-panel/test/agentSession.test.ts
ide/vscode/a2-harness-panel/test/agentEvidence.test.ts
ide/vscode/a2-harness-panel/test/agentReadiness.test.ts
ide/vscode/a2-harness-panel/test/foundationRender.test.ts
```

Edited (additive only):

```text
ide/vscode/a2-harness-panel/src/render.ts            + FoundationView type + 5 read-only sections
ide/vscode/a2-harness-panel/src/extension.ts         + assemble the read-only foundation view + ledger
docs/runbooks/a2-ide-extension-panel.md              + Foundation v0 section
handoffs/a2_local_coding_agent_foundation_v0_implementation_report_2026-06-08.md  (this report)
```

Not touched: `scripts/a2-ide-harness.sh` (helper), `buttons.ts`, the existing tests, Rust, schemas,
CI config. The single spawn boundary (`helperRunner.ts`) is unchanged.

---

## 3. What Foundation v0 Adds

```text
- Permission tier model (Tier 0–5) with id/name/summary/allowedActions/deniedActions/
  requiredGates/evidenceRequired; Tier 5 denied by default; Tiers 3–4 require explicit approval.
- A current-effective-tier concept that defaults to read-only (Tier 1), rising to Tier 2 only when
  an already-allowlisted read-only helper call has been exercised; an assertEffectiveTierSafe guard
  rejects any mutation/runtime tier (3–5) as the live effective tier.
- A global denied-command registry covering destructive cleanup, force branch/worktree deletion,
  history rewrite/force-push, service control, model/broker calls, raw :11434 inference, secret
  reads, live A2 chain execution, approval-line composition, network egress, watcher/polling/timer
  automation, and hidden command execution — with denials winning over any allowlist.
- A non-persistent agent session model (sessionId, objective, workspace/repo/branch/worktree,
  touchedSurfaces, allowedTier, status, evidenceLedger) that holds no secrets and writes nothing.
- An agent evidence ledger (kind/tier/action/status/summary/details/printedNotRun) rendered as
  session-local lines; print-only steps marked printed-not-run.
- Guard-safe agent-readiness view (workspace/repo/git/dirty/staged/unstaged/untracked, current tier,
  denied-registry loaded, safe-executor mode).
- Five read-only panel sections: Agent Readiness, Permission Tier, Denied Command Registry, Agent
  Evidence Ledger, Proposed Next Agent Lane.
```

---

## 4. What Remains Blocked

```text
- No mutation lane is enabled. No file editing by the panel.
- No autonomous source edits.
- No PR creation / branch deletion by the panel.
- No live A2 chain execution (preview / approval / apply-bundle / apply).
- No agent-run / agent-execute action control. No chain-execution control.
- No model / broker / runtime / service call. No raw :11434 inference. No secret reads.
- No new process spawn boundary, no fs use, no network egress, no watcher / polling / timer.
```

---

## 5. Permission Tiers Implemented

```text
Tier 0 — Observe Only                  (reachable; read-only)
Tier 1 — Print Commands Only           (reachable; default effective tier)
Tier 2 — Safe Read-Only Execution      (reachable after a read-only helper call)
Tier 3 — Disposable Worktree Mutation  (described; requires explicit approval; NOT enabled)
Tier 4 — PR Packaging                  (described; requires explicit approval; NOT enabled)
Tier 5 — Runtime / Model / Service     (described; denied by default; external to the cockpit)
```

---

## 6. Denied Command Registry Implemented

The registry classifies a command against denied families and returns the matched families plus a
reason. `evaluate(command, allowlist?)` checks the denied registry FIRST: a denied command is denied
even when the allowlist would permit it (denials win). v0 enforces nothing at runtime — there is no
executor — it is classification + display only, ready for a future executor to enforce.

---

## 7. Agent Readiness Behavior

Readiness is computed by a pure model from optional, already-gathered signals. v0 wires **no**
git probe (the panel guards forbid `fs`, process spawn, watchers, timers). With no git facts
supplied, repo/branch/dirty/staged/unstaged/untracked all render `not-checked`, accompanied by a
stated reason. Readiness is never green-by-default and git state is never fabricated. The
dirty-checkout warning fires only on a real `dirty: true` fact — `not-checked` never raises a false
warning and never implies a false all-clear. A future, separately approved lane may supply
guard-safe git facts (e.g. read-only VS Code Git API) to the same pure model.

---

## 8. Evidence Ledger Behavior

The agent evidence ledger is session-local and render-only; it writes no file. Each event carries
the permission tier and an allowed/denied/ok/blocked/info status; print-only steps are marked
`printed-not-run`. It is bounded (cap 200, most-recent retained) and non-mutating on append. It is
distinct from the existing read-only helper-action timeline (`evidence.ts`).

---

## 9. Tests / Guards / Build Results

```text
npm install --ignore-scripts : OK
guards (run-guards.js)        : PASS (15 src files audited)
compile (tsc -p .)            : clean
unit tests (mocha)            : 166 passing (53 new; previously 113)
```

New test coverage:

```text
- Tier 0–5 shape + classification; Tier 5 denied-by-default; Tiers 3–4 require approval;
  defaultEffectiveTier (Tier 1 / Tier 2); assertEffectiveTierSafe rejects tiers 3–5.
- Denied registry denies each unsafe family; denials win over allowlist; benign command allowed.
- Agent session requires a sessionId, defaults read-only, copies arrays defensively, carries no
  secret-like keys (structural no-secret guarantee).
- Evidence ledger printed-not-run marking, ordering, bounded/non-mutating append.
- Agent readiness renders not-checked (with reason) when no git probe; dirty warning only on a real
  dirty fact; honest workspace/registry/executor reporting.
- Foundation render: all five sections present; required vocabulary present; current tier marked;
  Tier 5 flagged denied-by-default; git not-checked rendered; no-mutation messaging; muted
  placeholder when absent; NO new agent-run/agent-execute/chain action control; field-setter
  ordering invariant preserved.
```

---

## 10. Safety Confirmation

```text
source mutation capability added         : NO
file editing capability added            : NO
PR packaging capability added            : NO
live A2 workflow run                     : NO
preview / approval / apply-bundle / apply: NO (none run)
model / broker call                      : NO
runtime touched                          : NO
raw :11434 app inference                 : NO
new process spawn boundary               : NO (helperRunner unchanged)
network egress                           : NO
watcher / polling / timer                : NO
helper script touched                    : NO
.claw artifacts modified                 : NO
disposable smoke cleanup                 : NO
install-smoke 448d7ea touched            : NO
destructive commands used                : NONE
```

Validation note (transparency): the lane's whole-file "disallowed action wording" scan matches
pre-existing, unmodified content on `origin/main` — the existing `buttons.ts` forbidden-label guard,
the existing `render.ts` Safety/Stop-Gates negations, existing tests that assert these buttons are
absent, and runbook safety negations. That scan therefore flags the merged baseline regardless of
this change. The scan's intent — that no NEW executable action control is introduced — was verified
against this lane's diff: no new button and no new `data-ui-action` is added; every banned substring
in the diff appears only in negation comments or in test assertions proving the controls' absence.

---

## 11. Next Recommended Lane

```text
A2 Local Coding Agent Foundation v0 Review / Push PR
```

Docs/review lane: review this foundation, push the branch, open a PR for operator review. Do not
design or implement Tier 3 (disposable worktree mutation) until v0 is merged and a separate,
explicitly-approved mutation lane is opened.
