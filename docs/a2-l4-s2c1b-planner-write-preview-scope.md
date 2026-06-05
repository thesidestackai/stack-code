# A2-L4-S2C-1b Scope Card — Workspace-Write-Previewable Transform (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT IMPLEMENTED.** This card designs the next slice of
> the planner-output → A2-plan transform: the **workspace-write-previewable** path that
> S2C-1a deliberately deferred. It implements no transform, edits no
> `scripts/transform_no_write_advisory.py`, adds no fixtures or tests, runs no `claw plan`
> preview/approve/apply, calls no model or broker, and mutates no runtime. Created
> 2026-06-05.
>
> Docs location note: Stack-Code keeps scope cards flat under `docs/` (e.g.
> `docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md`,
> `docs/a2-l4-s2b5-live-broker-smoke-scope-card.md`); there is **no** `docs/a2-l4/`
> subdirectory, so this card lands at `docs/a2-l4-s2c1b-…` to match convention.

---

## 1. Executive Summary

S2C-1a is complete and merged (PR #74, merge commit `3eed22b`). It added the
`NO_WRITE_ADVISORY` transform path — a validated `a2-l4-planner-output.v1` candidate with
**no** `patch_intent` becomes a non-approvable operator-review artifact ("No workspace
write proposed.") — and routed any candidate carrying a `patch_intent` to
`WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE`, deferring it here.

S2C-1b designs that deferred path. The goal is to let a validated planner-output that
**describes** a bounded edit (via the closed, prose-only `patch_intent`) be classified as
`WORKSPACE_WRITE_PREVIEWABLE` and transformed into an **A2-plan preview-request skeleton**
the existing A2-L2b chain can consume — **without** the transform inventing after-bytes,
fabricating a `preview_sha256`, or gaining any approve/apply authority.

The load-bearing constraint, derived from the merged schema and the L2b proof: the
planner-output's `patch_intent` is a **closed object carrying only `summary` + `notes`
(prose)**; it is schema-forbidden from carrying an applyable replacement body
(`schemas/a2-l4/fixtures/planner-output/invalid-patch-intent-direct-replacement.json` is
rejected). Therefore the transform **can never derive exact after-bytes from a candidate**.
The A2-plan step's `after_file` (the exact after-bytes source) is **operator-supplied**,
and only the existing A2-L2b `claw plan run --workspace-write-preview` produces the
approval-binding `preview_sha256`. S2C-1b prepares; A2-L2b previews; the operator approves;
A2 applies.

---

## 2. Current State: S2C-1a Complete

Merged S2C-1a surface (`3eed22b`, do not edit in this lane):

```text
scripts/transform_no_write_advisory.py
schemas/a2-l4/fixtures/no-write-advisory/*.json   (6 fixtures)
tests/a2_l4/test_transform_no_write_advisory.py
```

Proven S2C-1a behavior:

```text
validated planner-output, NO patch_intent  -> NO_WRITE_ADVISORY review artifact
  (approval_allowed=False, apply_allowed=False, workspace_write_preview=False,
   preview_sha256=None, no write_target, no after_file)
patch_intent present                       -> WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE (deferred here)
apply/approve/run/shell field, or raw :11434 -> REJECT_UNSAFE
schema-invalid / missing objective / no next step -> REJECT_AMBIGUOUS
no source write, no model/broker call, no claw plan preview/approve/apply, no execution primitive
```

S2C-1a classification is total, deterministic, and UNSAFE-first. S2C-1b extends — but does
not alter — that contract: the `NO_WRITE_ADVISORY` path and its artifact remain unchanged.

---

## 3. Objective for S2C-1b

Design (docs only) a transform path that:

```text
consumes a VALIDATED a2-l4-planner-output.v1 candidate that carries a descriptive
  patch_intent (summary + notes; NO applyable body — schema-enforced),
classifies it WORKSPACE_WRITE_PREVIEWABLE only when it is safe, single-target, bounded,
  and workspace-relative,
emits an A2-plan (a2-plan-schema) PREVIEW-REQUEST SKELETON: exactly one
  mode: workspace-write step with tools:[Write], a bounded write_target, an
  OPERATOR-SUPPLIED after_file path placeholder, and expected_post_write hints,
and hands that skeleton to the EXISTING A2-L2b chain, which alone produces preview_sha256.
```

The transform's output is **upstream of and subordinate to** the A2-L2b authority chain. It
is never part of the preview/approval integrity chain.

---

## 4. Explicit Non-Goals

This card authorizes none of the following, and the future implementation must not:

```text
implement the transform (Rust or Python) — design only
edit scripts/transform_no_write_advisory.py, a2-plan-runner, a2-plan-schema,
  rusty-claude-cli, the planner adapter, or any schema/adapter/CLI/Dockerfile/runtime file
add or edit fixtures or tests in this lane
invent or auto-fill after_file bytes from prose
produce, fabricate, or claim a preview_sha256 / PreviewRecord
run claw plan preview / approve / apply, or A2 apply
write directly to any workspace source file
approve or apply a plan, or emit an apply bundle
support multi-file / batch workspace writes (L2b proves exactly one target per bundle)
call a model or the broker; load/switch/evict models; clear VRAM; restart a service
introduce a raw localhost:11434 app-inference path or any execution primitive
```

---

## 5. A2 Write Authority Boundary

```text
S2C-1b may classify a candidate and PREPARE a previewable A2-plan skeleton.
S2C-1b must NOT grant apply authority.
A2-L2b preview_sha256 remains the ONLY write-preview integrity authority.
No transform may write directly to workspace source files.
No transform may approve or apply a plan.
```

The authority chain (from `docs/a2-l2b-run-plan-preview-operator-handoff.md`) is unchanged:
only `claw plan run --workspace-write-preview` (a2-plan-runner) produces a `PreviewRecord`
and `preview_sha256`; `claw plan approve` is TTY-enforced and binds to that exact
`preview_sha256`; `claw plan apply` re-verifies `payload_sha256` / `before_sha256` /
`after_sha256`, atomically replaces exactly one file, and fails closed with rollback on any
mismatch. The transform appears nowhere in that chain.

---

## 6. Planner Output Inputs

Source of truth: `schemas/a2-l4/planner-output.schema.json` (`a2-l4-planner-output.v1`,
closed object) + the read-only validator `scripts/validate_planner_output_schema.py`.

Relevant fields for S2C-1b:

```text
required: schema_version, task_id, workspace_root, task_summary, plan_steps[] (inert prose),
          risk_notes[], operator_next_steps[]
optional: patch_intent { summary, notes[] }     <- CLOSED, prose-only; the write-intent GATE
          preview_request { requested, reason }  <- CLOSED; advisory request only, no command
          candidate_files[], repo_context_summary, test_suggestions[],
          external_verifier_handoff, status_snapshot
hard-forbidden (schema `not` + closed object): approval_line, approval_command,
          apply_command, apply_bundle_command, run_command, shell_command, raw :11434, secrets
```

Load-bearing facts:

```text
patch_intent CANNOT carry an applyable diff / replacement body (closed-object policy;
  invalid-patch-intent-direct-replacement.json is rejected). It is descriptive intent only.
preview_request CANNOT carry a command (invalid-preview-request-command.json is rejected).
Therefore a planner-output contains NO exact after-bytes and NO executable specifics
  (tools, write_target, after_file). These are operator judgment by design.
```

---

## 7. Output Contract

S2C-1b output is a **bounded artifact**, never a source-file write. Two shapes:

```text
WORKSPACE_WRITE_PREVIEWABLE  -> an A2-plan preview-request SKELETON artifact:
    plan: a2-plan-schema Plan { name, mode, model_tier, steps:[ one workspace-write step ] }
    step: { id, description, mode: workspace-write, tools: [Write],
            write_target { path (operator-confirmed, workspace-relative), create_if_absent },
            after_file: <OPERATOR-SUPPLIED PATH PLACEHOLDER — bytes NOT provided by transform>,
            expected_post_write { must_contain[], must_not_contain[] } (carried from intent, advisory) }
    plus a NON-APPROVABLE envelope:
      approval_allowed = False
      apply_allowed    = False
      preview_sha256   = None         (the transform NEVER sets this)
      workspace_write_preview = False (no preview has been produced yet)
      operator_action_required = "supply after_file bytes; run A2-L2b --workspace-write-preview"

REJECT_* (UNSAFE | AMBIGUOUS)  -> clean refusal, NO artifact, deterministic non-zero exit.
```

Invariants (to be asserted by future tests): previewable is **not** approvable; the skeleton
cannot be fed to `claw plan approve` / `apply`; the transform emits no `preview_sha256`; the
artifact is incomplete-by-design until the operator supplies after-bytes.

---

## 8. Workspace-Write-Previewable Path

Conceptual flow (design, **not** implementation):

```text
planner output with a descriptive patch_intent (write-intent signal)
  -> transform validates it as a2-l4-planner-output.v1 (else REJECT_AMBIGUOUS)
  -> transform runs UNSAFE-first checks (apply/approve/run/shell, :11434,
       preview_sha256-bypass, runtime/secret mutation, execution primitive) -> REJECT_UNSAFE on hit
  -> transform confirms the intent is SAFE, SINGLE-target, BOUNDED, workspace-relative,
       and representable WITHOUT inventing intent (else REJECT_AMBIGUOUS)
  -> transform emits an A2-plan preview-request SKELETON (one workspace-write step) with an
       OPERATOR-SUPPLIED after_file PLACEHOLDER and advisory expected_post_write
  -> OPERATOR supplies the exact after_file bytes and confirms write_target + tools
  -> EXISTING A2-L2b: claw plan run --workspace-write-preview  ->  PreviewRecord + preview_sha256
  -> OPERATOR approval (claw plan approve, TTY, binds preview_sha256) remains required before apply
  -> A2 apply (claw plan apply) mutates exactly one file, re-verifying every sha
```

The transform stops at emitting the skeleton. Everything from `claw plan run` onward is the
existing, unchanged A2-L2b chain. Path-safety for `write_target.path` and `after_file` mirrors
the L2a lexical rules (no absolute, no `..`, deny `.git`/`.claw`/`.claude`/`.env*`/`secret*`/
`credentials*`/`*.pem`/`*.key`; `after_file` != `write_target.path`).

---

## 9. Refusal / Out-of-Scope Classes

The transform must refuse (no artifact, deterministic exit) when the candidate:

```text
REJECT_UNSAFE
  - requests direct apply/approve (apply_command, approval_line, …) — also schema-forbidden
  - requests a direct workspace mutation outside A2 (its own write/run/shell path)
  - references a raw localhost:11434 app-inference endpoint
  - requests / contains an execution primitive (shell, subprocess, curl, network call)
  - attempts to BYPASS preview_sha256 (claims a preview hash, asserts pre-approval,
    or supplies an apply bundle / approval line)
  - asks to modify runtime services / broker / Vault / secrets / models / VRAM
  - proposes a write_target outside workspace write-policy (absolute / `..` / denied path)

REJECT_AMBIGUOUS
  - fails a2-l4-planner-output.v1 schema validation
  - lacks objective / task_summary
  - lacks a proposed next step
  - patch_intent present but the write content is missing / under-specified (no bounded target,
    or a write that cannot be represented without inventing intent)
  - ambiguous, multi-file, or non-workspace-relative target
  - broad/undefined refactor or multi-file sweep (unsupported; L2b is one target per bundle)
```

A refusal is an escalation to the operator, never a retry of the forbidden action reframed.

---

## 10. Fixture Plan

Future fixtures (design only — **do not create in this lane**), grouped:

```text
valid-previewable-patch-intent.json        single bounded write_target, prose patch_intent,
                                           after_file placeholder -> WORKSPACE_WRITE_PREVIEWABLE
missing-patch-body.json                    patch_intent present but no bounded target -> REJECT_AMBIGUOUS
ambiguous-target.json                      vague / multi / non-relative target       -> REJECT_AMBIGUOUS
multi-file-write-intent.json               more than one write target (unsupported)  -> REJECT_AMBIGUOUS/OUT_OF_SCOPE
unsafe-apply-command.json                  apply_command / approval_line             -> REJECT_UNSAFE
raw-11434-reference.json                   localhost:11434 reference                 -> REJECT_UNSAFE
execution-primitive.json                   shell / subprocess / curl request         -> REJECT_UNSAFE
preview-sha256-bypass.json                 claims a preview_sha256 / pre-approval     -> REJECT_UNSAFE
```

The S2C-1a `no-write-advisory/` fixtures remain a **separate** class and are unchanged.
Future fixtures land under a new directory (e.g. `schemas/a2-l4/fixtures/write-previewable/`)
to keep the two classes independently greppable.

---

## 11. Test Plan

Future tests (design only — **do not create in this lane**), required themes:

```text
classification is total + deterministic over every fixture
WORKSPACE_WRITE_PREVIEWABLE artifact is previewable but NOT approvable
  (approval_allowed=False, apply_allowed=False, cannot be fed to approve/apply)
the transform produces NO preview_sha256 (preview_sha256 is None / absent)
the after_file is an operator-supplied placeholder; the transform never fills its bytes
no direct workspace source write occurs (output is a bounded artifact)
the transform has no apply authority and invokes no claw plan preview/approve/apply
A2-L2b preview_sha256 remains mandatory and is produced ONLY by the existing chain
REJECT_* cases produce a clean refusal with no artifact
CLI exit codes remain deterministic and distinct per class
no execution primitive (httpx/requests/urllib/subprocess/os.system/os.popen) in the source
```

CI must run on fixtures only — no live inference, no broker, no apply. Tests must be stdlib
`unittest` (CI runs `python -m unittest discover`; no pytest), matching the S2C-1a port.

---

## 12. Implementation Surfaces for Future Lane

Anticipated (for a separately scoped/approved S2C-1b **implementation** lane):

```text
scripts/transform_write_previewable.py            (new; mirrors transform_no_write_advisory.py style)
schemas/a2-l4/fixtures/write-previewable/*.json   (new fixtures)
tests/a2_l4/test_transform_write_previewable.py   (new stdlib unittest)
(reused, READ-ONLY) scripts/validate_planner_output_schema.py
(reference, NOT edited) a2-plan-schema, a2-plan-runner, rusty-claude-cli
```

No existing implementation file is edited; the new transform reuses the read-only validator
and emits an a2-plan skeleton artifact. The a2-plan-runner / CLI are referenced, never
modified.

---

## 13. Validation Gates

A future S2C-1b implementation (separately scoped/approved) must demonstrate:

```text
input is a validated a2-l4-planner-output.v1 (else REJECT_AMBIGUOUS)
classification is deterministic and total (every candidate lands in exactly one class)
WORKSPACE_WRITE_PREVIEWABLE emits a plan SKELETON that is provably non-approvable
  (no preview_sha256, cannot be fed to approve/apply) and whose after_file is operator-supplied
no source files are written by the transform (output is a bounded artifact)
no model call, no broker call, no claw plan preview/approve/apply invoked by the transform
REJECT_* cases produce a clean refusal, never a partial/forced preview
fixtures cover the previewable case and each refusal condition; CI is fixtures-only
tests are stdlib unittest (no pytest dependency)
```

---

## 14. STOP Gates

Design or implementation must STOP and escalate (not reframe) if it would:

```text
bypass the A2-L2b preview/approve/apply chain
grant apply or approve authority inside the transform
allow a direct workspace source write by the transform
fabricate, claim, or emit a preview_sha256 / PreviewRecord
auto-fill / invent after_file bytes from prose
introduce an execution primitive (shell / subprocess / curl / network)
treat a raw localhost:11434 reference as safe app inference
broaden a single bounded write into a multi-file / batch write
modify runtime services, broker, Vault, secrets, models, or VRAM
```

---

## 15. Future Merge / Review Requirements

```text
1. Read-only operator review of THIS scope card, then push + PR (docs-only).
2. (If approved) S2C-1b implementation lane: new transform + write-previewable fixtures +
   stdlib-unittest tests, fixtures-only CI, no model/preview/apply — separately scoped/approved.
3. Implementation PR must pass the full A2-L4 CI (planner-output validator, cargo fmt/test/
   clippy, shell tests, docs source-of-truth) and an exact-head merge gate.
4. (Later) S2D: Operator Review -> A2 Approval gate (binds preview_sha256), then S2E apply.

Do NOT implement any transform until this scope card is reviewed and merged.
```

---

## 16. Status Block

```text
status: DOCS-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not implement a transform or edit transform/runner/schema/adapter/CLI/broker code.
It does not add fixtures or tests.
It does not run claw plan preview/approve/apply or any A2 apply.
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It never makes a previewable candidate approvable, and it never fabricates or emits a
  write preview_sha256; after_file bytes are operator-supplied, never machine-invented.
It does not introduce a raw localhost:11434 app-inference path or any execution primitive.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write
authority. The model proposes; the transform prepares a previewable skeleton; the operator
supplies after-bytes and approves; A2 applies.
Next gate: read-only operator review of this scope card.
```
