# A2-L4-S2B Scope Card — Read-Only Planner CLI (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2B slice.
It describes whether and how a future **read-only planner CLI** should be
built: what it would orchestrate (operator task text, a workspace context
summary, and — only in a later, separately-authorized lane — a local
model planning request routed **only** through `localhost:11435`), how it
would consume the landed S2A stack (schema, validator, pretty-printer),
and the boundaries it must hold. This file itself authorizes **no
runtime change, no CLI, no model execution, no model load, no broker
call, no preview generation, no write-chain command, no approval/apply,
no patch-proposal artifact, no direct write, no approval bypass, and no
raw `localhost:11434` app inference**.

A2-L4-S2B is the sibling slice to A2-L4-S2A under the A2-L4-S2 read-only
task-planner card. S2A built the **planner-output contract layer**
(schema, fixtures, validator, pretty-printer); S2B scopes the **CLI that
would eventually produce** such output. This card carries exactly one
step: it scopes the future planner CLI and its sub-slices. It does **not**
create the CLI, and — critically — it does **not** authorize the model
call. The model-calling lane is deferred to a later, separately-reviewed
sub-slice with its own operator gate.

### Must State

```text
This card authorizes design only.
It does not authorize planner CLI implementation.
It does not authorize model execution.
It does not authorize broker calls.
It does not authorize preview generation.
It does not authorize write-chain commands.
It does not authorize approval/apply.
It does not authorize patch-proposal artifact creation.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2B defines, in design only, the boundary a **future read-only
planner CLI** must hold to. The CLI would be the operator-facing front
door that takes a task description, gathers read-only workspace context,
and (eventually) asks a local model — **only** through the SideStack
broker at `localhost:11435` — to propose a plan. The model's proposal is
emitted as a planner-output document, **validated** (S2A-5) and
**pretty-printed** (S2A-7) for the operator, and optionally packaged as an
advisory **external-verifier handoff** (ChatGPT/Claude). The CLI never
writes, never approves, never applies, never previews, and never lets the
model touch the workspace.

The recommended A2-L4-S2B scope is:

> Pin the future planner CLI's role (advisory, read-only proposer-front),
> its input contract (task text, workspace root, optional read-only
> hints), its output contract (a validated, pretty-printed planner
> output that authorizes nothing), the `:11435`-only routing boundary
> with the raw `:11434` prohibition, the VRAM boundary, the
> no-write/no-approval/no-apply/no-preview boundaries, the deferral of
> the model call to a later separately-gated sub-slice, and the S2B
> sub-slice sequence — without creating the CLI, without calling a model,
> and while the A2-L2b chain remains the only write authority.

The implementation of the planner CLI is **not authorized by this scope
card**, and **no CLI and no model call are created by it**. The next gate
before any implementation is operator review of this scope card.

## 2. Relationship To The S2A Stack

S2B is a **consumer** of the landed S2A artifacts; it changes none of
them:

- **Schema** (`schemas/a2-l4/planner-output.schema.json`, S2A-2) — the
  shape the CLI's output must conform to.
- **Validator** (`scripts/validate_planner_output_schema.py`, S2A-5) —
  the CLI validates every proposed plan before showing it; a refusal is
  surfaced, never coerced.
- **Pretty-printer** (`scripts/pretty_print_planner_output.py`, S2A-7) —
  the CLI renders the validated plan for operator review.
- **Operator guide** (`a2-l4-s2a-planner-output-operator-guide.md`) — the
  usage contract the CLI's output honors.

The CLI may **import/invoke** these read-only tools but must **not modify**
them. A plan that fails validation is shown as REFUSED and is never acted
on.

## 3. Relationship To A2-L4-S2 And The North Star

The A2-L4 North Star:

```text
local model proposes
operator reviews
A2 previews
operator approves
A2 applies
external ChatGPT/Claude may verify
```

S2B is the **"local model proposes"** front door — and **only** that. It
ends at "operator reviews": the CLI produces a validated, pretty-printed
proposal and stops. Everything downstream ("A2 previews / operator
approves / A2 applies") stays with the operator-gated A2-L2b chain. The
optional external verifier is advisory and grants no authority. The local
model never writes, approves, applies, bypasses A2, or calls raw Ollama.

## 4. What The Planner CLI Is (Future)

A future planner CLI would, in a fully-realized but still read-only form:

1. accept an operator **task description** and a **workspace root**.
2. gather a **read-only workspace context summary** (reading files /
   structure only — no mutation).
3. (in a later, separately-gated sub-slice) send a **planning request to
   a local model through `localhost:11435`** and receive a proposed
   planner-output document.
4. **validate** the proposal (S2A-5) and **pretty-print** it (S2A-7) for
   the operator.
5. optionally emit an advisory, secret-free **external-verifier handoff**.

Steps 1–2, 4–5 are pure read-only orchestration. Step 3 (the model call)
is the one capability this card explicitly **defers** to a later sub-slice
with its own operator gate, VRAM check, and broker-status check (§7, §8).

## 5. What It Must Not Do

The planner CLI must **never**:

- write, edit, stage, or delete any file; mutate `.claw/**`
- run `claw plan run/approve/apply-bundle/apply` or any write-chain
  command
- generate a preview, an `approval-result.json`, or an `apply-bundle.json`
- generate, template, or echo an approval line
- create a patch-proposal artifact (that is the separate A2-L4-S3 slice)
- execute anything contained in a plan (plan steps, test suggestions,
  preview requests are inert text)
- call a model, broker, or Ollama through **anything other than**
  `localhost:11435` — and never through raw `localhost:11434`
- load a model, start SGLang, start ComfyUI, or run GPU work without an
  explicit future operator-gated lane (§8)
- let the model's output reach the workspace except as inert,
  operator-reviewed display

A planner CLI is a **proposer and presenter**, never an actor.

## 6. Input / Output Contract

**Input:**

- a task description (operator text);
- a workspace root (read-only);
- optional read-only hints (e.g. files to focus on) — never flags that
  enable writing, approving, applying, or model autoaction.

**Output:**

- a planner-output document conforming to `a2-l4-planner-output.v1`,
  **validated** and **pretty-printed** to stdout;
- a clear statement that the output is **advisory and authorizes
  nothing** (per the S2A operator guide);
- a nonzero exit if the proposal fails validation (REFUSED), if input is
  unreadable, or — once the model lane exists — if the broker is
  unreachable; never a silent empty success.

The output is never written to disk as an authority artifact, and never
fed automatically into a write-chain.

## 7. LAW 1 Routing Boundary

```text
All app inference must route through localhost:11435.
Raw app inference through localhost:11434 is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation. A violation is a STOP.
```

When the model-call sub-slice eventually lands, it must use the SideStack
broker at `http://127.0.0.1:11435/v1` (see `examples/sidestack-local.env`)
and **must refuse** a raw `:11434` base URL — a refusal, never a fallback.
This card creates no inference path of any kind; it only fixes the rule
the future model-call lane must obey.

## 8. VRAM / Model Boundary

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
Any GPU/model lane needs broker status, current holder, VRAM check,
and operator approval.
```

This card does **not** authorize any GPU/model runtime. The future
model-call sub-slice must, before any inference: confirm broker
reachability, check the current VRAM holder and available headroom, and
defer to the operator on contention. No planner-CLI lane loads a model to
"test" the CLI without that gate.

## 9. Read-Only / No-Approval Boundaries

- **Read-only:** the CLI reads task text, workspace files, the schema,
  and (later) a model response; it writes no file, creates no directory,
  and leaves the workspace unchanged. `.claw/**` is never touched.
- **No-approval:** the CLI emits no approval line and no artifact a
  downstream tool could treat as an approval/apply. The operator-gated
  A2-L2b chain is the only path from proposal to action.
- **No-autoaction:** no flag, env var, or config makes the CLI write,
  approve, apply, or auto-run a model proposal.

## 10. S2B Sub-Slice Sequence (Design)

S2B should be built as small, separately-reviewed sub-slices, each with
its own scope card or implementation scope card:

```text
S2B-1  this docs-only scope card (the planner CLI boundary)
S2B-2  read-only workspace context-summary builder (no model call),
       offline, with fixtures/tests — pure read of files/structure
S2B-3  offline planner-CLI skeleton that wires task text + context +
       a STUBBED/fixture plan through validate + pretty-print
       (NO model call; deterministic fixture input)
S2B-4  the model-call lane — sends a planning request through
       localhost:11435 ONLY; separately operator-gated, with broker
       status + VRAM checks; refuses raw :11434
S2B-5  optional external-verifier handoff builder (advisory, secret-free)
```

Only S2B-1 is in view of this card, and only as design. S2B-2 and S2B-3
are read-only/offline and contain **no** model call. S2B-4 (the model
call) is the deferred, separately-gated capability and is **not**
authorized here. The exact sub-slice boundaries are confirmed or amended
under each lane's own review.

## 11. Allowed Future Touched Surfaces (Per Sub-Slice)

The future S2B implementation sub-slices, when separately authorized,
should touch **only** narrow, per-lane surfaces, e.g.:

```text
scripts/<planner-cli-or-context-builder>.py
tests/a2_l4/test_<that-tool>.py
docs/a2-l4-s2b-*.md
README.md
```

importing the S2A validator/pretty-printer **read-only**. Each sub-slice
re-confirms its exact allow-list under its own review. CI wiring (a
`.github` change) for any new tool is a separate, operator-approved lane,
as it was for the validator.

## 12. Forbidden Future Touched Surfaces

The future S2B sub-slices must **not** touch:

```text
scripts/validate_planner_output_schema.py (modify — import-only allowed)
scripts/pretty_print_planner_output.py (modify — import-only allowed)
schemas/a2-l4/** (modify)
.claw/**
rust/** (unless a separately-scoped, reviewed lane requires it)
ide/**
Cargo.toml / Cargo.lock
examples/** (runtime/broker config) except read-only reference
SideStackAI/**
runtime configs / systemd / Docker
```

Touching any of the above (other than read-only imports/reference) is
scope drift and a STOP (§14).

## 13. Validation Requirements

Before any S2B implementation sub-slice is accepted, its tests must
demonstrate (per the sub-slice's capability):

```text
context builder reads only; writes nothing; leaves workspace unchanged
offline skeleton produces output that PASSES the validator (or REFUSES
  cleanly on a bad fixture), with NO model call
no .claw mutation; no approval line; no write-chain invocation
(model-call lane, when gated) routes ONLY to localhost:11435 and refuses
  raw :11434; surfaces broker-unreachable as a clean error, not a hang
```

Each positive case is read-only and inert; each negative case is refused.
Model-call tests must not perform real inference in CI without an
explicit operator-gated arrangement.

## 14. STOP Gates

Any future A2-L4-S2B implementation lane must STOP — escalate — and not
proceed if any of the following is true:

1. The lane would write, edit, stage, or delete any file, or mutate
   `.claw/**`.
2. The lane would run a write-chain command (`claw plan
   run/approve/apply-bundle/apply`) or generate a preview/approval/apply
   artifact.
3. The lane would create a patch-proposal artifact (A2-L4-S3 territory).
4. The lane would call a model/broker/Ollama through anything other than
   `localhost:11435`, or through raw `localhost:11434`.
5. The lane would load a model, start SGLang, or start ComfyUI without
   the §8 broker-status + VRAM + operator gate.
6. The lane would add a model call **under this card** (S2B-1) rather than
   under the separately-gated S2B-4 lane.
7. The lane would modify the schema, validator, or pretty-printer
   (import-only is allowed).
8. The lane would expose a flag/env/config enabling write, approve,
   apply, or model autoaction.
9. The lane would touch any §12 forbidden surface.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 15. Non-Goals

A2-L4-S2B is explicitly **not**:

- a planner CLI implementation (those are S2B-2…S2B-5, each gated)
- a model call, broker call, or any inference (the model call is the
  deferred S2B-4 lane)
- a change to the schema, validator, or pretty-printer
- a preview, approval, or apply capability
- a patch-proposal artifact (that is A2-L4-S3)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a broker, model, SGLang, ComfyUI, or Ollama runtime change
- a raw `:11434` app-inference path
- a SideStackAI infrastructure change

## 16. Definition Of Done

This **scope card** is done when:

- it pins the future planner CLI's role (§§1, 3, 4) and what it must not
  do (§5)
- it pins the input/output contract (§6)
- it pins the `:11435`-only routing boundary with the raw `:11434`
  prohibition (§7) and the VRAM/model boundary (§8)
- it pins the read-only / no-approval / no-autoaction boundaries (§9)
- it lays out the S2B sub-slice sequence (§10) and **defers the model
  call** to the separately-gated S2B-4 lane
- it pins the allowed (§11) and forbidden (§12) future surfaces and the
  validation requirements (§13)
- it states plainly that it authorizes design only — no CLI, no model
  execution, no broker call, no preview, no write-chain, no
  approval/apply, no patch artifact, no direct writes, no approval
  bypass, no raw `:11434` app inference

A2-L4-S2B **implementation** is out of scope for this card and is done
only when the separately-authorized S2B sub-slice lanes land under their
own review.

## 17. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card**, then — only after operator
> approval — **A2-L4-S2B-2 (read-only workspace context-summary
> builder)**, the safest first implementation step: it reads workspace
> files/structure and produces an inert context summary with fixtures and
> tests, performs **no** model call, writes nothing, and imports nothing
> from the write-chain. The model-call lane (S2B-4) remains deferred until
> its own operator gate, broker-status check, and VRAM check.

## 18. References

- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 parent task-planner slice (S2A/S2B siblings).
- [`a2-l4-s2a-planner-output-contract-scope-card.md`](./a2-l4-s2a-planner-output-contract-scope-card.md)
  — A2-L4-S2A contract (the output the CLI produces).
- [`a2-l4-s2a-planner-output-operator-guide.md`](./a2-l4-s2a-planner-output-operator-guide.md)
  — operator guide for the schema/validator/pretty-printer the CLI uses.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`../schemas/a2-l4/planner-output.schema.json`](../schemas/a2-l4/planner-output.schema.json)
  — the schema (S2A-2).
- [`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py)
  — the validator (S2A-5; import-only for the CLI).
- [`../scripts/pretty_print_planner_output.py`](../scripts/pretty_print_planner_output.py)
  — the pretty-printer (S2A-7; import-only for the CLI).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — the A2-L2b operator chain (the only write authority).
- [`editor-vscode.md`](./editor-vscode.md) — source of the LAW-1
  `:11435`-only routing refusal this card inherits.

## 19. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize planner CLI implementation.
It does not authorize model execution, broker calls, or inference.
It does not authorize preview generation, write-chain commands, or
  approval/apply.
It does not authorize patch-proposal artifacts.
It does not authorize direct writes or approval bypass.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The local model proposes only; it never writes, approves, applies,
bypasses A2, or calls raw Ollama. The model-call lane (S2B-4) is deferred
until its own operator gate, broker-status check, and VRAM check.

Next gate: read-only operator review of this scope card.
```
