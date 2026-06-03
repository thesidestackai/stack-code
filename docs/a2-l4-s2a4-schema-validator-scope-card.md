# A2-L4-S2A-4 Scope Card — Schema Validator (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2A-4 lane.
It describes whether and how a future **read-only validator** for the
now-existing planner-output JSON Schema
([`schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json),
landed by S2A-2) should be implemented: what surfaces a future validator
lane (A2-L4-S2A-5) may and may not touch, how it must treat validation
failure, what it must never do (coerce, strip-and-accept, write, execute,
load a model, route to raw `:11434`, bypass approval), and what fixtures
([`schemas/a2-l4/fixtures/planner-output/`](../schemas/a2-l4/fixtures/planner-output/),
landed by S2A-3) it must exercise. This file itself authorizes **no
runtime change, no CLI change, no schema change, no validator code, no
model execution, no model load, no direct write, no approval bypass, no
model-generated approval line, and no raw `localhost:11434` app
inference**.

A2-L4-S2A-4 is the fourth sub-slice of A2-L4-S2A. Its predecessors:

- **S2A-1** ([`a2-l4-s2a1-json-schema-file-scope-card.md`](./a2-l4-s2a1-json-schema-file-scope-card.md))
  — docs-only card scoping the future schema file.
- **S2A-2** — landed `schemas/a2-l4/planner-output.schema.json` (the
  schema file; PR #58).
- **S2A-3** — landed the fixture pack under
  `schemas/a2-l4/fixtures/planner-output/` (PR #60).

S2A-1 §27 anticipated "a read-only validator" alongside the schema. This
card carries that exactly one step forward: it scopes the *future*
validator — its surfaces, semantics, failure policy, and fixture
obligations — but it still does **not** create the validator. Creating
the validator is the next lane (A2-L4-S2A-5), separately scoped and
reviewed.

### Must State

```text
This card authorizes design only.
It does not authorize schema validator implementation.
It does not authorize schema file changes.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2A-4 defines, in design only, the boundary a **future read-only
validator** for the planner-output contract must hold to. The validator
would let a future read-only consumer — a CLI pretty-printer, the
harness adapter, an IDE panel, or an external-verifier handoff builder —
confirm that a planner output is well-formed and inert **before any
human acts on it**. The validator is a *checking surface*, never an
*authority surface*: a validation pass grants nothing.

The recommended A2-L4-S2A-4 scope is:

> Pin the future validator's location, allowed/forbidden touched
> surfaces, input/output contract, the reject-never-coerce failure
> policy, the semantic checks the schema alone cannot express
> (workspace-relative/no-path-escape paths, `:11434` value refusal,
> secret/token pattern refusal), the fixtures it must exercise, and the
> exit-code/reporting contract — without creating the validator, and
> while the A2-L2b chain remains the only write authority.

The implementation of the validator is **not authorized by this scope
card**, and **no validator code is created by it**. This card defines
the boundary the future validator lane (A2-L4-S2A-5) must hold to. The
next gate before any implementation is operator review of this scope
card.

## 2. Relationship To A2-L4-S2A And S2A-1

A2-L4-S2A-4 is bounded by, and subordinate to, the S2A card and S2A-1:

- The S2A card
  ([`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md))
  fixed the required/optional/forbidden field sets, per-field semantics,
  and the inertness/operator-routability requirements.
- S2A-1 §§10–24 translated those into concrete schema-design
  constraints; §24 fixed the **validation-error policy** (any failure =
  STOP / refused output, never coercion); §25 listed the fixtures a
  validator must exercise.
- S2A-2 landed the schema file; S2A-3 landed the fixtures.

S2A-4 inherits every S2A and S2A-1 boundary verbatim and does not widen
it. It only translates S2A-1's validation-error policy and semantic-check
expectations into the concrete *validator design constraints* a future
validator must satisfy. Where this card and S2A-1/S2A differ, the
upstream cards remain authoritative.

## 3. Relationship To A2-L4 North Star

The A2-L4 North Star:

```text
Use local models to approximate Claude Code / Codex-style coding
assistance, while keeping A2 preview/approve/apply as the write
authority and ChatGPT/Claude as optional external verifier/reviewer.
```

A2-L4-S2A-4 is the **shape-and-safety-checking step** for the model's
proposal. The schema (S2A-2) makes "the planner output may become
structured" concrete; the validator makes "and it must be confirmed
inert before a human acts" operational. A validation pass is
necessary-for-well-formedness, never sufficient-for-action: the
validator never writes, approves, applies, or routes inference.

## 4. What S2A-4 Adds

A2-L4-S2A-4 adds, in design only:

1. A **recommended future validator path** (§7) and rationale.
2. **Allowed** (§8) and **forbidden** (§9) future touched surfaces for
   the validator lane.
3. The validator's **input/output contract** (§10).
4. The **schema-conformance check** (§11) and the **semantic checks the
   schema alone cannot express** (§12): workspace-relative/no-path-escape
   paths, `:11434` value refusal, secret/token pattern refusal.
5. The **reject-never-coerce failure policy** (§13) and the
   **no-strip-and-accept** rule (§14).
6. The **exit-code / reporting contract** (§15).
7. The **fixtures the validator must exercise** (§16).
8. **Secrets** (§17), **routing** (§18), and **VRAM** (§19) constraints.

S2A-4 adds **no validator code, no schema change, and no write
authority.** The A2-L2b chain remains the only writer.

## 5. What S2A-4 Does Not Add

A2-L4-S2A-4 does **not** add:

- any validator code, script, or implementation
- any change to `schemas/a2-l4/planner-output.schema.json` or the
  fixtures
- any write path, write command, or write flag
- any executable-command field, approval line, or apply affordance
- any change to `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, `claw plan apply`, or `claw plan status`
- any change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- any raw `localhost:11434` app-inference path
- any automatic model load, SGLang start, ComfyUI job, or GPU workload
- any new secret, key, or token surface

Any of the above must be opened as a separate, explicitly-authorized
lane and clear its own review. This card is **not** prior authorization
for it.

## 6. Validator Purpose

A future read-only validator would:

- confirm a planner output conforms to
  `schemas/a2-l4/planner-output.schema.json` (shape)
- add the read-only semantic checks the schema cannot express:
  workspace-relative/no-path-escape paths, `:11434` value refusal,
  secret/token pattern refusal (§12)
- **refuse** (never repair) any non-conforming output and surface the
  failure to the operator (§13)
- give every future read-only consumer (CLI, harness, IDE, verifier
  handoff builder) one shared, trustworthy gate

The validator's purpose is **checking, not authority**. A validated
output is still inert and still requires operator review; passing
validation never authorizes a write, an approval, or an apply.

## 7. Recommended Future Validator Path

The recommended future paths are:

```text
scripts/validate_planner_output_schema.py
tests/a2_l4/test_validate_planner_output_schema.py
```

Rationale:

- **`scripts/` for a read-only operator helper** — the repository
  already keeps operator-facing read-only helpers under `scripts/`
  (e.g. `scripts/claw-sidestack-local`); a validator that prints a
  pass/fail verdict for operator review fits there.
- **a sibling test module** — a `tests/a2_l4/` test exercises the
  validator against the S2A-3 fixtures so CI proves valid fixtures pass
  and invalid fixtures fail.
- **stdlib-preferred, no new dependency** — a Python `json` +
  hand-written checks, or `jsonschema` only if already available in CI,
  avoids adding a dependency; if a dependency would be required, that is
  a STOP (§20) and an operator decision.

This is a **recommendation only**; the future validator lane (S2A-5)
confirms or amends the path under its own review, or adopts a
repo-conventional location if a better one is discovered. This card
creates no file.

## 8. Future Allowed Touched Surfaces

For the future validator lane (S2A-5), allow **only**:

```text
scripts/validate_planner_output_schema.py
tests/a2_l4/test_validate_planner_output_schema.py
docs/a2-l4-s2a4-schema-validator-scope-card.md
README.md
```

- the validator script — the read-only validator the future lane
  creates.
- the test module — exercises the validator against the S2A-3 fixtures.
- this scope-card doc — only if the future lane needs a small, reviewed
  clarification (not a rewrite).
- `README.md` — only if adding or updating one Documentation Map line.

The future validator lane touches nothing else. If the lane discovers a
repo-conventional validator/test location that differs from §7, it
re-scopes under its own review before using it.

## 9. Future Forbidden Touched Surfaces

The future validator lane must **not** touch:

```text
schemas/a2-l4/planner-output.schema.json
schemas/a2-l4/fixtures/**
rust/**
ide/**
Cargo.toml
Cargo.lock
.github/** (beyond an already-green CI path filter)
wrappers/**
bin/**
examples/**
SideStackAI/**
.claw/**
runtime configs
```

The validator reads the schema and fixtures; it must not modify them. A
read-only validator requires no Rust, no IDE source, no Cargo manifest,
and no runtime config to exist. Touching any of the above is scope drift
and a STOP (§21).

## 10. Validator Input / Output Contract

The future validator must:

- accept a path (or stdin) to a single planner-output JSON document
- read it **read-only**; it never writes, edits, or stages any file
- emit a human-readable pass/fail verdict to stdout/stderr for operator
  review
- return a documented exit code (§15)

It must **not**:

- mutate the input, the schema, the fixtures, or any file
- write to `.claw/**` or any workspace path
- produce an approval line, `approval-result.json`, or
  `apply-bundle.json`
- invoke `claw plan run/approve/apply-bundle/apply` or any chain command
- call a model, the broker, or Ollama

## 11. Schema Conformance Check

The validator must check the document against
`schemas/a2-l4/planner-output.schema.json` for:

- the pinned `schema_version` literal
- presence of all required fields
- absence of forbidden top-level fields
- rejection of unknown fields (the schema is `additionalProperties:
  false`)
- correct types and the closed-object nested-field policy

A schema-conformance failure is a refusal (§13).

## 12. Semantic Checks Beyond The Schema

The validator must add the read-only semantic checks the schema alone
cannot express (S2A-1 §§14, 20, 21):

```text
workspace-relative / no-path-escape paths
:11434 URL value refusal
secret / token / key pattern refusal
```

Concretely:

- `workspace_root` and every `candidate_files` entry must be
  workspace-relative with no `..` path escape and no absolute-path
  escape outside the workspace root; an escape is a refusal.
- any field value carrying a `:11434` endpoint (or any raw
  backend-inference URL) is a refusal — the validator reinforces LAW 1
  as data and never treats `:11434` as a fallback.
- any field value matching a common secret/token/key pattern is a
  refusal (the validator **rejects**, never redacts-and-accepts), even
  in an otherwise-permitted text field.

These checks are read-only string/path inspections; they perform no
inference and load no model.

## 13. Validation Failure Policy

The validator must treat any failure as:

```text
STOP / refused planner output
```

never as:

```text
best-effort coercion
strip-and-accept
silent default
partial accept
```

On a missing required field, a forbidden field, an unknown field, a type
mismatch, a path escape, a secret-pattern match, or a `:11434` value, the
validator refuses the whole output and surfaces the failure to the
operator. It never repairs, trims, or partially accepts a non-conforming
output, and a validation pass is never authorization to write, approve,
or apply.

## 14. No Coercion / No Strip-And-Accept

The validator must never:

- delete, blank, or rewrite a forbidden/unknown field and then accept
  the remainder
- "fix up" a malformed field and continue
- downgrade a refusal to a warning that still yields a usable output

A non-conforming output is refused **as a whole**. The only safe
outcomes are *accept-as-inert* (fully conforming) or *refuse* (anything
else).

## 15. Exit-Code / Reporting Contract

The validator must document its exit codes. Recommended:

```text
0  valid and inert (all schema + semantic checks pass)
1  invalid (schema or semantic-check failure) — refused
2  usage / IO error (could not read input)
```

A non-zero exit is **never** silently swallowed by a future caller. The
verdict text names which check failed (e.g. "forbidden field
`approval_line`", "`:11434` endpoint refused", "path escape in
`candidate_files[2]`") without printing secret values — secret-pattern
failures report the field path only, never the matched value.

## 16. Fixtures The Validator Must Exercise

The validator's test module must exercise the S2A-3 fixtures under
`schemas/a2-l4/fixtures/planner-output/`:

```text
valid-minimal.json                          -> accept
valid-full.json                             -> accept
invalid-missing-required.json               -> refuse
invalid-unknown-field.json                  -> refuse
invalid-approval-line.json                  -> refuse
invalid-shell-command.json                  -> refuse
invalid-raw-11434-endpoint.json             -> refuse
invalid-secret-value.json                   -> refuse
invalid-preview-request-command.json        -> refuse
invalid-patch-intent-direct-replacement.json -> refuse
invalid-verifier-secret.json                -> refuse
```

Every `valid-*` fixture must be accepted as inert; every `invalid-*`
fixture must be refused (§13). The future lane may add semantic-only
fixtures (path escape, `:11434` value, secret-in-text) if needed to
exercise §12 checks the existing schema-level fixtures do not cover —
those additions are themselves scoped under S2A-5's review.

## 17. Secrets / Sensitive Data Constraints

- The validator introduces no new credential, key, or token surface.
- It **rejects** (never redacts-and-accepts) values matching common
  secret/token/key patterns (§12), reporting the field path only, never
  the matched value (§15).
- It never writes any value it read to a log, file, or external service.

## 18. Broker / Routing Constraints

```text
All model-related app inference routes through localhost:11435.
Raw localhost:11434 app inference is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation.
```

The validator performs no inference itself, but it reinforces the
routing boundary as data:

- a `:11434` value in any field is a refusal (§12).
- any `:11434` reference encountered while building the validator or its
  tests must be classified as management, docs/history, false positive,
  or violation; a violation is a STOP.

The inference that *produces* a planner output (in future S2 lanes)
routes through the SideStack broker at `http://127.0.0.1:11435/v1` (see
`examples/sidestack-local.env`); a base URL at `:11434` is a refusal,
not a fallback.

## 19. VRAM Safety Constraints

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker/current-holder/VRAM checks.
```

Authoring a validator and its tests is a pure text/data task and must
trigger no model or GPU activity. The validator never loads a model,
starts SGLang, submits ComfyUI jobs, or runs inference; it only reads
JSON and inspects strings/paths.

## 20. Dependency Policy

- The validator should prefer the Python standard library (`json`,
  `pathlib`, `re`, `sys`).
- It may use `jsonschema` **only** if it is already available in the CI
  environment without adding a declared dependency.
- If the validator would need a new declared dependency, the lane must
  **STOP and ask the operator** before adding it. Adding a dependency is
  not authorized by this card.

## 21. STOP Gates

Any future A2-L4-S2A-5 validator lane must STOP — escalate — and not
proceed if any of the following is true:

1. The validator or its tests would modify
   `schemas/a2-l4/planner-output.schema.json` or any fixture.
2. The validator would write, edit, or stage any file (including
   `.claw/**`), or produce an approval line / apply artifact.
3. The validator would call a model, the broker, or Ollama, or route to
   raw `localhost:11434`.
4. The validator would coerce, strip-and-accept, or partially accept a
   non-conforming output (§§13–14).
5. A model load, SGLang start, or ComfyUI job would occur to author or
   run the validator.
6. The lane would touch any §9 forbidden surface.
7. The lane would need a new declared dependency (§20) without operator
   approval.
8. The lane would modify `claw plan run/approve/apply-bundle/apply`,
   `claw plan status`, `a2-l2d-status.v1`, or any A2-L2b schema/marker.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 22. Non-Goals

A2-L4-S2A-4 is explicitly **not**:

- a validator implementation (that is S2A-5)
- a change to the schema (S2A-2) or fixtures (S2A-3)
- a planner implementation (that is S2 §22, S2B)
- a CLI pretty-printer (that is S2A-6)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a patch generator or patch-proposal artifact (that is A2-L4-S3)
- a new write command, write flag, or approval-bypass affordance
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a SideStackAI infrastructure change

## 23. Future Implementation Constraints

The recommended next lane after this card is:

```text
A2-L4-S2A-5 Schema Validator Implementation
```

It must be **separately scoped and reviewed**, and bounded by this card.
Per-lane constraints:

- **S2A-5** — creates the §7 validator and its test module, implementing
  §§10–16. Touches only the §8 allowed surfaces; touches none of the §9
  forbidden surfaces. Validates only — reads the schema/fixtures, writes
  nothing, executes nothing, loads no model. Exercises the §16 fixtures
  in CI. Adds no dependency without operator approval (§20).

No future lane under this card may authorize direct writes, model
execution, approval bypass, model-generated approval lines, or raw
`:11434` app inference.

## 24. Definition Of Done

This **scope card** is done when:

- it recommends a future validator path (§7) without creating it
- it pins the allowed (§8) and forbidden (§9) future touched surfaces
- it pins the input/output contract (§10)
- it pins the schema-conformance check (§11) and the semantic checks
  beyond the schema (§12)
- it pins the reject-never-coerce failure policy (§13) and the
  no-strip-and-accept rule (§14)
- it pins the exit-code/reporting contract (§15)
- it lists the fixtures the validator must exercise (§16)
- it pins the secrets (§17), routing (§18), VRAM (§19), and dependency
  (§20) constraints
- it states plainly that it authorizes design only — no validator code,
  no schema change, no implementation, no model execution, no direct
  writes, no approval bypass, no model-generated approval lines, no raw
  `:11434` app inference

A2-L4-S2A-4 **implementation** (the validator, its tests) is out of
scope for this card and is done only when the separately-authorized
S2A-5 lane lands under its own review.

## 25. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **A2-L4-S2A-5 (Schema Validator Implementation)**. S2A-5
> creates `scripts/validate_planner_output_schema.py` and
> `tests/a2_l4/test_validate_planner_output_schema.py` implementing this
> card's §§10–16, bounded strictly by §§8–9, exercising the S2A-3
> fixtures (§16). The validator reads only and executes/loads nothing. It
> is a separate, fresh-worktree PR with its own implementation scope
> card, and adds no dependency (§20) without operator approval.

## 26. References

- [`a2-l4-s2a1-json-schema-file-scope-card.md`](./a2-l4-s2a1-json-schema-file-scope-card.md)
  — A2-L4-S2A-1 schema-file scope card (§24 validation-error policy and
  §25 fixture requirements this card builds on).
- [`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md)
  — A2-L4-S2A parent slice (field-set/semantics source).
- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 task-planner slice.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`../schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json)
  — the schema the validator checks (S2A-2, PR #58).
- [`../schemas/a2-l4/fixtures/planner-output/`](../schemas/a2-l4/fixtures/planner-output/)
  — the fixtures the validator must exercise (S2A-3, PR #60).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`editor-vscode.md`](./editor-vscode.md) — read-only VS Code task
  wrapper; source of the LAW-1 `:11435`-only routing refusal this card
  inherits.

## 27. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize schema validator implementation.
It does not authorize schema file changes.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The planner-output schema validator is scoped but not created until the
separately-authorized A2-L4-S2A-5 lane lands under its own review.

Next gate: read-only operator review of this scope card.
```
