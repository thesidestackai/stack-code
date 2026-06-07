# A2-L4 IDE / Harness Workflow — Scope (Docs-Only) — 2026-06-07

> Docs-only scope card. This document designs an IDE-style / harness workflow that wraps the
> already-proven A2-L2b CLI chain in a safer, more visual operator surface. It implements NOTHING:
> no extension, no Rust change, no CLI change, no preview/approval/apply run, no model/broker/runtime
> call. It scopes a FUTURE, separately-token-gated implementation lane.

---

## 1. Executive Summary

The A2-L2b CLI chain is functionally complete and proven end-to-end (final evidence merged on `main`):

```text
preview → preview_sha256 → real-TTY approval → persisted approval-result → apply-bundle (generated) → apply (executor) → write-validated target
```

The chain is correct, but its operator surface is terminal-heavy: long commands, manual artifact
paths, an easy-to-omit `--approval-result-output`, and two confusingly-named commands (`apply-bundle`
the *generator* vs `plan apply` the *executor*). The operator is not terminal-first.

This card scopes an **IDE-adjacent harness** that makes the same chain visually understandable —
workspace selection, plan/after_file validation, before/after diff, hash/step_id display, an explicit
approval gate, and a final evidence panel — **without weakening a single safety gate**. The IDE layer
*wraps* the proven CLI; it never replaces it with a shortcut.

**Recommended v0:** an IDE-adjacent harness (VS Code / Cursor workspace tasks + command palette
scripts, a visible `.claw` artifact tree, generated before/after preview files, and a **real terminal
launch** for the TTY-bound approval). A full extension / webview is deferred to **v1**, only after the
command contracts are stable.

---

## 2. Problem Statement

The CLI flow works but the operator is not terminal-first. Observed pain:

```text
- too many long commands
- too much manual path handling (preview bundle, generator result, approval-result, apply-bundle)
- approval-result output path (--approval-result-output) is easy to omit
- command roles are confusing: apply-bundle (generator) vs plan apply (executor)
- hard to visually inspect before/after content
- hard to know what step is next
- real-TTY approval requirement is confusing inside Codex / Claude command runners
  (these are not real interactive TTYs; the approval guard fail-closes there — exit 7)
```

The IDE layer must make the workflow visually understandable without weakening the safety model.

---

## 3. Source of Truth

This scope is grounded in code and merged handoffs read read-only in this lane:

```text
handoffs/a2_l2b_chain_final_evidence_2026-06-06.md          (merged on main — proven chain evidence)
handoffs/a2_disposable_artifact_retirement_scope_2026-06-06.md (merged on main — retirement scope)
origin/main:
  3f25198 docs(a2): record final A2 L2b chain evidence (#89)
  cde1182 docs(a2): scope disposable artifact retirement (#90)
CLI source (read-only):
  rust/crates/rusty-claude-cli/src/main.rs        (plan run / approve / apply-bundle / apply entry points)
  rust/crates/a2-plan-runner/src/approval.rs      (approval grammar: `apply <step-id> <preview_sha256>`)
  rust/crates/a2-plan-runner/src/approval_ux.rs   (operator approval-line UX)
  rust/crates/a2-plan-runner/src/write_executor.rs (apply executor + exit codes)
  rust/crates/rusty-claude-cli/tests/plan_*.rs    (command contract tests)
Canonical hashes from the proven chain:
  preview_sha256 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
  before_sha256  d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b
  after_sha256   8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
```

---

## 4. Current CLI Chain

The proven, target-writing chain (exactly as run in the final evidence handoff):

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json> --approval-result-output <approval-result.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

Command-role clarification (the single most important fix the IDE must surface — see final evidence §9):

```text
claw plan run ... --workspace-write-preview   → PRODUCES the preview bundle + preview_sha256. Writes NO target.
claw plan approve ... --approval-result-output → real-TTY human approval; PERSISTS approval-result. Writes NO target.
claw plan apply-bundle <gen-result> <approval> → GENERATES apply-bundle.json. Writes NO target.
claw plan apply <apply-bundle.json>            → the EXECUTOR. This is the ONLY command that writes the target.
```

> apply-bundle **generates** the bundle. **plan apply** writes the target.

Schemas and markers observed on the wire:

```text
a2-l2b-approval-result.v1            (persisted approval-result)
a2-l2b-apply-bundle.v1               (apply bundle)
a2-l2b-apply-bundle-generator-result.v1 (apply-bundle generator envelope)
a2-l2b-apply-result.v1               (executor result; outcome "applied")
apply markers: a2-l2b-write-preflight-ok, a2-l2b-write-temp-created, a2-l2b-write-applied, a2-l2b-write-validated
approval markers: a2-l2b-diff-preview-ready, a2-l2b-approval-prompt, a2-l2b-approved
```

Relevant exit codes (from `a2-plan-runner`):

```text
0  applied / success                 7  approval denied / preview-ready next-step (EXIT_APPROVAL_DENIED)
5  parse / invalid request           8  rollback failed
6  write path refused                9  checkpoint failed / baseline mismatch
10 write IO failed                  11 validation rolled back
12 status refused
```

---

## 5. Operator UX Pain Points

```text
P1 long commands           : 4 multi-flag commands with long /tmp paths typed by hand.
P2 manual path handling    : operator must thread preview-bundle, generator-result, approval-result,
                             apply-bundle paths between commands; copy/paste errors are likely.
P3 omitted output path     : forgetting --approval-result-output silently loses the persisted approval.
P4 command-role confusion  : apply-bundle (generator) looks like "the apply"; plan apply is the real writer.
P5 no visual diff          : before/after content is only inspectable by manual file reads + sha compare.
P6 no "what's next" cue    : next_operator_command hint exists in stdout but is buried.
P7 TTY confusion in runners: Codex/Claude command runners are not real TTYs; approval fail-closes (exit 7),
                             which reads as a failure to a non-terminal-first operator.
```

---

## 6. IDE / Harness Goals

```text
G1 Make each chain stage operator-visible and labeled.
G2 Eliminate manual path threading: artifacts discovered/passed by the harness, not retyped.
G3 Make the apply-bundle (generator) vs plan apply (executor) distinction unmistakable.
G4 Show before/after diff and all binding hashes (preview_sha256, before_sha256, after_sha256) + step_id.
G5 Keep the human approval an explicit, real action bound to the exact hashes/step/target.
G6 Provide a clear, real-TTY path for approval (no fake-TTY shortcut).
G7 Preserve every A2 safety invariant unchanged (§12).
G8 Show final evidence (apply-result markers + target hash) at the end.
```

---

## 7. Non-Goals

```text
- NOT implementing an IDE extension in this lane (scope only).
- NOT editing Rust, the CLI, or any command behavior.
- NOT running preview / approval / apply / apply-bundle.
- NOT deleting or modifying any artifact or target file.
- NOT calling a model / broker / runtime / Vault / secrets.
- NOT installing or modifying VS Code / Cursor extensions or user IDE settings.
- NOT introducing any auto-approve, batch, --yes, or fake-TTY path.
- NOT replacing the CLI safety gates with an IDE shortcut.
```

---

## 8. Recommended V0 Workflow

**Stack-Code IDE Harness v0 — an IDE-adjacent harness (not an extension):**

```text
- VS Code / Cursor workspace tasks (.vscode/tasks.json) or command-palette-runnable scripts that
  invoke the EXISTING claw commands with prefilled, validated arguments.
- A visible file tree for .claw artifacts (preview bundles, approval-results, apply-bundles, checkpoints).
- Diff visibility: a generated before/after preview file (or the IDE's native diff viewer opened on
  the before/after content) so the operator can read the change before approving.
- A REAL terminal launch for the approval step: the harness opens an integrated terminal with the full
  `claw plan approve … --approval-result-output …` command prefilled; the operator types the approval
  line at a real TTY. The TTY guard is preserved, not emulated.
- A final evidence panel/document showing apply-result markers and the target after_sha256.
```

Operator-visible stages (the IDE surface maps 1:1 to the proven chain):

```text
1.  Select workspace + plan.yaml
2.  Validate plan and after_file
3.  Generate preview          → claw plan run … --workspace-write-preview     (writes NO target)
4.  Show before/after diff
5.  Show preview_sha256 and step_id
6.  Approval gate             → real terminal, human types approval line       (writes NO target)
7.  Persist approval-result   → --approval-result-output <path>
8.  Generate apply bundle     → claw plan apply-bundle <gen-result> <approval> (writes NO target)
9.  Apply once                → claw plan apply <apply-bundle.json>            (THE only target write)
10. Show final evidence and target hash
```

**V1 candidate (deferred):** a proper VS Code / Cursor extension or local panel:

```text
- A2 sidebar / tree view of artifacts
- Preview button
- Diff viewer
- Approval phrase input (bound to step_id + preview_sha256)
- Apply button DISABLED until a validated approval-result exists
- Artifact browser
- Evidence export
Do not build v1 until the v0 command contracts are stable and reviewed.
```

---

## 9. Approval UX Options

The visual layer must still require an explicit human approval action **bound to**:

```text
step_id
preview_sha256
target path
before_sha256
after_sha256
```

Approval-line grammar is fixed by source (`a2-plan-runner/src/approval.rs`, `approval_ux.rs`):

```text
apply <step-id> <preview_sha256>
e.g. apply preview_target_update 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

### Option A — IDE launches real terminal approval  (RECOMMENDED for v0)

```text
- IDE opens a real integrated terminal with the full `claw plan approve … --approval-result-output …`
  command prefilled.
- Operator types the approval line manually at a real TTY.
- Safest short-term path; preserves the current TTY guard exactly (no emulation).
- Lowest implementation risk.
```

### Option B — IDE webview approval input

```text
- Operator types exact `apply <step_id> <preview_sha256>` into a visual input box.
- Backend calls a library/API approval path (NOT a fake terminal / pretend-TTY).
- Requires NEW implementation design (a non-TTY approval entry point that is at least as strict as the
  current real-TTY guard — explicit, single-use, hash-bound, no batch/--yes).
- Must NOT bypass approval semantics. Until such an entry point is designed and proven, prefer Option A.
```

### Option C — IDE task runner only

```text
- IDE provides tasks/commands + documentation; operator still reads/acts in the terminal.
- Lowest implementation risk; least visual.
```

### Option D — Full VS Code / Cursor extension

```text
- Tree view for artifacts, diff viewer, staged workflow buttons.
- Highest UX value; largest implementation surface.
- Defer to v1.
```

**Recommendation:** Adopt **Option A** (real terminal approval) for v0, combined with the Option C
task-runner mechanics for the non-approval stages. Defer Option B and Option D to v1, and gate
Option B behind a dedicated, separately-reviewed non-TTY approval-entry design.

---

## 10. Visual Diff / Evidence Requirements

The harness must make the change and its evidence visible at every stage:

```text
Before/after diff:
  - render the preview's before content vs after_file content in the IDE diff viewer (or a generated
    before/after pair file), strictly READ-ONLY; rendering the diff must NOT write the target.
Binding display (always shown together near the approval gate):
  - target path
  - step_id
  - preview_sha256
  - before_sha256
  - after_sha256
  - the exact approval line: apply <step_id> <preview_sha256>
Artifact visibility:
  - the .claw artifact tree (preview bundle, preview-generator-result, approval-result, apply-bundle,
    checkpoint) shown read-only.
Command-role clarity:
  - clearly label which command writes the target (claw plan apply) vs which only generates (apply-bundle).
Final evidence (stage 10):
  - apply-result schema a2-l2b-apply-result.v1, outcome "applied"
  - markers a2-l2b-write-preflight-ok, a2-l2b-write-temp-created, a2-l2b-write-applied, a2-l2b-write-validated
  - target final hash == after_sha256
```

---

## 11. Command Mapping

IDE stage → exact underlying CLI command (the IDE wraps these; it does not reimplement them):

```text
Stage 1  Select workspace + plan.yaml   → (UI only) choose <workspace-root> and <plan.yaml>
Stage 2  Validate plan + after_file      → (read-only checks; see STOP conditions §17)
Stage 3  Generate preview               → claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
                                           (exit 7 = preview ready; emits next_operator_command)
Stage 4  Before/after diff              → render preview before vs after_file (read-only)
Stage 5  Show preview_sha256 + step_id  → from the preview bundle / generator result
Stage 6  Approval gate                  → REAL terminal: claw plan approve <preview-bundle.json> \
                                              --approval-result-output <approval-result.json>
                                           operator types: apply <step-id> <preview_sha256>
Stage 7  Persist approval-result        → produced by --approval-result-output (schema a2-l2b-approval-result.v1)
Stage 8  Generate apply bundle          → claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
                                           (GENERATOR; schema a2-l2b-apply-bundle.v1; writes NO target)
Stage 9  Apply once                     → claw plan apply <apply-bundle.json>
                                           (EXECUTOR; schema a2-l2b-apply-result.v1; THE only target write)
Stage 10 Final evidence + target hash   → apply-result markers + target == after_sha256
```

---

## 12. Safety Invariants

The IDE must wrap the proven A2 chain. It must NOT replace it with an unsafe shortcut.

```text
Preview does not write target.
Approval does not write target.
Apply-bundle generation does not write target.
Only plan apply writes target.
Apply runs at most once per approved preview.

No auto-approval.
No hidden apply.
No apply without validated approval-result.
No apply if target hash differs from before_sha.
No repeated apply unless explicitly reset in a new proof chain.
No model/broker/runtime calls.
No raw :11434 app inference.
No fake TTY / pipe / script / expect / pty wrapper / --yes / auto / batch for approval.
```

---

## 13. Artifact Model

The harness operates over the existing `.claw` artifact layout (read-only except as written by the CLI):

```text
<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json          (preview evidence)
<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-generator-result.json (preview gen result)
<workspace>/.claw/l2b-approval-results/<step-id>/approval-result.json                  (persisted approval)
<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json             (apply bundle)
<workspace>/.claw/l2b-checkpoints/<run-id>/<step-id>/ (manifest.json + before.bin)     (rollback baseline)
<workspace>/.claw/l2b-payloads/<run-id>/<step-id>/ (after.bin + after.sha256)          (payload)
<workspace>/<target-relative-path>                                                     (the single target write)
```

The harness:

```text
- DISCOVERS these artifacts to thread paths between stages (eliminating manual path handling).
- DISPLAYS them read-only.
- NEVER fabricates or edits an approval-result / apply-bundle; only the CLI produces them.
- treats the checkpoint before.bin as the rollback source of truth (display only).
```

---

## 14. Implementation Surface Options

```text
Option 1: VS Code tasks (.vscode/tasks.json) + markdown/operator panel docs.
          Lowest risk; native to VS Code/Cursor; real-terminal approval natural.
Option 2: repo-local script that prints the next command and opens the relevant files/diff.
          Low risk; editor-agnostic; pairs well with Option 1.
Option 3: local static HTML artifact viewer (read-only) for .claw artifacts + hashes + diff.
          Medium; nice for visibility; must stay strictly read-only.
Option 4: VS Code / Cursor extension or webview (buttons, sidebar, approval input).
          Highest UX, highest surface; defer to v1; Option-B approval requires a vetted non-TTY entry point.
```

---

## 15. Recommended Implementation Path

```text
V0 = Option 1 + Option 2 (+ optional read-only Option 3 viewer):
  - VS Code/Cursor tasks + command-palette scripts that wrap the EXISTING claw commands
  - visible .claw artifact tree + before/after diff (read-only)
  - REAL terminal launch for approval (Option A); no custom extension yet
  - final evidence panel/document
  Rationale: keeps the proven CLI as the only authority, adds visibility, and changes NO safety gate.

V1 = Option 4 (extension/webview) ONLY after v0 command contracts are stable and reviewed:
  - A2 sidebar, preview button, diff viewer, approval-phrase input, apply button (disabled until
    approval-result validates), artifact browser, evidence export.
  - Option B (webview approval input) is allowed in v1 ONLY behind a dedicated, separately-reviewed
    non-TTY approval entry point that is at least as strict as today's real-TTY guard.

Do not jump to a full extension until the workflow is stable.
```

---

## 16. Validation Requirements

A future implementation must demonstrate (without weakening any gate):

```text
- Each operator-visible stage maps to the documented CLI command (§11) and nothing else.
- Preview / approval / apply-bundle stages write NO target (verify target hash unchanged == before_sha
  until stage 9).
- Approval occurs at a real TTY (Option A) or a vetted non-TTY entry point (v1 Option B only);
  no fake-TTY / --yes / batch path exists.
- The apply executes exactly once and produces a2-l2b-apply-result.v1 with outcome "applied" and the
  write markers; target final hash == after_sha256.
- No model / broker / runtime / Vault call is made by the harness.
- The harness never edits .claw approval-results or apply-bundles; the CLI is the sole producer.
```

---

## 17. STOP Conditions

The harness (and any future implementation) must STOP if:

```text
- missing preview bundle
- approval-result missing
- preview hash mismatch (displayed preview_sha256 != bundle)
- target drift (target hash != before_sha256 before apply)
- prior apply marker present (a2-l2b-write-applied) for this preview — already applied
- user attempts to bypass approval (any auto/batch/--yes/fake-TTY)
- command ambiguity (apply-bundle generator vs plan apply executor not disambiguated)
- no real TTY available for the terminal approval path (Option A)
- apply button pressed twice (no repeated apply per approved preview)
- unreviewed after_file
- absolute after_file path
- unsafe target path (outside the workspace / runtime / service / secret path)
- runtime / service / secret path involvement
```

---

## 18. Future Lanes

```text
1. A2 IDE Harness Workflow Scope Review / Push PR (this card).
2. A2 IDE Harness Workflow Scope exact-head merge gate.
3. (token-gated) A2 IDE Harness Workflow Implementation — v0 Option 1/2 build, per the draft
   implementation prompt (handoffs/a2_ide_harness_workflow_implementation_prompt_DRAFT_2026-06-07.md),
   gated by token: APPROVED: Execute A2 IDE harness workflow implementation
4. (later) A2 IDE Harness v1 extension/webview — only after v0 contracts are stable and reviewed.
```

---

## Appendix — Recommendation (V0)

```text
V0 should be an IDE-adjacent harness:
- VS Code/Cursor compatible workspace commands/tasks
- visual artifact/diff instructions
- real terminal launch for approval
- no custom extension until command contracts are stable
Then v1 can be a proper extension/webview.
```
