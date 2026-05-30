# A2-L3 Scope Card — Harness Adapter (Docs-Only)

This document is a **design-only scope card** for the first per-adapter
lane bounded by the A2-L3 adapter boundary scope card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)).
It defines, in design only, the bounded behavior of a future **harness
adapter** that consumes the shipped A2-L2d read-only status surface as
a programmatic observer and assertion layer.

This file itself authorizes **no** runtime change, **no** CLI change,
**no** harness implementation, **no** IDE integration, and **no**
behavior that weakens the A2-L2b operator-gated chain. It is a
per-adapter scope card; the harness adapter implementation lane is a
separate, future, explicitly-authorized lane that this card constrains.

A2-L3 sits exactly one layer above A2-L2d in the planned progression,
and the harness adapter sits one layer below the A2-L3 adapter
boundary card:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, shipped)
      → IDE / harness adapter boundary (A2-L3, scope card shipped)
        → harness adapter per-adapter scope card (THIS DOCUMENT)
          → future harness adapter implementation (separate, future)
```

The per-adapter scope card for the IDE adapter is a separate, future
lane. This card does not author, authorize, or pre-empt it.

## 1. Executive Summary

The A2-L3 harness adapter is a **read-only observer and assertion
layer** over `a2-l2d-status.v1`. It is a machine-facing consumer — a
test runner, scripted observability tool, CI step, soak-test checker,
or operator script — that invokes `claw plan status` as a subprocess,
parses the emitted envelope, asserts on its fields, and emits
observability artifacts (logs, metrics, pass/fail results) at full
envelope fidelity.

The harness adapter is **not** a workflow controller. It does not
invoke `claw plan run`, `claw plan approve`, `claw plan apply-bundle`,
or `claw plan apply`. It does not generate `<approval-result.json>` or
`apply-bundle.json`. It does not retry, roll back, clean, or remediate
on STOP signals. It does not compose `approve` and `apply` into a
single action through any mechanism. Every write-adjacent affordance
the A2-L2b chain forbids being executed without operator approval
remains forbidden for the harness adapter by construction.

The harness adapter is chosen as the **first** per-adapter scope under
A2-L3 because it is the lower-risk surface: its consumption pattern is
machine-bounded (subprocess + JSON parse + assertion), the operator
gestures it requires are zero by default, and its failure mode is
"loud" (an assertion failure or a non-zero CI exit) rather than
"silent" (a hidden UI shortcut that quietly composes operator actions).
The IDE adapter, with its richer affordance surface, follows in a
separate per-adapter scope card lane after this one is reviewed and a
concrete implementation lane is reviewed and accepted.

This card defines the boundary the future harness adapter
implementation lane must hold to. The next gate before harness
implementation is operator review of this scope card, followed by a
harness-adapter implementation scope-card lane that enumerates its
allowed touched surfaces and validation plan.

## 2. Why Harness Adapter First

Three reasons place the harness adapter ahead of the IDE adapter in
the A2-L3 progression:

1. **Surface area is bounded by JSON, not by UI.** A harness adapter
   consumes `a2-l2d-status.v1` through a subprocess and a JSON parse.
   There are no buttons, no keybindings, no command-palette entries,
   no gutter affordances, no context-menu items, no status-bar pills,
   and no drag targets to design defensively. The set of write-
   adjacent surfaces an IDE could grow is broad; the set of write-
   adjacent surfaces a harness assertion library can grow is narrow.
   The narrow surface is the right surface to pin first.

2. **Failure mode is loud, not silent.** A harness assertion failure
   produces a non-zero exit and a structured diagnostic. A misdesigned
   IDE affordance — for example, a "Ready to Apply" pill that hides
   the underlying `stop_condition` — fails silently from the
   operator's perspective. The chain's safety depends on STOPs being
   seen, so the surface that fails loudly should be designed first.

3. **Test-harness consumption is the lower-coupling user.** A harness
   adapter has no requirement to render arbitrary state, infer
   intent, or anticipate future operator actions. It asserts on the
   envelope it received and reports. That bounded job is the
   appropriate first surface to design, because every other adapter
   will be expected to honor at least these constraints.

The harness adapter is therefore a forcing function: writing it down
first makes the boundary explicit before the higher-coupling IDE
adapter is opened.

## 3. Relationship To A2-L3 Adapter Boundary

[`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
defined the overall A2-L3 adapter boundary in design only. Its §9
already pinned a harness-boundary preamble naming the high-level
"MAY / MUST NOT" pair for harness consumers. This per-adapter scope
card expands that preamble into a full scope card for the harness
surface alone.

Specifically:

- The harness adapter remains bound by every safety invariant in the
  A2-L3 adapter boundary card §12 (`Safety Invariants`), §13
  (`Non-Goals`), and §14 (`Future Implementation Constraints`).
- The harness adapter remains bound by the four A2-L3-introduced
  invariants in that card §12: adapter read-only, adapter
  STOP-visibility, adapter no-write-surface, adapter no-state-
  invention, adapter no-shadow-contract.
- This card refines those invariants for the harness surface but
  does **not** relax any of them and does **not** introduce a parallel
  contract. Where this card and the boundary card differ in tone,
  the boundary card and (above it) A2-L2d remain authoritative on the
  contract.
- This card does **not** modify `a2-l2d-status.v1`, add new fields, add
  new commands, add new flags, add new schema versions, add new exit
  codes, or add new markers. The A2-L2d status schema
  ([`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)) is
  authoritative on the envelope.

## 4. Relationship To A2-L2d Status Contract

A2-L2d shipped `claw plan status <workspace> [<approval-result.json>]`,
the `a2-l2d-status.v1` envelope, `EXIT_STATUS_REFUSED == 12`, and the
read-only/network-egress-free/idempotency invariants pinned in
[`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants).
The harness adapter consumes that surface as-is:

- The harness adapter invokes only `claw plan status <workspace>
  [<approval-result.json>]`, with no flags. Every write-adjacent flag
  is refused by the A2-L2d implementation; the harness adapter does
  not re-enforce that refusal because the producer already does, but
  the harness adapter MUST NOT attempt to invoke variants the status
  command would refuse.
- The harness adapter treats `a2-l2d-status.v1` as the authoritative
  envelope shape. It does not parse any other surface, does not
  shortcut to `.claw/**` artifacts, and does not derive chain state
  from any source other than `claw plan status` stdout and exit code.
- The harness adapter honors every closed enum
  ([`a2-l2d-status-schema.md` §§4–7](./a2-l2d-status-schema.md#4-closed-phase-enum))
  as closed. Unknown enum values are themselves a STOP signal
  surfaced by the harness, not silently coerced.
- The harness adapter honors the refusal envelope
  ([`a2-l2d-status-schema.md` §9](./a2-l2d-status-schema.md#9-refusal-envelope))
  with the same envelope handling it applies to non-refusal envelopes.
  Exit code `12` is not collapsed into "treat as failure" without
  surfacing the refusal envelope's `stop_condition`.
- The harness adapter relies on the A2-L2d idempotency invariant
  ([`a2-l2d-status-schema.md` §10](./a2-l2d-status-schema.md#10-idempotency-rules)):
  two successive reads against an unchanged workspace produce byte-
  identical stdout. The harness adapter may assert on this property,
  but MUST NOT exploit it to skip rendering or to cache envelopes as
  authoritative state across runs.

## 5. Harness Adapter Responsibilities

When the future harness adapter implementation lane lands as a
separate scope, it must exhibit the following responsibilities. These
are the things the harness adapter exists *to do*.

- **Invoke `claw plan status` as a read-only subprocess.** The harness
  adapter MUST source chain state exclusively from
  `claw plan status <workspace> [<approval-result.json>]`. It MUST
  spawn the status binary as a subprocess, capture stdout and exit
  code, and parse stdout as JSON. No other subprocess is permitted.
- **Parse `a2-l2d-status.v1` envelopes.** The harness adapter MUST
  parse stdout as the `a2-l2d-status.v1` envelope schema and MUST
  refuse any envelope whose `schema_version` literal does not match
  exactly. Unrecognized versions trigger a STOP signal in the harness
  output; they are not best-effort parsed.
- **Assert envelope invariants.** The harness adapter MUST assert
  every property a caller declares (phase, stop_condition,
  is_approvable, is_apply_ready, the SHA fields, marker presence,
  evidence-path patterns) against the parsed envelope and report each
  assertion's pass/fail result individually.
- **Assert the `read_only_invariant` literal.** The harness adapter
  MUST assert that `read_only_invariant == "this command does not
  mutate state"` is present on every envelope it inspects. Absence
  or substitution is itself a STOP signal raised by the harness.
- **Surface full-fidelity STOP details.** When `stop_condition` is
  non-null, the harness adapter MUST emit the exact closed-enum value
  (e.g. `payload-sha-mismatch`), the `next_operator_command: "STOP —
  escalate"` literal, the full `evidence_paths` list, and the full
  `audit_markers` list in its reporting output. STOP details are not
  summarized away, redacted, or attenuated.
- **Honor `EXIT_STATUS_REFUSED == 12`.** When the status subprocess
  exits with code 12, the harness adapter MUST emit the refusal
  envelope verbatim (including its `stop_condition` and the
  `a2-l2d-status-refused` marker) and MUST classify the harness run
  as a STOP, not as a pass. Treating exit 12 as a transient failure
  to be retried is forbidden.
- **Assert idempotency on demand.** The harness adapter MAY invoke
  the status subprocess twice against the same workspace and assert
  the two stdout captures are byte-identical, exercising the A2-L2d
  idempotency invariant. The harness MUST NOT use cached stdout from
  a prior run as the second sample; both reads must be independent
  subprocess invocations.
- **Emit structured observability output.** The harness adapter MUST
  emit a pass/fail result, an assertion summary, the parsed envelope,
  the raw stdout capture (for byte-identical-verification consumers),
  the exit code, and the diagnostic message at full envelope
  fidelity. Output may be JSON, NDJSON, or another structured format
  the harness implementation lane chooses; it is not free-form text.
- **Pass through unchanged.** When the harness adapter has parsed and
  asserted an envelope, it MUST emit the envelope contents at
  granularities no coarser than the envelope itself carries. If the
  envelope distinguishes nine `phase` values, the harness emits nine
  distinct values (or the literal value observed); it does not
  collapse them.

## 6. Harness Adapter Non-Responsibilities

These are the things the harness adapter exists *to not do*. Each
maps to a named safety property in A2-L2b, A2-L2c, A2-L2d, or the
A2-L3 adapter boundary card.

- **Approval is not a harness responsibility.** The harness adapter
  MUST NOT call `claw plan approve`, MUST NOT compose the approval
  line `apply <step_id> <preview_sha256>`, MUST NOT generate an
  `<approval-result.json>` artifact, and MUST NOT feed any approval
  payload to any process other than the operator who is reading the
  harness report.
- **Apply-bundle generation is not a harness responsibility.** The
  harness adapter MUST NOT call `claw plan apply-bundle`, MUST NOT
  construct an apply bundle by hand, and MUST NOT package the inputs
  to apply-bundle generation as a single harness action.
- **Apply is not a harness responsibility.** The harness adapter MUST
  NOT call `claw plan apply` and MUST NOT compose apply with any
  preceding step. `phase == apply_bundle_ready` is a *read*, not a
  cue for the harness to act.
- **Retry on STOP is not a harness responsibility.** The harness
  adapter MUST NOT re-invoke any canonical chain command after a STOP
  signal. It MAY re-invoke `claw plan status` itself (one re-read per
  caller-initiated assertion cycle) but MUST NOT chain a status re-
  read into any write step.
- **Rollback is not a harness responsibility.** `phase == rolled_back`
  is a read-only diagnosis. The harness adapter MUST NOT initiate, pre-
  populate, or compose a rollback-adjacent command. There is no
  rollback affordance on the harness surface.
- **Remediation is not a harness responsibility.** The harness adapter
  MUST NOT hand-edit `.claw/**` artifacts, MUST NOT regenerate
  bundles, and MUST NOT invoke remediation scripts in response to a
  STOP signal. STOP escalation is human-driven.
- **Artifact mutation is not a harness responsibility.** The harness
  adapter MUST NOT write, rename, delete, copy, or move any file
  under `.claw/`, the workspace tree, the operator's home directory,
  or anywhere else outside the harness's own reporting output
  directory.
- **STOP attenuation is not a harness responsibility.** The harness
  adapter MUST NOT hide, debounce, collapse, summarize away, redact,
  or rate-limit STOP signals in any output it emits.
- **State invention is not a harness responsibility.** The harness
  adapter MUST NOT compute, infer, or report chain state that the
  envelope does not carry. Synthetic harness-only phases (e.g.
  `harness-says-ready`, `flaky-stop`, `pre-apply`) are forbidden.
- **CLI extension is not a harness responsibility.** The harness
  adapter MUST NOT depend on new CLI commands, flags, schema
  versions, exit codes, or markers in order to function. Any contract
  gap the harness needs is escalated as a separate scope-card lane
  against A2-L2d, not worked around inside the harness.
- **Cross-run aggregation is not a harness responsibility at this
  scope.** The harness adapter MAY assert on a single workspace's
  envelope per assertion cycle. Multi-run inventory, cross-workspace
  dashboards, and history rollups are out of scope and would require
  a separate scope-card lane that explicitly defines whether such
  aggregation is a contract extension or an adapter-side concern.

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
- caller-supplied input describing the workspace, the optional
  approval-result path, expected assertion values, and the cadence
  of any caller-driven repeat invocation.
- the harness's own configuration file, if any, supplied at the
  harness implementation lane's scope.

The harness adapter may **not** read:

- any `.claw/**` file directly; read is mediated exclusively through
  the status command's stdout.
- the workspace tree directly. If a STOP requires inspecting an
  evidence-path file's contents, that read is the operator's, not
  the harness's.
- broker endpoints, model endpoints, Ollama endpoints, or any HTTP
  surface.
- secrets, environment variables (other than the ones it sets for
  itself per §15), the operator's shell history, the operator's
  terminal state, or any non-workspace file beyond what `claw plan
  status` already reads.
- any on-disk cache of previously-seen envelopes the harness itself
  produced; the harness MUST NOT introduce an on-disk envelope cache.

The harness adapter does **not** subscribe to filesystem watchers,
git event streams, daemon push channels, or any notification surface
that would let it act without an explicit caller- or schedule-
initiated read.

## 8. Forbidden Actions

The future harness adapter implementation lane is forbidden from
performing any of the following. Each forbidden action maps to a
named safety property the chain depends on.

- spawning `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, or `claw plan apply` as a subprocess for any
  reason — including diagnostic, "dry-run", or "shadow" reasons.
- spawning any process other than the read-only `claw plan status`
  command (with no flags, with at most the two positional arguments
  the A2-L2d schema defines).
- pre-filling, auto-completing, generating, hashing, signing, or
  otherwise producing the TTY approval line `apply <step_id>
  <preview_sha256>` in any channel — visible or hidden.
- producing or persisting `<approval-result.json>` on the operator's
  behalf, anywhere on disk.
- producing or persisting `apply-bundle.json` on the operator's
  behalf, anywhere on disk.
- modifying any file under `.claw/`, the workspace tree, the
  operator's home directory, the harness's own configuration
  directory (beyond its own append-only report output), or anywhere
  else, except to emit the harness's own reporting artifacts.
- composing `approve` and `apply` into a single harness action,
  whether by chained subprocess, scripted shortcut, plugin, recorded
  action, or any other mechanism.
- offering a "one-click", "fast-path", "express", "skip", or "trust"
  mode that elides any operator gesture the canonical chain requires.
- adding write-adjacent flags or environment variables to the
  harness surface that map onto refused CLI flags (`--apply`,
  `--approve`, `--yes`, `--auto`, `--clean`, `--rollback`,
  `--mutate`, `--all-runs`, `--no-prompt`, `--skip-approval`,
  `--cache`); harness affordances that compose to the same semantic
  effect as those flags are equally forbidden.
- calling broker, model, Ollama, telemetry, analytics, error-
  reporting, or any other network endpoint at any phase of harness
  operation.
- caching envelope contents on disk as authoritative state across
  runs (the harness's reporting output is append-only diagnostic
  material, not a source-of-truth for chain state).
- watching the filesystem for `.claw/**` changes and refreshing
  without explicit caller- or schedule-initiated invocation.
- summarizing, collapsing, debouncing, rate-limiting, or otherwise
  attenuating any STOP signal carried by the envelope (see §11).
- introducing parallel status schemas, parallel envelope versions,
  or "extended" envelopes that wrap `a2-l2d-status.v1` with harness-
  specific fields and re-emit them as authoritative.
- normalizing an unknown enum value into a known one (e.g. coercing
  an unknown `stop_condition` to `null`, or coercing an unknown
  `phase` to `unknown` instead of surfacing the unknown literal).
- treating the envelope as authoritative for write decisions; the
  chain re-validates every input at apply time
  ([`a2-l2b-run-plan-preview-operator-handoff.md` §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
  and the harness MUST NOT pretend otherwise.

## 9. Input Contract

The future harness adapter may accept the following inputs from its
caller. Each input is read-only with respect to chain state.

- **Workspace root** (required). An absolute or relative path to the
  workspace that `claw plan status` should be invoked against. The
  harness adapter MUST forward this path to the subprocess verbatim;
  it MUST NOT canonicalize, expand, or substitute the path.
- **Optional approval-result path**. When supplied, passed as the
  second positional argument to `claw plan status`. The harness MUST
  NOT synthesize this file's contents and MUST NOT modify the file
  the caller supplied.
- **Expected `phase`** (optional). When supplied, the harness asserts
  the observed `phase` equals the expected value and reports the
  result. If absent, the harness emits the observed `phase` without
  assertion.
- **Expected `stop_condition`** (optional, nullable). When supplied,
  the harness asserts the observed `stop_condition` equals the
  expected value (including `null` when the caller expects no STOP).
  If absent, the harness emits the observed `stop_condition` without
  assertion.
- **Expected `read_only_invariant`** (optional). Defaults to the
  literal `"this command does not mutate state"`. The harness asserts
  the observed invariant equals the expected literal regardless of
  whether the caller supplied it explicitly; supplying a different
  expected value is a misuse the harness implementation lane MUST
  refuse.
- **Expected evidence-path patterns** (optional). When supplied,
  patterns are matched against the observed `evidence_paths` array.
  Match semantics (exact, glob, regex) are deferred to the harness
  implementation scope card; whatever semantics are chosen, they MUST
  NOT permit a STOP-relevant evidence path to be matched away by a
  permissive pattern.
- **Repeat-invocation policy** (optional). When supplied, names the
  number and cadence of caller-initiated re-invocations of `claw
  plan status` for the same assertion cycle (e.g. for idempotency
  assertions). The harness MUST NOT implicitly repeat invocations the
  caller did not request.

The harness adapter MUST NOT accept any input that would direct it to
invoke `claw plan run`, `claw plan approve`, `claw plan apply-bundle`,
`claw plan apply`, or any other write-adjacent command. Such inputs
are a category violation and the harness implementation lane MUST
refuse them at parse time, not at invocation time.

## 10. Output / Reporting Contract

The future harness adapter emits the following reporting output for
each invocation cycle. All fields are at full envelope fidelity; the
harness MUST NOT redact, summarize, or compress any STOP-relevant
content.

- **Pass/fail result.** A single classification reflecting whether
  every caller-declared assertion passed. STOP signals observed
  (whether or not the caller expected them) classify the cycle
  according to the caller's expectation: an unexpected STOP fails the
  cycle; an expected STOP passes the cycle. A STOP is never silently
  classified as a pass.
- **Parsed `a2-l2d-status.v1` envelope.** The full parsed envelope,
  emitted as JSON in the harness output. Every field of the envelope
  is preserved, including SHA fields, `audit_markers`, and
  `read_only_invariant`.
- **Raw stdout capture.** The byte string the status subprocess
  emitted to stdout, preserved exactly. This supports byte-identical
  idempotency assertions across paired invocations.
- **Exit code.** The integer exit code returned by the status
  subprocess.
- **Assertion summary.** A list of per-assertion entries naming the
  assertion (e.g. `phase == awaiting_approval`), the expected value,
  the observed value, and the per-assertion pass/fail flag.
- **Full-fidelity `stop_condition`.** When non-null, emitted as the
  exact closed-enum value with no rewording.
- **Full-fidelity `evidence_paths`.** Emitted as the exact list the
  envelope carried, in the order it was received, with no
  deduplication or trimming beyond what the envelope itself supplied.
- **Full-fidelity `audit_markers`.** Emitted as the exact list the
  envelope carried, with no marker filtering or renaming.
- **Diagnostic message.** A short human-readable summary of the
  pass/fail outcome. The diagnostic message is supplementary; it does
  NOT replace the structured fields above and the implementation lane
  MUST NOT permit a harness consumer to rely on the diagnostic alone.

The harness adapter MUST NOT emit any output that would imply
adapter authority over the chain. Output framing such as "harness
applied step", "harness approved preview", "harness retried after
STOP" is a category violation; "harness observed status", "harness
asserted phase", "harness reported STOP" is accurate framing.

## 11. STOP Condition Handling

The A2-L2b chain's safety is reasoned about in terms of STOP gates
that the operator must observe and escalate on
([`a2-l2b-run-plan-preview-operator-handoff.md` §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)).
A2-L2d surfaces those STOPs through the `stop_condition` enum, the
`next_operator_command: "STOP — escalate"` literal, the `phase`
values `non_approvable` / `rolled_back` / `unknown`, and the
`a2-l2d-status-stop-condition-detected` / `a2-l2d-status-refused`
markers. The A2-L3 adapter boundary card §11 pinned the STOP-
visibility rules for any adapter.

For the harness adapter specifically:

- **Verbatim STOP-value emission.** When `stop_condition` is
  non-null, the harness MUST emit the exact closed-enum value
  (e.g. `payload-sha-mismatch`, `live-target-missing`). Substituting
  human-friendly text is forbidden; the enum value IS the operator
  escalation signal and the harness preserves it.
- **STOP prominence parity in logs and metrics.** A STOP signal MUST
  receive at least the same structured-output prominence as a non-
  STOP signal. A harness that emits `phase` as a top-level metric
  but emits `stop_condition` as a nested debug field is a category
  violation.
- **No STOP debouncing.** If two successive caller-initiated reads
  produce the same STOP, the harness emits the STOP both times. The
  harness MUST NOT collapse "same STOP twice in a row" into a single
  reported event.
- **No STOP rate-limiting.** The harness MUST NOT throttle STOP
  emission. Every observed envelope with a non-null `stop_condition`
  is a STOP event in the harness output.
- **No suppression of STOP into warning.** The harness MUST NOT
  re-classify a STOP signal as a "warning", a "soft failure", a
  "skipped", or any other lower-severity classification. STOP is
  STOP; the harness's role is to surface it and to fail the cycle if
  the caller expected continuation.
- **No automatic resolution of STOP.** The harness MUST NOT re-invoke
  any chain command, edit any artifact, or pre-fill any approval
  payload in response to a STOP. STOP-resolution is human-driven and
  off the harness surface.
- **STOP-on-unknown.** Any unknown enum value (`phase`, `stop_
  condition`, `next_operator_command`, marker) MUST be treated as a
  STOP signal in the harness output and classified as a failure of
  any caller expectation that the cycle continue. Coercing an unknown
  value into a known one — including coercing an unknown
  `stop_condition` to `null` (an "unknown ok" normalization) — is a
  category violation.
- **STOP retention across repeated reads.** If the caller asks the
  harness to repeat-invoke `claw plan status` against the same
  workspace, and the first invocation observes a STOP while the
  second observes a non-STOP envelope, the harness MUST emit both
  observations distinctly. The STOP is NOT cleared from the report
  by a subsequent non-STOP observation; the report shows both.
- **STOP in emitted logs/metrics.** When the harness emits envelope
  contents to logs or metrics, `stop_condition`, `evidence_paths`,
  and `audit_markers` MUST be emitted at full fidelity. Field
  redaction for these fields is forbidden, including in "production"
  log levels where lower-fidelity output is otherwise acceptable.

## 12. Idempotency And Repeatability

A2-L2d guarantees that two successive `claw plan status` invocations
against an unchanged workspace produce byte-identical stdout
([`a2-l2d-status-schema.md` §10](./a2-l2d-status-schema.md#10-idempotency-rules)).
The harness adapter relies on this for repeatability assertions, but
the harness MUST NOT exploit it to skip work:

- **Independent invocations only.** When the caller asks for an
  idempotency assertion, the harness MUST invoke `claw plan status`
  twice as independent subprocesses. Returning the same cached
  stdout twice satisfies neither the assertion nor the underlying
  invariant.
- **Byte-identical comparison.** The harness MUST compare raw stdout
  bytes for equality. Comparing parsed envelopes for structural
  equality is not equivalent; the A2-L2d contract pins field order
  and whitespace for the producer's idempotency, so a structural-
  only comparison would mask a producer regression.
- **Repeatability is not memoization.** The harness MAY cache a
  parsed envelope in memory for the duration of a single assertion
  cycle. It MUST NOT cache envelopes across cycles as authoritative
  state, and it MUST NOT persist any envelope cache to disk.
- **Idempotency assertion failures are STOP signals.** If the harness
  observes non-byte-identical stdout across an idempotency pair, the
  harness MUST classify the cycle as a STOP signal in its own right
  and emit both raw stdout captures at full fidelity. The harness
  MUST NOT silently retry the pair, MUST NOT drop the first sample
  to "stabilize" the second, and MUST NOT recover by majority voting
  across additional samples.

## 13. CI / Test-Harness Boundary

A harness adapter is most commonly deployed inside CI, test
infrastructure, or scripted observability pipelines. The boundary
between the harness adapter's responsibilities and CI's
responsibilities is:

- **The harness asserts; CI orchestrates.** The harness emits a
  pass/fail result and structured detail. CI decides what to do
  with that result (fail the build, gate a downstream step, page an
  operator). CI is not the harness adapter, and the harness adapter
  is not CI; this card defines the harness, not the CI step calling
  it.
- **The harness fails loud; CI fails the build.** When the harness
  reports failure, CI MUST surface that failure to a human. CI MUST
  NOT translate a harness STOP report into a transient-flake retry,
  a "soft fail", or any other classification that allows downstream
  steps to proceed as if the chain were healthy. (The harness
  implementation lane will document this expectation; the boundary
  between harness output and CI consumption remains the operator's
  responsibility to verify.)
- **The harness never triggers chain writes from CI.** A CI step
  that runs the harness adapter MUST NOT compose, in the same
  pipeline, a step that invokes `claw plan approve`, `claw plan
  apply-bundle`, or `claw plan apply`. This is a property the
  operator enforces in their CI configuration; the harness adapter
  itself does not invoke those commands, but a misuse of CI would
  combine them. The harness's documentation MUST explicitly call out
  this misuse pattern so reviewers can refuse pipelines that exhibit
  it.
- **No harness-driven gating of chain writes.** A future feature in
  which the harness adapter "gates" a chain-write step by emitting a
  signal that a downstream CI step consumes — and that downstream
  step then invokes `claw plan approve` or `claw plan apply` — is
  out of scope for this card. The chain's safety derives from
  operator-driven approval and explicit operator-driven apply; a CI
  pipeline that automates either is a category violation and would
  re-introduce a write path the A2-L2b chain forbids being executed
  without operator approval.

## 14. Disposable Workspace Requirement

The harness adapter MUST be operated against **disposable
workspaces** by default. A disposable workspace is one where:

- The workspace contents are owned by the harness operator (or the
  CI runner), not by a production repository.
- The `.claw/**` artifact tree is generated by the harness setup or
  by a fixture, not by a long-lived operator session whose
  observation would be disturbed by harness reads.
- Any preview, approval-result, or apply-bundle artifacts present in
  the workspace are test fixtures or freshly-generated artifacts; they
  are not authoritative operator artifacts whose semantic meaning the
  harness reading could perturb (note: `claw plan status` is read-
  only, so the artifacts are not perturbed in fact; the requirement
  is about *operational hygiene*, not about implementation
  correctness).

The harness adapter MUST NOT be operated against a non-disposable
production workspace by default. If a future harness deployment
requires running against a non-disposable workspace (e.g. for a
read-only observability check on a real chain state), that deployment
requires its own explicitly-authorized scope card that pins:

- the exact non-disposable workspace path
- the exact cadence of harness invocations
- the exact set of envelopes the harness is expected to observe
- the exact escalation path when the harness observes an unexpected
  envelope

The harness implementation lane MUST surface a configuration check
that classifies the workspace as disposable or non-disposable and
MUST refuse non-disposable invocations unless the deployer has
provided the per-deployment scope card reference. The check details
are deferred to the harness implementation scope card; the
requirement is pinned here.

## 15. Safety Invariants

The harness adapter implementation lane must preserve, verbatim,
every property the prior lanes pinned. These are the same invariants
the A2-L3 adapter boundary card §12 pins, restated here for the
harness surface:

- preview before approval
- TTY/operator approval enforcement
  ([`a2-l2c-operator-quickref.md` §3](./a2-l2c-operator-quickref.md#3-tty-approval-eof-note))
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
  ([`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants))
- A2-L2d network-egress-free invariant
- A2-L2d idempotency invariant
- A2-L2d non-overlapping marker invariant (`a2-l2d-*` only; the
  harness adapter MUST NOT invent `a2-l3-*` markers that leak back
  into the status producer)
- A2-L2d non-overlapping exit-code invariant
- A2-L3 adapter read-only invariant: the harness mutates no file and
  generates no network egress beyond the `claw plan status`
  subprocess it spawns
- A2-L3 adapter STOP-visibility invariant: every STOP signal in an
  envelope reaches the operator at the granularity the envelope
  carries (§11)
- A2-L3 adapter no-write-surface invariant: no harness input,
  output, configuration, or composition produces a write action
  against the A2-L2b chain
- A2-L3 adapter no-state-invention invariant: the harness reports
  the envelope it received; it does not synthesize chain state
- A2-L3 adapter no-shadow-contract invariant: the harness consumes
  `a2-l2d-status.v1` as-is; it does not wrap it, extend it, or
  parallel-version it

In addition, the harness adapter adds these surface-specific
invariants:

- **Harness subprocess-bounded invariant.** The only subprocess the
  harness adapter spawns is `claw plan status` with at most the two
  positional arguments the A2-L2d schema defines, with no flags, with
  network-egress sentinels set per A2-L2d §11.
- **Harness output-bounded invariant.** The harness emits to its own
  configured report destination (stdout, NDJSON file, structured log
  sink) and does not write under `.claw/**` or the workspace tree.
- **Harness STOP-loud invariant.** Every harness output classifying
  a STOP MUST be at least as prominent as the corresponding non-STOP
  classification; STOP is never demoted to a lower output severity.
- **Harness disposable-default invariant.** The harness refuses to
  operate against a non-disposable workspace unless an explicit per-
  deployment scope card authorizes it (§14).

## 16. Non-Goals

The harness adapter at this scope must not:

- implement the harness adapter (deferred; this lane is docs-only)
- implement an IDE adapter (out of scope; separate per-adapter scope
  card lane)
- introduce or imply autonomous workspace-write execution
- introduce harness controls that approve, that apply, or that
  compose approval-and-apply into a single gesture
- introduce harness-driven retry of any A2-L2b chain command
- introduce harness-driven remediation of any `.claw/**` artifact
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  pre-approval, batch approval, or any approval-bypass affordance on
  the harness surface
- introduce a "fast-mode", "shadow-mode", "what-if", or "dry-run"
  mode that simulates downstream chain commands without invoking them
- modify `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, `claw plan apply`, or `claw plan status` behavior,
  exit codes, schemas, markers, or JSON field shapes
- modify `a2-l2b-*` or `a2-l2d-status.v1` schema versions or marker
  constants
- introduce an `a2-l3-*` schema, marker, exit code, or CLI surface
  (the contract the harness adapter consumes IS `a2-l2d-status.v1`;
  the harness introduces no parallel contract)
- call broker, model, or Ollama at any phase
- introduce filesystem watchers, daemon push channels, or background
  refresh of `.claw/**`
- introduce on-disk caches of envelope contents as authoritative
  state
- introduce cross-run inventory, cross-workspace dashboards, or
  history rollups
- introduce a harness assertion library that "remediates" STOP
  signals by re-running chain commands
- weaken any A2-L2b, A2-L2c, A2-L2d, or A2-L3 STOP gate

Any of the above must be opened as a separate, explicitly-
authorized lane.

## 17. Future Implementation Constraints

When the harness adapter implementation lane is opened as a separate
scope card, it must hold to all of the following. This card pins the
boundary; the implementation scope card pins the concrete touched
surfaces and validation matrix.

- **Allowed Future Touched Surfaces** must be explicitly enumerated
  in the implementation scope card before any code or wrapper is
  authored. The implementation lane MUST NOT touch any file outside
  that enumerated list. Likely surfaces include a new harness crate
  or harness module under an A2-L3-named path within
  `rust/crates/` (specific name deferred), new tests under that
  crate's `tests/`, and new documentation under `docs/`. Concrete
  paths are deferred to the implementation scope card.
- **Forbidden Surfaces** must explicitly include every A2-L2b
  module
  (`rust/crates/a2-plan-runner/src/{approval,approval_ux,
  checkpoint,diff_preview,preflight,report,runner,write_executor,
  write_payload,write_preview,write_runtime,markers}.rs`), every
  A2-L2b schema constant, every A2-L2b exit-code constant, every
  A2-L2b/`a2-l2d-*` marker constant, and
  `rust/crates/a2-plan-runner/src/status.rs` (the harness consumes
  the status command's stdout; it does not modify the producer). The
  IDE-adapter-only surfaces, when the IDE adapter scope card is
  authored, will be additional forbidden surfaces for the harness
  implementation.
- **Validation** must include:
  - forbidden-language sniff against the staged diff for the same
    regex family as the A2-L2b, A2-L2c, A2-L2d, and A2-L3 adapter
    boundary cards;
  - tests asserting the harness spawns no subprocess other than
    `claw plan status` with at most the two A2-L2d positional
    arguments;
  - tests asserting the harness performs zero filesystem writes
    under any input outside its own configured report destination;
  - tests asserting the harness performs zero network egress
    beyond the status subprocess;
  - tests asserting the harness emits every STOP signal at the
    granularity §11 requires;
  - tests asserting the harness classifies an unexpected STOP as a
    failed cycle and an expected STOP as a passed cycle, with no
    silent reclassification path;
  - tests asserting the harness refuses inputs that would direct
    it to invoke any chain-write command;
  - regression tests confirming every existing
    A2-L2b/L2c/L2d/L3-boundary test still passes unchanged.
- **STOP-rendering coverage** must include golden-file tests for:
  - every closed `stop_condition` value
    ([`a2-l2d-status-schema.md` §6](./a2-l2d-status-schema.md#6-closed-stop_condition-enum));
  - every closed `phase` value
    ([`a2-l2d-status-schema.md` §4](./a2-l2d-status-schema.md#4-closed-phase-enum));
  - the refusal envelope (exit `EXIT_STATUS_REFUSED == 12`);
  - at least one unknown-enum-value synthetic fixture per closed
    enum (`phase`, `stop_condition`, `next_operator_command`,
    marker);
  - an idempotency-mismatch fixture (two successive reads producing
    non-byte-identical stdout) and the harness's STOP-classified
    response to it.
- **Caller-driven invocation only.** The implementation scope card
  must explicitly forbid background polling, filesystem watchers,
  daemon channels, and implicit harness-initiated re-invocations.
  Every status read is caller-initiated or schedule-initiated by an
  external caller (e.g. CI), never harness-initiated for its own
  reasons.
- **Disposable-workspace check.** The implementation scope card must
  define the runtime check that classifies the configured workspace
  as disposable or non-disposable (§14), and must define the refusal
  path when a non-disposable workspace is configured without an
  explicit per-deployment scope card reference.

## 18. Definition Of Done

This **scope card** is done when:

- `docs/a2-l3-harness-adapter-scope-card.md` exists and matches the
  sectional structure of this card.
- The card defines harness adapter responsibilities and non-
  responsibilities in non-softening language.
- The card pins the input contract, output/reporting contract,
  STOP-handling rules, idempotency rules, CI boundary, and
  disposable-workspace requirement.
- The card pins the safety invariants without escape hatches.
- The card declares the A2-L3 harness adapter as docs-only at this
  scope-card stage.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, A2-L2d, or A2-L3 STOP gate is weakened.
- A single cross-link line MAY be added to the A2-L3 adapter
  boundary scope card, the A2-L2d scope card, the A2-L2d status
  schema, or the A2-L2d operator quick reference if an obvious
  location exists, but no such cross-link is required for this
  scope card itself to land. *(This scope card is authored without
  cross-links to keep the lane strictly limited to a single new
  docs file; cross-links may be added in a follow-up lane.)*
- The card is reviewed by the operator before any harness adapter
  implementation lane is opened.

The harness adapter **implementation lane** is out of scope for this
card. Definition of done for that lane will be authored when its own
scope card is created, bounded by the constraints in §§5–17 above.

## 19. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **Harness adapter implementation scope-card lane (docs-only)** —
> author a concrete scope card for the harness adapter that
> enumerates its allowed touched surfaces (likely a new harness crate
> or module under `rust/crates/`, tests, and one new docs file), its
> forbidden surfaces (per §17), its concrete validation plan, its
> STOP-rendering golden-test matrix, its disposable-workspace check
> design, and its definition of done — all bounded by both
> [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
> and this card. Do not author the harness implementation in the same
> lane as its implementation scope card.

The lane *after* the harness implementation scope card lands is:

> **The harness adapter implementation lane** — implement the
> harness adapter under the constraints pinned by this card and that
> adapter's implementation scope card, with golden tests for every
> STOP signal, every phase, the refusal envelope, every unknown-enum
> fixture, and idempotency-mismatch behavior. The implementation
> lane MUST NOT expand the contract; any contract gap discovered
> during implementation is escalated as a separate scope-card lane
> against A2-L2d.

Neither lane permits autonomous workspace-write execution. Both
remain bounded by the A2-L2b, A2-L2c, A2-L2d, and A2-L3 safety
properties and by §§5–17 of this card.

## 20. References

- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 adapter boundary scope card; the parent card this
  per-adapter card refines for the harness surface. Section 9 of
  that card pinned the harness-boundary preamble this scope card
  expands.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card; section 10 ("IDE / Harness Boundary") is the
  upstream preamble that A2-L3 expanded.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the
  contract the harness consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`. The
  at-the-keyboard companion to the contract the harness consumes.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference; TTY approval EOF note in §3 is
  load-bearing for the approval boundary the harness must never
  compose around.
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
- IDE adapter scope card authored: **no** (separate future per-
  adapter lane).
- Autonomous-write authorization: **no**.
- Approval / apply boundary weakened: **no**.
- A2-L2b / A2-L2c / A2-L2d / A2-L3-boundary STOP gate weakened:
  **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- A2-L3 adapter boundary card (`a2-l3-adapter-boundary-scope-card
  .md`) modified: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by a harness-adapter implementation scope-card
  lane.
