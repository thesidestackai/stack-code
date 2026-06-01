# A2-L3 Scope Card — IDE Adapter (Docs-Only)

This document is a **design-only scope card** for the IDE-side
per-adapter lane bounded by the A2-L3 adapter boundary scope card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)).
It defines, in design only, the bounded behavior of a future **IDE
adapter** that consumes the shipped A2-L2d read-only status surface as
a human-facing visual observer.

This file itself authorizes **no** runtime change, **no** CLI change,
**no** IDE implementation, **no** adapter implementation, **no**
broker/model/Ollama traffic, **no** approve/apply UI controls, **no**
approve/apply composition, **no** autonomous workspace-write
execution, and **no** behavior that weakens the A2-L2b operator-gated
chain. It is a per-adapter scope card; the IDE adapter
implementation lane is a separate, future, explicitly-authorized lane
that this card constrains.

A2-L3 progression as of this card:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, shipped)
      → IDE / harness adapter boundary (A2-L3, scope card shipped, PR #42)
        → harness adapter per-adapter scope card (A2-L3, shipped, PR #44)
          → harness adapter implementation scope card (A2-L3, shipped, PR #45)
            → harness adapter implementation (A2-L3, shipped, PR #46)
              → harness adapter usage guide (A2-L3, shipped, PR #47)
                → harness PR43 preservation patch (A2-L3, shipped, PR #48)
                  → IDE adapter per-adapter scope card (THIS DOCUMENT)
                    → future IDE adapter implementation scope card (separate, future)
                      → future IDE adapter implementation (separate, future)
```

This scope card authorizes **design only**. It does not authorize IDE
implementation. It does not authorize adapter implementation. It does
not authorize approve/apply UI controls. It does not authorize
autonomous workspace-write execution.

## 1. Executive Summary

The A2-L3 IDE adapter is a **read-only visual observer** of
`a2-l2d-status.v1`, never a **workflow controller**. It is a
human-facing surface — a panel, sidebar, hover, status-bar entry,
editor decoration, or analogous IDE affordance — that renders the
envelope emitted by `claw plan status <workspace>
[<approval-result.json>]` on explicit operator refresh and presents
its fields verbatim for the operator to read and act on in their own
terminal.

The IDE adapter is **not** an approval executor, **not** an apply
executor, **not** an apply-bundle generator, **not** a composite
approve+apply controller, **not** a retry-after-STOP affordance, and
**not** a write surface for `.claw/**`, workspace files, or any other
file outside the operator's normal IDE editing. Every write-adjacent
affordance the A2-L2b chain forbids being executed without operator
approval remains forbidden for the IDE adapter by construction.

The IDE adapter is **chosen second** in the A2-L3 per-adapter
progression because its affordance surface is broader, its failure
mode is quieter, and its visual presentation is the one most likely
to attenuate a STOP signal under UX pressure. Writing this card down
*after* the harness adapter implementation lane shipped is
intentional: the harness adapter exercised the read-only contract
end-to-end, surfaced no producer-side gaps, and provided a worked
example the IDE adapter implementation lane can study before
designing its own visual surface.

This card defines the boundary the future IDE adapter
implementation lane must hold to. The next gate before any IDE
implementation is operator review of this scope card, followed by an
IDE-adapter implementation scope-card lane that enumerates its
allowed touched surfaces and validation plan.

## 2. Why IDE Adapter Comes After Harness

Three reasons place the IDE adapter after the harness adapter in the
A2-L3 progression:

1. **Affordance surface is broad, not narrow.** A harness adapter
   consumes the envelope through a subprocess and a JSON parse; its
   write-adjacent failure modes are bounded by the assertion API.
   An IDE adapter exposes buttons, keybindings, command-palette
   entries, gutter affordances, context-menu items, hover actions,
   status-bar pills, drag targets, and any number of other
   affordances an IDE host can grow. The set of plausible
   write-adjacent surfaces is broad enough that designing the IDE
   adapter without first letting the harness adapter pin the
   contract would risk inventing UX shortcuts that quietly collapse
   the TTY-enforced approval boundary
   ([`a2-l2c-operator-quickref.md` §3](./a2-l2c-operator-quickref.md#3-tty-approval-eof-note))
   or the explicit-apply boundary
   ([`a2-l2b-run-plan-preview-operator-handoff.md` §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)).

2. **Failure mode is silent, not loud.** A harness assertion failure
   produces a non-zero exit and a structured diagnostic. A
   misdesigned IDE affordance — for example, a "Ready to Apply"
   status pill that hides the underlying `stop_condition`, a "copy
   approval line" gesture that quietly composes with a "run in
   terminal" shortcut, or a "refresh" affordance that
   auto-clears a STOP after a non-STOP refresh — fails silently from
   the operator's perspective. The chain's safety depends on STOPs
   being seen and the operator-gated chain being preserved; the
   visual surface that fails silently is the one whose boundary must
   be pinned most strictly *before* a single pixel is rendered.

3. **The harness adapter is a worked example.** The shipped harness
   adapter
   ([`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md),
   PR #46 + PR #47 + PR #48) demonstrates a complete read-only
   consumption of `a2-l2d-status.v1` with golden coverage of every
   closed enum, every STOP signal, every refusal envelope, and every
   schema-drift case. The IDE adapter implementation lane can study
   that crate's parser, STOP taxonomy, and idempotency rules as a
   reference for envelope handling *before* designing the visual
   presentation layer. Writing the IDE adapter scope card after the
   harness adapter shipped lets that reference inform the IDE
   adapter's design without the IDE adapter inheriting any of the
   harness adapter's machine-facing concerns (CI integration,
   disposable-workspace classification, NDJSON output).

The IDE adapter is therefore a forcing function in the opposite
direction from the harness adapter: writing it down *after* the
harness shipped makes the visual-presentation boundary explicit
before the higher-affordance IDE surface is opened.

## 3. Relationship To A2-L3 Adapter Boundary

[`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
defined the overall A2-L3 adapter boundary in design only. Its §8
already pinned an IDE-boundary preamble naming the high-level
"MAY / MUST NOT" pair for IDE consumers. This per-adapter scope
card expands that preamble into a full scope card for the IDE
surface alone.

Specifically:

- The IDE adapter remains bound by every safety invariant in the
  A2-L3 adapter boundary card §12 (`Safety Invariants`), §13
  (`Non-Goals`), and §14 (`Future Implementation Constraints`).
- The IDE adapter remains bound by the five A2-L3-introduced
  invariants in that card §12: adapter read-only, adapter
  STOP-visibility, adapter no-write-surface, adapter no-state-
  invention, and adapter no-shadow-contract.
- This card refines those invariants for the IDE surface but does
  **not** relax any of them and does **not** introduce a parallel
  contract. Where this card and the boundary card differ in tone, the
  boundary card and (above it) A2-L2d remain authoritative on the
  contract.
- This card does **not** modify `a2-l2d-status.v1`, add new fields,
  add new commands, add new flags, add new schema versions, add new
  exit codes, or add new markers. The A2-L2d status schema
  ([`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)) is
  authoritative on the envelope.

## 4. Relationship To A2-L3 Harness Adapter

The A2-L3 harness adapter
([`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md),
[`a2-l3-harness-adapter-implementation-scope-card.md`](./a2-l3-harness-adapter-implementation-scope-card.md),
[`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md))
is a machine-facing observer of `a2-l2d-status.v1`. The IDE adapter
defined here is its human-facing counterpart. Both consume the same
envelope, both honor the same closed enums, both surface every STOP
signal at full fidelity, and both refuse to compose any write action
against the A2-L2b chain.

Important distinctions:

- **The IDE adapter does NOT depend on the harness adapter crate.**
  Future IDE implementation MAY study `rust/crates/a2-harness-adapter/`
  as a worked example, MAY reuse parser-level patterns by re-deriving
  them, and MAY borrow STOP-taxonomy ideas; it MUST NOT take a Cargo
  dependency on the harness adapter crate, because the harness adapter
  is a library API for machine consumers, not a UI library. Coupling
  the two would let IDE-side affordance pressure leak into the
  harness adapter's API surface.
- **The IDE adapter does NOT extend the harness adapter's assertion
  model.** The harness adapter classifies cycles as PASS / FAIL /
  STOP for CI consumption; the IDE adapter renders state for human
  observation. The IDE adapter MUST NOT surface a "pass/fail" pill
  derived from a synthetic assertion the operator did not declare in
  their own terminal session.
- **The IDE adapter does NOT inherit the harness adapter's
  disposable-workspace classifier as a permission grant.** If a
  future IDE adapter chooses to surface a disposable-workspace
  indicator at all, it does so as a read-only display of the
  classifier's decision (or of a future status-envelope field if
  A2-L2d adds one in a separate lane); the IDE adapter MUST NOT
  classify workspaces itself, MUST NOT permit operator-side
  classification overrides, and MUST NOT use the classification to
  unlock any UI affordance.

The harness adapter and the IDE adapter are **siblings under the
A2-L3 adapter boundary**, not parent-and-child. Neither one is the
authority on the other's surface.

## 5. Relationship To A2-L2d Status Contract

A2-L2d shipped `claw plan status <workspace> [<approval-result.json>]`,
the `a2-l2d-status.v1` envelope, `EXIT_STATUS_REFUSED == 12`, and the
read-only / network-egress-free / idempotency invariants pinned in
[`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants).
The IDE adapter consumes that surface as-is:

- The IDE adapter invokes only `claw plan status <workspace>
  [<approval-result.json>]`, with no flags. Every write-adjacent flag
  is refused by the A2-L2d implementation; the IDE adapter does not
  re-enforce that refusal because the producer already does, but the
  IDE adapter MUST NOT attempt to invoke variants the status command
  would refuse and MUST NOT expose UI affordances that compose to the
  same semantic effect as those flags.
- The IDE adapter treats `a2-l2d-status.v1` as the authoritative
  envelope shape. It does not parse any other surface, does not
  shortcut to `.claw/**` artifacts, and does not derive chain state
  from any source other than `claw plan status` stdout and exit code.
- The IDE adapter honors every closed enum
  ([`a2-l2d-status-schema.md` §§4–7](./a2-l2d-status-schema.md#4-closed-phase-enum))
  as closed. Unknown enum values are themselves STOP signals
  surfaced verbatim in the IDE rendering, not silently coerced or
  re-styled.
- The IDE adapter honors the refusal envelope
  ([`a2-l2d-status-schema.md` §9](./a2-l2d-status-schema.md#9-refusal-envelope))
  with the same display rules it applies to non-refusal envelopes.
  Exit code `12` is not collapsed into a generic "command failed"
  banner without surfacing the refusal envelope's `stop_condition`,
  `next_operator_command: "STOP — escalate"`, and `evidence_paths`.
- The IDE adapter relies on the A2-L2d idempotency invariant
  ([`a2-l2d-status-schema.md` §10](./a2-l2d-status-schema.md#10-idempotency-rules)):
  two successive reads against an unchanged workspace produce byte-
  identical stdout. The IDE adapter MAY surface a "no change since
  last refresh" indicator derived from byte-equality, but MUST NOT
  use this property to skip rendering, MUST NOT debounce successive
  STOP signals, and MUST NOT cache envelopes to disk.

## 6. IDE Adapter Responsibilities

When the future IDE adapter implementation lane lands as a separate
scope, it must exhibit the following responsibilities. These are
the things the IDE adapter exists *to do*.

- **Invoke `claw plan status` only on explicit operator refresh.**
  The IDE adapter MUST source chain state exclusively from
  `claw plan status <workspace> [<approval-result.json>]`. The
  invocation is triggered only by an operator-visible "refresh"
  control (button, command-palette entry, keybinding bound by the
  operator, or initial-load when the panel is opened by the
  operator); no background polling, no filesystem watcher, no
  timer-based auto-refresh.
- **Parse the `a2-l2d-status.v1` envelope verbatim.** The IDE
  adapter MUST parse stdout as the pinned envelope and MUST refuse
  any envelope whose `schema_version` literal is not exactly
  `a2-l2d-status.v1`. Unrecognized versions surface as STOP signals
  in the rendered panel, not as best-effort displays.
- **Render every envelope field for the operator.** The IDE adapter
  MAY render `schema_version`, `workspace_root`, `run_id`,
  `step_id`, `phase`, `next_operator_command`, `is_approvable`,
  `is_apply_ready`, `before_sha256`, `after_sha256`,
  `payload_sha256`, `live_target_sha256`, `stop_condition`,
  `evidence_paths`, `audit_markers`, and `read_only_invariant`.
  Verbatim rendering of closed-enum values is required.
- **Surface `next_operator_command` as copyable text only.** The IDE
  adapter MAY display `next_operator_command` as a selectable
  string and MAY offer a "copy to clipboard" affordance whose only
  effect is placing the literal string on the system clipboard for
  the operator to paste into their own terminal. The IDE adapter
  MUST NOT execute the string, MUST NOT pre-populate a terminal
  with the string, MUST NOT prompt the operator to confirm
  execution, and MUST NOT compose the copy gesture with any other
  action.
- **Display evidence paths as local file links only.** The IDE
  adapter MAY surface each entry in `evidence_paths` as a path the
  operator can click to open in the IDE's own editor (the same way
  any in-editor file link behaves). It MUST NOT read, hash,
  preview, summarize, or otherwise process the file contents
  itself; opening the file is the operator's action, executed by
  the IDE host's normal file-open behavior.
- **Display `read_only_invariant` verbatim.** The IDE adapter MUST
  surface `read_only_invariant: "this command does not mutate
  state"` as a visible, non-collapsible safety marker on every
  rendered envelope. The marker MAY be positioned discreetly; it
  MUST NOT be hidden, abbreviated, substituted, or omitted, and
  alteration or absence MUST be classified as a STOP signal in its
  own right (§13).
- **Render STOP signals at parity with non-STOP rendering.** The
  IDE adapter MUST render `stop_condition`, `phase` values
  `non_approvable` / `rolled_back` / `unknown`,
  `next_operator_command == "STOP — escalate"`, and the
  `a2-l2d-status-stop-condition-detected` / `a2-l2d-status-refused`
  markers with at least the visual prominence of non-STOP rendering.
  See §12 for the full STOP-rendering rule set.
- **Pass through `EXIT_STATUS_REFUSED`.** When `claw plan status`
  exits `12`, the IDE adapter MUST render the refusal envelope
  using the same display rules as a non-refusal envelope, including
  `stop_condition`, `next_operator_command: "STOP — escalate"`, and
  the `a2-l2d-status-refused` marker
  ([`a2-l2d-status-schema.md` §9](./a2-l2d-status-schema.md#9-refusal-envelope)).
- **Provide a copy-to-clipboard affordance for the command text and
  evidence paths only.** The IDE adapter MAY offer copy actions
  scoped to the `next_operator_command` string and to individual
  `evidence_paths` entries. It MUST NOT offer copy actions that
  bundle multiple fields into a single composite paste payload
  (e.g. "copy approval line preformatted for the terminal"), and
  MUST NOT offer copy actions whose payload differs from the
  envelope's verbatim string.
- **Provide a collapsible raw-envelope view.** The IDE adapter MAY
  offer a collapsible "raw JSON" / "raw status" disclosure that
  renders the unparsed envelope bytes (or the parsed envelope re-
  emitted as canonical JSON) for the operator to inspect directly.
  This view is supplementary to the structured rendering; the
  structured rendering is the primary surface.
- **Provide a refresh control.** The IDE adapter MAY expose a
  single "refresh status" control (button, command-palette entry,
  keybinding) that re-invokes `claw plan status` and re-renders the
  new envelope. This re-invocation is the only CLI subprocess the
  adapter is permitted to spawn, and only for the read-only status
  command itself with no flags. See §14.

## 7. IDE Adapter Non-Responsibilities

These are the things the IDE adapter exists *to not do*. Each maps
to a named safety property in A2-L2b, A2-L2c, A2-L2d, the A2-L3
adapter boundary card, or the A2-L3 harness adapter cards.

- **Approval is not an IDE responsibility.** The IDE adapter MUST
  NOT call `claw plan approve`, MUST NOT compose the approval line
  `apply <step_id> <preview_sha256>`, MUST NOT generate an
  `<approval-result.json>` artifact, MUST NOT surface an "approve"
  button (or command-palette entry, keybinding, context-menu item,
  gutter affordance, lens, hover action, drag target, status-bar
  action, or any equivalent), and MUST NOT pop a modal that
  captures the operator's approval line and forwards it to
  `claw plan approve`. TTY enforcement is the approval boundary; an
  IDE modal is not a TTY.
- **Apply-bundle generation is not an IDE responsibility.** The IDE
  adapter MUST NOT call `claw plan apply-bundle`, MUST NOT construct
  an apply bundle by hand, MUST NOT package the inputs to apply-
  bundle generation as a single IDE action, and MUST NOT surface
  any UI affordance that composes to the same semantic effect.
- **Apply is not an IDE responsibility.** The IDE adapter MUST NOT
  call `claw plan apply`, MUST NOT surface an "apply" button (or
  equivalent affordance), and MUST NOT compose apply with any other
  action.
- **Run is not an IDE responsibility.** The IDE adapter MUST NOT
  call `claw plan run` (with or without `--workspace-write-preview`),
  MUST NOT surface a "start chain" or "run plan" button, and MUST
  NOT pre-populate the operator's terminal with any
  `claw plan run …` invocation.
- **Approve+apply composition is not an IDE responsibility.** The
  IDE adapter MUST NOT compose `approve` and `apply` into a single
  IDE action through any mechanism, whether by chained subprocess,
  sequenced UI gesture, scripted shortcut, plug-in, macro, recorded
  action, workspace task, IDE-host run-configuration, or anything
  else that produces a single operator gesture that triggers both
  steps.
- **Automatic-approval and automatic-apply settings are not an IDE
  responsibility.** The IDE adapter MUST NOT expose a setting,
  preference, configuration field, workspace setting, environment
  variable, or any persistent operator-toggleable affordance whose
  effect would skip, batch, pre-approve, or auto-execute any chain
  step.
- **Batch approval / preapproval is not an IDE responsibility.**
  The IDE adapter MUST NOT offer "approve all pending previews",
  "approve all in workspace", "approve everything matching X", or
  any equivalent batch affordance. Each preview is approved by the
  operator in their own terminal, individually, per the canonical
  A2-L2b chain.
- **Retry on STOP is not an IDE responsibility.** The IDE adapter
  MUST NOT re-invoke any canonical chain command after a STOP
  signal. It MAY allow the operator to invoke a manual refresh of
  `claw plan status` (one re-read per operator gesture) but MUST NOT
  chain a status re-read into any write step and MUST NOT
  auto-refresh after STOP to clear it.
- **Rollback is not an IDE responsibility.** `phase == rolled_back`
  is a read-only diagnosis surfaced by the envelope; the IDE
  adapter MUST NOT initiate, pre-populate, suggest the initiation
  of, or compose any rollback-adjacent command. There is no
  "rollback" affordance on the IDE surface.
- **STOP suppression is not an IDE responsibility.** The IDE
  adapter MUST NOT hide, debounce, collapse, attenuate, summarize-
  away, dismiss, mute, snooze, ignore, or rate-limit STOP signals.
  Every `stop_condition` value, every `STOP — escalate` directive,
  and every STOP-bearing marker must reach the operator at full
  fidelity on every rendered envelope (§12).
- **State invention is not an IDE responsibility.** The IDE adapter
  MUST NOT compute, infer, or display chain state that the envelope
  does not carry. Synthetic IDE-only phases or pills (e.g.,
  "Pending", "Queued", "In flight", "Progressing", "Healthy",
  "Almost ready") that are not derivable from a single envelope
  field are forbidden.
- **CLI extension is not an IDE responsibility.** The IDE adapter
  MUST NOT propose, request, or depend on new CLI commands, flags,
  schema versions, exit codes, or markers in order to function. Any
  contract gap an IDE adapter discovers is escalated as a separate
  scope-card lane against A2-L2d; the adapter does not work around
  the contract.
- **Workspace mutation is not an IDE responsibility.** The IDE
  adapter MUST NOT write, rename, delete, copy, or move any file
  under `.claw/`, the workspace tree, the operator's home
  directory, or anywhere else. The IDE adapter MUST NOT introduce
  its own on-disk caches; if it caches in memory it MUST clearly
  mark such state as adapter-local and non-authoritative.
- **`.claw/**` parsing is not an IDE responsibility.** The IDE
  adapter MUST NOT read or parse any `.claw/**` file directly. The
  status command is the only mediated read; `.claw/**` files are
  inspected by the operator opening them through the IDE host's
  normal file-open behavior when they click an `evidence_paths`
  entry.
- **Trust-this-workspace mode is not an IDE responsibility.** The
  IDE adapter MUST NOT offer a "trust" mode, a "trusted workspace"
  toggle, or any other persistent affordance whose effect is to
  loosen STOP visibility, the refresh cadence, the disposable-
  workspace expectations (§15), or any other safety property.
- **Filesystem watching is not an IDE responsibility.** The IDE
  adapter MUST NOT subscribe to filesystem watchers, Git event
  streams, daemon push channels, IDE-host file-change events, or
  any notification surface that would let it refresh, render, or
  act without an explicit operator gesture (§14).

## 8. Allowed Reads

The future IDE adapter implementation lane may read, and only read,
the following:

- stdout of `claw plan status <workspace>` (success envelope, exit
  `0`).
- stdout of `claw plan status <workspace> <approval-result.json>`
  (success envelope, exit `0`).
- stdout of `claw plan status <workspace> [<approval-result.json>]`
  refusal envelope (exit `EXIT_STATUS_REFUSED == 12`).
- exit code of the above invocations.
- operator-selected inputs: the workspace path the operator named,
  the optional approval-result path the operator named, and an
  operator-triggered refresh event.
- the IDE adapter's own in-memory state for the duration of a
  single rendered panel session (parsed envelope, copy-to-clipboard
  payload, collapsed/expanded view state). This state is adapter-
  local and non-authoritative.

The IDE adapter may **not** read:

- any `.claw/**` file directly; read is mediated exclusively through
  the status command's stdout.
- any other workspace file directly. If the operator opens an entry
  from `evidence_paths` in their editor, that read is performed by
  the IDE host's editor, not by the IDE adapter.
- broker endpoints, model endpoints, Ollama endpoints, or any HTTP
  surface.
- secrets, environment variables, the operator's shell history, the
  operator's terminal state, or any non-workspace file beyond what
  `claw plan status` itself already reads.
- prior IDE adapter session state on disk (the IDE adapter MUST NOT
  introduce an on-disk cache, an IDE-workspace-storage envelope
  cache, or any persistent envelope-derived state).
- IDE-host telemetry channels, analytics surfaces, error-reporting
  endpoints, or any other observability channel.

The IDE adapter explicitly does **not** subscribe to a filesystem
watcher, a Git event stream, a daemon push channel, an IDE-host
file-change event, or any notification surface that would let it
act without an operator-initiated refresh (§14).

## 9. Forbidden Actions

The future IDE adapter implementation lane is forbidden from
performing any of the following. Each forbidden action maps to a
named safety property the chain depends on.

- spawning `claw plan run`, `claw plan approve`, `claw plan
  apply-bundle`, or `claw plan apply` as a subprocess for any
  reason — including diagnostic, "dry-run", or "shadow" reasons
- spawning any process other than the read-only `claw plan status`
  command, and only with no flags and at most the two A2-L2d
  positional arguments
- pre-filling, auto-completing, generating, hashing, signing, or
  otherwise producing the TTY approval line `apply <step_id>
  <preview_sha256>` in any channel — visible or hidden
- producing or persisting `<approval-result.json>` on the operator's
  behalf, anywhere on disk
- producing or persisting `apply-bundle.json` on the operator's
  behalf, anywhere on disk
- modifying any file under `.claw/`, the workspace tree, the
  operator's home directory, the IDE host's workspace-storage
  directory, the IDE host's settings directory, or anywhere else,
  except to render UI state in memory
- composing `approve` and `apply` into a single IDE action, whether
  by chained subprocess, sequenced UI gesture, scripted shortcut,
  plug-in, macro, recorded action, IDE task, workspace task, IDE-
  host run-configuration, or any other mechanism
- offering a "one-click", "auto", "fast", "express", "skip",
  "trust", "approve-and-apply", "approve-when-X", or "preapprove"
  mode that elides any operator gesture the canonical chain
  requires
- offering an "approve" button, command-palette entry, keybinding,
  context-menu item, gutter affordance, lens, hover action, drag
  target, status-bar action, or any equivalent affordance
- offering an "apply" button or any equivalent affordance
- offering a "run" button or any equivalent affordance
- offering an "approve + apply" composite affordance
- offering an "approve all pending previews" or any other batch-
  approval affordance
- offering an "automatic-approval" or "automatic-apply" setting,
  preference, workspace setting, or environment-driven configuration
- adding write-adjacent flags to the IDE adapter UI that map onto
  refused CLI flags (`--apply`, `--approve`, `--yes`, `--auto`,
  `--clean`, `--rollback`, `--mutate`, `--all-runs`, `--no-prompt`,
  `--skip-approval`, `--cache`); UI affordances that compose to the
  same semantic effect as those flags are equally forbidden
- popping a modal, inline-input prompt, command-palette text field,
  or any other in-IDE input surface that captures the operator's
  approval line and forwards it to `claw plan approve` — TTY
  enforcement is the approval boundary; an IDE input is not a TTY
- calling broker, model, Ollama, telemetry, analytics, error-
  reporting, IDE-host marketplace endpoints, or any other network
  endpoint at any phase of IDE adapter operation
- caching envelope contents on disk in any form (`.claw/l3-ide-
  cache/`, `~/.cache/claw-ide/`, IDE workspace storage, IDE global
  storage, IDE secret storage, IDE settings.json, or anywhere
  else)
- watching the filesystem for `.claw/**` changes and refreshing
  without operator gesture
- subscribing to IDE-host file-change events, Git event streams,
  daemon push channels, or any notification surface that would
  trigger refresh without operator gesture
- background-polling `claw plan status` on any timer
- summarizing, collapsing, debouncing, rate-limiting, snoozing,
  muting, dismissing, ignoring, or otherwise attenuating any STOP
  signal carried by the envelope (§12)
- introducing parallel status schemas, parallel envelope versions,
  or "extended" envelopes that wrap `a2-l2d-status.v1` with IDE-
  specific fields and re-emit them as authoritative
- normalizing an unknown enum value into a known one (e.g. coercing
  an unknown `stop_condition` to `null`, or coercing an unknown
  `phase` to `unknown` instead of surfacing the unknown literal)
- displaying chain state in a way that suggests adapter authority
  over the chain (panel headers like "Chain Controller", "Apply
  Manager", "Approval Helper", "Chain Director" are category
  violations; "Chain Status", "Read-Only Observer", or "Chain
  Inspector" is accurate framing)
- treating the envelope as authoritative for write decisions; the
  chain re-validates every input at apply time
  ([`a2-l2b-run-plan-preview-operator-handoff.md` §6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))
  and the IDE adapter MUST NOT pretend otherwise
- operating against `/home/suki/stack-code`, `/home/suki/sidestackai`,
  or any production repository in any default test or default
  configuration fixture (the IDE adapter implementation lane MUST
  enumerate the disposable workspace expectations applicable to its
  surface; see §15)

## 10. IDE Surface Contract

The IDE adapter surface contract pins what the IDE adapter MAY
expose to the operator and what it MUST NOT expose. The contract is
phrased in IDE-host-neutral terms: the implementation lane chooses
the host (VS Code extension, JetBrains plugin, language-server-
backed panel, web-based IDE, or other) and the host's native
affordance vocabulary, but the surface contract holds across all
hosts.

### IDE input

The IDE adapter may consume only the following inputs:

```text
claw plan status stdout (success envelope, exit 0)
claw plan status stdout (refusal envelope, exit 12)
claw plan status exit code
operator-selected workspace path
optional operator-selected approval-result path
operator-triggered refresh event
```

No other input is in scope. Specifically: no filesystem-watcher
event, no Git event, no IDE-host file-change event, no daemon
push, no timer, no broker message, no model message, no telemetry
input, no remote configuration, no auto-discovery of workspaces,
no auto-discovery of approval-result paths.

### IDE output

The IDE adapter may render the following fields and affordances:

```text
schema_version (verbatim)
workspace_root (verbatim)
run_id (verbatim, null-aware)
step_id (verbatim, null-aware)
phase (verbatim, closed-enum value)
next_operator_command (verbatim, as copyable text)
is_approvable (verbatim, boolean)
is_apply_ready (verbatim, boolean)
before_sha256 (verbatim, null-aware)
after_sha256 (verbatim, null-aware)
payload_sha256 (verbatim, null-aware)
live_target_sha256 (verbatim, null-aware)
stop_condition (verbatim, closed-enum value, null-aware)
evidence_paths (verbatim list, each as a local file link)
audit_markers (verbatim list, closed-enum members)
read_only_invariant (verbatim, pinned literal)
raw envelope JSON (collapsible, supplementary)
diagnostic message (supplementary, non-load-bearing)
refresh affordance (single explicit operator control)
copy-to-clipboard affordance for next_operator_command
copy-to-clipboard affordance for individual evidence_paths entries
```

No other output is in scope. Specifically: no synthetic pass/fail
pill, no "Ready to Apply" composite indicator, no "Healthy" /
"Unhealthy" pill that subsumes `stop_condition`, no aggregate
"chain progress" gauge derived from multiple fields, no IDE-host
notification badge whose state is not equal to a single envelope
field, no preview-content rendering, no diff rendering against
`evidence_paths` files, no IDE-side hash computation against
`evidence_paths` files.

### IDE forbidden controls

Future IDE adapter implementation MUST NOT include any of:

```text
approve button
apply button
apply-bundle button
run button
approve-and-apply button
approve-when-X rule
automatic-approval setting
automatic-apply setting
batch approval action
preapproval mechanism
one-click continue affordance
trust-this-workspace mode
ignore STOP button
mute STOP button
snooze STOP button
dismiss STOP button
hide STOP affordance
"chain controller" framing
"apply manager" framing
"approval helper" framing
inline approval-line input field
inline approval-line modal
IDE task that invokes any write step
IDE run-configuration that invokes any write step
keybinding bound to any write step
command-palette entry bound to any write step
context-menu action bound to any write step
gutter / lens / hover action bound to any write step
status-bar action bound to any write step
drag-target action bound to any write step
```

Affordances that compose to the same semantic effect as any of the
above are equally forbidden, regardless of how the host exposes
them.

## 11. Display Rules For Status Fields

The IDE adapter rendering MUST honor the following per-field display
rules. Each rule is derived from the A2-L2d schema-of-record and the
A2-L3 adapter boundary card; this card pins them in IDE-rendering
terms.

- **`schema_version`** — verbatim. Any literal other than
  `a2-l2d-status.v1` is a STOP signal (§13); the IDE adapter MUST
  refuse to best-effort-parse the envelope.
- **`workspace_root`** — verbatim. The IDE adapter MUST NOT
  canonicalize, expand, normalize, or re-resolve the path; the
  envelope's value is what gets rendered.
- **`run_id`** / **`step_id`** — verbatim or, when null, rendered
  as a distinguishable "(none)" / "(no run)" placeholder. The
  placeholder MUST NOT be confusable with a real run identifier; an
  empty-string render is forbidden.
- **`phase`** — verbatim closed-enum value. The IDE adapter MAY
  apply visual styling per phase (color, icon, label) but MUST NOT
  drop the underlying literal value from the rendering. A "phase
  pill" that displays only an icon and no text is forbidden.
- **`next_operator_command`** — verbatim, as copyable text. See §15
  for copy-to-clipboard rules. The IDE adapter MUST NOT translate,
  truncate, or pre-decorate the string (no "click to run", no
  "execute in terminal" suffix).
- **`is_approvable`** / **`is_apply_ready`** — verbatim boolean. The
  IDE adapter MAY render these as styled chips ("Approvable",
  "Apply ready") but MUST NOT use them to gate any UI affordance
  whose effect is a write step. A `true` value never unlocks any
  approve/apply UI; it is read-only state.
- **`before_sha256`** / **`after_sha256`** / **`payload_sha256`** /
  **`live_target_sha256`** — verbatim hex strings, or null-aware
  placeholders. The IDE adapter MAY render these in a collapsed
  "SHA detail" section but MUST surface them on operator request
  without further user interaction beyond the disclosure gesture.
- **`stop_condition`** — verbatim closed-enum value when non-null;
  null-aware placeholder when null. See §12 for STOP-rendering
  rules. The IDE adapter MUST NOT substitute friendly text for the
  closed-enum value (e.g. rendering `payload-sha-mismatch` as
  "Mismatch detected" is forbidden); the enum literal is the
  operator escalation signal.
- **`evidence_paths`** — verbatim list, sorted in the order the
  envelope carries. Each entry rendered as a local file link per
  §13. The IDE adapter MUST NOT deduplicate, reorder, truncate
  (beyond UI-host scroll limits whose overflow is the operator's
  responsibility to scroll past), or otherwise filter the list.
- **`audit_markers`** — verbatim list, sorted in the order the
  envelope carries. The IDE adapter MUST surface every marker; it
  MUST NOT drop, summarize, or coalesce markers. An unknown marker
  is itself a STOP signal (§13).
- **`read_only_invariant`** — verbatim pinned literal. The IDE
  adapter MUST surface this on every rendered envelope; absence or
  substitution is a STOP signal in its own right.

## 12. STOP Condition Visibility Rules

The A2-L2b chain's safety is reasoned about in terms of STOP gates
that the operator must observe and escalate on
([`a2-l2b-run-plan-preview-operator-handoff.md` §8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)).
A2-L2d surfaces those STOPs through the `stop_condition` enum, the
`next_operator_command: "STOP — escalate"` literal, the `phase`
values `non_approvable` / `rolled_back` / `unknown`, and the
`a2-l2d-status-stop-condition-detected` / `a2-l2d-status-refused`
markers. The A2-L3 adapter boundary card §11 pinned the cross-
adapter STOP-visibility rules; the harness adapter scope card §11
pinned them for machine-facing consumers. This section pins them
for the IDE adapter's visual surface.

The IDE adapter MUST:

- **render STOP verbatim** — every closed-enum value
  (`payload-sha-mismatch`, `live-target-missing`,
  `live-target-sha-changed`, etc.) is rendered as the exact enum
  literal. Substituting friendly text is forbidden; the enum value
  IS the operator escalation signal.
- **show STOP with at least equal prominence to non-STOP rendering**
  — STOP rendering is at minimum visually or programmatically
  prominent as the corresponding non-STOP rendering. A green
  "Healthy" pill with a high-contrast surface paired with a red
  "Blocked" pill with a low-contrast surface is a category
  violation. The implementation lane chooses concrete styling but
  MUST satisfy parity.
- **retain STOP across refresh until status changes** — once a STOP
  is rendered, the rendering MUST persist until a new refresh
  (operator-initiated, per §14) produces an envelope whose STOP
  state has changed. The IDE adapter MUST NOT clear a STOP
  rendering without a refresh.
- **render the refresh-cleared STOP only when the new envelope
  no longer carries it** — if the operator manually refreshes and
  the new envelope still carries the same STOP, the IDE adapter
  MUST continue to render the STOP. STOP is only cleared from the
  display when a refresh produces an envelope with no
  `stop_condition` and no STOP-bearing phase.
- **treat unknown enum values as STOP** — any `phase`,
  `stop_condition`, `next_operator_command` shape, or
  `audit_markers` entry not in the A2-L2d closed enums is itself a
  STOP signal. The IDE adapter MUST render the unknown literal
  verbatim, classify the panel as STOP, and present the unknown-
  value diagnostic at parity with known STOPs.
- **never replace STOP with non-load-bearing prose** — a STOP
  rendering MUST include the closed-enum value verbatim. A
  rendering that displays only "Something went wrong" or "Action
  required" without the underlying `stop_condition` literal is a
  category violation.
- **never hide `evidence_paths`** — when a STOP fires,
  `evidence_paths` MUST remain visible to the operator without
  further user interaction beyond opening the panel. A collapsed-
  by-default evidence-paths section that hides the list under a
  disclosure is a category violation; the list itself is the
  primary operator diagnostic.
- **never debounce STOP signals** — if two successive operator-
  initiated refreshes produce the same STOP signal, the IDE
  adapter MUST render the STOP both times. The IDE adapter MUST
  NOT collapse "same STOP twice in a row" into a single rendered
  event, MUST NOT throttle STOP visibility, and MUST NOT use any
  per-session "you've already seen this" affordance.
- **never offer a STOP-snooze affordance** — no snooze, mute,
  dismiss, ignore, or "remind me later" action for any STOP
  signal. The chain's safety depends on STOPs being seen, not
  cleared.
- **never down-classify STOP into "warning" or "info"** — the IDE
  adapter MUST NOT re-classify a STOP signal as a "warning", a
  "soft failure", a "tip", an "info banner", or any other lower-
  severity classification. STOP is STOP.
- **render every STOP-bearing field at full fidelity** — the
  closed-enum value, the `next_operator_command: "STOP —
  escalate"` literal, the `evidence_paths` list, and the
  `audit_markers` list are all rendered verbatim. None of these
  fields may be summarized, truncated, or redacted in any IDE-host
  log-level or production-mode setting.
- **never propagate STOP rendering to non-operator surfaces** — the
  IDE adapter MUST NOT forward STOP signals to telemetry,
  analytics, error-reporting, IDE-host marketplace dashboards, or
  any other surface beyond the operator-facing IDE rendering.

## 13. Evidence Path Rendering Rules

`evidence_paths` is the operator's primary STOP-diagnosis surface
([`a2-l2d-operator-quickref.md` §6](./a2-l2d-operator-quickref.md#6-stop-conditions)).
The IDE adapter MAY render each entry as a local file link the
operator can click to open in the IDE host's editor, subject to the
following constraints.

The IDE adapter MAY:

- render each entry as a clickable link whose action opens the file
  in the IDE host's editor (the same way any in-editor file link
  behaves).
- render the link's text verbatim as the envelope-carried path.
- render the path's existence indicator (present / missing) using
  IDE-host conventions, provided the indicator is derived from the
  IDE host's normal file-availability check and not from a custom
  IDE-adapter probe that reads `.claw/**` or workspace files
  outside the envelope-permitted scope.

The IDE adapter MUST NOT:

- read the file contents itself.
- preview the file contents in the IDE adapter panel.
- hash, sign, summarize, or otherwise process the file contents.
- rewrite the path before rendering (no canonicalization, no
  expansion, no normalization, no substitution).
- hide a missing file from the operator. If the path does not
  resolve, the IDE adapter MUST surface that state to the operator
  alongside the path itself.
- create a file at an `evidence_paths` location that does not
  exist.
- follow a path outside the workspace root without explicit
  rendering that calls out the out-of-workspace location to the
  operator. Out-of-workspace `evidence_paths` entries are
  technically possible (the optional approval-result path may live
  outside `<workspace>/.claw/**`); the IDE adapter MUST NOT silently
  open such entries as if they were workspace files.
- mutate, rename, delete, copy, or move any `evidence_paths` file.
- compose the file-open gesture with any other action (no
  "open-and-mark-reviewed", no "open-and-stage", no "open-in-
  read-only-mode-then-edit").
- offer "open all evidence paths" as a single gesture. Each path is
  opened individually by an explicit operator click.

## 14. Refresh / Polling Boundary

The IDE adapter is permitted to invoke `claw plan status` only in
response to an explicit operator gesture. The refresh boundary is
where IDE adapters are most prone to drifting into background
controllers; this section pins the rule set without escape hatches.

The IDE adapter MUST:

- refresh only on explicit operator action — the initial panel-open
  gesture, an operator-clicked "refresh" button, an operator-
  triggered command-palette entry, or an operator-bound keybinding.
- treat the refresh control as a single invocation per gesture. One
  click triggers one `claw plan status` subprocess, period.
- pass through `claw plan status` arguments verbatim — the
  workspace path the operator named, and (if the operator named
  one) the approval-result path. No additional arguments, no
  flags, no synthesized inputs.
- preserve the operator's cursor / selection / panel scroll
  position across a refresh whenever the host permits.

The IDE adapter MUST NOT:

- use filesystem watchers (chokidar, watchman, IDE-host file-change
  events, `fs::notify`, inotify, FSEvents, or any other).
- background-poll `claw plan status` on any timer.
- subscribe to daemon push channels, broker messages, Git event
  streams, or any notification surface that would trigger refresh
  without operator gesture.
- auto-refresh after STOP to clear it. A STOP rendering persists
  until the operator explicitly refreshes and observes a non-STOP
  envelope.
- batch refreshes (a single refresh gesture invokes the status
  command once, not multiple times for the same panel session).
- refresh as a side effect of any other IDE action (no "refresh on
  save", no "refresh on focus", no "refresh on Git pull", no
  "refresh on workspace change").
- offer an "auto-refresh every N seconds" setting, preference,
  workspace setting, or environment-driven configuration.

## 15. Copy-To-Clipboard Boundary

The IDE adapter MAY offer copy-to-clipboard affordances scoped to
single envelope fields. The boundary exists because copy actions
are the easiest path from a read-only display surface to a
composite write workflow; this section pins the rule set.

The IDE adapter MAY:

- offer a "copy `next_operator_command`" affordance whose only
  effect is placing the verbatim envelope-carried string on the
  system clipboard.
- offer a "copy this evidence path" affordance for each individual
  `evidence_paths` entry whose only effect is placing the verbatim
  path on the system clipboard.
- offer a "copy raw envelope JSON" affordance whose only effect is
  placing the canonical envelope JSON on the system clipboard.

The IDE adapter MUST NOT:

- compose multiple fields into a single composite paste payload
  (e.g. "copy approval line preformatted for the terminal", "copy
  full chain command sequence", "copy approve-then-apply
  preformatted").
- alter the copied payload (no decoration, no terminal-prefixing,
  no shell-quoting changes, no path-canonicalization, no SHA
  insertion).
- chain the copy gesture into any other action (no "copy and open
  terminal", no "copy and run in terminal", no "copy and switch
  focus to terminal", no "copy and prompt for confirmation").
- persist the clipboard payload anywhere other than the system
  clipboard (no IDE-side clipboard history, no audit log of copy
  events that includes the payload, no analytics event that
  captures the payload).
- offer "copy and execute" as any combined gesture.
- offer "copy approval line" — the approval line is not an envelope
  field; composing it inside the IDE adapter is forbidden.

## 16. Disposable Workspace Handling

The IDE adapter operates against workspaces the operator is already
editing in their IDE host. Unlike the harness adapter, the IDE
adapter does **not** classify workspaces itself; classification is a
machine-facing concern handled by the harness adapter
([`a2-l3-harness-adapter-scope-card.md` §14](./a2-l3-harness-adapter-scope-card.md#14-disposable-workspace-requirement),
[`a2-l3-harness-adapter-implementation-scope-card.md` §11](./a2-l3-harness-adapter-implementation-scope-card.md#11-disposable-workspace-classification-design)).

That said, the IDE adapter MUST honor the underlying disposability
expectation:

- **No surface that implies operator-side classification override.**
  The IDE adapter MUST NOT expose a "this workspace is disposable"
  toggle, a "trusted workspace" setting, or any operator-facing
  affordance whose effect would loosen STOP visibility or refresh
  cadence based on a disposability claim.
- **No surface that implies cross-workspace authority.** The IDE
  adapter MUST NOT display chain state for workspaces other than
  the one the operator explicitly named. No cross-workspace
  dashboard, no workspace-list pill, no aggregated panel.
- **Display-only disposability indicator.** If A2-L2d evolves in a
  future separately-scoped lane to add a disposability field to the
  envelope, the IDE adapter MAY render that field verbatim as a
  read-only indicator. Until such a field exists, the IDE adapter
  surfaces no disposability indicator at all. The IDE adapter MUST
  NOT invent its own disposability indicator from the workspace
  path or any other heuristic.
- **No different behavior between disposable and non-disposable.**
  Every rule in this card holds regardless of the workspace's
  disposability. STOP visibility, refresh cadence, copy-to-clipboard
  scope, evidence-path rendering, and forbidden actions are
  identical for disposable and non-disposable workspaces.

The IDE adapter implementation lane MUST NOT take a Cargo
dependency on the harness adapter's disposable-workspace classifier
crate; if a future shared classifier emerges, that is a separate
scope-card lane.

## 17. Security / Secrets Boundary

The IDE adapter MUST NOT read, log, persist, or relay any of:

- environment variables (the IDE adapter does not need any
  environment variables to invoke `claw plan status`; the producer
  reads what it needs from the OS environment, not from the IDE
  adapter).
- the operator's shell history.
- the operator's terminal state.
- the operator's home directory beyond what the IDE host already
  reads as part of normal IDE operation.
- any secret material from `.claw/**` (none should exist there; the
  IDE adapter still MUST NOT emit such material if it does, and
  MUST NOT render it in the raw-envelope view if it appears).
- the operator's git config, credentials, SSH keys, or GPG keys.
- broker, model, or Ollama API keys or tokens.
- IDE host secret-storage values, IDE host marketplace tokens, or
  IDE host telemetry tokens.

The IDE adapter MUST NOT relay envelope contents to:

- telemetry endpoints (IDE-host or third-party).
- analytics endpoints.
- error-reporting endpoints.
- IDE-host marketplace dashboards.
- broker, model, or Ollama endpoints.
- any network endpoint at any phase of IDE adapter operation.

The IDE adapter implementation lane MUST ensure the IDE adapter's
panel rendering, clipboard payloads, and in-memory state are free
of any caller-secret material derived from anything other than the
envelope itself. The envelope contains no secrets by A2-L2d
construction, but the IDE adapter MUST NOT introduce a code path
that injects secrets from any other source into its rendering.

## 18. Safety Invariants

The IDE adapter implementation lane must preserve, verbatim, every
property the prior lanes pinned. These are the same invariants the
A2-L3 adapter boundary card §12 and the A2-L3 harness adapter
scope card §15 pin, restated here for the IDE surface:

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
- A2-L2d non-overlapping marker invariant (`a2-l2d-*` only; the IDE
  adapter MUST NOT invent `a2-l3-*` markers that leak back into the
  status producer)
- A2-L2d non-overlapping exit-code invariant
- A2-L3 adapter read-only invariant: the IDE adapter mutates no file
  and generates no network egress beyond the `claw plan status`
  subprocess it spawns
- A2-L3 adapter STOP-visibility invariant: every STOP signal in an
  envelope reaches the operator at the granularity the envelope
  carries (§12)
- A2-L3 adapter no-write-surface invariant: no IDE input, output,
  configuration, control, keybinding, command-palette entry,
  context-menu action, gutter affordance, lens, hover action,
  status-bar action, drag target, or composition thereof produces a
  write action against the A2-L2b chain
- A2-L3 adapter no-state-invention invariant: the IDE adapter
  renders the envelope it received; it does not synthesize chain
  state
- A2-L3 adapter no-shadow-contract invariant: the IDE adapter
  consumes `a2-l2d-status.v1` as-is; it does not wrap it, extend
  it, or parallel-version it

In addition, the IDE adapter adds these surface-specific invariants:

- **IDE subprocess-bounded invariant.** The only subprocess the IDE
  adapter spawns is `claw plan status` with at most the two
  positional arguments the A2-L2d schema defines, with no flags.
- **IDE refresh-bounded invariant.** Every status invocation is
  triggered by an explicit operator gesture; no filesystem watcher,
  daemon push, IDE-host file-change event, timer, or background
  poller initiates a refresh.
- **IDE copy-bounded invariant.** Copy-to-clipboard affordances are
  scoped to single envelope fields and place verbatim payloads on
  the system clipboard with no further action.
- **IDE STOP-loud invariant.** Every STOP signal rendered in the IDE
  panel is at least as visually prominent as the corresponding
  non-STOP rendering; no down-classification, no hiding, no
  snoozing.
- **IDE no-classifier-override invariant.** The IDE adapter never
  classifies workspaces and never exposes operator-side
  classification overrides; every safety rule holds regardless of
  workspace disposability.
- **IDE no-cross-workspace-authority invariant.** The IDE adapter
  renders state for the workspace the operator explicitly named;
  it never aggregates state across workspaces.

## 19. Non-Goals

The IDE adapter at this scope must not:

- implement the IDE adapter (deferred; this lane is docs-only)
- implement a harness adapter, an IDE-host plugin manifest, an IDE
  host extension package, or any operator-runnable artifact
- introduce or imply autonomous workspace-write execution
- introduce IDE controls that approve, that apply, that apply-
  bundle, that run, or that compose any combination of those
- introduce IDE-driven retry, remediation, or rollback of any chain
  step
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  pre-approval, batch approval, or any approval-bypass affordance
  on the IDE surface
- introduce a "fast", "shadow", "what-if", "preview-this", or
  "dry-run" mode that simulates downstream chain commands
- introduce a "trust this workspace" setting, a "trusted workspace"
  toggle, or any operator-facing affordance that loosens STOP
  visibility or refresh cadence
- introduce IDE-host file-change-event subscriptions, filesystem
  watchers, daemon channels, or background refresh
- introduce on-disk caches of envelope contents
- introduce cross-workspace dashboards, multi-run inventories, or
  history rollups
- introduce a parallel adapter contract for the IDE (the IDE adapter
  consumes `a2-l2d-status.v1` as-is, with no IDE-specific schema
  wrapper, marker, or envelope-version variant)
- introduce a Cargo dependency on `rust/crates/a2-harness-adapter/`
- introduce shared crates between the harness adapter and the IDE
  adapter without a separate scope-card lane
- introduce a CLI subcommand on `claw plan` for IDE operations
- modify `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, `claw plan apply`, or `claw plan status` behavior, exit
  codes, schemas, markers, or JSON field shapes
- modify `a2-l2b-*` or `a2-l2d-status.v1` schema versions or marker
  constants
- introduce an `a2-l3-*` schema, marker, exit code, or CLI surface
- call broker, model, or Ollama at any phase
- relay envelope contents to IDE-host telemetry, analytics, error-
  reporting, or marketplace endpoints
- weaken any A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter boundary, A2-L3
  harness adapter, or A2-L3 harness adapter implementation STOP
  gate
- run against `/home/suki/stack-code`, `/home/suki/sidestackai`, or
  any production repository in any default test, fixture, or
  packaged operator-facing artifact

Any of the above must be opened as a separate, explicitly-
authorized lane.

## 20. Future Implementation Constraints

When the IDE adapter implementation lane is opened as a separate
scope card, it must hold to all of the following. This card pins
the boundary; the implementation scope card pins the concrete
touched surfaces and validation matrix.

### IDE input — implementation lane bound

Future IDE adapter implementation may consume only:

```text
claw plan status stdout
claw plan status exit code
operator-selected workspace path
optional operator-selected approval-result path
operator-triggered refresh event
```

No other input is in scope. The implementation scope card MUST
restate this enumeration and MUST add an assertion test that the
implementation refuses any other input path at integration time.

### IDE output — implementation lane bound

Future IDE adapter implementation may render only:

```text
phase
next_operator_command as copyable text
is_approvable
is_apply_ready
stop_condition
evidence_paths
audit_markers
read_only_invariant
raw status JSON
diagnostic message
schema_version
workspace_root
run_id
step_id
before_sha256 / after_sha256 / payload_sha256 / live_target_sha256
refresh affordance
copy-to-clipboard affordance scoped to single envelope fields
```

No other output is in scope. Specifically: no synthetic pass/fail
pill, no aggregate "chain progress" gauge, no IDE-host notification
badge whose state is not equal to a single envelope field, no
preview-content render, no diff render against `evidence_paths`
files, no IDE-side hash computation against `evidence_paths` files.

### IDE forbidden controls — implementation lane bound

Future IDE adapter implementation must not include any of:

```text
approve button
apply button
apply-bundle button
run button
approve-and-apply button
automatic-approval setting
automatic-apply setting
batch approval
preapproval
one-click continue
trust this workspace mode
ignore STOP button
mute STOP button
dismiss STOP button
```

(Per §10, this is the canonical forbidden-control set the
implementation scope card MUST restate. Surface- and host-specific
extensions to the forbidden set are encouraged.)

### IDE STOP rendering — implementation lane bound

Future IDE adapter implementation must:

```text
render STOP verbatim
show STOP with at least equal prominence to non-STOP
retain STOP across refresh until status changes
treat unknown enum values as STOP
never replace STOP with friendly non-load-bearing prose
never hide evidence_paths
```

(Per §12, this is the canonical STOP-rendering rule set.
Implementation scope-card validation MUST include golden tests for
every closed-enum STOP case and an unknown-enum synthetic fixture.)

### IDE refresh boundary — implementation lane bound

Future IDE adapter implementation must:

```text
refresh only on explicit operator action
never use filesystem watchers
never run background polling
never run daemon push channels
never auto-refresh after STOP to clear it
```

(Per §14. The implementation scope card MUST include a test
asserting the IDE adapter never spawns `claw plan status` outside
an operator gesture and MUST include a static-grep guard against
filesystem-watcher crates such as `notify` / `chokidar` / equivalent
IDE-host watcher APIs.)

### IDE file-link boundary — implementation lane bound

Future IDE adapter implementation may display `evidence_paths` as
local file links, but:

```text
must not edit the files
must not rewrite the paths
must not hide missing files
must not create files
must not follow paths outside workspace without explicit warning
```

(Per §13. The implementation scope card MUST include tests that
exercise out-of-workspace `evidence_paths` entries — at minimum the
operator-supplied approval-result path that may live outside
`<workspace>/.claw/**` — and assert the IDE adapter renders an
explicit warning surface for the operator.)

### IDE host surface — implementation lane bound

The IDE adapter implementation scope card MUST:

- enumerate the concrete IDE host(s) targeted (e.g. VS Code
  extension, JetBrains plugin, language-server-backed panel).
- enumerate the concrete affordances exposed in each host
  (panels, command-palette entries, context-menu items, etc.).
- enumerate, per host, the keybindings the operator may bind to
  the refresh and copy actions, and assert no keybinding is bound
  to any forbidden control.
- enumerate the concrete crates / packages / files the
  implementation lane will touch.
- enumerate the validation matrix that proves the IDE adapter
  honors every rule in §§6–18.
- include a static-grep guard against approve/apply/run/apply-
  bundle invocations anywhere in the IDE adapter source.
- include a static-grep guard against filesystem-watcher and
  background-polling APIs.
- include a static-grep guard against IDE-host telemetry,
  analytics, marketplace dashboard, and error-reporting APIs.
- include a static-grep guard against persistent operator-toggleable
  approve/apply-adjacent settings.
- include tests asserting the IDE adapter spawns only
  `claw plan status` with at most two A2-L2d positional arguments
  and no flags.
- include tests asserting the IDE adapter writes no file outside
  in-memory rendering state (no on-disk cache, no IDE workspace
  storage, no IDE global storage).
- include tests asserting every closed-enum `phase`,
  `stop_condition`, `next_operator_command`, and marker value is
  rendered verbatim under the §§11–12 display rules.
- include tests asserting the refusal envelope (exit `12`) is
  rendered with full fidelity including `a2-l2d-status-refused`.

### IDE allowed touched surfaces — implementation lane bound

The IDE adapter implementation scope card MUST enumerate the
allowed file paths it touches. Likely surfaces (deferred to that
scope card) include:

- a new IDE-host extension package under a yet-to-be-named path
  (e.g. `ide/<host>/claw-status-panel/`), with its own manifest and
  source files.
- new tests under that package's test suite.
- new documentation under `docs/` (likely `docs/a2-l3-ide-adapter-
  usage.md` as the operator-facing companion).
- optional one-line README cross-link.

The implementation lane MUST NOT touch any file outside the
enumeration, and the enumeration MUST explicitly forbid every
A2-L2b module, every A2-L2b/A2-L2d schema constant, every A2-L2b/
A2-L2d marker constant, `rust/crates/a2-plan-runner/src/status.rs`,
`rust/crates/a2-harness-adapter/**`, and every `rust/crates/`
workspace crate that is not the IDE adapter's own.

### IDE forbidden surfaces — implementation lane bound

The implementation scope card MUST explicitly forbid touching:

- `rust/crates/a2-plan-runner/src/**` (every A2-L2b/A2-L2d module
  and constant)
- `rust/crates/a2-plan-runner/tests/**`
- `rust/crates/a2-harness-adapter/**` (the harness adapter remains
  authoritative on its own surface)
- `rust/crates/rusty-claude-cli/src/**` and `tests/**`
- `rust/crates/api/**`, `commands/**`, `compat-harness/**`,
  `mock-anthropic-service/**`, `plugins/**`, `runtime/**`,
  `telemetry/**`, `tools/**`
- `wrappers/**`, `bin/**`, `examples/**`, `scripts/**`,
  `Makefile`, `justfile`
- `.github/workflows/**` (existing workflows will pick up new files
  via member addition / IDE-host packaging conventions; workflow
  changes are a separate scope-card lane)
- `SideStackAI/**` (out of scope by the cross-project boundary the
  operator pinned)
- `.claw/**` in any repository, including any workspace the IDE
  adapter renders

### IDE validation matrix — implementation lane bound

The IDE adapter implementation lane MUST pass the following CI
matrix before merge. The matrix is enforced by existing workspace
CI plus new in-package tests and grep guards.

| Check | Mechanism | Mandatory |
|-------|-----------|-----------|
| existing workspace CI (fmt, clippy, test) | existing workflow | yes |
| docs source-of-truth | existing workflow | yes |
| shell tests | existing workflow | yes |
| STOP-rendering golden matrix (every closed enum + unknown) | new in-package tests | yes |
| refusal-envelope rendering golden test | new in-package test | yes |
| operator-gesture-only refresh test | new in-package test | yes |
| no-filesystem-watcher test (static grep + dependency audit) | new in-package CI step | yes |
| no-background-polling test (timer audit) | new in-package CI step | yes |
| copy-to-clipboard boundary test (single-field-only) | new in-package test | yes |
| evidence-path rendering test (in-workspace + out-of-workspace) | new in-package tests | yes |
| forbidden-control static-grep guard (approve / apply / run / apply-bundle / automatic-approval / batch / trust) | new in-package CI step | yes |
| no-telemetry / no-network static-grep guard | new in-package CI step | yes |
| filesystem-write sentinel test | new in-package test | yes |
| subprocess-bounded test (`claw plan status` only) | new in-package test | yes |
| `.claw/**` no-read test (envelope-mediated only) | new in-package test | yes |
| read_only_invariant verbatim rendering test | new in-package test | yes |
| schema-version refusal test | new in-package test | yes |
| no-cross-workspace-aggregation test | new in-package test | yes |
| no-Cargo-dependency-on-a2-harness-adapter test | new in-package CI step | yes |

Each check is a hard gate. Skipping any check in the implementation
lane is a STOP gate. Implementation lane MAY add additional checks
beyond this minimum.

## 21. Definition Of Done

This **scope card** is done when:

- `docs/a2-l3-ide-adapter-scope-card.md` exists and matches the
  sectional structure of this card.
- The card defines IDE adapter responsibilities and non-
  responsibilities in non-softening language.
- The card pins the IDE surface contract (input, output, forbidden
  controls), the per-field display rules, the STOP visibility
  rules, the evidence-path rendering rules, the refresh/polling
  boundary, the copy-to-clipboard boundary, the disposable-
  workspace handling, the security/secrets boundary, the safety
  invariants, the non-goals, and the future implementation
  constraints.
- The card pins the safety invariants without escape hatches.
- The card declares the A2-L3 IDE adapter as docs-only at this
  scope-card stage.
- The card explicitly states it authorizes design only; it does
  not authorize IDE implementation, adapter implementation,
  approve/apply UI controls, or autonomous workspace-write
  execution.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter boundary, A2-L3
  harness adapter, A2-L3 harness adapter implementation, or PR43
  preservation STOP gate is weakened.
- A single cross-link line MAY be added to the A2-L3 adapter
  boundary scope card, the A2-L3 harness adapter scope card, the
  A2-L3 harness adapter implementation scope card, the A2-L3
  harness adapter usage guide, the A2-L2d scope card, the A2-L2d
  status schema, or the A2-L2d operator quick reference if an
  obvious location exists, but no such cross-link is required for
  this scope card itself to land. *(This scope card is authored
  without cross-links to keep the lane strictly limited to a single
  new docs file; cross-links may be added in a follow-up lane.)*
- The card is reviewed by the operator before any IDE adapter
  implementation scope-card lane is opened.

The IDE adapter **implementation scope card** is out of scope for
this card. Definition of done for that lane will be authored when
its own scope card is created, bounded by the constraints in
§§6–20 above.

The IDE adapter **implementation lane** is doubly out of scope: it
is bounded by both this card and the not-yet-authored IDE adapter
implementation scope card. No code, no manifest, no wrapper, no
test, no host-package descriptor, and no runtime artifact lands
under either of those lanes without explicit operator review.

## 22. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **IDE adapter implementation scope-card lane (docs-only)** —
> author a concrete implementation scope card for the IDE adapter
> that enumerates the targeted IDE host(s) (VS Code, JetBrains,
> language-server-backed panel, or other), the concrete affordances
> exposed in each host (panel, command-palette entry, context-menu
> item, keybinding, copy gesture, refresh gesture), the allowed
> touched surfaces (likely a new IDE-host extension package and
> tests), the forbidden surfaces (per §20), the concrete validation
> matrix (per §20), the STOP-rendering golden-test matrix, the
> refresh-gesture-only test, the copy-to-clipboard boundary test,
> the evidence-path rendering tests, the no-telemetry / no-network
> guards, and the disposable-workspace surfacing rules. Do not
> author the IDE implementation in the same lane as its
> implementation scope card.

The lane *after* the IDE adapter implementation scope card lands is:

> **The IDE adapter implementation lane (code-bearing, per IDE
> host)** — implement the IDE adapter for exactly one IDE host
> under the constraints pinned by this card and that IDE host's
> implementation scope card, with golden tests for every STOP
> signal, every phase, the refusal envelope, every unknown-enum
> fixture, and the refresh/copy/file-link boundaries. The
> implementation lane MUST NOT expand the contract; any contract
> gap discovered during implementation is escalated as a separate
> scope-card lane against A2-L2d.

Neither lane permits autonomous workspace-write execution. Neither
lane permits approve / apply / apply-bundle UI controls. Both
remain bounded by the A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter
boundary, A2-L3 harness adapter, and A2-L3 harness adapter
implementation safety properties and by §§6–20 of this card.

## 23. References

- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 Adapter Boundary Scope Card; the cross-adapter
  constraints any per-adapter scope card must hold to. Section 8
  ("IDE Boundary") is the upstream preamble this scope card
  expands.
- [`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md)
  — A2-L3 Harness Adapter Scope Card; the sibling per-adapter
  scope card whose structure this card mirrors for the IDE surface.
- [`a2-l3-harness-adapter-implementation-scope-card.md`](./a2-l3-harness-adapter-implementation-scope-card.md)
  — A2-L3 Harness Adapter Implementation Scope Card; the worked
  example of a per-implementation scope card the IDE adapter
  implementation scope card will follow.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md)
  — A2-L3 Harness Adapter Usage Guide; the operator-facing companion
  to the merged harness adapter implementation, useful as a model
  for the future IDE adapter usage guide.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card; section 10 ("IDE / Harness Boundary") is the
  original preamble that A2-L3 expanded.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the
  contract the IDE adapter consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`. The
  at-the-keyboard companion to the contract the IDE adapter
  consumes.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference; TTY approval EOF note in §3 is
  load-bearing for the approval boundary the IDE adapter must
  never compose around.
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.
- PR #39 (`12fff14`) — A2-L2d scope card.
- PR #40 (`0f75800`) — A2-L2d read-only `claw plan status` command
  + `a2-l2d-status.v1`.
- PR #41 (`4c2b15e`) — A2-L2d operator quick reference.
- PR #42 (`21d9b5b`) — A2-L3 adapter boundary scope card.
- PR #44 (`f63d5ac`) — A2-L3 harness adapter scope card.
- PR #45 (`97e9d9b`) — A2-L3 harness adapter implementation scope
  card.
- PR #46 (`c171d11`) — A2-L3 read-only harness adapter crate.
- PR #47 (`90819e8`) — A2-L3 harness adapter usage guide.
- PR #48 (`2930d21`) — A2-L3 PR43 harness assertions preservation
  patch.

## 24. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- IDE adapter implementation: **not started; not authorized by this
  card**.
- IDE adapter implementation scope card authored: **no** (separate
  future lane).
- Harness adapter touched by this card: **no** (the merged harness
  adapter lanes remain authoritative on their own surface).
- Autonomous-write authorization: **none granted**.
- Approval / apply boundary weakened: **no**.
- Approve / apply UI controls authorized: **no**.
- Approve / apply / apply-bundle composition authorized: **no**.
- Background polling / filesystem watcher authorized: **no**.
- Direct `.claw/**` parsing authorized: **no**.
- IDE mutation of workspace files authorized: **no**.
- IDE mutation of `.claw/**` authorized: **no**.
- A2-L2b / A2-L2c / A2-L2d / A2-L3-boundary / A2-L3-harness STOP
  gate weakened: **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- A2-L3 adapter boundary card or A2-L3 harness adapter cards
  modified: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by an IDE adapter implementation scope-card lane
  bounded by §§6–20 above.
