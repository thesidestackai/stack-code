# A2-L4-S2B-5 Scope Card — Live Broker Smoke (Docs-Only)

This document is a **design-only scope card** for the A2-L4-S2B-5 slice:
the **first, operator-gated live broker smoke** of the planner invocation
adapter. It describes whether and how a future lane would exercise the
landed S2B-4 adapter (`scripts/invoke_planner_model_via_broker.py`)
against the **real** SideStack broker — once, with a small prompt, behind
an explicit operator gate — and the hard guardrails that smoke must hold
to. This file itself authorizes **no model execution, no model load, no
model switch, no broker call, no GPU work, no SGLang start, no ComfyUI
job, no write-chain command, no approval/apply, no direct write, no
approval bypass, and no raw `localhost:11434` app inference**.

The S2B-4 adapter is **dry-run safe by default**: with no flag it builds
and prints the request payload and makes no call; a live call happens only
behind ``--allow-live-broker-call``. S2B-5 is the lane that would, for the
first time, pass that flag against the real broker — and it is precisely
the moment that needs the strongest gate, because it is the first time
inference actually runs. This card pins that gate. It does **not** open
it, and it runs **no** smoke.

### Must State

```text
This card authorizes design only.
It does not authorize the live broker smoke's execution.
It does not authorize model execution.
It does not authorize a model load or a model switch.
It does not authorize a broker call.
It does not authorize GPU work, SGLang start, or ComfyUI jobs.
It does not authorize write-chain commands.
It does not authorize approval/apply.
It does not authorize direct writes.
It does not authorize approval bypass.
It does not authorize raw localhost:11434 app inference.
```

## 1. Executive Summary

A2-L4-S2B-5 defines, in design only, the boundary the **future live broker
smoke** must hold to. That smoke would invoke the landed S2B-4 adapter
with ``--allow-live-broker-call`` against the broker at
``http://127.0.0.1:11435`` — **once**, with a **small** task and a
read-only workspace context summary — to confirm end to end that a local
model can return a candidate planner-output document over the approved
route. The candidate is **advisory only**: it is printed for the operator,
optionally run through the existing read-only validator and pretty-printer,
and **never** validated-into-authority, approved, or applied.

The recommended A2-L4-S2B-5 scope is:

> Pin the future live smoke's preconditions (broker health/status, current
> VRAM holder, VRAM headroom, explicit operator model approval), its
> `:11435`-only route with the raw `:11434` prohibition, its no-model-load
> / no-model-switch / no-SGLang / no-ComfyUI boundary, its shape (one
> small prompt, a bounded timeout, advisory-only output, no file writes,
> no A2 apply/approve), and its clean-failure rule — **without** running a
> model, loading or switching a model, or touching the broker, and while
> the A2-L2b preview/approve/apply chain remains the only write authority.

The execution of the live smoke is **not authorized by this scope card**,
and **no smoke is run by it**. The next gate before any live smoke is
operator review of this scope card plus the §3 preconditions.

## 2. Relationship To S2B-4 (The Adapter)

S2B-4 landed `scripts/invoke_planner_model_via_broker.py`: a broker-routed
planner invocation adapter that is dry-run safe by default and refuses an
unsafe broker URL (raw upstream port, or non-loopback host) and an unsafe
context (A2 state-tree path, or credential-like value). Its tests exercise
only a fake in-process loopback server; it has never made a live call.

S2B-5 is the lane that would run that adapter **live, once** — changing
nothing about the adapter, only invoking it with the live flag after the
§3 gate passes. Everything the adapter already refuses, the smoke inherits.
The smoke adds **no** new capability to the adapter; it only exercises the
already-built, already-gated live path under operator supervision.

## 3. Mandatory Preconditions (The Gate)

Before the live smoke runs, the operator and the lane must, in order,
confirm:

```text
1. broker health/status  — the broker at :11435 is reachable and healthy
                           (assert on the response body, not just an HTTP
                           code or a fast timing).
2. current holder        — which model/process currently holds VRAM, so the
                           smoke neither evicts nor contends with a live
                           workload.
3. VRAM headroom         — enough free VRAM exists for the planner model
                           WITHOUT forcing a load that starves the current
                           holder; if not, the smoke does not run.
4. operator model approval — the operator explicitly approves the specific
                           model the smoke will use, having seen the holder
                           and headroom facts.
```

If any of 1–3 cannot be determined, or 4 is not granted, the smoke
**does not run**. Determining the holder/headroom is itself read-only; a
sealed Vault or unreachable broker surfaces as a clean refusal, never a
hang. This card authorizes none of these checks to execute; it fixes the
gate the future smoke must pass.

## 4. Model-Load / Model-Switch Boundary

```text
No model load to make the smoke fit.
No model switch / eviction of the current holder.
No automatic SGLang start.
No ComfyUI jobs.
No heavy or parallel inference.
```

The smoke runs **only** if a suitable model is already servable within the
available VRAM headroom under the operator-approved choice. The smoke must
**never** load a model, switch models, or evict the current holder to make
room. If the approved model is not already available without a load/switch,
that is an operator decision in a separate, explicitly-gated step — not
something the smoke does on its own.

## 5. LAW 1 Routing Boundary

```text
All app inference must route through localhost:11435.
Raw app inference through localhost:11434 is prohibited.
Any :11434 reference must be classified as management, docs/history,
false positive, or violation. A violation is a STOP.
```

The smoke must use the broker at ``http://127.0.0.1:11435`` (the adapter's
default; see `examples/sidestack-local.env`) and the adapter already
**refuses** a raw `:11434` base URL or a non-loopback host. Every `:11434`
token in this card is a **docs/history** statement of the prohibition, not
a route; this card creates no inference path of any kind.

## 6. Smoke Shape

The future live smoke would be:

```text
input:    one SMALL operator task (a few words) + a read-only workspace
          context summary produced by the S2B-2 builder
command:  scripts/invoke_planner_model_via_broker.py
            --task "<small task>"
            --context-summary <path>
            --broker-url http://127.0.0.1:11435
            --allow-live-broker-call
            --timeout <bounded, small>
route:    :11435 only (raw :11434 refused by the adapter)
calls:    exactly ONE broker request; no retries storms, no loops
output:   the candidate planner-output response, printed; optionally piped
          through the existing read-only validator + pretty-printer for
          operator review
```

The smoke is a **single** small request with a **bounded** timeout. It is
not a benchmark, not a load test, and not a loop.

## 7. Output / Authority Boundary

- The model's response is a **candidate proposal**. It authorizes nothing.
- It is **printed** for the operator; it may be **validated** and
  **pretty-printed** read-only; it is **never** written to disk as an
  authority artifact, **never** approved, **never** applied.
- The smoke emits **no** approval line and **no** artifact a downstream
  tool could treat as an approval/apply.
- A failed/unreachable/sealed call is a **clean nonzero refusal**, never a
  hang and never a fabricated success.

The A2-L2b preview/approve/apply chain remains the only path from a
proposal to an actual change.

## 8. Read-Only / No-Approval Boundaries

- **Read-only:** the smoke reads the task, the context summary, and the
  model response; it writes no file, creates no directory, and leaves the
  workspace unchanged. The A2 state tree is never touched.
- **No-approval:** the smoke emits no approval line and no apply artifact.
- **No-autoaction:** no flag, env var, or config makes the smoke write,
  approve, apply, load a model, switch models, or loop.

## 9. Where S2B-5 Sits In The S2B Sequence

```text
S2B-1  docs-only scope card (the planner CLI boundary)            [landed]
S2B-2  read-only workspace context-summary builder (no model)     [landed]
S2B-3  offline planner-CLI skeleton (fixture plan; NO model call) [landed]
S2B-4  broker-routed planner invocation adapter (dry-run safe;
       live only behind --allow-live-broker-call)                 [landed]
S2B-5  the first operator-gated LIVE broker smoke of the adapter  [THIS CARD: design only]
```

Only S2B-5 is in view of this card, and only as **design**. The execution
of the live smoke is a later, separately-gated step that runs only after
this card is reviewed and the §3 preconditions pass.

## 10. Allowed Future Touched Surfaces (Smoke Lane)

A future live-smoke lane, when separately authorized, should touch **only**
narrow surfaces, e.g.:

```text
docs/a2-l4-s2b5-*.md            (a smoke runbook / results note)
README.md                       (index line)
```

reading the S2B-2 builder's context-summary output and invoking the landed
S2B-4 adapter **as-is** (no source change), with broker config read from
`examples/sidestack-local.env` **read-only**. A smoke that needs to change
the adapter is **not** this lane — it is a separate, reviewed adapter
change. CI must not run a live smoke; any live inference happens only in an
operator-supervised manual run, never in CI.

## 11. Forbidden Future Touched Surfaces

The future S2B-5 smoke lane must **not** touch:

```text
scripts/invoke_planner_model_via_broker.py (modify — invoke as-is only)
scripts/build_workspace_context_summary.py (modify — invoke/read as-is)
scripts/validate_planner_output_schema.py (modify — import/invoke read-only)
scripts/pretty_print_planner_output.py (modify — import/invoke read-only)
schemas/a2-l4/** (modify)
the A2 state tree
rust/** / ide/** / Cargo.toml / Cargo.lock
.github/** (no CI live-inference job)
examples/** (modify — read-only reference to sidestack-local.env allowed)
SideStackAI/**
broker / Ollama / SGLang / ComfyUI runtime, systemd, or Docker config
```

Touching any of the above (other than read-only reference / as-is
invocation) is scope drift and a STOP (§13).

## 12. Validation Requirements

Before the live smoke is accepted as having passed, the operator-supervised
run must demonstrate:

```text
the §3 gate was satisfied (broker healthy, holder known, headroom
  sufficient, operator approved the model) BEFORE the call
the route was :11435 only; a raw :11434 base URL would have been refused
exactly ONE broker request was made, within a bounded timeout
no model was loaded and no model was switched/evicted to make room
the candidate response was printed (and optionally validated/pretty-printed
  read-only); nothing was written, approved, or applied
a failure (unreachable/sealed/timeout) would have surfaced as a clean
  nonzero refusal, not a hang or a fabricated success
```

No live inference runs in CI; the smoke is a manual, operator-supervised
step only.

## 13. STOP Gates

Any future S2B-5 smoke lane must STOP — escalate — and not proceed if any
of the following is true:

1. The §3 gate is not fully satisfied (broker unhealthy, holder unknown,
   headroom insufficient, or operator model approval not granted).
2. The smoke would load a model, switch models, or evict the current holder
   to make room.
3. The smoke would start SGLang or ComfyUI, or run heavy/parallel/looped
   inference.
4. The smoke would route to anything other than `localhost:11435`, or to
   raw `localhost:11434`.
5. The smoke would write a file, mutate the A2 state tree, emit an approval
   line, or run an A2 write-chain command.
6. The smoke would run in CI, or as anything other than a single, small,
   operator-supervised manual request.
7. The smoke would modify the adapter or any sibling script (invoke-as-is
   only).
8. The smoke would treat the model's output as authority (validate-into-
   apply, approve, or apply).
9. The smoke would touch any §11 forbidden surface.

A STOP is an escalation to the operator, never a prompt to retry the
forbidden action with different framing.

## 14. Non-Goals

A2-L4-S2B-5 (this card) is explicitly **not**:

- a live smoke execution (that is the separately-gated smoke lane)
- a model run, model load, model switch, broker call, or any inference
- a GPU/VRAM/SGLang/ComfyUI runtime change or a "test" model load
- a change to the adapter or any sibling script
- a CI live-inference job
- a preview, approval, or apply capability
- a benchmark, load test, or inference loop
- a replacement for the A2-L2b operator-gated chain
- a broker, Ollama, SGLang, ComfyUI, systemd, or Docker change
- a raw `:11434` app-inference path
- a SideStackAI infrastructure change

## 15. Definition Of Done

This **scope card** is done when:

- it pins the future live smoke's role (§§1, 6) and its authority boundary
  (§7)
- it pins the mandatory preconditions gate — broker health/status, current
  holder, VRAM headroom, operator model approval (§3)
- it pins the no-model-load / no-model-switch / no-SGLang / no-ComfyUI
  boundary (§4)
- it pins the `:11435`-only route with the raw `:11434` prohibition (§5)
- it pins the smoke shape: one small prompt, bounded timeout, single call,
  advisory-only output, no writes, no A2 apply/approve (§§6–8)
- it places S2B-5 in the S2B sequence (§9) and keeps the smoke deferred to
  the separately-gated, operator-supervised run
- it pins the allowed (§10) and forbidden (§11) surfaces and the validation
  requirements (§12)
- it states plainly that it authorizes design only — no smoke, no model
  run, no model load/switch, no broker call, no GPU work, no preview, no
  write-chain, no approval/apply, no direct writes, no approval bypass, no
  raw `:11434` app inference

S2B-5 **execution** is out of scope for this card and is done only when the
separately-authorized, operator-supervised live smoke runs under its own
review and the §3 gate.

## 16. Next Lane Recommendation

The recommended next lane is:

> **Read-only PR review of this scope card.** Execution of the live broker
> smoke (S2B-5) remains deferred until its own operator gate: broker
> health/status, current-holder and VRAM-headroom checks, and explicit
> operator model approval. No model is loaded, switched, or called to "try
> out" the smoke before that gate.

## 17. References

- [`a2-l4-s2b4-broker-routed-model-invocation-scope-card.md`](./a2-l4-s2b4-broker-routed-model-invocation-scope-card.md)
  — A2-L4-S2B-4 scope card for the model-call lane the adapter implements.
- [`a2-l4-s2b-readonly-planner-cli-scope-card.md`](./a2-l4-s2b-readonly-planner-cli-scope-card.md)
  — A2-L4-S2B parent card and the S2B sub-slice sequence.
- [`a2-l4-local-model-coding-loop-scope-card.md`](./a2-l4-local-model-coding-loop-scope-card.md)
  — A2-L4 parent scope card (advisory-loop boundary, VRAM boundary).
- [`../scripts/invoke_planner_model_via_broker.py`](../scripts/invoke_planner_model_via_broker.py)
  — the landed S2B-4 adapter the smoke would invoke as-is.
- [`../scripts/build_workspace_context_summary.py`](../scripts/build_workspace_context_summary.py)
  — the S2B-2 read-only context builder (read-only).
- [`../scripts/validate_planner_output_schema.py`](../scripts/validate_planner_output_schema.py)
  — the validator (read-only, optional for reviewing the candidate).
- [`../scripts/pretty_print_planner_output.py`](../scripts/pretty_print_planner_output.py)
  — the pretty-printer (read-only, optional for reviewing the candidate).
- [`../examples/sidestack-local.env`](../examples/sidestack-local.env)
  — the approved `:11435` broker routing config (read-only reference).
- [`editor-vscode.md`](./editor-vscode.md) — source of the LAW-1
  `:11435`-only routing refusal this card inherits.

## 18. Status

```text
status: DESIGN-ONLY SCOPE CARD — NOT AUTHORIZED FOR EXECUTION

This card authorizes design only.
It does not authorize the live broker smoke's execution.
It does not authorize model execution, a model load, or a model switch.
It does not authorize a broker call, GPU work, SGLang starts, or ComfyUI jobs.
It does not authorize preview generation, write-chain commands, or
  approval/apply.
It does not authorize direct writes or approval bypass.
It does not authorize raw localhost:11434 app inference.

The A2-L2b preview/approve/apply chain remains the only write authority.
The local model proposes only; it never writes, approves, applies,
bypasses A2, or calls a raw upstream port. The live smoke runs only behind
the operator gate, broker health/status check, current-holder check, VRAM
headroom check, and explicit operator model approval defined in §3 — and
never loads or switches a model to make room (§4).

Next gate: read-only operator review of this scope card.
```
