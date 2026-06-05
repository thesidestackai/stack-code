# DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL

⚠️ REVIEW REQUIRED: This is a **future implementation prompt**, authored 2026-06-05 by the
S2C-1b prompt-drafting lane. It has **not** been run. It implements the **second** slice of
the planner-output → A2-plan transform: the **workspace-write-previewable** case that S2C-1a
deliberately deferred. It must be reviewed and merged, then invoked only with the exact
operator approval token below, before any code is written.

Implements: `docs/a2-l4-s2c1b-planner-write-preview-scope.md` (merged PR #75,
`812a7c4d12eedc9dd1d648a699827ed517e1ca0b`) and the parent
`docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md` (§8 class
`WORKSPACE_WRITE_PREVIEWABLE`, §11, §13).

> Convention note: Stack-Code keeps handoff-style implementation prompts under `handoffs/`
> (e.g. `s2c1a_no_write_advisory_transform_implementation_prompt_DRAFT_2026-06-04.md`). This
> draft follows that convention; a reviewer may relocate it to `docs/` if the repo prefers a
> single docs tree.

---

# CLAUDE CODE PROMPT — S2C-1b Planner Write-Preview Transform (Implementation)

## 1. Status and Approval Requirement

Implements the workspace-write-previewable slice of the planner-output → A2-plan transform:
a validated `a2-l4-planner-output.v1` candidate **with an explicit, bounded `patch_intent`**
→ an **A2-plan preview-request SKELETON** that the **existing** A2-L2b chain can take to
**preview only**. The transform itself runs no preview, no approve, no apply; it fabricates
no `preview_sha256`; it never invents `after_file` bytes.

Do **not** begin unless the operator has provided this **exact** token in the current
instruction:

```text
APPROVED: Execute S2C-1b planner write-preview transform implementation
```

If that exact token is missing, STOP immediately and report:

```text
BLOCKED: missing required approval token.
```

This prompt is DRAFT ONLY. It does not authorize immediate execution; approval is mandatory
and is never optional.

## 2. Role

You are a careful Stack-Code transform implementation engineer. Follow:
OBSERVE → VERIFY → TDD (test-first) → IMPLEMENT MINIMAL → VALIDATE → REPORT.

## 3. Objective

Add the smallest code + fixtures + tests that:

```text
1. Accept a validated a2-l4-planner-output.v1 document with an explicit patch_intent.
2. Classify it WORKSPACE_WRITE_PREVIEWABLE only when it is safe, single-target, bounded,
   workspace-relative, and representable WITHOUT inventing intent.
3. For WORKSPACE_WRITE_PREVIEWABLE, emit an A2-plan preview-request SKELETON (one
   workspace-write step) plus a NON-APPROVABLE envelope: approval_allowed=false,
   apply_allowed=false, preview_sha256=null, with an OPERATOR-SUPPLIED after_file PLACEHOLDER.
4. For anything else (no patch_intent → NO_WRITE_ADVISORY is S2C-1a's job; unsafe; ambiguous;
   under-specified; multi-file; schema-invalid), refuse cleanly. Never force a preview.
```

This slice prepares a previewable candidate; the existing A2-L2b chain alone produces the
approval-binding `preview_sha256`. It adds **no** approve/apply capability.

## 4. Source of Truth

```text
docs/a2-l4-s2c1b-planner-write-preview-scope.md              (merged design, PR #75 — THIS slice)
docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md  (parent design; §8/§11/§13)
docs/a2-plan-schema.md                                        (A2 plan / write_target / after_file rules)
docs/a2-l2b-run-plan-preview-operator-handoff.md              (preview_sha256 / PreviewRecord authority)
schemas/a2-l4/planner-output.schema.json                      (a2-l4-planner-output.v1; patch_intent is closed/prose-only)
scripts/validate_planner_output_schema.py                     (existing read-only validator — REUSE, do not re-implement)
scripts/transform_no_write_advisory.py                        (S2C-1a sibling — mirror style; do NOT edit)
rust/crates/a2-plan-schema/src/plan_schema.rs                 (A2 plan shape — reference only)
rust/crates/a2-plan-runner/src/{diff_preview,write_preview}.rs (preview_sha256 boundary — reference only)
```

Load-bearing fact: `patch_intent` is a **closed object carrying only `summary` + `notes`
(prose)**; it is schema-forbidden from carrying an applyable replacement body. Therefore the
candidate contains **no exact after-bytes**; the A2 step's `after_file` is operator-supplied,
never machine-derived.

## 5. Hard Boundaries

The implementation MUST NOT:

```text
run claw plan preview / approve / apply, or any A2 apply path
produce, fabricate, or claim a preview_sha256 / PreviewRecord
create a preview hash outside the existing A2 preview authority
make its artifact approvable or applyable
approve anything or apply anything
invent a write_target path or invent after_file post-write bytes
write/modify any workspace source file (it emits a bounded artifact only)
run model inference or call the broker / :11435 / :11434
reference raw :11434 except as a rejection pattern
broaden beyond single-file workspace-write candidates (multi-file/broad-refactor is out of scope)
edit rust/ runner/CLI/apply/schema code or the broker adapter
mutate runtime / services / Vault / secrets
```

Allowed: add a bounded transform module + fixtures + tests; read the planner-output
schema/validator and the A2 plan schema docs; emit an A2-plan skeleton artifact to a
caller-specified output path or stdout.

LAW 1: no app inference here at all; raw `:11434` may appear only as a rejection pattern.

## 6. Clean Worktree Setup

One lane = one worktree = one branch = one PR. Create a fresh Stack-Code worktree from
`origin/main` under `/mnt/vast-data/git-worktrees/...`; do not work in `/home/suki/stack-code`.
Run a preflight that STOPs on staged/dirty tracked changes.

## 7. Preflight

```text
verify the exact approval token is present (else STOP: BLOCKED missing approval token)
git: branch from origin/main; clean worktree; no staged changes
confirm docs/a2-l4-s2c1b-planner-write-preview-scope.md present on base (PR #75 ancestor of origin/main)
confirm schemas/a2-l4/planner-output.schema.json and the validator exist
confirm scripts/transform_no_write_advisory.py exists (S2C-1a sibling for style) — read-only
```

## 8. Implementation Scope

Smallest viable surface (prefer a Python script sibling to the existing planner-output
tools, mirroring `scripts/transform_no_write_advisory.py`, unless review prefers a Rust
crate):

```text
scripts/transform_write_previewable.py              (new) — read candidate, validate, classify, emit A2-plan skeleton
tests/a2_l4/test_transform_write_previewable.py     (new) — stdlib unittest, fixtures-driven
schemas/a2-l4/fixtures/write-previewable/*.json      (new) — fixtures (see §10)
```

Constraints:

```text
reuse scripts/validate_planner_output_schema.py read-only; do not re-implement validation
tests MUST be stdlib unittest (CI runs `python -m unittest discover`; NO pytest dependency)
keep S2C-1a's no-write-advisory transform + fixtures UNCHANGED (NO_WRITE_ADVISORY stays separate)
do NOT touch rust/, the a2-plan-runner, the CLI, the broker adapter, or the schema files
```

## 9. Input Contract

Accept ONLY a candidate that:

```text
validates as a2-l4-planner-output.v1 via the existing validator (else REJECT_AMBIGUOUS)
carries an explicit patch_intent (prose summary + notes; NO applyable body — schema-enforced)
names exactly ONE bounded, workspace-relative write_target (no absolute, no `..`,
  no .git/.claw/.claude/.env*/secret*/credentials*/*.pem/*.key)
is accompanied by an OPERATOR-SUPPLIED after_file PATH (placeholder accepted; bytes NOT
  provided by the transform; after_file path != write_target.path)
has task_summary (objective) present and non-empty
has at least one proposed next step (operator_next_steps and/or plan_steps non-empty)
requests no apply/approve/run/shell action; references no raw :11434; targets no runtime service
```

A candidate with NO `patch_intent` is **not** this slice's input — it is `NO_WRITE_ADVISORY`,
already handled by S2C-1a. A `patch_intent` candidate that is unsafe, ambiguous,
under-specified, or multi-file is refused (see §14), never transformed.

## 10. Fixture Contract

Add fixtures under `schemas/a2-l4/fixtures/write-previewable/` (one expected class each):

```text
valid-single-file-write-previewable.json   -> WORKSPACE_WRITE_PREVIEWABLE (skeleton emitted, non-approvable)
missing-write-target.json                   -> REJECT_AMBIGUOUS (patch_intent but no bounded target)
ambiguous-target.json                       -> REJECT_AMBIGUOUS (vague / undefined target)
absolute-path-target.json                   -> REJECT_AMBIGUOUS / path refusal (absolute write_target)
target-outside-workspace.json               -> REJECT_AMBIGUOUS / path refusal (`..` escape / non-relative)
raw-11434-reference.json                     -> REJECT_UNSAFE (raw upstream endpoint)
dangerous-command-request.json               -> REJECT_UNSAFE (shell/exec/apply request)
multi-file-write-intent.json                 -> REJECT_AMBIGUOUS / OUT_OF_SCOPE (more than one target)
patch-intent-without-after-bytes.json        -> REJECT_AMBIGUOUS (no operator after_file source supplied)
preview-sha256-bypass.json                   -> REJECT_UNSAFE (claims a preview hash / pre-approval)
```

Note in the prompt and tests: a read-only / no-write candidate is **not** an S2C-1b case —
it remains S2C-1a's `NO_WRITE_ADVISORY`. Tests must assert the exact class AND, for the valid
case, the skeleton + non-approvable envelope invariants in §11/§12/§13.

## 11. Transform Contract

The future transform must:

```text
classify input deterministically and totally (every candidate lands in exactly one class)
produce an A2-plan workspace-write step ONLY when all required fields are explicit/safe/single/bounded
set mode = workspace-write
set tools = [Write]   (workspace-write steps must declare Write)
set write_target.path (operator-confirmed, workspace-relative, lexically safe) + create_if_absent
carry after_file as an OPERATOR-SUPPLIED PATH PLACEHOLDER (never fill its bytes)
set expected_post_write { must_contain[], must_not_contain[] } (carried from intent; advisory)
include a human-readable step description
preserve operator-review notes (objective, assumptions/notes, risks, candidate_files)
NOT run preview itself (unless the reviewed prompt explicitly limits it to offline fixture
  shape-validation of the emitted plan; even then, no preview_sha256 is produced)
NOT approve, NOT apply, NOT emit an apply bundle
```

Refusals are clean non-zero exits with a reason code; never a partial/forced skeleton.

## 12. A2 Plan Output Contract

For `WORKSPACE_WRITE_PREVIEWABLE`, emit a JSON artifact (to stdout or a caller-specified
output path — never overwriting a source file) of shape:

```json
{
  "artifact_type": "workspace_write_preview_request",
  "schema_version": "a2-l4-write-preview-request.v1",
  "approval_allowed": false,
  "apply_allowed": false,
  "workspace_write_preview": false,
  "preview_sha256": null,
  "operator_action_required": "supply after_file bytes, then run A2-L2b: claw plan run --workspace-write-preview",
  "plan": {
    "name": "<from task_id / task_summary>",
    "mode": "workspace-write",
    "model_tier": "FAST",
    "steps": [
      {
        "id": "s1",
        "description": "<inert description from patch_intent.summary + plan_steps>",
        "mode": "workspace-write",
        "tools": ["Write"],
        "write_target": { "path": "<workspace-relative, operator-confirmed>", "create_if_absent": false },
        "after_file": "<OPERATOR-SUPPLIED PATH PLACEHOLDER — bytes NOT provided by transform>",
        "expected_post_write": { "must_contain": [], "must_not_contain": [] }
      }
    ]
  },
  "source_candidate_path": "<path>",
  "source_candidate_sha256": "<hash of the candidate bytes>",
  "operator_review_notes": "Read-only preview-request skeleton. Non-approvable. Carries no write preview_sha256. A2-L2b remains the only write authority; supply after_file bytes and run --workspace-write-preview."
}
```

Invariants the artifact MUST hold: `approval_allowed=false`, `apply_allowed=false`,
`workspace_write_preview=false`, `preview_sha256=null`, exactly one workspace-write step,
`after_file` is an operator-supplied placeholder (transform never fills its bytes). The
artifact is **non-approvable**: nothing downstream (plan approve / plan apply) may consume it
as an approval, and it carries no write `preview_sha256`.

## 13. Preview Boundary

The future implementation MUST preserve, unchanged:

```text
the transform creates an A2-plan candidate (skeleton) ONLY
the EXISTING A2 preview (claw plan run --workspace-write-preview) creates the PreviewRecord
the EXISTING A2 preview computes preview_sha256 (payload/before/after sha)
the transform does NOT fabricate or emit preview_sha256
approval remains SEPARATE (claw plan approve, TTY-enforced) and binds to preview_sha256
apply remains SEPARATE (claw plan apply) and re-validates the full authority chain (one file)
```

The transform is upstream of and subordinate to this chain; it is never part of the
integrity/authority chain.

## 14. Rejection Rules

The transform must refuse (no artifact, deterministic non-zero exit) when the candidate:

```text
REJECT_UNSAFE
  requests direct apply/approve (apply_command, approval_line, …) — also schema-forbidden
  requests a shell/subprocess/curl/network execution primitive
  references a raw localhost:11434 app-inference endpoint
  attempts to BYPASS preview_sha256 (claims a preview hash / asserts pre-approval / supplies an apply bundle)
  asks to modify runtime services / broker / Vault / secrets / models
  names a write_target outside workspace policy (absolute / `..` / denied path)

REJECT_AMBIGUOUS
  fails a2-l4-planner-output.v1 schema validation
  lacks objective / task_summary
  lacks a proposed next step
  has patch_intent but no bounded single write_target
  has an ambiguous, multi-file, or non-workspace-relative target
  needs invented post-write bytes (no operator-supplied after_file source)
  proposes a broad/undefined refactor or multi-file sweep (unsupported; one target per bundle)
```

A rejection is an escalation to the operator, never a retry of the forbidden action reframed.

## 15. Validation Plan

```text
classification is total + deterministic across all fixtures
valid case emits a skeleton with approval_allowed=false, apply_allowed=false,
  workspace_write_preview=false, preview_sha256=null, exactly one workspace-write step,
  operator-supplied after_file placeholder
no fixture path produces an approvable artifact, an apply bundle, or a write preview_sha256
after_file bytes are never filled by the transform
no source file is modified by running the transform (output is a bounded artifact)
no model/broker call; no claw plan preview/approve/apply invoked by the transform
read-only/no-write candidate is NOT handled here (stays NO_WRITE_ADVISORY / S2C-1a)
tests are stdlib unittest (no pytest); CI is fixtures-only (no live inference/apply)
repo CI green (cargo fmt/test/clippy if Rust touched — it should NOT be; shell/py tests;
  planner-output validator; docs source-of-truth)
```

## 16. STOP Gates

STOP — escalate, never reframe — if the implementation would:

```text
proceed without the exact approval token
edit rust/ runner/CLI/apply path, the schema, or the broker adapter
run claw plan preview / approve / apply
call a model or the broker (:11435), or reference raw :11434 as an app-inference route
make the transform an authority to approve/apply
fabricate, claim, or emit a preview_sha256 / PreviewRecord outside A2 preview authority
invent a write_target path or invent after_file post-write bytes
allow an absolute or out-of-workspace write_target
broaden to multi-file / broad-refactor handling without a separate scope card
mutate runtime / services / Vault / secrets
```

## 17. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | PARTIAL | BLOCKED | FAIL
MODE: S2C_1B_WRITE_PREVIEWABLE_TRANSFORM_IMPL
APPROVAL: token present / exact:
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
INPUT CONTRACT: validator reused / patch_intent required / single bounded target / operator after_file:
CLASSIFICATION: classes implemented / total+deterministic:
TRANSFORM: one workspace-write step / mode / tools[Write] / write_target / after_file placeholder /
  expected_post_write / no preview run:
A2 PLAN OUTPUT: artifact_type / approval_allowed=false / apply_allowed=false /
  workspace_write_preview=false / preview_sha256 null / one step:
PREVIEW BOUNDARY: A2-L2b sole preview_sha256 author / transform fabricates none:
FIXTURES: list / each class covered / no-write stays S2C-1a:
TESTS: stdlib unittest / pass/fail / CI state:
SAFETY: A2 apply touched / preview run / approve|apply run / model|broker call /
  write preview_sha256 fabricated / write_target|after_file invented / runtime touched /
  source files modified / raw 11434 app inference:
AUTHORITY: A2-L2b remains only write authority / skeleton non-approvable:
STOP GATES HIT: none | details
NEXT BEST LANE:
```

A2-L2b remains the only write authority. The model proposes; the transform prepares a
previewable skeleton; the operator supplies after-bytes and approves; A2 applies. This slice
only emits a non-approvable preview-request skeleton and never enters the
preview/approve/apply chain or fabricates a `preview_sha256`.
