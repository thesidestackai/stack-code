# A2-L4-S2A-6 Scope Card — Planner-Output Pretty-Printer (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2A-6 lane.
It describes whether and how a future **read-only CLI pretty-printer**
for the planner-output contract should be implemented: what it reads,
what it prints, the surfaces a future implementation lane (A2-L4-S2A-7)
may and may not touch, and the boundaries it must hold (read-only,
no-write, no-command, no-model, no-approval). This file itself
authorizes **no runtime change, no CLI, no schema change, no validator
change, no model execution, no model load, no direct write, no approval
bypass, no model-generated approval line, and no raw `localhost:11434`
app inference**.

A2-L4-S2A-6 is the sixth sub-slice of A2-L4-S2A. Its predecessors:

- **S2A-1** ([`a2-l4-s2a1-json-schema-file-scope-card.md`](./a2-l4-s2a1-json-schema-file-scope-card.md))
  — docs-only card scoping the future schema file.
- **S2A-2** — landed `schemas/a2-l4/planner-output.schema.json` (PR #58).
- **S2A-3** — landed the fixture pack under
  `schemas/a2-l4/fixtures/planner-output/` (PR #60).
- **S2A-4** ([`a2-l4-s2a4-schema-validator-scope-card.md`](./a2-l4-s2a4-schema-validator-scope-card.md))
  — docs-only card scoping the read-only validator.
- **S2A-5** — landed `scripts/validate_planner_output_schema.py` and its
  tests, with CI coverage (PR #62).

This card carries exactly one step forward: it scopes a *future*
read-only pretty-printer — its input/output contracts, rendering rules,
and boundaries — but it still does **not** create the pretty-printer.
Creating it is the next lane (A2-L4-S2A-7), separately scoped and
reviewed.

### Must State

```text
This card authorizes design only.
It does not authorize pretty-printer implementation.
It does not authorize schema or validator changes.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2A-6 defines, in design only, the boundary a **future read-only
CLI pretty-printer** for planner output must hold to. The pretty-printer
would let an operator read a planner-output document as a clear,
human-friendly report — what the plan proposes, which files it touched,
its risk notes, and (critically) whether the document is valid and
inert — **before** the operator decides anything. The pretty-printer is
a *display surface*, never an *authority surface*: rendering a document
grants nothing, and a rendered document is still inert and still
requires operator judgement.

The recommended A2-L4-S2A-6 scope is:

> Pin the future pretty-printer's location, allowed/forbidden touched
> surfaces, input contract (one planner-output JSON path), output
> contract (a stdout-only operator report), the read-only / no-write /
> no-command / no-model / no-approval boundaries, the rendering rules,
> the invalid-input rendering behavior, and the secret-handling rule
> (field path only, never the value) — without creating the
> pretty-printer, and while the A2-L2b chain remains the only write
> authority.

The implementation of the pretty-printer is **not authorized by this
scope card**, and **no pretty-printer code is created by it**. This card
defines the boundary the future implementation lane (A2-L4-S2A-7) must
hold to. The next gate before any implementation is operator review of
this scope card.

## 2. Relationship To S2A Schema

The pretty-printer renders documents whose shape is fixed by the
planner-output JSON Schema
([`../schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json),
S2A-2):

- it renders the required fields (`schema_version`, `task_id`,
  `workspace_root`, `task_summary`, `plan_steps`, `risk_notes`,
  `operator_next_steps`) and any present optional fields
  (`repo_context_summary`, `candidate_files`, `test_suggestions`,
  `patch_intent`, `preview_request`, `external_verifier_handoff`,
  `status_snapshot`).
- it must **not** modify, re-version, or re-interpret the schema. The
  schema remains the single source of truth for shape; the pretty-printer
  is a consumer of it.
- it treats `schema_version` as a literal to display and to gate on (an
  unknown version is rendered as a refusal, §14), never as something to
  rewrite.

## 3. Relationship To S2A Validator

The pretty-printer is layered **on top of** the read-only validator
([`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py),
S2A-5):

- the pretty-printer **may import and use** the validator to decide
  whether a document is valid and inert before rendering it as
  "accepted".
- the validator remains the authority on accept/refuse; the
  pretty-printer never relaxes, overrides, or coerces a validator
  refusal into an "accepted" render.
- a validator refusal is rendered as a **refusal report** (§14), never
  as a partially-accepted or repaired document.
- the pretty-printer adds **no** new validation authority: it does not
  invent acceptance criteria the validator does not enforce, and a
  pretty-printed document is no more authorized for action than a
  validated one (which is to say: not at all).

## 4. What The Pretty-Printer Adds

A future pretty-printer adds, in design only:

1. A single, read-only, operator-facing **rendering** of a planner-output
   document.
2. A clear **validity banner** (valid-and-inert vs refused) derived from
   the validator, never from the pretty-printer's own judgement.
3. **Human-readable sections** for the plan steps, risk notes, candidate
   files, operator next steps, and the advisory fields.
4. A safe **invalid-input rendering** that names what failed without
   echoing forbidden payloads or secrets (§§14, 15).

It adds **no** execution, write, approval, or model capability.

## 5. What It Does Not Add

A2-L4-S2A-6 does **not** add:

- any pretty-printer code, script, or implementation (that is S2A-7)
- any change to the schema (S2A-2), the fixtures (S2A-3), or the
  validator (S2A-5)
- any write path, write command, or write flag
- any executable-command field, approval line, or apply affordance
- any change to `claw plan run/approve/apply-bundle/apply` or `claw plan
  status`
- any change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- any raw `localhost:11434` app-inference path
- any automatic model load, SGLang start, ComfyUI job, or GPU workload
- any new secret, key, or token surface

Any of the above must be opened as a separate, explicitly-authorized
lane. This card is **not** prior authorization for it.

## 6. Input Contract

The future pretty-printer must:

- accept **one** path to a planner-output JSON document (positional
  argument), or read one document from stdin.
- read it **read-only**.
- treat a missing file, unreadable file, or malformed JSON as an input
  error (nonzero exit, §14), never as an empty success.

It must **not**:

- accept a directory to walk-and-mutate
- accept flags that enable writing, approving, applying, or model calls
- accept a planner-output document as a source of commands to run (§9)

## 7. Output Contract

The future pretty-printer must:

- print an operator-friendly report to **stdout only**.
- print diagnostics/errors to stderr.
- exit `0` only when the document is valid and inert; exit nonzero when
  the document is refused or unreadable (§14).
- **never** write the report (or anything else) to a file on disk.

The report is for human reading. It is not a machine artifact other
tools act on, and it carries no authority field.

## 8. Read-Only Boundary

The pretty-printer is strictly read-only:

- it reads the input document and (optionally) the schema and validator.
- it opens **no** file for writing, appends to **no** file, and creates
  **no** file or directory.
- it leaves the input document byte-for-byte unchanged.
- it never touches `.claw/**` (§9) or any workspace path.

## 9. No-Write Boundary

The pretty-printer must never:

- write, edit, stage, or delete any file
- mutate `.claw/**` (the A2-L2b state tree) in any way
- emit an `approval-result.json`, `apply-bundle.json`, or any artifact a
  write-chain consumes
- persist its rendered report to disk

`.claw/**` is mutated only by the operator-gated A2-L2b
preview/approve/apply chain, never by a display tool.

## 10. No-Command Boundary

The pretty-printer must never:

- run a shell command, subprocess, or `claw plan
  run/approve/apply-bundle/apply`
- execute, eval, or shell-out anything contained in the planner-output
  document (`plan_steps`, `test_suggestions`, `preview_request`, etc. are
  inert text to **display**, never to run)
- run tests or builds itself

Anything that looks like a command in the document is rendered as
descriptive text, clearly marked as a suggestion for the operator, never
executed.

## 11. No-Model Boundary

The pretty-printer must never:

- call a model, the broker, or Ollama
- route to `localhost:11435` or `localhost:11434`
- perform any inference

It is a pure local text-rendering tool. The inference that *produces* a
planner output (future S2B lanes) routes through the SideStack broker at
`http://127.0.0.1:11435/v1`; a base URL at raw `:11434` is a refusal,
not a fallback. The pretty-printer performs no inference of any kind.

## 12. No-Approval Boundary

The pretty-printer must never:

- generate, template, or echo an approval line (`apply <step_id>
  <preview_sha256>`)
- produce or simulate an approval gesture
- present any output a downstream tool could mistake for an approval or
  apply authorization

Rendering a `preview_request: {requested: true}` field is displaying an
operator's *option to consider a preview*, never an approval and never an
instruction to run the chain.

## 13. Rendering Rules

The future pretty-printer should:

- lead with a **validity banner**: `VALID (inert)` or `REFUSED` derived
  from the validator (§3).
- render required fields first (task summary, plan steps), then optional
  advisory fields (risk notes, candidate files, test suggestions,
  patch-intent notes, preview request, external-verifier handoff,
  status snapshot).
- render `plan_steps` as an ordered, readable list (`step_id`,
  `description`, optional `rationale`).
- render arrays-of-text (risk notes, operator next steps) as bullet
  lists.
- clearly label advisory/optional fields as advisory (they carry no
  authority).
- mark anything that resembles a command or endpoint as **descriptive
  only — not executed** (§§10, 11).
- never reformat in a way that changes meaning or implies authorization.

## 14. STOP / Invalid Input Rendering

When the input is invalid (validator refusal, unknown `schema_version`,
malformed JSON, unreadable file):

- the pretty-printer renders a **REFUSED** report naming *which* check
  failed (e.g. "forbidden field `approval_line`", "unknown
  `schema_version`", "`:11434` endpoint refused", "path escape in
  `candidate_files[2]`").
- it exits **nonzero**.
- it **never** coerces, strips-and-accepts, repairs, or partially renders
  a refused document as "accepted".
- it **never** echoes a forbidden executable payload in a way that could
  be copy-run; forbidden fields are reported by name, not by
  reproducing their would-be command body.

A refusal is a display of "this output is not safe to act on", never a
prompt to retry the forbidden action.

## 15. Secrets / Sensitive Data Handling

- The pretty-printer introduces no new credential, key, or token surface.
- When the validator flags a secret/token/key pattern, the pretty-printer
  renders the **field path only** (e.g. "`risk_notes[0]`: secret-like
  value refused"), **never the matched value**.
- It never writes any value it read to a log, file, or external service
  (it writes nothing to disk at all, §9).
- An `external_verifier_handoff` is rendered as advisory, secret-free
  review input; if it somehow carries a secret it is a validator refusal
  (§14), rendered by field path only.

## 16. Future Implementation Constraints

The recommended next lane after this card is:

```text
A2-L4-S2A-7 Pretty-Printer Implementation
```

It must be **separately scoped and reviewed**, and bounded by this card.
Per-lane constraints:

- **S2A-7** — creates the §17 pretty-printer and its test module,
  implementing §§6–15. Touches only the §17 allowed surfaces; touches
  none of the §18 forbidden surfaces. Reads only — writes nothing,
  executes nothing, loads no model. Prefers stdlib; adds no dependency
  without operator approval. May import the S2A-5 validator.

No future lane under this card may authorize direct writes, model
execution, approval bypass, model-generated approval lines, or raw
`:11434` app inference.

## 17. Allowed Future Touched Surfaces

For the future pretty-printer lane (S2A-7), allow **only**:

```text
scripts/pretty_print_planner_output.py
tests/a2_l4/test_pretty_print_planner_output.py
docs/a2-l4-s2a6-planner-output-pretty-printer-scope-card.md
README.md
```

- the pretty-printer script — the read-only tool the future lane creates.
- the test module — exercises rendering against the S2A-3 fixtures.
- this scope-card doc — only if a small, reviewed clarification is needed.
- `README.md` — only if adding or updating one Documentation Map line.

The pretty-printer **may import** the existing
`scripts/validate_planner_output_schema.py` (read-only) but must **not
modify** it. If a repo-conventional location differs from the above, the
future lane re-scopes under its own review before using it.

## 18. Forbidden Future Touched Surfaces

The future pretty-printer lane must **not** touch:

```text
scripts/validate_planner_output_schema.py (modify — import-only is allowed)
schemas/a2-l4/planner-output.schema.json
schemas/a2-l4/fixtures/**
rust/**
ide/**
Cargo.toml
Cargo.lock
.github/** (beyond an already-green CI path filter, which is itself a
            separate operator-approved lane)
wrappers/**
bin/**
examples/**
SideStackAI/**
.claw/**
runtime configs
```

Touching any of the above (other than importing the validator read-only)
is scope drift and a STOP (§21).

## 19. Validation Requirements

Before a pretty-printer implementation (S2A-7) is accepted, its tests
must demonstrate:

```text
valid-minimal renders (exit 0)
valid-full renders (exit 0)
invalid fixture returns failure (nonzero exit)
forbidden approval_line displays refusal, not an executable line
forbidden raw_11434_endpoint displays refusal
secret-like fixture displays only the field path, not the secret value
no output file is written
no .claw mutation
the input document is left unchanged
```

Each positive case renders cleanly and exits `0`; each negative case is
rendered as a refusal and exits nonzero. CI coverage for the test module
(a Python test job + path filter) is a separate, operator-approved lane,
as it was for the validator.

## 20. Non-Goals

A2-L4-S2A-6 is explicitly **not**:

- a pretty-printer implementation (that is S2A-7)
- a change to the schema (S2A-2), fixtures (S2A-3), or validator (S2A-5)
- a planner implementation (that is S2 §22, S2B)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a patch generator or patch-proposal artifact (that is A2-L4-S3)
- a new write command, write flag, or approval-bypass affordance
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a SideStackAI infrastructure change

## 21. STOP Gates

Any future A2-L4-S2A-7 pretty-printer lane must STOP — escalate — and not
proceed if any of the following is true:

1. The tool would write, edit, stage, or delete any file (including
   `.claw/**`), or persist its report to disk.
2. The tool would run a shell command/subprocess, or execute anything
   contained in the planner-output document.
3. The tool would call a model, the broker, or Ollama, or route to raw
   `localhost:11434`.
4. The tool would generate, template, or echo an approval line, or
   present output a downstream tool could treat as an approval/apply.
5. The tool would modify the schema, the fixtures, or the validator
   (import-only of the validator is allowed).
6. The tool would coerce, strip-and-accept, repair, or partially render a
   refused document as "accepted".
7. A model load, SGLang start, or ComfyUI job would occur to build or run
   the tool.
8. The lane would need a new declared dependency without operator
   approval.
9. The lane would touch any §18 forbidden surface.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 22. Definition Of Done

This **scope card** is done when:

- it recommends a future pretty-printer path (§17) without creating it
- it pins the input contract (§6) and output contract (§7)
- it pins the read-only (§8), no-write (§9), no-command (§10), no-model
  (§11), and no-approval (§12) boundaries
- it pins the rendering rules (§13) and the invalid-input rendering
  behavior (§14)
- it pins the secret-handling rule (§15: field path only, never the
  value)
- it pins the future implementation constraints (§16), allowed (§17) and
  forbidden (§18) touched surfaces, and validation requirements (§19)
- it states plainly that it authorizes design only — no pretty-printer
  code, no schema/validator change, no implementation, no model
  execution, no direct writes, no approval bypass, no model-generated
  approval lines, no raw `:11434` app inference

A2-L4-S2A-6 **implementation** (the pretty-printer, its tests) is out of
scope for this card and is done only when the separately-authorized
S2A-7 lane lands under its own review.

## 23. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **A2-L4-S2A-7 (Pretty-Printer Implementation)**. S2A-7
> creates `scripts/pretty_print_planner_output.py` and
> `tests/a2_l4/test_pretty_print_planner_output.py` implementing this
> card's §§6–15, bounded strictly by §§17–18, importing the S2A-5
> validator read-only and exercising the S2A-3 fixtures (§19). The tool
> reads only and executes/loads nothing. It is a separate, fresh-worktree
> PR with its own implementation scope card, and adds no dependency
> without operator approval.

## 24. References

- [`a2-l4-s2a4-schema-validator-scope-card.md`](./a2-l4-s2a4-schema-validator-scope-card.md)
  — A2-L4-S2A-4 validator scope card (the validator this tool layers on).
- [`a2-l4-s2a1-json-schema-file-scope-card.md`](./a2-l4-s2a1-json-schema-file-scope-card.md)
  — A2-L4-S2A-1 schema-file scope card.
- [`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md)
  — A2-L4-S2A parent slice (field-set/semantics source).
- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 task-planner slice.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`../schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json)
  — the schema the rendered documents conform to (S2A-2, PR #58).
- [`../schemas/a2-l4/fixtures/planner-output/`](../schemas/a2-l4/fixtures/planner-output/)
  — the fixtures the pretty-printer's tests render (S2A-3, PR #60).
- [`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py)
  — the read-only validator the pretty-printer imports (S2A-5, PR #62).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`editor-vscode.md`](./editor-vscode.md) — source of the LAW-1
  `:11435`-only routing refusal this card inherits.

## 25. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize pretty-printer implementation.
It does not authorize schema or validator changes.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The planner-output pretty-printer is scoped but not created until the
separately-authorized A2-L4-S2A-7 lane lands under its own review.

Next gate: read-only operator review of this scope card.
```
