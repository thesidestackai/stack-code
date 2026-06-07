# A2 Disposable Artifact Retirement — Scope (Docs-Only) — 2026-06-06

> Docs-only retirement PLAN. This document deletes, moves, archives, compresses, and modifies
> NOTHING. It scopes a FUTURE, separately-token-gated retirement lane. Inventory below was captured
> read-only.

---

## 1. Executive Summary

The A2-L2b proof chain is functionally complete (preview → approval → persisted approval-result →
apply-bundle → `plan apply` executor → write-validated target), and the final evidence handoff is
**merged on `main`** (`handoffs/a2_l2b_chain_final_evidence_2026-06-06.md`, merge commit
`3f251987379239cd8213006e334916ba26404d22`). This document only scopes the OPTIONAL future retirement
of disposable trial/build artifacts. It performs **no deletion**. Retirement is **not required** for
functional completion (see §7 Keep-Indefinitely).

---

## 2. Current State

```text
A2-L2b chain:           complete; target written before_sha → after_sha and write-validated.
final evidence:         merged on main (3f25198).
local post-merge cleanup: complete (main ff'd; PR #89 worktree/branch/stale-ref removed; no force).
this lane:              docs-only retirement scope; deletes nothing.
```

---

## 3. Preserved Evidence on main

```text
handoffs/a2_l2b_chain_final_evidence_2026-06-06.md   (PERMANENT — on main)
final evidence merge commit: 3f251987379239cd8213006e334916ba26404d22
```

This record on `main` is the durable, authoritative evidence of the chain. It must be preserved
permanently and is independent of the disposable /tmp artifacts.

---

## 4. Artifact Inventory (read-only, 2026-06-06)

```text
PRESENT  /tmp/s2c1d_ready_to_preview_20260605_142019                              dir  ~152K   (disposable trial workspace root)
PRESENT  /tmp/s2c1d_ready_to_preview_20260605_142019/workspace                    dir  ~116K
PRESENT  …/preview_target_update/preview-bundle.json                             file ~4K    (preview evidence)
PRESENT  …/preview_target_update/preview-generator-result.json                   file ~4K    (preview evidence)
PRESENT  …/l2b-approval-results/preview_target_update/approval-result.json        file ~4K    (approval evidence)
PRESENT  …/preview_target_update/apply-bundle.json                               file ~4K    (apply evidence)
PRESENT  …/sample/preview_target.txt                                             file ~4K    (applied target, at after_sha256)
PRESENT  /tmp/s2e_apply_executor_20260606.log                                    file ~4K    (apply-result envelope log)
PRESENT  /media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw    file ~157M  (built CLI, dc73a5e — possibly shared build output)
PRESENT  /mnt/vast-data/git-worktrees/stack-code-a2-l2b-preview-cli-build-20260605 dir ~1.4G  (preview-CLI build worktree incl. cargo target)
```

Sizes are approximate (read-only `du`). The disposable /tmp proof workspace is tiny (~152K total);
the disk weight is the build worktree (~1.4G, mostly cargo `target/`) and the standalone binary (~157M).

---

## 5. Must Preserve / Archive Before Deletion

A future retirement lane MUST preserve or archive (copy out before removing the /tmp workspace):

```text
preview-bundle.json
preview-generator-result.json
approval-result.json
apply-bundle.json
/tmp/s2e_apply_executor_20260606.log
the final target file (sample/preview_target.txt at after_sha256 8a7b6e95…)
the final handoff from main (handoffs/a2_l2b_chain_final_evidence_2026-06-06.md)
```

Recorded canonical hashes (for archive verification):

```text
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
```

Archive destination (suggested, operator-confirmable): a durable, non-/tmp path (e.g. under
`/mnt/vast-data/…`); the `.claw` evidence JSONs are already largely captured inline in the merged
final handoff, so archiving is belt-and-suspenders.

---

## 6. Candidate Disposable Artifacts (inventory only — DO NOT DELETE here)

```text
/tmp/s2c1d_ready_to_preview_20260605_142019                              (disposable trial workspace — safest to retire after archive; ~152K)
/mnt/vast-data/git-worktrees/stack-code-a2-l2b-preview-cli-build-20260605 (preview-CLI build worktree — ~1.4G; biggest reclaim; remove via `git worktree remove` non-force, NOT rm -rf)
/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw     (built binary — ~157M; SEE CAUTION below)
```

**Caution on the built `claw` binary:**

```text
- It may be shared Cargo target output (a target/debug/ artifact), not exclusive to this proof.
- Do NOT recommend deleting the binary alone unless source/build ownership confirms it is not a
  shared build artifact.
- Prefer keeping shared build artifacts; if disk pressure is the driver, handle the build worktree's
  cargo target/ via a SEPARATE build-cache cleanup lane (e.g. `cargo clean` in that worktree, or
  `git worktree remove` of the dated build worktree), never a raw rm of one binary.
```

The build worktree under `/mnt/vast-data/git-worktrees/` should be retired with
`git worktree remove <exact-path>` (non-force, only if clean) — never `rm -rf`.

---

## 7. Keep-Indefinitely Option

```text
Retirement is OPTIONAL. The functional chain is complete and its evidence is preserved on main.
If disk pressure is low, keeping ALL A2 artifacts indefinitely is fully acceptable.
The only material disk consumers are the build worktree (~1.4G) and the binary (~157M); the proof
workspace itself is negligible (~152K) and can simply be kept.
Recommendation: retire only if disk needs to be reclaimed; otherwise keep.
```

---

## 8. Future Retirement Token

The future retirement EXECUTION prompt must require this exact token, and STOP without it:

```text
APPROVED: Execute A2 disposable artifact retirement
```

This drafting lane required no token (it deletes nothing).

---

## 9. Future Retirement Execution Rules

The future retirement lane must:

```text
- be a SEPARATE lane, gated by the exact token (§8).
- list EXACT paths to retire; act only on those named paths.
- verify the final evidence handoff is merged on main (3f25198) BEFORE any deletion.
- ARCHIVE-BEFORE-DELETE: copy the §5 must-preserve set to a durable non-/tmp location and verify
  hashes, BEFORE removing the /tmp workspace.
- use named-path operations only:
    git worktree remove <exact-path>        (non-force; for the build worktree, only if clean)
    explicit per-file/dir removal of the named /tmp workspace AFTER archive + operator confirmation.
- NEVER use `rm -rf`, `git clean`, `find … -delete`, `find … -exec rm`, `git reset --hard`,
  `git worktree remove --force`, or `git fetch --prune`.
- treat the built binary conservatively (§6 caution); shared build artifacts handled in a separate
  build-cache lane.
- STOP on any §13 condition.
```

---

## 10. Explicit Non-Goals

```text
- no preview rerun
- no approval rerun
- no apply rerun
- no runtime / model / broker / Vault touch
- no source-code changes
- no deletion, move, archive, compression, or permission change in THIS drafting lane
```

---

## 11. Validation Before Retirement

The future lane must confirm, before deleting anything:

```text
- final evidence handoff present on main (merge 3f25198) and readable.
- every §5 must-preserve artifact exists and was archived to a durable location with matching hashes
  (preview_sha256 1c856762…, before d646ebba…, after 8a7b6e95…).
- the applied target (if still present) is at after_sha256 (or its content is captured in the handoff/archive).
- each candidate path is exactly within the expected disposable set; ownership is unambiguous.
- operator token present.
```

---

## 12. Validation After Retirement

The future lane must confirm, after deleting only the named paths:

```text
- the archived evidence remains intact (hashes re-verify).
- the final evidence handoff on main is untouched.
- only the named candidate paths were removed; nothing else.
- the control checkout and origin/main are unchanged.
- no force/destructive command was used.
```

---

## 13. STOP Conditions

The future retirement lane must STOP if any of:

```text
- final evidence handoff missing on main
- any §5 evidence artifact missing before archive
- archive hash mismatch
- the applied target is present but NOT at after_sha (unexpected drift)
- a candidate path's ownership is unclear or points outside the expected disposable set
- the built binary appears to be a shared build artifact and deletion was requested
- the operator token is missing
- a force deletion or broad cleanup command would be required
- a build worktree is dirty (do not force-remove)
```

---

## 14. Recommended Future Lanes

```text
1. A2 Disposable Artifact Retirement Scope Review / Push PR (this doc).
2. A2 Disposable Artifact Retirement Scope exact-head merge gate.
3. (token-gated) A2 Disposable Artifact Retirement Execution — archive-before-delete of the named
   /tmp workspace + build worktree, per §9.
4. (optional, separate) Build-cache cleanup lane for cargo target/ if disk pressure warrants.
```

---

## 15. Final Recommendation

```text
Retirement is OPTIONAL and currently UNNECESSARY for correctness — the chain is complete and its
evidence is permanently on main. If/when disk reclaim is wanted, run a SEPARATE token-gated lane that
archives the §5 evidence first, then removes ONLY the named /tmp workspace and the build worktree via
safe, non-force commands, treating the built binary conservatively as possibly-shared. Until then,
keeping everything is acceptable.
```
