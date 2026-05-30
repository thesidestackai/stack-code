# A2-L3 Scope Card — IDE / Harness Adapter Boundary (Docs-Only)

This document is a **design-only scope card** for the A2-L3 lane. It
describes what an IDE or harness adapter may safely consume from the
shipped A2-L2d read-only status surface, what it must never do, and
the validation required before any implementation lane is allowed to
land. This file itself authorizes **no runtime change, no CLI change,
no adapter implementation, no IDE integration, and no autonomous
workspace-write behavior**.

A2-L3 sits exactly one layer above A2-L2d in the planned progression:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, shipped)
      → IDE / harness adapter boundary (A2-L3, this scope card)
        → future adapter implementation (separate, future)
          → future IDE UI (separate, future)
```

A2-L2d closed the read-side gap by shipping `claw plan status
<workspace> [<approval-result.json>]`, the `a2-l2d-status.v1`
envelope, and the operator quick reference. A2-L3 closes the
**adapter-boundary** gap by defining, in design only, the bounded set
of behaviors a future IDE or harness adapter may exhibit when it
consumes `a2-l2d-status.v1`. It defines neither the adapter
implementation nor the IDE UI.

## 1. Executive Summary

A2-L3 defines, in design only, a **read-only observer contract** for
IDE and harness consumers of the A2-L2d status surface. The
adapter is a *display and inspection* layer over `a2-l2d-status.v1`.
It does not act on the chain, does not invent state, does not
compose `approve` and `apply`, and does not introduce a parallel
write surface. The TTY-enforced approval boundary and the
explicit-apply boundary that A2-L2b's safety derives from remain
authoritative; A2-L3 does not weaken them and does not introduce a
surface from which they could be weakened.

The recommended A2-L3 scope is:

> Define, in docs only, the bounded behavior of any future IDE or
> harness adapter that consumes `a2-l2d-status.v1`. The adapter is a
> **read-only observer**, never a **workflow controller**. It may
> render every field of the envelope verbatim, may surface
> `next_operator_command` for the operator to copy and run in their
> own terminal, and must never invoke any CLI command, mutate any
> file, suppress any STOP signal, or expose any approve/apply/retry/
> rollback affordance. The adapter implementation itself remains a
> separate, future, explicitly-authorized lane bounded by this scope
> card.

The implementation of A2-L3 is **not authorized by this scope card**.
This card defines the boundary the future implementation lane must
hold to. The next gate before implementation is operator review of
this scope card.

## 2. Why A2-L3 Exists

Stack-Code is on the IDE/harness path. The next consumer of the
A2-L2b chain — whether a terminal operator returning mid-run, a test
harness asserting on chain state, or a future IDE panel surfacing
chain state in a sidebar — will read `a2-l2d-status.v1` envelopes.
Without a defined adapter boundary, the first consumer to ship would
necessarily invent its own consumption rules, and the invention is
the moment a STOP gate can quietly leak into a "convenience"
affordance.

Two specific failure modes motivate writing this card *before* any
adapter ships:

1. **Read-surface drift into write-surface.** A read panel that
   shows `next_operator_command: "claw plan approve …"` is one
   plausible refactor away from "click here to run that command for
   you." That single refactor collapses the TTY-enforced approval
   boundary
   ([A2-L2b handoff §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain),
   [A2-L2c quickref §3](./a2-l2c-operator-quickref.md#3-tty-approval-eof-note))
   and the explicit-apply boundary
   ([A2-L2b handoff §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates))
   in a single change. A2-L3 forbids that refactor by construction:
   an adapter that ever spawns the canonical chain commands on
   behalf of the operator is out of scope.

2. **STOP attenuation by adapter UX.** A read panel summarizing
   "chain state" can quietly collapse `phase`, `stop_condition`,
   `is_approvable`, and `evidence_paths` into a single status pill
   ("Ready", "Blocked"). That summarization hides the exact reason
   the chain stopped, the exact files to inspect, and the exact
   STOP-gate language the operator must escalate on. A2-L3 forbids
   STOP attenuation: every STOP signal in the envelope must reach
   the operator with at least the granularity the envelope carries.

A2-L3 is the smallest reversible step that addresses both failure
modes before any adapter ships.

## 3. Relationship To A2-L2d

A2-L2d shipped:

- `claw plan status <workspace> [<approval-result.json>]` — read-only
  CLI command
  ([A2-L2d operator quickref §2](./a2-l2d-operator-quickref.md#2-command)).
- `a2-l2d-status.v1` envelope — pinned schema-of-record with closed
  `phase`, `stop_condition`, `next_operator_command`, and
  `audit_markers` enums, fixed field order, byte-identical idempotent
  stdout
  ([A2-L2d status schema §3](./a2-l2d-status-schema.md#3-output-envelope)).
- `EXIT_STATUS_REFUSED == 12` — read-time refusal exit code
  ([A2-L2d status schema §8](./a2-l2d-status-schema.md#8-exit-codes)).
- Read-only, network-egress-free, and idempotency invariants —
  enforced in `rust/crates/a2-plan-runner/src/status.rs` and the
  associated tests
  ([A2-L2d status schema §11](./a2-l2d-status-schema.md#11-read-only-invariants)).
- IDE / harness boundary preamble — the A2-L2d scope card's section 10
  ([A2-L2d scope card §10](./a2-l2d-readonly-inspection-scope-card.md#10-ide--harness-boundary))
  named the high-level "MAY / MUST NOT" pair that A2-L3 now turns
  into a full scope card.

A2-L3 is the **consumer-side** specification of that A2-L2d §10
preamble. It does not modify `a2-l2d-status.v1`, does not add fields,
does not add commands, does not add flags, does not add markers, does
not add exit codes, and does not change any A2-L2b behavior. The
A2-L2d contract remains authoritative on every contract question;
A2-L3 governs only how a future adapter consumes that contract.

## 4. Adapter Responsibilities

When the future A2-L3 adapter is implemented as a separate lane, it
must exhibit the following responsibilities. These are the things the
adapter *exists to do*.

- **Consume `a2-l2d-status.v1` only.** The adapter MUST source chain
  state exclusively from stdout of `claw plan status <workspace>
  [<approval-result.json>]`. It MUST NOT re-implement any
  `.claw/l2b-*` parsing logic, MUST NOT shortcut to artifact reads,
  and MUST NOT derive state from any other source.
- **Display every field verbatim.** The adapter MAY render
  `schema_version`, `workspace_root`, `run_id`, `step_id`, `phase`,
  `next_operator_command`, `is_approvable`, `is_apply_ready`,
  `before_sha256`, `after_sha256`, `payload_sha256`,
  `live_target_sha256`, `stop_condition`, `evidence_paths`,
  `audit_markers`, and `read_only_invariant`. Verbatim rendering of
  closed-enum values is required; rewording a `stop_condition` from
  `payload-sha-mismatch` to "Mismatch detected" is forbidden by
  §11.
- **Surface the next operator command as text.** The adapter MAY
  display `next_operator_command` as a copyable string. It MAY offer
  "copy to clipboard." It MUST NOT execute it, schedule it, prompt
  to execute it, or compose it with any other operator action.
- **Display evidence paths as inspectable references.** The adapter
  MAY surface each entry in `evidence_paths` as a path the operator
  can open in their editor or shell. It MAY render the path as a
  link if the underlying surface supports that. It MUST NOT read,
  preview, or otherwise process the contents of those files;
  operator inspection of the artifact is the operator's action.
- **Honor the `read_only_invariant` literal.** The adapter MUST
  surface `read_only_invariant: "this command does not mutate
  state"` as a visible, non-collapsible safety marker on every
  rendered envelope. The adapter MAY position it discreetly; it MUST
  NOT hide, abbreviate, or substitute it.
- **Re-invoke the status command on operator request.** The adapter
  MAY offer a "refresh" operator action that re-runs `claw plan
  status` and re-renders the new envelope. This re-invocation is the
  only CLI subprocess the adapter is permitted to spawn, and only
  for the read-only status command itself with no flags.
- **Pass through `EXIT_STATUS_REFUSED`.** When `claw plan status`
  exits `12`, the adapter MUST render the refusal envelope using the
  same display rules as a non-refusal envelope, including
  `stop_condition`, `next_operator_command: "STOP — escalate"`, and
  the `a2-l2d-status-refused` marker
  ([A2-L2d status schema §9](./a2-l2d-status-schema.md#9-refusal-envelope)).
- **Display historical state already in artifacts, only by
  re-invoking status.** If the operator wants prior-run state, the
  adapter MAY re-run `claw plan status` against a different
  workspace (one re-invocation per request). It MUST NOT inventory,
  iterate, watch, or aggregate across runs or workspaces beyond what
  the envelope carries.

## 5. Adapter Non-Responsibilities

These are the things the adapter exists *to not do*. Each maps to a
named safety property in A2-L2b, A2-L2c, or A2-L2d.

- **Approval is not an adapter responsibility.** The adapter MUST
  NOT call `claw plan approve`, MUST NOT pre-fill the approval
  prompt, MUST NOT emit approval text on the operator's behalf, MUST
  NOT surface an "approve" button, toggle, menu item, keybinding,
  drag target, or context-menu entry, and MUST NOT carry an approval
  decision through any side channel.
- **Apply-bundle generation is not an adapter responsibility.** The
  adapter MUST NOT call `claw plan apply-bundle`, MUST NOT construct
  an apply bundle by hand, and MUST NOT package the inputs to
  apply-bundle generation as a single adapter action.
- **Apply is not an adapter responsibility.** The adapter MUST NOT
  call `claw plan apply`, MUST NOT surface an "apply" button, and
  MUST NOT compose apply with any other action.
- **Retry is not an adapter responsibility.** The adapter MUST NOT
  re-invoke any canonical chain command after a refusal or STOP
  signal. The operator's escalation path is human, not adapter-
  triggered.
- **Rollback is not an adapter responsibility.** `phase ==
  rolled_back` is a read-only diagnosis surfaced by the envelope; the
  adapter MUST NOT initiate, suggest the initiation of, or
  pre-populate any rollback-adjacent command. There is no
  "auto-rollback" affordance on the adapter surface.
- **Artifact mutation is not an adapter responsibility.** The
  adapter MUST NOT write, rename, delete, copy, or move any file
  under `.claw/`, the workspace tree, the operator's home directory,
  or anywhere else. The adapter MUST NOT introduce its own on-disk
  caches; if it caches in memory it MUST clearly mark such state as
  adapter-local and non-authoritative.
- **STOP suppression is not an adapter responsibility.** The
  adapter MUST NOT hide, debounce, collapse, attenuate, summarize-
  away, or rate-limit STOP signals. Every `stop_condition` value and
  every `STOP — escalate` directive must reach the operator at least
  once per envelope rendered.
- **State invention is not an adapter responsibility.** The adapter
  MUST NOT compute, infer, or display chain state that the envelope
  does not carry. "Pending", "queued", "in-flight", or "progressing"
  status pills that are not derivable from a single envelope field
  are forbidden.
- **CLI extension is not an adapter responsibility.** The adapter
  MUST NOT propose, request, or depend on new CLI commands, flags,
  schema versions, exit codes, or markers in order to function. Any
  contract gap an adapter discovers is escalated as a separate
  scope-card lane; the adapter does not work around the contract.

## 6. Allowed Reads

The future adapter implementation lane may read, and only read, the
following:

- stdout of `claw plan status <workspace>` (success envelope, exit
  `0`).
- stdout of `claw plan status <workspace> <approval-result.json>`
  (success envelope, exit `0`).
- stdout of `claw plan status <workspace> [<approval-result.json>]`
  refusal envelope (exit `EXIT_STATUS_REFUSED == 12`).
- exit code of the above invocations.
- operator-supplied input that names a workspace, names an
  approval-result path, or requests a refresh.

The adapter may not read:

- any `.claw/**` file directly (read is mediated exclusively through
  the status command).
- any other workspace file directly. If the operator opens an entry
  from `evidence_paths` in their editor, that read is performed by
  the operator's editor, not by the adapter.
- broker endpoints, model endpoints, Ollama endpoints, or any HTTP
  surface.
- secrets, environment variables, the operator's shell history, the
  operator's terminal state, or any non-workspace file beyond what
  `claw plan status` itself already reads.
- prior adapter-process state on disk (the adapter MUST NOT
  introduce an on-disk cache).

The adapter explicitly does **not** subscribe to a filesystem
watcher, a Git event stream, a daemon push channel, or any
notification surface that would let it act without an operator-
initiated refresh.

## 7. Forbidden Actions

The future adapter implementation lane is forbidden from performing
any of the following. Each forbidden action maps to a named safety
property the chain depends on.

- spawning `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, or `claw plan apply` as a subprocess for any reason
- spawning any process other than the read-only `claw plan status`
  command, and only with no flags (the status command refuses every
  write-adjacent flag by contract; the adapter does not need to
  re-enforce that, but it MUST NOT attempt to invoke variants that
  the status command would refuse)
- pre-filling, auto-completing, or otherwise generating the
  TTY approval line `apply <step_id> <preview_sha256>` in any
  channel — operator-visible or otherwise
- producing or persisting `<approval-result.json>` on the operator's
  behalf
- producing or persisting `apply-bundle.json` on the operator's
  behalf
- modifying any file under `.claw/`, the workspace tree, the
  operator's home directory, the adapter's own configuration
  directory, or anywhere else, except to render UI state in memory
- composing `approve` and `apply` into a single adapter action,
  whether by chained subprocess, sequenced UI gesture, scripted
  shortcut, plug-in, macro, recorded action, or any other mechanism
- offering a "one-click", "auto", "fast", "express", "skip", or
  "trust" mode that elides any operator gesture the canonical chain
  requires
- adding write-adjacent flags to the adapter UI that map onto
  refused CLI flags (`--apply`, `--approve`, `--yes`, `--auto`,
  `--clean`, `--rollback`, `--mutate`, `--all-runs`, `--no-prompt`,
  `--skip-approval`, `--cache`); UI affordances that compose to the
  same semantic effect as those flags are equally forbidden
- calling broker, model, Ollama, telemetry, analytics, error-
  reporting, or any other network endpoint at any phase of adapter
  operation
- caching envelope contents on disk in any form (`.claw/l3-adapter-
  cache/`, `~/.cache/claw/`, IDE workspace storage, harness
  artifact directory, or anywhere else)
- watching the filesystem for `.claw/**` changes and refreshing
  without operator gesture
- summarizing, collapsing, debouncing, rate-limiting, or otherwise
  attenuating any STOP signal carried by the envelope (see §11)
- introducing parallel status schemas, parallel envelope versions,
  or "extended" envelopes that wrap `a2-l2d-status.v1` with adapter-
  specific fields and re-emit them as authoritative
- treating the envelope as authoritative for write decisions; the
  chain re-validates every input at apply time
  ([A2-L2b handoff §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
  and the adapter MUST NOT pretend otherwise

## 8. IDE Boundary

An IDE adapter is the human-facing instance of this contract — a
panel, sidebar, hover, status-bar entry, or editor decoration in a
graphical environment.

The IDE adapter:

- MAY render the envelope in a dedicated panel or sidebar.
- MAY show `phase`, `stop_condition`, `next_operator_command`,
  `is_approvable`, `is_apply_ready`, and `evidence_paths` as
  primary surfaces; MAY show the SHA fields, `audit_markers`, and
  `read_only_invariant` as secondary detail.
- MAY render `evidence_paths` as clickable links that open the file
  in the IDE's own editor.
- MAY provide a "refresh status" command bound to a keybinding or
  command-palette entry.
- MAY provide a "copy next operator command" affordance that puts
  the literal `next_operator_command` string on the system
  clipboard.
- MUST NOT provide an "approve" button, command-palette entry,
  keybinding, context-menu item, gutter affordance, lens, hover
  action, drag target, status-bar action, or any equivalent
  affordance.
- MUST NOT provide an "apply" button or any equivalent affordance.
- MUST NOT provide an "approve + apply" composite action.
- MUST NOT provide an automatic "approve-when-X" rule, an "approve all
  pending previews" batch action, or any preapproval mechanism.
- MUST NOT pop a modal that captures the operator's approval line
  and forwards it to `claw plan approve`. TTY enforcement is the
  approval boundary; an IDE modal is not a TTY.
- MUST NOT display chain state in a way that suggests adapter
  authority over the chain. A panel header that reads "Chain
  Controller" or "Apply Manager" is a category violation; "Chain
  Status" or "Read-Only Observer" is accurate framing.
- MUST surface every STOP signal in the envelope at the same
  granularity it surfaces non-STOP state. A "Blocked" pill that
  hides the underlying `stop_condition` value is forbidden (see
  §11).
- MUST visibly display `read_only_invariant: "this command does not
  mutate state"` on every rendered envelope.

The IDE adapter is bounded to **display** and **operator-
copy/refresh** affordances. Every other affordance is out of scope
for A2-L3 and requires a separate scope card.

## 9. Harness Boundary

A harness adapter is the machine-facing instance of this contract —
a test runner, scripted observability tool, CI step, soak-test
checker, or operator script that consumes `a2-l2d-status.v1`
envelopes programmatically rather than for human display.

The harness adapter:

- MAY invoke `claw plan status <workspace> [<approval-result.json>]`
  as a read-only subprocess.
- MAY parse the JSON envelope.
- MAY assert on `schema_version`, `phase`, `stop_condition`,
  `is_approvable`, `is_apply_ready`, `next_operator_command`, the
  SHA fields, `audit_markers`, and `read_only_invariant`.
- MAY use those assertions to gate downstream operator actions
  (e.g., a CI step that fails the build when `stop_condition` is
  non-null and human escalation is expected).
- MAY emit envelope contents as structured logs or metrics for
  observability.
- MAY re-invoke the status command on a fixed cadence the harness
  schedules, with each invocation being an independent read.
- MUST NOT call `claw plan approve`, `claw plan apply-bundle`, or
  `claw plan apply` based on envelope contents.
- MUST NOT chain a successful status read into any write step that
  the canonical operator-gated chain reserves to a human.
- MUST NOT auto-resolve a STOP signal by retrying, hand-editing
  artifacts, regenerating bundles, or invoking remediation scripts.
- MUST NOT cache envelopes across runs in a way that becomes a
  source-of-truth for chain state (every assertion is against a
  fresh status invocation; cached values are diagnostic only).
- MUST NOT redact, summarize, or compress `stop_condition` /
  `evidence_paths` / `audit_markers` in emitted logs or metrics
  such that the human reader loses any STOP-relevant detail.
- MUST set `HTTP_PROXY`, `HTTPS_PROXY`, and `OLLAMA_HOST` to
  unreachable sentinels for the status invocation when the harness
  is operating in a network-isolated context, mirroring the
  invariants A2-L2d's tests already enforce
  ([A2-L2d status schema §11](./a2-l2d-status-schema.md#11-read-only-invariants)).

The harness adapter is bounded to **invoke**, **parse**, **assert**,
and **emit observability**. Every write-adjacent automation is out
of scope for A2-L3 and would re-introduce a write path the A2-L2b
chain explicitly forbids being executed without operator approval.

## 10. Status Contract Consumption Rules

The following rules govern any adapter's consumption of
`a2-l2d-status.v1`. They are derived from the envelope's existing
properties and do not modify the contract.

1. **Schema-version literal pinning.** Adapters MUST refuse any
   envelope whose `schema_version` is not the literal
   `a2-l2d-status.v1`. Adapters MUST NOT fall back to a "best
   effort" parse of an unrecognized version; they MUST escalate.
2. **Closed-enum trust.** Adapters MUST treat the `phase`,
   `next_operator_command`, `stop_condition`, and `audit_markers`
   value sets as closed at the values pinned in
   [A2-L2d status schema §§4–7](./a2-l2d-status-schema.md#4-closed-phase-enum).
   An unknown enum value is itself a STOP signal — adapters MUST
   surface it as such, not coerce it.
3. **Field-order independence.** Adapters MUST NOT assert on
   stdout byte order beyond what JSON parsing already enforces.
   `a2-l2d-status.v1` pins field order for the producer's
   idempotency, not for the consumer's parser.
4. **Idempotency expectation.** Adapters MAY assume two successive
   reads against an unchanged workspace produce equal envelopes.
   Adapters MUST NOT exploit this to skip rendering; every
   operator-initiated refresh is honored by re-invoking the status
   command.
5. **Read-only invariant as canary.** Adapters MUST treat the
   absence or alteration of the
   `read_only_invariant: "this command does not mutate state"`
   literal as a STOP signal. A malformed or substituted invariant
   indicates a broken producer and must be escalated, not absorbed.
6. **`next_operator_command` as text, not as instruction to the
   adapter.** The string is for the operator to execute in their
   own terminal. The adapter MUST treat it as opaque display data.
7. **`evidence_paths` as references, not as fetch targets.**
   Adapters MAY render paths and MAY open them in operator-driven
   tools. Adapters MUST NOT fetch, parse, hash, or otherwise process
   the file contents.
8. **No envelope wrapping.** Adapters MUST NOT wrap an
   `a2-l2d-status.v1` envelope inside an adapter-versioned envelope
   and re-emit the result as authoritative. Pass-through display is
   permitted; rewrapping is not.

## 11. STOP Condition Visibility Rules

The A2-L2b chain's safety is reasoned about in terms of STOP gates
that the operator must observe and escalate on
([A2-L2b handoff §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)).
A2-L2d surfaces those STOPs through the `stop_condition` enum, the
`next_operator_command: "STOP — escalate"` literal, the `phase`
values `non_approvable` / `rolled_back` / `unknown`, and the
`a2-l2d-status-stop-condition-detected` / `a2-l2d-status-refused`
markers.

Adapter STOP visibility rules:

- **Verbatim STOP-value rendering.** When `stop_condition` is
  non-null, the adapter MUST render its exact enum value
  (e.g., `payload-sha-mismatch`, `live-target-missing`). Substituting
  human-friendly text is forbidden; the enum value IS the operator
  escalation signal.
- **STOP-prominence parity.** STOP rendering MUST be at least as
  visually or programmatically prominent as the corresponding
  non-STOP rendering. A green "Healthy" pill that has a high-
  contrast surface and a red "Blocked" pill that has a low-contrast
  surface is a category violation.
- **No STOP debouncing.** If two successive refreshes both produce
  the same STOP signal, the adapter MUST render the STOP both
  times. Adapters MUST NOT collapse "same STOP twice in a row" into
  a single notification.
- **No STOP rate-limiting.** Adapters MUST NOT throttle STOP
  notifications. Every rendered envelope with a non-null
  `stop_condition` is a STOP event from the operator's perspective.
- **STOP-on-unknown.** If the adapter receives a `phase`,
  `stop_condition`, `next_operator_command`, or marker value not in
  the A2-L2d schema's closed enums, the adapter MUST treat that as
  a STOP signal in its own right and surface it with the same
  prominence as a known STOP.
- **STOP retention across refresh.** Adapters MAY clear a STOP from
  the rendered surface after a refresh produces a non-STOP envelope.
  Adapters MUST NOT clear a STOP from the rendered surface without a
  refresh, and MUST NOT clear it across an operator-invoked
  re-status if the new envelope still carries it.
- **STOP in logs/metrics.** Harness adapters that emit envelope
  contents to logs or metrics MUST emit `stop_condition`,
  `evidence_paths`, and `audit_markers` at full fidelity. Field
  redaction for these fields is forbidden.
- **No "ignore STOP for N seconds" affordance.** Adapter UIs MUST
  NOT offer a snooze, mute, dismiss, or ignore action for STOP
  signals. The chain's safety depends on STOPs being seen, not
  cleared.

## 12. Safety Invariants

The A2-L3 implementation lane must preserve, verbatim, every
property the prior lanes pinned:

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
  ([A2-L2d status schema §11](./a2-l2d-status-schema.md#11-read-only-invariants))
- A2-L2d network-egress-free invariant
- A2-L2d idempotency invariant
- A2-L2d non-overlapping marker invariant (`a2-l2d-*` only;
  adapters MUST NOT invent `a2-l3-*` markers that leak back into
  the status producer)
- A2-L2d non-overlapping exit-code invariant

In addition, A2-L3 adds:

- **Adapter read-only invariant.** No adapter operation may
  mutate any file, send any network egress (beyond the `claw plan
  status` subprocess it spawns), or call broker/model/Ollama.
- **Adapter STOP-visibility invariant.** Every STOP signal in an
  envelope reaches the operator at the granularity the envelope
  carries (§11).
- **Adapter no-write-surface invariant.** No adapter UI or
  programmatic surface composes to a write action against the A2-L2b
  chain (§5, §7, §8, §9).
- **Adapter no-state-invention invariant.** Adapters render the
  envelope; they do not synthesize chain state.
- **Adapter no-shadow-contract invariant.** Adapters consume
  `a2-l2d-status.v1` as-is; they do not wrap it, extend it, or
  parallel-version it.

## 13. Non-Goals

A2-L3 must not:

- implement an adapter (deferred; this lane is docs-only)
- implement an IDE integration (deferred)
- implement a harness integration (deferred)
- introduce or imply autonomous workspace-write execution
- introduce IDE controls that approve, that apply, or that compose
  approval-and-apply into a single gesture
- introduce harness-driven approve / apply / apply-bundle automation
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  preapproval, batch approval, or any approval-bypass affordance on
  the adapter surface
- modify `claw plan run`, `claw plan approve`,
  `claw plan apply-bundle`, `claw plan apply`, or `claw plan status`
  behavior, exit codes, schemas, markers, or JSON field shapes
- modify `a2-l2b-*` or `a2-l2d-status.v1` schema versions or marker
  constants
- introduce an `a2-l3-*` schema, marker, exit code, or CLI surface
  (the contract A2-L3 governs IS `a2-l2d-status.v1`; A2-L3 introduces
  no parallel contract)
- call broker, model, or Ollama at any phase
- introduce filesystem watchers, daemon push channels, or background
  refresh of `.claw/**`
- introduce on-disk caches of envelope contents
- weaken any A2-L2b, A2-L2c, or A2-L2d STOP gate
- implement multi-run inventory, cross-workspace dashboards, or
  history rollups (every such consumer needs a separate scope card
  that explicitly defines whether it is a status-contract extension
  or an adapter-side aggregation; A2-L3 does neither)
- introduce a harness adapter that auto-remediates STOP signals
- introduce an IDE adapter that surfaces chain state through any
  channel that bypasses the rendered panel's STOP visibility (e.g.,
  a status-bar pill with no STOP affordance)

Any of the above must be opened as a separate, explicitly-
authorized lane.

## 14. Future Implementation Constraints

When the A2-L3 implementation lane is opened (as a separate scope
card per concrete adapter — one for IDE, one for harness, or one
combined card that explicitly enumerates its surfaces), it must
hold to all of the following:

- **Allowed Future Touched Surfaces** must be explicitly enumerated
  in the implementation scope card before any code or wrapper is
  authored. The implementation lane MUST NOT touch any file outside
  that enumerated list. Likely surfaces include a new adapter crate
  under `rust/crates/`, new tests under that crate's `tests/`, and
  new documentation under `docs/`. Specific paths are deferred to
  the implementation scope card.
- **Forbidden Surfaces** must explicitly include every A2-L2b
  module
  (`rust/crates/a2-plan-runner/src/{approval,approval_ux,
  checkpoint,diff_preview,preflight,report,runner,write_executor,
  write_payload,write_preview,write_runtime,markers}.rs`), every
  A2-L2b schema constant, every A2-L2b exit-code constant, every
  A2-L2b/`a2-l2d-*` marker constant, and
  `rust/crates/a2-plan-runner/src/status.rs` (the adapter consumes
  the status command's stdout; it does not modify the producer).
- **Validation** must include: forbidden-language sniff against the
  staged diff for the same regex family as the A2-L2b, A2-L2c, and
  A2-L2d cards; tests asserting the adapter spawns no subprocess
  other than `claw plan status`; tests asserting the adapter
  performs zero filesystem writes under any input; tests asserting
  the adapter performs zero network egress beyond the status
  subprocess; tests asserting the adapter renders every STOP signal
  at the granularity §11 requires; regression tests confirming
  every existing A2-L2b/L2c/L2d test still passes unchanged.
- **STOP rendering** must be covered by golden-file tests against
  every closed `stop_condition` value, every closed `phase` value,
  the refusal envelope, and at least one unknown-enum-value
  synthetic fixture.
- **Operator-driven refresh** is the only adapter-initiated CLI
  invocation. The implementation scope card must explicitly forbid
  background polling, filesystem watchers, and daemon channels.

## 15. Definition Of Done

This **scope card** is done when:

- `docs/a2-l3-adapter-boundary-scope-card.md` exists and matches the
  sectional structure of this card.
- The card defines adapter responsibilities and non-responsibilities
  in non-softening language.
- The card pins the IDE boundary and the harness boundary
  separately.
- The card pins the status contract consumption rules.
- The card pins the STOP visibility rules without escape hatches.
- The card declares A2-L3 as docs-only at this scope-card stage.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, or A2-L2d STOP gate is weakened.
- A single cross-link line MAY be added to README, to the A2-L2d
  scope card, to the A2-L2d status schema, or to the A2-L2d
  operator quick reference if an obvious location exists, but no
  such cross-link is required for this scope card itself to land.
  *(This scope card is authored without README or sibling cross-
  links to keep the lane strictly limited to a single new
  docs file; cross-links may be added in a follow-up lane.)*
- The card is reviewed by the operator before any A2-L3
  implementation lane is opened.

The A2-L3 **implementation lane** is out of scope for this card.
Definition of done for that lane will be authored when its own scope
card is created, bounded by the constraints in §§4–14 above.

## 16. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **A2-L3 implementation scope-card lane (per-adapter, docs-only)** —
> author a concrete scope card for a single adapter surface (IDE
> *or* harness, not both in one lane) that enumerates its allowed
> touched surfaces, its forbidden surfaces, its validation plan, its
> STOP-rendering test matrix, and its definition of done, all bounded
> by this A2-L3 adapter boundary scope card. Do not author the
> adapter implementation in the same lane as its scope card.

The lane *after* the per-adapter scope card lands is:

> **The first concrete adapter implementation lane** — implement
> exactly one adapter (IDE *or* harness) under the constraints
> pinned by both this card and that adapter's scope card, with
> golden tests for every STOP signal and read-only invariant. The
> implementation lane MUST NOT expand the contract; any contract
> gap discovered during implementation is escalated as a separate
> scope-card lane.

Neither lane permits autonomous workspace-write execution. Both
remain bounded by the A2-L2b, A2-L2c, and A2-L2d safety properties
and by §§4–14 of this card.

## 17. References

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- [`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md) — A2-L2c scope
  card; the structural model this card follows.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card; section 10 ("IDE / Harness Boundary") is the
  preamble this card expands into a full scope.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the
  contract A2-L3 governs the consumption of.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.
- PR #39 (`12fff14`) — A2-L2d scope card.
- PR #40 (`0f75800`) — A2-L2d read-only `claw plan status` command +
  `a2-l2d-status.v1`.
- PR #41 (`4c2b15e`) — A2-L2d operator quick reference.

## 18. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- Adapter implementation authorized: **no**.
- IDE integration authorized: **no**.
- Harness integration authorized: **no**.
- Autonomous-write authorization: **no**.
- Approval / apply boundary weakened: **no**.
- A2-L2b / A2-L2c / A2-L2d STOP gate weakened: **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by a per-adapter implementation scope-card lane.
