# A2-L3 Harness Adapter — Usage Guide

This document is the operator/developer companion to the merged A2-L3
Harness Adapter crate at
[`rust/crates/a2-harness-adapter/`](../rust/crates/a2-harness-adapter/).
It explains what the adapter is, how to configure it, how disposable-
workspace classification works, how STOP signals reach the operator,
how CI should consume it, and — explicitly — what the adapter is not
allowed to do.

It is **documentation only**. It introduces no new CLI command,
subcommand, flag, exit code, marker, schema, or JSON field. It does
not change any runtime behavior and it does not weaken any A2-L2b,
A2-L2c, A2-L2d, A2-L3 adapter boundary, or A2-L3 harness adapter STOP
gate.

For the bounding scope cards, see
[`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md),
[`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md),
and
[`a2-l3-harness-adapter-implementation-scope-card.md`](./a2-l3-harness-adapter-implementation-scope-card.md).
For the contract the adapter consumes, see
[`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md).

## 1. Purpose

The A2-L3 harness adapter is a **read-only assertion and reporting
layer** over the shipped A2-L2d `claw plan status` command and its
`a2-l2d-status.v1` envelope. It is intended for use by:

- test harnesses that need to assert chain state programmatically
- scripted observability tools that emit envelope contents to logs or
  metrics
- CI steps that gate downstream operator actions based on observed
  chain state
- operator scripts that want a structured, parsed view of where a
  workspace is in the A2-L2b preview-to-apply chain

The adapter is **not** a workflow controller. It does not invoke
`claw plan run`, `claw plan approve`, `claw plan apply-bundle`, or
`claw plan apply`. It does not compose any sequence of those into a
single action. It does not pre-fill the TTY approval line. It does
not generate `<approval-result.json>` or `apply-bundle.json`. It
does not retry, roll back, clean, or remediate on STOP signals. Each
canonical chain command remains separately operator-invoked under the
unmodified A2-L2b safety properties.

## 2. What It Consumes

The adapter sources chain state exclusively from one subprocess:

```text
claw plan status <workspace> [<approval-result.json>]
```

No flags. The two positional arguments mirror the A2-L2d schema-of-
record verbatim: `<workspace>` is required; `<approval-result.json>`
is the only permitted read outside `<workspace>/.claw/**`. See
[`a2-l2d-status-schema.md` §2](./a2-l2d-status-schema.md#2-canonical-invocation)
for the canonical invocation rules.

The subprocess's stdout is parsed as the
[`a2-l2d-status.v1`](./a2-l2d-status-schema.md#3-output-envelope)
envelope. The adapter validates:

- `schema_version` literal exactly equals `a2-l2d-status.v1`
- `read_only_invariant` literal exactly equals
  `"this command does not mutate state"`
- every closed-enum value (`phase`, `stop_condition`,
  `next_operator_command`, `audit_markers`) is one of the schema-
  pinned values

Any drift on these properties is itself a STOP signal in the
harness's report (see §4). The adapter does **not** parse any other
surface, does **not** shortcut to `.claw/**` artifacts, and does
**not** derive chain state from any source other than `claw plan
status` stdout and exit code.

The production subprocess invoker sets three network-sentinel
environment variables on the spawned process to unreachable values:

- `HTTP_PROXY`
- `HTTPS_PROXY`
- `OLLAMA_HOST`

This mirrors the A2-L2d producer's own network-egress-free invariants
([`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants))
and makes the no-network property visible to operators inspecting
the subprocess's environment.

## 3. Disposable Workspace Requirement

The harness adapter must operate against **disposable workspaces** by
default. The disposable classifier enforces **AND-semantics over four
required signals**; missing any signal classifies the workspace as
non-disposable and refuses subprocess invocation unless the caller
supplies a per-deployment authorisation doc reference (recorded
verbatim, never parsed).

The four AND-signals are:

1. **Path-prefix allowlist.** The workspace path must be under at
   least one caller-configured root in
   `ClassifierConfig::disposable_path_prefixes`. The default
   allowlist is empty; callers MUST configure at least one prefix
   (e.g. the system tempdir, a CI runner workdir, a per-test
   tempdir).
2. **Marker file.** The workspace must contain a marker file at the
   pinned relative path `.claw/harness-disposable.marker`. The
   classifier reads only metadata of this file; the file's contents
   are not parsed.
3. **Owner UID match.** The workspace root's `fs::metadata().uid()`
   (on Unix) must equal `ClassifierConfig::expected_owner_uid`. On
   non-Unix platforms the signal is treated as missing; non-Unix
   callers must supply a per-deployment authorisation doc reference.
4. **Explicit caller declaration.**
   `ClassifierConfig::caller_declared_disposable` must be `true`.
   This signal alone is **insufficient**; the classifier requires
   the other three signals in addition.

The classifier emits one of three decisions:

- `Disposable { signals }` — all four signals passed.
- `NonDisposableButAuthorizedBy { signals, authorization_doc }` —
  one or more AND-signals failed but the caller supplied a per-
  deployment authorisation doc reference. The reference is recorded
  verbatim in the harness report; the classifier never parses the
  doc and never treats it as permission to mutate.
- `NonDisposableAndRefused { signals }` — one or more AND-signals
  failed and no authorisation doc was supplied. The harness refuses
  to invoke the subprocess and emits a STOP signal.

Forbidden classifier behaviours (enforced by tests):

- The classifier never silently defaults to `Disposable` when a
  signal is missing.
- The classifier never accepts caller declaration alone.
- The classifier never reclassifies a workspace mid-cycle.
- The classifier never writes any file as a side effect.

See
[`rust/crates/a2-harness-adapter/tests/disposable_classifier.rs`](../rust/crates/a2-harness-adapter/tests/disposable_classifier.rs)
for the AND-semantics test matrix.

## 4. STOP Signals

The harness emits STOP signals whenever the producer raises one
through the envelope **or** whenever the harness itself detects a
drift, an idempotency mismatch, a refused config, a classifier
refusal, or a caller-expectation mismatch. STOP signals are reported
verbatim — the harness never debounces, summarises, downgrades, or
rate-limits them.

The STOP signal kinds emitted by the harness are:

- **Producer STOP signals** — the envelope itself surfaced a STOP:
  - `ProducerStopCondition(<stop_condition>)` — non-null
    `stop_condition` value (one of the 11 closed values in
    [`a2-l2d-status-schema.md` §6](./a2-l2d-status-schema.md#6-closed-stop_condition-enum))
  - `StopBearingPhase(<phase>)` — `phase` is `non_approvable`,
    `rolled_back`, or `unknown`
  - `ProducerStopEscalate` — `next_operator_command` literal is
    `STOP — escalate`
  - `ProducerRefused` — subprocess exited with
    `EXIT_STATUS_REFUSED == 12`
- **Schema-drift STOP signals** — the producer emitted something
  outside the closed contract:
  - `SchemaVersionMismatch(<observed>)` — first-field literal not
    `a2-l2d-status.v1`
  - `ReadOnlyInvariantAltered(<observed>)` —
    `read_only_invariant` literal absent or different
  - `InvalidJson(<error>)` — stdout did not parse as JSON
  - `SchemaDrift(<error>)` — JSON parsed but envelope structure or
    enum values did not match `a2-l2d-status.v1` (unknown `phase`,
    unknown `stop_condition`, missing required field)
  - `UnknownEnumLiteral { field, value }` — closed-enum value not in
    the schema (e.g. unknown `next_operator_command` shape, unknown
    `audit_markers` member)
- **Harness-detected STOP signals**:
  - `IdempotencyMismatch` — two paired status invocations against
    the same workspace produced non-byte-identical stdout
  - `ConfigReferencedChainWriteCommand(<offending-substring>)` —
    caller supplied a config string referencing one of the chain-
    write subcommands; refused at parse time before any subprocess
    is spawned
  - `NonDisposableWorkspaceRefused(<workspace-path>)` — classifier
    refused the workspace and no authorisation doc was supplied
- **Caller-expectation mismatch STOP signals**:
  - `ExpectedContinueObservedStop` — caller expected the chain to
    continue but a STOP was observed
  - `ExpectedStopObservedContinue` — caller expected a STOP but the
    chain continued
  - `WrongStopValue { expected, observed }` — caller expected a
    specific `stop_condition` but a different one was observed

For every STOP, the harness preserves the verbatim literal:

- the exact `stop_condition` enum value
- the exact `next_operator_command` string
- the full `evidence_paths` array (every artifact path the producer
  read)
- the full `audit_markers` array (the `a2-l2d-*` markers the
  producer emitted)
- the raw stdout capture, exit code, and observed envelope per
  invocation

Unknown enum literals are never coerced into known values. A
schema-drift case never silently becomes a non-STOP outcome. An
unknown `stop_condition` is never normalised to `null` ("unknown
ok").

## 5. CI Consumption Pattern

CI pipelines consuming the harness adapter follow the same shape:

1. **Invoke** the harness library against a disposable workspace
   classified per §3. The classifier refuses non-disposable
   workspaces by default; CI MUST configure the path-prefix
   allowlist and the owner-uid expectation explicitly per pipeline.
2. **Parse** the harness report. The report carries the
   classification (`Pass` / `Fail` / `Stop`), the per-cycle
   diagnostic, the classifier decision, the per-invocation argv +
   raw stdout + exit code + parsed envelope, the per-assertion
   pass/fail entries, and the full STOP-signal list.
3. **Assert** caller expectations against the report. Typical CI
   patterns:
   - assert observed `phase` equals the expected step in the
     pipeline (e.g. `awaiting_approval` after a preview-generation
     step in a soak-test rig)
   - assert observed `stop_condition` is `None` when the pipeline
     expects continuation
   - assert observed `stop_condition` equals a specific closed value
     when the pipeline is exercising a STOP-rendering case
   - assert idempotency byte-identical equality when running paired
     reads
4. **Fail** the CI build when the harness reports STOP (or when a
   caller-declared assertion fails). The harness's STOP signals are
   the operator-escalation signal; CI MUST surface them at full
   fidelity to the human reviewer and MUST NOT translate a STOP
   into a transient flake, retry, or "soft fail" classification.

A CI pipeline calling the harness adapter MUST NOT, in the same
pipeline, run any step that invokes `claw plan run`, `claw plan
approve`, `claw plan apply-bundle`, or `claw plan apply` based on
the harness report. The chain's safety derives from operator-driven
approval and explicit operator-driven apply; a CI pipeline that
chains a harness read into an automated chain-write step is a
category violation and re-introduces a write path the A2-L2b chain
forbids being executed without operator approval.

The harness adapter itself enforces this property by refusing — at
config-parse time, before any subprocess is spawned — any
`HarnessAssertionConfig` whose string fields reference any of those
four chain-write subcommands.

## 6. What It Does Not Do

The harness adapter MUST NOT do any of the following. Each non-
behaviour is enforced by either type-level constraints, runtime
config refusal, or in-crate integration tests.

- invoke `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, or `claw plan apply`
- generate `<approval-result.json>` or `apply-bundle.json` on the
  operator's behalf
- compose `approve` and `apply` (or any chain-write pair) into a
  single harness action
- pre-fill, auto-complete, sign, or otherwise produce the TTY
  approval line `apply <step_id> <preview_sha256>`
- modify any file under `.claw/`, the workspace tree, the operator's
  home directory, or anywhere outside the harness's own configured
  report destination
- initiate, pre-populate, or suggest the initiation of rollback
- clean stale or rolled-back runs
- call broker, model, Ollama, telemetry, analytics, error-reporting,
  or any other network endpoint
- depend on any HTTP client crate (`reqwest`, `hyper`, `ureq`,
  `surf`, `isahc`, `awc`)
- depend on `tokio` or any other async runtime
- watch the filesystem or subscribe to a daemon push channel for
  background refresh
- introduce on-disk caches of envelope contents as authoritative
  state across runs
- silently default to disposable when classifier signals are missing
- accept caller declaration alone as sufficient for the disposable
  classifier
- accept an authorisation doc reference as permission to mutate any
  file (the reference is recorded verbatim; the harness never parses
  it and never reclassifies the workspace based on it)
- run against a non-disposable repository in the default
  configuration
- coerce, normalise, debounce, or rate-limit any STOP signal
- redact `stop_condition`, `evidence_paths`, or `audit_markers` in
  emitted logs or metrics
- modify `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, `claw plan apply`, or `claw plan status` behavior, exit
  codes, schemas, markers, or JSON field shapes
- modify the `a2-l2d-status.v1` schema, marker, exit code, or CLI
  surface
- introduce parallel status schemas, parallel envelope versions, or
  extended envelopes that wrap `a2-l2d-status.v1`

Any of the above behaviours requires a separate, explicitly-
authorised scope-card lane.

## 7. Library Surface

The crate is library-first; the public surface at
[`rust/crates/a2-harness-adapter/src/lib.rs`](../rust/crates/a2-harness-adapter/src/lib.rs)
re-exports the following modules at a high level:

- **`envelope`** — `StatusEnvelope`, `Phase`, `StopCondition`,
  `NextOpCommandShape`, `parse_envelope`,
  `classify_next_operator_command`, `EnvelopeParseError`, plus the
  pinned literals (`STATUS_SCHEMA_V1`, `READ_ONLY_INVARIANT_LITERAL`,
  `EXIT_STATUS_REFUSED`).
- **`classifier`** — `classify_workspace`, `ClassifierConfig`,
  `ClassifierSignals`, `WorkspaceClassification`,
  `DISPOSABLE_MARKER_REL_PATH`.
- **`config`** — `HarnessAssertionConfig`, `ExpectedOutcome`,
  `ConfigError`, `REPEAT_INVOCATION_CAP`.
- **`invoker`** — `StatusInvoker` trait, production
  `ClawPlanStatusInvoker` (sets the three network-sentinel env vars
  on the spawned subprocess), test-side `MockStatusInvoker`, pure
  `build_status_argv` audit function, `StatusInvocation` and
  `MockInvocationRecord` records.
- **`cycle`** — `run_cycle` entry point; `CycleError` for setup-
  failure path distinguishing.
- **`report`** — `HarnessRunReport`, `CycleClassification`,
  `AssertionEntry`, `InvocationRecord`.
- **`stop`** — `StopSignal`, `StopKind`, `phase_is_stop_bearing`.

This is a surface map, not a code tutorial. The doc comments on each
item in
[`rust/crates/a2-harness-adapter/src/`](../rust/crates/a2-harness-adapter/src/)
carry the per-item contract. The in-crate integration tests under
[`rust/crates/a2-harness-adapter/tests/`](../rust/crates/a2-harness-adapter/tests/)
are the executable specification of behaviour; treat them as the
canonical examples of how to wire up the library.

## 8. References

- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 adapter boundary (parent scope card for all per-adapter
  lanes).
- [`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md)
  — A2-L3 harness adapter behavioural scope card.
- [`a2-l3-harness-adapter-implementation-scope-card.md`](./a2-l3-harness-adapter-implementation-scope-card.md)
  — A2-L3 harness adapter implementation scope card (concrete
  touched surfaces, validation matrix).
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the contract
  the harness consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for the `claw plan status`
  command.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference for the gated chain commands the
  harness observes (but never invokes).
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- [`rust/crates/a2-harness-adapter/`](../rust/crates/a2-harness-adapter/)
  — the harness adapter crate source-of-record.
