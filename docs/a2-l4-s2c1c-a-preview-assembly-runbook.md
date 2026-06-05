# A2-L4-S2C-1c-a Runbook — Preview Assembly (S2C-1b skeleton → ready-to-preview A2-L2b input) (Docs-Only)

> Status: **DOCS-ONLY OPERATOR RUNBOOK — NOT AN EXECUTION LANE.** This runbook describes the
> safe **manual** steps to turn a merged S2C-1b `workspace_write_preview_request` skeleton plus
> operator-reviewed `after_file` bytes into a **ready-to-preview** A2-L2b input (`plan.yaml` +
> `--workspace-root`). It **stops before** `claw plan run --workspace-write-preview`. It runs no
> preview/approve/apply, computes no `preview_sha256`, invents no `after_file` bytes, edits no
> transform/runner/CLI/schema code, calls no model or broker, and mutates no runtime.
> Created 2026-06-05.
>
> Docs location note: Stack-Code keeps these flat under `docs/` (e.g.
> `docs/a2-l4-s2c1c-preview-path-integration-scope.md`), so this runbook lands at
> `docs/a2-l4-s2c1c-a-…` to match convention.

---

## 1. Executive Summary

S2C-1b (merged, PR #77 `97569a7`) emits a **non-approvable** `workspace_write_preview_request`
skeleton whose `.plan` already matches the `a2-plan-schema` shape, but which is **not yet
runnable**: it is JSON (A2-L2b takes a plan **YAML** file), its `after_file` is a **path
placeholder with no materialized bytes**, and it carries `preview_sha256: null`. S2C-1c (merged,
PR #78 `d609daa`) scoped the integration boundary.

This runbook is the S2C-1c **Option 1** first slice: the **operator** manually (1) validates the
skeleton's non-approvable envelope, (2) materializes and reviews the exact `after_file` bytes,
(3) chooses the `--workspace-root`, and (4) serializes `skeleton.plan` → `plan.yaml`. The result
is a **ready-to-preview input** — and the runbook **STOPS there**. Running the existing A2-L2b
`--workspace-write-preview` (which alone produces the `PreviewRecord` + `preview_sha256`) is a
separate, separately-approved lane (S2C-1d).

---

## 2. Status and Scope

**DOCS-ONLY OPERATOR RUNBOOK — NOT AN EXECUTION LANE.**

In scope: the manual, safe operator procedure to assemble a ready-to-preview A2-L2b input from a
S2C-1b skeleton + operator-reviewed after-bytes, with the preconditions, the skeleton/after-file
checks, the `plan.yaml` assembly, the ready-to-preview checklist, the hard STOP before preview,
and the rejection conditions.

Out of scope (this runbook authorizes none of these): running `claw plan run`/preview/approve/
apply or any A2 apply; computing or fabricating a `preview_sha256`/`PreviewRecord`; inventing
`after_file` bytes or a `write_target`; editing transform/runner/CLI/schema/adapter code;
model/broker calls; runtime/service/Vault mutation; raw `localhost:11434` app inference.

---

## 3. Source of Truth

```text
docs/a2-l4-s2c1c-preview-path-integration-scope.md   (merged S2C-1c design; §6 gap, §8 assembly, §13 STOPs)
scripts/transform_write_previewable.py               (merged S2C-1b skeleton shape)
tests/a2_l4/test_transform_write_previewable.py       (skeleton invariants)
docs/a2-plan-schema.md                                (a2-plan-schema plan.yaml + L2a path/after_file rules)
docs/a2-l2b-run-plan-preview-operator-handoff.md      (existing preview→approve→apply chain + exit codes)
```

---

## 4. Inputs

The operator must have, before assembly:

```text
- the S2C-1b skeleton JSON (artifact_type == "workspace_write_preview_request")
- the chosen workspace root (the directory the write is scoped to)
- the EXACT after_file bytes, materialized and REVIEWED by the operator
- the target path (write_target.path) — workspace-relative, operator-confirmed
- the expected_post_write expectations (must_contain / must_not_contain), operator-reviewed
```

---

## 5. Output

The runbook produces **only**:

```text
- a ready-to-preview plan.yaml (an a2-plan-schema workspace-write plan)
- a ready-to-preview checklist (all preconditions satisfied)
- an operator handoff summary (workspace-root + plan.yaml + materialized after_file location)
```

It MUST NOT produce: a `PreviewRecord`; a `preview_sha256`; an approval; an apply; or any write
to the live `write_target` (the live target is only written later, by A2 apply, in a separate lane).

---

## 6. Non-Goals

```text
This runbook does not run claw plan preview.
This runbook does not run approve.
This runbook does not run apply.
This runbook does not compute preview_sha256.
This runbook does not fabricate preview_sha256.
This runbook does not invent after_file bytes.
This runbook does not grant the S2C-1b transform preview/approve/apply authority.
```

---

## 7. Operator Preconditions

```text
- The S2C-1b transform (PR #77) and S2C-1c scope card (PR #78) are merged on origin/main.
- The operator has a S2C-1b skeleton produced by scripts/transform_write_previewable.py
  (exit 0, WORKSPACE_WRITE_PREVIEWABLE).
- The operator understands that A2-L2b alone computes preview_sha256, and that approval/apply are
  later, separate, gated lanes.
- The operator can materialize and review the exact after_file bytes out-of-band.
```

---

## 8. Workspace Root Selection

```text
- Choose the workspace root the write is scoped to (A2-L2b's --workspace-root).
- write_target.path and after_file are interpreted relative to this root.
- The root must be a real, intended workspace (for proving the flow, a disposable temp workspace
  is recommended, mirroring the A2-L2b smoke evidence).
- The root must NOT be a runtime/service tree, Vault, or a secrets store.
- Do not point the root at /home/suki/stack-code or /home/suki/sidestackai for trial runs; use a
  disposable workspace.
```

---

## 9. S2C-1b Skeleton Validation

Confirm, by reading the skeleton JSON (do not edit it):

```text
artifact_type           == "workspace_write_preview_request"
approval_allowed        == false
apply_allowed           == false
workspace_write_preview == false
preview_sha256          == null            (must be null; never a 64-hex value)
plan.mode               == "workspace-write"
plan.steps has EXACTLY ONE step, with:
  mode  == "workspace-write"
  tools == ["Write"]
  write_target.path present, workspace-relative, lexically safe
  after_file present, workspace-relative, lexically safe, != write_target.path
  expected_post_write present
```

If any check fails, REJECT (see §16). Do not "fix" the skeleton by hand.

---

## 10. After-File Materialization

```text
- The OPERATOR materializes the exact after_file bytes at skeleton.plan.steps[0].after_file
  (relative to the chosen workspace root).
- The bytes ARE the exact intended post-write content of write_target. They are operator-authored.
- No model-generated bytes may be used without explicit operator review.
- No component (transform, assembler, runbook) invents or auto-fills the bytes.
- after_file must be bounded in size (keep it small, single-file).
- after_file path must be workspace-relative and L2a-safe: no absolute, no `..`, and not under
  `.git`/`.claw`/`.claude`, and the final component must not match `.env*`/`secret*`/
  `credentials*`/`*.pem`/`*.key`. (Note: a2-l2a denies `.claw` unconditionally, so materialize
  under a non-denied path such as `materialized/...`.)
- after_file must NOT target a runtime/service/secret path.
```

---

## 11. After-File Review

```text
- The operator reviews the exact materialized after_file bytes before assembly.
- The operator confirms the bytes match the intended edit described by the skeleton's patch_intent
  summary and the step description.
- The operator confirms the bytes contain no secrets/tokens/keys and no raw localhost:11434
  endpoint.
- If the operator cannot confirm the exact bytes, STOP — do not assemble a ready-to-preview input.
```

---

## 12. Plan YAML Assembly

Serialize `skeleton.plan` into an `a2-plan-schema` `plan.yaml` (a pure, offline transcription —
field names already align 1:1; read no after_file bytes; compute no hash):

```yaml
name: <skeleton.plan.name>
mode: workspace-write
model_tier: FAST
steps:
  - id: <skeleton.plan.steps[0].id>            # e.g. s1
    description: <skeleton.plan.steps[0].description>   # inert, human-readable
    mode: workspace-write
    tools: [Write]
    write_target:
      path: <skeleton write_target.path>       # workspace-relative, operator-confirmed
      create_if_absent: <skeleton write_target.create_if_absent>   # e.g. false
    after_file: <skeleton after_file>          # the operator-materialized, reviewed path
    expected_post_write:
      must_contain: <skeleton expected_post_write.must_contain>
      must_not_contain: <skeleton expected_post_write.must_not_contain>
```

Assembly rules:

```text
- exactly one workspace-write step
- model_tier must be FAST (a2-plan-schema refuses DEEP)
- write_target and after_file are copied verbatim from the validated skeleton
- do NOT add extra steps, tools, or fields
- do NOT run any command; this step only writes a plan.yaml text file
```

---

## 13. Ready-to-Preview Checklist

All must be TRUE before handoff:

```text
[ ] skeleton envelope validated (approval_allowed/apply_allowed/workspace_write_preview=false,
    preview_sha256=null)  — §9
[ ] exactly one workspace-write step (tools:[Write], write_target, after_file, expected_post_write)
[ ] after_file bytes MATERIALIZED at the after_file path                                    — §10
[ ] after_file bytes REVIEWED by the operator (no secrets, no :11434, matches intent)        — §11
[ ] write_target and after_file are workspace-relative, L2a-safe, and after_file != write_target
[ ] workspace root chosen and is not a runtime/service/secret tree                           — §8
[ ] plan.yaml assembled (one step, FAST, verbatim fields)                                    — §12
[ ] NO preview_sha256 computed; NO PreviewRecord created; NO approve/apply performed
```

---

## 14. STOP Before Preview

When the checklist in §13 is fully satisfied, emit and honor this exact STOP:

```text
STOP BEFORE A2-L2B PREVIEW:
A ready-to-preview plan.yaml has been assembled.
No PreviewRecord has been created.
No preview_sha256 has been computed.
No approve/apply has occurred.
Proceed to a separate approved preview execution lane only.
```

The runbook ends here. Do not run preview from this lane.

---

## 15. Preview Lane Handoff

The ready-to-preview artifacts are handed to a **separate, separately-approved** preview execution
lane (S2C-1d). For reference only — **this runbook does not run it** — that lane runs the existing,
runtime-proven A2-L2b command:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

Notes for the future preview lane (informational, not authorized here): `--workspace-write-preview`
halts after writing preview artifacts (exit `7` = `write_preview_ready`, a success state). **A2-L2b
alone** reads the `after_file` bytes, builds the `PreviewRecord`, and computes the `preview_sha256`
(`payload_sha256`/`before_sha256`/`after_sha256`). Approval (TTY `apply <step_id> <preview_sha256>`)
and apply remain further separate, gated lanes.

---

## 16. Rejection Conditions

Reject (do not assemble / do not hand off) if:

```text
the skeleton has approval_allowed=true
the skeleton has apply_allowed=true
the skeleton has a non-null preview_sha256
the plan has more than one write step
after_file is missing (no materialized bytes)
after_file is unreviewed by the operator
after_file is an absolute path (or otherwise not allowed by L2a path policy)
the target is outside the workspace (`..` escape / non-relative)
write_target or after_file is a runtime/service/secret path (or an L2a-denied path)
the skeleton, after_file, or plan references a raw localhost:11434 endpoint
approve or apply is requested
preview execution is requested inside this runbook
```

A rejection is an escalation to the operator, never a reframed retry of the forbidden action.

---

## 17. Validation Plan

This runbook is validated (in review) by confirming:

```text
it is docs-only (no scripts/tests/schemas/rust/src/runtime changes)
it authorizes no preview/approve/apply and computes no preview_sha256
it requires operator-materialized + operator-reviewed after_file bytes (never invented)
it preserves A2-L2b as the sole PreviewRecord/preview_sha256 authority
it contains the exact STOP-before-preview message (§14)
raw :11434 appears only as a rejection/prohibition pattern
the assembly step is a pure offline transcription that reads no after_file bytes
```

A future S2C-1c-b (offline assembler, separately scoped) may automate §12; if built, it must be
fixtures-only, stdlib `unittest` (no pytest), read no after_file bytes, and compute no hash.

---

## 18. Follow-On Lanes

```text
1. Read-only review of THIS runbook, then push/merge (docs-only).
2. S2C-1c-a runbook exact-head merge gate.
3. S2C-1d A2-L2b Preview Execution Scope / Prompt Draft (preview-only; still no approve/apply).
4. S2C-1d Preview Execution Approval Gate (run the existing --workspace-write-preview; obtain
   PreviewRecord + preview_sha256).
5. S2D Approval Gate Scope (operator approval binds preview_sha256).
6. S2E Apply Gate Scope (A2 apply re-validates the chain and writes exactly one file).

Do NOT run preview/approve/apply until this runbook is reviewed and merged and the preview
execution lane is separately scoped and approved.
```

---

## 19. Status Block

```text
status: DOCS-ONLY OPERATOR RUNBOOK — NOT AUTHORIZED TO PREVIEW/APPROVE/APPLY

This runbook authorizes manual preview-assembly only, up to a ready-to-preview input.
It does not run claw plan run/preview/approve/apply or any A2 apply.
It does not compute or fabricate a preview_sha256, and never creates a PreviewRecord.
It does not invent after_file bytes; the operator materializes and reviews them.
It does not grant the S2C-1b transform preview/approve/apply authority.
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It does not introduce a raw localhost:11434 app-inference path.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write authority.
The model proposes; the transform prepares a non-approvable skeleton; the operator materializes and
reviews the after-bytes and assembles plan.yaml; A2-L2b previews; the operator approves; A2 applies.
Next gate: read-only operator review of this runbook.
```
