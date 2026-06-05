# A2-L4-S2C-1 Scope Card — Planner-Output → A2-Plan Transform (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT IMPLEMENTED.** This card designs the missing
> bridge from an A2-L4 **planner-output** candidate to an A2-L2a/L2b **plan** that the
> existing preview/approve/apply chain can consume. It implements no transform, runs no
> `claw plan` preview/approve/apply, calls no model or broker, edits no runner/schema/
> adapter code, and mutates no runtime. Created 2026-06-04.

> Docs location note: Stack-Code keeps scope cards flat under `docs/` (e.g.
> `docs/a2-l4-s2b5-…`, `docs/a2-l2c-scope-card.md`); there is **no** `docs/runbooks/`
> directory in this repo, so this card lands at `docs/a2-l4-s2c-…` to match convention.

---

## 1. Executive Summary

The A2-L4 advisory loop now proves, live, that a local model **proposes** a planner
candidate and an operator **reviews** it. The next link — **A2 previews** — is blocked
by a real gap: the A2 preview/approve/apply chain consumes an **A2 plan**
(`a2-plan-schema`: `Plan { name, steps[] }`, each `PlanStep` carrying `mode`, `tools`,
`write_target`, `after_file`, `expected_post_write`), while the planner emits an
**inert, descriptive planner-output document** (`a2-l4-planner-output.v1`) that carries
no `tools`, no `write_target`, and no exact after-bytes. No transform between the two
exists.

This card designs that transform **in docs only**. It classifies candidates, proposes a
field mapping, and — critically — separates the two cases:

- **No-write advisory candidate** (like the captured live smoke): produce an
  **operator-review artifact** that records "No workspace write proposed"; it is
  **never** approvable and produces **no** write `preview_sha256`.
- **Workspace-write candidate**: only via an explicit `patch_intent` + operator-supplied
  exact after-bytes, mapped into an A2 workspace-write plan step, then run through the
  **existing** A2 preview (which alone produces the approval-binding `preview_sha256`).

The recommended first slice is the **no-write advisory** transform (S2C-1a): it matches
the captured candidate, needs no `write_target`/`after_file` design, writes no source,
and reaches no apply path. The A2-L2b `preview_sha256`-bound approve/apply chain remains
the **only** write authority throughout.

---

## 2. Status and Scope

**DOCS-ONLY SCOPE CARD — NOT IMPLEMENTED.**

In scope: design of the planner-output → A2-plan transform — candidate classification,
field mapping, no-write vs. workspace-write handling, rejection conditions, and the
preview/approve/apply authority boundary.

Out of scope (this card authorizes none of these):

```text
transform implementation (Rust or Python)
edits to a2-plan-runner / a2-plan-schema / rusty-claude-cli / the planner adapter
running claw plan preview / approve / apply
A2 apply
live model inference / broker calls
model load/switch/evict, VRAM clear, service restart
raw localhost:11434 app inference
```

The A2-L2b preview/approve/apply chain remains the only write authority.

---

## 3. Prior Blocker Summary

The A2-L4-S2C lane (consume captured candidate → A2 preview) was correctly **BLOCKED**:

```text
candidate parsed OK and was advisory/read-only and operator-reviewable
A2 preview expects an A2 plan (a2-plan-schema), not a planner-output document
no planner-output → A2-plan transform exists
the candidate proposed a read-only inspection (no write_target), so the
  workspace-write diff-preview had nothing to render
```

Two-layer gap (this card addresses both):

1. **Schema gap** — planner-output (`a2-l4-planner-output.v1`) ≠ A2 plan (`a2-plan-schema`).
2. **Shape gap** — the captured live candidate (`{objective, assumptions,
   proposed_next_step, risks}`) matched the smoke's *ad-hoc task instruction*, not even
   the formal `a2-l4-planner-output.v1` contract. A production transform must consume the
   **validated** `a2-l4-planner-output.v1` document (via the existing
   `validate_planner_output_schema.py`), not free-form model text.

---

## 4. Source of Truth

- Planner-output contract: `schemas/a2-l4/planner-output.schema.json`
  (`a2-l4-planner-output.v1`) + read-only validator `scripts/validate_planner_output_schema.py`
  + pretty-printer `scripts/pretty_print_planner_output.py`.
- A2 plan schema: `rust/crates/a2-plan-schema/src/plan_schema.rs`
  (`Plan`, `PlanStep`, `PlanMode`, `WriteTarget`, `ExpectedPostWriteContract`) +
  `plan_validate.rs`.
- A2 preview/approve/apply: `rust/crates/a2-plan-runner/src/{diff_preview,write_preview,
  approval,runner}.rs` (`build_preview`, `produce_write_preview`, `PreviewRecord`,
  `preview_sha256`, `canonical_preview_record_for_approval`).
- CLI surface: `rust/crates/rusty-claude-cli/src/main.rs` — `claw plan …
  --workspace-write-preview`, `plan preview-bundle`, `plan approve`, `plan apply`,
  `plan apply-bundle` (preview/approve/apply are separate, gated subcommands).
- Captured candidate evidence: `/tmp/rhsmoke_out_20260604_163714.txt`
  (resident-holder live smoke; advisory, read-only inspection, no write).
- Parent cards: `docs/a2-l4-s2b5-live-broker-smoke-scope-card.md`,
  `docs/a2-l4-s2a-planner-output-contract-scope-card.md`, the A2-L2b preview/approve/apply
  cards.

---

## 5. Current Planner-Output Shape

`a2-l4-planner-output.v1` (closed object, `additionalProperties:false`) —

Required: `schema_version`, `task_id`, `workspace_root`, `task_summary`,
`plan_steps[]` (ordered **inert descriptive** text — never an argv/command),
`risk_notes[]`, `operator_next_steps[]`.

Optional: `repo_context_summary`, `candidate_files[]` (inspect-only paths),
`test_suggestions[]`, `patch_intent`, `preview_request`, `external_verifier_handoff`,
`status_snapshot`.

Hard-forbidden (schema `not` + closed object): `approval_line`, `approval_command`,
`apply_command`, `apply_bundle_command`, `run_command`, `shell_command`, … The document
is **inert**: conformance is necessary for well-formedness and **never sufficient for
action**. It carries no executable command, no approval line, no raw `:11434` endpoint,
and no secret.

Key consequence: a planner-output proposes **intent and description**, not the exact
executable specifics (tool list, exact after-bytes) an A2 write step requires.

---

## 6. Current A2 Plan / Preview Shape

`a2-plan-schema` — `Plan { name, mode, model_tier, steps[] }`. Each `PlanStep`:

```text
id, description
mode: read-only | workspace-write            (read-only is the default)
model_tier: FAST | DEEP
tools: [..]                                   (must be explicitly declared)
expected_output { must_contain[] }
write_target { path, create_if_absent }       (workspace-write only)
after_file: <workspace-relative path>         (REQUIRED for workspace-write,
                                               FORBIDDEN for read-only) — its bytes
                                               are the exact after-bytes for the write
expected_post_write { must_contain[], must_not_contain[] }
```

Preview: `produce_write_preview` / `build_preview` operate on **workspace-write** steps
(those carrying `write_target` + `after_file`) and emit a `PreviewRecord` +
sanitized `PreviewDisplay`, bound by `preview_sha256`. A **read-only** step has no
proposed write and therefore **nothing to diff-preview**.

---

## 7. Transform Problem

The transform must reconcile two facts:

1. Planner-output is **descriptive/inert**; A2 plan steps need **executable specifics**
   (`tools`, `write_target`, `after_file` exact bytes). These cannot be safely
   auto-synthesized from advisory prose without **inventing intent**.
2. The A2 workspace-write preview requires an actual proposed write (`write_target` +
   `after_file`). A purely advisory/read-only candidate has none — forcing a write
   preview would fabricate a change the model never proposed.

Therefore the transform is **not** a blind field copy. It is a **classifier + bounded
mapper** that (a) produces an operator-review artifact for no-write candidates, and
(b) for write candidates, produces only an A2 plan **skeleton** whose exact after-bytes
and tool list are **operator-supplied/operator-confirmed**, never machine-invented.

---

## 8. Candidate Classification

The transform must classify every candidate into exactly one:

```text
NO_WRITE_ADVISORY          valid planner-output; no patch_intent / no write proposed
                           (read-only inspection, candidate_files, operator_next_steps).
                           -> operator-review artifact; NOT approvable; no write sha256.

WORKSPACE_WRITE_PREVIEWABLE valid planner-output WITH an explicit patch_intent naming a
                           bounded write_target, AND an operator-supplied exact after-
                           bytes source. -> A2 workspace-write plan step -> existing A2
                           preview produces PreviewRecord + preview_sha256.

REJECT_UNSAFE              requests apply/approve/run/shell, bypasses A2, references raw
                           :11434, proposes dangerous commands, or asks to modify runtime
                           services. -> rejected before any preview.

REJECT_AMBIGUOUS           missing required fields, no objective/task_summary, no
                           proposed next step, broad/undefined refactor, ambiguous or
                           non-workspace-relative target, or a write that cannot be
                           represented without inventing intent. -> rejected.
```

Classification runs **after** the existing `validate_planner_output_schema.py` confirms
the candidate is a conforming, inert `a2-l4-planner-output.v1` document. A non-conforming
candidate (e.g. the ad-hoc smoke shape) is `REJECT_AMBIGUOUS` until it is re-emitted in
the contract shape.

---

## 9. Field Mapping Proposal

| planner-output field | A2-plan destination | Mapping rule |
|----------------------|--------------------|--------------|
| `task_id` / `task_summary` | `Plan.name` / step `description` context | Direct, inert text. |
| `workspace_root` | transform scope / workspace check | Validator enforces workspace-relative, no-escape. |
| `plan_steps[]` (descriptive) | `PlanStep.description` | Direct text only — **never** auto-derive `tools`/`mode`. |
| `candidate_files[]` | review artifact "inspect" list (Category A) | Read-only; never a `write_target`. |
| `risk_notes[]` | review artifact "risks" | Carried for operator review. |
| `operator_next_steps[]` | review artifact "next steps" | Carried for operator review. |
| `repo_context_summary` | review artifact context | Inert text. |
| `test_suggestions[]` | review artifact / step `expected_output` hint | Descriptive; never an auto-runner. |
| `patch_intent` (optional) | **gateway** to a workspace-write step | Only field that can justify `mode: workspace-write`; still needs operator-supplied `after_file` + explicit `write_target` (`create_if_absent`) + `expected_post_write`. |
| `preview_request` (optional) | request to enter A2 preview | Advisory request only; the operator, not the model, authorizes preview. |
| `tools` (N/A in planner-output) | `PlanStep.tools[]` | **Operator judgment** — cannot be auto-mapped; must be explicit. |
| exact after-bytes (N/A) | `PlanStep.after_file` bytes | **Operator-supplied/materialized** — never machine-invented. |

"Cannot be mapped automatically": `tools`, `after_file` exact bytes, and `mode:
workspace-write` selection. These require operator confirmation by design.

---

## 10. Read-Only / No-Write Candidate Handling (Category A)

For candidates like the captured live smoke (read-only `proposed_next_step`, no
`write_target`):

```text
Do NOT force a workspace-write diff-preview.
Produce an OPERATOR-REVIEW artifact that states: "No workspace write proposed."
The artifact MAY include: objective/task_summary, assumptions/risk_notes,
  proposed next step(s), candidate_files to inspect, source candidate path,
  and a review recommendation.
The artifact MUST be non-approvable: it carries NO write preview_sha256 and cannot be
  fed to plan approve / plan apply.
It is the read-only terminus of "model proposes → operator reviews → A2 records a
  no-write preview status."
```

This is the cheapest, safest slice and matches today's evidence.

---

## 11. Workspace-Write Candidate Handling (Category B)

For a future candidate proposing a bounded edit, ALL must hold before any A2 preview:

```text
the planner-output validates as a2-l4-planner-output.v1 (inert)
it carries an explicit patch_intent naming a single bounded write_target
the write_target path is workspace-relative with no escape (validator-enforced)
the operator supplies/confirms the exact after-bytes (after_file); the model does NOT
  invent them
the step is small-scope (one bounded file edit), mode: workspace-write, tools explicit
expected_post_write (must_contain / must_not_contain) is set for operator review
```

Then, and only then, the transform emits an `a2-plan-schema` Plan, and the **existing**
A2 preview (`produce_write_preview` / `claw plan … --workspace-write-preview` or
`plan preview-bundle`) computes the `PreviewRecord` + `preview_sha256`. The transform
itself never previews, approves, or applies.

---

## 12. Rejection / STOP Conditions

Reject (no preview produced) if the planner-output:

```text
requests direct apply/approve (apply_command, approval_line, …) — also schema-forbidden
bypasses A2 (proposes its own write/run/shell path)
lacks objective / task_summary
lacks a proposed next step (empty plan_steps / operator_next_steps)
proposes a broad/undefined refactor or multi-file sweep
proposes dangerous commands (rm -rf, git clean, reset --hard, find -delete, git add -A)
references a raw localhost:11434 app-inference endpoint
has an ambiguous or non-workspace-relative target
asks to modify runtime services / broker / Vault / secrets
proposes a write whose exact after-bytes cannot be represented without inventing intent
fails a2-l4-planner-output.v1 schema validation
```

A rejection is an escalation to the operator, never a retry of the forbidden action in
different framing.

---

## 13. Preview Authority Boundary

```text
Preview is NON-AUTHORITATIVE. The human-readable PreviewDisplay binds nothing.
Only the PreviewRecord (and the preview_sha256 it pins) can later bind an approval.
A NO_WRITE_ADVISORY artifact carries NO write preview_sha256 and is non-approvable.
A WORKSPACE_WRITE preview_sha256 is produced ONLY by the existing A2 preview, over an
  actual write_target + after_file — never fabricated by the transform.
```

---

## 14. Approval / Apply Boundary

```text
Approval (claw plan approve / run_plan_approve) binds to a specific preview_sha256 and
  writes no target files.
Apply (claw plan apply / apply-bundle) validates the full authority chain and is the
  only step that mutates a workspace file.
The transform NEVER approves or applies and NEVER emits an apply bundle.
A2 remains the only write path; the model proposes, the operator approves, A2 applies.
```

---

## 15. Validation Plan

A future S2C-1a implementation (separately scoped/approved) must demonstrate:

```text
input is a validated a2-l4-planner-output.v1 document (or it is REJECT_AMBIGUOUS)
classification is deterministic and total (every candidate lands in exactly one class)
NO_WRITE_ADVISORY produces a review artifact that is provably non-approvable
  (no write preview_sha256, cannot be fed to approve/apply)
no source files are written by the transform (output is a bounded artifact)
no model call, no broker call, no claw plan preview/approve/apply is invoked by the
  transform itself
REJECT_* cases produce a clean refusal, never a partial/forced preview
fixtures cover: the captured no-write candidate, a patch_intent write candidate, and
  each rejection condition
```

CI must not run live inference or apply; transform tests use fixtures only.

---

## 16. Implementation Options

```text
Option 1 — No-write advisory review-artifact transform first  [RECOMMENDED]
  Implement only Category A: validated planner-output (no patch_intent) -> operator-
  review artifact ("No workspace write proposed"), non-approvable. Smallest surface,
  matches today's evidence, no write_target/after_file design needed.

Option 2 — Full classifier + workspace-write skeleton
  Implement A + B + rejection in one slice. Larger: needs patch_intent → write_target/
  after_file mapping, operator after-bytes handoff, and A2-preview wiring. Defer until a
  real write candidate exists.

Option 3 — Broaden the planner-output schema to embed plan steps directly  [REJECTED]
  Make planner-output carry a2-plan-schema steps. Rejected: collapses the inert-proposal
  / executable-plan boundary and risks a planner-output implying executable authority,
  which a2-l4-planner-output.v1 explicitly forbids.
```

---

## 17. Recommended First Slice

**S2C-1a — No-write advisory review-artifact transform (Option 1).** Rationale:

```text
matches the captured resident-holder candidate (read-only, no write_target)
requires no write_target / after_file / tools design
writes no source files; reaches no approve/apply path
proves "model proposes → operator reviews → A2 records a no-write preview status"
graduates cleanly to S2C-1b (workspace-write) only when a real patch_intent candidate
  and an operator after-bytes handoff exist
```

Do **not** implement the workspace-write transform first; there is no write-candidate
evidence yet, and Category B needs the operator after-bytes design.

---

## 18. Follow-On Lanes

```text
1. Read-only review of THIS scope card, then push/merge (docs-only).
2. (If approved) S2C-1a implementation: no-write advisory review-artifact transform,
   fixtures-only, no model/preview/apply — separately scoped and approved.
3. (Later) S2C-1b: workspace-write transform behind patch_intent + operator after-bytes
   handoff, wiring into the existing A2 preview to produce preview_sha256.
4. (Later) S2D: Operator Review → A2 Approval gate (binds preview_sha256), then S2E apply.

Do NOT implement any transform until this scope card is reviewed and merged.
```

---

## 19. Status Block

```text
status: DOCS-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not implement a transform or edit runner/schema/adapter/broker code.
It does not run claw plan preview/approve/apply or any A2 apply.
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It does not make a no-write advisory candidate approvable, and it never fabricates a
  write preview_sha256.
It does not introduce a raw localhost:11434 app-inference path.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write
authority. The model proposes; the operator approves; A2 applies.
Next gate: read-only operator review of this scope card.
```
