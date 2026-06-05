# A2-L4-S2C-1d Scope Card — Preview Execution (run the existing A2-L2b workspace-write preview) (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT AN EXECUTION LANE.** This card scopes a future,
> separately-approved lane that runs the **existing** A2-L2b workspace-write preview on a
> ready-to-preview input (assembled per the merged S2C-1c-a runbook) to obtain a
> `PreviewRecord` + `preview_sha256`. **Preview-only** — it never approves or applies. This
> docs lane itself runs no preview/approve/apply, calls no model or broker, fabricates no
> `preview_sha256`, invents no `after_file` bytes, edits no transform/runner/CLI/schema code,
> and mutates no runtime. Created 2026-06-05.
>
> Docs location note: Stack-Code keeps these flat under `docs/` (e.g.
> `docs/a2-l4-s2c1c-a-preview-assembly-runbook.md`), so this card lands at
> `docs/a2-l4-s2c1d-…` to match convention.

---

## 1. Executive Summary

The S2C chain now reaches a **ready-to-preview** input: S2C-1b (PR #77 `97569a7`) emits a
non-approvable `workspace_write_preview_request` skeleton; S2C-1c (PR #78 `d609daa`) scoped the
preview-path integration; S2C-1c-a (PR #79 `e91d925`) gave the operator a runbook to assemble a
`plan.yaml` + materialized, reviewed `after_file` bytes + chosen `--workspace-root` — and to
**STOP before** preview.

S2C-1d scopes the next step: a future, **token-gated** execution lane that runs **only** the
existing, runtime-proven A2-L2b command —

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

— to produce a `PreviewRecord` and the approval-binding `preview_sha256`, plus write-preview-ready
evidence. The lane is **preview-only**: it never approves or applies, never modifies the live
`write_target`, and never fabricates a hash. Approval (binding `preview_sha256`) and apply remain
further, separate, gated lanes.

This card is design only. It does **not** run the command.

---

## 2. Status and Scope

**DOCS-ONLY SCOPE CARD — NOT AUTHORIZED TO RUN PREVIEW.**

In scope: the inputs, preconditions, the exact preview command, the mandatory STOP-before-preview
message and approval token, the expected outputs, the `preview_sha256` authority, post-preview
validation, the approval/apply boundary, the rejection/STOP conditions, and the follow-on sequence
for a future preview execution lane.

Out of scope (this card authorizes none of these): running `claw plan run`/preview/approve/apply
or any A2 apply; computing or fabricating a `preview_sha256`/`PreviewRecord`; inventing
`after_file` bytes or a `write_target`; modifying the live `write_target`; editing transform/
runner/CLI/schema/adapter code; model/broker calls; runtime/service/Vault mutation; raw
`localhost:11434` app inference.

---

## 3. Source of Truth

```text
docs/a2-l4-s2c1c-a-preview-assembly-runbook.md       (ready-to-preview assembly + STOP-before-preview)
docs/a2-l4-s2c1c-preview-path-integration-scope.md   (preview-path gap + authority boundary)
scripts/transform_write_previewable.py                (S2C-1b skeleton shape — non-approvable, preview_sha256=null)
docs/a2-plan-schema.md                                (a2-plan-schema plan.yaml + L2a path/after_file rules)
docs/a2-l2b-run-plan-preview-operator-handoff.md      (runtime-proven preview→approve→apply chain; exit codes; artifacts)
rust/crates/a2-plan-runner/src/{diff_preview,write_preview}.rs  (PreviewRecord/preview_sha256 — reference only)
rust/crates/rusty-claude-cli/src/main.rs              (claw plan run/preview/approve/apply — reference only)
```

---

## 4. Inputs

The future preview execution lane requires:

```text
- a ready-to-preview plan.yaml (a2-plan-schema workspace-write plan, one step), per S2C-1c-a
- the workspace root (--workspace-root)
- the materialized after_file bytes (at the plan's after_file path, relative to the root)
- the S2C-1c-a ready-to-preview checklist (all items satisfied)
- the S2C-1c-a operator handoff summary
```

---

## 5. Preconditions

Before running preview, the future lane must verify:

```text
- the S2C-1c-a assembly runbook was followed
- plan.yaml exists and is a valid a2-plan-schema workspace-write plan
- the workspace root exists (and is a real intended workspace; disposable temp recommended for trials)
- the after_file path exists and its bytes are materialized
- the after_file bytes were operator-reviewed (no secrets, no :11434, matches intent)
- the plan contains EXACTLY ONE workspace-write step (tools:[Write], write_target, after_file != write_target)
- the write_target path is workspace-relative and L2a-safe (no absolute, no `..`, not under
  .git/.claw/.claude, final component not .env*/secret*/credentials*/*.pem/*.key)
- no runtime/service/secret path is targeted
- no approval or apply has occurred
- no preview_sha256 is present upstream (the skeleton's was null; nothing fabricated one)
```

---

## 6. Non-Goals

```text
This lane does not approve.
This lane does not apply.
This lane does not call a model.
This lane does not call broker.
This lane does not fabricate preview_sha256.
This lane does not invent after_file bytes.
This lane does not modify target files directly.
```

---

## 7. Preview Execution Command

The future execution lane (and only that lane, behind the approval token in §8) may run the
existing, runtime-proven A2-L2b command:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

**This docs lane must not run it.** The lane runs only this existing command — it adds no new
preview engine and computes no hash itself; `--workspace-write-preview` halts the runner right
after the preview artifacts are written (it does not approve or apply).

---

## 8. STOP Before Preview

### Mandatory STOP message

The future execution prompt must print this **exact** message before running preview:

```text
STOP BEFORE A2-L2B PREVIEW EXECUTION:
About to run the existing A2-L2b workspace-write preview command.
This will create PreviewRecord and preview_sha256.
It will not approve or apply.
Proceed only with explicit operator approval for preview execution.
```

### Approval token (exact)

The future execution prompt must require this **exact** token, and STOP if it is missing:

```text
APPROVED: Execute S2C-1d A2-L2b preview execution
```

Without that exact token the future lane reports `BLOCKED: missing required approval token.` and
runs nothing.

---

## 9. Expected Preview Outputs

The expected preview-ready outcome is the **existing** A2-L2b behavior (do not re-implement it):

```text
- a preview bundle / preview-generator-result artifact is emitted under <workspace>/.claw/
  (e.g. .claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json and
   preview-generator-result.json; payload after.bin + after.sha256; checkpoint manifest)
- a PreviewRecord exists
- a preview_sha256 exists (binding payload_sha256 / before_sha256 / after_sha256)
- the command halts at the preview-ready state (it does not proceed to approve/apply)
```

Exit-code note (from the A2-L2b operator handoff): the preview-ready halt is exit `7`
(`write_preview_ready`). **Exit `7` is overloaded** — it is also used for approval-denied — so the
execution lane MUST disambiguate via the structured `status` / `outcome` fields and MUST NOT treat
the bare code as sufficient. If any exit code / status field is uncertain at execution time, the
execution lane must **discover it from the live output**, not guess.

---

## 10. PreviewRecord / preview_sha256 Authority

```text
Only A2-L2b `claw plan run --workspace-write-preview` produces a PreviewRecord.
Only A2-L2b computes preview_sha256 (payload_sha256 / before_sha256 / after_sha256).
The execution lane reads/reports these; it never computes or fabricates them.
The PreviewDisplay is non-authoritative; only the PreviewRecord (and the preview_sha256 it pins)
  can later bind an approval.
```

---

## 11. Post-Preview Validation

After running preview, the future lane must validate:

```text
- the command exit code (disambiguated via status/outcome — see §9)
- a preview artifact exists
- a PreviewRecord exists
- a preview_sha256 exists
- the live target file was NOT modified (preview does not write the target)
- no approve occurred
- no apply occurred
- no runtime was touched
- the preview output is operator-reviewable
- no approval token for APPLY is generated or implied by the preview step
```

---

## 12. Approval Boundary

```text
Preview is NOT approval. Producing a preview_sha256 grants no apply authority.
Approval is a later, SEPARATE lane (S2D): `claw plan approve <preview-bundle.json>`, TTY-enforced,
  with the approval line `apply <step_id> <preview_sha256>`.
Approval binds to the exact preview_sha256 produced by A2-L2b — never a fabricated value.
This lane and its execution lane never approve.
```

---

## 13. Apply Boundary

```text
Apply is a later, SEPARATE lane (S2E): `claw plan apply <apply-bundle.json>`.
Apply re-verifies payload_sha256 / before_sha256 / after_sha256, atomically replaces exactly one
  file, and fails closed with rollback on any mismatch.
The apply-bundle is produced ONLY by `claw plan apply-bundle` (never hand-authored).
This lane and its execution lane never apply and never emit an apply bundle.
A2 remains the only write path; the model proposes, the operator approves, A2 applies.
```

---

## 14. Rejection / STOP Conditions

The future execution lane must STOP — escalate, never reframe — and run no preview if:

```text
plan.yaml is missing
the workspace root is missing
after_file is missing (no materialized bytes)
after_file was not operator-reviewed
the plan has more than one write step
the write_target path is outside the workspace (`..` / non-relative / absolute)
the write_target or after_file is a runtime/service/secret path (or an L2a-denied path)
a preview_sha256 is already present upstream (the input claims one)
preview would need to fabricate a preview_sha256
the preview command or its expected status is unclear (discover, do not guess)
approve or apply is requested
a model/broker call would be required
the live target file was modified before/without apply
the skeleton/plan/after_file references a raw localhost:11434 endpoint
secrets/tokens/keys appear in the preview output
the exact approval token (§8) is absent
```

---

## 15. Validation Plan

This scope card is validated (in review) by confirming:

```text
it is docs-only (no scripts/tests/schemas/rust/src/runtime changes)
it authorizes no preview/approve/apply from THIS lane (the execution lane is token-gated)
it preserves A2-L2b as the sole PreviewRecord/preview_sha256 authority
it contains the exact STOP-before-preview message (§8) and the exact approval token (§8)
preview is preview-only: it never approves/applies and never modifies the live target
raw :11434 appears only as a rejection/prohibition pattern
exit-code/status handling requires disambiguation/discovery, not guessing
```

A future S2C-1d **preview execution prompt** (separately scoped/approved) operationalizes this
card; only that prompt, with the exact token, may run the preview command.

---

## 16. Follow-On Lanes

```text
1. Read-only review of THIS scope card, then push/merge (docs-only).
2. S2C-1d preview execution PROMPT draft (token-gated; still preview-only).
3. S2C-1d preview execution APPROVAL gate (run the existing --workspace-write-preview; obtain
   PreviewRecord + preview_sha256; preview-only).
4. S2D approval gate scope/prompt (operator approval binds preview_sha256).
5. S2E apply gate scope/prompt (A2 apply re-validates the chain and writes exactly one file).

Do NOT run preview/approve/apply until this scope card is reviewed and merged and the preview
execution prompt is separately scoped and approved (with the exact token).
```

---

## 17. Status Block

```text
status: DOCS-ONLY SCOPE CARD — NOT AUTHORIZED TO RUN PREVIEW/APPROVE/APPLY

This card authorizes design only.
It does not run claw plan run/preview/approve/apply or any A2 apply.
It does not compute or fabricate a preview_sha256, and never creates a PreviewRecord.
It does not invent after_file bytes or a write_target, and never modifies the live target.
It does not grant preview/approve/apply authority outside the token-gated execution lane.
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It does not introduce a raw localhost:11434 app-inference path.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write authority.
The model proposes; the transform prepares a non-approvable skeleton; the operator assembles a
ready-to-preview input; a token-gated lane runs the existing A2-L2b preview to obtain
preview_sha256; the operator approves; A2 applies.
Next gate: read-only operator review of this scope card.
```
