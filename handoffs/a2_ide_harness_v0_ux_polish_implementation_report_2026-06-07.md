# A2 IDE Harness v0 — UX Polish Implementation Report — 2026-06-07

> Implementation closeout for the small print/validate-only UX polish scoped in
> docs/a2-l4-ide-harness-v0-ux-polish-scope.md. The build lane ran NO A2 command (no
> preview/approval/apply/apply-bundle), made no model/broker/runtime call, and modified no real
> target or .claw artifact. The helper remains print/validate-only; no execution mode was added.

---

## 1. Approval

```text
Operator token (affirmative standalone line): APPROVED: Execute A2 IDE harness v0 UX polish implementation
Gate: satisfied. Implementation proceeded under the merged polish scope.
```

---

## 2. Source of Truth

```text
docs/a2-l4-ide-harness-v0-ux-polish-scope.md   (merged on main @ 2907147)
scripts/a2-ide-harness.sh                       (v0 helper, merged on main @ c5986e8)
```

---

## 3. What Changed

```text
scripts/a2-ide-harness.sh                       PATCHED
  - new artifact-based helpers: _first_artifact(), detect_chain_state(), print_next_step_hint()
  - find-artifacts now prints a next-step hint (both .claw-present and not-yet branches)
  - new read-only subcommand: audit-workspace --workspace <ws> [--target <t> --after-sha <sha>]
  - usage updated: audit-workspace documented + an explicit "Detection note" (artifact/hash, not logs)
docs/runbooks/a2-ide-harness-workflow.md        PATCHED
  - A2: Audit Workspace task row added
  - audit-workspace usage line added
  - new "Chain state & next-step hints (artifact/hash-based)" section explaining the false positive
    and why detection uses .claw artifacts + target hash, never free-text logs
.vscode/tasks.json                              PATCHED
  - added read-only "A2: Audit Workspace" task (calls the helper's audit-workspace; never calls claw)
handoffs/a2_ide_harness_v0_ux_polish_implementation_report_2026-06-07.md  NEW (this report)
```

---

## 4. Chain-State Model

```text
detect_chain_state(ws) inspects .claw ARTIFACTS and echoes exactly one of:
  not-started | preview-ready | approval-ready | apply-bundle-ready | applied | unknown

Evidence precedence (strongest first):
  apply-result.json present        -> applied
  apply-bundle.json present        -> apply-bundle-ready
  approval-result.json present     -> approval-ready
  preview-bundle.json present      -> preview-ready
  .claw exists, none of the above  -> unknown
  no .claw                         -> not-started
```

Each state maps to a precise next-step hint (print-preview → print-approval → print-apply-bundle →
print-apply → verify-final).

---

## 5. False-Positive Fix

```text
Root cause (from the smoke test): verify-final prints marker names (a2-l2b-write-applied, etc.) as
operator guidance, so a broad grep over free-text helper output falsely read them as execution evidence.

Fix: detection is now ARTIFACT/HASH-based. audit-workspace and find-artifacts decide "applied" from the
presence of the executor-written apply-result.json artifact, and verify the target via sha256 against the
expected after_sha256. Marker-name guidance is KEPT (it helps operators) but is never treated as evidence.
audit-workspace explicitly states it inspects artifacts + hash only, never free-text logs, and never runs claw.
```

---

## 6. Safety Boundaries Preserved

```text
print/validate-only: yes (no execution mode added; audit-workspace inspects files + hashes only)
no live A2 command: helper never executes claw (no exec/eval; no $A2_CLAW invocation)
real-terminal approval: unchanged (print-approval still requires a REAL TTY; grammar shown)
apply-bundle = GENERATOR; plan apply = EXECUTOR / only target writer: labels unchanged
no auto-approval / no hidden apply / no batch/--yes/fake-TTY
no model / broker / runtime call; no raw :11434 inference
absolute after_file refusal + sensitive-path warning: unchanged
```

---

## 7. Validation Results

```text
bash -n scripts/a2-ide-harness.sh:        OK
python3 -m json.tool .vscode/tasks.json:  valid
help smoke:                               OK (audit-workspace + detection note present)
find-artifacts hints:                     OK (not-started branch + .claw-present branch)
audit-workspace state smokes (fixtures, /tmp, self-removed):
  not-started        -> "No .claw yet…"                       OK
  preview-ready      -> "Next: print-approval…"               OK
  approval-ready     -> "Next: print-apply-bundle…"           OK
  apply-bundle-ready -> "Next: print-apply…"                  OK
  applied            -> "Next: verify-final…"                 OK
  applied + matching after_sha  -> MATCH, rc=0                OK
  applied + wrong after_sha     -> MISMATCH, rc=3             OK
direct-execution scan:                    only prohibition/guidance text; no claw execution
git diff --check:                         clean
```

---

## 8. Build-Lane Safety Attestation

```text
live preview run:           NO
live approval run:          NO
live apply-bundle run:      NO
live apply run:             NO
model/broker call:          NO
runtime touched:            NO
target modified:            NO (only a self-created /tmp fixture, removed by explicit guarded path)
artifacts modified/deleted: NO (no real .claw artifacts touched)
auto-approval / hidden apply: NO
Rust / schema / runtime:    NO
```

---

## 9. Next Lane

```text
A2 IDE Harness v0 UX Polish Review / Push PR (helper + runbook + tasks + report).
Do not run the live A2 workflow from the review lane; live exercise stays operator-driven at a real TTY.
```
