# A2-L4-S2A Scope Card — Planner Output Contract (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2A lane. It
describes, in concept only, the output contract a future read-only
local-model task planner would emit: what fields it may carry, which are
required, which are optional, which are forbidden, what each means, and
how the output stays **inert and operator-routable without ever becoming
executable**. This file itself authorizes **no runtime change, no CLI
change, no schema file, no schema validator, no model execution, no
model load, no direct write, no approval bypass, no model-generated
approval line, and no raw `localhost:11434` app inference**.

A2-L4-S2A is the first sub-slice of A2-L4-S2. The S2 card
([`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md))
defined the planner as an **advisory, read-only, non-mutating,
non-authoritative** role and named its §22 sub-slice sequence:

> **S2A** — docs-only contract pinning the conceptual task-plan shape
> (S2 §15) into a concrete output schema. No code.

The S2 card explicitly held the task-plan contract **conceptual only**
and deferred any concrete schema to this slice. This card carries that
one step further — it pins the *conceptual contract shape, field
semantics, and the inertness/routability invariants* — but it still
does **not** create the schema file or validator; that is a future
sub-slice of S2A (§25, S2A-1/S2A-2).

### Must State

```text
This card authorizes design only.
It does not authorize implementation.
It does not create a schema file.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2A defines, in design only, the **planner output contract**: the
conceptual shape and per-field semantics of the inert task-plan object a
future read-only local-model planner (A2-L4-S2) would emit. The contract
exists so that a downstream operator — and, in separately-authorized
future lanes, a read-only adapter — can consume a structured plan
without any field carrying executable authority.

The recommended A2-L4-S2A scope is:

> Pin the conceptual planner-output contract — its required/optional/
> forbidden fields, per-field semantics, and the inertness and
> operator-routability invariants every field must hold — so a future
> schema/validator lane (S2A-1/S2A-2) can implement it safely, while the
> contract embeds no executable command, no approval line, no secret,
> and no write path; all model inference stays routed through the
> SideStack broker at `localhost:11435`; raw `localhost:11434` app
> inference is prohibited; and the A2-L2b chain remains the only write
> authority.

The implementation of A2-L4-S2A is **not authorized by this scope
card**, and **no schema file is created by it**. This card defines the
boundary the future schema/validator lanes (§25) must hold to. The next
gate before any implementation is operator review of this scope card.

## 2. Relationship To A2-L4-S2

A2-L4-S2A is bounded by, and subordinate to, the S2 card:

- S2 §13 ("Allowed Planner Outputs") named the inert output fields:
  `task_summary`, `repo_context_summary`, `candidate_files`,
  `proposed_plan_steps`, `risk_notes`, `test_suggestions`,
  `patch_intent`, `preview_request`, `external_verifier_handoff`.
- S2 §15 ("Task Plan Contract") declared the contract **conceptual
  only** and deferred a concrete schema to "a future sub-slice (S2A)".
- S2A inherits every S2 boundary verbatim: the planner is advisory,
  read-only, non-mutating, non-authoritative; the operator is the sole
  actor; A2 is the only write authority; routing is `:11435`-only; the
  VRAM boundary holds.

S2A does **not** widen the planner's role. It only sharpens the *shape
and semantics* of what the planner may emit, and the invariants that
keep every field inert. Where this card and the S2 card differ, the S2
card and (above it) the parent A2-L4 card remain authoritative.

## 3. Relationship To A2-L4 North Star

The A2-L4 North Star:

```text
Use local models to approximate Claude Code / Codex-style coding
assistance, while keeping A2 preview/approve/apply as the write
authority and ChatGPT/Claude as optional external verifier/reviewer.

The model may propose.
The operator decides.
A2 writes only after explicit approval.
```

A2-L4-S2A is the **shape of the proposal**. It makes the model's
"propose" step structured and reviewable while guaranteeing that no
field in the proposal can itself decide or write. "The model may
propose" becomes a concrete, inert contract; "the operator decides" and
"A2 writes only after explicit approval" stay untouched.

## 4. What S2A Adds

A2-L4-S2A adds, in design only:

1. A **conceptual contract shape** (§11) — the named output object.
2. A **required-field set** (§12), **optional-field set** (§13), and
   **forbidden-field set** (§14).
3. **Per-field semantics** (§15) defining what each field means and what
   it must never carry.
4. Sharpened **boundary statements** for `patch_intent` (§16),
   `preview_request` (§17), `test_suggestions` (§18), and
   `external_verifier_handoff` (§19).
5. **Inertness** (§21) and **operator-routability** (§22) requirements.
6. **Validation expectations** for a future implementation (§23) and the
   schema/validator sub-slices (§25) — none authorizing writes.

S2A adds **no schema file, no validator, and no write authority.** The
A2-L2b chain remains the only writer.

## 5. What S2A Does Not Add

A2-L4-S2A does **not** add:

- any schema file, JSON Schema, or validator code
- any planner implementation
- any write path, write command, or write flag
- any planner-initiated write, approve, apply, or apply-bundle
- any approval-bypass affordance or model-generated approval line
- any field that carries an executable command
- any change to `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, `claw plan apply`, or `claw plan status`
- any change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- any raw `localhost:11434` app-inference path
- any automatic model load, SGLang start, ComfyUI job, or GPU workload
- any new secret, key, or token surface

Any of the above must be opened as a separate, explicitly-authorized
lane and clear its own review. This card is **not** prior authorization
for it.

## 6. Planner Output Role

The planner output is **inert, structured advice**: a description of a
proposed approach that an operator reads and decides on. Its role is to:

- make the planner's reasoning legible and reviewable
- give the operator a single structured object to inspect
- support (in future, separately-authorized lanes) read-only
  consumption by an adapter, an external verifier, or a pretty-printer

The output's role is **not** to act. No field executes, writes,
approves, applies, or routes around the operator. Producing the output
applies nothing and stages nothing.

## 7. Operator Role

The operator remains the **sole actor** (per S2 §7 and parent §7):

- chooses the task and the model
- approves any VRAM/model lane
- reviews the planner output
- decides whether to shape a `plan.yaml` and run `claw plan run
  --workspace-write-preview`
- runs every A2 write-chain command and performs the TTY approval line
- decides whether to send an `external_verifier_handoff` to ChatGPT/
  Claude

Operator review is the gate between the planner output and any write.
Nothing in S2A removes, weakens, automates, or pre-empts that gate.

## 8. A2 Authority

```text
A2 preview/approval/apply remains the only write authority.
The planner output cannot directly write, approve, apply, or generate
approval lines.
```

The canonical A2-L2b chain is unchanged by S2A:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

The planner output's `preview_request` field (§17) is a *request the
operator may act on*, not an invocation. No field in the contract is a
chain command. S2A inserts no new step and replaces no step.

## 9. Broker / Model Routing Boundary

```text
All model-related app inference routes through localhost:11435.
Raw localhost:11434 app inference is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation.
```

S2A inherits the LAW-1 routing invariant the S2 and parent cards pin and
the read-only VS Code task wrapper enforces
([`editor-vscode.md`](./editor-vscode.md)): the effective
`OPENAI_BASE_URL` for any app inference that produces a planner output
must be the SideStack broker at `http://127.0.0.1:11435/v1` (see
`examples/sidestack-local.env`), and a base URL pointing at `:11434`
(raw Ollama) is a refusal, not a fallback.

Any `:11434` reference an implementation lane encounters must be
classified before the lane proceeds:

- **management** — Ollama admin/health endpoints used by the broker
  itself, not app inference.
- **docs/history** — a reference in documentation or git history with no
  live effect.
- **false positive** — a substring match that is not a base-URL or
  inference target.
- **violation** — a live app-inference path bypassing the broker. A
  STOP: the lane refuses to proceed until it routes through `:11435`.

A `raw_11434_endpoint` field is forbidden in the contract (§14): the
output must never carry a raw Ollama endpoint as data.

## 10. VRAM Safety Boundary

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker/current-holder/VRAM checks.
```

S2A is GPU-budget-aware by construction. Producing a planner output is a
read-and-reason step, not a model-management step:

- No S2A lane loads a model casually or as a side effect of building or
  validating the output contract.
- No S2A lane starts an SGLang server automatically.
- No S2A lane submits ComfyUI jobs or any media-generation work.
- No S2A lane runs heavy parallel inference.
- Any lane needing a GPU or a model must first confirm broker
  reachability, the current VRAM holder, and available headroom, and
  must defer to the operator on contention. Model choice and lane
  approval are operator-owned (§7).

## 11. Conceptual Contract Shape

The planner output is a single conceptual object. **This is conceptual
only — A2-L4-S2A does not create a schema file.** A future S2A
implementation lane (§25, S2A-1/S2A-2) must create the actual schema and
validator separately. Conceptual fields:

```text
schema_version
task_id
workspace_root
task_summary
repo_context_summary
candidate_files
plan_steps
risk_notes
test_suggestions
patch_intent
preview_request
external_verifier_handoff
status_snapshot
operator_next_steps
```

The field names above are conceptual labels for design discussion. The
concrete on-disk/wire names, types, and nesting are pinned by the future
schema lane, bounded by this card's required/optional/forbidden sets and
field semantics (§§12–15).

## 12. Required Fields

A future contract should treat these as **required** — every valid
planner output carries them:

- **`schema_version`** — the pinned contract-version literal, so
  consumers can reject drift.
- **`task_id`** — a stable identifier for the planning request (for
  operator correlation; not a credential).
- **`workspace_root`** — the workspace the plan was produced against,
  recorded verbatim.
- **`task_summary`** — a concise restatement of the operator task.
- **`plan_steps`** — the ordered, inert description of proposed steps.
- **`risk_notes`** — the planner's stated expected risk.
- **`operator_next_steps`** — inert, human-readable guidance on what the
  operator might do next (review, run a preview, run tests). Guidance,
  never an executed action.

A missing required field is a validation failure in the future
implementation (§23), not a silent default.

## 13. Optional Fields

A future contract should treat these as **optional** — present when the
planner has something to say, absent otherwise:

- **`repo_context_summary`** — an inert summary of relevant repo state.
- **`candidate_files`** — workspace-relative paths the plan touches, no
  path escape.
- **`test_suggestions`** — inert test/suite names or suggested commands
  for operator review (§18).
- **`patch_intent`** — an inert description of intended edits (§16).
- **`preview_request`** — an inert request that the operator generate an
  A2 preview (§17).
- **`external_verifier_handoff`** — an inert, secret-free handoff for
  optional ChatGPT/Claude review (§19).
- **`status_snapshot`** — an inert copy/summary of the read-only
  `a2-l2d-status.v1` envelope the planner observed, for context only.

An absent optional field carries no meaning beyond "not provided"; it
never implies a default action.

## 14. Forbidden Fields

A future contract must **forbid** any field that contains or implies
executable authority, a write, an approval, or a secret:

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

A forbidden field appearing in an output is a validation failure and a
STOP (§§23, 27). The future validator must reject — never coerce or
strip-and-accept — any output carrying a forbidden field.

## 15. Field Semantics

Per-field meaning and hard limits:

- **`schema_version`** — contract-version literal; consumers reject any
  other value. Carries no behavior.
- **`task_id`** / **`workspace_root`** — correlation/context only;
  `workspace_root` is recorded verbatim and never re-resolved into an
  action.
- **`task_summary`** / **`repo_context_summary`** — prose summaries;
  descriptive, never imperative-executable.
- **`candidate_files`** — workspace-relative path strings; data the
  operator may open, never auto-opened or auto-edited; no path escape.
- **`plan_steps`** — ordered inert descriptions of *what* the operator
  might change and in what order; never embedded shell/chain commands.
- **`risk_notes`** — the planner's risk assessment; advisory text.
- **`test_suggestions`** — inert test names / suggested commands for
  operator review; never executed (§18).
- **`patch_intent`** — inert description of intended edits, not an
  applyable artifact (§16).
- **`preview_request`** — inert request for an operator-run A2 preview
  (§17); never an invocation.
- **`external_verifier_handoff`** — inert, secret-free handoff for
  optional external review (§19); confers no write authority.
- **`status_snapshot`** — inert copy/summary of `a2-l2d-status.v1`;
  STOP-bearing values preserved verbatim, never coerced.
- **`operator_next_steps`** — inert guidance; suggestions the operator
  reads, never auto-run.

No field may carry an executable command, an approval line, an apply
bundle, a secret, or a raw `:11434` endpoint.

## 16. Patch Intent Boundary

```text
patch_intent is not a patch file.
patch_intent is not an applyable artifact.
patch_intent must not contain direct file replacement payloads unless a
future patch-proposal artifact lane explicitly authorizes that.
patch_intent may describe intended edits in prose or structured inert
notes only.
```

The inert patch-proposal *artifact* format is the parent card's A2-L4-S3
scope (parent §14). S2A's `patch_intent` is strictly the *description of
intent*, not the artifact. A `patch_intent` that tried to be an
applyable patch, a staged diff, or a pre-built A2 write artifact is a
category violation and a STOP.

## 17. Preview Request Boundary

```text
preview_request is inert.
preview_request does not run claw plan run.
preview_request does not create preview artifacts.
preview_request does not create approval lines.
preview_request is a request for the operator or a future authorized A2
integration lane to consider.
```

Acting on a `preview_request` is an operator gesture that routes through
the unchanged A2 chain (§8). Wiring a `preview_request` into an
operator-run preview is the S2 card's future sub-slice (S2 §22, S2D),
and even then the operator runs the preview — the contract field never
does.

## 18. Test Suggestion Boundary

```text
test_suggestions are inert.
They may name tests.
They may suggest commands for operator review.
They must not execute tests automatically.
They must not run shell commands.
```

A `test_suggestions` entry is advice the operator may choose to run in
their own environment. Test output the operator shares back is an
allowed read (S2 §12) the planner may use to revise a plan. Full test
request/report integration is the S2 card's future sub-slice (S2 §22,
S2F).

## 19. External Verifier Handoff Boundary

```text
external_verifier_handoff is operator-gated.
It must not contain secrets.
It must not contain token values.
It must not contain private keys.
It must not grant write authority to ChatGPT/Claude.
External verifier output is advisory only.
```

Sending a handoff to an external service is an **outward-facing
publish**; the operator decides what may leave the local environment.
The external verifier's output carries no write authority and cannot
approve, apply, or pre-fill the approval line. The concrete handoff
artifact format is a future sub-slice (§25, S2A-5; and S2 §22, S2E).

## 20. Secrets / Sensitive Data Boundary

- No contract field may carry a secret, token, env secret, private key,
  or credential (the forbidden-field set §14 names these explicitly).
- The planner must not read environment variables, shell history,
  terminal state, git credentials, tokens, or Vault material to populate
  any field. The `a2-l2d-status.v1` envelope contains no secrets by
  A2-L2d construction, and S2A introduces no path that injects any into
  `status_snapshot`.
- An `external_verifier_handoff` (§19) is an outward publish: it must be
  operator-gated and secret-free.
- S2A introduces no new credential, key, or token surface.

## 21. Inertness Requirements

Every field and the object as a whole must be **inert**:

- Producing or parsing the output applies nothing, writes nothing, and
  approves nothing.
- No field is interpreted as a command by any consumer. A future
  pretty-printer or adapter renders the output; it never executes it.
- The output never templates an approval line, `approval-result.json`,
  or `apply-bundle.json`.
- The output never mutates `.claw/**`, the workspace, or any file.
- A future validator must reject any output that embeds an executable
  command or a forbidden field (§14), rather than coercing it to inert.

## 22. Operator-Routability Requirements

The output should be understandable and actionable **by an operator**,
but **not executable by itself**. It should support:

```text
human review
A2 preview planning
test planning
external verifier handoff
risk assessment
```

It must not support:

```text
direct execution
direct file mutation
approval generation
apply generation
automation bypass
```

"Routable" means a human (or a future read-only consumer) can read the
plan and decide; it never means the plan auto-routes into a write.

## 23. Validation Expectations For Future Implementation

When a future schema/validator lane (§25) implements the contract, it
should:

- reject any output missing a required field (§12)
- reject any output carrying a forbidden field (§14)
- reject any field that embeds an executable command, approval line,
  apply bundle, secret, or raw `:11434` endpoint
- reject any `candidate_files` / path value that escapes the workspace
- preserve STOP-bearing `status_snapshot` values verbatim (never coerce)
- validate read-only and side-effect-free, mirroring the A2-L2d
  validator's read-only/network-egress-free posture
- never treat a validation pass as authorization to write, approve, or
  apply

These are expectations for a **future** lane; S2A implements no
validator and creates no schema file.

## 24. Failure Modes

The failure modes S2A must design against, each resolving to a
validation failure or STOP rather than a silent write:

1. **Executable field smuggling.** A field carrying a shell/chain
   command. → Forbidden-field set (§14); validator rejects (§23).
2. **Approval-line embedding.** Any field templating the TTY approval
   line. → Forbidden (§§14, 21); approval is operator-only.
3. **patch_intent overreach.** `patch_intent` emitted as an applyable
   patch. → Boundary held (§16); that is A2-L4-S3's scope.
4. **preview_request self-execution.** A field implying a run of `claw
   plan run`. → Inert request only (§17).
5. **Secret leakage.** A secret/token/key in any field or in a verifier
   handoff. → Forbidden-field set (§§14, 20); secret-free handoff (§19).
6. **Raw `:11434` smuggling.** A `raw_11434_endpoint` field or a `:11434`
   base URL in data. → Forbidden (§§9, 14).
7. **STOP coercion.** A `status_snapshot` STOP value normalized to "ok".
   → Preserved verbatim (§§15, 23).
8. **Schema-file scope creep.** This lane creating a schema/validator. →
   Out of scope (§§5, 25); STOP gate (§27).
9. **False-success masking.** Output reported as "applied" when no
   operator apply occurred. → Only an operator `claw plan apply` and the
   `applied` phase in `a2-l2d-status.v1` constitute applied state.

## 25. Future Implementation Constraints

Possible follow-up lanes, each its own separately-authorized lane. **No
slice may authorize direct writes.**

```text
S2A-1: JSON schema file scope card
S2A-2: schema/validator implementation
S2A-3: fixture pack for valid/invalid planner outputs
S2A-4: CLI pretty-printer for planner output
S2A-5: external verifier handoff format
```

Per-lane boundaries:

- **S2A-1** — docs-only scope card pinning the concrete JSON schema
  shape. No schema file yet; the card scopes it.
- **S2A-2** — the schema file and a read-only validator implementing
  §§12–15 and §23. Validates only; writes nothing, executes nothing.
- **S2A-3** — a fixture pack of valid and invalid planner outputs (incl.
  forbidden-field and executable-smuggling negatives). Test data only.
- **S2A-4** — a read-only CLI pretty-printer that renders a planner
  output for human review. Renders; never executes a field.
- **S2A-5** — the inert external-verifier handoff format (§19). Produces
  an artifact; sends nothing without an operator gesture.

Each lane opens as its own fresh-worktree PR with its own scope card or
implementation scope card, bounded by this card, the S2 card, and the
parent A2-L4 card.

## 26. Non-Goals

A2-L4-S2A is explicitly **not**:

- a schema file or validator (that is S2A-1/S2A-2)
- a planner implementation (that is S2 §22, S2B)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a patch generator or patch-proposal artifact (that is A2-L4-S3)
- a new write command, write flag, or approval-bypass affordance
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a requirement that an external verifier participate
- a SideStackAI infrastructure change

## 27. STOP Gates

Any future A2-L4-S2A implementation lane must STOP — escalate — and not
proceed if any of the following is true:

1. The lane would create a schema file or validator under *this* (S2A)
   scope card rather than under S2A-1/S2A-2.
2. A contract field would carry an executable command, an approval line,
   `approval-result.json`, or `apply-bundle.json`.
3. A contract field would carry a secret, token, env secret, or private
   key.
4. A contract field would carry a raw `localhost:11434` endpoint, or app
   inference would route to raw `:11434`.
5. A model load, SGLang start, or ComfyUI job would occur without
   broker/current-holder/VRAM checks.
6. The lane would run or template `claw plan run/approve/apply-bundle/
   apply`, or modify those or `claw plan status`.
7. The lane would modify `a2-l2d-status.v1` or any A2-L2b schema/marker.
8. A `status_snapshot` STOP value would be coerced, downgraded, or
   hidden.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 28. Definition Of Done

This **scope card** is done when:

- it defines the inert planner-output role and its hard limits
- it pins the conceptual contract shape (§11) without creating a schema
  file
- it defines required (§12), optional (§13), and forbidden (§14) fields
- it defines per-field semantics (§15)
- it draws the patch-intent (§16), preview-request (§17),
  test-suggestion (§18), and external-verifier (§19) boundaries
- it pins the secrets (§20), inertness (§21), and operator-routability
  (§22) requirements
- it states validation expectations for a future lane (§23) without
  implementing a validator
- it pins the broker `:11435` routing boundary and the `:11434`
  prohibition, and the VRAM boundary
- it preserves the A2-L2b `preview → approve → apply` chain as the only
  write authority
- it enumerates future sub-slices, none of which authorize direct writes
- it states plainly that it authorizes design only — no implementation,
  no schema file, no model execution, no direct writes, no approval
  bypass, no model-generated approval lines, no raw `:11434` app
  inference

A2-L4-S2A **implementation** (schema file, validator, fixtures,
pretty-printer) is out of scope for this card and is done only when each
separately-authorized lane (§25) lands under its own review.

## 29. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **S2A-1 (JSON schema file scope card, docs-only)**. S2A-1
> pins the concrete JSON schema shape for the contract this card
> describes, still without creating the schema file, bounded strictly by
> this card's §§11–23. The schema/validator implementation (S2A-2) and
> the remaining S2A lanes follow in their own per-lane PRs, each with its
> own scope card or implementation scope card, and none authorizing
> direct writes.

## 30. References

- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 parent slice (this card's §13/§15 source).
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference for the gated chain.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record (read-only contract a
  `status_snapshot` would summarize).
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md) —
  A2-L3 read-only harness adapter (a model for read-only consumers).
- [`a2-l3-ide-adapter-usage.md`](./a2-l3-ide-adapter-usage.md) — A2-L3
  read-only VS Code Claw Status Panel (a model for read-only consumers).
- [`editor-vscode.md`](./editor-vscode.md) — read-only VS Code task
  wrapper; source of the LAW-1 `:11435`-only routing refusal S2A
  inherits.

## 31. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize implementation.
It does not create a schema file.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The planner output contract is conceptual and inert until a future
schema/validator lane (S2A-1/S2A-2) is separately authorized.

Next gate: read-only operator review of this scope card.
```
