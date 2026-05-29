# A2-L2d Scope Card — Read-Only Inspection / Status Contract (Docs-Only)

This document is a **design-only scope card** for the A2-L2d lane. It
describes what A2-L2d is, what it must not become, and the validation
required before any implementation lane is allowed to land. This file
itself authorizes **no runtime change, no CLI change, and no autonomous
workspace-write behavior**.

A2-L2d is the read-only continuation of the operator-gated A2-L2b chain.
A2-L2b proved the chain at runtime
([`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)).
A2-L2c (PRs #37 / #38,
[`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md) and
[`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md)) closed
the operator-ergonomics docs gap on top of that chain. A2-L2d closes the
remaining **read-only state-recovery and harness-readability** gap —
still docs-only at this scope-card stage.

## 1. Executive Summary

A2-L2d defines, in design only, a **read-only inspection / status
contract** for the A2-L2b preview-to-apply chain. It is the next safe
step between operator-driven CLI usage today and any future IDE or
harness surface. It is *not* an IDE lane, *not* a runtime lane, and
*not* an authorization for autonomous writes of any kind.

The recommended A2-L2d scope is:

> Define an `a2-l2d-status.v1` envelope (schema + future `claw plan
> status <workspace>` read-only command) that aggregates state from
> existing `.claw/l2b-*` artifacts, identifies the latest run / pending
> step, names the next allowed operator command, surfaces any active
> STOP condition from handoff section 8, and emits a stable, versioned,
> side-effect-free JSON contract for operator, harness, and (future) IDE
> consumption.

The implementation of A2-L2d is **not authorized by this scope card**.
This card defines the boundary the future implementation lane must hold
to. The next gate before implementation is operator review of this
scope card.

## 2. Problem Statement

The A2-L2b chain is runtime-proven and the A2-L2c quick reference makes
the operator path copy-pasteable, but two read-side gaps remain:

1. **Mid-chain state recovery is multi-file.** An operator returning to
   a workspace mid-chain must read at least:
   - `.claw/l2b-runs/<run-id>/status.json` (run pin)
   - `.claw/l2b-runs/<run-id>/run-manifest.json` (operator entry points)
   - `.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`
     (canonical SHAs, step id)
   - `.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-generator-result.json`
     (`is_binary` / `is_redacted` / `is_truncated`, payload SHA)
   - the captured approval-result JSON (the chain does not persist this
     under `.claw/`; operator-side capture only)
   - `.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json`
     (if generated)

   Each file is independently parseable, but there is no normalized
   view that answers "where am I in the chain and what may I do next?"
   in a single read.

2. **No stable read contract exists for harness or IDE consumption.**
   Any future IDE panel, harness step, or scripted operator tool that
   wants to *display* chain state would today either re-derive the
   state from those files (re-implementing operator JSON-parsing rules
   in every consumer) or wait for a contract. Without a defined
   read-only contract, the first consumer to ship would necessarily
   invent its own state model — and the invention is the moment a STOP
   gate can quietly leak into a "convenience" affordance.

A2-L2d closes both gaps by **specifying a read-only contract**, not by
adding any write affordance.

## 3. Relationship to A2-L2b and A2-L2c

A2-L2b proved the operator-gated chain:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

A2-L2c documented the chain for operators (quick reference + exit-code
disambiguation + TTY EOF note + per-step artifact map).

A2-L2d is the next strategic step on the IDE/harness path, in the
correct order:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, this scope card)
      → harness/IDE adapter (separate, future)
        → IDE UI (separate, future)
```

A2-L2d does *not* advance the write surface. It does *not* introduce a
new operator gate. It does *not* weaken any A2-L2b or A2-L2c gate. It
adds a read layer that consumers can rely on instead of inventing.

## 4. Recommended Scope

`A2-L2d Read-Only Artifact Inspector / Status Contract`

The future A2-L2d implementation lane is bounded as follows.

**Objective.** Define and ship a read-only, side-effect-free way to
answer, for any workspace touched by the A2-L2b chain:

- What run / step is this workspace currently in?
- What artifacts exist for that step?
- What state is the chain in (preview-ready, awaiting approval, apply
  bundle ready, applied, rolled back, non-approvable, no run, unknown)?
- What command may the operator run next, given that state?
- Is this preview approvable?
- Is this apply-ready?
- What evidence supports each of those answers (which files were read,
  which SHAs matched, which markers were observed)?
- What STOP condition (if any) from
  [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates),
  section 8, currently applies?

**Why now.** Stack-Code is on the IDE/harness path. The next consumer
of the chain — whether a terminal operator returning mid-run, a test
harness asserting on state, or a future IDE panel — needs a stable
read-only contract. Defining that contract now, before any consumer
ships, is the smallest reversible step that preserves the A2-L2b
operator-gating property and makes future consumers safe by
construction.

**Why this size.** A2-L2d is specifically the *read* lane. It pairs a
new on-disk schema (`a2-l2d-status.v1`) with a new read-only CLI
affordance (`claw plan status <workspace>`) and nothing else. It does
not include any write, approval, apply, or model affordance. It does
not include the IDE/harness adapter itself. It does not include any UI.

## 5. Non-Goals

A2-L2d must not:

- introduce or imply autonomous workspace-write execution
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  preapproval, batch approval, or any approval-bypass affordance
- merge `approve` and `apply` into a single command
- introduce any new write subcommand or write flag on any existing
  subcommand
- modify `claw plan run`, `claw plan approve`,
  `claw plan apply-bundle`, or `claw plan apply` behavior, exit codes,
  schemas, markers, or JSON field shapes
- modify `a2-l2b-*` schema versions or marker constants
- call broker, model, or Ollama at any phase
- write, rename, or delete any file under `.claw/` or under the
  workspace tree from inside the status command
- write into any non-disposable repository (no commits, pushes,
  merges, branch creation, or worktree creation from inside the
  status command)
- weaken any A2-L2b or A2-L2c STOP gate
- introduce an IDE / GUI surface (deferred to a later lane)
- introduce a harness adapter implementation (deferred to a later
  lane; A2-L2d only ships the contract the adapter would consume)
- auto-roll-back, auto-clean, or auto-discard stale runs (any
  mutation is out of scope)

## 6. Allowed Future Touched Surfaces

When A2-L2d is later executed as an implementation lane, the only
surfaces it may touch are:

- `rust/crates/a2-plan-runner/src/status.rs` (new module; read-only)
- `rust/crates/a2-plan-runner/src/lib.rs` (module wire-up only, no
  changes to existing modules)
- `rust/crates/rusty-claude-cli/src/main.rs` and/or its `plan`
  subcommand dispatch (one new read-only `status` subcommand; no
  changes to existing subcommand bodies)
- `rust/crates/a2-plan-runner/tests/l2d_status.rs` (new tests; no
  changes to existing tests)
- `rust/crates/rusty-claude-cli/tests/plan_status.rs` (new tests; no
  changes to existing tests)
- `docs/a2-l2d-status-schema.md` (new; schema-of-record for the
  envelope)
- `docs/a2-l2d-operator-quickref.md` (new, optional; operator-facing
  copy-paste for `claw plan status`)
- one cross-link line in `README.md` "Documentation map"
- one cross-link line in
  [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  section 10 ("Recommended Next Lanes")
- one cross-link line in
  [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md)
  section 6 ("References")

No other file in the repository may be modified by the A2-L2d
implementation lane.

## 7. Forbidden Surfaces

The A2-L2d implementation lane is forbidden from touching:

- `rust/crates/a2-plan-runner/src/{approval,approval_ux,checkpoint,
  diff_preview,preflight,report,runner,write_executor,write_payload,
  write_preview,write_runtime,markers}.rs`
- `rust/crates/a2-plan-runner/tests/l2b_*` and any existing A2-L2b
  test under `rust/crates/a2-plan-runner/tests/`
- `rust/crates/rusty-claude-cli/tests/plan_apply_bundle.rs` and any
  existing A2-L2b apply-chain test
- existing schema constants:
  - `a2-l2b-preview-bundle.v1`
  - `a2-l2b-preview-bundle-generator-result.v1`
  - `a2-l2b-apply-bundle.v1`
  - `a2-l2b-apply-bundle-generator-result.v1`
  - `a2-l2b-apply-result.v1`
  - `a2-l2b-run-plan-write-preview-run-manifest.v1`
  - `a2-l2b-run-plan-write-preview-status.v1`
- existing exit-code constants in
  `rust/crates/a2-plan-runner/src/{approval,checkpoint,report,runner,
  write_executor,write_runtime}.rs`
- existing marker constants (any `a2-l2b-*` or `a2-l1-*` marker)
- `rust/Cargo.toml`, `rust/Cargo.lock`
- `.github/workflows/**`
- `.github/scripts/**` (the doc source-of-truth script is read-only
  input to validation, not a target)
- shell wrappers, launchers, `bin/`

If the A2-L2d implementation lane needs to touch any of the above,
the lane has exceeded its scope and must be reopened as a separate,
explicitly-authorized lane.

## 8. Read-Only Inputs

The future `claw plan status <workspace>` command may read, and only
read, the following inputs:

- `<workspace>/.claw/l2b-runs/` directory enumeration (latest run by
  mtime, or all runs if explicitly requested)
- `<workspace>/.claw/l2b-runs/<run-id>/run-manifest.json`
- `<workspace>/.claw/l2b-runs/<run-id>/status.json`
- `<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`
- `<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-generator-result.json`
- `<workspace>/.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json`
  (if present)
- `<workspace>/.claw/l2b-checkpoints/<run-id>/<step-id>/manifest.json`
- `<workspace>/.claw/l2b-checkpoints/<run-id>/<step-id>/before.bin`
  (SHA only; no content surfacing)
- `<workspace>/.claw/l2b-payloads/<run-id>/<step-id>/after.bin`
  (SHA only; no content surfacing)
- `<workspace>/.claw/l2b-payloads/<run-id>/<step-id>/after.sha256`
- the live target file referenced by the preview record (SHA only,
  for STOP-condition detection; no mutation, no content surfacing)
- an *optional* operator-supplied approval-result JSON path passed as
  a positional argument (the chain does not persist this under
  `.claw/`)

The command must not read:

- secrets, environment variables beyond what `claw` already reads at
  startup
- the workspace tree outside the live target whose SHA must be
  verified for STOP detection
- network endpoints of any kind
- the broker `:11435`, Ollama `:11434`, model APIs, or any other HTTP
  surface
- the operator's shell history, terminal state, or any non-workspace
  file

## 9. Proposed Read-Only Output Contract

The proposed pinned envelope is `a2-l2d-status.v1`. Fields (subject to
implementation-lane refinement, but **bounded** by this scope):

| Field                  | Type                | Meaning                                                                                  |
|------------------------|---------------------|------------------------------------------------------------------------------------------|
| `schema_version`       | string (literal)    | Pinned `a2-l2d-status.v1`.                                                               |
| `workspace_root`       | string (abs path)   | Operator-supplied workspace root, after lexical normalization.                           |
| `run_id`               | string \| null      | Latest run id under `.claw/l2b-runs/`, or null if no run exists.                         |
| `step_id`              | string \| null      | Pending step id from the run manifest, or null if none.                                  |
| `phase`                | enum string         | One of: `no_run_found`, `preview_ready`, `awaiting_approval`, `approval_captured`, `apply_bundle_ready`, `applied`, `rolled_back`, `non_approvable`, `unknown`. |
| `next_operator_command`| string              | Literal command string from the canonical chain, or `"STOP — escalate"` when a STOP condition applies, or `"(no run found — start with claw plan run …)"`. |
| `is_approvable`        | bool                | Derived from the preview-generator-result `is_binary`/`is_redacted`/`is_truncated`. False when any of those is true. False when no preview exists. |
| `is_apply_ready`       | bool                | True only when `apply-bundle.json` exists, validates against `a2-l2b-apply-bundle.v1`, and the on-disk payload SHA matches `preview_record.payload_sha256`. |
| `before_sha256`        | string \| null      | From `preview_record` if available.                                                      |
| `after_sha256`         | string \| null      | From `preview_record` if available.                                                      |
| `payload_sha256`       | string \| null      | From the on-disk payload SHA file if available.                                          |
| `live_target_sha256`   | string \| null      | SHA of the live target file *now*, for STOP comparison. Read-only.                       |
| `stop_condition`       | string \| null      | One of the named STOP gates from handoff section 8, or null.                             |
| `evidence_paths`       | array of strings    | Every artifact path that was read to produce this envelope.                              |
| `audit_markers`        | array of strings    | A2-L2d-only markers (e.g. `a2-l2d-status-read`, `a2-l2d-status-no-run-found`, `a2-l2d-status-stop-condition-detected`). Must not reuse `a2-l1-*` or `a2-l2b-*` markers. |
| `read_only_invariant`  | string (literal)    | Pinned literal `"this command does not mutate state"`, present in every successful emission. |

Notes:

- The envelope is *additive*: it derives from existing artifacts and
  does not produce new on-disk artifacts, except optionally a
  `.claw/l2d-status/<run-id>/last-read.json` cache **only if**
  explicitly authorized by the implementation lane and re-reviewed.
  This scope card does **not** authorize that cache; the default is
  pure-stdout emission.
- The envelope is *idempotent*: running `claw plan status <workspace>`
  twice on an unchanged workspace must produce byte-identical stdout.
- The envelope must never include broker, model, Ollama, network, or
  process-environment information.

## 10. IDE / Harness Boundary

A future IDE or harness adapter may consume `a2-l2d-status.v1`
envelopes as **read-only** state. Specifically, an adapter MAY:

- read the envelope from stdout of `claw plan status <workspace>`
- surface `phase`, `next_operator_command`, `is_approvable`,
  `is_apply_ready`, and `stop_condition` in a panel
- offer a button that *renders* the next operator command for the
  operator to copy and run in their own terminal
- show `evidence_paths` so the operator can inspect raw artifacts

A future IDE or harness adapter MUST NOT:

- call `claw plan approve`, `claw plan apply-bundle`, or
  `claw plan apply` on behalf of the operator from the read surface
- compose `approve` and `apply` in a single panel action
- offer an "approve" button, "apply" button, "auto-approve" toggle, or
  any equivalent affordance from inside the read-only surface
- bypass the TTY-enforced approval boundary by supplying approval
  input through any non-TTY channel
- mutate `.claw/` or the workspace tree
- treat the envelope as authoritative for write decisions; the chain
  itself re-validates every input at apply time
  ([handoff section 6](./a2-l2b-run-plan-preview-operator-handoff.md#6-authority-chain))

Any future write affordance on an IDE/harness surface must be opened
as a separate, explicitly-authorized lane (A2-L3 or later). A2-L2d is
**not** prior authorization for any write surface.

## 11. Safety Invariants

The A2-L2d implementation lane must preserve, verbatim:

- preview before approval
- TTY/operator approval enforcement
- approval bound to `step_id` + `preview_sha256` from the preview
  record
- apply-bundle generation as a separate offline step
- apply as a separate explicit operator step
- single-file write per apply bundle
- no model-generated `after` bytes
- no broker, model, or Ollama traffic at any phase
- no commits, pushes, merges performed inside the chain
- no autonomous mutation of any non-disposable repo

In addition, A2-L2d adds:

- **read-only invariant**: `claw plan status <workspace>` must produce
  zero filesystem mutations under `.claw/`, the workspace tree, the
  operator's home directory, or anywhere else, under any input
- **network-egress-free invariant**: `claw plan status <workspace>`
  must produce zero network egress (no broker, model, Ollama,
  telemetry, or any other endpoint)
- **non-overlapping marker invariant**: every marker emitted by the
  status command must be `a2-l2d-*`; reusing `a2-l1-*` or `a2-l2b-*`
  markers from the status surface is forbidden
- **non-overlapping exit-code invariant**: any new exit code must not
  reuse or overload existing constants from `approval`, `checkpoint`,
  `report`, `runner`, `write_executor`, or `write_runtime`; existing
  codes (including the overloaded `7`) are out of scope to change
- **idempotency invariant**: two successive `claw plan status
  <workspace>` invocations on an unchanged workspace must emit
  byte-identical stdout

## 12. Validation Plan

When the A2-L2d implementation lane runs, it must verify all of:

- `git diff --name-only` lists **only** files from section 6. Any
  other path fails the lane.
- `git diff --check` is clean.
- `python3 .github/scripts/check_doc_source_of_truth.py` passes.
- Forbidden-language sniff against the staged diff finds no language
  authorizing autonomous writes, `--yes`, preapproval, batch approval,
  approval bypass, or apply automation. The same regex used by the
  A2-L2c scope-card validation phase applies.
- Read-only invariant: a test fixture runs `claw plan status` against
  a frozen workspace tree captured via `mtime`/SHA snapshot pre- and
  post-invocation; any mtime or SHA delta fails the lane.
- Network-egress-free invariant: tests run with no broker reachable
  and no model endpoint reachable; the status command must succeed.
  (For added rigor: tests may set `HTTP_PROXY` /
  `HTTPS_PROXY` / `OLLAMA_HOST` to deliberately unreachable values.)
- Idempotency invariant: tests run the status command twice
  back-to-back on an unchanged workspace and diff stdout. Any
  difference fails the lane.
- Phase-coverage tests: golden-file tests cover every member of the
  `phase` enum at least once, including `no_run_found`,
  `preview_ready`, `awaiting_approval`, `approval_captured`,
  `apply_bundle_ready`, `applied`, `rolled_back`, `non_approvable`,
  and `unknown`.
- STOP-condition tests: every named STOP condition from
  [handoff section 8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)
  has at least one fixture producing the expected `stop_condition`
  value and `next_operator_command: "STOP — escalate"`.
- Schema cross-check: every field in the envelope is documented in
  `docs/a2-l2d-status-schema.md`; every field documented there is
  present in the source.
- A2-L2b regression guard: existing A2-L2b tests (`l2b_*`,
  `plan_apply_bundle.rs`) are unchanged and continue to pass.
- A2-L2c regression guard: existing A2-L2c docs are unchanged
  except for the single cross-link permitted in section 6 of this
  scope card.

## 13. STOP Gates (for the future A2-L2d implementation lane)

The A2-L2d implementation lane must STOP and refuse to land if any of
the following are true:

- Any test or fixture observes a filesystem mutation under `.claw/`
  or the workspace tree caused by `claw plan status`.
- Any test or fixture observes network egress from `claw plan status`
  to a broker, model, Ollama, telemetry, or any other endpoint.
- The lane introduces, renames, or modifies any existing CLI
  command, subcommand, flag, exit code, marker, schema, or JSON field
  outside of section 6's allow-list.
- The lane introduces a write-adjacent flag on `claw plan status`
  (`--apply`, `--approve`, `--yes`, `--auto`, `--clean`,
  `--rollback`, `--mutate`, etc.).
- The lane introduces an IDE-driven approval or apply path.
- The lane attempts to consolidate `approve` + `apply` into a single
  command from any surface (including the status surface).
- The lane attempts to invent a "no-prompt" approval pathway.
- The lane attempts to auto-rollback or auto-clean stale runs.
- The lane requires touching any forbidden surface from section 7 to
  "make the status command work" — the status must align to source,
  not the other way.
- The lane requires running a live smoke against
  `/home/suki/stack-code`, `/home/suki/sidestackai`, or any other
  non-disposable repo as part of authoring.
- The handoff or A2-L2c quick reference gains any edit beyond the
  single cross-link line permitted in section 6.
- `origin/main` advances during the lane in a way that invalidates
  the artifact paths or schema versions this scope card references —
  the lane must rebase and re-verify.
- Two successive `claw plan status` calls on an unchanged workspace
  produce divergent stdout.

## 14. Options Considered and Rejected

Three larger or differently-shaped candidate scopes were considered
and rejected for A2-L2d:

- **Option C — IDE/Harness Adapter Boundary Design (docs-only).**
  Defines what an IDE/harness adapter may consume and surface and
  explicitly forbids write affordances on the adapter surface.
  **Rejected** as the *next* lane because without the
  `a2-l2d-status.v1` contract defined first, the adapter would
  necessarily invent a state model. The boundary doc should follow
  the contract, not precede it. Defer to A2-L3-adapter-boundary
  after A2-L2d ships.

- **Option D — Read + Write Convenience Bundle ("status with apply
  buttons").** Would package a read surface together with one-click
  approve/apply affordances for IDE consumption.
  **Rejected outright.** A read surface that exposes write
  affordances is no longer a read surface; it invalidates the
  TTY-enforced approval and the explicit-apply boundary that A2-L2b's
  safety derives from. Forbidden by this scope card.

- **Option E — Auto-rollback / auto-clean status.** Would have the
  status command "tidy" stale or rolled-back runs.
  **Rejected outright.** Any auto-mutation in a "read-only" status
  command is a category violation. Status must be strictly read.

- **Option F — Embed status into `claw plan apply`.** Would have
  `claw plan apply` print a structured status before applying.
  **Rejected.** Re-mixes read and write layers and risks degrading
  the explicit-apply boundary into a "show-then-apply" flow that an
  IDE could weaponize.

- **Option G — Status as a `claw plan` flag (`--status`) on existing
  commands.** Would add a `--status` flag to `claw plan run`,
  `claw plan approve`, `claw plan apply-bundle`, or `claw plan
  apply`.
  **Rejected.** Adds a flag to commands whose flag surface is part
  of the A2-L2b safety property; per section 5, A2-L2d must not
  modify existing subcommands or flags.

The recommended A2-L2d scope (sections 4-13) is intentionally the
smallest reversible step that closes the named read-side gaps while
preserving every A2-L2b gate.

## 15. Definition of Done

The A2-L2d implementation lane is done when **all** of the following
hold:

- `claw plan status <workspace>` exists, is read-only by
  construction, and emits a stdout envelope matching
  `a2-l2d-status.v1` as defined in section 9.
- `docs/a2-l2d-status-schema.md` documents every field in the
  envelope and pins the schema version.
- Tests pass against all `phase` enum members from section 9.
- Tests pass against every named STOP condition from handoff
  section 8.
- Read-only invariant test passes (no FS mutations).
- Network-egress-free invariant test passes.
- Idempotency invariant test passes.
- Existing A2-L2b tests are unchanged and still pass.
- Existing A2-L2c docs are unchanged except for one permitted
  cross-link line each in the handoff and the quick reference.
- `README.md` "Documentation map" has at most one new cross-link
  line pointing at the A2-L2d schema doc (and optionally one for the
  A2-L2d operator quick reference, if included).
- No file outside section 6's allow-list is modified.
- No Rust module from section 7 is touched.
- No new exit code, marker, schema, or JSON field outside the
  `a2-l2d-status.v1` envelope is introduced.
- No broker, model, or Ollama traffic is generated by the lane or by
  the new command.
- No commits, pushes, or merges are performed by the chain.

## 16. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **A2-L2d implementation lane** — implement `claw plan status
> <workspace>`, the `a2-l2d-status.v1` envelope, and
> `docs/a2-l2d-status-schema.md`, within the boundaries defined by
> this scope card.

The lane *after* A2-L2d ships is:

> **A2-L3 harness/IDE adapter boundary (docs-only)** — define the
> read-only adapter contract for IDE/harness consumers of
> `a2-l2d-status.v1`, with explicit prohibition of write affordances
> on the adapter surface. Open as a separate scope card.

Neither lane permits autonomous workspace-write execution; both
remain bounded by the A2-L2b and A2-L2c safety properties.

## 17. References

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- [`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md) — A2-L2c
  scope card; the structural model this scope card follows.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md)
  — A2-L2c operator quick reference; the operator-facing surface
  A2-L2d augments with read-only state recovery.
- [`a2-plan-schema.md`](./a2-plan-schema.md) — A2 plan YAML schema
  (the L1a/L2a offline validator surface).
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.

## 18. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- Autonomous-write authorization: **no**.
- Approval / apply boundary weakened: **no**.
- Next gate before implementation: operator review of this scope card.
