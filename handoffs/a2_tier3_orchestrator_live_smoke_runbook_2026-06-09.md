# A2 Tier 3 Write-Capable Orchestrator — Live Smoke Runbook (operator-terminal only, 2026-06-09)

> This runbook hands the OPERATOR a ready, gate-verified kit to prove the live `apply-lane` path of
> `scripts/a2-tier3-write-orchestrator.sh` once, end-to-end, inside a single throwaway disposable
> worktree. The live `apply-lane` step is OPERATOR-RUN AT A REAL TERMINAL ONLY — it requires a
> human-typed approval (`claw plan approve` fails closed off-TTY). Do not run `apply-lane` from an
> automated agent / CI / command runner.
>
> The `validate-lane` (pure gate) step below has already been run and passes (see "Gate proof"); it
> performs no claw call, creates no worktree, and writes nothing.

Merged dependencies (all on `origin/main`): #113 reconciliation, #114 DRAFT redirect, #115 orchestrator
v0, #116 plan-gate `write_target.path` fix (top: `9d66496`).

---

## 1. Safety contract (do not weaken)

```text
- Writes occur ONLY inside a FRESH throwaway disposable worktree under /mnt/vast-data/git-worktrees/.
  Never the control checkout (/home/suki/stack-code), never a real/live target.
- REAL interactive terminal required: you type the approval line yourself; no --yes, no batch, no fake-TTY.
- One disposable worktree, exactly one declared file written, then STOP for review.
- Rollback = ABANDON the disposable worktree. Never force-remove, never push/PR/merge/branch-delete.
- No model/broker/runtime/network/Vault; no raw :11434 app inference. The panel stays read-only.
```

---

## 2. Prerequisites

```text
1. A built `claw` binary. Point A2_CLAW at it, e.g.:
     export A2_CLAW="/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw"
   (a2-ide-harness.sh uses the same default; override as needed).
2. Clean control checkout on origin/main (the orchestrator refuses a dirty base).
3. The plan's `after_file` (the byte SOURCE) MUST exist inside the disposable worktree at apply time.
   Because apply-lane creates the worktree FRESH from origin/main, after_file must name a path that is
   TRACKED on origin/main. The template plan below uses examples/a2_l1a_valid_readonly_plan.yaml (a
   small tracked file) as the byte source, so it is guaranteed present in the fresh worktree.
4. The disposable worktree path and branch in the lane must be FREE (not already existing).
```

---

## 3. Fixtures (templates — adjust the dated worktree path/branch as you like)

Stage these three files anywhere outside the disposable worktree (e.g. a scratch dir). The orchestrator
reads them; it never writes them.

`lane.json` — the operator-approved lane:

```json
{
  "objective": "Tier 3 orchestrator live smoke: write one declared file in a throwaway worktree",
  "worktreePlan": {
    "worktreePath": "/mnt/vast-data/git-worktrees/stack-code-a2-tier3-smoke-20260609",
    "branch": "smoke/a2-tier3-write-orchestrator-20260609",
    "base": "origin/main"
  },
  "declaredPaths": [
    "/mnt/vast-data/git-worktrees/stack-code-a2-tier3-smoke-20260609/SMOKE_NOTES.md"
  ],
  "proposedWrites": [
    "/mnt/vast-data/git-worktrees/stack-code-a2-tier3-smoke-20260609/SMOKE_NOTES.md"
  ],
  "proposedCommands": [],
  "operatorApproved": true
}
```

`evidence.json` — the dry-run-ready evidence (must be `ready: true` and the same `worktreePath`):

```json
{
  "ready": true,
  "worktreePath": "/mnt/vast-data/git-worktrees/stack-code-a2-tier3-smoke-20260609",
  "summary": "dry-run ready for the smoke lane",
  "wouldCreateWorktree": false,
  "wouldWriteFiles": false
}
```

`plan.yaml` — the workspace-write plan. The file actually written is `write_target.path`
(`SMOKE_NOTES.md`, in the declared set); `after_file` is the byte source:

```yaml
name: a2-tier3-smoke
mode: read-only
model_tier: FAST
steps:
  - id: write-smoke-notes
    description: Write a single declared file in the disposable worktree
    mode: workspace-write
    tools: [Write]
    write_target:
      path: SMOKE_NOTES.md
      create_if_absent: true
    after_file: examples/a2_l1a_valid_readonly_plan.yaml
```

If the live `claw plan run` requires `expected_post_write` for your build, add a `must_contain` that
matches bytes present in the chosen `after_file` source (the template source is a YAML file containing,
e.g., `name:`), and a `must_not_contain` that is absent.

---

## 4. Gate proof (already run; pure — no claw, no worktree, no writes)

```text
$ bash scripts/a2-tier3-write-orchestrator.sh validate-lane \
    --approved-lane lane.json --dry-run-evidence evidence.json --plan plan.yaml
validate-lane: OK — all pure gates pass (scope, denials, plan, evidence-ready, approval).
  NOTE: this is the pure gate check only. apply-lane additionally requires a real TTY,
        a clean control checkout, origin/main, a unique/free worktree, and operator review.
(exit 0)

# negative control — tamper write_target.path out of the declared set:
$ ... --plan plan_bad.yaml   ->   GATE REFUSED: plan write_target ... -> rejected ...   (exit 4)
```

The gate layer (exact-path scope on `write_target.path`, denials-win, dry-run-ready, operator approval,
worktree-plan rules) is verified for this fixture set. What remains is the LIVE drive, below.

---

## 5. Operator command sequence (REAL TERMINAL ONLY)

```bash
# 0. Point at a built claw and confirm a clean base on origin/main.
export A2_CLAW="/path/to/built/claw"
git -C /home/suki/stack-code status -sb     # expect clean, on main

# 1. Pure gate check (safe to repeat; no claw, no worktree, no writes).
bash /home/suki/stack-code/scripts/a2-tier3-write-orchestrator.sh validate-lane \
  --approved-lane lane.json --dry-run-evidence evidence.json --plan plan.yaml

# 2. LIVE drive — at a REAL interactive terminal. apply-lane will:
#      - re-run the gates, refuse off-TTY (exit 7),
#      - create ONE disposable worktree from origin/main,
#      - drive: claw plan run -> approve (YOU type:  apply <step-id> <preview_sha256>) ->
#        apply-bundle -> apply  (claw plan apply writes SMOKE_NOTES.md inside the worktree),
#      - print checkpoint/apply-result evidence + a git diff summary, then STOP.
bash /home/suki/stack-code/scripts/a2-tier3-write-orchestrator.sh apply-lane \
  --approved-lane lane.json --dry-run-evidence evidence.json --plan plan.yaml

# 3. Review the printed diff + .claw/.../apply-result.json inside the disposable worktree.
#    Confirm SMOKE_NOTES.md landed ONLY inside the throwaway worktree.

# 4. Rollback-by-abandon — leave the worktree for inspection, or retire it later with a
#    NON-FORCE removal once you are done (never --force, never branch -D in this flow):
#      git -C /home/suki/stack-code worktree remove <the-throwaway-worktree>
```

Do NOT run `apply-lane` in a non-interactive context — it will (correctly) refuse at the TTY gate
(exit 7) and create no worktree.

### The approval step is interactive — it is NOT stuck

At STEP 2 the orchestrator prints a "what happens next" banner, then runs `claw plan approve`. claw shows
the diff preview and then **waits for your input at this terminal** — after a long diff the cursor can
look idle, but it is waiting, not hung. Look for claw's line `To approve, type exactly:` followed by the
real `apply <step-id> <preview_sha256>` (scroll up if the diff pushed it off-screen), type that EXACT
line, and press Enter. To abort with no write, press Ctrl-C. The orchestrator never types, pipes, or
composes the approval for you — you must type it. If claw refuses (exit 7 / approval-denied: wrong line,
replayed hash, off-TTY, or a `--yes`/`--auto`/batch form), the orchestrator now prints a specific
diagnostic and STOPs with nothing written; re-run at a real terminal and type the exact line claw prints.

---

## 6. Pass criteria

```text
- apply-lane reaches STEP 4 and claw writes SMOKE_NOTES.md inside the disposable worktree only.
- An apply-result artifact (.claw/.../apply-result.json) and a checkpoint exist under the worktree.
- git diff --stat (printed) shows exactly SMOKE_NOTES.md changed, inside the worktree.
- The control checkout (/home/suki/stack-code) is unchanged; no real/live target was written.
- No push/PR/merge/branch-delete/force-remove occurred.
```

---

## 7. What remains unproven until the operator runs §5

```text
- The live claw drive (run/approve/apply-bundle/apply) against a built A2_CLAW. The orchestrator's
  GATE layer is verified (§4); the live write path is exercised only by the operator at a real terminal.
- Exact .claw artifact filenames/locations are taken from a2-ide-harness.sh + the merged scope, not from
  a live run in this lane. If claw's artifact names differ on your build, apply-lane will fail SAFELY
  (no target write) at the artifact-locate step; report the mismatch rather than forcing.
- expected_post_write semantics for your build (§3 note) may need a must_contain aligned to after_file.
```

---

## 8. This runbook lane did NOT

```text
- run the orchestrator's apply-lane, run live A2, or create a disposable smoke worktree
- write any target file or mutate any .claw artifact
- call model/broker/runtime/:11434/Vault; use no raw app inference
- push/PR/merge/delete branches/force-remove worktrees
It only: generated the fixtures, ran the pure validate-lane gate check (proof in §4), and wrote this doc.
```
