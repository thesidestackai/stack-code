# A2 Tier 3 Foundation v0 — Implementation Report — 2026-06-08

> First Tier-3-related source lane. It adds the **readiness/state/render layer only** for the
> disposable worktree mutation path. It adds NO mutation executor, NO worktree-creation control, and
> NO write capability. The panel remains read-only / print-validate; the single spawn boundary is
> unchanged.

---

## 1. Executive Summary

Tier 3 Foundation v0 adds a read-only control plane for the disposable worktree mutation path, built
on the merged Foundation v0 and the merged Tier 3 design scope
(`docs/a2-tier3-disposable-worktree-mutation-scope.md`, PR #106). It adds five pure model modules
(Tier 3 readiness, disposable worktree plan, declared mutation scope, safe mutation policy, mutation
evidence ledger) and eight read-only panel sections. It enables **no** new capability: there is no
mutation executor, no worktree-creation control, no file write, and no agent-run / agent-execute /
apply / approve control.

Build is green: guards PASS (20 src files audited), `tsc` compile clean, `218 passing` unit tests
(up from 166; 52 new), all headless — no GUI, no chain, no model/broker/runtime call, no mutation.

---

## 2. Files Changed

New source modules (pure, no IO):

```text
src/tier3Readiness.ts          honest Tier 3 readiness (not-checked when unprobed; not-ready by default)
src/disposableWorktreePlan.ts  worktree plan validation (plan only; never creates; control-checkout-safe)
src/mutationScope.ts           declared exact-path set; classifyWrite accept/reject; control-checkout reject
src/safeMutationPolicy.ts      denials-win + Tier-3 allowlist; write gated by declared scope; classification only
src/mutationEvidence.ts        mutation ledger shape + format/append (printed-not-run)
```

New tests:

```text
test/tier3Readiness.test.ts
test/disposableWorktreePlan.test.ts
test/mutationScope.test.ts
test/safeMutationPolicy.test.ts
test/mutationEvidence.test.ts
test/tier3Render.test.ts
```

Edited (additive only):

```text
src/render.ts                            + Tier3View type + 8 read-only sections (tier3Block)
src/extension.ts                         + buildTier3View() (default not-ready) + mutationLedger session field
docs/runbooks/a2-ide-extension-panel.md  + Tier 3 Foundation v0 section
handoffs/a2_tier3_disposable_worktree_mutation_foundation_implementation_report_2026-06-08.md (this report)
```

Not touched: `scripts/a2-ide-harness.sh` (helper), Foundation v0 modules' behavior, Rust, schemas,
CI. The single spawn boundary (`helperRunner.ts`) is unchanged.

---

## 3. What Was Added

```text
- Tier 3 readiness model: per-dimension honest tri-state (control-checkout-clean, origin/main,
  worktree-path-free, branch-name-free, operator-approved) + plan-valid / declared-scope /
  denied-registry; overall is "ready" ONLY when every gate is affirmatively yes — never ready by
  default. No guard-safe probe is wired in v0, so gated dimensions render not-checked with a reason.
- Disposable worktree plan model: validates a proposed worktree path + mutation branch + base WITHOUT
  creating anything; rejects paths outside the disposable root, the control checkout (or paths that
  contain it), a non-origin/main base, and main/master/whitespace branches.
- Declared mutation scope model: normalizes a declared exact-path set; classifyWrite accepts only a
  path that is in the declared set, inside the disposable worktree, and not under the control
  checkout (resolved via pure path normalization — traversal escapes are rejected). Deny by default.
- Safe mutation policy model: evaluateTier3Command checks the denied-command registry FIRST then a
  conservative Tier-3 allowlist (denials win); evaluateTier3Write gates writes by the declared scope.
  Classification/display only — nothing executes.
- Mutation evidence ledger: checkpoint/mutation/validation/decision/note events; printed-not-run
  markers; bounded + non-mutating append; render-only.
- Eight read-only panel sections: Tier 3 Readiness, Disposable Worktree Plan, Declared Touched Files,
  Mutation Approval Gate, Diff Summary (placeholder), Validation Results (placeholder),
  Rollback/Abandon guidance, Mutation Evidence Ledger.
```

---

## 4. What Remains Blocked (no mutation in v0)

```text
- No mutation lane is enabled. No file editing by the panel. No worktree creation by the panel.
- No mutation executor. No agent-run / agent-execute / apply / approve control.
- No PR creation / branch deletion by the panel.
- No live A2 chain (preview / approval / apply-bundle / apply).
- No model / broker / runtime / service call. No raw :11434 inference. No secret reads.
- No new process spawn boundary, no fs use, no network egress, no watcher / polling / timer.
```

---

## 5. Safety Confirmation

```text
file-writing capability added            : NO
worktree-creation capability added       : NO
mutation executor added                  : NO
write/create/agent-run/apply/approve control added : NO
network/broker/model/runtime/secret/:11434 added   : NO
fs/spawn added outside the single boundary : NO (helperRunner unchanged; guards confirm)
helper script touched                    : NO
.claw artifacts modified                 : NO
real target writes                       : NO
install-smoke 448d7ea touched            : NO
disposable smoke/demo cleanup            : NO
destructive commands used                : NONE
```

Honesty note: Tier 3 readiness renders `not-checked` and overall `not-ready` by default (no guard-safe
probe is wired in v0); git/worktree state is never fabricated. A dirty control checkout is a hard
block, surfaced prominently. The safe-mutation policy keeps denials winning over the Tier-3 allowlist
and writes limited to the declared exact-path set inside the disposable worktree — classification
only.

---

## 6. Tests / Guards / Build Results

```text
npm install --ignore-scripts : OK
guards (run-guards.js)        : PASS (20 src files audited; single spawn boundary intact)
compile (tsc -p .)            : clean
unit tests (mocha)            : 218 passing (52 new; previously 166)
```

New coverage:

```text
- Tier 3 readiness: not-checked when unprobed; not-ready by default; dirty control checkout is a hard
  block; ready only when every gate is yes.
- Disposable worktree plan: accepts a well-formed plan; rejects outside-root, control-checkout,
  non-origin/main base, and main/master/whitespace branches; never creates.
- Mutation scope: classifyWrite accepts a declared in-worktree path; rejects outside-set,
  outside-worktree, control-checkout (incl. traversal), and non-absolute paths.
- Safe mutation policy: denials win over the Tier-3 allowlist; non-allowlisted commands denied;
  writes gated by declared scope; nothing executes.
- Mutation ledger: printed-not-run marking; bounded/non-mutating append.
- Tier 3 render: all eight sections present; not-checked/not-ready rendered; plan-not-created and
  operator-not-approved shown; dirty-control-checkout block shown; NO write/create/executor/agent-run/
  apply/approve control; field-setter ordering invariant preserved.
```

---

## 7. Next Recommended Lane

```text
Tier 3 Foundation v0 Review / Push PR
```

Docs/review + push lane: review the Tier 3 readiness/state/render layer, push the branch, open a PR
for operator review. Do not design or implement an actual mutation executor or a worktree-creation
control until Tier 3 Foundation v0 is merged and a separate, explicitly-approved mutation lane is
opened.
