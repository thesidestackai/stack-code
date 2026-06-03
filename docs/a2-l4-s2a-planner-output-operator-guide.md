# A2-L4-S2A Planner-Output Operator Guide

This guide explains how to use the A2-L4-S2A planner-output stack: the
**schema**, the **fixtures**, the read-only **validator**, and the
read-only **pretty-printer**. It is an operator usage doc, not a scope
card and not a change to any of those artifacts.

The single most important point first:

> **A valid planner output authorizes nothing.** It is inert data a human
> reads. Validation and pretty-printing confirm a document is *well-formed
> and inert* — they never grant write, approve, or apply authority. The
> A2-L2b preview/approve/apply chain remains the **only** write authority.

## 1. The Stack At A Glance

| Layer | Path | Role |
|-------|------|------|
| Schema | `schemas/a2-l4/planner-output.schema.json` | Fixes the planner-output shape (required/optional/forbidden fields, closed objects). |
| Fixtures | `schemas/a2-l4/fixtures/planner-output/` | Valid and invalid example documents used by tests and `--self-test`. |
| Validator | `scripts/validate_planner_output_schema.py` | Read-only. Checks schema conformance + semantic safety (paths, `:11434`, secrets). Accept/refuse. |
| Pretty-printer | `scripts/pretty_print_planner_output.py` | Read-only. Renders a document as an operator-friendly report after validating it. |

All four are inert/read-only. None loads a model, calls the broker or
Ollama, routes to raw `localhost:11434`, writes a file, mutates
`.claw/**`, or emits an approval line.

## 2. Schema

- **Path:** `schemas/a2-l4/planner-output.schema.json`
- **Dialect:** JSON Schema 2020-12.
- **Version literal:** `schema_version` must equal
  `a2-l4-planner-output.v1`. Consumers reject any other value.
- **Required fields:** `schema_version`, `task_id`, `workspace_root`,
  `task_summary`, `plan_steps`, `risk_notes`, `operator_next_steps`.
- **Optional fields:** `repo_context_summary`, `candidate_files`,
  `test_suggestions`, `patch_intent`, `preview_request`,
  `external_verifier_handoff`, `status_snapshot`.
- **Closed objects:** the top level and every nested object are
  `additionalProperties: false`. Unknown fields — and the explicitly
  forbidden ones (`approval_line`, `apply_command`, `run_command`,
  `shell_command`, `autonomous_apply`, `auto_approve`,
  `raw_11434_endpoint`, `secret_value`, `token_value`, `private_key`, …)
  — are rejected.

The schema validates **shape only**. Value-level safety
(workspace-relative paths, `:11434` refusal, secret-pattern refusal) is
enforced by the validator (§4), not by the schema.

## 3. Fixtures

- **Path:** `schemas/a2-l4/fixtures/planner-output/`
- **Valid:** `valid-minimal.json` (required fields only),
  `valid-full.json` (all optional fields populated).
- **Invalid (each a rejected case):** `invalid-missing-required.json`,
  `invalid-unknown-field.json`, `invalid-approval-line.json`,
  `invalid-shell-command.json`, `invalid-raw-11434-endpoint.json`,
  `invalid-secret-value.json`, `invalid-preview-request-command.json`,
  `invalid-patch-intent-direct-replacement.json`,
  `invalid-verifier-secret.json`.

The forbidden-field and secret fixtures use **placeholder** values, not
real payloads or secrets. They exist to prove the validator refuses them.

## 4. Validator Usage

`scripts/validate_planner_output_schema.py` is a read-only operator
helper. Stdlib only; no third-party dependency.

Validate one document:

```bash
python3 scripts/validate_planner_output_schema.py path/to/planner-output.json
```

Run the bundled fixture self-test (validates every fixture and reports
accept/refuse vs expected):

```bash
python3 scripts/validate_planner_output_schema.py --self-test
```

Override the schema path (defaults to the in-repo schema):

```bash
python3 scripts/validate_planner_output_schema.py --schema schemas/a2-l4/planner-output.schema.json path/to/doc.json
```

**Exit codes:**

| Code | Meaning |
|------|---------|
| `0` | valid and inert (all schema + semantic checks pass) |
| `1` | refused (a schema or semantic-check failure) |
| `2` | usage / IO error (could not read the input or schema) |

**What it checks:**

1. **Schema conformance** — `schema_version` literal, required fields,
   forbidden/unknown fields, closed objects, types, `minLength`/`minItems`.
2. **Semantic checks the schema cannot express** —
   - `workspace_root` and `candidate_files` must be workspace-relative
     with no path escape (no `..`, no absolute path);
   - no field value may carry a `:11434` endpoint reference;
   - no field value may match a secret/token/key pattern (reported by
     **field path only**, never by value).

The validator **refuses** a non-conforming document as a whole. It never
coerces, strips-and-accepts, repairs, or partially accepts it.

## 5. Pretty-Printer Usage

`scripts/pretty_print_planner_output.py` is a read-only display tool. It
validates a document (via the validator) and renders it for human
reading. Stdlib only; no third-party dependency.

Render one document:

```bash
python3 scripts/pretty_print_planner_output.py path/to/planner-output.json
```

Read from stdin:

```bash
cat path/to/planner-output.json | python3 scripts/pretty_print_planner_output.py -
```

**Exit codes** mirror the validator: `0` valid (rendered as `VALID
(inert)`), `1` refused (rendered as `REFUSED`), `2` IO/usage error.

**Output:** an operator-friendly report on **stdout only**. It writes
**nothing** to disk. Command-like fields (`test_suggestions`,
`preview_request`, `patch_intent`) are rendered as **descriptive only —
not executed**. A refusal names the failed checks by field; it never
reproduces a forbidden payload as a runnable line and never echoes a
secret value (field path only).

## 6. What A Valid Output Means

A `VALID (inert)` verdict means **only**:

- the document conforms to `a2-l4-planner-output.v1`;
- it carries no forbidden/unknown field;
- its paths are workspace-relative with no escape;
- it carries no `:11434` endpoint and no secret-pattern value.

That is the entire meaning. It is a statement about *well-formedness and
inertness*, nothing more.

## 7. What A Valid Output Does NOT Authorize

A valid output does **not**:

- authorize any write, edit, or file creation;
- authorize a preview, approval, or apply;
- authorize running any command, test, or build named in the document;
- authorize any model call, broker call, or inference;
- grant an external verifier (ChatGPT/Claude) any authority;
- mutate `.claw/**` or trigger the A2-L2b chain.

Acting on a planner output is always a **separate, operator-gated step**
through the A2-L2b preview/approve/apply chain. Validation/printing are
upstream read-only checks, never the authorization itself.

## 8. How Invalid Output Is Handled

- The validator returns exit `1` and lists each failed check by field.
- The pretty-printer renders a `REFUSED` report (exit `1`) and shows the
  same failures.
- Neither tool repairs, trims, or partially accepts the document.
- Forbidden payloads are reported by field **name**; secret-pattern hits
  are reported by field **path**, never by value.
- A malformed or unreadable document is an IO error (exit `2`), never a
  silent empty success.

Treat any non-zero exit as **"do not act on this output"** and surface it
to the operator.

## 9. How The External-Verifier Handoff Stays Advisory

The optional `external_verifier_handoff` field is **advisory input only**
for a second opinion from an external reviewer (ChatGPT/Claude):

- it must be **secret-free** — a secret inside it is a validator refusal
  (reported by field path, never by value);
- sending it is an **operator gesture**, never automated by these tools;
- the external verifier's response is **advisory** and carries no write,
  approve, or apply authority;
- the handoff never appears as an authority field; the A2-L2b chain
  remains the only writer regardless of any external review.

## 10. Boundaries Recap

```text
read-only:      no file/dir created; input left unchanged; .claw untouched
no-command:     nothing in the document is ever executed
no-model:       no model/broker/Ollama; no :11435 or raw :11434 inference
no-approval:    no approval line generated, templated, or echoed
refusal:        reject-never-coerce; forbidden payloads named, not run;
                secrets shown by field path only, never by value
authority:      A2-L2b preview/approve/apply is the ONLY write authority
```

## 11. References

- [`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md)
  — the S2A contract (field sets and semantics).
- [`a2-l4-s2a1-json-schema-file-scope-card.md`](./a2-l4-s2a1-json-schema-file-scope-card.md)
  — S2A-1 schema-file scope card.
- [`a2-l4-s2a4-schema-validator-scope-card.md`](./a2-l4-s2a4-schema-validator-scope-card.md)
  — S2A-4 validator scope card.
- [`a2-l4-s2a6-planner-output-pretty-printer-scope-card.md`](./a2-l4-s2a6-planner-output-pretty-printer-scope-card.md)
  — S2A-6 pretty-printer scope card.
- [`../schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json)
  — the schema (S2A-2).
- [`../schemas/a2-l4/fixtures/planner-output/`](../schemas/a2-l4/fixtures/planner-output/)
  — the fixtures (S2A-3).
- [`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py)
  — the validator (S2A-5).
- [`../scripts/pretty_print_planner_output.py`](../scripts/pretty_print_planner_output.py)
  — the pretty-printer (S2A-7).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — the A2-L2b operator chain (the only write authority).

## 12. Status

```text
status: OPERATOR GUIDE — read-only usage of the S2A planner-output stack.

This guide documents existing read-only tooling. It authorizes no
implementation, no model execution, no direct writes, no approval bypass,
and no raw localhost:11434 app inference. The A2-L2b preview/approve/apply
chain remains the only write authority.
```
