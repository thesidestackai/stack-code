# A2 IDE Harness Workflow — Implementation Prompt (DRAFT, Token-Gated) — 2026-06-07

> DRAFT future-execution prompt. This is NOT authorized to run. It builds the v0 IDE-adjacent harness
> scoped in `docs/a2-l4-ide-harness-workflow-scope.md`. It must not run until the scope card and this
> prompt are reviewed and merged, and the exact approval token below is present in the invocation.

---

## 1. Status and Approval Requirement

```text
Status:     DRAFT — not authorized.
Gate:       This implementation lane MUST NOT proceed unless the operator includes the EXACT token:

            APPROVED: Execute A2 IDE harness workflow implementation

            If the token is absent, STOP immediately and do nothing beyond reporting "token missing".
Precondition: docs/a2-l4-ide-harness-workflow-scope.md is reviewed and merged on origin/main.
```

---

## 2. Role

```text
You are a careful Stack-Code product/workflow engineer building a v0 IDE-adjacent harness that WRAPS
the proven A2-L2b CLI chain in a safer, more visual operator surface. You add visibility and remove
manual path handling; you NEVER weaken a safety gate and you NEVER reimplement the CLI's authority.
Follow: OBSERVE → VERIFY → DISCOVER → DESIGN → DRAFT → VALIDATE → COMMIT → REPORT.
```

---

## 3. Objective

```text
Build v0 of the IDE harness per docs/a2-l4-ide-harness-workflow-scope.md §8/§15:
- VS Code/Cursor workspace tasks + command-palette scripts wrapping the EXISTING claw commands
- visible .claw artifact tree + read-only before/after diff
- REAL terminal launch for the TTY-bound approval (Option A)
- final evidence panel/document (apply-result markers + target after_sha256)
Do NOT build a full extension/webview (that is v1). Do NOT change the CLI or Rust.
```

---

## 4. Source of Truth

```text
docs/a2-l4-ide-harness-workflow-scope.md                          (the v0 scope — authoritative)
handoffs/a2_l2b_chain_final_evidence_2026-06-06.md                (proven chain + command roles §9)
handoffs/a2_disposable_artifact_retirement_scope_2026-06-06.md    (artifact inventory)
CLI command contracts (read-only):
  rust/crates/rusty-claude-cli/src/main.rs
  rust/crates/a2-plan-runner/src/approval.rs       (grammar: apply <step-id> <preview_sha256>)
  rust/crates/a2-plan-runner/src/approval_ux.rs
  rust/crates/a2-plan-runner/src/write_executor.rs (exit codes)
  rust/crates/rusty-claude-cli/tests/plan_*.rs
Command roles (do not confuse):
  claw plan run … --workspace-write-preview   → produces preview; writes NO target
  claw plan approve … --approval-result-output → real-TTY approval; persists approval-result; writes NO target
  claw plan apply-bundle <gen-result> <approval> → GENERATOR; writes NO target
  claw plan apply <apply-bundle.json>          → EXECUTOR; THE only target write
```

---

## 5. Hard Boundaries

Do NOT:

```text
edit Rust code
edit CLI behavior / command grammar
add any auto-approve / batch / --yes / fake-TTY / pty / expect / script approval path
call a model / broker / runtime / Vault / secrets
use raw localhost:11434 app inference
install or modify VS Code / Cursor extensions or user IDE settings (v0 is tasks/scripts only)
write outside the repo's designated harness location agreed in discovery
push / open PR / merge without separate approval
```

Do NOT run destructive commands:

```bash
git clean
rm -rf
find ... -delete
find ... -exec rm
git reset --hard
git add .
git add -A
git branch -D
git worktree remove --force
git fetch --prune
```

A2-action boundary during build (see §12): do NOT run preview / approval / apply / apply-bundle as
part of building the harness. Wiring is verified with fixtures and dry inspection, not live A2 runs.

---

## 6. Clean Worktree Setup

```text
- Use a FRESH isolated Stack-Code worktree from current origin/main.
- Do NOT edit /home/suki/stack-code (control checkout only).
- Branch:   feat/a2-ide-harness-workflow-v0-<YYYYMMDD>
- Worktree: /mnt/vast-data/git-worktrees/stack-code-a2-ide-harness-workflow-v0-<YYYYMMDD>
- Preflight: control checkout clean; origin/main verifiable; scope card present on origin/main;
  branch/worktree paths free. STOP on any unexpected tracked/staged change.
```

---

## 7. Implementation Discovery

Before choosing technology, DISCOVER current repo structure (read-only) and pick the least-risk v0:

```text
- Is there an existing .vscode/ or task config? Existing scripts/ conventions? A docs/ runbook style?
- Where do harness scripts belong (scripts/ vs a new tools/ dir)? Match existing conventions.
- Confirm the exact claw command grammar + flags from main.rs and tests (do not assume).
- Confirm the .claw artifact layout from the final evidence handoff (§11/§13 of the scope card).
Do NOT decide final technology blindly. Discover, then choose.
```

---

## 8. V0 Implementation Scope

Preferred v0 implementation options to EVALUATE during discovery:

```text
Option 1: VS Code tasks (.vscode/tasks.json) + markdown/operator panel docs.
Option 2: repo-local script that prints the next command and opens the relevant files/diff.
Option 3: local static HTML artifact viewer (read-only).
Option 4: VS Code extension/webview.
```

Default recommendation:

```text
Start with Option 1 or 2 (optionally a read-only Option 3 viewer).
Do NOT jump to a full extension (Option 4) until the workflow is stable.
```

Deliver the 10 operator-visible stages from the scope card (§8) as visible, labeled steps:

```text
1 select workspace+plan → 2 validate → 3 generate preview → 4 before/after diff → 5 show preview_sha256+step_id
→ 6 approval gate (real terminal) → 7 persist approval-result → 8 generate apply-bundle → 9 apply once
→ 10 final evidence + target hash
```

---

## 9. Command Mapping

The harness wraps (never reimplements) these exact commands:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json> --approval-result-output <approval-result.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>   (GENERATOR — writes NO target)
claw plan apply <apply-bundle.json>                                             (EXECUTOR — the only target write)
Approval line grammar (fixed): apply <step-id> <preview_sha256>
The UI must clearly label which command writes the target (claw plan apply) vs generates (apply-bundle).
```

---

## 10. Approval UX Boundary

```text
v0 uses Option A (real terminal approval):
  - the harness opens a REAL integrated terminal with the full `claw plan approve … --approval-result-output …`
    prefilled; the operator types `apply <step-id> <preview_sha256>` at a real TTY.
  - the TTY guard is preserved, NOT emulated. No fake-TTY/pty/expect/script/--yes/auto/batch.
The approval action must be bound to and DISPLAY: step_id, preview_sha256, target path, before_sha256, after_sha256.
Option B (webview approval input) is OUT OF SCOPE for v0; it is allowed in v1 only behind a dedicated,
separately-reviewed non-TTY approval entry point at least as strict as the current real-TTY guard.
```

---

## 11. Validation Plan

```text
- Each stage maps to the documented CLI command (scope §11) and nothing else.
- Fixture/dry verification that preview/approval/apply-bundle stages write NO target (target hash
  unchanged == before_sha until apply).
- Approval path uses a real TTY only (no fake-TTY/--yes/batch anywhere in the harness).
- A documented dry run (against a disposable workspace, OPERATOR-DRIVEN, see §12) shows apply executes
  exactly once → a2-l2b-apply-result.v1 outcome "applied" + write markers + target == after_sha256.
- No model/broker/runtime/Vault call originates from the harness.
- The harness never fabricates/edits .claw approval-results or apply-bundles.
- Tests follow repo conventions discovered in §7 (e.g. CI runner expectations).
```

---

## 12. No-A2-Action Boundary During Build

```text
While BUILDING the harness, the implementer must NOT itself run preview/approval/apply/apply-bundle.
- Harness wiring is verified with fixtures, recorded artifacts, and read-only inspection.
- Any live A2 chain exercise is a SEPARATE, OPERATOR-DRIVEN validation step at a real TTY against a
  disposable /tmp workspace — never run autonomously by the build lane.
- The build lane writes code/docs/config only; it does not write any A2 target file.
```

---

## 13. Future E2E Test Plan

```text
- A disposable /tmp workspace + plan.yaml + reviewed after_file fixture.
- Operator-driven, real-TTY walk of all 10 stages via the harness.
- Assert: preview_sha256 stable across stages; target unchanged until apply; exactly one apply;
  apply-result markers present; target final hash == after_sha256; no model/broker/runtime touch.
- Negative cases (must STOP): missing preview bundle, missing approval-result, hash mismatch, target
  drift, prior apply marker, non-TTY approval attempt, double apply, unreviewed/absolute after_file,
  unsafe target path.
- Archive evidence per the retirement scope's archive-before-delete rules; delete nothing in-lane.
```

---

## 14. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_IDE_HARNESS_WORKFLOW_V0_IMPLEMENTATION
TOKEN PRESENT: yes/no (must be yes to proceed)
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
DISCOVERY: repo conventions chosen / option selected (1/2/3) / command grammar confirmed:
IMPLEMENTATION: stages delivered / command mapping / approval option (A) / artifact tree / diff / evidence panel:
SAFETY: preview-no-write / approval-no-write / apply-bundle-no-write / single apply / real-TTY only /
        no model-broker-runtime / no fake-TTY / no source or CLI change:
VALIDATION: stage-map / no-target-write-pre-apply / single-apply / markers+hash / test conventions:
NO-A2-ACTION-DURING-BUILD: confirmed (no preview/approval/apply/apply-bundle run by build lane):
STOP GATES HIT: none | details
NEXT BEST LANE: name / objective / tool / why / touched surfaces / mutation risk / STOP gate / first command
```
