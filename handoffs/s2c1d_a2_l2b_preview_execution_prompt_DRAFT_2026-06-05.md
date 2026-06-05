# DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL

⚠️ REVIEW REQUIRED: This is a **future preview-execution prompt**, authored 2026-06-05 by the
S2C-1d prompt-drafting lane. It has **not** been run. It is the **first** lane permitted to run the
**existing** A2-L2b workspace-write preview command — **preview only** — and only with the exact
operator token below. It must be reviewed and merged, then invoked only with that token, before any
preview command is run.

Implements: `docs/a2-l4-s2c1d-preview-execution-scope.md` (merged PR #80,
`8f3f447cf6df1d2ce4c15910cec8ab0d14bce856`), operationalizing its §4–§14. Inputs are produced per
the merged S2C-1c-a runbook (`docs/a2-l4-s2c1c-a-preview-assembly-runbook.md`, PR #79 `e91d925`).

> Convention note: Stack-Code keeps handoff-style execution prompts under `handoffs/` (e.g.
> `s2c1a_…_DRAFT_2026-06-04.md`, `s2c1b_…_DRAFT_2026-06-05.md`). This draft follows that convention;
> a reviewer may relocate it to `docs/` if the repo prefers a single docs tree.

---

# CLAUDE CODE PROMPT — S2C-1d A2-L2b Preview Execution (Preview-Only)

## 1. Status and Approval Requirement

Runs the **existing** A2-L2b workspace-write preview on a ready-to-preview input to obtain a
`PreviewRecord` + `preview_sha256`. **Preview only** — it never approves or applies, never modifies
the live `write_target`, never calls a model or the broker, never fabricates a hash.

Do **not** begin unless the operator has provided this **exact** token in the current instruction:

```text
APPROVED: Execute S2C-1d A2-L2b preview execution
```

If that exact token is missing, STOP immediately and report:

```text
BLOCKED: missing required approval token.
```

This prompt is DRAFT ONLY until reviewed and merged. Approval is mandatory and never optional.

## 2. Role

You are a careful Stack-Code A2-L2b preview operator. Follow:
OBSERVE → VERIFY → STOP-GATE → RUN PREVIEW ONLY → CAPTURE → VALIDATE → REPORT.

## 3. Objective

Run **exactly one** preview command:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

to produce, via the existing A2-L2b runner, a `PreviewRecord` and the approval-binding
`preview_sha256`, plus write-preview-ready evidence — and STOP. Do **not** approve or apply.

## 4. Source of Truth

```text
docs/a2-l4-s2c1d-preview-execution-scope.md           (merged scope; §7 command, §8 token, §9 outputs, §11 validation)
docs/a2-l4-s2c1c-a-preview-assembly-runbook.md         (how the ready-to-preview input was assembled)
docs/a2-l2b-run-plan-preview-operator-handoff.md       (runtime-proven chain; exit codes; artifact layout)
docs/a2-plan-schema.md                                 (a2-plan-schema plan.yaml + L2a path rules)
scripts/transform_write_previewable.py                 (S2C-1b skeleton — non-approvable, preview_sha256=null; reference only)
```

## 5. Hard Boundaries

The execution MUST NOT:

```text
run claw plan approve / approve, or claw plan apply / apply, or A2 apply
emit an apply bundle
modify the live write_target file (preview writes only its own .claw/ artifacts)
compute or fabricate a preview_sha256 / PreviewRecord (the existing runner produces them)
invent or alter after_file bytes
call a model or the broker (:11435), or reference raw :11434 as an app-inference route
load/switch/evict models, clear VRAM, or restart a service
edit rust/ runner/CLI/schema or the broker adapter, or any repo source
point --workspace-root at /home/suki/stack-code or /home/suki/sidestackai (use the operator's
  intended workspace; a disposable temp workspace is recommended, mirroring the A2-L2b smoke evidence)
run more than one preview command
```

Allowed: verify inputs; print the STOP message; run exactly one `--workspace-write-preview`
command; read the artifacts it wrote; validate; report. LAW 1: no app inference; raw `:11434` may
appear only as a rejection pattern.

## 6. Clean Worktree / Workspace Preflight

```text
operate from a clean state; do not reconcile dirty checkouts
the preview workspace (--workspace-root) is the operator's intended workspace, NOT a control checkout
confirm no unexpected staged/tracked changes in any repo you touch read-only
```

## 7. Input Artifact Requirements

Require, from the S2C-1c-a assembly:

```text
- a ready-to-preview plan.yaml (a2-plan-schema, one workspace-write step)
- the workspace root (--workspace-root)
- the materialized after_file bytes (at the plan's after_file path, under the root)
- the S2C-1c-a ready-to-preview checklist (all items satisfied)
- the operator handoff summary
- explicit evidence that the after_file bytes were operator-reviewed (no secrets, no :11434, matches intent)
```

## 8. Ready-to-Preview Validation (Preflight)

Verify ALL before the STOP gate (else STOP — see §15):

```text
the exact approval token (§1) is present
plan.yaml exists and is a valid a2-plan-schema workspace-write plan
the workspace root exists (and is not a control checkout / runtime / service tree)
the after_file path exists and its bytes are materialized
the after_file bytes were operator-reviewed
the plan has EXACTLY ONE workspace-write step (tools:[Write], write_target, after_file != write_target)
the write_target path is workspace-relative and L2a-safe (no absolute, no `..`, not under
  .git/.claw/.claude; final component not .env*/secret*/credentials*/*.pem/*.key)
the write_target is not a runtime/service/secret path
the CURRENT live write_target state is captured (record its sha256 / absence) BEFORE preview
no prior approve/apply has occurred
no upstream preview_sha256 is present (the input does not already claim one)
no raw :11434 reference appears in plan/after_file
no model/broker call is required
```

## 9. STOP Before Preview

Print this **exact** message, then proceed only with the operator's explicit go-ahead:

```text
STOP BEFORE A2-L2B PREVIEW EXECUTION:
About to run the existing A2-L2b workspace-write preview command.
This will create PreviewRecord and preview_sha256.
It will not approve or apply.
Proceed only with explicit operator approval for preview execution.
```

## 10. Execute A2-L2b Preview Only

Run **no more than one** preview command:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

Do not approve. Do not apply. Do not run any other plan subcommand. Do not retry the command
(retries are allowed only for **reading** already-written artifacts, never for re-running preview).

## 11. Preview Output Capture

Capture and record:

```text
- the exact command run
- the exit code
- stdout / stderr (sanitized; no secrets)
- generated artifact paths under <workspace>/.claw/ (e.g. l2b-preview-bundles/<run-id>/<step-id>/
  preview-bundle.json + preview-generator-result.json; l2b-payloads/.../after.bin + after.sha256;
  l2b-checkpoints/.../manifest.json)
- the PreviewRecord location
- the preview_sha256 (and its payload_sha256 / before_sha256 / after_sha256 components)
- the write-preview-ready outcome
```

Exit-code note: preview-ready is exit `7` (`write_preview_ready`), but exit `7` is **overloaded**
with approval-denied. Disambiguate by the structured `status` / `outcome` fields and the presence of
the preview artifacts — never assume success from the bare code. If any exit code / status is
unclear, STOP and report (do not guess).

## 12. Post-Preview Validation

Prove ALL:

```text
a PreviewRecord exists
a preview_sha256 exists
the live write_target file was NOT modified (re-read its sha256 / absence; it must equal the §8
  pre-preview capture)
no approve occurred
no apply occurred
no model/broker call occurred
runtime untouched
the preview output is operator-reviewable
no apply token / apply bundle was generated or implied by the preview step
no secrets appeared in output or logs
```

## 13. Approval Boundary

```text
Preview is NOT approval. Producing a preview_sha256 grants no apply authority.
Approval is a later, SEPARATE lane (S2D): `claw plan approve <preview-bundle.json>`, TTY-enforced,
  with the approval line `apply <step_id> <preview_sha256>`, binding the exact A2-L2b preview_sha256.
This lane never approves.
```

## 14. Apply Boundary

```text
Apply is a later, SEPARATE lane (S2E): `claw plan apply <apply-bundle.json>`.
Apply re-verifies payload_sha256 / before_sha256 / after_sha256, atomically replaces exactly one
  file, and fails closed with rollback on any mismatch.
The apply-bundle is produced ONLY by `claw plan apply-bundle` (never hand-authored).
This lane never applies and never emits an apply bundle.
A2 remains the only write path; the model proposes, the operator approves, A2 applies.
```

## 15. Failure Handling

STOP — escalate, never reframe — and run no preview (or halt after preview) if:

```text
the exact approval token is absent
plan.yaml is missing
the workspace root is missing
after_file is missing (no materialized bytes)
after_file was not operator-reviewed
the plan has more than one write step
the write_target is outside the workspace (`..` / non-relative / absolute) or a denied/runtime/service/secret path
an upstream preview_sha256 is present before preview
the preview command or its expected status is unclear
preview would need to fabricate a preview_sha256
approve or apply is requested
a model/broker call would be required
the live target file changes during preview (it must not)
secrets appear in output/logs
a raw localhost:11434 reference appears
```

No retries except re-reading already-written artifacts.

## 16. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | PARTIAL | BLOCKED | FAIL
MODE: S2C_1D_A2_L2B_PREVIEW_EXECUTION_PREVIEW_ONLY
APPROVAL: token present / exact:
WORKSPACE_ROOT / PLAN / AFTER_FILE:
PREFLIGHT: token / plan / root / after_file materialized+reviewed / one-step / target safe /
  pre-preview target capture / no prior approve-apply / no upstream preview_sha256 / no :11434:
STOP-BEFORE-PREVIEW printed:
PREVIEW COMMAND: exact command / exit code / status+outcome (exit-7 disambiguated):
OUTPUTS: artifact paths / PreviewRecord / preview_sha256 (payload/before/after):
POST-PREVIEW: PreviewRecord exists / preview_sha256 exists / target UNMODIFIED (pre==post) /
  no approve / no apply / no model|broker / runtime untouched / operator-reviewable / no apply token / no secrets:
AUTHORITY: A2-L2b is sole preview_sha256 author / approve+apply remain separate gated lanes:
SAFETY: approve/apply run / model|broker call / runtime touched / live target modified / fabricated hash / raw :11434:
STOP GATES HIT: none | details
NEXT BEST LANE:
```

A2-L2b remains the only write authority. The model proposes; the transform prepares a non-approvable
skeleton; the operator assembles a ready-to-preview input and reviews the after-bytes; this lane runs
the existing A2-L2b preview to obtain `preview_sha256` (preview-only); the operator approves; A2
applies. This lane never approves, never applies, and never modifies the live target.
