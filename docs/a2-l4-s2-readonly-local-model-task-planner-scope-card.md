# A2-L4-S2 Scope Card — Read-Only Local Model Task Planner (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2 lane. It
describes what a future read-only local-model task planner is, what it
may read, what it may produce, and what it must never become. This file
itself authorizes **no runtime change, no CLI change, no model
execution, no model load, no direct write, no approval bypass, no
model-generated approval line, and no raw `localhost:11434` app
inference**.

A2-L4-S2 is the second slice of the A2-L4 local-model coding loop. The
parent card
([`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md))
defined the loop's boundary: the local model is an **advisory
proposer**, the A2-L2b chain is the **only write authority**, and
ChatGPT/Claude are an **optional external verifier**. The parent card's
§21 implementation sequence names this slice:

> **L4-S2** — a read-only planner that reads allowed inputs and emits a
> plan proposal. No writes, no model auto-load, broker-routed inference
> only.

This card expands that one line into the planner's role, input/output
contracts, routing and VRAM boundaries, failure modes, and the
sub-slices a future implementation must hold to. It does not implement
the planner.

### Must State

```text
This card authorizes design only.
It does not authorize implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2 defines, in design only, a **read-only local-model task
planner**: a future component that reads operator-supplied task text and
allowed repository context, optionally invokes a broker-routed local
model for reasoning, and emits an **inert task plan** — a summary,
candidate files, ordered plan steps, risk notes, test suggestions, a
patch intent, an A2 preview request, and an optional external-verifier
handoff. Every output is **data the operator reads and decides on**,
never a command the planner executes.

The recommended A2-L4-S2 scope is:

> Define the read-only task-planner role and its input/output contracts
> so a future implementation can help an operator plan coding work —
> producing an inert task plan that the operator manually shapes into a
> `plan.yaml` for the unchanged A2-L2b `preview → approve → apply-bundle
> → apply` chain — while all app inference routes through the SideStack
> broker at `localhost:11435`, raw `localhost:11434` app inference is
> prohibited, no GPU/model lane runs without broker/current-holder/VRAM
> checks, and the planner never writes, approves, applies, or generates
> an approval line.

The implementation of A2-L4-S2 is **not authorized by this scope card**.
This card defines the boundary the future implementation lanes (§22)
must hold to. The next gate before any implementation is operator review
of this scope card.

## 2. Relationship To A2-L4 North Star

The A2-L4 North Star:

```text
Use local models to approximate Claude Code / Codex-style coding
assistance, while keeping A2 preview/approve/apply as the write
authority and ChatGPT/Claude as optional external verifier/reviewer.

The model may propose.
The operator decides.
A2 writes only after explicit approval.
```

A2-L4-S2 is the **planning front-end** of that loop. It is where the
local model first "thinks" about a task — but its thinking lands as an
inert plan, not an action. S2 advances the North Star by making the
model's proposal *structured and reviewable*, while leaving every write
decision with the operator and every write with A2.

## 3. Relationship To Existing A2 Chain

A2-L4-S2 sits **above** and **before** the A2 chain; it changes none of
it:

- **A2-L2b** — runtime-proven `preview → approve → apply-bundle → apply`
  chain. The only write authority. Unchanged by S2.
- **A2-L2c** — operator quick reference. Unchanged by S2.
- **A2-L2d** — read-only `claw plan status` + `a2-l2d-status.v1`
  envelope. S2 may *read* this to summarize chain phase, exactly as the
  A2-L3 adapters do. Unchanged by S2.
- **A2-L3 adapters** — read-only harness crate and VS Code panel
  consuming `a2-l2d-status.v1`. S2 is a sibling planning component, not a
  controller over them. Unchanged by S2.
- **A2-L4 (S1/parent)** — the advisory-loop scope. S2 is its second
  slice and inherits every boundary in it.

A task plan that the operator chooses to act on becomes input the
operator manually shapes into a `plan.yaml` for `claw plan run
--workspace-write-preview`. From that point the unchanged A2 chain
proceeds with its TTY approval boundary and explicit-apply boundary
fully intact. The planner never runs any of the four chain commands.

## 4. What S2 Adds

A2-L4-S2 adds, in design only:

1. A **planner role contract** — what the planner reads, produces, and
   may never do (§§6, 11–14).
2. An **allowed-inputs contract** — the operator-supplied task text,
   workspace root, and optional hints the planner consumes (§11).
3. An **allowed-outputs contract** — the inert task-plan fields the
   planner emits (§13), with the conceptual task-plan shape (§15).
4. A **broker-routed inference boundary** — any model reasoning routes
   through `localhost:11435`; raw `localhost:11434` app inference
   prohibited (§9).
5. A **VRAM safety boundary** — no casual loads, no auto SGLang/ComfyUI,
   no heavy parallel inference (§10).
6. Sub-slice definitions (§22) for a future implementation, none of
   which authorize direct writes.

S2 adds **no write authority of any kind.** The A2-L2b chain remains the
only writer.

## 5. What S2 Does Not Add

A2-L4-S2 does **not** add:

- any write path, write command, or write flag
- any planner-initiated write, approve, apply, or apply-bundle
- any composition of `approve` and `apply`
- any approval-bypass affordance or model-generated approval line
- any autonomous workspace-write execution
- any model execution of `claw plan run`, `claw plan approve`, `claw
  plan apply-bundle`, or `claw plan apply`
- any change to `claw plan status` or the `a2-l2d-status.v1` envelope
- any change to any A2-L2b schema or marker
- any raw `localhost:11434` app-inference path
- any automatic model load, SGLang start, ComfyUI job, or GPU workload
- any background daemon, watcher, or auto-trigger
- any IDE or harness write control
- any new secret, key, or token surface

Any of the above must be opened as a separate, explicitly-authorized
lane and clear its own review. This card is **not** prior authorization
for it.

## 6. Planner Role

The future planner may eventually:

```text
summarize operator task
summarize repo context
identify likely files
identify likely tests
propose an ordered task plan
propose risk notes
propose patch intent
request A2 preview generation
prepare an external verifier handoff
```

The future planner must remain:

```text
advisory
read-only
non-mutating
non-authoritative
```

The planner is **not** a workflow controller, **not** an executor, and
**not** an approver. It reads, reasons (via broker-routed inference),
and emits an inert plan. It does not orchestrate the chain, does not
pre-fill the approval line, does not generate `approval-result.json` or
`apply-bundle.json`, and does not retry, roll back, clean, or remediate.

## 7. Operator Role

The operator remains responsible for:

```text
choosing task
choosing model
approving VRAM/model lane
reviewing plan
running A2 write-chain commands
performing TTY approval
deciding when to ask ChatGPT/Claude for verification
```

The operator is the **only actor** that runs `claw plan run
--workspace-write-preview`, `claw plan approve`, `claw plan
apply-bundle`, and `claw plan apply`, and the only party who performs the
TTY-enforced approval line `apply <step_id> <preview_sha256>`. Operator
review is the gate between a plan and any write; nothing in S2 removes,
weakens, automates, or pre-empts that gate.

## 8. A2 Authority

```text
A2 preview/approval/apply remains the only write authority.
The model planner cannot directly write, approve, apply, or generate
approval lines.
```

The canonical A2-L2b chain is unchanged by S2:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

S2 inserts **no new step** and **replaces no step**. The planner's
`preview_request` output (§18) is a *request the operator may act on*,
not an invocation. The planner never touches any of the four commands.

## 9. Broker / Model Routing Boundary

```text
All app inference routes through localhost:11435.
Raw localhost:11434 app inference is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation.
```

A2-L4-S2 inherits the LAW-1 routing invariant the parent card pins and
the read-only VS Code task wrapper already enforces
([`editor-vscode.md`](./editor-vscode.md)): the effective
`OPENAI_BASE_URL` for any app inference must be the SideStack broker at
`http://127.0.0.1:11435/v1` (see `examples/sidestack-local.env`), and a
base URL pointing at `:11434` (raw Ollama) is a refusal, not a fallback.

Any `:11434` reference an implementation lane encounters must be
classified before the lane proceeds:

- **management** — Ollama admin/health endpoints used by the broker
  itself, not app inference. Out of S2's app path.
- **docs/history** — a reference in documentation or git history with no
  live effect. Recorded, not acted on.
- **false positive** — a substring match that is not a base-URL or
  inference target.
- **violation** — a live app-inference path bypassing the broker. A
  STOP: the lane refuses to proceed until it routes through `:11435`.

No A2-L4-S2 implementation lane may open a raw `:11434` app-inference
path under any justification.

## 10. VRAM Safety Boundary

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker/current-holder/VRAM checks.
```

A2-L4-S2 is GPU-budget-aware by construction:

- No S2 lane loads a model casually or as a side effect of a read,
  parse, or planning step.
- No S2 lane starts an SGLang server automatically.
- No S2 lane submits ComfyUI jobs or any media-generation work.
- No S2 lane runs heavy parallel inference.
- Any lane needing a GPU or a model must first confirm broker
  reachability, the current VRAM holder, and available headroom, and
  must defer to the operator on contention. A cold model load that would
  evict another holder is an operator decision, never an automatic one.
- "choosing model" and "approving VRAM/model lane" are operator
  responsibilities (§7); the planner proposes a model preference at
  most, and never loads one itself.

## 11. Allowed Inputs

The future planner may accept these operator-supplied inputs:

- **operator task text** — the natural-language description of what the
  operator wants planned
- **workspace root** — the workspace path to plan against
- **allowed file/path hints** — optional operator-supplied paths the
  planner may prioritize reading (within the workspace)
- **optional test target hints** — optional names of tests/suites the
  operator suggests the plan consider
- **optional model preference** — an optional operator hint about which
  broker-routed model to use (still subject to §10)
- **optional external verifier target** — an optional operator hint that
  a plan should be prepared for ChatGPT/Claude review (§19)

All inputs are operator-supplied. The planner does not infer a workspace
to plan against, does not widen its read scope beyond the workspace plus
operator-supplied hints, and treats every input as untrusted text — it
never executes an input.

## 12. Allowed Reads

The planner may read, **only through approved read-only means**:

- repository source files within the workspace root
- operator-supplied allowed file/path hints (within the workspace)
- the current `a2-l2d-status.v1` envelope via the read-only `claw plan
  status` command (network-egress-free, idempotent), to summarize chain
  phase the same way the A2-L3 adapters do
- existing A2 scope cards, usage guides, and operator quick references
- test output and build logs the operator chooses to share

The planner's reads must not mutate state, must not parse secrets
(§20), and must not reach the filesystem outside the workspace except
through operator-supplied, read-only inputs.

## 13. Allowed Planner Outputs

The future planner may emit these **inert data** outputs:

```text
task_summary
repo_context_summary
candidate_files
proposed_plan_steps
risk_notes
test_suggestions
patch_intent
preview_request
external_verifier_handoff
```

These must be **inert data, not commands that execute writes.** Emitting
any of them applies nothing, writes nothing, and approves nothing. The
operator inspects them and decides what, if anything, to act on.

## 14. Forbidden Planner Actions

The planner must **not**:

```text
write files
edit files
stage files
commit
push
run write-chain commands
run approve/apply/apply-bundle
generate approval lines
start services
load models without approval
touch secrets
mutate .claw/**
mutate workspace files
trigger IDE write controls
call raw localhost:11434 for app inference
```

In addition, the planner must not:

- compose `approve` and `apply` into a single action
- generate `approval-result.json` or `apply-bundle.json`
- pre-fill, auto-complete, or sign the TTY approval line
- start an SGLang server, submit ComfyUI jobs, or run GPU workloads
  (§10)
- call broker, model, Ollama, telemetry, or analytics endpoints for any
  purpose other than approved, broker-routed inference
- background-poll, watch the filesystem, or self-trigger
- coerce, downgrade, debounce, or hide any STOP signal from
  `a2-l2d-status.v1`

Each forbidden action remains forbidden regardless of how a future lane
is framed. The planner is advisory; none of these are advisory acts.

## 15. Task Plan Contract

The task plan is **conceptual only in this scope card.** A2-L4-S2 does
**not** create a schema file; pinning a concrete schema is a future
sub-slice (§22, S2A). Conceptually, a task plan should carry:

```text
schema_version
task_id
workspace_root
task_summary
candidate_files
plan_steps
risk_notes
test_suggestions
preview_request
external_verifier_notes
```

Conceptual invariants the future contract must hold:

- **Inert.** The plan is data the operator inspects; producing it applies
  nothing and stages nothing.
- **No approval content.** It must not contain, imply, or template an
  approval line, an `approval-result.json`, or an `apply-bundle.json`.
- **Workspace-bounded.** `candidate_files` and any path reference name
  workspace-relative paths with no path escape.
- **Operator-routable, not auto-routed.** The operator — not the planner,
  not any adapter — decides whether to shape the plan into a `plan.yaml`
  for `claw plan run`.
- **Versioned.** A future on-disk format is pinned by its own sub-slice
  (S2A); this card pins only the inertness and operator-routing
  invariants, not the wire format.

## 16. Patch Proposal Boundary

S2 is a **planner**, not a patch generator. The boundary between S2 and
the patch-proposal slice (A2-L4-S3) is:

- S2 may emit a **`patch_intent`** — a description of *what* the operator
  might change and *why*, expressed as inert plan data.
- S2 must **not** emit a literal applyable patch, a diff staged for
  application, or any artifact that a write step could consume directly.
  The inert patch-proposal artifact format is A2-L4-S3's scope, governed
  by the parent card §14.
- A `patch_intent` that tried to apply itself, stage itself, or
  pre-build any A2 write artifact would be a category violation and a
  STOP.

## 17. Test Request Boundary

The planner may **suggest** tests; it does not run the write chain to
obtain them:

- A `test_suggestions` output names tests/suites the operator might run
  and why. It is advice.
- The operator decides whether to run the suggestion, and runs it in
  their own environment.
- Test output the operator shares back is an **allowed read** (§12) the
  planner may use to revise a plan.
- A test suggestion must never be a disguised write, a chain-write
  command, or a planner-initiated subprocess. The planner does not
  execute tests itself; it suggests. Full test request/report
  integration is a future sub-slice (§22, S2F).

## 18. A2 Preview Request Boundary

The planner may emit a **`preview_request`** — a request that the
operator generate an A2 preview — without running any write-chain
command itself:

- A `preview_request` is **inert data**: it names the proposed
  `plan.yaml` shape and the workspace root, as something the operator may
  feed to `claw plan run --workspace-write-preview`.
- The planner must **not** run `claw plan run` (or any chain command),
  must **not** generate the preview, and must **not** generate the
  approval line.
- Acting on a `preview_request` is an operator gesture that routes
  through the unchanged A2 chain (§8). Wiring a `preview_request` into an
  operator-run preview is a future sub-slice (§22, S2D), and even then
  the operator runs the preview — the planner never does.

## 19. External Verifier Handoff Boundary

The planner may prepare an **`external_verifier_handoff`** for optional
ChatGPT/Claude review:

- The handoff is **inert data** the operator may choose to send to an
  external verifier for a second opinion. It carries no write authority.
- Sending a handoff to an external service is an **outward-facing
  publish**; the operator decides what may leave the local environment,
  and the handoff must carry no secrets (§20).
- The external verifier is **optional**. The planner is fully functional
  with local-model reasoning and operator review alone; the verifier
  handoff only enables an optional review pass. The verifier's output
  carries no write authority and cannot approve, apply, or pre-fill the
  approval line. The handoff artifact format is a future sub-slice (§22,
  S2E).

## 20. Secrets / Sensitive Data Boundary

- The planner must not read environment variables, shell history,
  terminal state, git credentials, tokens, or Vault material. The
  `a2-l2d-status.v1` envelope contains no secrets by A2-L2d
  construction, and S2 introduces no path that injects any.
- Task plans, patch intents, test suggestions, and preview requests must
  not embed secrets.
- An `external_verifier_handoff` (§19) is an outward publish: it must be
  operator-gated and must carry no secret material.
- All broker-routed inference uses the operator's configured local
  broker; S2 introduces no new credential, key, or token surface.

## 21. Failure Modes

The failure modes S2 must design against, each resolving to a refusal or
a STOP rather than a silent write:

1. **Plan-to-apply drift.** A plan silently becoming a write without an
   operator gesture. → Plans are inert (§15); only operator-run chain
   commands write.
2. **Approval pre-fill.** The planner or a plan templating the TTY
   approval line. → Forbidden (§§7, 8, 14, 15); approval is operator-only.
3. **Raw `:11434` bypass.** Inference routed around the broker. →
   Refusal at the routing boundary (§9).
4. **Casual VRAM load.** A model load or SGLang/ComfyUI start as a side
   effect. → Forbidden without broker/current-holder/VRAM checks (§10).
5. **Patch-intent overreach.** `patch_intent` emitted as an applyable
   patch. → Boundary held; that is A2-L4-S3's scope (§16).
6. **Preview self-execution.** The planner running `claw plan run`. →
   Forbidden; `preview_request` is inert (§18).
7. **Secret leakage to external verifier.** A handoff carrying secrets.
   → Operator-gated, secret-free publish only (§§19, 20).
8. **STOP suppression.** The planner coercing, downgrading, or hiding a
   `a2-l2d-status.v1` STOP to keep planning. → STOP signals preserved
   verbatim; the planner yields to operator escalation.
9. **False-success masking.** A plan reported as "applied" when no
   operator apply occurred. → Only an operator `claw plan apply` and the
   `applied` phase in `a2-l2d-status.v1` constitute applied state.

## 22. Future Implementation Constraints

Possible follow-up sub-slices, each its own separately-authorized lane.
**No slice may authorize direct writes.**

```text
S2A: planner output contract scope card
S2B: read-only planner CLI implementation
S2C: broker-routed local model invocation adapter
S2D: A2 preview request integration
S2E: external verifier handoff artifact
S2F: test request/report integration
```

Per-sub-slice boundaries:

- **S2A** — docs-only contract pinning the conceptual task-plan shape
  (§15) into a concrete output schema. No code.
- **S2B** — a read-only planner CLI that reads allowed inputs (§11),
  reads allowed sources (§12), and emits the inert task plan (§13). No
  writes, no model auto-load.
- **S2C** — a broker-routed local-model invocation adapter that routes
  all inference through `:11435` (§9) under the VRAM boundary (§10).
  Inference only; no writes.
- **S2D** — wiring a `preview_request` into operator-run `claw plan run
  --workspace-write-preview` for **preview only** (§18). The operator
  runs the preview; the planner never does.
- **S2E** — the inert external-verifier handoff artifact (§19). Produces
  an artifact; sends nothing without an operator gesture.
- **S2F** — test request/report integration (§17). Suggests and reads;
  never runs the write chain.

Each sub-slice opens as its own fresh-worktree PR with its own scope card
or implementation scope card, bounded by this card and the parent A2-L4
card.

## 23. Non-Goals

A2-L4-S2 is explicitly **not**:

- an autonomous coding agent that writes, approves, or applies on its own
- a replacement for the A2-L2b operator-gated chain
- a patch generator (that is A2-L4-S3)
- a new write command, write flag, or approval-bypass affordance
- a model-driven CI write step
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a promotion of any A2-L3 read-only adapter into a controller
- a requirement that an external verifier participate
- a SideStackAI infrastructure change

## 24. STOP Gates

Any future A2-L4-S2 implementation lane must STOP — escalate — and not
proceed if any of the following is true:

1. A plan would write, stage, or apply without an operator gesture.
2. Any output contains or templates an approval line,
   `approval-result.json`, or `apply-bundle.json`.
3. App inference would route to raw `localhost:11434`.
4. A model load, SGLang start, or ComfyUI job would occur without
   broker/current-holder/VRAM checks.
5. The lane would run `claw plan run/approve/apply-bundle/apply` from the
   planner, or modify those commands or `claw plan status` behavior, exit
   codes, schemas, markers, or JSON field shapes.
6. The lane would modify `a2-l2d-status.v1` or any A2-L2b schema/marker.
7. The lane would promote an A2-L3 read-only adapter into a write
   controller.
8. The lane would read secrets into a plan or send secret material to an
   external verifier.
9. A STOP signal from `a2-l2d-status.v1` would be coerced, downgraded,
   debounced, or hidden by the planner.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 25. Definition Of Done

This **scope card** is done when:

- it defines the read-only planner role and its hard limits
- it defines the allowed inputs, allowed reads, and allowed inert outputs
- it pins the broker `:11435` routing boundary and the `:11434`
  prohibition
- it pins the VRAM safety boundary
- it draws the planner/patch (§16), planner/preview (§18), and
  planner/verifier (§19) boundaries
- it states the task plan is conceptual only (no schema file created)
- it preserves the A2-L2b `preview → approve → apply` chain as the only
  write authority
- it enumerates future sub-slices, none of which authorize direct writes
- it states plainly that it authorizes design only — no implementation,
  no model execution, no direct writes, no approval bypass, no
  model-generated approval lines, no raw `:11434` app inference

A2-L4-S2 **implementation** is out of scope for this card and is done
only when each separately-authorized sub-slice (§22) lands under its own
review.

## 26. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **S2A (planner output contract scope card, docs-only)**.
> S2A pins the conceptual task-plan shape (§15) into a concrete output
> schema without any code, model execution, or write path, bounded
> strictly by this card's §§6–24. Code-bearing sub-slices (S2B onward)
> follow in their own per-slice PRs, each with its own scope card or
> implementation scope card, and none authorizing direct writes.

## 27. References

- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (this slice's §21 source).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference for the gated chain.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record (read-only contract the planner
  reads).
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md) —
  A2-L3 read-only harness adapter usage guide.
- [`a2-l3-ide-adapter-usage.md`](./a2-l3-ide-adapter-usage.md) — A2-L3
  read-only VS Code Claw Status Panel usage guide.
- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 adapter boundary and its read-only-observer invariants.
- [`editor-vscode.md`](./editor-vscode.md) — read-only VS Code task
  wrapper; source of the LAW-1 `:11435`-only routing refusal S2 inherits.

## 28. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize model-generated approval lines.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The local model task planner is advisory and read-only until a future
sub-slice explicitly wires a request through A2 under its own
authorization.

Next gate: read-only operator review of this scope card.
```
