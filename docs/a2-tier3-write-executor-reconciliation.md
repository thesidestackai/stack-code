# A2 Tier 3 Write-Capable Executor — Reconciliation with the Existing Rust Write Executor

> Docs-only reconciliation note. It compares the merged Tier 3 write-capable executor DESIGN against
> the write-execution capability that ALREADY exists in `rust/crates/a2-plan-runner`, and recommends
> the safest integration lane. It implements nothing, edits no source, creates no executor, runs no
> live A2 workflow, creates no worktree, and writes no target.

---

## 1. Why this note exists

The merged write-capable design scope
(`docs/a2-tier3-mutation-executor-write-capable-design-scope.md`, PR #112) assumed the Tier 3
write-capable step would be a **new external executor**. While opening the implementation lane, an
inventory of `origin/main` found that a **proven, tested write-execution capability already exists** in
the Rust crate `a2-plan-runner`. Building a new parallel executor would duplicate — and risk diverging
from — an already-hardened safety surface. This note reconciles the design with what exists and
recommends the integration path, before any executor code is written.

---

## 2. What already exists on `origin/main`

```text
rust/crates/a2-plan-runner/src/write_executor.rs   (~1725 lines)  A2-L2b single-file write executor (slice 4)
rust/crates/a2-plan-runner/src/checkpoint.rs       (~864 lines)   A2-L2b checkpoint store (slice 2)
rust/crates/a2-plan-runner/src/write_preview.rs / diff_preview / approval / write_runtime / write_payload
rust/crates/a2-plan-runner/tests/l2b_write_executor.rs, l2b_checkpoint_store.rs
```

The IDE harness panel (PR #92 onward) and the whole A2-L2b chain the panel PRINTS commands for
(`claw plan run / approve / apply-bundle / apply`) drive **this** executor. `claw plan apply` is the
real, only command that writes a target.

### 2.1 Safety contract already enforced by `write_executor.rs`

```text
- Mutates EXACTLY one file when the full authority chain matches; otherwise mutates nothing.
  (authority chain = ResolvedWriteTarget + CheckpointHandle + PreviewRecord + ApprovalDecision::Approved
   + ApprovedWritePayload, hash-bound by (step_id, preview_sha256) and after_sha256.)
- Same-directory temp + atomic rename + parent fsync; cross-device rename impossible by construction.
- No-clobber on new-file create (hard_link refuses overwrite); atomic in-place replace for existing.
- Pre-write baseline verified TWICE (before temp create + immediately before commit rename).
- Post-write the committed bytes are reopened and re-hashed; mismatch triggers bounded rollback.
- Automatic rollback ONLY for immediate post-write validation failure; refused if on-disk state drifted,
  checkpoint missing/corrupt, parent dir changed, or target is no longer a regular file.
- No subprocess. No shell-out to patch/diff porcelain. No broker / model / network. No `unsafe`.
```

### 2.2 Safety contract already enforced by `checkpoint.rs`

```text
- Never mutates an operator target; reads read-only, streams + SHA-256 into the checkpoint store.
- Writes ONLY inside <workspace_root>/.claw/l2b-checkpoints/<run-id>/<step-id>/.
- Refuses overwrite at the leaf step dir; refuses targets over a size cap; refuses non-regular-file targets.
- No subprocess/broker/model/approval/diff/rollback; minimal dependency set.
- 0o700 dirs / 0o600 files on Unix.
```

---

## 3. The merged Tier 3 design vs. what exists

```text
Design concern (PR #112)                         Already exists in a2-plan-runner?         Gap for Tier 3
Authority chain before any write                 YES (approval + preview + payload + ckpt)  none
Checkpoint before write                          YES (checkpoint.rs)                         none
Atomic write + post-write re-hash                YES (write_executor.rs)                     none
Bounded rollback                                 YES (auto, post-validation; strict refuse)  rollback-by-abandon is a
                                                                                             worktree-level wrapper concern
Exact-path / single-target scoping               YES (single file, resolved target)          Tier 3 declares a SET; the Rust
                                                                                             executor is one-file-per-call
Denials-win / Tier-3 allowlist                   N/A (the executor writes; it runs no cmds)  lives in the wrapper/dry-run layer
Disposable worktree lifecycle                     NO (executor is workspace-relative)         THE genuinely-new piece
Operator approval (real terminal, human-typed)   YES (claw plan approve)                      none
No model/broker/network/runtime                   YES                                         none
```

Conclusion: the **write + checkpoint + rollback + approval-chain** core is already built and hardened.
The only genuinely-new Tier 3 element is the **disposable-worktree lane** (create a fresh worktree from
origin/main, run the existing executor against a target inside it, then abandon the worktree) plus
multi-file orchestration over a declared set (the existing executor is single-file-per-call by design).

---

## 4. Options considered

```text
1. Wrap / drive the existing `claw plan apply` path:
   - The Tier 3 write-capable step orchestrates a disposable worktree and, inside it, drives the EXISTING
     claw apply chain (run -> approve -> apply-bundle -> apply) once per declared file. No new executor.
   - Pros: zero duplication of the hardened write surface; reuses the full authority chain, checkpoint,
     atomic write, rollback, and the proven test suite. Smallest new code. Operator approval stays the
     existing real-terminal, human-typed flow.
   - Cons: requires a thin orchestrator (worktree lifecycle + per-file iteration over the declared set).

2. Minimal external operator wrapper around the existing executor:
   - A standalone, operator-invoked tool (outside the panel) that performs ONLY the disposable-worktree
     lifecycle (create from origin/main; per declared file invoke the existing claw apply chain; produce
     diff; run approved validation; STOP; rollback = abandon the worktree). It calls the existing
     executor; it does not re-implement writing.
   - Pros: keeps the new surface tiny and outside the panel; the panel stays read-only and PRINTS the
     wrapper command. Still reuses the hardened write core.
   - Cons: a new script to maintain; its exact location/name needs separate approval (panel must not
     spawn it).

3. Extend the existing Rust executor in a separately-approved Rust lane:
   - Add disposable-worktree awareness / multi-file orchestration inside a2-plan-runner.
   - Pros: one cohesive, fully-tested Rust surface; strongest guarantees.
   - Cons: largest blast radius; edits hardened Rust under `unsafe_code = forbid`; needs a dedicated,
     separately-approved Rust lane with its own review. Out of scope for the current DRAFT's boundaries.
```

---

## 5. Recommendation

```text
Primary: Option 1 (wrap / drive the existing claw apply path) for the FIRST write-capable lane.
  - Do NOT build a new write executor. Reuse a2-plan-runner's write_executor + checkpoint via the
    existing `claw plan apply` chain that the panel already prints.
  - The only new design work is the DISPOSABLE-WORKTREE LANE around it (create from origin/main; iterate
    the declared exact-path set; diff; approved validation; STOP; rollback-by-abandon).
  - Keep the dry-run (executorDryRun.ts) as the precondition and the panel read-only (it PRINTS the
    wrapper/apply commands; it never spawns them, creates a worktree, or writes).

If any automation of the worktree lane is wanted later: Option 2 (a minimal external wrapper), with its
exact location/name separately approved, panel never spawning it.

Defer Option 3 (extending Rust) unless a dedicated, separately-approved Rust lane is explicitly opened.
```

This avoids duplicating an already-tested write surface and keeps the new surface to the
disposable-worktree lane only.

---

## 6. Impact on the merged write-capable design scope

```text
The PR #112 design remains valid in spirit (fresh-disposable-worktree-only writes; dry-run precondition;
explicit per-lane operator approval; exact-path scope; denials-win; checkpoint; diff; approved-only
validation; rollback-by-abandon; panel read-only; Tier 4 out of scope), with ONE correction:
  - "the executor writes files into the disposable worktree" should be realized by DRIVING the existing
    claw apply chain (a2-plan-runner write_executor) per declared file — NOT by a new write executor.
The write-capable implementation DRAFT
(handoffs/a2_tier3_mutation_executor_write_capable_implementation_prompt_DRAFT_2026-06-09.md) should be
revised to target an orchestrator over the existing executor (Option 1/2), not a new executor, before
any implementation lane begins.
```

---

## 7. Explicit Non-Goals (this note)

```text
No implementation. No executor script creation. No source edits (TS or Rust).
No write-capable implementation. No live A2 chain execution.
No disposable worktree creation. No target writes. No .claw mutation.
No model/broker/runtime/service action. No raw :11434 app inference.
No cleanup of smoke/demo artifacts. No touching install-smoke 448d7ea.
```

---

## 8. STOP Gates / Constraints carried forward

```text
The disposable-worktree lane (whichever option) must keep: clean control checkout; isolated worktree
from origin/main; exact-path scope (reject-outside / control-checkout reject); denials win over the
Tier-3 allowlist; dry-run-ready precondition; explicit per-lane operator approval; pre-write checkpoint;
diff summary; approved-only validation; rollback-by-abandon (never force-remove / force-delete /
revert-in-place). The panel stays read-only. No push / PR / merge (Tier 4, separate). Writes occur only
inside a fresh disposable worktree, never the control checkout or a real/live target.
```

---

## 9. Recommended Next Lane

```text
Name        : Tier 3 Write-Capable Executor DRAFT Revision (docs-only)
Type        : docs-only
Objective   : revise the write-capable implementation-prompt DRAFT to target an ORCHESTRATOR over the
              existing claw apply chain (Option 1, optionally Option 2's external wrapper) rather than a
              new executor; keep all STOP gates; re-state that the panel stays read-only and that any
              external wrapper's location needs separate approval.
Tool        : Claude Code (docs-only) + operator review.
Why         : align the implementation prompt with the existing, tested write surface before any code lane.
Mutation    : none (docs-only).
STOP gate   : do not begin any write-capable implementation until the revised DRAFT is reviewed/merged
              AND the operator explicitly opens an implementation lane (and approves any external
              wrapper's location). The panel stays read-only.
```

After this reconciliation is reviewed/merged, the revised DRAFT becomes the source of truth for the
write-capable implementation lane.
