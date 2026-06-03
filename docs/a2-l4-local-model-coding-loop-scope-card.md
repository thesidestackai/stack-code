# A2-L4 Scope Card — Local Model Coding Loop (Docs-Only)

This document is a **design-only scope card** for the A2-L4 lane. It
describes what an A2-L4 local-model coding loop is, what it must never
become, how it routes model traffic, and the validation any future
implementation lane must clear before it is allowed to land. This file
itself authorizes **no runtime change, no CLI change, no model
execution, no direct write, no approval bypass, and no raw
`localhost:11434` app inference**.

A2-L4 is the next conceptual layer above the A2-L3 read-only adapters.
A2-L2b proved the operator-gated `preview → approve → apply-bundle →
apply` chain at runtime
([`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)).
A2-L2c closed the operator-ergonomics docs gap
([`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md)). A2-L2d
added the read-only `claw plan status` command and the
`a2-l2d-status.v1` envelope
([`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)). A2-L3 added
read-only adapters that *consume* that envelope — the harness adapter
crate
([`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md))
and the VS Code Claw Status Panel
([`a2-l3-ide-adapter-usage.md`](./a2-l3-ide-adapter-usage.md)). Every
one of those layers is read-only or operator-gated. A2-L4 asks the next
question: **can a local model participate in the loop as an advisory
proposer without ever becoming a writer?**

The answer this card commits to is: **yes, but only as an advisory
proposer, and only through the existing A2 chain as the sole write
authority.**

## 1. Executive Summary

A2-L4 defines, in design only, how a **local model** may participate in
a coding-agent loop on this repository while preserving the existing A2
safety chain end-to-end. The local model is an **advisory proposer**: it
may read allowed inputs, summarize repository state, propose a plan,
propose patch text, request tests, and request that the operator
generate an A2 preview. It may **never** write files, approve changes,
apply changes, generate approval lines, or bypass operator review.

The recommended A2-L4 scope is:

> Define the boundary, routing, VRAM, and artifact contracts under which
> a local model can approximate Claude Code / Codex-style coding
> assistance as an **advisory proposer only** — producing plan proposals
> and patch-proposal artifacts that an operator manually feeds into the
> unchanged A2-L2b `preview → approve → apply-bundle → apply` chain —
> while ChatGPT/Claude remain an optional external verifier, all app
> inference routes through the SideStack broker at `localhost:11435`,
> raw `localhost:11434` app inference is prohibited, and no GPU/model
> lane runs without explicit broker/current-holder/VRAM checks.

The implementation of A2-L4 is **not authorized by this scope card**.
This card defines the boundary the future implementation lanes must hold
to. The next gate before any implementation is operator review of this
scope card.

### Must State

```text
This card authorizes design only.
It does not authorize implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize raw localhost:11434 app inference.
```

## 2. North Star

```text
Use local models to approximate Claude Code / Codex-style coding
assistance, while keeping ChatGPT/Claude as optional external
verifier/reviewer.
```

Restated as the operating invariant for this lane:

- The local model is the **engine of proposals**, never the engine of
  changes.
- The A2-L2b chain is the **only write authority**.
- ChatGPT/Claude are **optional external verifiers**, never required and
  never granted write authority.
- The model **may propose** changes. The model **must not** directly
  write, approve, apply, or bypass the A2 chain.

## 3. Current Foundation

The A2 stack A2-L4 builds on, all merged on `origin/main`:

- **A2-L2b** — runtime-proven operator-gated `preview → approve →
  apply-bundle → apply` chain. The only write authority in the system.
  Unchanged by A2-L4.
- **A2-L2c** — operator quick reference for the A2-L2b chain. Docs-only.
  Unchanged by A2-L4.
- **A2-L2d** — read-only `claw plan status <workspace>
  [<approval-result.json>]` command and the `a2-l2d-status.v1` envelope.
  Read-only, network-egress-free, idempotent. Unchanged by A2-L4.
- **A2-L3 harness adapter** — read-only assertion/reporting crate at
  `rust/crates/a2-harness-adapter/` consuming `a2-l2d-status.v1`. Never
  invokes a chain-write command. Unchanged by A2-L4.
- **A2-L3 IDE adapter** — read-only VS Code Claw Status Panel at
  `ide/vscode/claw-status-panel/` (PR #52, `553434a`) consuming
  `a2-l2d-status.v1`. A viewer, never a controller. Unchanged by A2-L4.

The repository also ships, outside the A2 lanes but relevant to A2-L4
routing:

- `examples/sidestack-local.env` — sets
  `OPENAI_BASE_URL="http://127.0.0.1:11435/v1"` (the SideStack broker
  route), sourced by `scripts/claw-sidestack-local`.
- `docs/editor-vscode.md` — the read-only VS Code task wrapper, whose
  LAW-1 routing wrapper **refuses to exec `claw` if the effective base
  URL points at `:11434` (raw Ollama)**. A2-L4 inherits this refusal as
  a hard invariant.

## 4. What A2-L4 Adds

A2-L4 adds, in design only, a defined role for a **local model as an
advisory proposer** in the coding loop, plus the contracts that keep
that role safe:

1. A **local model role contract** — what the model may read, may
   propose, and may never do (§§6, 11, 12, 13).
2. A **patch-proposal artifact contract** — a structured, inert,
   non-applying representation of proposed changes the operator can
   inspect and choose to route into the A2 chain (§14).
3. A **broker/model routing boundary** — all app inference through
   `localhost:11435`; raw `localhost:11434` app inference prohibited
   (§9).
4. A **VRAM safety boundary** — no casual loads, no automatic SGLang
   starts, no ComfyUI jobs, no heavy parallel inference; any GPU/model
   lane requires broker/current-holder/VRAM checks (§10).
5. An **external-verifier role** — ChatGPT/Claude as an optional review
   pass over a proposal, never a write authority (§17).

A2-L4 adds **no new write authority of any kind.** The A2-L2b chain
remains the only writer.

## 5. What A2-L4 Does Not Add

A2-L4 does **not** add:

- any new write path, write command, or write flag
- any model-initiated write, approve, apply, or apply-bundle affordance
- any composition of `approve` and `apply`
- any approval-bypass affordance (`--yes`, `--auto`, `--skip-approval`,
  `--no-prompt`, preapproval, batch approval)
- any autonomous workspace-write execution
- any change to `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, `claw plan apply`, or `claw plan status` behavior, exit
  codes, schemas, markers, or JSON field shapes
- any change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- any raw `localhost:11434` app-inference path
- any background model daemon, watcher, or auto-trigger
- any new IDE write control or harness write control

Any of the above must be opened as a separate, explicitly-authorized
lane and clear its own review. This card is **not** prior authorization
for it.

## 6. Local Model Role

The local model is an **advisory proposer**. Concretely, within an
A2-L4 loop the model:

- **Reads** allowed inputs through approved read-only tools (§11).
- **Summarizes** repository state and the current `a2-l2d-status.v1`
  chain phase.
- **Proposes** a plan: an ordered description of intended changes.
- **Proposes** patch text: the literal proposed edit, emitted as an
  inert patch-proposal artifact (§14), never written to a target file.
- **Requests** that tests be run (it does not run the write chain to do
  so; §16).
- **Requests** that the operator generate an A2 preview from a proposal
  (§15).
- **Explains** the expected risk of a proposal.

The model's output is **advice and artifacts**, not actions. The model
is advisory until and unless a future, separately-authorized
implementation lane explicitly wires a proposal into the A2 chain — and
even then the A2-L2b operator-gated approval and explicit-apply
boundaries remain intact and TTY-enforced.

The model is **not** a workflow controller. It does not orchestrate the
chain, does not pre-fill the approval line, does not generate
`approval-result.json` or `apply-bundle.json`, and does not retry, roll
back, clean, or remediate on a STOP signal.

## 7. Operator Role

The operator is the **sole actor** in the loop. The operator:

- Decides whether to accept, reject, or revise any model proposal.
- Is the **only** party who runs `claw plan run
  --workspace-write-preview`, `claw plan approve`, `claw plan
  apply-bundle`, and `claw plan apply`.
- Performs the TTY-enforced approval line `apply <step_id>
  <preview_sha256>` themselves; no model, artifact, or external verifier
  may pre-fill, auto-complete, or sign it.
- Inspects every `evidence_paths` entry and every `stop_condition` when
  a STOP fires, and escalates rather than coercing the chain forward.
- Decides whether to invoke an optional external verifier (§17) and how
  to weigh its output.
- Owns every VRAM/GPU decision (§10) and confirms broker/current-holder
  state before any model lane runs.

Operator review is the gate between a proposal and any write. Nothing in
A2-L4 removes, weakens, automates, or pre-empts that gate.

## 8. A2 Chain Authority

```text
A2 preview/approval/apply remains the only write authority.
The local model is advisory until a future implementation explicitly
wires it through A2.
```

The canonical A2-L2b chain is unchanged by A2-L4:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

A2-L4 inserts **no new step** into this chain and **replaces no step**.
A model proposal that the operator chooses to act on becomes input the
operator manually shapes into a `plan.yaml` for `claw plan run`. From
that point the chain proceeds exactly as A2-L2b/L2c/L2d already define
it, with the TTY approval boundary and explicit-apply boundary fully
intact. The model never touches any of these four commands.

## 9. Broker / Model Routing Boundary

```text
All app inference must route through localhost:11435.
Raw localhost:11434 app-inference paths are prohibited.
Any 11434 references must be classified as management, docs/history,
false positive, or violation.
```

A2-L4 inherits the LAW-1 routing invariant already enforced for the
read-only VS Code task wrapper
([`docs/editor-vscode.md`](./editor-vscode.md)): the effective
`OPENAI_BASE_URL` for any app inference must be the SideStack broker at
`http://127.0.0.1:11435/v1` (see `examples/sidestack-local.env`), and a
base URL pointing at `:11434` (raw Ollama) is a refusal, not a fallback.

Any `:11434` reference an implementation lane encounters must be
classified before the lane proceeds:

- **management** — Ollama admin/health endpoints used by the broker
  itself, not app inference. Out of A2-L4's app path.
- **docs/history** — a reference in documentation or git history with no
  live effect. Recorded, not acted on.
- **false positive** — a substring match that is not a base-URL or
  inference target.
- **violation** — a live app-inference path bypassing the broker. A
  STOP: the lane refuses to proceed until it routes through `:11435`.

This boundary is a hard invariant. No A2-L4 implementation lane may open
a raw `:11434` app-inference path under any justification.

## 10. VRAM Safety Boundary

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker/current-holder/VRAM checks.
```

A2-L4 is GPU-budget-aware by construction:

- No A2-L4 lane loads a model casually or as a side effect of a docs,
  parser, or artifact step.
- No A2-L4 lane starts an SGLang server automatically.
- No A2-L4 lane submits ComfyUI jobs or any media-generation work.
- No A2-L4 lane runs heavy parallel inference.
- Any lane that needs a GPU or a model must first confirm broker
  reachability, the current VRAM holder, and available headroom, and
  must defer to the operator on contention. A cold model load that would
  evict another holder is an operator decision, never an automatic one.

This card authorizes none of the above as actions; it pins them as
boundaries any future implementation lane must hold.

## 11. Allowed Reads

The model may read, **only through approved read-only tools**:

- repository source files within the workspace
- the current `a2-l2d-status.v1` envelope (via `claw plan status`, which
  is itself read-only and network-egress-free)
- existing A2 scope cards, usage guides, and operator quick references
- test output and build logs the operator chooses to share
- an explicitly operator-supplied `<approval-result.json>` **as a read**
  (never as an approval; mirrors A2-L2d's read-only treatment)

The model's reads must not mutate state, must not parse secrets, and
must not reach the filesystem outside the workspace except through the
same operator-supplied, read-only positional contract A2-L2d already
defines.

## 12. Allowed Proposals

The model may propose:

```text
read allowed files through approved tools
summarize repository state
propose a plan
propose patch text
request tests
request A2 preview generation
explain expected risk
```

Every one of these is **advisory output**, not an action. A proposal is
inert until an operator chooses to act on it, and acting on it always
routes through the unchanged A2-L2b chain.

## 13. Forbidden Actions

The model must **not**:

```text
write files directly
approve changes
apply changes
generate approval lines
bypass operator review
run write-chain commands
start services
load models
touch secrets
mutate .claw/**
mutate workspace outside approved apply path
```

In addition, the model must not:

- compose `approve` and `apply` into a single action
- generate `approval-result.json` or `apply-bundle.json`
- pre-fill, auto-complete, or sign the TTY approval line
- open a raw `localhost:11434` app-inference path (§9)
- start an SGLang server, submit ComfyUI jobs, or trigger model loads
  (§10)
- call broker, model, Ollama, telemetry, or analytics endpoints for any
  purpose other than approved, broker-routed inference
- background-poll, watch the filesystem, or self-trigger
- coerce, downgrade, debounce, or hide any STOP signal

Each forbidden action remains forbidden regardless of how a future lane
is framed. The model is advisory; none of these are advisory acts.

## 14. Patch Proposal Contract

A2-L4 patches are **proposals, not writes**. The future patch-proposal
artifact must satisfy:

- **Inert.** The artifact is data the operator inspects; producing it
  applies nothing. It is never written to a target source file and never
  staged.
- **Explicit target.** It names the workspace-relative target path(s) it
  proposes to change, with no path escape outside the workspace.
- **Self-describing risk.** It carries the model's stated expected risk
  and rationale alongside the proposed text.
- **Operator-routable, not auto-routed.** The operator — not the model,
  not any adapter — decides whether to shape the proposal into a
  `plan.yaml` for `claw plan run`. There is no path by which an artifact
  flows into a write without an operator gesture.
- **No approval content.** The artifact must not contain, imply, or
  template an approval line, an `approval-result.json`, or an
  `apply-bundle.json`.
- **Versioned and bounded.** Any future on-disk artifact format is
  pinned by its own schema lane; this card does not pin the wire format,
  only the inertness and operator-routing invariants.

A patch proposal that tried to apply itself, stage itself, or pre-build
any A2 write artifact would be a category violation and a STOP.

## 15. Preview / Approval / Apply Contract

The A2-L2b `preview → approve → apply` contract is **untouched** by
A2-L4 and remains the only write path:

- **Preview** is generated only by an operator running `claw plan run
  --workspace-write-preview`. The model may *request* that the operator
  do this; it may not run it.
- **Approval** is performed only by the operator via `claw plan approve`
  with the TTY-enforced approval line. No model, artifact, or external
  verifier may approve, pre-fill, or bypass it.
- **Apply-bundle** is generated only by the operator running `claw plan
  apply-bundle`. The model may not hand-build it.
- **Apply** is performed only by the operator running `claw plan apply`
  on an operator-generated apply-bundle.

`claw plan status` (A2-L2d) remains available as a read-only inspection
between these steps. A2-L4 adds nothing to it and weakens none of its
read-only invariants.

## 16. Test / Validation Request Contract

The model may **request** tests and validation; it does not run the
write chain to obtain them:

- A test request is advisory: it names what the model would like
  exercised (e.g. "run `cargo test -p <crate>`") and why.
- The operator decides whether to run the request, and runs it in their
  own environment.
- Test output the operator shares back is an **allowed read** (§11) that
  the model may use to revise a proposal.
- A test request must never be a disguised write, a chain-write command,
  or a model-initiated subprocess. The model does not execute tests
  itself; it asks.
- Any inference involved in interpreting test output routes through the
  broker at `:11435` (§9) and respects the VRAM boundary (§10).

## 17. External Verifier Role

ChatGPT/Claude (or any external model) is an **optional external
verifier**, never required and never a writer:

- The operator may choose to send a model proposal to an external
  verifier for a second-opinion review.
- The external verifier's output is **advisory review**, weighed by the
  operator like any other input. It carries no write authority and
  cannot approve, apply, or pre-fill the approval line.
- Sending a proposal to an external service is an **outward-facing
  publish**; the operator decides what may leave the local environment,
  and secrets must never be included (§19).
- The external verifier is **optional**. The A2-L4 loop is fully
  functional with local-model proposals and operator review alone; the
  external verifier only adds an optional review pass.

## 18. IDE / Harness Relationship

A2-L4 is a **sibling** of the A2-L3 adapters, not a parent or a rewrite:

- The A2-L3 IDE panel and harness adapter remain **read-only consumers**
  of `a2-l2d-status.v1`. A2-L4 does not add a write affordance to either
  and does not ask either to invoke a chain-write command.
- An A2-L4 loop may *read* `claw plan status` output the same way the
  adapters do, to summarize chain phase for the operator.
- No A2-L4 lane may extend an A2-L3 adapter into a controller, add a
  "propose-and-apply" button, or wire a model proposal into an
  adapter-driven write. Each remains a viewer/reporter.
- Any A2-L4 surface that an IDE or harness later consumes inherits the
  A2-L3 adapter-boundary invariants
  ([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)).

## 19. Security / Secrets Boundary

- The model must not read environment variables, shell history, terminal
  state, git credentials, tokens, or Vault material. The
  `a2-l2d-status.v1` envelope contains no secrets by A2-L2d
  construction, and A2-L4 introduces no path that injects any.
- Patch proposals and test requests must not embed secrets.
- Sending any proposal to an external verifier (§17) is an outward
  publish: it must be operator-gated and must carry no secret material.
- All broker-routed inference uses the operator's configured local
  broker; A2-L4 introduces no new credential, key, or token surface.

## 20. Failure Modes

The failure modes A2-L4 must design against, each resolving to a refusal
or a STOP rather than a silent write:

1. **Proposal-to-apply drift.** A proposal silently becoming a write
   without an operator gesture. → Patch proposals are inert (§14); only
   operator-run chain commands write.
2. **Approval pre-fill.** A model or artifact templating the TTY
   approval line. → Forbidden (§§7, 13, 14); approval is operator-only.
3. **Raw `:11434` bypass.** App inference routed around the broker. →
   Refusal at the routing boundary (§9).
4. **Casual VRAM load.** A model load or SGLang/ComfyUI start as a side
   effect. → Forbidden without broker/current-holder/VRAM checks (§10).
5. **Secret leakage to external verifier.** A proposal carrying secrets
   to ChatGPT/Claude. → Operator-gated, secret-free publish only (§§17,
   19).
6. **Adapter promotion.** An A2-L3 read-only adapter gaining a write
   button via A2-L4. → Adapters stay read-only consumers (§18).
7. **STOP suppression.** A loop coercing, downgrading, or hiding a STOP
   to keep proposing. → STOP signals preserved verbatim; the loop yields
   to operator escalation.
8. **False-success masking.** A proposal reported as "applied" when no
   operator apply occurred. → Only an operator `claw plan apply` and the
   `applied` phase in `a2-l2d-status.v1` constitute applied state.

## 21. Implementation Slices

Safe future slices, each its own separately-authorized lane. **No slice
may authorize direct writes.**

```text
L4-S1: local model coding-loop architecture docs
L4-S2: read-only local model task planner
L4-S3: patch proposal artifact format
L4-S4: A2 preview integration, still no apply
L4-S5: operator review + external verifier handoff
L4-S6: test request/report integration
```

Per-slice boundaries:

- **L4-S1** — docs-only architecture for the advisory loop. No code.
- **L4-S2** — a read-only planner that reads allowed inputs and emits a
  plan proposal. No writes, no model auto-load, broker-routed inference
  only.
- **L4-S3** — the inert patch-proposal artifact format (§14). Produces
  artifacts; applies nothing.
- **L4-S4** — wiring a proposal into operator-run `claw plan run
  --workspace-write-preview` for **preview only**. Still no model
  approve, no model apply; the operator runs the preview.
- **L4-S5** — operator review surface plus optional external-verifier
  handoff (§17). Advisory review only.
- **L4-S6** — test request/report integration (§16). Requests and reads;
  never runs the write chain.

Each slice opens as its own fresh-worktree PR with its own scope card or
implementation scope card, bounded by this card.

## 22. Non-Goals

A2-L4 is explicitly **not**:

- an autonomous coding agent that writes, approves, or applies on its
  own
- a replacement for the A2-L2b operator-gated chain
- a new write command, write flag, or approval-bypass affordance
- a model-driven CI write step
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a change to `a2-l2d-status.v1` or any A2-L2b schema/marker
- a promotion of any A2-L3 read-only adapter into a controller
- a requirement that an external verifier participate
- a SideStackAI infrastructure change

## 23. STOP Gates

Any future A2-L4 implementation lane must STOP — escalate — and not
proceed if any of the following is true:

1. A proposal would write, stage, or apply without an operator gesture.
2. Any artifact contains or templates an approval line,
   `approval-result.json`, or `apply-bundle.json`.
3. App inference would route to raw `localhost:11434`.
4. A model load, SGLang start, or ComfyUI job would occur without
   broker/current-holder/VRAM checks.
5. The lane would modify `claw plan run/approve/apply-bundle/apply` or
   `claw plan status` behavior, exit codes, schemas, markers, or JSON
   field shapes.
6. The lane would modify `a2-l2d-status.v1` or any A2-L2b schema/marker.
7. The lane would promote an A2-L3 read-only adapter into a write
   controller.
8. The lane would send secret material to an external verifier or read
   secrets into a proposal.
9. A STOP signal from `a2-l2d-status.v1` would be coerced, downgraded,
   debounced, or hidden by the loop.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 24. Definition Of Done

This **scope card** is done when:

- it defines the local-model advisory-proposer role and its hard limits
- it pins the broker `:11435` routing boundary and the `:11434`
  prohibition
- it pins the VRAM safety boundary
- it pins the inert patch-proposal contract
- it preserves the A2-L2b `preview → approve → apply` chain as the only
  write authority
- it defines the optional external-verifier role
- it enumerates implementation slices, none of which authorize direct
  writes
- it states plainly that it authorizes design only — no implementation,
  no model execution, no direct writes, no approval bypass, no raw
  `:11434` app inference

A2-L4 **implementation** is out of scope for this card and is done only
when each separately-authorized slice (§21) lands under its own review.

## 25. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **L4-S1 (local model coding-loop architecture docs,
> docs-only)**. L4-S1 expands the advisory-loop architecture into a
> concrete design without any code, model execution, or write path,
> bounded strictly by this card's §§6–23. Code-bearing slices (L4-S2
> onward) follow in their own per-slice PRs, each with its own scope
> card or implementation scope card, and none authorizing direct writes.

## 26. References

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative; the only write
  authority).
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference for the gated chain.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record (read-only contract A2-L4 reads).
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md) —
  A2-L3 read-only harness adapter usage guide.
- [`a2-l3-ide-adapter-usage.md`](./a2-l3-ide-adapter-usage.md) — A2-L3
  read-only VS Code Claw Status Panel usage guide.
- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 adapter boundary and its invariants.
- [`editor-vscode.md`](./editor-vscode.md) — read-only VS Code task
  wrapper; source of the LAW-1 `:11435`-only routing refusal A2-L4
  inherits.

## 27. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize implementation.
It does not authorize model execution.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The local model is advisory until a future implementation lane
explicitly wires a proposal through A2 under its own authorization.

Next gate: read-only operator review of this scope card.
```
