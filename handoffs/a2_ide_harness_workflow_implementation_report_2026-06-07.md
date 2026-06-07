# A2 IDE Harness Workflow — v0 Implementation Report — 2026-06-07

> Implementation closeout for the v0 IDE-adjacent A2-L2b harness. The build lane ran NO A2
> command (no preview / approval / apply / apply-bundle), made no model/broker/runtime call,
> and modified no target or .claw artifact. v0 is print/validate-only by design.

---

## 1. Approval

```text
Token provided by operator (affirmative, standalone line): 
  APPROVED: Execute A2 IDE harness workflow implementation
Gate: satisfied. Implementation proceeded under the merged scope + DRAFT prompt.
```

---

## 2. Source of Truth

```text
docs/a2-l4-ide-harness-workflow-scope.md                       (merged on main @ 481ccb3)
handoffs/a2_ide_harness_workflow_implementation_prompt_DRAFT_2026-06-07.md (merged on main @ 481ccb3)
```

---

## 3. Implementation Surface

```text
.vscode/tasks.json                              EXTENDED (8 existing "Claw:" tasks preserved;
                                                added 8 "A2:" tasks + 9 new promptString inputs)
scripts/a2-ide-harness.sh                       NEW (executable; print/validate-only helper)
docs/runbooks/a2-ide-harness-workflow.md        NEW (operator runbook)
handoffs/a2_ide_harness_workflow_implementation_report_2026-06-07.md  NEW (this report)
```

Chosen v0 = Option 1 (VS Code/Cursor tasks) + Option 2 (repo-local helper script) + the runbook.
Not a full extension: the existing `ide/vscode/claw-status-panel/` extension was left untouched;
a richer panel/webview remains future v1, per the scope card, until the command contracts are
exercised and stable.

---

## 4. What the Helper Does

```text
Subcommands (all read-only / print-only):
  help
  validate-input    --workspace --plan       (read-only checks; refuses absolute after_file)
  print-preview     --workspace --plan       (prints STEP 1; writes no target)
  find-artifacts    --workspace              (locates .claw artifacts; shows sha256)
  print-approval    --workspace --preview-bundle --approval-output  (prints STEP 2; real-TTY)
  print-apply-bundle --preview-generator-result --approval-result   (prints STEP 3; generator)
  print-apply       --apply-bundle           (prints STEP 4; the only target writer)
  verify-final      --workspace --target --after-sha  (read-only hash check)

It NEVER executes `claw`. There is no exec mode (by design). It calls no model/broker/runtime.
A2_CLAW overrides the printed binary path (default: the dated build artifact); paths are printed
shell-quoted so they paste safely even with spaces.
```

---

## 5. Safety Boundaries Preserved

```text
Preview does not write target.            (helper only prints the command)
Approval does not write target.           (helper only prints; real-TTY required; grammar shown)
apply-bundle generation does not write.   (labeled GENERATOR)
Only `claw plan apply` writes the target. (labeled EXECUTOR; "run once" warning)
No auto-approval / no hidden apply / no batch / no --yes / no fake-TTY.
No model / broker / runtime call. No raw :11434 inference.
Absolute after_file refused; runtime/service/secret-looking paths warned.
```

---

## 6. Validation Results

```text
scope guard:                exactly 3 working-tree changes (+1 new dir): .vscode/tasks.json (M),
                            scripts/a2-ide-harness.sh (new), docs/runbooks/a2-ide-harness-workflow.md (new),
                            plus this report (new) → 4 files staged at commit
forbidden surface guard:    NO_FORBIDDEN_SURFACE (no rust/src/tests/schemas/runtime/*.rs/*.ts/*.tsx)
json validation:            python3 -m json.tool .vscode/tasks.json → valid
bash syntax:                bash -n scripts/a2-ide-harness.sh → OK
no-direct-A2-execution:     confirmed — no exec/eval; no bare $A2_CLAW invocation; `claw plan …`
                            appears only in comments/help/docs/printed strings, never executed
help smoke:                 OK
print-mode smoke:           print-preview + print-apply → rc=0, correctly quoted commands printed
functional smoke:           validate-input relative after_file → rc=0 (OK);
                            validate-input absolute after_file → rc=3 (refused + sensitive warning);
                            verify-final hash mismatch → rc=3 (detected)
git diff --check:           clean
```

---

## 7. Build-Lane Safety Attestation

```text
preview run:            NO
approval run:           NO
apply-bundle run:       NO
apply run:              NO
model/broker call:      NO
runtime touched:        NO
target modified:        NO
artifacts modified/deleted: NO (only a self-created /tmp test fixture was removed by explicit path)
auto-approval:          NO
hidden apply:           NO
source/Rust modified:   NO
```

---

## 8. Remaining / Future v1

```text
- Full VS Code/Cursor extension or webview panel (A2 sidebar, diff viewer, evidence export).
- Approval-phrase input bound to a vetted non-TTY approval entry point (Option B), at least as
  strict as today's real-TTY guard — only after that entry point is designed and reviewed.
- Apply button disabled until the approval-result validates.
Defer all of the above until v0 command contracts are exercised end-to-end and stable.
```

---

## 9. Next Lane

```text
A2 IDE Harness Workflow v0 Review / Push PR (docs + tasks + helper script).
Do not run the live A2 workflow from the build lane; live exercise is operator-driven at a real TTY.
```
