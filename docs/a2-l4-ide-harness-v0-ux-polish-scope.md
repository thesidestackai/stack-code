# A2-L4 IDE Harness v0 — UX Polish Scope (Docs-Only) — 2026-06-07

> Docs-only scope card. It implements NOTHING: no helper/tasks/runbook edit, no Rust/runtime change,
> no preview/approval/apply run, no model/broker/runtime call. It scopes a small FUTURE polish patch
> based on the print/validate-only operator smoke test of A2 IDE Harness v0.

---

## 1. Executive Summary

A2 IDE Harness v0 is merged on `main` (`c5986e8`) and was operator-smoke-tested in print/validate-only
mode. The smoke test **passed with notes**. The single finding is a **false-positive detection
issue**, not accidental execution: `verify-final` prints A2 marker names as operator guidance, and a
broad grep over free-text smoke logs matched that guidance as if `apply` had run. No live
preview/approval/apply ran; the target was unchanged and the repo stayed clean. This scope performs
no implementation — it defines a small, safe future patch.

---

## 2. Smoke-Test Finding

```text
CLASSIFICATION: PASS_WITH_NOTES   MODE: A2_IDE_HARNESS_V0_OPERATOR_SMOKE_PRINT_VALIDATE_ONLY
help:                       PASS
task labels:                PASS (8 A2 tasks present)
validate-input:             PASS (relative after_file → rc=0)
absolute after_file refusal: PASS (rc=3, refused + relative-path guidance)
print-preview:              PASS (prints `claw plan run … --workspace-write-preview`; "Writes NO target")
find-artifacts:             PASS (clean empty-state)
print-approval:             PASS (real-terminal note + approval grammar)
print-apply-bundle:         PASS (labeled GENERATOR)
print-apply:                PASS (labeled EXECUTOR / only target writer)
verify-final mismatch:      PASS (rc=3)
verify-final match:         PASS (rc=0)
live preview/approval/apply: NO
target modified:            NO
repo clean:                 YES
note:                       false positive came from static guidance text containing marker strings
```

---

## 3. Source of Truth

```text
Merged v0 (origin/main @ c5986e8):
  scripts/a2-ide-harness.sh                       (print/validate-only helper)
  .vscode/tasks.json                              (8 A2 tasks + 8 Claw tasks)
  docs/runbooks/a2-ide-harness-workflow.md        (operator runbook)
  handoffs/a2_ide_harness_workflow_implementation_report_2026-06-07.md
Exact false-positive source (read-only):
  scripts/a2-ide-harness.sh:344-347  (verify-final "Apply-result evidence to look for" guidance block,
                                      which prints: a2-l2b-apply-result.v1, outcome: applied,
                                      a2-l2b-write-applied, a2-l2b-write-validated)
Operator smoke test report (PASS_WITH_NOTES) — preceding lane.
```

---

## 4. What Worked

```text
- Generator-vs-executor distinction is unmistakable (apply-bundle GENERATOR vs plan apply EXECUTOR).
- Missing-path inputs degrade to WARNINGS while still printing the correct next command (rc=0) — good
  for "show me the next step" usage.
- Commands print shell-quoted, so the space-containing default A2_CLAW path pastes safely.
- Safety logic verified: absolute after_file refused, hash mismatch caught, sensitive-path warned,
  real-terminal approval required (no auto/--yes/fake-TTY).
- No live A2 command runs from the helper or tasks; no exec/eval path.
```

---

## 5. What Was Confusing

```text
- The smoke test's broad accidental-execution grep over free-text logs produced a FALSE POSITIVE:
  verify-final PRINTS marker tokens (a2-l2b-write-applied, outcome: applied, …) as evidence-to-look-for
  guidance, so a free-text grep matches guidance text identically to real execution evidence.
- find-artifacts only has an empty-state hint; it does not tell the operator the precise next subcommand
  once some .claw artifacts exist.
```

---

## 6. Root Cause of False Positive

```text
- `verify-final` guidance (scripts/a2-ide-harness.sh:344-347) intentionally prints marker names so a
  human operator knows what to look for in the real apply output. These names are useful.
- The smoke check searched FREE-TEXT logs for those same marker tokens.
- Therefore guidance text and real execution evidence are indistinguishable to that grep.
- Reliable detection must be ARTIFACT/STATE-based, not text-based:
    * presence of .claw apply-result JSON (a2-l2b-apply-result.v1) under <workspace>/.claw
    * target hash drift (target != before_sha)
    * actual command exit codes / structured logs
    * files newly created under <workspace>/.claw
  Free-text helper output is guidance, not evidence.
```

---

## 7. Recommended Polish

```text
Preferred:
- KEEP the marker names in operator guidance (they help humans verify a real apply).
- Change smoke guidance/tests to check ARTIFACT PATHS and TARGET HASH DRIFT instead of free-text logs.
- Add `find-artifacts` next-step hints (state machine over .claw contents).
- Optionally add a read-only `audit-workspace` / `smoke-check` helper mode that is explicitly
  artifact-aware (no execution).

Avoid:
- removing useful marker guidance just to satisfy a broad grep
- hiding apply marker names from operators
- adding ANY execution mode to the helper
- relying on free-text logs alone to decide whether apply ran
```

---

## 8. Candidate Patch Surface

```text
scripts/a2-ide-harness.sh                         (find-artifacts hints; optional audit-workspace mode)
docs/runbooks/a2-ide-harness-workflow.md          (document the next-step hints / audit mode)
handoffs/a2_ide_harness_v0_ux_polish_implementation_report_2026-06-07.md  (future report)
.vscode/tasks.json                                (ONLY if adding a read-only "A2: Audit Workspace" task)
No Rust. No runtime. No schemas.
```

### `find-artifacts` next-step hints to scope

```text
- no .claw present:                               "Run/print preview first."
- preview-bundle present, approval-result absent: "Next: print approval."
- approval-result present, apply-bundle absent:   "Next: print apply-bundle."
- apply-bundle present, target still before_sha:  "Next: print apply."
- apply-result / write marker present:            "Chain appears applied; verify final target."
All hints are read-only (inspect artifacts + hashes); none execute claw.
```

---

## 9. Non-Goals

```text
- NOT implementing the polish in this lane (scope only).
- NOT removing marker-name guidance.
- NOT adding any execution / auto-approval / apply path.
- NOT editing Rust, schemas, or runtime.
- NOT a full extension/webview (still future v1 per the original scope card).
```

---

## 10. Safety Boundaries

```text
The helper stays PRINT/VALIDATE-ONLY. The polish must preserve:
- Preview/approval/apply-bundle write no target; only `claw plan apply` writes, once.
- No auto-approval, no hidden apply, no batch/--yes/fake-TTY.
- No model/broker/runtime call; no raw :11434 inference.
- Real-terminal approval remains required.
- audit-workspace / find-artifacts hints are read-only; they never run claw or mutate artifacts.
```

---

## 11. Validation Plan

A future implementation must validate (no live A2 commands):

```text
- bash -n scripts/a2-ide-harness.sh
- JSON validation if .vscode/tasks.json changes
- helper `help` smoke
- print modes still print only (no execution)
- find-artifacts next-step hints across states: no .claw; preview only; approval present;
  apply-bundle present; applied marker present
- false-positive detection no longer treats guidance text as execution — detection is artifact/hash-based
- no live preview/approval/apply runs; target unchanged; repo clean
```

---

## 12. STOP Conditions

```text
- live preview / approval / apply requested or run
- helper begins executing `claw plan …`
- auto-approval introduced
- hidden apply introduced
- model / broker / runtime call introduced
- deletion / retirement of artifacts introduced
- Rust / runtime / schema touched
- smoke test still relies on free-text logs alone to decide whether apply ran
```

---

## 13. Future Lanes

```text
1. A2 IDE Harness v0 UX Polish Scope Review / Push PR (this card).
2. A2 IDE Harness v0 UX Polish Scope exact-head merge gate.
3. (implementation) A2 IDE Harness v0 UX Polish — find-artifacts hints + artifact-aware smoke,
   per §7/§8, validated per §11; helper stays print/validate-only.
4. (later, separate) A2 IDE Harness v1 extension/webview (still gated as in the original scope card).
```

---

## 14. Final Recommendation

```text
Proceed with a small, print/validate-only polish: keep the useful marker guidance, make smoke detection
artifact/hash-aware rather than free-text, and add read-only find-artifacts next-step hints (optionally a
read-only audit-workspace mode). No execution mode, no marker-guidance removal, no Rust/runtime change.
Review and merge this scope before implementing.
```
