# DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL

⚠️ REVIEW REQUIRED: This is a **future implementation prompt**, authored 2026-06-05 by the
S2C-1a prompt-drafting lane. It has **not** been run. It implements the **first**
slice of the planner-output → A2-plan transform: the **no-write advisory review-artifact**
case. It must be reviewed and merged, then invoked only with the exact operator approval
token below, before any code is written.

Implements: `docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md` (merged PR #72,
`87a87e884158e3f8a99f31c48410c53107f0ed2e`), §8 class `NO_WRITE_ADVISORY`, §10, §15, §17
(Option 1 — recommended first slice).

> Convention note: Stack-Code had no `handoffs/` directory (its handoff-style docs lived
> flat under `docs/`, e.g. `a2-l2b-run-plan-preview-operator-handoff.md`). This draft is
> placed under `handoffs/` to match the cross-repo handoff convention and keep the
> drafting-lane validation deterministic; a reviewer may relocate it to `docs/` if the
> repo prefers a single docs tree.

---

# CLAUDE CODE PROMPT — S2C-1a No-Write Advisory Review-Artifact Transform (Implementation)

## 1. Status and Approval Requirement

Implements the first, lowest-risk slice of the planner-output → A2-plan transform: a
validated `a2-l4-planner-output.v1` candidate **with no `patch_intent`** → a
**non-approvable operator-review artifact** stating "No workspace write proposed." No
model call, no broker call, no `claw plan` preview/approve/apply, no workspace-write
preview, no `write_target`/`after_file`, no write `preview_sha256`.

Do **not** begin unless the operator has provided this **exact** token in the current
instruction:

```text
APPROVED: Execute S2C-1a no-write advisory transform implementation
```

If that exact token is missing, STOP immediately and report:

```text
BLOCKED: missing required approval token.
```

## 2. Role

You are a careful Stack-Code transform implementation engineer. Follow:
OBSERVE → VERIFY → TDD (test-first) → IMPLEMENT MINIMAL → VALIDATE → REPORT.

## 3. Objective

Add the smallest code + fixtures + tests that:

```text
1. Accept a validated a2-l4-planner-output.v1 document as input.
2. Classify it: NO_WRITE_ADVISORY only if it has NO patch_intent and proposes no write.
3. For NO_WRITE_ADVISORY, emit a NON-APPROVABLE operator-review artifact:
     "No workspace write proposed." + advisory fields + approval/apply explicitly false.
4. For anything else (patch_intent present, unsafe, ambiguous, schema-invalid), refuse
   cleanly as OUT_OF_SCOPE / REJECT — do NOT transform it in this slice.
```

This slice proves "model proposes → operator reviews → A2 records a no-write preview
status." It adds **no** workspace-write capability.

## 4. Source of Truth

```text
docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md   (merged design; §8/§10/§15/§17)
schemas/a2-l4/planner-output.schema.json                       (a2-l4-planner-output.v1)
scripts/validate_planner_output_schema.py                      (existing read-only validator)
scripts/pretty_print_planner_output.py                         (existing read-only pretty-printer)
rust/crates/a2-plan-schema/src/plan_schema.rs                  (A2 plan shape — reference only)
rust/crates/a2-plan-runner/src/diff_preview.rs                 (preview_sha256 boundary — reference only)
```

The implementation reuses the existing validator for input conformance; it does not
re-implement schema validation.

## 5. Hard Boundaries

The implementation MUST NOT:

```text
produce a workspace-write preview
call claw plan preview / approve / apply, or any A2 apply path
produce a write preview_sha256
write/modify any source file the transform is meant to operate on (it emits an artifact only)
run model inference or call the broker / :11435 / :11434
invent write_target or after_file
make a no-write advisory artifact approvable or applyable
broaden to the workspace-write (Category B) transform
mutate runtime / services / Vault / secrets
```

Allowed: add a bounded transform module + fixtures + tests; read the planner-output
schema/validator; emit a review artifact to a caller-specified output path or stdout.

LAW 1: no app inference here at all; raw `:11434` may appear only as a rejection pattern.

## 6. Clean Worktree Setup

One lane = one worktree = one branch = one PR. Create a fresh Stack-Code worktree from
`origin/main` under `/mnt/vast-data/git-worktrees/...`; do not work in
`/home/suki/stack-code`. Run a preflight that STOPs on staged/dirty tracked changes.

## 7. Preflight

```text
verify approval token present (else STOP)
git: branch from origin/main; clean worktree; no staged changes
confirm docs/a2-l4-s2c-...-scope.md present on base (PR #72 ancestor of origin/main)
confirm schemas/a2-l4/planner-output.schema.json and the validator exist
```

## 8. Implementation Scope

Smallest viable surface (prefer a Python script sibling to the existing planner-output
tools, mirroring scripts/validate_planner_output_schema.py / pretty_print_planner_output.py,
unless review prefers a Rust crate):

```text
scripts/transform_no_write_advisory.py   (new) — read candidate, validate, classify, emit artifact
tests/a2_l4/test_transform_no_write_advisory.py (new) — fixtures-driven tests
schemas/a2-l4/fixtures/no-write-advisory/*.json (new) — fixtures (see §12)
```

Do NOT touch rust/, the runner, the CLI, the broker adapter, or the schema files.

## 9. Input Contract

Accept ONLY a candidate that:

```text
validates as a2-l4-planner-output.v1 via the existing validator (else REJECT_AMBIGUOUS)
has NO patch_intent field (patch_intent present => OUT_OF_SCOPE for S2C-1a)
has no workspace-write request / no write_target intent
has task_summary (objective) present and non-empty
has at least one proposed next step (operator_next_steps and/or plan_steps non-empty)
has risk_notes present (may be empty array but field present)
```

If `patch_intent` is present, classify `WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE` and refuse
in this slice (a later S2C-1b lane handles it). Never transform it here.

## 10. Classification Rules

```text
NO_WRITE_ADVISORY              valid planner-output, no patch_intent, no write proposed -> emit artifact
WORKSPACE_WRITE_PREVIEWABLE    patch_intent present -> OUT_OF_SCOPE for S2C-1a; clean refusal
REJECT_UNSAFE                  apply/approve/run/shell request, A2 bypass, raw :11434, dangerous cmd,
                               runtime-service mutation -> clean refusal (also schema-forbidden)
REJECT_AMBIGUOUS               schema-invalid, missing objective/next-step, ambiguous/non-workspace-
                               relative target -> clean refusal
```

Classification is total and deterministic: every input lands in exactly one class. Refusals
are clean non-zero exits with a reason code; never a partial/forced artifact.

## 11. Output Artifact Contract

For `NO_WRITE_ADVISORY`, emit a JSON artifact (to stdout or a caller-specified `/tmp` /
ignored path — never overwriting a source file) with at least:

```json
{
  "artifact_type": "no_write_advisory_review",
  "schema_version": "a2-l4-no-write-advisory-review.v1",
  "approval_allowed": false,
  "apply_allowed": false,
  "workspace_write_preview": false,
  "preview_sha256": null,
  "message": "No workspace write proposed.",
  "source_candidate_path": "<path>",
  "source_candidate_sha256": "<hash of the candidate bytes>",
  "objective": "<task_summary>",
  "assumptions_or_plan_summary": ["<plan_steps text ...>"],
  "proposed_next_steps": ["<operator_next_steps ...>"],
  "risks": ["<risk_notes ...>"],
  "candidate_files_to_inspect": ["<candidate_files ...>"],
  "operator_review_notes": "Read-only advisory. Inspect-only next step. A2 preview/approve/apply not entered."
}
```

Invariants the artifact MUST hold: `approval_allowed=false`, `apply_allowed=false`,
`workspace_write_preview=false`, `preview_sha256=null`/absent-for-write, `message` exactly
"No workspace write proposed.". The artifact is **non-approvable**: nothing downstream
(plan approve / plan apply) may consume it as an approval, and it carries no write
`preview_sha256`.

## 12. Fixture Plan

Add fixtures under `schemas/a2-l4/fixtures/no-write-advisory/`:

```text
valid-no-write-advisory.json            -> NO_WRITE_ADVISORY (artifact emitted, non-approvable)
has-patch-intent.json                   -> WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE (refused this slice)
unsafe-apply-request.json               -> REJECT_UNSAFE (schema-forbidden apply field)
raw-11434-reference.json                -> REJECT_UNSAFE (raw upstream endpoint)
missing-objective.json                  -> REJECT_AMBIGUOUS (no task_summary)
ambiguous-no-next-step.json             -> REJECT_AMBIGUOUS (no proposed next step)
```

Tests must assert the exact class AND, for the valid case, the artifact invariants in §11.

## 13. Validation Plan

```text
classification is total + deterministic across all fixtures
valid case emits an artifact with approval_allowed=false, apply_allowed=false,
  workspace_write_preview=false, preview_sha256 null/absent, exact message
no fixture path produces an approvable artifact or a write preview_sha256
no source file is modified by running the transform (output is a bounded artifact)
no model/broker call; no claw plan preview/approve/apply invoked
patch_intent fixture is refused (not transformed) in this slice
repo CI green (cargo fmt/test/clippy if Rust touched — it should not be; shell/py tests;
  planner-output validator; docs source-of-truth)
```

## 14. STOP Gates

STOP — escalate, never reframe — if the implementation would:

```text
touch any A2 apply path / run claw plan preview|approve|apply
call a model or the broker (:11435), or reference raw :11434 as an app-inference route
make a no-write advisory artifact approvable or applyable
generate a write preview_sha256
invent a write_target or after_file
broaden to the workspace-write (Category B) transform
edit rust/ runner/CLI/schema or the broker adapter
mutate runtime / services / Vault / secrets
proceed without the exact approval token
```

## 15. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | PARTIAL | BLOCKED | FAIL
MODE: S2C_1A_NO_WRITE_ADVISORY_TRANSFORM_IMPL
APPROVAL: token present / exact:
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
INPUT CONTRACT: validator reused / patch_intent excluded / required fields:
CLASSIFICATION: classes implemented / total+deterministic:
OUTPUT ARTIFACT: artifact_type / approval_allowed=false / apply_allowed=false /
  workspace_write_preview=false / preview_sha256 null / exact message:
FIXTURES: list / each class covered:
TESTS: pass/fail / CI state:
SAFETY: A2 apply touched / preview run / model|broker call / write preview_sha256 /
  write_target|after_file invented / runtime touched / source files modified:
AUTHORITY: A2 remains only write authority / no-write artifact non-approvable:
STOP GATES HIT: none | details
NEXT BEST LANE:
```

A2 remains the only write authority. The model proposes; the operator approves; A2
applies. This slice only records a no-write advisory review status and never enters the
preview/approve/apply chain.
