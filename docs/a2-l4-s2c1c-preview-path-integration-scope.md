# A2-L4-S2C-1c Scope Card — Preview-Path Integration (S2C-1b → A2-L2b) (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT IMPLEMENTED.** This card designs how the merged
> S2C-1b `workspace_write_preview_request` skeleton reaches the **existing** A2-L2b
> workspace-write preview chain. It implements no preview-input assembler, edits no
> transform/runner/CLI/schema code, runs no `claw plan` preview/approve/apply, calls no
> model or broker, and mutates no runtime. Created 2026-06-05.
>
> Docs location note: Stack-Code keeps scope cards flat under `docs/` (e.g.
> `docs/a2-l4-s2c1b-planner-write-preview-scope.md`); there is no `docs/a2-l4/`
> subdirectory, so this card lands at `docs/a2-l4-s2c1c-…` to match convention.

---

## 1. Executive Summary

S2C-1b (merged, PR #77 `97569a7`) added a transform that turns a validated
`a2-l4-planner-output.v1` candidate with a descriptive `patch_intent`, plus an
operator-supplied `write_target` and `after_file` path, into a **non-approvable**
`workspace_write_preview_request` skeleton. The skeleton's `.plan` object is already
shaped like an `a2-plan-schema` `Plan` (one `mode: workspace-write` step), but it is **not
yet runnable**: it is JSON (A2-L2b consumes a plan **YAML** file), its `after_file` is a
**path placeholder whose bytes were never materialized**, and it deliberately carries
`preview_sha256: null`.

S2C-1c scopes the bridge from that skeleton to the **existing** A2-L2b preview:

```text
S2C-1b skeleton (non-approvable, preview_sha256=null)
→ operator materializes + reviews exact after_file bytes
→ skeleton.plan serialized to an a2-plan-schema plan.yaml
→ EXISTING A2-L2b: claw plan run <plan.yaml> --workspace-root <ws> --workspace-write-preview
→ A2-L2b alone produces PreviewRecord + preview_sha256
→ (later, separate lane) operator approval binds to preview_sha256
→ (later, separate lane) A2 apply re-validates the authority chain and writes one file
```

The card defines the gap, the operator after-file rules, the assembly step, and — most
importantly — the **STOP point**: this lane (and its recommended first slice) goes no
further than producing a *ready-to-preview* input. **A2-L2b remains the sole authority that
computes `preview_sha256`.** No preview/approve/apply is authorized here.

---

## 2. Status and Scope

**DOCS-ONLY SCOPE CARD — NOT IMPLEMENTED.**

In scope: design of how the S2C-1b skeleton + operator-supplied after-bytes become valid
input to the existing A2-L2b `--workspace-write-preview` command — the after-file handling
rules, the skeleton→plan.yaml assembly, the preview/approval/apply authority boundary, the
rejection conditions, and an implementation-option recommendation.

Out of scope (this card authorizes none of these):

```text
implementing a preview-input assembler (Python or Rust)
editing scripts/transform_write_previewable.py or its tests/fixtures
editing a2-plan-runner / a2-plan-schema / rusty-claude-cli / any runner or CLI
running claw plan run / preview / approve / apply, or any A2 apply
materializing or inventing after_file bytes
live model inference / broker calls; model load/switch/evict; VRAM clear; service restart
raw localhost:11434 app inference
```

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write
authority.

---

## 3. Source of Truth

```text
scripts/transform_write_previewable.py                       (merged S2C-1b transform — skeleton shape)
tests/a2_l4/test_transform_write_previewable.py              (skeleton invariants)
docs/a2-l4-s2c1b-planner-write-preview-scope.md              (S2C-1b design)
handoffs/s2c1b_planner_write_preview_transform_implementation_prompt_DRAFT_2026-06-05.md  (S2C-1b impl prompt)
docs/a2-plan-schema.md                                        (a2-plan-schema plan.yaml + L2a path/after_file rules)
docs/a2-l2b-run-plan-preview-operator-handoff.md              (runtime-proven preview→approve→apply chain + authority)
rust/crates/a2-plan-schema/src/plan_schema.rs                 (Plan/PlanStep/WriteTarget — reference only)
rust/crates/a2-plan-runner/src/{diff_preview,write_preview}.rs (PreviewRecord/preview_sha256 — reference only)
rust/crates/rusty-claude-cli/src/main.rs                      (claw plan run/preview/approve/apply — reference only)
```

---

## 4. Current S2C-1b Output

The transform emits a JSON `workspace_write_preview_request` artifact:

```text
artifact_type            = "workspace_write_preview_request"
schema_version           = "a2-l4-write-preview-request.v1"
approval_allowed         = false
apply_allowed            = false
workspace_write_preview  = false
preview_sha256           = null
operator_action_required = "supply after_file bytes, then run the existing A2-L2b workspace-write-preview"
plan = {
  name, mode: "workspace-write", model_tier: "FAST",
  steps: [ { id, description, mode: "workspace-write", tools: ["Write"],
             write_target: { path: <operator>, create_if_absent: false },
             after_file: <operator path placeholder — bytes NOT read>,
             expected_post_write: { must_contain: [], must_not_contain: [] } } ]
}
source_candidate_path, source_candidate_sha256, objective, assumptions_or_plan_summary,
risks, operator_review_notes
```

Properties (asserted by S2C-1b tests): exactly one workspace-write step; `tools == ["Write"]`;
`write_target`/`after_file` are operator-supplied and lexically policy-safe (no absolute, no
`..`, no `.git`/`.claw`/`.claude`/`.env*`/`secret*`/`credentials*`/`*.pem`/`*.key`; `after_file
!= write_target`); **the transform never reads `after_file` bytes** and **never sets a real
`preview_sha256`**.

---

## 5. Current A2-L2b Preview Input

A2-L2b's runtime-proven preview entry point (from the operator handoff):

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
```

`plan.yaml` is an `a2-plan-schema` document. A workspace-write step requires:

```text
mode: workspace-write
tools: [Write]                       # must include Write
write_target: { path, create_if_absent }   # workspace-relative, lexically safe
after_file: <workspace-root-relative path>  # REQUIRED; its BYTES are the exact after-bytes;
                                            # must != write_target.path; L2a deny-rules apply
expected_post_write: { must_contain[], must_not_contain[] }   # optional, advisory
```

`--workspace-write-preview` halts the runner right after writing the preview artifacts
(exit `7` = `write_preview_ready`, a success state). It produces, under `<workspace>/.claw/`:
a `preview-bundle.json`, a `preview-generator-result.json`, a payload `after.bin` +
`after.sha256`, and a checkpoint manifest. **A2-L2b reads the `after_file` bytes at preview
time** and binds them into a `PreviewRecord` with `payload_sha256` / `before_sha256` /
`after_sha256` — the `preview_sha256` that a later approval binds to.

---

## 6. Gap Analysis

| Concern | S2C-1b skeleton | A2-L2b preview needs | Gap to close |
|---------|-----------------|----------------------|--------------|
| Format | JSON artifact (`.plan` sub-object) | a plan **YAML** file path | serialize `skeleton.plan` → `plan.yaml` (field names already align) |
| after_file bytes | a **path placeholder**; bytes never read/materialized | the file must **exist** with the exact reviewed after-bytes | operator materializes + reviews the bytes at that path **before** preview |
| workspace root | `source` candidate had `workspace_root`; skeleton plan does not pass `--workspace-root` | `--workspace-root <ws>` flag | operator supplies `--workspace-root`; write_target/after_file resolve under it |
| preview_sha256 | `null` (never fabricated) | computed by A2-L2b at preview time | nothing else may compute it; only `claw plan run --workspace-write-preview` may |
| approvability | `approval_allowed/apply_allowed = false` | preview produces a separately-approvable `preview-bundle.json` | approval is a later, separate lane bound to the real `preview_sha256` |

Summary of the irreducible gap: **(a) a near-mechanical skeleton→plan.yaml serialization, and
(b) an operator-owned after-file materialization + review.** Neither step may compute or
fabricate a `preview_sha256`; only the existing A2-L2b preview does.

---

## 7. Operator-Supplied After-File Handling

```text
The operator (not any model, transform, or assembler) materializes the exact after_file bytes.
No component may invent, generate, or auto-fill after_file bytes.
The operator REVIEWS the exact bytes before preview (they are the post-write content).
after_file path rules (already enforced by the S2C-1b skeleton; re-asserted here): workspace-
  relative, lexically safe (no absolute, no `..`, no `.git`/`.claw`/`.claude`/`.env*`/`secret*`/
  `credentials*`/`*.pem`/`*.key`), and != write_target.path.
  Note: a2-l2a denies `.claw` unconditionally, so the materialized after_file must live under a
  non-denied workspace path (e.g. `materialized/...`), not under `.claw/`.
after_file must be bounded in size (a future assembler/runner enforces a size cap; the operator
  keeps it small and single-file).
after_file must not target a runtime/service/secret path.
If the operator cannot produce reviewed exact bytes, STOP — do not preview.
```

---

## 8. Preview Input Assembly

Conceptual flow (design, **not** implementation):

```text
1. Take the S2C-1b skeleton (artifact_type == workspace_write_preview_request).
2. Confirm the non-approvable envelope is intact: approval_allowed=false, apply_allowed=false,
   workspace_write_preview=false, preview_sha256=null. (If any is otherwise -> STOP, see §13.)
3. Operator materializes the exact after_file bytes at skeleton.plan.steps[0].after_file and
   reviews them.
4. Serialize skeleton.plan -> plan.yaml (a2-plan-schema): name, mode: workspace-write,
   model_tier: FAST, steps[0].{id, description, mode, tools:[Write], write_target{path,
   create_if_absent}, after_file, expected_post_write}. Field names already align 1:1.
5. Hand off to the EXISTING A2-L2b command (a later, separately approved lane):
   claw plan run plan.yaml --workspace-root <ws> --workspace-write-preview
6. A2-L2b alone reads after_file bytes, builds the PreviewRecord, and computes preview_sha256.
```

The assembly step (4) is a pure, offline format conversion. It must **not** read after_file
bytes, compute any hash that resembles `preview_sha256`, or run preview.

---

## 9. Preview Execution Boundary

```text
This scope card stops at a READY-TO-PREVIEW input (plan.yaml + materialized after_file + chosen
  --workspace-root). It does NOT run preview.
Running `claw plan run --workspace-write-preview` is a future, SEPARATELY APPROVED lane.
The future preview lane runs ONLY the existing, runtime-proven A2-L2b command; it adds no new
  preview engine and computes no preview hash itself.
```

---

## 10. PreviewRecord / preview_sha256 Authority

```text
Only A2-L2b `claw plan run --workspace-write-preview` produces a PreviewRecord.
Only A2-L2b computes preview_sha256 (payload_sha256 / before_sha256 / after_sha256).
The S2C-1b skeleton's preview_sha256 is null and MUST stay null upstream of preview.
No assembler, transform, runbook step, or operator action fabricates a preview_sha256.
The PreviewDisplay is non-authoritative; only the PreviewRecord (and the preview_sha256 it pins)
  can later bind an approval.
```

---

## 11. Approval Boundary

```text
Approval (`claw plan approve <preview-bundle.json>`) is a later, SEPARATE lane.
Approval is TTY-enforced; the approval line is `apply <step_id> <preview_sha256>`.
Approval binds to the exact preview_sha256 produced by A2-L2b — never a value from the skeleton.
This card and its first slice never approve anything.
```

---

## 12. Apply Boundary

```text
Apply (`claw plan apply <apply-bundle.json>`) is a later, SEPARATE lane.
Apply re-verifies payload_sha256 / before_sha256 / after_sha256, atomically replaces exactly one
  file, and fails closed with rollback on any mismatch.
The apply-bundle is produced ONLY by `claw plan apply-bundle` (never hand-authored).
This card and its first slice never apply anything and never emit an apply bundle.
A2 remains the only write path; the model proposes, the operator approves, A2 applies.
```

---

## 13. Rejection / STOP Conditions

STOP — escalate, never reframe — and do not proceed to preview if:

```text
after_file is missing (no materialized bytes)
after_file is ambiguous or not yet reviewed by the operator
after_file bytes were generated by a model without operator review
after_file path is absolute, contains `..`, or is otherwise outside the workspace
write_target or after_file path targets a runtime/service/secret path (or an L2a-denied path)
the step is not exactly one workspace-write step (multi-file / read-only mismatch)
preview would need to fabricate or supply a preview_sha256
the skeleton has approval_allowed=true or apply_allowed=true
the skeleton already carries a non-null preview_sha256
the candidate or plan references a raw localhost:11434 app-inference endpoint
any approve/apply is attempted from this lane
any runtime / service / model / broker is touched
```

---

## 14. Validation Plan

A future, separately-approved implementation (assembler and/or runbook) must demonstrate:

```text
the non-approvable envelope is verified before assembly (approval_allowed/apply_allowed=false,
  workspace_write_preview=false, preview_sha256=null)
the assembled plan.yaml is a valid a2-plan-schema workspace-write plan (one step, tools:[Write],
  write_target + after_file present, after_file != write_target, L2a path rules satisfied)
the assembler reads no after_file bytes and computes no preview_sha256
the operator materializes + reviews after_file bytes out-of-band
preview is run ONLY by the existing A2-L2b command, in a separate approved lane
fixtures/tests (if Option 2 is chosen) are offline and fixtures-only; stdlib unittest (no pytest)
no model/broker call; no claw preview/approve/apply invoked by the assembler
```

---

## 15. Implementation Options

```text
Option 1 — Docs/prompt-only preview-assembly runbook  [RECOMMENDED FIRST]
  No code. A docs runbook giving the operator the exact, safe manual steps: verify the skeleton
  envelope, materialize + review after_file bytes, hand-serialize skeleton.plan to plan.yaml, and
  invoke the existing `claw plan run --workspace-write-preview`. Smallest surface; adds no new
  authority code; proves the artifact flow end-to-end with a real candidate before any tooling.

Option 2 — Offline preview-input assembler
  A small stdlib tool that consumes the skeleton JSON, re-verifies the non-approvable envelope and
  the a2-plan-schema shape, and emits a plan.yaml — WITHOUT reading after_file bytes, computing any
  hash, or running preview. Add only if Option 1's manual serialization proves repetitive/error-prone.

Option 3 — Direct integration into claw plan / the runner  [REJECTED / DEFERRED]
  Teach the CLI/runner to consume the skeleton directly. Rejected for now: it touches runner/CLI
  authority surfaces and risks blurring the "transform is non-authoritative / A2-L2b owns
  preview_sha256" boundary. Revisit only if a reviewed CLI-architecture lane requires it.
```

Recommendation: **Option 1 first, then Option 2 if repeated use proves valuable.** Rationale:
no new authority code, preserves the A2 boundary, and lets the team prove the artifact flow
before adding tooling.

---

## 16. Recommended First Slice

**S2C-1c-a — Preview-assembly operator runbook (Option 1, docs-only).** It documents the safe
manual path from a merged S2C-1b skeleton to a *ready-to-preview* A2-L2b input, stopping before
`claw plan run`. It introduces no code, computes no `preview_sha256`, and authorizes no
preview/approve/apply. Graduates to Option 2 (offline assembler) only if the manual
skeleton→plan.yaml step proves repetitive.

Do **not** implement an assembler or run preview first; prove the flow with the runbook and a
real operator-reviewed after_file.

---

## 17. Follow-On Lanes

```text
1. Read-only review of THIS scope card, then push/merge (docs-only).
2. (If approved) S2C-1c-a: preview-assembly operator runbook (Option 1), docs-only — separately scoped.
3. (Later, if needed) S2C-1c-b: offline preview-input assembler (Option 2), fixtures-only, no preview.
4. (Later, separately approved) S2C-1d: run the EXISTING A2-L2b `--workspace-write-preview` on an
   assembled input to obtain PreviewRecord + preview_sha256 (preview-only; no approve/apply).
5. (Later) S2D: Operator Review -> A2 Approval gate (binds preview_sha256), then S2E apply.

Do NOT run preview/approve/apply until this scope card is reviewed and merged and the preview lane
is separately scoped and approved.
```

---

## 18. Status Block

```text
status: DOCS-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not implement a preview-input assembler or edit transform/runner/CLI/schema code.
It does not run claw plan run/preview/approve/apply or any A2 apply.
It does not materialize or invent after_file bytes; the operator supplies and reviews them.
It never fabricates a preview_sha256; only the existing A2-L2b preview computes it.
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It does not introduce a raw localhost:11434 app-inference path.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write authority.
The model proposes; the transform prepares a non-approvable skeleton; the operator materializes and
reviews the after-bytes and assembles the plan; A2-L2b previews; the operator approves; A2 applies.
Next gate: read-only operator review of this scope card.
```
