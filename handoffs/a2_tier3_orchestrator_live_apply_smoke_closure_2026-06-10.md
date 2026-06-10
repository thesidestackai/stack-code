# A2 Tier 3 Orchestrator Live Apply Smoke Closure — 2026-06-10

## Classification

PASS_WITH_NOTES

(All required evidence persisted EXCEPT a persisted `apply-result.json` file — the apply-result was
emitted to stdout only. See "Apply Result Nuance".)

## Executive Outcome

One end-to-end live apply smoke succeeded in a throwaway disposable worktree, at a real operator
terminal, after the approval-stdin diagnostics landed on `main`
(`scripts/a2-tier3-write-orchestrator.sh`, through PR #120 / `06b1edf`). The orchestrator drove the
existing `claw plan apply` / `a2-plan-runner` write_executor chain to write exactly one declared file
(`SMOKE_NOTES.md`) inside the disposable worktree; the control checkout stayed clean.

## Successful Smoke Worktree

```text
/mnt/vast-data/git-worktrees/stack-code-a2-tier3-live-smoke-20260609_201950-4441
branch: smoke/a2-tier3-live-smoke-20260609_201950-4441   (base origin/main)
run-id: 01KTQRQHAPZN9RANMT78MQ2B45   step-id: write-smoke-notes
```

## Proven (verified read-only from the worktree, 2026-06-10)

- `validate-lane` passed (pure gate check).
- preview `rc=7` accepted as write-preview-ready / approval-pending, only because the preview-ready
  artifacts were present.
- human approval accepted at a real terminal (no pipe/script/fake-TTY/auto).
- `approval-result.json` persisted.
- `apply-bundle.json` persisted.
- the existing `claw plan apply` / write_executor wrote `SMOKE_NOTES.md` inside the disposable worktree.
- control checkout `/home/suki/stack-code` remained clean (only `.claw/` + `SMOKE_NOTES.md` are
  untracked, inside the disposable worktree).
- no push / PR / merge / branch deletion / force cleanup occurred during the smoke.
- rollback posture remains: abandon the disposable worktree.

## Evidence (verified paths under the successful smoke worktree)

```text
SMOKE_NOTES.md                       SMOKE_NOTES.md            sha256 cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8
approval-result.json                 .claw/approval-result.json
preview-bundle.json                  .claw/l2b-preview-bundles/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/preview-bundle.json
preview-generator-result.json        .claw/l2b-preview-bundles/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/preview-generator-result.json
apply-bundle.json                    .claw/l2b-preview-bundles/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/apply-bundle.json
checkpoint manifest                  .claw/l2b-checkpoints/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/manifest.json
payload after.bin                    .claw/l2b-payloads/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/after.bin
payload after.sha256                 .claw/l2b-payloads/01KTQRQHAPZN9RANMT78MQ2B45/write-smoke-notes/after.sha256
run manifest                         .claw/l2b-runs/01KTQRQHAPZN9RANMT78MQ2B45/run-manifest.json
run status                           .claw/l2b-runs/01KTQRQHAPZN9RANMT78MQ2B45/status.json
```

## Apply Result Nuance

`claw plan apply` EMITTED an `a2-l2b-apply-result.v1` JSON object to **stdout** (operator-captured):

```json
{"exit_code":0,"markers":["a2-l2b-write-preflight-ok","a2-l2b-write-temp-created","a2-l2b-write-applied","a2-l2b-write-validated"],"outcome":"applied","schema_version":"a2-l2b-apply-result.v1","step_id":"write-smoke-notes","target_relative_path":"SMOKE_NOTES.md"}
```

**No persisted `apply-result.json` file was observed** in the artifact listing (verified 2026-06-10:
`find <worktree>/.claw -name 'apply-result*.json'` returned nothing). The apply outcome is therefore
evidenced by the stdout JSON above (exit_code 0, outcome "applied", markers including
`a2-l2b-write-applied` and `a2-l2b-write-validated`) plus the written `SMOKE_NOTES.md` and the persisted
preflight artifacts — not by a persisted apply-result file.

## Operator UX Finding

The successful approval required, at a real terminal:

1. type/paste the exact approval line claw printed (`apply write-smoke-notes <preview_sha256>`);
2. press Enter;
3. press **Ctrl+D once** if the process continued waiting for EOF.

The merged approval diagnostics (PR #120) surface the per-exit-code cause when this is not done (e.g.
exit 7 = refused / non-approvable / EOF / drift / non-TTY). The runbook
(`handoffs/a2_tier3_orchestrator_live_smoke_runbook_2026-06-09.md`) is updated with the Ctrl+D step.

## Not Proven / Not Claimed

- no Tier 4 packaging (stage/commit/PR) — out of scope.
- no production / real-target write — only `SMOKE_NOTES.md` inside a throwaway worktree.
- no panel-triggered execution — the panel stays read-only; the orchestrator is operator-run only.
- no autonomous approval — approval was human-typed at a real terminal.
- no cleanup of partial or successful smoke worktrees in this lane.
- no persisted `apply-result.json` file — stdout-only on this build; do not claim one unless a future
  build adds it.

## Follow-Up Recommendation

Docs-only review/merge of this closure + the runbook patch, then an OPTIONAL, explicitly-approved
housekeeping lane to retire the accumulated merged/clean partial smoke worktrees (non-force only;
preserve the successful smoke worktree until you have harvested its evidence).
