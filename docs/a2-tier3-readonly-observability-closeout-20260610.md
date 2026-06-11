# A2 Tier 3 Read-Only Observability — Closeout — 2026-06-10

> **Docs-only closeout.** This note records that the Tier 3 **read-only observability**
> stack is complete on `main`. It performs no action: no collector run, no live smoke, no
> orchestrator/validate-lane/apply-lane, no worktree creation or cleanup, and no source
> change. It only documents the shipped chain, the safety posture, preserved evidence, CI
> coverage, and next options.

## 1. Executive Summary

Tier 3 read-only observability is **complete on `main`**. The full chain shipped, in order:

```text
evidence surface contract (WHAT)  →  collector design (HOW)  →  collector implementation
scope card (BOUNDARIES)  →  read-only evidence collector (CODE)  →  status panel scope card
(BOUNDARIES)  →  A2 harness panel CI  →  read-only status panel renderer (CODE)
```

An operator can now answer "Is Tier 3 ready? What was the last proven run? Where is the
evidence? Is the control checkout clean? What is safe to do next?" entirely **read-only**,
via the collector's `a2-tier3-evidence-snapshot.v0` and the panel renderer that displays it.
No execution capability was added anywhere in this chain.

## 2. Shipped Chain (on `main`, top = 3d33426)

```text
#122  29f5271  fix(a2): approve on enter without weakening tty gate   (Enter-to-Approve)
#123  17e1b77  docs(a2): define tier 3 evidence surface contract       (WHAT may be displayed)
#124  b1a6518  docs(a2): design read-only tier 3 evidence collector    (HOW gathered, read-only)
#125  b03ee09  docs(a2): scope card for collector implementation       (code BOUNDARIES)
#126  3144dfd  feat(a2): read-only tier 3 evidence collector           (collector CODE)
#127  12e2d38  docs(a2): scope card for read-only status panel         (panel BOUNDARIES)
#129  ff76b7a  ci(a2): validate harness panel changes                  (A2 harness panel CI)
#128  3d33426  feat(a2): read-only tier 3 status panel renderer        (panel renderer CODE)
```

(#122 is the approval-UX fix that preceded the observability work; #123–#128 are the
observability chain, with #129 inserted before #128's merge to provide CI coverage.)

## 3. What Is Now Proven

```text
- A Tier 3 live apply smoke succeeded once in a throwaway disposable worktree (closure
  2026-06-10, PASS_WITH_NOTES) — exactly one declared file (SMOKE_NOTES.md) written inside
  the worktree; control checkout stayed clean.
- The read-only collector emits a deterministic a2-tier3-evidence-snapshot.v0 from git state
  + a worktree .claw artifact tree, with a passing sha256 cross-check against the recorded
  payload digest.
- The collector was verified deterministic from the merged main (identical bytes across runs;
  .claw file count stable → mutates nothing).
- The status panel renderer consumes snapshot-only input, is fail-closed on schema mismatch,
  and renders unknown/null as UNKNOWN/—.
- The A2 harness panel CI is present and has gated the renderer: PR #128 was re-checked under
  the new workflow and merged green.
```

## 4. Read-Only Safety Boundary

The entire chain is read-only by construction:

```text
- the collector does NOT run claw / orchestrator / validate-lane / apply-lane, writes no
  target, mutates no .claw artifact, and performs no network fetch.
- the panel renderer does NOT gather evidence itself and does NOT execute shell commands.
- the panel exposes ZERO execution controls: no Run, no Apply, no Approve, no Create Worktree,
  no Cleanup.
- no runtime / model / broker / Vault access; no raw :11434 app inference anywhere in the chain.
- approval remains a human-typed terminal step (DO_NOT_RUN on the surface); the panel never
  composes or submits an approval.
```

These boundaries are enforced structurally: the collector's source-grep guards and the
panel package's `run-guards.js` (now run in CI) reject fs/network/process/secret/approval
patterns in live code.

## 5. Evidence and Smoke-Worktree Preservation State

```text
- the canonical successful smoke worktree remains PRESERVED:
  /mnt/vast-data/git-worktrees/stack-code-a2-tier3-live-smoke-20260609_201950-4441
- the partial smoke worktrees (11 of them, alongside the canonical success = 12 total) remain
  PRESERVED as local forensic artifacts.
- cleanup/retirement of any smoke worktree requires a SEPARATE, exact-path, explicitly-approved
  lane (non-force only). It is out of scope here.
- install-smoke / local commit 448d7ea
  (stack-code-a2-extension-panel-install-smoke-scope-20260607) must remain untouched unless
  separately scoped.
```

## 6. CI Coverage State

```text
- Rust/docs CI (rust-ci.yml) covers the collector crate (rust/**): cargo fmt / test / clippy
  --workspace exercise rust/crates/a2-evidence-collector on every rust/ change.
- A2 harness panel CI (a2-harness-panel.yml, #129) covers ide/vscode/a2-harness-panel/**:
  npm ci → run-guards → mocha → tsc on every panel change.
- PR #128 was re-gated after #129: the renderer branch was rebased onto the CI-bearing main,
  the panel CI attached and ran green, and #128 merged WITH CI coverage (closing the earlier
  zero-CI gap).
```

## 7. What Is Explicitly Not Included

```text
- no Tier 4 packaging (stage/commit/PR/branch automation) of any kind
- no autonomous approval and no preapproval / non-interactive / fake-TTY approval path
- no panel execution (the panel stays a read-only mirror)
- no worktree cleanup automation
- no mutation flow beyond the existing, already-approved Tier 3 orchestrator
- no live-panel wiring of snapshot acquisition (the renderer is a pure module; acquisition is
  a separate, separately-approved concern)
```

## 8. Remaining Risks / Caveats

```text
- apply-result is stdout-only for the successful smoke; NO persisted apply-result.json file was
  observed. The collector reports apply_result_mode = stdout_only and never claims a persisted
  file unless one is actually present. Do not assume a persisted apply-result on this build.
- smoke worktrees remain as local forensic artifacts; they accumulate disk and should be
  retired only via a separate explicitly-approved non-force lane.
- future panel integration must remain snapshot-only; acquiring the snapshot (running the
  collector / reading a file) must not be wired into the guard-safe panel modules without a
  separately-approved design.
- the panel renderer is not yet wired into the live extension view; it is a tested, read-only
  module awaiting an integration decision.
```

## 9. Recommended Next Strategic Lanes (in order)

```text
1. Operator-facing integration review — how the read-only status panel renderer is surfaced in
   the live extension (snapshot acquisition stays outside guard-safe modules; display-only).
2. Read-only collector invocation UX design (if needed) — how an operator runs the collector
   and feeds its snapshot to the panel, read-only.
3. Tier 3 review workflow polish — freshness banners, partial-count surfacing, link ergonomics.
4. Tier 4 packaging DESIGN ONLY — and only after explicit operator approval (see §10).
```

## 10. STOP Gates Before Tier 4

```text
- no Tier 4 implementation without a new explicit operator approval.
- no expansion from read-only status into execution controls (Run / Apply / Approve / Create
  Worktree / Cleanup) on any surface.
- no packaging / branch / push automation without a separate, explicitly-approved design.
- no runtime / model / broker / Vault integration, and no raw :11434 app inference.
- Tier 4 (any capability expansion) remains gated behind STABLE read-only observability —
  which this closeout records as achieved, not as license to proceed.
```

---

*Authored read-only from the merged chain (#123–#129 on `main`) and
`handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md`. Docs-only; no source,
script, Rust, IDE/panel, runtime, or CI file was touched, and no collector/smoke/orchestrator
action was taken in this lane.*
