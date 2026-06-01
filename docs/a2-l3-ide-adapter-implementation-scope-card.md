# A2-L3 Scope Card — IDE Adapter Implementation (Docs-Only)

This document is a **design-only implementation scope card** for the
future A2-L3 IDE Adapter. It is bounded by the merged A2-L3 IDE
Adapter Scope Card
([`a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md))
and the merged A2-L3 Adapter Boundary Scope Card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)).

This card defines, in design only, the concrete future
implementation constraints for the IDE adapter — recommended shape,
allowed touched surfaces, forbidden touched surfaces, the validation
matrix the future implementation lane must hold to, and the STOP
gates that future implementation review must enforce. It itself
authorizes **no** runtime change, **no** new code, **no** IDE
extension package, **no** test runs, **no** broker/model/Ollama
traffic, **no** harness modification, **no** approve/apply UI
controls, **no** approve/apply composition, **no** autonomous
workspace-write execution, and **no** IDE execution against any
workspace.

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
                  → IDE adapter per-adapter scope card (A2-L3, shipped, PR #49)
                    → IDE adapter IMPLEMENTATION scope card (THIS DOCUMENT)
                      → future IDE adapter implementation (separate, future)
```

This scope card authorizes **design only**. It does not authorize
IDE implementation. It does not authorize adapter implementation. It
does not authorize approve/apply/apply-bundle execution. It does not
authorize approve/apply UI controls. It does not authorize
autonomous workspace-write execution.

## 1. Executive Summary

A2-L3 IDE Adapter Implementation is the next lane after the IDE
adapter per-adapter scope card landed at
[`8d520e6` on `origin/main` (PR #49)](https://github.com/thesidestackai/stack-code/pull/49).
This card is the per-implementation scope card that
[`a2-l3-ide-adapter-scope-card.md` §20](./a2-l3-ide-adapter-scope-card.md#20-future-implementation-constraints)
requires before any IDE implementation code is authored.

The recommended future implementation shape is a **dedicated read-
only IDE-host extension package** that shells out only to
`claw plan status` and renders `a2-l2d-status.v1` without direct
`.claw/**` parsing, without any Rust crate dependency on
`rust/crates/a2-harness-adapter/`, and without any host-platform
write affordance. The first targeted IDE host is **VS Code**, as
the lowest-friction starting host with the broadest operator-
reachable footprint; **JetBrains plugin** and **language-server-
backed panel** hosts follow in separate per-host implementation
lanes that re-derive the same envelope-parser and STOP-renderer
under this card's constraints.

The implementation lane shells out to `claw plan status` from the
IDE host's native scripting environment (TypeScript for VS Code,
Java/Kotlin for JetBrains, language-server runtime for an LSP-
backed panel), parses the small, schema-pinned `a2-l2d-status.v1`
envelope JSON natively in that environment, and renders it through
the host's native affordance vocabulary — panel, command-palette
entry, copy-to-clipboard, refresh control, collapsible raw-envelope
view. It performs no other subprocess execution, no filesystem
write, no network egress, no envelope persistence as authoritative
state, no `.claw/**` parsing, no background polling, and no
filesystem watching.

This card declares this shape as the **default future
implementation shape** because:

- An IDE-host extension package keeps the IDE adapter consumable
  from the IDE host operators actually use, without forcing a Rust
  crate / FFI dependency that the harness adapter scope card §17
  already pinned as out of scope for the harness surface (and that
  is equally out of scope for the IDE surface).
- A dedicated package gives the future implementation lane a clean
  forbidden-vs-allowed enforcement target: any change to
  `rust/crates/a2-plan-runner/src/status.rs` (the producer), to any
  A2-L2b module, or to `rust/crates/a2-harness-adapter/**` (the
  sibling harness surface) is a category violation visible in the
  diff.
- The IDE host's native scripting environment can re-derive the
  envelope parser from `a2-l2d-status.v1` (a small, pinned schema)
  without taking a Cargo dependency on the harness adapter crate,
  preserving the parent IDE adapter scope card §4 invariant that
  the IDE adapter and the harness adapter are siblings, not parent-
  and-child.
- The IDE-host-native test harness (e.g. `@vscode/test-electron` for
  VS Code) runs in isolation from the runtime chain; network-egress
  sentinels, mock workspaces, and STOP golden fixtures can be
  enforced inside the package's own test suite without touching any
  workspace-level CI workflow.

This card defines the boundary the future IDE adapter
implementation lane must hold to. The next gate before that
implementation is operator review of this scope card. This card
authorizes **only** the future implementation lane to be *opened
under these constraints*; it does not itself author code, run
tests, or merge anything.

## 2. Why This Scope Card Exists

The merged A2-L3 IDE adapter scope card §20 explicitly requires a
per-implementation scope card before any IDE code is written. That
requirement exists because:

1. **Behavioral boundaries are not implementation boundaries.** The
   IDE adapter scope card defines what the IDE adapter *is and is
   not* in behavioral terms — read-only visual observer, never a
   workflow controller, no approve/apply UI controls, every STOP at
   parity. The implementation lane needs answers to different
   questions: which IDE host(s) are targeted, which package layout
   and language are chosen, which CI mechanism enforces the
   no-network and no-watcher invariants, which static-grep guards
   refuse forbidden affordances. A separate scope card is the right
   place for those answers, because the wrong answer (e.g. importing
   `chokidar` "for the convenience of operator-facing refresh"
   without realizing it is a filesystem watcher) would leak the
   adapter boundary into the IDE host's package manifest.

2. **IDE host affordance pressure is the largest threat surface.** A
   misdesigned IDE affordance — for example, a "Ready to Apply"
   pill that hides the underlying `stop_condition`, a "copy
   approval line" gesture that composes with a "run in terminal"
   shortcut, or an "auto-refresh" setting that turns into a daemon
   poller — fails silently from the operator's perspective. The
   chain's safety depends on STOPs being seen and the operator-
   gated chain being preserved; the implementation lane must enter
   with a pinned validation matrix that exercises every affordance-
   pressure failure mode *before* a single host-package file is
   written.

3. **STOP-rendering coverage is non-trivial.** The IDE adapter must
   correctly render every closed `stop_condition` value, every
   closed `phase` value, the refusal envelope, every unknown-enum
   synthetic fixture, every schema-drift case, every idempotency
   property, and every visibility/parity property — and it must do
   so through a *visual surface* whose acceptable rendering is
   harder to specify than a JSON assertion. That golden matrix is
   large enough that defining it here — before code is written —
   prevents the implementation lane from shipping with partial
   coverage and discovering gaps in operator review.

4. **IDE host package layouts diverge.** A VS Code extension lives
   under a directory with `package.json` and an `extension.ts`
   entry; a JetBrains plugin lives under a directory with
   `plugin.xml` and Kotlin source; a language-server-backed panel
   lives under whichever package the LSP runtime targets. The exact
   touched-surface enumeration is host-dependent, so this card pins
   the *first* host (VS Code) explicitly and pins the *boundary*
   that subsequent host implementation lanes must hold to without
   pre-empting their concrete enumerations.

5. **CI validation matrix needs to exist before code does.** The
   future implementation lane will be reviewed against the CI
   matrix; the matrix needs to be written down first so the review
   is not "did the author remember every check" but "does the diff
   exercise every pinned check."

## 3. Relationship To A2-L3 IDE Adapter Scope Card

The merged IDE adapter scope card
([`a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md))
pinned:

- relationship to the adapter boundary, the harness adapter, and
  the A2-L2d status contract (§§3–5)
- IDE adapter responsibilities and non-responsibilities (§§6–7)
- allowed reads and forbidden actions (§§8–9)
- IDE surface contract — input, output, forbidden controls (§10)
- per-field display rules (§11)
- STOP condition visibility rules (§12)
- evidence path rendering rules (§13)
- refresh / polling boundary (§14)
- copy-to-clipboard boundary (§15)
- disposable workspace handling (§16)
- security / secrets boundary (§17)
- safety invariants (§18)
- non-goals (§19)
- future implementation constraints (§20)
- definition of done and next-lane recommendation (§§21–22)

This implementation scope card is **subordinate** to those pins. It
does not modify, soften, or expand them. Where this card adds
detail (host selection, touched-surface enumeration, CI matrix,
package manifest constraints, dependency audit), the detail must
remain consistent with the IDE adapter scope card; any tension is
resolved in favor of the IDE adapter scope card, and any contract
gap discovered during implementation is escalated as a separate
scope-card lane rather than worked around here.

## 4. Relationship To A2-L3 Adapter Boundary

The A2-L3 adapter boundary scope card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md))
pinned the overall adapter boundary, including its five cross-
adapter invariants (read-only, STOP-visibility, no-write-surface,
no-state-invention, no-shadow-contract). The IDE adapter
implementation lane bounded by this card preserves every one of
those invariants verbatim. This card adds IDE-specific mechanical
constraints (host targets, touched surfaces, CI matrix) but does
not introduce any cross-adapter contract. Any future implementation
finding that suggests modifying the boundary card must be opened as
a separate scope-card lane against the boundary card.

## 5. Relationship To A2-L2d Status Contract

The A2-L2d `a2-l2d-status.v1` schema-of-record
([`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)) is the
contract the IDE adapter consumes. This card does **not**:

- modify `a2-l2d-status.v1` (no field additions, no field removals,
  no field-order changes)
- add new `phase`, `stop_condition`, `next_operator_command`, or
  marker values
- add new exit codes
- change `EXIT_STATUS_REFUSED == 12` semantics
- add new CLI subcommands, flags, or positional arguments to
  `claw plan status`
- change `rust/crates/a2-plan-runner/src/status.rs` or its tests
- change `rust/crates/a2-harness-adapter/**` (the sibling harness
  surface)

If the implementation lane discovers that a missing field, marker,
or enum value is required to honor the IDE behavioral contract,
that discovery is escalated as a separate scope-card lane against
A2-L2d. The IDE implementation does not work around a contract
gap; it surfaces the gap and waits.

## 6. Recommended Implementation Shape

The recommended future implementation shape, subject to operator
review and modification in the implementation lane itself, is:

- **A new IDE-host extension package at `ide/<host>/claw-status-
  panel/`** with its own host-native package manifest, source files,
  and tests. The first targeted host is `ide/vscode/claw-status-
  panel/`.
- **No new CLI binary, no new Rust crate, no new workspace member.**
  The IDE adapter shells out only to the existing `claw plan
  status` binary; it does not add a Rust workspace member, does not
  add a `[[bin]]` target, and does not depend on
  `rust/crates/a2-harness-adapter/**`. The implementation lane MUST
  refuse any design that introduces a new Rust crate or workspace
  member as part of the IDE adapter.
- **IDE-host-native scripting language.** For VS Code, the package
  is implemented in TypeScript using the `vscode` extension API. For
  JetBrains, the package is implemented in Kotlin (or Java) using
  the JetBrains plugin SDK. For an LSP-backed panel, the package is
  implemented in whichever runtime the LSP host targets. The
  envelope parser is re-derived in the host's native language from
  the pinned `a2-l2d-status.v1` schema; the parser is not imported
  from the harness adapter crate.
- **Single forward dependency: the system `claw` binary.** The
  package invokes `claw plan status <workspace> [<approval-result.
  json>]` as a subprocess (via the host's native subprocess API —
  Node `child_process` for VS Code, JetBrains `GeneralCommandLine`
  for JetBrains, LSP-runtime equivalent for LSP) with no flags and
  at most the two A2-L2d positional arguments. No FFI, no HTTP,
  no language-bindings, no Rust crate import.
- **Synchronous-on-operator-gesture only.** Every status invocation
  is triggered by an explicit operator gesture (panel open, refresh
  button click, command-palette entry, operator-bound keybinding).
  No background polling, no filesystem watcher, no IDE host file-
  change event subscription, no daemon push channel, no timer.
- **In-memory rendering state only.** The package keeps a single
  parsed envelope + raw stdout capture in memory for the duration
  of a single panel session. Nothing is written to disk: no IDE
  workspace storage, no IDE global storage, no IDE secret storage,
  no host-side cache file.
- **First-host implementation lane scope.** The first per-host
  implementation lane targets exactly one IDE host (VS Code). It
  adds golden fixtures for every closed enum value, the refusal
  envelope, every unknown-enum synthetic case, every STOP-rendering
  rule, every refresh-boundary rule, every copy-to-clipboard
  boundary rule, and every evidence-path rendering rule.
- **A small documentation file at `docs/a2-l3-ide-adapter-
  usage.md`** explaining the package shape, the operator-facing
  surface, and the explicit non-authorisations. This is a future
  docs deliverable, not authored here.

Uncertainty markers (the implementation lane is encouraged to
deviate with justification):

- *first IDE host*: VS Code is the default; alternative first hosts
  (JetBrains, LSP-backed) are acceptable if the implementation lane
  scope card justifies the change against host-reach and host-
  testability criteria.
- *package directory*: `ide/<host>/claw-status-panel/` is the
  default layout; the implementation lane MAY choose a different
  directory naming convention if it pins the choice in its own
  scope card and the choice does not shadow any existing
  `rust/crates/`, `docs/`, or workspace-tooling path.
- *envelope-parser sharing across hosts*: each per-host
  implementation lane re-derives the parser in the host-native
  language; the implementation lane MAY add a small per-package
  type-generation step (e.g. a JSON-schema-derived `.d.ts` file
  committed in the package) if doing so does not introduce a Rust
  dependency or a cross-host shared parser package.
- *no shared parser across hosts*: this is intentional. A shared
  parser would couple IDE host packages to each other (and
  potentially to the harness adapter crate); each per-host package
  remains independent.

## 7. Allowed Future Touched Surfaces

The future IDE adapter implementation lane MAY touch only the files
in the explicit enumeration below. The implementation lane MUST NOT
touch any file outside this enumeration.

For the **first per-host implementation lane** (default: VS Code):

- `ide/vscode/claw-status-panel/package.json` (new file — extension
  manifest)
- `ide/vscode/claw-status-panel/tsconfig.json` (new file)
- `ide/vscode/claw-status-panel/.vscodeignore` (new file)
- `ide/vscode/claw-status-panel/README.md` (new file — package-
  local README; not the workspace README)
- `ide/vscode/claw-status-panel/src/extension.ts` (new file —
  activation entry point)
- `ide/vscode/claw-status-panel/src/<modules>.ts` (new files,
  module names left to the implementation lane within the
  package's source tree; e.g. `envelope.ts`, `parser.ts`,
  `stop.ts`, `panel.ts`, `clipboard.ts`, `subprocess.ts`)
- `ide/vscode/claw-status-panel/test/<test files>.ts` (new files,
  integration tests scoped to this package only)
- `ide/vscode/claw-status-panel/test/fixtures/<golden files>` (new
  files, deterministic JSON fixtures for the STOP/phase/refusal
  matrix and unknown-enum synthetic cases)
- `docs/a2-l3-ide-adapter-usage.md` (new file, optional in the
  implementation lane; if the implementation lane defers usage docs
  to a follow-up lane, that is acceptable).
- Optionally one new line in `README.md` cross-linking the package
  and this scope card, if an obvious location exists. The
  implementation lane MAY omit this cross-link; no further README
  edits are within scope.

For **subsequent per-host implementation lanes** (JetBrains, LSP-
backed, or other), the allowed surfaces follow the same shape under
the per-host directory:

- `ide/<host>/claw-status-panel/<manifest>` (host-native manifest;
  e.g. `plugin.xml` for JetBrains)
- `ide/<host>/claw-status-panel/<build-config>` (host-native build
  configuration; e.g. `build.gradle.kts` for JetBrains)
- `ide/<host>/claw-status-panel/README.md` (package-local README)
- `ide/<host>/claw-status-panel/src/<modules>.<ext>` (host-native
  source files)
- `ide/<host>/claw-status-panel/test/<test files>.<ext>` (host-
  native test files)
- `ide/<host>/claw-status-panel/test/fixtures/<golden files>`
  (golden fixtures)
- `docs/a2-l3-ide-adapter-usage-<host>.md` (host-specific usage
  guide, optional)
- Optionally one new line in `README.md` cross-linking the new host
  package.

The implementation lane MUST enumerate, in its own per-host scope
card preamble (a top-of-PR comment is acceptable), the exact subset
of these files it touches. Touching a file outside the enumeration
is a STOP gate (§22).

Each per-host implementation lane is a **separate PR**. Multiple
hosts MUST NOT be implemented in a single lane.

## 8. Forbidden Future Touched Surfaces

The future IDE adapter implementation lane MUST NOT touch any of
the following files. Each forbidden surface maps to a named safety
property the chain depends on.

- `rust/crates/a2-plan-runner/src/status.rs` — the A2-L2d producer.
  The IDE adapter consumes its stdout; modifying the producer to
  accommodate the IDE adapter is a category violation.
- `rust/crates/a2-plan-runner/src/approval.rs`
- `rust/crates/a2-plan-runner/src/approval_ux.rs`
- `rust/crates/a2-plan-runner/src/checkpoint.rs`
- `rust/crates/a2-plan-runner/src/diff_preview.rs`
- `rust/crates/a2-plan-runner/src/preflight.rs`
- `rust/crates/a2-plan-runner/src/report.rs`
- `rust/crates/a2-plan-runner/src/runner.rs`
- `rust/crates/a2-plan-runner/src/write_executor.rs`
- `rust/crates/a2-plan-runner/src/write_payload.rs`
- `rust/crates/a2-plan-runner/src/write_preview.rs`
- `rust/crates/a2-plan-runner/src/write_runtime.rs`
- `rust/crates/a2-plan-runner/src/markers.rs`
- Any A2-L2b schema-version constant, exit-code constant, or
  `a2-l2b-*` / `a2-l2d-*` marker constant.
- `rust/crates/a2-plan-runner/tests/**` — existing A2-L2b/L2d tests
  remain authoritative; the IDE adapter MUST NOT add tests there.
- `rust/crates/a2-harness-adapter/**` — the sibling harness adapter
  surface. The IDE adapter MUST NOT modify any harness crate file,
  MUST NOT import the harness crate, MUST NOT take a Cargo
  dependency on the harness crate, and MUST NOT add a TypeScript
  or other-host wrapper that re-exports harness crate types.
- `rust/crates/rusty-claude-cli/src/**` — no new CLI subcommand,
  flag, or argument is in scope.
- `rust/crates/rusty-claude-cli/tests/**` — existing
  `plan_status.rs` and friends remain authoritative; the IDE
  adapter MUST NOT add CLI integration tests there.
- `rust/crates/api/**`, `rust/crates/commands/**`,
  `rust/crates/compat-harness/**`,
  `rust/crates/mock-anthropic-service/**`,
  `rust/crates/plugins/**`, `rust/crates/runtime/**`,
  `rust/crates/telemetry/**`, `rust/crates/tools/**` — none of
  these are part of the IDE adapter surface.
- Workspace `Cargo.toml`, `Cargo.lock` — no Rust workspace member
  is added by this lane. (If a future per-host implementation lane
  ever needs Rust workspace changes, that lane MUST escalate via a
  separate scope-card lane against this card.)
- `wrappers/**`, `bin/**`, `examples/**` — no operator-facing
  wrapper or example is in scope; usage examples (if any) live
  inside the per-host package's own README or in the optional
  per-host usage docs.
- `.github/workflows/**` — no new workflow file or workflow change
  by default. If a per-host implementation lane requires a workflow
  to run IDE-host-specific tests that existing workflows cannot
  exercise (e.g. `@vscode/test-electron` requires Xvfb / headless
  Electron), that workflow change is a **separate scope-card lane**
  against `.github/workflows/**`, not a side-effect of the IDE
  implementation lane.
- `scripts/**`, `Makefile`, `justfile`, or any other top-level
  build script — none of these are touched.
- `SideStackAI/**` — out of scope by the cross-project boundary
  the operator pinned.
- `.claw/**` in any repository, including any workspace the IDE
  adapter renders — the IDE adapter writes nothing under `.claw/`
  and reads nothing under `.claw/` directly.
- Other IDE host packages — a VS Code implementation lane MUST NOT
  touch `ide/jetbrains/**` or `ide/lsp/**`; each per-host lane is
  bounded strictly to its own host directory.

The implementation lane MUST NOT depend on, or anticipate, any
file path under a different IDE host's directory.

## 9. IDE Input Contract

The future IDE adapter implementation accepts only the following
inputs. This restates the IDE adapter scope card §10 / §20 in
implementation terms; the constraints themselves are unchanged.

- **`claw plan status` stdout.** Captured from the subprocess as a
  raw byte string. The implementation MAY decode as UTF-8 before
  JSON-parsing; it MUST also retain the raw byte string for the
  collapsible raw-envelope view (§10).
- **`claw plan status` exit code.** Captured as an integer; `0`
  means success-envelope, `12` (`EXIT_STATUS_REFUSED`) means
  refusal-envelope, any other value classifies the cycle as a STOP
  signal in its own right.
- **Operator-selected workspace path.** Read from the host's
  workspace API (VS Code: `vscode.workspace.workspaceFolders` after
  operator selection, or a workspace path the operator passed
  through a command-palette argument). The implementation MUST NOT
  canonicalize, expand, normalize, or substitute the path before
  passing it to the subprocess.
- **Optional operator-selected approval-result path.** Read from
  the operator's selection (e.g. a file-open dialog the operator
  triggers, or a command-palette argument). The implementation MUST
  NOT read, write, generate, or mutate the file's contents itself.
- **Operator-triggered refresh event.** A discrete UI gesture: the
  operator clicked the refresh button, executed the refresh
  command-palette entry, or pressed an operator-bound keybinding.
  Every refresh event triggers exactly one `claw plan status`
  invocation.
- **In-memory panel state.** The currently rendered parsed
  envelope, raw stdout capture, collapsed/expanded state of the
  raw-envelope disclosure, copy-to-clipboard pending payload (if
  any), and the operator's cursor / selection position. This state
  is package-local, non-authoritative, and not persisted.

The IDE adapter implementation MUST NOT accept any input that
would direct it to invoke `claw plan run`, `claw plan approve`,
`claw plan apply-bundle`, `claw plan apply`, or any non-status
subprocess. Such inputs are a category violation and the
implementation lane MUST refuse them at construction time — no
configuration option, no host command, no keybinding, and no host-
side run-configuration may be wired through the IDE adapter
package to any chain-write subprocess.

The IDE adapter implementation MUST NOT consume:

- filesystem-watcher events
- IDE-host file-change events (e.g. VS Code
  `vscode.workspace.onDidChangeFiles`,
  `vscode.workspace.onDidSaveTextDocument`,
  `vscode.workspace.onDidOpenTextDocument` — any IDE host event
  whose firing is not the operator's explicit refresh gesture is
  out of scope)
- Git event streams or `git status` polling
- daemon push channels
- timer-driven refresh events
- broker, model, Ollama, or any HTTP message
- IDE-host telemetry, analytics, marketplace dashboard, or error-
  reporting input

## 10. IDE Output / Display Contract

The future IDE adapter implementation renders only the following
outputs. This restates the IDE adapter scope card §10 / §11 in
implementation terms; the constraints themselves are unchanged.

### Primary rendered fields

For every observed envelope, the implementation renders:

- `phase` (closed-enum value, verbatim, with host-native styling
  permitted but the literal value MUST remain visible)
- `next_operator_command` (as copyable text only; see §14)
- `is_approvable` (boolean, may be styled as a chip; never gates
  any UI affordance whose effect is a write step)
- `is_apply_ready` (boolean, same constraint as `is_approvable`)
- `stop_condition` (closed-enum value when non-null, null-aware
  placeholder when null; verbatim closed-enum string; see §11)
- `evidence_paths` (verbatim list, each entry rendered as a local
  file link per §12)
- `audit_markers` (verbatim list, closed-enum members)
- `read_only_invariant` (pinned literal, surfaced on every
  rendered envelope, never hidden / abbreviated / substituted)
- `schema_version` (verbatim; any literal other than
  `a2-l2d-status.v1` is a STOP signal)
- `workspace_root` (verbatim; never canonicalized or re-resolved)
- `run_id` / `step_id` (verbatim or null-aware placeholder
  distinguishable from a real identifier)
- `before_sha256` / `after_sha256` / `payload_sha256` /
  `live_target_sha256` (verbatim hex strings or null-aware
  placeholders; MAY be in a collapsed "SHA detail" disclosure
  that opens on a single operator gesture)

### Supplementary rendered fields

- Raw envelope JSON (collapsible disclosure; supplementary to the
  structured rendering)
- Diagnostic message (supplementary, non-load-bearing; the
  structured fields above remain authoritative)
- Refresh affordance (single explicit operator control)
- Copy-to-clipboard affordances scoped per §14

### Forbidden rendered outputs

The implementation MUST NOT render:

- a synthetic pass/fail pill derived from a synthetic assertion the
  operator did not declare in their own terminal session
- a "Ready to Apply" composite indicator (or any composite
  indicator that subsumes `stop_condition`)
- a "Healthy" / "Unhealthy" pill that subsumes `stop_condition`
- an aggregate "chain progress" gauge derived from multiple fields
- an IDE-host notification badge whose state is not equal to a
  single envelope field
- preview content rendering (the IDE adapter does not read preview
  file contents)
- diff rendering against `evidence_paths` files
- IDE-side hash computation against `evidence_paths` files
- a chain-state pill for any workspace other than the one the
  operator explicitly named
- any forbidden control enumerated in the IDE adapter scope card
  §§7, 9, 10, 20 — restated in §22 of this card

### Container layout

The implementation lane chooses the host-native panel layout
(sidebar panel, tree view, webview, status-bar entry, or
combination) and pins the choice in its own per-host scope card.
Whichever layout is chosen MUST satisfy:

- STOP rendering parity with non-STOP rendering (§11)
- `read_only_invariant` visible on every rendered envelope
- `evidence_paths` visible without further disclosure when a STOP
  is rendered

## 11. STOP Rendering Golden-Test Matrix

The future implementation lane's in-package integration tests MUST
include golden fixtures for the following cases. Each fixture is a
deterministic JSON envelope (or refusal envelope, or unparseable
synthetic stdout) that exercises exactly one rule and asserts the
IDE adapter renders the relevant field verbatim, classifies the
panel correctly, and emits the expected visual / structural state.

**Closed `stop_condition` values** (from
[`a2-l2d-status-schema.md` §6](./a2-l2d-status-schema.md#6-closed-stop_condition-enum)):

- `workspace-root-invalid`
- `run-manifest-unreadable`
- `preview-bundle-unreadable`
- `payload-sha-mismatch`
- `live-target-missing`
- `live-target-sha-changed`
- `approval-decision-not-approved`
- `approval-sha-mismatch`
- `approval-step-id-mismatch`
- `apply-bundle-schema-mismatch`
- `apply-bundle-target-path-mismatch`

**Closed `phase` values** (from
[`a2-l2d-status-schema.md` §4](./a2-l2d-status-schema.md#4-closed-phase-enum)):

- `no_run_found`
- `preview_ready`
- `awaiting_approval`
- `approval_captured`
- `apply_bundle_ready`
- `applied`
- `rolled_back`
- `non_approvable`
- `unknown`

**Refusal envelope** (from
[`a2-l2d-status-schema.md` §9](./a2-l2d-status-schema.md#9-refusal-envelope)):

- one fixture per closed `stop_condition` value that can fire as a
  refusal cause (at minimum: `workspace-root-invalid`,
  `run-manifest-unreadable`, `preview-bundle-unreadable`)
- exit code `EXIT_STATUS_REFUSED == 12` propagated through the
  subprocess invocation, classified as a STOP panel, with the
  refusal envelope rendered verbatim and the
  `a2-l2d-status-refused` marker visible

**Unknown-enum synthetic fixtures**:

- one fixture per closed enum (`phase`, `stop_condition`,
  `next_operator_command`, marker) carrying an unknown value. Each
  fixture MUST cause the IDE adapter to classify the panel as a
  STOP in its own right and render the unknown literal verbatim.
- one fixture with an unknown enum value combined with a known
  STOP, to assert that both STOP signals are rendered
  independently and neither is collapsed into the other.

**Schema-drift fixtures**:

- `missing_read_only_invariant_fixture_is_stop` — a synthetic
  envelope whose `read_only_invariant` field is absent MUST
  classify the panel as STOP (the parser surfaces this as schema
  drift because the required field is missing).
- `substituted_read_only_invariant_fixture_is_stop` — a synthetic
  envelope whose `read_only_invariant` literal is replaced by any
  string other than the pinned literal MUST classify the panel as
  STOP and MUST preserve the substituted literal verbatim on the
  emitted STOP rendering; no coercion to the pinned literal is
  permitted.
- `schema_version_mismatch_is_stop` — any literal other than
  `a2-l2d-status.v1` MUST classify the panel as STOP and render
  the observed literal verbatim.
- `unparseable_stdout_is_stop` — synthetic raw stdout that is not
  valid JSON MUST surface as a STOP panel, MUST attach no parsed
  envelope to the panel state, and MUST preserve the raw stdout
  bytes verbatim on the panel state for the operator escalation
  read.
- `missing_required_field_is_stop` — any required field absent
  from the envelope MUST classify the panel as STOP and render
  the missing-field name verbatim.

**Producer-broken / sibling-harness preservation fixtures** (mirror
of the harness adapter PR43 preservation matrix, restated for IDE
rendering):

- `non_null_stop_condition_with_empty_evidence_paths_is_stop` — an
  envelope carrying a non-null `stop_condition` together with an
  empty `evidence_paths` array MUST classify the panel as STOP and
  surface the offending `stop_condition` verbatim. The A2-L2d
  producer always populates at least one evidence path when a STOP
  fires; an empty list is producer-broken drift the IDE adapter
  raises in its own right.
- `exit_12_with_refused_marker_is_accepted_as_refusal` — an envelope
  with `EXIT_STATUS_REFUSED == 12` whose `audit_markers` contains
  the pinned `a2-l2d-status-refused` literal MUST classify the
  panel as STOP (the refusal itself) and MUST NOT raise the
  missing-marker STOP.
- `exit_12_without_refused_marker_is_stop` — an envelope with
  `EXIT_STATUS_REFUSED == 12` whose `audit_markers` does NOT
  contain the pinned `a2-l2d-status-refused` literal MUST surface
  an additional STOP carrying the observed marker list verbatim,
  alongside the underlying refusal STOP.

**STOP-visibility rule fixtures** (mirror of the IDE adapter scope
card §12):

- `render_stop_verbatim` — a fixture asserting the rendered
  surface contains the exact closed-enum literal (e.g.
  `payload-sha-mismatch`) and not a friendly-text substitution
  (e.g. "Mismatch detected").
- `stop_parity_with_non_stop` — a fixture asserting STOP rendering
  has at-least-parity visual or programmatic prominence with non-
  STOP rendering. Implementation lane chooses the assertion
  mechanism (host-native snapshot test, computed-style assertion,
  webview DOM inspection, or equivalent).
- `stop_retained_across_refresh` — paired fixtures: refresh #1
  produces a STOP; refresh #2 produces the same STOP. The IDE
  adapter MUST render the STOP both times, MUST NOT collapse, MUST
  NOT debounce, MUST NOT rate-limit, MUST NOT downgrade.
- `stop_cleared_only_after_non_stop_refresh` — paired fixtures:
  refresh #1 produces a STOP; refresh #2 produces a non-STOP
  envelope. The STOP rendering is cleared only after refresh #2.
- `evidence_paths_visible_under_stop` — a STOP fixture where the
  evidence_paths list MUST be visible on the rendered surface
  without further operator interaction beyond opening the panel
  (i.e. not hidden under a collapsed-by-default disclosure).
- `no_snooze_mute_dismiss_ignore_affordance` — a static assertion
  (host-native command-palette / context-menu / keybinding audit)
  that no command, keybinding, context-menu item, or other host
  affordance exposes a STOP snooze / mute / dismiss / ignore /
  "remind me later" gesture.
- `no_stop_downgrade_to_warning` — a fixture asserting STOP is
  never re-classified as "warning", "info", "soft failure", or any
  lower-severity classification in the rendered surface.

**Caller-expectation parity** (mirror of the harness adapter
implementation scope card §12 caller-expectation matrix, restated
for IDE rendering where applicable):

- the IDE adapter has no caller-declared assertion concept (unlike
  the harness adapter); the equivalent is the operator's visual
  expectation. The implementation lane MUST surface every STOP at
  the granularity the envelope carries; "the operator expected
  continuation" is not a defense against a STOP rendering being
  attenuated.

The implementation lane MAY add additional fixtures beyond this
minimum. It MUST NOT ship with fewer.

## 12. Evidence Path Rendering Design

`evidence_paths` is the operator's primary STOP-diagnosis surface
([`a2-l2d-operator-quickref.md` §6](./a2-l2d-operator-quickref.md#6-stop-conditions)).
The implementation lane MUST follow the rules pinned in the IDE
adapter scope card §13 and prove them via in-package tests.

The implementation MAY:

- render each entry as a clickable link whose action opens the file
  in the IDE host's editor (the same way any in-editor file link
  behaves: VS Code `vscode.window.showTextDocument` after
  `vscode.workspace.openTextDocument`; JetBrains `FileEditorManager.
  openFile`; LSP-runtime equivalent).
- render the link's text verbatim as the envelope-carried path.
- render a host-native "missing file" indicator using IDE-host
  conventions, provided the indicator is derived from the IDE
  host's normal file-availability check (e.g. `fs.exists` /
  `fs.stat`) and not from a custom IDE-adapter probe that reads
  `.claw/**` or workspace files outside the envelope-permitted
  scope.

The implementation MUST NOT:

- read the file contents itself.
- preview the file contents in the IDE adapter panel.
- hash, sign, summarize, or otherwise process the file contents.
- rewrite the path before rendering (no canonicalization, no
  expansion, no normalization, no substitution).
- hide a missing file from the operator. If the path does not
  resolve, the implementation MUST surface that state to the
  operator alongside the path itself.
- create a file at an `evidence_paths` location that does not
  exist.
- follow a path outside the workspace root without explicit
  rendering that calls out the out-of-workspace location to the
  operator.
- mutate, rename, delete, copy, or move any `evidence_paths` file.
- compose the file-open gesture with any other action (no
  "open-and-mark-reviewed", no "open-and-stage", no "open-in-
  read-only-mode-then-edit").
- offer "open all evidence paths" as a single gesture.

### Required tests

The implementation lane's in-package tests MUST include:

- `evidence_path_in_workspace_renders_as_link` — a fixture with an
  evidence path under the workspace root renders as a clickable
  link that triggers the host's standard file-open behavior.
- `evidence_path_out_of_workspace_renders_with_warning` — a
  fixture with an evidence path outside the workspace root (e.g.
  the operator-supplied approval-result path) renders with an
  explicit warning surface that the path is outside the workspace.
  The link still opens the file via the host's standard behavior,
  but only after the operator clicks past or acknowledges the
  warning rendering.
- `evidence_path_missing_renders_with_indicator` — a fixture with
  an evidence path that does not exist on the local filesystem
  renders with a host-native missing-file indicator alongside the
  path text. The path text is NOT removed; the operator sees both
  the path and the missing-file state.
- `evidence_paths_visible_under_stop` (also under §11) — when the
  panel renders a STOP, the evidence_paths list is visible without
  further operator interaction.
- `no_file_content_read_test` — a static / runtime assertion that
  the implementation calls no API that reads the contents of any
  `evidence_paths` file (no `fs.readFile`, no `vscode.workspace.
  openTextDocument(path).then(doc => doc.getText())` driven by the
  IDE adapter package code path). The host's standard file-open
  affordance, triggered by the operator's click, is permitted
  because that read is performed by the host's editor on the
  operator's behalf — not by the IDE adapter.
- `no_file_create_test` — a snapshot of the tempdir before and
  after a panel session asserts no file was created at any
  `evidence_paths` location.
- `no_open_all_test` — a static assertion that no command,
  keybinding, or context-menu entry composes to opening more than
  one `evidence_paths` entry in a single operator gesture.

## 13. Refresh / Polling Validation Design

The IDE adapter is permitted to invoke `claw plan status` only in
response to an explicit operator gesture. The implementation lane
MUST prove this property by:

- **Operator-gesture-only invocation test.** A test that asserts
  the IDE adapter package code path never invokes `claw plan
  status` outside of an operator gesture. The mechanism is host-
  native (mock subprocess wrapper with a process-spawn counter, or
  audit log of host commands fired). The assertion is: total
  spawns equals total operator gestures.
- **One-gesture-one-spawn test.** A test that asserts a single
  operator gesture (panel open, refresh button, command-palette
  entry, keybinding) triggers exactly one `claw plan status`
  invocation. No batching, no fan-out, no implicit re-invocation.
- **No-filesystem-watcher dependency audit.** A static-grep guard
  against the package source for:
  - `chokidar`
  - `watchman`
  - `vscode.workspace.createFileSystemWatcher`
  - `vscode.workspace.onDidChangeFiles`
  - `vscode.workspace.onDidCreateFiles`
  - `vscode.workspace.onDidDeleteFiles`
  - `vscode.workspace.onDidRenameFiles`
  - `fs.watch`
  - `fs.watchFile`
  - `notify` / `inotify` / `FSEvents` host-equivalents (for
    non-VS-Code hosts)
- **No-background-polling guard.** A static-grep guard against the
  package source for:
  - `setInterval`
  - `setTimeout` of any duration (the implementation lane MAY
    justify a single short timeout for a UX debouncing gesture
    that does NOT trigger any subprocess invocation; the guard's
    default refusal stands until such justification is pinned in
    the per-host implementation scope card)
  - `vscode.workspace.onDidSaveTextDocument`
  - `vscode.workspace.onDidOpenTextDocument`
  - `vscode.workspace.onDidChangeTextDocument`
  - `vscode.window.onDidChangeWindowState`
  - `vscode.workspace.onDidChangeConfiguration` for any
    setting whose change handler triggers a status invocation
- **No-daemon-channel test.** A static-grep guard against IPC
  channels, Unix domain sockets, named pipes, WebSocket clients,
  or any other persistent connection mechanism.
- **No-Git-event-stream test.** A static-grep guard against
  `simple-git`, `nodegit`, `vscode.extensions.getExtension('vscode.
  git')` event subscriptions, or any other Git event surface.
- **No-auto-refresh-on-STOP test.** A fixture pair where refresh
  #1 returns a STOP envelope; the test asserts the IDE adapter
  does NOT spawn a second `claw plan status` invocation after the
  STOP without an operator gesture. The fixture pair holds for at
  least the duration of a host-native debouncing window plus a
  margin.
- **No-auto-refresh-setting audit.** A static assertion that the
  package's settings manifest contains no `auto-refresh`, `polling-
  interval`, `refresh-on-save`, `refresh-on-focus`, `refresh-on-
  git-pull`, or any operator-toggleable setting whose effect
  initiates a non-gesture invocation.

The implementation MUST NOT:

- subscribe to filesystem watchers (chokidar, watchman, IDE-host
  file-change events, `fs::notify`, inotify, FSEvents, or any
  other).
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

## 14. Copy-To-Clipboard Validation Design

The IDE adapter MAY offer copy-to-clipboard affordances scoped to
single envelope fields. The implementation lane MUST prove the
boundary holds by:

- **Single-field-only copy test.** A test that asserts every copy
  action exposed by the IDE adapter copies exactly one envelope
  field's verbatim string to the system clipboard. The mechanism
  is host-native (VS Code: assert against
  `vscode.env.clipboard.writeText` call payloads via a clipboard-
  spy; JetBrains: assert against the clipboard manager's last
  write).
- **Verbatim-payload test.** Per copy action, a test that asserts
  the clipboard payload exactly equals the envelope field's value
  (no decoration, no terminal-prefixing, no shell-quoting changes,
  no path-canonicalization, no SHA insertion, no truncation, no
  whitespace normalization).
- **No-composite-payload test.** A static-grep guard against
  string-concatenation patterns that would combine two or more
  envelope fields into a single clipboard payload (e.g. `"apply " +
  step_id + " " + preview_sha256`).
- **No-chained-action test.** A static assertion that no copy
  command is wired through a host-native command sequence to a
  terminal-open / terminal-write / terminal-execute action. The
  implementation lane chooses the audit mechanism (manifest /
  command-binding audit, runtime assertion that the copy command
  resolves to a single `clipboard.writeText` call).
- **No-clipboard-history test.** A static assertion that the
  package's settings, secret-storage, and workspace-storage do not
  include any field whose name or shape suggests retained
  clipboard payloads (e.g. `clipboardHistory`, `recentCopies`,
  `pastedFromIDEAdapter`).
- **No-analytics-capture test.** A static-grep guard against
  telemetry / analytics / error-reporting SDK calls in any code
  path that handles clipboard payloads.

### Allowed copy actions

- copy `next_operator_command` (verbatim string)
- copy one `evidence_paths` entry (verbatim string)
- copy raw envelope JSON (canonical envelope JSON, verbatim
  byte-for-byte; the implementation lane chooses whether to copy
  the raw stdout capture or the parsed-and-reserialized JSON, and
  pins the choice in its per-host scope card)

### Forbidden copy actions

- "copy approval line preformatted for the terminal"
- "copy full chain command sequence"
- "copy approve-then-apply preformatted"
- "copy and execute"
- "copy and open terminal"
- "copy and run in terminal"
- "copy and switch focus to terminal"
- "copy and prompt for confirmation"
- "copy approval line" — the approval line is not an envelope
  field; composing it inside the IDE adapter is forbidden.

## 15. No-Write / No-Approve / No-Apply Validation

The IDE adapter scope card §§7, 9 forbid every chain-write
operation. The implementation lane MUST prove this property by:

- **Filesystem-write sentinel test.** An integration test that
  snapshots a disposable tempdir's full content tree before and
  after a panel session, asserts byte-identical equality, and
  fails the lane if any byte differs. Implementation lane pins the
  exact snapshot mechanism.
- **`claw plan run|approve|apply-bundle|apply` refusal test.** A
  test that supplies an input deliberately constructed to direct
  the IDE adapter to invoke each of those commands (e.g. an
  envelope-derived setting value containing one of the chain-write
  subcommands, an operator-supplied workspace path containing one
  of those subcommands) and asserts the IDE adapter refuses the
  input at construction time.
- **Subprocess argv audit test.** A test that captures the argv of
  every subprocess the IDE adapter spawns during a panel session
  and asserts the only program name observed is the configured
  `claw plan status` binary path. Implementation lane pins the
  mock-subprocess wrapper.
- **Static-grep guard.** The implementation lane's package CI step
  MUST run a grep over the package source for:
  - `claw plan run`
  - `claw plan approve`
  - `claw plan apply-bundle`
  - `claw plan apply`
  - `approval-result\.json` (read-only references are permitted in
    comments / docstrings / variable names; runtime construction
    is forbidden)
  - `apply-bundle\.json` (same constraint)
  - `fs.writeFile`
  - `fs.appendFile`
  - `fs.createWriteStream`
  - `fs.mkdir` (the implementation lane MAY justify a single
    `fs.mkdirSync` for the IDE-host extension's own host-managed
    extension storage if such storage is required for non-
    envelope-payload state; the guard's default refusal stands
    until pinned in the per-host scope card)
  - `fs.rename`
  - `fs.unlink`
  - `fs.rm`
  - `fs.rmdir`
  - host-native write-API equivalents (VS Code: `vscode.workspace.
    fs.writeFile`, `vscode.workspace.fs.delete`, `vscode.workspace.
    fs.rename`)

  and refuse the lane if any match appears in non-test, non-
  comment source code. The implementation scope card MAY adjust
  the exact regex; the expectation that the guard exists does not
  change.
- **No-`.claw/`-write test.** A test that runs a full panel
  session against a workspace whose `.claw/**` tree is snapshotted
  before and after and asserts byte-identical equality of the
  `.claw/**` tree.
- **No-write-controls audit.** A static assertion that the
  package's manifest declares no command, keybinding, context-
  menu item, gutter affordance, lens, hover action, status-bar
  action, drag target, or any other host affordance whose name or
  binding suggests a chain-write step. The audit mechanism is
  host-native (VS Code: parse `package.json`
  `contributes.commands` and refuse any command title or ID
  matching `/approve|apply|run plan|apply.bundle/`; JetBrains:
  parse `plugin.xml` action group definitions; LSP-runtime
  equivalent).

The IDE adapter MUST NOT write under `.claw/`, the workspace
tree, the operator's home directory, the IDE host's settings
directory, the IDE host's secret-storage, or anywhere outside the
IDE host's standard non-authoritative cache directory (and even
then only if the per-host implementation scope card justifies the
write against non-envelope-payload state).

## 16. No-Network / No-Broker / No-Model Validation

The IDE adapter scope card §17 forbids broker, model, Ollama,
telemetry, analytics, error-reporting, IDE-host marketplace
endpoints, and any HTTP traffic. The implementation lane MUST
prove this property by:

- **Dependency audit.** The package's manifest (e.g. VS Code
  `package.json` `dependencies` and `devDependencies`, JetBrains
  `build.gradle.kts`) MUST NOT depend on:
  - `axios`
  - `node-fetch`
  - `got`
  - `request`
  - `superagent`
  - `ky`
  - `undici` (the Node 18+ built-in fetch is acceptable if
    statically guarded against, but the package MUST NOT use
    `globalThis.fetch` or `fetch()` in runtime code)
  - any IDE-host telemetry SDK (e.g. `@vscode/extension-
    telemetry`, JetBrains analytics SDK, Sentry SDK, Datadog SDK,
    New Relic SDK, Honeycomb SDK, OpenTelemetry SDK)
  - any error-reporting SDK
  - any IDE-host marketplace dashboard SDK
  - any HTTP client whose presence in the package's installed
    dependency tree (per `npm ls --all --json` for npm, equivalent
    for other package managers) is not already required by an
    explicitly-justified transitive dependency. The implementation
    scope card MUST list the exact dependency set and the review
    MUST confirm no networking client is in the transitive closure
    beyond what is justified.
- **Network-sentinel test.** A package integration test that sets
  `HTTP_PROXY`, `HTTPS_PROXY`, and `OLLAMA_HOST` to unreachable
  sentinels (mirroring the A2-L2d invariants in
  [`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants))
  and runs a full panel session against a mock `claw plan status`
  subprocess. The test asserts the session completes successfully
  and the sentinels are never resolved.
- **Subprocess-bounded test.** A test that asserts the IDE
  adapter spawns *only* the `claw plan status` subprocess with at
  most the two A2-L2d positional arguments, no flags. The
  implementation lane chooses the mechanism (process-spawn audit
  log, mock-subprocess wrapper); the assertion is the same.
- **Static-grep guard.** The implementation lane's package CI
  step MUST run a grep over the package source for:
  - `http://`
  - `https://` (URLs in comments / docstrings referencing public
    documentation are acceptable if the linter exempts comments;
    URLs in runtime code paths are forbidden)
  - `fetch(`
  - `XMLHttpRequest`
  - `vscode.env.openExternal` (or host-native equivalent)
  - `vscode.workspace.openTextDocument(Uri.parse('https://...'))`
    or any other host API that initiates an outbound network call
  - `ollama_host`
  - `broker_url`
  - `telemetry_url`
  - any host-native URL-based notification API

  and refuse the lane if any match appears in non-test, non-
  comment runtime code. The implementation scope card MAY adjust
  the exact regex; the expectation that the guard exists does not
  change.
- **No-WebSocket / no-IPC test.** A static-grep guard against
  `WebSocket`, `ws` package, `net.createConnection`, named-pipe
  client APIs, and any other persistent connection mechanism.

The IDE adapter MUST NOT relay envelope contents to:

- telemetry endpoints (IDE-host or third-party)
- analytics endpoints
- error-reporting endpoints
- IDE-host marketplace dashboards
- broker, model, or Ollama endpoints
- any network endpoint at any phase of IDE adapter operation

## 17. Direct `.claw/**` Parsing Prohibition

The IDE adapter MUST consume chain state exclusively through
`claw plan status` stdout. Direct parsing of `.claw/**` artifacts
is forbidden. The implementation lane MUST prove this by:

- **Static-grep guard.** The implementation lane's package CI step
  MUST run a grep over the package source for:
  - `\.claw`
  - `l2b-runs`
  - `l2b-preview-bundles`
  - `l2b-checkpoints`
  - `l2b-payloads`
  - `run-manifest\.json`
  - `preview-bundle\.json`
  - `apply-bundle\.json`
  - `after\.sha256`

  and refuse the lane if any match appears in non-test, non-
  comment, non-fixture runtime code that involves a filesystem
  read. (The strings MAY appear in code that renders verbatim
  envelope-carried evidence paths — but the package code MUST NOT
  call any filesystem-read API against those paths.)
- **No-direct-read test.** A test that runs a full panel session
  against a workspace whose `.claw/**` tree is monitored for read
  events. The test asserts no read event fires against any
  `.claw/**` file from the IDE adapter package code path. The
  monitoring mechanism is host-native (strace / dtrace / ETW for
  full-OS tracing; or a runtime read-spy injected before package
  initialization). The host's standard editor-open behavior,
  triggered by the operator clicking an `evidence_paths` entry, is
  permitted because that read is performed by the host's editor on
  the operator's behalf — not by the IDE adapter package code.

The IDE adapter MUST consume only:

- `claw plan status` stdout (success envelope, exit `0`)
- `claw plan status` stdout (refusal envelope, exit `12`)
- `claw plan status` exit code

The IDE adapter MUST NOT read:

- `<workspace>/.claw/l2b-runs/**`
- `<workspace>/.claw/l2b-preview-bundles/**`
- `<workspace>/.claw/l2b-checkpoints/**`
- `<workspace>/.claw/l2b-payloads/**`
- any other `<workspace>/.claw/**` file
- any other workspace file directly. If the operator opens an
  entry from `evidence_paths` in their editor, that read is
  performed by the IDE host's editor, not by the IDE adapter.
- any non-workspace file (the operator-supplied approval-result
  path is forwarded as a subprocess argument; the IDE adapter
  package itself never reads its contents).

## 18. Security / Secrets Boundary

The IDE adapter MUST NOT read, log, persist, or relay any of:

- environment variables (the IDE adapter does not need any
  environment variables to invoke `claw plan status`; the producer
  reads what it needs from the OS environment, not from the IDE
  adapter)
- the operator's shell history
- the operator's terminal state
- the operator's home directory beyond what the IDE host already
  reads as part of normal IDE operation
- any secret material from `.claw/**` (none should exist there;
  the IDE adapter still MUST NOT emit such material if it does,
  and MUST NOT render it in the raw-envelope view if it appears)
- the operator's git config, credentials, SSH keys, or GPG keys
- broker, model, or Ollama API keys or tokens
- IDE host secret-storage values (VS Code: `secrets.get`;
  JetBrains: `PasswordSafe`; or host-equivalent)
- IDE host marketplace tokens
- IDE host telemetry tokens

The implementation lane MUST ensure the IDE adapter's panel
rendering, clipboard payloads, and in-memory state are free of any
caller-secret material derived from anything other than the
envelope itself. The envelope contains no secrets by A2-L2d
construction.

The implementation lane MUST include:

- **No-environment-variable-read test.** A test that asserts the
  package code path does not read any environment variable beyond
  what the host's standard activation function reads (i.e. nothing
  the IDE adapter package itself adds).
- **No-secret-storage-access test.** A static-grep guard against
  `vscode.SecretStorage` (or `context.secrets`), JetBrains
  `PasswordSafe`, and host-equivalents. The guard refuses the lane
  if the package code reads from or writes to any secret-storage
  API.
- **Deterministic-rendering test.** A test that runs the IDE
  adapter against a fixture envelope under two different
  environment-variable configurations (empty environment, full
  operator environment) and asserts the rendered surface is
  identical (modulo workspace path). The mechanism is host-native
  (webview snapshot, tree-view snapshot, or computed-output
  serialization).

## 19. Disposable Workspace Handling

The IDE adapter operates against workspaces the operator is
already editing in their IDE host. Unlike the harness adapter, the
IDE adapter does **not** classify workspaces itself.

The implementation lane MUST hold to the IDE adapter scope card §16
constraints:

- **No operator-side classification override.** The package MUST
  NOT expose any setting, preference, configuration field,
  workspace setting, command, or operator-facing affordance whose
  effect would classify the current workspace as
  disposable/trusted/etc. and loosen STOP visibility or refresh
  cadence based on that classification.
- **No cross-workspace authority.** The package MUST NOT display
  chain state for workspaces other than the one the operator
  explicitly named. No cross-workspace dashboard, no workspace-
  list pill, no aggregated panel.
- **No Cargo dependency on the harness adapter's classifier
  crate.** The package MUST NOT import the harness adapter
  classifier; if a future shared classifier emerges, that is a
  separate scope-card lane.
- **Identical behavior across disposable / non-disposable.** Every
  rule in this card holds regardless of the workspace's
  disposability. STOP visibility, refresh cadence, copy-to-
  clipboard scope, evidence-path rendering, and forbidden actions
  are identical for disposable and non-disposable workspaces.

### Required tests

- **No-classification-override test.** A static-grep guard against
  any package setting whose name suggests workspace classification
  (`disposable`, `trusted`, `safe-mode`, `production`, `read-
  only-mode`, etc.). The guard refuses the lane if any such
  setting exists.
- **No-cross-workspace-aggregation test.** A test that runs the
  package against two distinct workspaces and asserts the rendered
  panel for workspace A contains no state for workspace B (and
  vice versa). The mechanism is host-native (webview snapshot or
  tree-view inspection).

If a future A2-L2d lane adds a disposability field to the
`a2-l2d-status.v1` envelope, the IDE adapter MAY render that field
verbatim as a read-only indicator. Until such a field exists, the
IDE adapter surfaces no disposability indicator at all. The IDE
adapter MUST NOT invent its own disposability indicator from the
workspace path or any other heuristic.

## 20. CI / Test Matrix

The IDE adapter implementation lane MUST pass the following CI
matrix before merge. The matrix is enforced by existing workspace
CI (fmt, clippy, test, docs source-of-truth, shell tests) plus new
per-host package tests and grep guards.

| Check | Mechanism | Mandatory |
|-------|-----------|-----------|
| existing workspace CI (Rust fmt, clippy, test) — unchanged by this lane | existing workflow | yes |
| docs source-of-truth | existing workflow | yes |
| shell tests | existing workflow | yes |
| per-host package install | new in-package CI step (e.g. `npm ci` for VS Code) | yes |
| per-host package lint | new in-package CI step (e.g. `eslint` / `ktlint`) | yes |
| per-host package type-check | new in-package CI step (e.g. `tsc --noEmit`) | yes |
| per-host package test | new in-package CI step (e.g. `npm test`, `@vscode/test-electron` for VS Code) | yes |
| STOP golden matrix (§11) | new in-package tests | yes |
| Refusal envelope matrix (§11) | new in-package tests | yes |
| Unknown-enum synthetic matrix (§11) | new in-package tests | yes |
| Schema-drift matrix (§11) | new in-package tests | yes |
| Sibling-harness preservation matrix (§11) | new in-package tests | yes |
| STOP-visibility rules (§11) | new in-package tests | yes |
| Evidence-path rendering tests (§12) | new in-package tests | yes |
| Out-of-workspace path warning test (§12) | new in-package test | yes |
| Missing-file indicator test (§12) | new in-package test | yes |
| No-file-content-read test (§12) | new in-package test or CI step | yes |
| No-open-all test (§12) | new in-package test or CI step | yes |
| Operator-gesture-only invocation test (§13) | new in-package test | yes |
| One-gesture-one-spawn test (§13) | new in-package test | yes |
| No-filesystem-watcher dependency audit (§13) | new in-package CI step | yes |
| No-background-polling guard (§13) | new in-package CI step | yes |
| No-daemon-channel test (§13) | new in-package CI step | yes |
| No-Git-event-stream test (§13) | new in-package CI step | yes |
| No-auto-refresh-on-STOP test (§13) | new in-package test | yes |
| No-auto-refresh-setting audit (§13) | new in-package CI step | yes |
| Single-field-only copy test (§14) | new in-package test | yes |
| Verbatim-payload test (§14) | new in-package test | yes |
| No-composite-payload test (§14) | new in-package CI step | yes |
| No-chained-action test (§14) | new in-package test or CI step | yes |
| No-clipboard-history test (§14) | new in-package CI step | yes |
| No-analytics-capture test (§14) | new in-package CI step | yes |
| Filesystem-write sentinel test (§15) | new in-package test | yes |
| `claw plan run|approve|apply-bundle|apply` refusal test (§15) | new in-package test | yes |
| Subprocess argv audit test (§15) | new in-package test | yes |
| Static-grep no-write guard (§15) | new in-package CI step | yes |
| No-`.claw/`-write test (§15) | new in-package test | yes |
| No-write-controls manifest audit (§15) | new in-package CI step | yes |
| Dependency audit (§16) | implementation scope card review + new in-package test asserting installed dependencies excludes HTTP clients | yes |
| Network-sentinel test (§16) | new in-package test | yes |
| Subprocess-bounded test (§16) | new in-package test | yes |
| Static-grep no-network guard (§16) | new in-package CI step | yes |
| No-WebSocket / no-IPC test (§16) | new in-package CI step | yes |
| Static-grep no-`.claw/**`-parse guard (§17) | new in-package CI step | yes |
| No-direct-`.claw/**`-read test (§17) | new in-package test | yes |
| No-environment-variable-read test (§18) | new in-package test | yes |
| No-secret-storage-access test (§18) | new in-package CI step | yes |
| Deterministic-rendering test (§18) | new in-package test | yes |
| No-classification-override test (§19) | new in-package CI step | yes |
| No-cross-workspace-aggregation test (§19) | new in-package test | yes |

Each check is a hard gate. Skipping any check in the
implementation lane is a STOP gate (§22).

The implementation lane MUST NOT introduce a new CI workflow file
unless the existing workflows cannot exercise a required check; if
a workflow change is needed (e.g. Xvfb for `@vscode/test-electron`),
that is a separate scope-card lane against `.github/workflows/**`,
not a side-effect of the IDE implementation lane.

## 21. Non-Goals

The IDE adapter implementation must not:

- implement more than one IDE host in a single lane
- modify the harness adapter crate (`rust/crates/a2-harness-
  adapter/**`) in any way
- modify any A2-L2b / A2-L2d / A2-L3-boundary / A2-L3-harness
  source, schema, marker, exit code, or test
- introduce a Rust workspace member, Cargo manifest member, or any
  Rust crate
- introduce a CLI subcommand on `claw plan` for IDE operations
- introduce a CLI binary alongside `claw` for IDE operations
- introduce or imply autonomous workspace-write execution
- introduce IDE controls that approve, that apply, that apply-
  bundle, that run, or that compose any combination of those
- introduce IDE-driven retry, remediation, or rollback of any
  chain step
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  pre-approval, batch approval, or any approval-bypass affordance
- introduce a "fast", "shadow", "what-if", "preview-this", or
  "dry-run" mode that simulates downstream chain commands
- introduce a "trust this workspace" setting, a "trusted
  workspace" toggle, or any operator-facing affordance that
  loosens STOP visibility or refresh cadence
- introduce IDE-host file-change-event subscriptions, filesystem
  watchers, daemon channels, or background refresh
- introduce on-disk caches of envelope contents (the implementation
  lane MAY justify a single non-authoritative cache file for non-
  envelope-payload UI state — e.g. last-opened panel position —
  but the default refusal stands until pinned in the per-host
  scope card and never includes envelope contents)
- introduce cross-workspace dashboards, multi-run inventories, or
  history rollups
- introduce a parallel adapter contract for the IDE (the IDE
  adapter consumes `a2-l2d-status.v1` as-is, with no IDE-specific
  schema wrapper, marker, or envelope-version variant)
- introduce shared crates between the harness adapter and the IDE
  adapter without a separate scope-card lane
- modify `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, `claw plan apply`, or `claw plan status` behavior,
  exit codes, schemas, markers, or JSON field shapes
- modify `a2-l2b-*` or `a2-l2d-status.v1` schema versions or
  marker constants
- introduce an `a2-l3-*` schema, marker, exit code, or CLI surface
- call broker, model, or Ollama at any phase
- relay envelope contents to IDE-host telemetry, analytics, error-
  reporting, or marketplace endpoints
- weaken any A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter boundary, A2-L3
  harness adapter, A2-L3 harness adapter implementation, or
  A2-L3 IDE adapter STOP gate
- run against `/home/suki/stack-code`, `/home/suki/sidestackai`,
  or any production repository in any default test, fixture, or
  packaged operator-facing artifact

Any of the above must be opened as a separate, explicitly-
authorized lane.

## 22. Future Implementation STOP Gates

The implementation lane is a STOP gate failure if any of the
following hold at PR review time:

- a file outside the §7 allowed enumeration is touched
- any §8 forbidden surface is touched
- multiple IDE hosts are implemented in a single lane
- the harness adapter crate (`rust/crates/a2-harness-adapter/**`)
  is touched, imported, or depended on
- a new Rust crate or Cargo workspace member is introduced
- any §11 STOP golden matrix case is absent or coverage is partial
- any §12 evidence-path rendering test is absent
- any §13 refresh-boundary test is absent
- any §14 copy-to-clipboard test is absent
- any §15 no-write/no-approve/no-apply validation step is absent
- any §16 no-network validation step is absent
- any §17 no-`.claw/**` parsing validation step is absent
- any §18 security/secrets boundary test is absent
- any §19 disposable-workspace handling test is absent
- any §20 CI matrix check is skipped, disabled, or guarded behind
  an env var
- the IDE adapter invokes any subprocess other than `claw plan
  status`
- the IDE adapter spawns a subprocess outside an operator gesture
- the IDE adapter writes any file outside the host's non-
  authoritative cache directory (and even there, only with
  per-host scope-card justification for non-envelope-payload
  state)
- the IDE adapter depends on an HTTP client crate / package
- the IDE adapter depends on an IDE-host telemetry / analytics /
  error-reporting SDK
- the IDE adapter depends on a filesystem-watcher package
- the IDE adapter subscribes to an IDE-host file-change event
- the IDE adapter exposes any of the forbidden controls
  enumerated in the IDE adapter scope card §10 / §20:
  - approve button
  - apply button
  - apply-bundle button
  - run button
  - approve-and-apply button
  - automatic-approval setting
  - automatic-apply setting
  - batch approval
  - preapproval
  - one-click continue
  - trust-this-workspace mode
  - ignore STOP button
  - mute STOP button
  - dismiss STOP button
  - hide STOP button
  - inline approval-line input
  - terminal prefill of approval lines
  - command-palette approve/apply actions
  - keybinding approve/apply actions
  - context-menu approve/apply actions
  - gutter/lens/hover/status-bar write actions
  - drag-target write actions
- the IDE adapter renders chain state with adapter-authority
  framing (panel headers like "Chain Controller", "Apply Manager",
  "Approval Helper", "Chain Director")
- the IDE adapter substitutes friendly text for any closed-enum
  literal
- the IDE adapter relays envelope contents to telemetry,
  analytics, or marketplace endpoints
- the implementation scope card omits the touched-surface
  enumeration the lane will commit to
- the IDE adapter operates against `/home/suki/stack-code`,
  `/home/suki/sidestackai`, or any production repository in any
  default test, fixture, or packaged operator-facing artifact

Hitting any of these gates blocks the implementation lane. The
review path is to refuse the lane and to open a separate scope-
card lane addressing the underlying issue.

## 23. Definition Of Done

This **implementation scope card** is done when:

- `docs/a2-l3-ide-adapter-implementation-scope-card.md` exists
  and matches the sectional structure of this card.
- The card defines the recommended implementation shape (per-
  host extension package, default VS Code first, no Rust crate,
  no harness crate dependency).
- The card pins the allowed and forbidden future touched surfaces.
- The card pins the IDE input contract, the IDE output / display
  contract, the STOP rendering golden-test matrix, the evidence-
  path rendering design, the refresh/polling validation design,
  the copy-to-clipboard validation design, the no-write / no-
  approve / no-apply validation, the no-network / no-broker /
  no-model validation, the direct `.claw/**` parsing prohibition,
  the security/secrets boundary, the disposable-workspace
  handling, the CI/test matrix, the non-goals, and the future
  implementation STOP gates.
- The card explicitly states it authorizes design only; it does
  not authorize IDE implementation, adapter implementation,
  approve/apply/apply-bundle execution, approve/apply UI
  controls, or autonomous workspace-write execution.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter boundary, A2-L3
  harness adapter, A2-L3 harness adapter implementation, or
  A2-L3 IDE adapter STOP gate is weakened.
- A single cross-link line MAY be added to the A2-L3 IDE adapter
  scope card, the A2-L3 adapter boundary scope card, the A2-L3
  harness adapter scope card, the A2-L3 harness adapter
  implementation scope card, the A2-L3 harness adapter usage
  guide, the A2-L2d scope card, the A2-L2d status schema, or the
  A2-L2d operator quick reference if an obvious location exists,
  but no such cross-link is required for this scope card itself
  to land. *(This scope card is authored without cross-links to
  keep the lane strictly limited to a single new docs file;
  cross-links may be added in a follow-up lane.)*
- The card is reviewed by the operator before any IDE adapter
  implementation lane is opened.

The IDE adapter **implementation lane** is out of scope for this
card. Definition of done for that lane will be authored by the
implementation lane itself, bounded by §§6–22 above.

## 24. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **First per-host IDE adapter implementation lane (code-bearing,
> single host)** — open a single PR that creates `ide/vscode/
> claw-status-panel/` with the package manifest, TypeScript
> source, integration test suite covering every check in the §20
> CI matrix, the STOP rendering golden fixtures per §11, the
> evidence-path rendering tests per §12, the refresh-boundary
> tests per §13, the copy-to-clipboard tests per §14, the no-
> write / no-approve / no-apply tests per §15, the no-network /
> no-broker / no-model tests per §16, the no-`.claw/**` parsing
> tests per §17, the security/secrets tests per §18, and the
> disposable-workspace handling tests per §19. The PR's diff is
> bounded strictly by §§7–8 of this card.

The implementation lane MUST open with a top-of-PR comment that
enumerates the exact subset of §7 it touches; touching anything
beyond that enumeration is a STOP gate (§22).

Lanes that follow the first per-host implementation lane, in
order:

> **First-host IDE adapter usage documentation lane (docs-only)**
> — author `docs/a2-l3-ide-adapter-usage.md` (or a per-host
> variant) and cross-link from the README. May fold into the
> first per-host implementation lane if the implementation lane's
> reviewer accepts the docs additions alongside the package.

> **Subsequent per-host IDE adapter implementation lanes** —
> JetBrains plugin, language-server-backed panel, or other host,
> each in its own PR, each bounded by this card and the first-
> host implementation lane's worked example. None of these lanes
> permits autonomous workspace-write execution.

> **Workflow change lane (docs-only or workflow-only)** — only if
> the first per-host implementation lane discovered that an
> existing workflow cannot exercise a required §20 CI check (e.g.
> Xvfb for `@vscode/test-electron`). That lane is bounded by the
> exact workflow file(s) it adds and the exact CI check(s) it
> enables.

None of these lanes permits autonomous workspace-write execution.
None permits approve / apply / apply-bundle UI controls. All
remain bounded by the A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter
boundary, A2-L3 harness adapter, A2-L3 harness adapter
implementation, and A2-L3 IDE adapter safety properties.

## 25. References

- [`a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md)
  — A2-L3 IDE Adapter Scope Card; the parent card this per-
  implementation card refines into concrete touched-surface and
  validation constraints.
- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 Adapter Boundary Scope Card; the cross-adapter
  constraints any per-adapter implementation must hold to.
- [`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md)
  — A2-L3 Harness Adapter Scope Card; the sibling per-adapter
  scope card whose validation matrix and STOP-rendering taxonomy
  inform this card's IDE-side equivalent.
- [`a2-l3-harness-adapter-implementation-scope-card.md`](./a2-l3-harness-adapter-implementation-scope-card.md)
  — A2-L3 Harness Adapter Implementation Scope Card; the
  structural model this card mirrors for the IDE-side surface.
- [`a2-l3-harness-adapter-usage.md`](./a2-l3-harness-adapter-usage.md)
  — A2-L3 Harness Adapter Usage Guide; the operator-facing
  companion to the merged harness adapter, useful as a model for
  the future IDE adapter usage guide.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card; section 10 ("IDE / Harness Boundary") is
  the original preamble that A2-L3 expanded.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record. Authoritative on the
  contract the IDE adapter consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference; TTY approval EOF note in §3 is
  load-bearing for the approval boundary the IDE adapter must
  never compose around.
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the
  handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.
- PR #39 (`12fff14`) — A2-L2d scope card.
- PR #40 (`0f75800`) — A2-L2d read-only `claw plan status`
  command + `a2-l2d-status.v1`.
- PR #41 (`4c2b15e`) — A2-L2d operator quick reference.
- PR #42 (`21d9b5b`) — A2-L3 adapter boundary scope card.
- PR #44 (`f63d5ac`) — A2-L3 harness adapter scope card.
- PR #45 (`97e9d9b`) — A2-L3 harness adapter implementation scope
  card.
- PR #46 (`c171d11`) — A2-L3 read-only harness adapter crate.
- PR #47 (`90819e8`) — A2-L3 harness adapter usage guide.
- PR #48 (`2930d21`) — A2-L3 PR43 harness assertions preservation
  patch.
- PR #49 (`8d520e6`) — A2-L3 IDE adapter scope card.

## 26. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- IDE adapter implementation: **not started; not authorized by
  this card**.
- First per-host implementation lane authored: **no** (separate
  future lane).
- Harness adapter modified: **no** (the merged harness adapter
  surfaces remain authoritative on their own surface).
- Harness adapter crate dependency introduced: **no**.
- New Rust crate or Cargo workspace member introduced: **no**.
- Autonomous-write authorization: **none granted**.
- Approval / apply boundary weakened: **no**.
- Approve / apply / apply-bundle execution authorized: **no**.
- Approve / apply UI controls authorized: **no**.
- Approve / apply / apply-bundle composition authorized: **no**.
- Background polling / filesystem watcher authorized: **no**.
- Direct `.claw/**` parsing authorized: **no**.
- IDE mutation of workspace files authorized: **no**.
- IDE mutation of `.claw/**` authorized: **no**.
- A2-L2b / A2-L2c / A2-L2d / A2-L3-boundary / A2-L3-harness /
  A2-L3-harness-implementation / A2-L3-IDE STOP gate weakened:
  **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- A2-L3 adapter boundary card, A2-L3 harness adapter cards, or
  A2-L3 IDE adapter scope card modified: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by the first per-host IDE adapter implementation
  lane bounded by §§6–22 above.
