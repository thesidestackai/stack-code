# A2-L3 Scope Card — Harness Adapter (Docs-Only)

This document is a **design-only scope card** for the first concrete
A2-L3 adapter surface: a future **harness adapter** that consumes the
A2-L2d `a2-l2d-status.v1` envelope through the already-shipped
read-only command:

```text
claw plan status <workspace> [<approval-result.json>]
```

This file itself authorizes **no harness implementation, no IDE
implementation, no Rust source change, no CLI surface change, no
runtime change, no broker / model / Ollama traffic, no approve / apply
/ apply-bundle execution, no approve+apply composition, no write
controls, no autonomous workspace-write execution, and no weakening of
any A2-L2b, A2-L2c, A2-L2d, or A2-L3 STOP gate.** It defines the
bounded behavior a future harness-adapter implementation lane must
hold to.

This card chooses **harness adapter first** because it is the lower-
risk concrete A2-L3 surface: a harness adapter is a machine-facing
read-only observer and assertion layer, with no operator-clickable
buttons and no UI surface that could be refactored into a write
affordance. The IDE adapter remains deferred to a separate scope card.

A2-L3 sits exactly one layer above A2-L2d:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, shipped)
      → IDE / harness adapter boundary (A2-L3 boundary card, shipped)
        → harness adapter scope card (A2-L3, this card, design-only)
          → future harness adapter implementation (separate, future)
            → future IDE adapter scope card + implementation (separate, future)
```

## 1. Executive Summary

A2-L3 defines, in design only, the bounded behavior of any future
**harness adapter** that consumes `a2-l2d-status.v1`. The harness
adapter is a **read-only observer and assertion / reporting layer**,
never a workflow controller. It may invoke
`claw plan status <workspace> [<approval-result.json>]` as a read-only
subprocess, parse the JSON envelope, assert on every closed enum
value and on every read-only invariant the envelope carries, and emit
its full-fidelity contents as structured logs or metrics. It MUST NOT
call `claw plan approve`, `claw plan apply-bundle`, or
`claw plan apply`; MUST NOT compose approve and apply into a single
harness action; MUST NOT mutate any file under `.claw/`, the workspace
tree, the operator's home directory, the harness artifact directory,
or anywhere else; MUST NOT call broker, model, Ollama, or any other
network endpoint; MUST NOT mask, debounce, summarize, or rate-limit
any STOP signal; and MUST NOT run against a non-disposable repository.

The recommended A2-L3 harness adapter scope is:

> Define, in docs only, the bounded behavior of any future harness
> adapter that consumes `a2-l2d-status.v1`. The harness adapter is a
> read-only observer and assertion / reporting layer over the
> shipped `claw plan status` command. It may invoke status, parse the
> envelope, assert on every field, fail CI / test runs when STOP
> conditions appear, and emit observability with full-fidelity STOP
> detail. It MUST NOT execute any A2-L2b chain command other than
> `claw plan status`, MUST NOT compose approve+apply, MUST NOT mutate
> any file, MUST NOT call broker / model / Ollama, and MUST NOT run
> against a non-disposable repository. The harness adapter
> implementation itself remains a separate, future, explicitly-
> authorized lane bounded by this scope card and by the A2-L3 adapter
> boundary scope card.

The implementation of this lane is **not authorized by this scope
card**. This card defines the boundary that a future harness adapter
implementation lane must hold to. The next gate before any
implementation lane opens is operator review of this scope card.

```text
This card authorizes design only.
It does not authorize harness implementation.
It does not authorize IDE implementation.
It does not authorize approve/apply automation.
```

## 2. Why Harness Adapter First

Stack-Code is on the IDE/harness path. Two concrete A2-L3 adapter
surfaces are envisioned: an IDE adapter (human-facing panel / sidebar
/ status bar) and a harness adapter (machine-facing CI step / test
runner / soak checker / scripted observability tool). Only one
concrete adapter surface can be scoped per lane (see
[A2-L3 boundary card §16](./a2-l3-adapter-boundary-scope-card.md#16-next-lane-recommendation)).
This card scopes the **harness adapter** first.

The harness adapter is the lower-risk surface for three reasons:

1. **No clickable controls.** A harness adapter has no buttons, no
   keybindings, no command-palette entries, no gutter affordances,
   and no context-menu items. The class of failure where a "view
   status" control silently becomes a "click to approve" control
   ([A2-L3 boundary card §2](./a2-l3-adapter-boundary-scope-card.md#2-why-a2-l3-exists))
   cannot occur on a surface that has no controls.

2. **Machine-readable failures are loud.** Harness assertions either
   PASS or FAIL; STOP conditions either gate a CI run or they do not.
   There is no "low-contrast pill" failure mode of the kind §11 of the
   A2-L3 boundary card forbids — a harness assertion that quietly
   degrades STOP visibility is itself a harness bug a CI step can
   detect via test fixtures.

3. **Disposable execution context.** A harness adapter is, by its
   nature, run against ephemeral test workspaces and CI runners. A
   harness adapter explicitly run against a non-disposable repository
   is out of scope for this card (§14), so the blast radius of an
   incorrectly behaving harness adapter is bounded to the disposable
   workspace it was invoked against.

The IDE adapter remains deferred because its human-facing UI surface
is the harder safety problem: an IDE adapter must defend the TTY-
enforced approval boundary
([A2-L2b handoff §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
and the explicit-apply boundary
([A2-L2b handoff §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates))
against an entire surface area of plausible "convenience" affordances
the A2-L3 boundary card §8 enumerates. Scoping a harness adapter
first lets the project gain operational experience consuming
`a2-l2d-status.v1` before any human-clickable surface ships.

## 3. Relationship To A2-L3 Adapter Boundary

This card is the first concrete instance of the per-adapter scope
card lane recommended by
[A2-L3 boundary card §16](./a2-l3-adapter-boundary-scope-card.md#16-next-lane-recommendation):

> author a concrete scope card for a single adapter surface (IDE *or*
> harness, not both in one lane) that enumerates its allowed touched
> surfaces, its forbidden surfaces, its validation plan, its STOP-
> rendering test matrix, and its definition of done, all bounded by
> this A2-L3 adapter boundary scope card.

The A2-L3 adapter boundary scope card remains **authoritative** on
every adapter-boundary question. This card never weakens that
boundary, never expands the `MAY` set the boundary card pins, and
never narrows the `MUST NOT` set the boundary card pins. Where this
card is silent, the boundary card governs. Where this card narrows,
the narrower rule applies.

Specifically, this card inherits every constraint from:

- [A2-L3 boundary card §4](./a2-l3-adapter-boundary-scope-card.md#4-adapter-responsibilities) — adapter responsibilities.
- [A2-L3 boundary card §5](./a2-l3-adapter-boundary-scope-card.md#5-adapter-non-responsibilities) — adapter non-responsibilities.
- [A2-L3 boundary card §6](./a2-l3-adapter-boundary-scope-card.md#6-allowed-reads) — allowed reads.
- [A2-L3 boundary card §7](./a2-l3-adapter-boundary-scope-card.md#7-forbidden-actions) — forbidden actions.
- [A2-L3 boundary card §9](./a2-l3-adapter-boundary-scope-card.md#9-harness-boundary) — harness boundary preamble.
- [A2-L3 boundary card §10](./a2-l3-adapter-boundary-scope-card.md#10-status-contract-consumption-rules) — status contract consumption rules.
- [A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules) — STOP condition visibility rules.
- [A2-L3 boundary card §12](./a2-l3-adapter-boundary-scope-card.md#12-safety-invariants) — adapter safety invariants.
- [A2-L3 boundary card §13](./a2-l3-adapter-boundary-scope-card.md#13-non-goals) — A2-L3 non-goals.

This card adds the harness-specific input contract, output / reporting
contract, harness failure rules, idempotency rules, CI boundary, and
disposable-workspace requirement, all bounded by the above
inheritance.

## 4. Relationship To A2-L2d Status Contract

A2-L2d shipped:

- `claw plan status <workspace> [<approval-result.json>]` — read-only
  CLI command
  ([A2-L2d quickref §2](./a2-l2d-operator-quickref.md#2-command)).
- `a2-l2d-status.v1` envelope — pinned schema-of-record with closed
  `phase`, `stop_condition`, `next_operator_command`, and
  `audit_markers` enums, fixed field order, byte-identical idempotent
  stdout
  ([A2-L2d schema §3](./a2-l2d-status-schema.md#3-output-envelope)).
- `EXIT_STATUS_REFUSED == 12` — read-time refusal exit code
  ([A2-L2d schema §8](./a2-l2d-status-schema.md#8-exit-codes)).
- Read-only, network-egress-free, and idempotency invariants
  ([A2-L2d schema §11](./a2-l2d-status-schema.md#11-read-only-invariants)).

This card defines how a future harness adapter consumes the above
contract. It does not modify `a2-l2d-status.v1`, does not add fields,
does not add commands, does not add flags, does not add markers, does
not add exit codes, and does not change any A2-L2b or A2-L2d behavior.
The A2-L2d contract remains authoritative on every contract question;
this card governs only the harness adapter's consumption of that
contract.

## 5. Harness Adapter Responsibilities

When the future harness adapter is implemented as a separate lane, it
must exhibit the following responsibilities. These are the things the
harness adapter *exists to do*.

- **Invoke `claw plan status` as a read-only subprocess.** The
  harness adapter MUST source chain state exclusively from stdout of
  `claw plan status <workspace> [<approval-result.json>]`. It MUST
  NOT re-implement any `.claw/l2b-*` parsing logic, MUST NOT shortcut
  to artifact reads, and MUST NOT derive chain state from any other
  source.
- **Parse the JSON envelope.** The harness adapter MUST parse the
  envelope as JSON, MUST refuse any envelope whose `schema_version`
  is not the literal `a2-l2d-status.v1`
  ([A2-L3 boundary card §10.1](./a2-l3-adapter-boundary-scope-card.md#10-status-contract-consumption-rules)),
  and MUST treat the `phase`, `next_operator_command`, `stop_condition`,
  and `audit_markers` value sets as closed at the pinned A2-L2d enums.
- **Assert on envelope shape.** The harness adapter MUST verify that
  every required field documented in
  [A2-L2d schema §3](./a2-l2d-status-schema.md#3-output-envelope) is
  present and well-typed, and that no extra top-level fields appear.
  A divergence is an assertion failure; the harness MUST NOT coerce
  or repair the envelope.
- **Assert on `read_only_invariant`.** The harness adapter MUST
  assert that `read_only_invariant == "this command does not mutate
  state"` on every envelope. Absence, substitution, or alteration of
  that literal is a STOP signal in its own right
  ([A2-L3 boundary card §10.5](./a2-l3-adapter-boundary-scope-card.md#10-status-contract-consumption-rules))
  and MUST fail the harness assertion.
- **Surface STOP conditions at full fidelity.** Whenever the envelope
  carries a non-null `stop_condition`, the harness adapter MUST
  surface its exact closed-enum value (e.g.,
  `payload-sha-mismatch`, `live-target-missing`), MUST surface every
  entry of `evidence_paths`, and MUST surface every entry of
  `audit_markers`. Summarizing, debouncing, rate-limiting,
  abbreviating, redacting, or rewording STOP signals is forbidden
  ([A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules)).
- **Assert evidence-paths presence when STOP fires.** Whenever
  `stop_condition` is non-null, the harness adapter MUST assert that
  `evidence_paths` is non-empty (the A2-L2d producer always populates
  at least one evidence path when a STOP is detected; an empty
  `evidence_paths` array under a non-null `stop_condition` is itself
  a STOP signal indicating a broken producer).
- **Emit observability with full-fidelity STOP detail.** The harness
  adapter MAY emit envelope contents as structured logs or metrics.
  When it does, it MUST emit `stop_condition`, `evidence_paths`, and
  `audit_markers` at full fidelity
  ([A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules)).
  Field redaction for these fields in observability output is
  forbidden.
- **Fail the CI run when STOP conditions appear.** When a harness
  test case asserts that the chain should be in a continue state but
  the envelope carries a non-null `stop_condition`, the harness MUST
  fail the test. When a harness test case asserts that the chain
  should be in a STOP state but the envelope carries a null
  `stop_condition`, the harness MUST fail the test. Failing the test
  is the harness's only mechanism for surfacing chain-state divergence
  to the operator.
- **Verify idempotency.** The harness adapter MAY invoke
  `claw plan status` twice against the same unchanged workspace and
  assert byte-identical stdout
  ([A2-L2d schema §10](./a2-l2d-status-schema.md#10-idempotency-rules)).
  Non-idempotent output on an unchanged workspace is a STOP signal in
  its own right and MUST fail the harness assertion.
- **Honor `EXIT_STATUS_REFUSED`.** When `claw plan status` exits
  `12`, the harness adapter MUST parse the refusal envelope using the
  same rules as a non-refusal envelope, MUST assert the
  `a2-l2d-status-refused` marker is present
  ([A2-L2d schema §9](./a2-l2d-status-schema.md#9-refusal-envelope)),
  and MUST surface the `stop_condition` at full fidelity.
- **Operate only on disposable workspaces.** The harness adapter MUST
  refuse to run against a workspace that has not been explicitly
  marked as disposable by the harness invocation (§14). The harness
  adapter exists to exercise the status contract; it does not exist
  to inspect production state.

## 6. Harness Adapter Non-Responsibilities

These are the things the harness adapter exists *to not do*. Each
maps to a named safety property in A2-L2b, A2-L2c, A2-L2d, or A2-L3
boundary card.

- **Approval is not a harness responsibility.** The harness adapter
  MUST NOT call `claw plan approve`, MUST NOT construct an
  `<approval-result.json>` on the operator's behalf, MUST NOT emit
  approval text to any channel, and MUST NOT pre-fill the TTY
  approval prompt.
- **Apply-bundle generation is not a harness responsibility.** The
  harness adapter MUST NOT call `claw plan apply-bundle`, MUST NOT
  construct an apply bundle by hand, and MUST NOT package the inputs
  to apply-bundle generation as a harness action.
- **Apply is not a harness responsibility.** The harness adapter
  MUST NOT call `claw plan apply` based on envelope contents, MUST
  NOT surface an "apply" affordance in any channel, and MUST NOT
  compose apply with any other action.
- **Approve + apply composition is not a harness responsibility.**
  The harness adapter MUST NOT chain a successful status read into
  any approval, apply-bundle, or apply step. Composing approve and
  apply into a single harness action is forbidden by construction.
- **Retry is not a harness responsibility.** The harness adapter MUST
  NOT re-invoke any canonical chain command after a refusal or STOP
  signal. The operator's escalation path is human, not harness-
  triggered.
- **Rollback is not a harness responsibility.** `phase == rolled_back`
  is a read-only diagnosis surfaced by the envelope; the harness
  adapter MUST NOT initiate, suggest the initiation of, or pre-
  populate any rollback-adjacent command. There is no auto-rollback
  affordance on the harness surface.
- **Artifact mutation is not a harness responsibility.** The harness
  adapter MUST NOT write, rename, delete, copy, or move any file
  under `.claw/`, the workspace tree, the operator's home directory,
  the harness artifact directory, or anywhere else. The harness
  adapter MUST NOT introduce its own on-disk caches of envelope
  contents
  ([A2-L3 boundary card §7](./a2-l3-adapter-boundary-scope-card.md#7-forbidden-actions)).
- **STOP suppression is not a harness responsibility.** The harness
  adapter MUST NOT hide, debounce, collapse, attenuate, summarize-
  away, or rate-limit STOP signals. Every `stop_condition` value and
  every `STOP — escalate` directive must reach the operator at the
  granularity the envelope carries.
- **STOP auto-resolution is not a harness responsibility.** The
  harness adapter MUST NOT auto-resolve a STOP signal by retrying,
  hand-editing artifacts, regenerating bundles, or invoking
  remediation scripts. STOP is a human escalation signal.
- **State invention is not a harness responsibility.** The harness
  adapter MUST NOT compute, infer, or emit chain state that the
  envelope does not carry. "Pending", "queued", "in-flight", or
  "progressing" assertions that are not derivable from a single
  envelope field are forbidden.
- **CLI extension is not a harness responsibility.** The harness
  adapter MUST NOT propose, request, or depend on new CLI commands,
  flags, schema versions, exit codes, or markers in order to
  function. Any contract gap the harness adapter discovers is
  escalated as a separate scope-card lane.
- **Workflow control is not a harness responsibility.** The harness
  adapter is a read-only observer and assertion / reporting layer.
  It is **not** a workflow controller, not an approval executor, not
  an apply executor, and not a remediation runner.

## 7. Allowed Reads

The future harness adapter implementation lane may read, and only
read, the following:

- stdout of `claw plan status <workspace>` (success envelope, exit
  `0`).
- stdout of `claw plan status <workspace> <approval-result.json>`
  (success envelope, exit `0`).
- stdout of `claw plan status <workspace> [<approval-result.json>]`
  refusal envelope (exit `EXIT_STATUS_REFUSED == 12`).
- exit code of the above invocations.
- harness-test-case configuration that specifies the workspace path,
  the optional `<approval-result.json>` path, the expected `phase`,
  the expected `stop_condition` (including null), the expected
  `read_only_invariant` literal, and any expected evidence-path
  patterns.

The harness adapter may not read:

- any `.claw/**` file directly (read is mediated exclusively through
  the status command).
- any workspace file directly (the harness adapter does not open
  evidence-path entries from the envelope).
- broker endpoints, model endpoints, Ollama endpoints, or any HTTP
  surface.
- secrets, environment variables beyond what is required to set
  network-isolation sentinels (`HTTP_PROXY`, `HTTPS_PROXY`,
  `OLLAMA_HOST` —
  [A2-L3 boundary card §9](./a2-l3-adapter-boundary-scope-card.md#9-harness-boundary)),
  the operator's shell history, the operator's terminal state, or any
  non-workspace file beyond what `claw plan status` itself already
  reads.
- prior harness-process state on disk (the harness adapter MUST NOT
  introduce an on-disk cache of envelope contents).

The harness adapter explicitly does **not** subscribe to a filesystem
watcher, a Git event stream, a daemon push channel, or any
notification surface that would let it re-invoke status without an
explicit harness-test-case schedule.

## 8. Forbidden Actions

The future harness adapter implementation lane is forbidden from
performing any of the following.

- spawning `claw plan run`, `claw plan approve`,
  `claw plan apply-bundle`, or `claw plan apply` as a subprocess for
  any reason.
- spawning any process other than the read-only `claw plan status`
  command, and only with no flags
  ([A2-L2d schema §2](./a2-l2d-status-schema.md#2-canonical-invocation)).
- pre-filling, automated completion, or otherwise generating the TTY
  approval line `apply <step_id> <preview_sha256>` in any channel.
- producing or persisting `<approval-result.json>` on the operator's
  behalf.
- producing or persisting `apply-bundle.json` on the operator's
  behalf.
- modifying any file under `.claw/`, the workspace tree, the
  operator's home directory, the harness artifact directory, or
  anywhere else, except to emit harness-local logs / metrics /
  test-result reports that the harness's own CI surface consumes.
- composing `approve` and `apply` into a single harness action,
  whether by chained subprocess, sequenced harness step, scripted
  shortcut, plug-in, macro, recorded action, or any other mechanism.
- offering a "one-click", "fast", "express", "skip", or "trust" mode
  that elides any operator gesture the canonical chain requires.
- adding write-adjacent flags to the harness CLI that map onto
  refused `claw plan status` flags (`--apply`, `--approve`, `--yes`,
  `--auto`, `--clean`, `--rollback`, `--mutate`, `--all-runs`,
  `--no-prompt`, `--skip-approval`, `--cache`); harness flags that
  compose to the same semantic effect as those flags are equally
  forbidden.
- calling broker, model, Ollama, telemetry, analytics, error-
  reporting, or any other network endpoint at any phase of harness
  operation.
- caching envelope contents on disk in any form (`.claw/l3-harness-
  cache/`, `~/.cache/claw-harness/`, harness artifact directory, or
  anywhere else).
- watching the filesystem for `.claw/**` changes and re-invoking
  status without an explicit harness-test-case schedule.
- summarizing, collapsing, debouncing, rate-limiting, or otherwise
  attenuating any STOP signal carried by the envelope (§11).
- introducing parallel status schemas, parallel envelope versions, or
  "extended" envelopes that wrap `a2-l2d-status.v1` with harness-
  specific fields and re-emit them as authoritative.
- treating the envelope as authoritative for write decisions; the
  chain re-validates every input at apply time
  ([A2-L2b handoff §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
  and the harness adapter MUST NOT pretend otherwise.
- becoming a hidden workflow driver via any composition of allowed
  reads, allowed observability output, and harness-CLI configuration.
- running against a non-disposable repository (§14).

## 9. Input Contract

A future harness adapter implementation MUST accept exactly the
following inputs per test case. No other inputs are permitted; harness
test cases that require inputs not listed below MUST escalate as a
separate scope-card lane rather than extending this input contract.

| Input                          | Required | Source                                                                                                                          |
|--------------------------------|----------|---------------------------------------------------------------------------------------------------------------------------------|
| `workspace_root`               | required | Absolute or relative path to a disposable workspace (§14). Passed verbatim to `claw plan status` as the first positional.       |
| `approval_result_path`         | optional | Absolute or relative path to an `<approval-result.json>`. Passed verbatim as the second positional when supplied. Must already exist on disk; the harness MUST NOT create it. |
| `expected_schema_version`      | required | Literal `"a2-l2d-status.v1"`. Hardcoded; not operator-configurable.                                                             |
| `expected_phase`               | required | One of the closed `phase` enum values from [A2-L2d schema §4](./a2-l2d-status-schema.md#4-closed-phase-enum), or the sentinel `"any"` when the test case is phase-agnostic. |
| `expected_stop_condition`      | required | One of the closed `stop_condition` enum values from [A2-L2d schema §6](./a2-l2d-status-schema.md#6-closed-stop_condition-enum), or `null` for the "continue" expectation. Sentinel `"any-stop"` is permitted for tests that require a STOP without pinning which one. |
| `expected_read_only_invariant` | required | Literal `"this command does not mutate state"`. Hardcoded; not operator-configurable.                                           |
| `expected_evidence_path_patterns` | optional | List of glob patterns each entry in `evidence_paths` must match against. Optional only when `expected_stop_condition == null` AND the test case does not assert on evidence specifically. |
| `disposable_workspace_marker`  | required | Operator-supplied affirmation that `workspace_root` is disposable (§14). Absence MUST cause the harness to refuse to run.       |

Notes:

- The harness adapter MUST NOT accept a `--yes`, `--auto`,
  `--skip-approval`, `--no-prompt`, preapproval, batch-approval, or
  approval-bypass input on any channel. None of these inputs exist on
  the harness surface.
- The harness adapter MUST NOT accept an input that selects which
  canonical chain command to invoke. The only command the harness
  adapter ever spawns is `claw plan status`, with no flags.
- The harness adapter MUST NOT accept a "network-allowed" or
  "broker-allowed" input on any channel. Network egress beyond the
  `claw plan status` subprocess is forbidden.

## 10. Output / Reporting Contract

A future harness adapter implementation MUST emit exactly the
following outputs per test case. No other outputs are permitted;
harness reporting surfaces that require outputs not listed below MUST
escalate as a separate scope-card lane rather than extending this
output contract.

| Output                       | Required | Content                                                                                                                                                                 |
|------------------------------|----------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `result`                     | required | `"pass"` or `"fail"`. No "warn", "soft-fail", "skip-on-stop", or "ignore-stop" state is permitted.                                                                      |
| `parsed_envelope`            | required | The parsed `a2-l2d-status.v1` envelope emitted verbatim by the harness adapter as structured data. Pretty-JSON byte equivalence is not required; field-for-field equality is. |
| `assertion_summary`          | required | One entry per assertion the harness performed, each carrying the assertion name, the expected value, the observed value, and a `"pass"` / `"fail"` flag.                |
| `stop_condition_full`        | required | The exact `stop_condition` enum value (or null) from the envelope, re-emitted at full fidelity. Field redaction is forbidden.                                           |
| `evidence_paths_full`        | required | Every entry of `evidence_paths` from the envelope, re-emitted at full fidelity, in the same lexicographic order. Field redaction is forbidden.                          |
| `audit_markers_full`         | required | Every entry of `audit_markers` from the envelope, re-emitted at full fidelity, in the same lexicographic order. Field redaction is forbidden.                           |
| `diagnostic_message`         | required | A human-readable description of the assertion outcome. For STOP cases, MUST quote the exact `stop_condition` enum value and the exact `next_operator_command` literal. |
| `exit_code`                  | required | `0` on `result == "pass"`, non-`0` on `result == "fail"`, matching the harness's CI convention. STOP signals MUST surface as non-`0` regardless of CI ergonomics.       |

Notes:

- The harness adapter MUST NOT emit a "best-guess" repair of the
  envelope. If the envelope cannot be parsed, the output is a
  `result == "fail"` with the raw stdout text quoted in
  `diagnostic_message`.
- The harness adapter MAY emit additional observability streams
  (structured logs, metrics, traces) that mirror the above outputs at
  full fidelity. It MUST NOT emit observability streams that redact
  `stop_condition`, `evidence_paths`, or `audit_markers`
  ([A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules)).

## 11. STOP Condition Handling

The harness adapter inherits every STOP visibility rule from
[A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules)
verbatim, plus the following harness-specific rules.

- **Verbatim STOP-value reporting.** When `stop_condition` is non-
  null, the harness adapter MUST report its exact closed-enum value
  (e.g., `payload-sha-mismatch`, `live-target-missing`) in every
  output channel — `parsed_envelope`, `stop_condition_full`,
  `assertion_summary`, `diagnostic_message`, and any observability
  stream the harness adapter emits.
- **STOP-prominence parity.** STOP reporting MUST be at least as
  prominent in observability output as non-STOP reporting. A
  structured log line that flattens a STOP to a single boolean
  `has_stop=true` is a category violation; the full `stop_condition`
  enum value, the full `evidence_paths` list, and the full
  `audit_markers` list MUST appear.
- **No STOP debouncing.** If two successive invocations both produce
  the same STOP signal, the harness adapter MUST report the STOP both
  times. Adapters MUST NOT collapse "same STOP twice in a row" into a
  single notification.
- **No STOP rate-limiting.** The harness adapter MUST NOT throttle
  STOP notifications. Every emitted envelope with a non-null
  `stop_condition` is a STOP event from the operator's perspective.
- **STOP-on-unknown.** If the harness adapter receives a `phase`,
  `stop_condition`, `next_operator_command`, or marker value not in
  the A2-L2d schema's closed enums, the harness adapter MUST treat
  that as a STOP signal in its own right and fail the test with the
  observed unknown value quoted verbatim.
- **STOP retention across invocations.** The harness adapter MUST
  NOT clear a STOP from its output without a fresh
  `claw plan status` invocation. STOP state is per-envelope; the
  harness adapter never claims STOP has resolved without re-reading.
- **STOP in observability.** When the harness adapter emits envelope
  contents as structured logs or metrics, it MUST emit
  `stop_condition`, `evidence_paths`, and `audit_markers` at full
  fidelity. Field redaction is forbidden
  ([A2-L3 boundary card §11](./a2-l3-adapter-boundary-scope-card.md#11-stop-condition-visibility-rules)).
- **No "ignore STOP for N seconds" affordance.** The harness adapter
  MUST NOT offer a snooze, mute, dismiss, or ignore action for STOP
  signals on any input or configuration surface.
- **STOP cannot be summarized away.** Forbidden harness behaviors
  include: summarizing STOP away; hiding STOP in debug logs only;
  turning STOP into a warning; retrying past STOP; normalizing
  unknown STOP into "unknown ok". Each is an explicit category
  violation of §6 and §11.

## 12. Idempotency And Repeatability

A2-L2d pins byte-identical idempotent stdout on two successive
invocations against an unchanged workspace
([A2-L2d schema §10](./a2-l2d-status-schema.md#10-idempotency-rules)).
The harness adapter MAY rely on this guarantee for the following
assertions.

- **Idempotency assertion.** The harness adapter MAY invoke
  `claw plan status` twice against the same unchanged workspace and
  assert byte-identical stdout. Non-idempotent output on an unchanged
  workspace is a STOP signal in its own right and MUST fail the
  assertion.
- **Repeatability assertion.** The harness adapter MAY invoke
  `claw plan status` against a fixture workspace whose `.claw/**`
  contents are pinned, and assert that the parsed envelope equals a
  golden-file expectation field-for-field. Golden files MUST be
  versioned alongside the harness adapter test corpus.
- **No envelope caching.** The idempotency guarantee does not
  authorize the harness adapter to cache envelope contents across
  invocations. Every assertion is against a fresh `claw plan status`
  invocation; cached values are diagnostic only and never re-used in
  later assertions.
- **No envelope wrapping.** The harness adapter MUST NOT wrap an
  `a2-l2d-status.v1` envelope inside a harness-versioned envelope
  and re-emit the result as authoritative
  ([A2-L3 boundary card §10.8](./a2-l3-adapter-boundary-scope-card.md#10-status-contract-consumption-rules)).
  Pass-through reporting is permitted; rewrapping is not.
- **No idempotency repair.** If two successive invocations against
  the same unchanged workspace produce divergent stdout, the harness
  adapter MUST report the divergence verbatim and fail the test. It
  MUST NOT pick one of the two envelopes as "canonical" and discard
  the other.

## 13. CI / Test-Harness Boundary

The harness adapter is intended to be invoked from CI pipelines, test
runners, soak-test schedules, and scripted observability tools. The
following rules govern that boundary.

- **CI runs only against disposable workspaces.** Every CI invocation
  of the harness adapter MUST be against a disposable workspace
  (§14). A CI step that points the harness adapter at a production
  repository, a developer's own working tree, or any other
  non-disposable repository is forbidden by construction.
- **CI never composes harness with approve / apply.** A CI step that
  chains a successful harness assertion into `claw plan approve`,
  `claw plan apply-bundle`, or `claw plan apply` is forbidden. The
  harness adapter exists to verify status contract behavior; the
  canonical operator-gated chain is the only path to a write, and
  that path requires the TTY-enforced approval boundary
  ([A2-L2b handoff §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
  which CI does not satisfy.
- **CI honors STOP signals.** A CI step that observes a non-null
  `stop_condition` from the harness adapter MUST treat the build as
  failed. Marking the step as "soft fail" or "advisory" is forbidden.
- **CI surfaces STOP at full fidelity.** A CI step MUST surface the
  exact `stop_condition` enum value, the full `evidence_paths` list,
  and the full `audit_markers` list in the build log. Truncation or
  summarization in CI annotations is forbidden.
- **CI never auto-remediates.** A CI step that auto-remediates a
  STOP signal — by retrying, by hand-editing artifacts, by
  regenerating bundles, by invoking remediation scripts, or by any
  other mechanism — is forbidden. STOP escalation is human.
- **CI never schedules approval.** A CI step that schedules a
  follow-up approval, queues approval for an operator, or pre-stages
  an approval-result file based on harness outputs is forbidden. The
  operator constructs and submits approval via the canonical chain,
  not via CI plumbing.
- **CI scheduling is operator-defined.** The harness adapter has no
  built-in scheduler. Background polling, filesystem watchers, and
  daemon channels are forbidden
  ([A2-L3 boundary card §14](./a2-l3-adapter-boundary-scope-card.md#14-future-implementation-constraints)).
  Re-invocation cadence is the CI step's responsibility, not the
  harness adapter's.

## 14. Disposable Workspace Requirement

The harness adapter MUST run only against disposable workspaces. A
disposable workspace is one that satisfies all of the following.

- The workspace is created fresh per harness invocation, or is
  created from a pinned fixture archive whose contents are pinned in
  the harness corpus.
- The workspace contains no production state, no operator-private
  state, no credentials, no secrets, and no data whose loss would
  matter beyond the harness invocation.
- The workspace is owned by the harness invocation for the duration
  of the test, and is torn down (or left for forensic inspection)
  after the test completes.
- The workspace is not a checkout of, a worktree of, or a clone of
  any non-disposable repository.

Enforcement rules:

- The harness adapter MUST require an explicit
  `disposable_workspace_marker` input (§9). Absence MUST cause the
  harness adapter to refuse to run with a STOP-class diagnostic
  message naming the missing marker.
- The harness adapter MUST NOT silently accept a workspace whose
  path is ambiguous (a missing directory, a non-directory file, a
  symlink to a non-disposable repository). Each is a STOP-class
  refusal; the harness adapter does not infer disposability.
- A harness invocation against a non-disposable workspace is **out
  of scope for this card** and is forbidden by §6 and §8. Any future
  use of the harness adapter against a non-disposable workspace must
  be opened as a separate, explicitly-authorized scope card.

## 15. Safety Invariants

The future harness adapter implementation lane must preserve, verbatim,
every property the prior lanes pinned:

- preview before approval
- TTY/operator approval enforcement
  ([A2-L2c quickref §3](./a2-l2c-operator-quickref.md#3-tty-approval-eof-note))
- approval bound to `step_id` + `preview_sha256` from the preview
  record
- apply-bundle generation as a separate offline step
- apply as a separate explicit operator step
- single-file write per apply bundle
- no model-generated `after` bytes
- no broker, model, or Ollama traffic at any phase
- no commits, pushes, or merges performed inside the chain
- no autonomous mutation of any non-disposable repo
- A2-L2d read-only invariant
  ([A2-L2d schema §11](./a2-l2d-status-schema.md#11-read-only-invariants))
- A2-L2d network-egress-free invariant
- A2-L2d idempotency invariant
- A2-L2d non-overlapping marker invariant (`a2-l2d-*` only; the
  harness adapter MUST NOT invent `a2-l3-*` markers that leak back
  into the status producer)
- A2-L2d non-overlapping exit-code invariant
- A2-L3 adapter read-only invariant
  ([A2-L3 boundary card §12](./a2-l3-adapter-boundary-scope-card.md#12-safety-invariants))
- A2-L3 adapter STOP-visibility invariant
- A2-L3 adapter no-write-surface invariant
- A2-L3 adapter no-state-invention invariant
- A2-L3 adapter no-shadow-contract invariant

In addition, this card adds:

- **Harness read-only invariant.** No harness adapter operation may
  mutate any file beyond harness-local logs / metrics / test-result
  reports the harness's own CI surface consumes; no harness adapter
  operation may send network egress beyond the `claw plan status`
  subprocess it spawns; no harness adapter operation may call
  broker / model / Ollama.
- **Harness STOP-fidelity invariant.** Every STOP signal in an
  envelope reaches the operator at full fidelity across every
  harness output channel (§11).
- **Harness no-workflow-driver invariant.** No combination of
  allowed harness reads, allowed harness observability output, and
  harness-CLI configuration composes into a hidden workflow driver
  against the A2-L2b chain.
- **Harness no-non-disposable-target invariant.** No harness
  invocation runs against a non-disposable workspace; absence of the
  disposability marker is a STOP-class refusal (§14).
- **Harness no-envelope-wrapping invariant.** No harness adapter
  re-emits a harness-versioned envelope as authoritative
  ([A2-L3 boundary card §10.8](./a2-l3-adapter-boundary-scope-card.md#10-status-contract-consumption-rules)).

## 16. Non-Goals

This card must not:

- implement the harness adapter (deferred; this card is docs-only).
- implement an IDE adapter (deferred to a separate scope card).
- implement an IDE adapter scope card (out of scope for this lane).
- introduce or imply autonomous workspace-write execution.
- introduce harness controls that approve, that apply, that compose
  approval-and-apply into a single gesture, or that bypass the
  TTY-enforced approval boundary in any way.
- introduce harness-driven `claw plan approve`, `claw plan apply-
  bundle`, or `claw plan apply` invocation.
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  preapproval, batch approval, or any approval-bypass affordance on
  the harness surface.
- modify `claw plan run`, `claw plan approve`,
  `claw plan apply-bundle`, `claw plan apply`, or `claw plan status`
  behavior, exit codes, schemas, markers, or JSON field shapes.
- modify `a2-l2b-*`, `a2-l2d-status.v1`, or A2-L3 boundary card
  contracts.
- introduce an `a2-l3-harness-*` schema, marker, exit code, or CLI
  surface (the contract this lane governs IS `a2-l2d-status.v1`;
  this lane introduces no parallel contract).
- call broker, model, or Ollama at any phase.
- introduce filesystem watchers, daemon push channels, or background
  re-invocation of `claw plan status`.
- introduce on-disk caches of envelope contents.
- weaken any A2-L2b, A2-L2c, A2-L2d, or A2-L3 STOP gate.
- introduce a harness adapter that auto-remediates STOP signals.
- introduce a harness adapter that runs against a non-disposable
  repository.
- introduce a CI plug-in that pre-stages approval-result files based
  on harness outputs.
- introduce a CI plug-in that schedules apply on a successful harness
  assertion.

Any of the above must be opened as a separate, explicitly-authorized
lane.

## 17. Future Implementation Constraints

When the harness adapter implementation lane is opened (as a separate
scope card), it must hold to all of the following.

- **Allowed touched surfaces** must be explicitly enumerated in the
  implementation scope card before any code or wrapper is authored.
  The implementation lane MUST NOT touch any file outside that
  enumerated list. Likely surfaces include a new harness-adapter
  crate under `rust/crates/`, new tests under that crate's `tests/`,
  new fixture directories under that crate's `tests/fixtures/`, and
  new documentation under `docs/`. Specific paths are deferred to the
  implementation scope card.
- **Forbidden surfaces** must explicitly include every A2-L2b
  module
  (`rust/crates/a2-plan-runner/src/{approval,approval_ux,
  checkpoint,diff_preview,preflight,report,runner,write_executor,
  write_payload,write_preview,write_runtime,markers}.rs`), every
  A2-L2b schema constant, every A2-L2b exit-code constant, every
  A2-L2b/`a2-l2d-*` marker constant, and
  `rust/crates/a2-plan-runner/src/status.rs` (the harness consumes
  the status command's stdout; it does not modify the producer).
- **Validation** must include: forbidden-language sniff against the
  staged diff for the same regex family as the A2-L2b / A2-L2c /
  A2-L2d / A2-L3 boundary cards; tests asserting the harness adapter
  spawns no subprocess other than `claw plan status`; tests
  asserting the harness adapter performs zero filesystem writes
  beyond harness-local logs / metrics / test-result reports under
  any input; tests asserting the harness adapter performs zero
  network egress beyond the status subprocess; tests asserting the
  harness adapter surfaces every STOP signal at full fidelity (§11);
  regression tests confirming every existing A2-L2b / A2-L2c /
  A2-L2d test still passes unchanged.
- **STOP-rendering test matrix** must cover: every closed
  `stop_condition` value, every closed `phase` value, every closed
  `next_operator_command` shape, the refusal envelope, at least one
  unknown-enum-value synthetic fixture, at least one missing
  `read_only_invariant` synthetic fixture, at least one substituted
  `read_only_invariant` synthetic fixture, at least one non-
  idempotent-stdout synthetic fixture, and at least one
  unparseable-stdout synthetic fixture.
- **Operator-driven re-invocation** is the only harness-initiated
  CLI invocation. The implementation scope card must explicitly
  forbid background polling, filesystem watchers, and daemon
  channels.
- **Disposability enforcement** must include a positive test that
  the harness refuses to run when `disposable_workspace_marker` is
  absent, and a negative test that the harness proceeds when the
  marker is present and the workspace is genuinely disposable.
- **Network-isolation enforcement** must include tests that run with
  `HTTP_PROXY`, `HTTPS_PROXY`, and `OLLAMA_HOST` set to unreachable
  sentinels, mirroring the A2-L2d invariants
  ([A2-L2d schema §11](./a2-l2d-status-schema.md#11-read-only-invariants)).
- **No production / SideStackAI / live-broker invocation.** Tests
  MUST NOT contact the SideStackAI broker, the SideStackAI model
  endpoints, or any live broker / model / Ollama surface. Tests run
  fully offline.

## 18. Definition Of Done

This **scope card** is done when:

- `docs/a2-l3-harness-adapter-scope-card.md` exists and matches the
  sectional structure required by the prompt.
- The card defines harness adapter responsibilities and non-
  responsibilities in non-softening language.
- The card pins the input contract, the output / reporting contract,
  the STOP handling rules, the idempotency and repeatability rules,
  the CI / test-harness boundary, and the disposable-workspace
  requirement.
- The card declares the harness adapter as docs-only at this scope-
  card stage.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, A2-L2d, or A2-L3 STOP gate is weakened.
- No `a2-l2d-status.v1` contract field, enum, marker, or exit code
  is modified.
- The card is reviewed by the operator before any harness adapter
  implementation scope-card lane is opened.

The harness adapter **implementation lane** is out of scope for this
card. Definition of done for that lane will be authored when its own
scope card is created, bounded by the constraints in §§5–17 above and
by every constraint inherited from the A2-L3 boundary card and from
the prior A2-L2b / A2-L2c / A2-L2d lanes.

## 19. Next Lane Recommendation

The recommended next lane after this scope card is reviewed and CI
turns green on its PR is:

> **Read-only PR review of this scope card.** Operator reviews the
> harness-specific input contract, output / reporting contract,
> STOP-handling rules, idempotency rules, CI boundary, disposable-
> workspace requirement, and future-implementation constraints
> against the A2-L3 boundary card, the A2-L2d status schema, and the
> A2-L2b operator handoff. No implementation lane opens until the
> review pins any further narrowing required.

The lane *after* the scope card review lands is:

> **Harness adapter implementation scope-card lane (docs-only).**
> Author a concrete implementation scope card that enumerates the
> allowed touched surfaces, the forbidden surfaces, the validation
> plan, the STOP-rendering test matrix, the disposability-
> enforcement test matrix, the network-isolation test matrix, and
> the definition of done, all bounded by this card and by the A2-L3
> boundary card. Do not author the harness adapter implementation
> in the same lane as its scope card.

The lane *after* the implementation scope card lands is:

> **The harness adapter implementation lane.** Implement the
> harness adapter under the constraints pinned by this card and by
> its own scope card, with golden tests for every STOP signal, the
> read-only invariant, the disposable-workspace requirement, and the
> network-isolation requirement. The implementation lane MUST NOT
> expand the contract; any contract gap discovered during
> implementation is escalated as a separate scope-card lane.

None of the above lanes permits autonomous workspace-write execution.
All remain bounded by the A2-L2b, A2-L2c, A2-L2d, and A2-L3 boundary
safety properties and by §§5–17 of this card.

## 20. References

- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 adapter boundary scope card. Authoritative on every
  adapter-boundary question this card narrows.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the contract
  the harness adapter consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card. Section 10 ("IDE / Harness Boundary") is the
  preamble the A2-L3 boundary card expanded.
- [`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md) — A2-L2c scope
  card; the structural model this card follows.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference.
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.
- PR #39 (`12fff14`) — A2-L2d scope card.
- PR #40 (`0f75800`) — A2-L2d read-only `claw plan status` command +
  `a2-l2d-status.v1`.
- PR #41 (`4c2b15e`) — A2-L2d operator quick reference.
- PR #42 (`21d9b5b`) — A2-L3 adapter boundary scope card.

## 21. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- Harness adapter implementation authorized: **no**.
- IDE adapter implementation authorized: **no**.
- IDE adapter scope card authorized: **no**.
- Autonomous-write authorization: **no**.
- Approval / apply boundary weakened: **no**.
- A2-L2b / A2-L2c / A2-L2d / A2-L3 STOP gate weakened: **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- A2-L3 adapter boundary scope card modified: **no**.
- SideStackAI touched: **no**.
- Live smokes run: **no**.
- Tests run: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by a harness adapter implementation scope-card
  lane.
