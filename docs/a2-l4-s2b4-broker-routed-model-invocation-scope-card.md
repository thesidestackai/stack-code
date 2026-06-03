# A2-L4-S2B-4 Scope Card — Broker-Routed Model Invocation (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2B-4 slice:
the **deferred model-call lane** of the planner CLI. It describes whether
and how a future lane would replace the S2B-3 *fixture* plan with a plan
**proposed by a local model** — and the hard guardrails that lane must
hold to. This file itself authorizes **no runtime change, no model
execution, no model load, no broker call, no GPU work, no SGLang start,
no ComfyUI job, no write-chain command, no approval/apply, no direct
write, no approval bypass, and no raw `localhost:11434` app inference**.

S2B-4 is the one capability the S2B sequence has deliberately deferred at
every prior step. S2B-1 scoped the planner CLI; S2B-2 built the read-only
workspace context-summary builder; S2B-3 wired an **offline** skeleton
that runs a *fixture* plan through validate + pretty-print with **no model
call**. S2B-4 is where a model would finally propose the plan — and it is
exactly the lane that needs the strongest gate, because it is the first
time inference enters the loop. This card pins that gate. It does **not**
open it.

### Must State

```text
This card authorizes design only.
It does not authorize the model-call lane's implementation.
It does not authorize model execution.
It does not authorize a model load.
It does not authorize a broker call.
It does not authorize GPU work, SGLang start, or ComfyUI jobs.
It does not authorize write-chain commands.
It does not authorize approval/apply.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2B-4 defines, in design only, the boundary the **future
broker-routed model-invocation lane** must hold to. That lane would take
the offline S2B-3 skeleton and swap its fixture plan for a plan a **local
model** proposes — but only by sending the planning request through the
**SideStack broker at `localhost:11435`**, never to a raw upstream port.
The model's response is treated exactly as the fixture was: **validated**
(S2A-5), **pretty-printed** (S2A-7), and presented to the operator as an
**advisory** proposal that authorizes nothing.

The recommended A2-L4-S2B-4 scope is:

> Pin the future model-call lane's role (the model proposes a plan, the
> operator reviews it, A2 previews/approves/applies), its `:11435`-only
> routing boundary with the raw `:11434` prohibition, the mandatory
> pre-inference gate (broker status, current VRAM holder, VRAM headroom,
> operator approval), the no-auto-load / no-SGLang-auto-start /
> no-ComfyUI boundary, the advisory-only output contract (validate +
> pretty-print, never write/approve/apply), and the
> broker-unreachable-is-a-clean-error rule — **without** calling a model,
> loading a model, or touching the broker, and while the A2-L2b
> preview/approve/apply chain remains the only write authority.

The implementation of the model-call lane is **not authorized by this
scope card**, and **no model call is created by it**. The next gate before
any implementation is operator review of this scope card.

## 2. Relationship To S2B-3 (The Offline Skeleton)

S2B-3 landed `scripts/run_offline_planner_cli.py`: an **offline** skeleton
that combines operator task text, a read-only workspace context summary,
and a **fixture** planner-output document through the existing validator
and pretty-printer. It calls no model and writes nothing.

S2B-4 is the **single, surgical** change of replacing that fixture input
with a model-proposed plan — and **nothing else**. Specifically, the
future lane:

- reuses the S2B-2 context builder (read-only) and the S2A
  validator/pretty-printer (import-only) **unchanged**;
- keeps every S2B-3 refusal and the OFFLINE-vs-LIVE distinction explicit
  (a live run must clearly announce it consulted a model);
- adds **one** new capability: a broker-routed planning request whose
  response is fed into the *same* validate → pretty-print path.

Everything S2B-3 forbids (writes, approvals, applies, executing plan
contents, touching the A2 state tree) S2B-4 forbids identically. The only
new surface is the inference call, and §§7–8 gate it.

## 3. Relationship To A2-L4 And The North Star

The A2-L4 North Star:

```text
local model proposes
operator reviews
A2 previews
operator approves
A2 applies
external ChatGPT/Claude may verify
```

S2B-4 is the **"local model proposes"** step made real — and **only**
that step. It ends at "operator reviews": the lane produces a validated,
pretty-printed proposal and stops. Everything downstream ("A2 previews /
operator approves / A2 applies") stays with the operator-gated A2-L2b
chain. The local model never writes, approves, applies, bypasses A2, or
calls a raw upstream port.

## 4. What The Model-Call Lane Is (Future)

A future S2B-4 lane would, in a fully-realized but still advisory form:

1. accept an operator **task description** and a **workspace root**
   (as S2B-3 already does).
2. gather the **read-only workspace context summary** (S2B-2).
3. **before any inference**, run the §8 pre-inference gate: confirm
   broker reachability, check the current VRAM holder and headroom, and
   obtain explicit operator approval to proceed.
4. send a **planning request to a local model through `localhost:11435`
   only**, with the task text and the read-only context as input, and
   receive a proposed planner-output document.
5. **validate** the proposal (S2A-5) and **pretty-print** it (S2A-7) for
   the operator, exactly as S2B-3 does with a fixture.
6. clearly mark the run as **LIVE** (a model was consulted) and the output
   as **advisory** — it authorizes nothing.

Steps 1–2, 5–6 are the already-landed read-only orchestration. Steps 3–4
are the new, gated capability this card scopes but does not build.

## 5. What The Model-Call Lane Must Not Do

The model-call lane must **never**:

- write, edit, stage, or delete any file; mutate the A2 state tree
- run `claw plan run/approve/apply-bundle/apply` or any write-chain
  command
- generate a preview, an `approval-result.json`, or an `apply-bundle.json`
- generate, template, or echo an approval line
- create a patch-proposal artifact (that is the separate A2-L4-S3 slice)
- execute anything the model returns (plan steps, test suggestions,
  preview requests are inert text — never run)
- call a model through **anything other than** `localhost:11435` — and
  never through raw `localhost:11434`
- **auto-load** a model, start SGLang, start ComfyUI, or run GPU work
  without the §8 broker-status + VRAM + operator gate
- proceed past a broker-unreachable, sealed-Vault, or no-VRAM-headroom
  condition (each is a clean refusal, never a hang or a silent fallback)
- let the model's output reach the workspace except as inert,
  operator-reviewed display

The model is a **proposer**, never an actor. A proposal that fails
validation is shown as REFUSED and is never acted on.

## 6. Input / Output Contract

**Input:**

- a task description (operator text);
- a workspace root (read-only);
- optional read-only hints — never flags that enable writing, approving,
  applying, or model autoaction;
- broker routing config read **only** from the approved
  `examples/sidestack-local.env` (`http://127.0.0.1:11435/v1`), never a
  raw upstream base URL.

**Output:**

- a planner-output document conforming to `a2-l4-planner-output.v1`,
  **validated** and **pretty-printed** to stdout;
- an explicit **LIVE** marker stating a model was consulted, alongside the
  S2B-3 statements that no file was written, no approval line was emitted,
  and no write-chain command was run;
- a clear statement that the output is **advisory and authorizes
  nothing**;
- a **nonzero exit** if the proposal fails validation (REFUSED), if input
  is unreadable, if the broker is unreachable/sealed, or if the
  pre-inference gate is not satisfied — **never** a silent empty success
  and never a fixture-substituted fallback that hides a failed call.

The output is never written to disk as an authority artifact and never
fed automatically into a write-chain.

## 7. LAW 1 Routing Boundary

```text
All app inference must route through localhost:11435.
Raw app inference through localhost:11434 is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation. A violation is a STOP.
```

The model-call lane must use the SideStack broker at
`http://127.0.0.1:11435/v1` (see `examples/sidestack-local.env`) and
**must refuse** a raw `:11434` base URL — a refusal, never a fallback.
Every `:11434` token in this card is a **docs/history** statement of the
prohibition, not an inference path; this card creates no inference path of
any kind.

## 8. Mandatory Pre-Inference Gate (Broker Status, Holder, VRAM, Operator)

Before any inference, the future lane must, in order:

```text
1. broker status check   — confirm the broker at :11435 is reachable and
                           healthy (assert on the response body, not just
                           an HTTP code or a fast timing).
2. current holder check  — determine which model/process currently holds
                           VRAM, so the request does not silently evict or
                           contend with a live workload.
3. VRAM headroom check   — confirm there is enough free VRAM for the
                           planning model without forcing a load that
                           starves another holder.
4. operator approval     — obtain explicit operator approval to proceed,
                           surfacing the holder + headroom facts.
```

```text
No casual model loads.
No automatic SGLang starts.
No ComfyUI jobs.
No heavy parallel inference.
```

If any of steps 1–3 fails, or step 4 is not granted, the lane **STOPS**
with a clean, explicit error — it does **not** load a model, evict a
holder, start a GPU backend, or fall back to a fixture to manufacture a
success. A sealed Vault or unreachable broker surfaces as a refusal, never
a hang. This card authorizes **none** of these checks to run; it fixes the
gate the future lane must implement and pass.

## 9. Read-Only / No-Approval Boundaries

- **Read-only:** the lane reads task text, workspace files, the schema,
  and a model response; it writes no file, creates no directory, and
  leaves the workspace unchanged. The A2 state tree is never touched.
- **No-approval:** the lane emits no approval line and no artifact a
  downstream tool could treat as an approval/apply. The operator-gated
  A2-L2b chain is the only path from proposal to action.
- **No-autoaction:** no flag, env var, or config makes the lane write,
  approve, apply, auto-load a model, or auto-run a model proposal.

## 10. Where S2B-4 Sits In The S2B Sequence

```text
S2B-1  docs-only scope card (the planner CLI boundary)            [landed]
S2B-2  read-only workspace context-summary builder (no model)     [landed]
S2B-3  offline planner-CLI skeleton (fixture plan; NO model call) [landed]
S2B-4  the model-call lane — sends a planning request through
       localhost:11435 ONLY; separately operator-gated, with
       broker status + holder + VRAM checks; refuses raw :11434   [THIS CARD: design only]
S2B-5  optional external-verifier handoff builder (advisory)      [deferred]
```

Only S2B-4 is in view of this card, and only as **design**. The
implementation of S2B-4 is a later, separately-reviewed lane with its own
operator gate, broker-status check, and VRAM check.

## 11. Allowed Future Touched Surfaces (Implementation Lane)

When the S2B-4 **implementation** lane is separately authorized, it should
touch **only** narrow surfaces, e.g.:

```text
scripts/run_offline_planner_cli.py  (extend with a gated LIVE path, or a
                                     sibling scripts/run_planner_cli.py)
tests/a2_l4/test_<that-tool>.py
docs/a2-l4-s2b4-*.md
README.md
```

importing the S2B-2 context builder and the S2A validator/pretty-printer
**read-only**, and reading broker config from `examples/sidestack-local.env`
**read-only**. Each surface is re-confirmed under that lane's own review.
CI wiring (a `.github` change) for any new behavior is a separate,
operator-approved lane. Live model-call tests must not perform real
inference in CI without an explicit operator-gated arrangement; the
default test path stays offline/mocked at the boundary.

## 12. Forbidden Future Touched Surfaces

The future S2B-4 implementation lane must **not** touch:

```text
scripts/validate_planner_output_schema.py (modify — import-only allowed)
scripts/pretty_print_planner_output.py (modify — import-only allowed)
scripts/build_workspace_context_summary.py (modify — import-only allowed)
schemas/a2-l4/** (modify)
the A2 state tree
rust/** (unless a separately-scoped, reviewed lane requires it)
ide/**
Cargo.toml / Cargo.lock
examples/** (modify — read-only reference to sidestack-local.env allowed)
SideStackAI/**
broker / Ollama / SGLang / ComfyUI runtime, systemd, or Docker config
```

Touching any of the above (other than read-only imports/reference) is
scope drift and a STOP (§14).

## 13. Validation Requirements

Before the S2B-4 implementation lane is accepted, its tests must
demonstrate:

```text
a LIVE run routes ONLY to localhost:11435 and refuses a raw :11434 base
  URL (refusal, never a fallback)
the pre-inference gate is enforced: broker-unreachable, sealed-Vault, no
  current-holder info, or insufficient VRAM each yields a clean nonzero
  refusal — never a hang, never a fixture-substituted false success
a model proposal is run through the SAME validator + pretty-printer as the
  fixture path; an invalid proposal is REFUSED, never coerced
no file is written; the A2 state tree is never touched; no approval line
  is emitted; no write-chain command is invoked
the run is clearly marked LIVE (a model was consulted) and the output is
  marked advisory (authorizes nothing)
real inference does not run in CI without an explicit operator-gated
  arrangement
```

Each positive case ends in an inert, operator-reviewed proposal; each
negative case is refused.

## 14. STOP Gates

Any future S2B-4 implementation lane must STOP — escalate — and not
proceed if any of the following is true:

1. The lane would write, edit, stage, or delete any file, or mutate the
   A2 state tree.
2. The lane would run a write-chain command (`claw plan
   run/approve/apply-bundle/apply`) or generate a preview/approval/apply
   artifact.
3. The lane would create a patch-proposal artifact (A2-L4-S3 territory).
4. The lane would call a model through anything other than
   `localhost:11435`, or through raw `localhost:11434`.
5. The lane would auto-load a model, start SGLang, or start ComfyUI
   without the §8 broker-status + holder + VRAM + operator gate.
6. The lane would proceed past a broker-unreachable, sealed-Vault, or
   no-VRAM-headroom condition instead of refusing cleanly.
7. The lane would substitute a fixture (or any canned plan) to manufacture
   a success when the model call fails.
8. The lane would modify the schema, validator, pretty-printer, or context
   builder (import-only is allowed).
9. The lane would expose a flag/env/config enabling write, approve, apply,
   or model autoaction.
10. The lane would add the model call **under this card** (S2B-4 design)
    rather than under the separately-gated S2B-4 implementation lane.
11. The lane would touch any §12 forbidden surface.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 15. Non-Goals

A2-L4-S2B-4 (this card) is explicitly **not**:

- a model-call lane implementation (that is the separately-gated S2B-4
  implementation lane)
- a model call, broker call, model load, or any inference
- a GPU/VRAM/SGLang/ComfyUI runtime change or a "test" model load
- a change to the schema, validator, pretty-printer, or context builder
- a preview, approval, or apply capability
- a patch-proposal artifact (that is A2-L4-S3)
- an autonomous coding agent that writes, approves, or applies
- a replacement for the A2-L2b operator-gated chain
- a broker, Ollama, SGLang, ComfyUI, systemd, or Docker change
- a raw `:11434` app-inference path
- a SideStackAI infrastructure change

## 16. Definition Of Done

This **scope card** is done when:

- it pins the future model-call lane's role (§§1, 3, 4) and what it must
  not do (§5)
- it pins the input/output contract, including the LIVE marker and the
  no-silent-success / no-fixture-fallback rule (§6)
- it pins the `:11435`-only routing boundary with the raw `:11434`
  prohibition (§7)
- it pins the mandatory pre-inference gate — broker status, current
  holder, VRAM headroom, operator approval — and the no-auto-load /
  no-SGLang / no-ComfyUI boundary (§8)
- it pins the read-only / no-approval / no-autoaction boundaries (§9)
- it places S2B-4 in the S2B sequence and keeps the model call deferred to
  the separately-gated implementation lane (§10)
- it pins the allowed (§11) and forbidden (§12) future surfaces and the
  validation requirements (§13)
- it states plainly that it authorizes design only — no model call, no
  model load, no broker call, no GPU work, no preview, no write-chain, no
  approval/apply, no patch artifact, no direct writes, no approval bypass,
  no raw `:11434` app inference

S2B-4 **implementation** is out of scope for this card and is done only
when the separately-authorized model-call lane lands under its own review.

## 17. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card.** Implementation of the
> model-call lane (S2B-4) remains deferred until its own operator gate,
> broker-status + current-holder + VRAM checks, and explicit operator
> approval. No model is loaded or called to "try out" the lane before that
> gate.

## 18. References

- [`a2-l4-s2b-readonly-planner-cli-scope-card.md`](./a2-l4-s2b-readonly-planner-cli-scope-card.md)
  — A2-L4-S2B parent card defining the planner CLI and the S2B sub-slice
  sequence (S2B-4 is the deferred model-call lane named there).
- [`a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./a2-l4-s2-readonly-local-model-task-planner-scope-card.md)
  — A2-L4-S2 parent task-planner slice.
- [`a2-l4-s2a-planner-output-operator-guide.md`](./a2-l4-s2a-planner-output-operator-guide.md)
  — operator guide for the schema/validator/pretty-printer the lane uses.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary).
- [`../scripts/run_offline_planner_cli.py`](../scripts/run_offline_planner_cli.py)
  — the landed S2B-3 offline skeleton this lane would extend.
- [`../scripts/build_workspace_context_summary.py`](../scripts/build_workspace_context_summary.py)
  — the S2B-2 read-only context builder (import-only).
- [`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py)
  — the validator (S2A-5; import-only).
- [`../scripts/pretty_print_planner_output.py`](../scripts/pretty_print_planner_output.py)
  — the pretty-printer (S2A-7; import-only).
- [`../examples/sidestack-local.env`](../examples/sidestack-local.env)
  — the approved `:11435` broker routing config (read-only reference).
- [`editor-vscode.md`](./editor-vscode.md) — source of the LAW-1
  `:11435`-only routing refusal this card inherits.

## 19. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR IMPLEMENTATION

This card authorizes design only.
It does not authorize the model-call lane's implementation.
It does not authorize model execution, a model load, or a broker call.
It does not authorize GPU work, SGLang starts, or ComfyUI jobs.
It does not authorize preview generation, write-chain commands, or
  approval/apply.
It does not authorize patch-proposal artifacts.
It does not authorize direct writes or approval bypass.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The local model proposes only; it never writes, approves, applies,
bypasses A2, or calls a raw upstream port. The model-call lane runs only
behind the operator gate, broker-status check, current-holder check, and
VRAM check defined in §8.

Next gate: read-only operator review of this scope card.
```
