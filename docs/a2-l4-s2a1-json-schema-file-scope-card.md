# A2-L4-S2A-1 Scope Card — JSON Schema File (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2A-1 lane.
It describes whether and how the conceptual planner-output contract
(A2-L4-S2A) should later become an actual JSON Schema file: where that
file should live, what surfaces a future schema lane may and may not
touch, which fields the schema must require / allow / forbid, how it
must constrain fields so they can never become executable transport,
how versioning and unknown-field handling work, and the fixtures a
validator lane must have. This file itself authorizes **no runtime
change, no CLI change, no schema file, no schema validator, no model
execution, no model load, no direct write, no approval bypass, no
model-generated approval line, and no raw `localhost:11434` app
inference**.

A2-L4-S2A-1 is the first sub-slice of A2-L4-S2A. The S2A card
([`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md))
pinned the planner-output contract **conceptually** and explicitly held
the concrete schema for a later lane. Its §25 sub-slice sequence names
this lane:

> **S2A-1** — docs-only scope card pinning the concrete JSON schema
> shape. No schema file yet; the card scopes it.

This card carries that exactly one step: it scopes the *future* schema
file — its path, surfaces, field constraints, versioning,
unknown-field/validation policy, and fixtures — but it still does
**not** create the schema file or any validator. Creating the schema
file is the next lane (A2-L4-S2A-2), separately scoped and reviewed.

### Must State

```text
This card authorizes design only.
It does not authorize schema file creation.
It does not authorize schema validator implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2A-1 defines, in design only, the boundary a **future JSON
Schema file** for the planner-output contract must hold to. The schema
would let a future read-only consumer — a CLI pretty-printer, the
harness adapter, an IDE panel, or an external-verifier handoff builder —
validate that a planner output is well-formed and inert before any
human acts on it. The schema is a *validation surface*, never an
*authority surface*: conforming to it grants nothing.

The recommended A2-L4-S2A-1 scope is:

> Pin the future JSON Schema's location, allowed/forbidden touched
> surfaces, required/optional/forbidden field sets, field-level safety
> constraints (no executable command transport, no approval line, no raw
> `:11434` endpoint, no secrets), `additionalProperties: false`
> unknown-field policy, reject-never-coerce validation policy,
> versioning rules, and the fixture pack a validator lane must ship —
> without creating the schema file or any validator, and while the
> A2-L2b chain remains the only write authority.

The implementation of A2-L4-S2A-1 is **not authorized by this scope
card**, and **no schema file is created by it**. This card defines the
boundary the future schema-file lane (A2-L4-S2A-2) must hold to. The
next gate before any implementation is operator review of this scope
card.

## 2. Relationship To A2-L4-S2A

A2-L4-S2A-1 is bounded by, and subordinate to, the S2A card:

- S2A §11 ("Conceptual Contract Shape") named the 14 conceptual fields
  and declared them conceptual only — "A2-L4-S2A does not create a
  schema file".
- S2A §12/§13/§14 fixed the required/optional/forbidden field sets.
- S2A §15 fixed per-field semantics; §§16–19 drew the patch-intent,
  preview-request, test-suggestion, and external-verifier boundaries;
  §21/§22 pinned inertness and operator-routability; §23 listed the
  validation expectations a future lane must meet.
- S2A §25 named S2A-1 as the docs-only lane that pins the concrete JSON
  schema shape, with "No schema file yet; the card scopes it."

S2A-1 inherits every S2A boundary verbatim and does not widen it. It
only translates S2A's conceptual contract into the concrete *schema
design constraints* a future schema file must satisfy. Where this card
and the S2A card differ, the S2A card and (above it) the S2 and parent
A2-L4 cards remain authoritative.

## 3. Relationship To A2-L4 North Star

The A2-L4 North Star:

```text
Use local models to approximate Claude Code / Codex-style coding
assistance, while keeping A2 preview/approve/apply as the write
authority and ChatGPT/Claude as optional external verifier/reviewer.
```

A2-L4-S2A-1 is the **shape-checking step** for the model's proposal. A
JSON Schema makes "the planner output may become structured" concrete
while guaranteeing "the output must remain inert" — the schema rejects
any structure that could carry execution, approval, or secrets. The
schema never becomes an executable authority surface; passing validation
is necessary-for-well-formedness, never sufficient-for-action.

## 4. What S2A-1 Adds

A2-L4-S2A-1 adds, in design only:

1. A **recommended future schema path** (§7) and rationale.
2. **Allowed** (§8) and **forbidden** (§9) future touched surfaces for
   the schema-file lane.
3. **Versioning rules** (§10) for the schema and the contract.
4. The **required** (§11), **optional** (§12), and **forbidden** (§13)
   field sets, re-pinned from S2A for the schema.
5. **Field-level safety constraints** (§14) and an explicit
   **command/execution payload prohibition** (§15).
6. Per-field schema constraints for `patch_intent` (§16),
   `preview_request` (§17), `test_suggestions` (§18), and
   `external_verifier_handoff` (§19).
7. **Secrets** (§20), **routing** (§21), and **VRAM** (§22) constraints.
8. An **unknown-field policy** (§23), a **validation-error policy**
   (§24), and the **fixture requirements** (§25) for the validator lane.

S2A-1 adds **no schema file, no validator, and no write authority.** The
A2-L2b chain remains the only writer.

## 5. What S2A-1 Does Not Add

A2-L4-S2A-1 does **not** add:

- any JSON Schema file or any `.json` file
- any validator code or planner implementation
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

## 6. Schema File Purpose

A future JSON Schema for the planner output would:

- give read-only consumers a single, versioned way to validate that a
  planner output is well-formed and inert before a human acts on it
- make the required/optional/forbidden field sets machine-checkable
- reject — structurally — any output that smuggles an executable
  command, an approval line, a raw backend endpoint, or a secret
- serve as the shared contract for future CLI, harness, IDE, and
  external-verifier lanes without coupling any of them to each other

The schema's purpose is **validation, not authority**. A schema-valid
output is still inert and still requires operator review; validation
never authorizes a write, an approval, or an apply.

## 7. Recommended Future Schema Path

The recommended future path is:

```text
schemas/a2-l4/planner-output.schema.json
```

Rationale:

- **central schema directory** — a top-level `schemas/` tree is a
  natural home for cross-cutting contracts; none exists yet, so the
  schema lane creates a clean, purpose-built location.
- **not tied to Rust implementation** — keeping it out of `rust/`
  avoids coupling the contract to the Rust workspace or a Cargo crate.
- **not tied to IDE implementation** — keeping it out of `ide/` avoids
  coupling it to any single host (the A2-L3 adapters deliberately
  re-derive their parsers per host; the schema should be host-neutral).
- **usable by future CLI, harness, IDE, and verifier lanes** — a
  neutral path lets every future read-only consumer reference one
  source of truth.

This is a **recommendation only**; the future schema-file lane (S2A-2)
confirms or amends the path under its own review. This card creates no
directory and no file.

## 8. Future Allowed Touched Surfaces

For the future schema-file lane (S2A-2), allow **only**:

```text
schemas/a2-l4/planner-output.schema.json
docs/a2-l4-s2a-planner-output-contract-scope-card.md
docs/a2-l4-s2a1-json-schema-file-scope-card.md
README.md
```

- `schemas/a2-l4/planner-output.schema.json` — the schema file the
  future lane creates.
- the two scope-card docs — only if the future lane needs a small,
  reviewed clarification (not a rewrite).
- `README.md` — only if adding or updating one Documentation Map line.

The future schema-file lane touches nothing else.

## 9. Future Forbidden Touched Surfaces

The future schema-file lane must **not** touch:

```text
rust/**
ide/**
Cargo.toml
Cargo.lock
tests/**
.github/**
scripts/**
wrappers/**
bin/**
examples/**
SideStackAI/**
runtime configs
```

A schema file is data, not code: it requires no Rust, no IDE source, no
Cargo manifest, no test harness, no workflow, and no runtime config to
exist. Touching any of the above is scope drift and a STOP (§28).

## 10. Schema Versioning Rules

- The schema carries a pinned **`schema_version`** literal (a required
  field, §11). Consumers reject any output whose `schema_version` is not
  a known value.
- The schema file itself should be versioned in lockstep with the
  contract: a breaking change to the field sets, semantics, or
  constraints requires a new `schema_version` literal **and** a
  scope-card amendment (this card or S2A), not a silent edit.
- Additive, non-breaking clarifications that do not change accepted
  inputs may keep the version; any change that would accept previously
  rejected input, or reject previously accepted input, is breaking and
  bumps the version.
- The schema must use a stable JSON Schema dialect declared via
  `$schema`; the dialect is fixed by the future schema lane and pinned
  there.

## 11. Required Field Set

The future schema must require these top-level fields (from S2A §12):

```text
schema_version
task_id
workspace_root
task_summary
plan_steps
risk_notes
operator_next_steps
```

A missing required field is a validation failure (§24), never a silent
default.

## 12. Optional Field Set

The future schema must permit these optional top-level fields (from S2A
§13), absent or present:

```text
repo_context_summary
candidate_files
test_suggestions
patch_intent
preview_request
external_verifier_handoff
status_snapshot
```

An absent optional field carries no meaning beyond "not provided" and
never implies a default action.

## 13. Forbidden Field Set

The future schema must reject any output carrying these fields (from S2A
§14):

```text
approval_line
approval_command
apply_command
apply_bundle_command
run_command
shell_command
write_command
autonomous_apply
auto_approve
raw_11434_endpoint
secret_value
token_value
env_secret
private_key
```

Because the schema denies unknown top-level fields by default (§23), any
forbidden field is rejected both by the explicit forbid intent and by
the closed-object policy. The schema must **reject** — never coerce,
strip-and-accept, or ignore — an output carrying a forbidden field.

## 14. Field-Level Safety Constraints

The future schema must constrain string and array fields so they cannot
become executable command transport. Examples:

```text
No approval line strings.
No shell command arrays.
No raw backend URLs.
No secret-looking values.
No apply/run command payload fields.
```

Concretely, the schema design should:

- type `plan_steps`, `risk_notes`, `operator_next_steps`, and the
  summaries as descriptive text/arrays-of-text, not as
  command/argv structures
- constrain `workspace_root` and `candidate_files` to path strings, with
  the validator (S2A-2) enforcing workspace-relative, no-path-escape
  semantics
- forbid any field whose value is, or templates, an approval line
  (`apply <step_id> <preview_sha256>`)
- forbid any field carrying a URL pointing at `:11434` (§21)
- forbid any field carrying a secret/token/key pattern (§20)

The schema constrains *shape*; the validator lane (S2A-2) adds the
read-only semantic checks the schema alone cannot express.

## 15. Command / Execution Payload Prohibition

No field in the schema may be, contain, or template an executable
payload:

- no shell command, argv array, or command string
- no `claw plan run/approve/apply-bundle/apply` invocation or template
- no approval line, `approval-result.json`, or `apply-bundle.json`
  content
- no raw `localhost:11434` endpoint or any backend-inference URL
- no field whose semantics are "run this" / "apply this" / "approve
  this"

A planner output is inert data a human reads. The schema must make it
structurally impossible to express an action as a conforming field. A
conforming document confers no execution authority of any kind.

## 16. Patch Intent Constraints

For `patch_intent`, the future schema must enforce (from S2A §16):

- `patch_intent` is **not a patch file** and **not an applyable
  artifact**.
- it must **not** carry direct file-replacement payloads (full file
  bodies staged for application) unless a future patch-proposal artifact
  lane (parent A2-L4-S3) explicitly authorizes that — which this card
  does not.
- it may carry intended edits **as prose or structured inert notes
  only** (e.g. a description of what to change and why), never an
  applyable diff or a staged replacement body.

## 17. Preview Request Constraints

For `preview_request`, the future schema must enforce (from S2A §17):

- `preview_request` is **inert**.
- it must **not** be, contain, or template a run of `claw plan run` (or
  any chain command).
- it must **not** carry a generated preview artifact.
- it must **not** carry an approval line.
- it is a **request** for the operator (or a future, separately
  authorized A2-integration lane) to consider — never an invocation.

## 18. Test Suggestion Constraints

For `test_suggestions`, the future schema must enforce (from S2A §18):

- `test_suggestions` are **inert**.
- they may **name** tests or **suggest** commands for operator review,
  as descriptive text.
- they must **not** be structured as an executable command runner, and
  the validator (S2A-2) must ensure they are not auto-executed.
- they must **not** carry a shell command intended for automatic
  execution.

## 19. External Verifier Handoff Constraints

For `external_verifier_handoff`, the future schema must enforce (from
S2A §19):

- it is **operator-gated** (sending it is an operator gesture).
- it must **not** contain secrets, token values, env secrets, or
  private keys (overlaps the forbidden-field and secrets constraints,
  §§13, 20).
- it must **not** grant write authority to ChatGPT/Claude; the handoff
  is review input, not an instruction the verifier can act on.
- external-verifier output is **advisory only** and is not represented
  in the schema as an authority field.

The concrete handoff artifact format is a separate future lane (S2A-5).

## 20. Secrets / Sensitive Data Constraints

- No schema field may carry a secret, token, env secret, private key, or
  credential. The forbidden-field set (§13) names these explicitly, and
  the closed-object policy (§23) blocks novel secret-bearing fields.
- The validator lane (S2A-2) should additionally reject values that
  match common secret/token/key patterns even in permitted text fields,
  rejecting (not redacting) on match.
- An `external_verifier_handoff` (§19) is an outward publish and must be
  secret-free.
- The schema introduces no new credential, key, or token surface.

## 21. Broker / Routing Constraints

```text
All model-related app inference routes through localhost:11435.
Raw localhost:11434 app inference is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation.
```

The schema is a static data contract and performs no inference itself,
but it must reinforce the routing boundary as data:

- `raw_11434_endpoint` is a forbidden field (§13).
- no permitted field may carry a `:11434` base URL or any
  backend-inference URL as a value (§§14, 15).
- any `:11434` reference encountered while building the schema or its
  fixtures must be classified as management, docs/history, false
  positive, or violation; a violation is a STOP.

The inference that *produces* a planner output (in future S2 lanes)
routes through the SideStack broker at `http://127.0.0.1:11435/v1` (see
`examples/sidestack-local.env`); a base URL at `:11434` (raw Ollama) is
a refusal, not a fallback ([`editor-vscode.md`](./editor-vscode.md)).

## 22. VRAM Safety Constraints

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker/current-holder/VRAM checks.
```

Authoring a schema file and its fixtures is a pure text/data task and
must trigger no model or GPU activity:

- no S2A-1 or S2A-2 lane loads a model, starts SGLang, submits ComfyUI
  jobs, or runs inference to produce or validate the schema.
- any future lane that does need a model or GPU must first confirm
  broker reachability, the current VRAM holder, and available headroom,
  and defer to the operator on contention.

## 23. Unknown Field Policy

The future schema must **deny unknown top-level fields by default**
(`additionalProperties: false` at the top level, and on nested objects
unless a field's design explicitly requires openness):

```text
deny unknown top-level fields by default
```

Reason:

```text
planner output is a security-sensitive boundary
new fields require explicit review
```

A new field is added only by amending the contract (S2A) and the schema
(S2A-2) under review, with a `schema_version` bump if the change is
breaking (§10). Unknown fields are **rejected**, never preserved or
passed through, because a silently-accepted unknown field is exactly the
smuggling vector the forbidden-field set guards against.

## 24. Validation Error Policy

A future schema/validator must treat any validation failure as:

```text
STOP / refused planner output
```

never as:

```text
best-effort coercion
strip-and-accept
silent default
```

Specifically, on a missing required field, a forbidden field, an unknown
field, a type mismatch, a path escape, a secret-pattern match, or a
`:11434` value, the validator refuses the whole output and surfaces the
failure to the operator. It never repairs, trims, or partially accepts a
non-conforming output, and it never treats a validation pass as
authorization to write, approve, or apply.

## 25. Fixture Requirements For Future Schema Lane

Before a validator implementation (S2A-2) lands, a fixture pack (the S2A
§25 S2A-3 lane, or bundled with S2A-2 under its own review) must include
at least:

```text
valid minimal planner output
valid full planner output
missing required field
unknown top-level field
forbidden approval_line
forbidden shell_command
forbidden raw_11434_endpoint
forbidden secret_value
preview_request containing executable command attempt
patch_intent containing direct file replacement payload attempt
external_verifier_handoff containing secret-like value
```

Each negative fixture must be a **rejected** case (§24); each positive
fixture must validate cleanly and remain inert. The fixtures are test
data only — they create no runtime behavior and no write path.

## 26. Non-Goals

A2-L4-S2A-1 is explicitly **not**:

- a JSON Schema file or any `.json` file (that is S2A-2)
- a validator implementation (that is S2A-2)
- a planner implementation (that is S2 §22, S2B)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a patch generator or patch-proposal artifact (that is A2-L4-S3)
- a new write command, write flag, or approval-bypass affordance
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a SideStackAI infrastructure change

## 27. Future Implementation Constraints

The recommended next lane after this card is:

```text
A2-L4-S2A-2 JSON Schema File Implementation
```

It must be **separately scoped and reviewed**, and bounded by this card.
Per-lane constraints:

- **S2A-2** — creates `schemas/a2-l4/planner-output.schema.json` (the
  §7 path) implementing §§10–24, plus a read-only validator. Touches
  only the §8 allowed surfaces; touches none of the §9 forbidden
  surfaces. Validates only — writes nothing, executes nothing, loads no
  model. The fixture pack (§25) lands with it or in the S2A-3 lane.

No future lane under this card may authorize direct writes, model
execution, approval bypass, model-generated approval lines, or raw
`:11434` app inference.

## 28. STOP Gates

Any future A2-L4-S2A-1 implementation lane must STOP — escalate — and
not proceed if any of the following is true:

1. The lane would create a `.json` / schema file or a validator under
   *this* (S2A-1) scope card rather than under S2A-2.
2. The schema or a fixture would carry an executable command, an
   approval line, `approval-result.json`, or `apply-bundle.json`.
3. The schema or a fixture would carry a secret, token, env secret, or
   private key (outside an explicitly-rejected negative fixture).
4. The schema would permit a raw `localhost:11434` endpoint value, or
   app inference would route to raw `:11434`.
5. A model load, SGLang start, or ComfyUI job would occur to author or
   validate the schema.
6. The lane would touch any §9 forbidden surface (`rust/**`, `ide/**`,
   Cargo files, `tests/**`, `.github/**`, `scripts/**`, `wrappers/**`,
   `bin/**`, `examples/**`, `SideStackAI/**`, runtime configs).
7. The schema would adopt an open unknown-field policy
   (`additionalProperties: true`) at the top level.
8. The lane would modify `claw plan run/approve/apply-bundle/apply`,
   `claw plan status`, `a2-l2d-status.v1`, or any A2-L2b schema/marker.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 29. Definition Of Done

This **scope card** is done when:

- it recommends a future schema path (§7) without creating it
- it pins the allowed (§8) and forbidden (§9) future touched surfaces
- it pins versioning rules (§10)
- it re-pins the required (§11), optional (§12), and forbidden (§13)
  field sets for the schema
- it pins field-level safety constraints (§14) and the
  command/execution payload prohibition (§15)
- it pins the patch-intent (§16), preview-request (§17),
  test-suggestion (§18), and external-verifier (§19) schema constraints
- it pins the secrets (§20), routing (§21), and VRAM (§22) constraints
- it sets the unknown-field policy (§23) and validation-error policy
  (§24)
- it lists the fixture requirements (§25) for the validator lane
- it states plainly that it authorizes design only — no schema file, no
  validator, no implementation, no model execution, no direct writes, no
  approval bypass, no model-generated approval lines, no raw `:11434`
  app inference

A2-L4-S2A-1 **implementation** (the schema file, the validator, the
fixtures) is out of scope for this card and is done only when the
separately-authorized S2A-2 lane (and S2A-3 fixtures) land under their
own review.

## 30. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **A2-L4-S2A-2 (JSON Schema File Implementation)**. S2A-2
> creates `schemas/a2-l4/planner-output.schema.json` implementing this
> card's §§10–24 and a read-only validator, bounded strictly by §§8–9,
> with the §25 fixture pack landing alongside it (or in the S2A-3 lane).
> S2A-2 writes only the schema file and a validator that executes
> nothing and loads no model. It is a separate, fresh-worktree PR with
> its own scope card or implementation scope card.

## 31. References

- [`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md)
  — A2-L4-S2A parent slice (this card's field-set/semantics source;
  §25 names S2A-1).
- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 task-planner slice.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record; a model for a versioned,
  closed-enum, read-only contract a `status_snapshot` would summarize.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md) —
  A2-L3 read-only harness adapter (a future schema consumer).
- [`a2-l3-ide-adapter-usage.md`](./a2-l3-ide-adapter-usage.md) — A2-L3
  read-only VS Code Claw Status Panel (a future schema consumer).
- [`editor-vscode.md`](./editor-vscode.md) — read-only VS Code task
  wrapper; source of the LAW-1 `:11435`-only routing refusal this card
  inherits.

## 32. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize schema file creation.
It does not authorize schema validator implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The planner-output JSON Schema is scoped but not created until the
separately-authorized A2-L4-S2A-2 lane lands under its own review.

Next gate: read-only operator review of this scope card.
```
