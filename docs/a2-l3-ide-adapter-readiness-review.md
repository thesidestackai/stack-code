# A2-L3 IDE Adapter — Readiness Review

This document is the readiness-review record for the first A2-L3
IDE Adapter Implementation lane. It captures the state of the A2
stack at the gate between the merged IDE adapter implementation
scope card ([PR #50, commit `e98c92d`](https://github.com/thesidestackai/stack-code/pull/50))
and the not-yet-opened first per-host IDE adapter implementation
lane.

It is **documentation only**. It does **not** authorize IDE
implementation, adapter implementation, runtime changes, write
controls, approve/apply/apply-bundle execution, autonomous
workspace-write execution, or weakening of any A2-L2b / A2-L2c /
A2-L2d / A2-L3 STOP gate. The first per-host IDE adapter
implementation lane is a separate, future, explicitly-authorized
lane bounded by the IDE adapter scope card
([`a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md))
and the IDE adapter implementation scope card
([`a2-l3-ide-adapter-implementation-scope-card.md`](./a2-l3-ide-adapter-implementation-scope-card.md)).

## 1. Summary

The first A2-L3 IDE adapter implementation lane is **ready to
open**.

The decision derives from three findings:

1. The IDE adapter implementation scope card is concrete (47-check
   CI matrix, exact static-grep patterns, exact STOP-rendering
   golden matrix, first-host VS Code justification, every
   forbidden control enumerated).
2. The merged harness adapter crate
   ([`rust/crates/a2-harness-adapter/`](../rust/crates/a2-harness-adapter/))
   passes its full integration test surface from a fresh
   `cargo test -p a2-harness-adapter` invocation, exercising every
   closed-enum path, every STOP signal kind, every schema-drift
   mode, every idempotency property, every no-write guarantee,
   every argv audit, and every PR43 preservation case. This closes
   the consumer-signal gap raised by the prior decision gate.
3. The first per-host target (VS Code) remains correct under the
   IDE adapter implementation scope card §6 justification —
   lowest-friction operator reach, mature test harness
   (`@vscode/test-electron`), single-manifest forbidden-affordance
   audit surface.

No contract gap, no STOP-gate weakening, no scope ambiguity
surfaced during the review.

## 2. Current State

Merged sequence on `origin/main`:

```text
e98c92d docs(a2-l3): add IDE adapter implementation scope card (#50)
8d520e6 docs(a2-l3): add IDE adapter scope card (#49)
2930d21 test(a2-l3): preserve PR43 harness assertions (#48)
90819e8 docs(a2-l3): add harness adapter usage guide (#47)
c171d11 feat(a2-l3): add read-only harness adapter crate (#46)
97e9d9b docs(a2-l3): add harness adapter implementation scope card (#45)
f63d5ac docs(a2-l3): add harness adapter scope card (#44)
21d9b5b docs(a2-l3): add adapter boundary scope card (#42)
4c2b15e docs(a2-l2d): add operator quick reference (#41)
0f75800 feat(a2-l2d): add read-only claw plan status command and a2-l2d-status.v1 (#40)
```

The A2 stack at this gate:

- **A2-L2b**: runtime-proven preview → approve → apply-bundle →
  apply chain; unchanged by A2-L3.
- **A2-L2c**: operator quick reference; unchanged by A2-L3.
- **A2-L2d**: `claw plan status` + `a2-l2d-status.v1` schema +
  read-only / network-egress-free / idempotency invariants;
  unchanged by A2-L3.
- **A2-L3 harness**: adapter boundary card (#42) + harness adapter
  scope card (#44) + harness implementation scope card (#45) +
  read-only harness adapter crate (#46) + usage guide (#47) +
  PR43 preservation patch (#48). Crate has 56 tests across 10
  test binaries, all passing.
- **A2-L3 IDE**: IDE adapter scope card (#49) + IDE adapter
  implementation scope card (#50). No code, no extension package
  yet.

## 3. IDE Scope Readiness

The IDE adapter implementation scope card pins:

- 47-check CI matrix (§20) covering install, lint, type-check,
  test, STOP golden matrix, refusal envelope matrix, unknown-enum
  matrix, schema-drift matrix, sibling-harness preservation
  matrix, STOP-visibility rules, evidence-path rendering,
  refresh-boundary, copy-to-clipboard boundary, no-write, no-
  network, no-`.claw/**` parsing, security/secrets, and
  disposable-workspace handling.
- exact static-grep patterns for forbidden APIs (e.g. `chokidar`,
  `setInterval`, `axios`, `fs.writeFile`, `vscode.workspace.
  createFileSystemWatcher`, `claw plan run|approve|apply-bundle|
  apply`).
- exact STOP-rendering golden matrix per closed `phase` /
  `stop_condition` value, refusal envelope, unknown-enum synthetic
  fixtures, schema-drift fixtures, and PR43 preservation fixtures.
- allowed touched-surface enumeration per host (default first-host
  `ide/vscode/claw-status-panel/`).
- forbidden touched-surface enumeration (every A2-L2b module, the
  A2-L2d producer, `rust/crates/a2-harness-adapter/**`, workspace
  `Cargo.toml` / `Cargo.lock`, every non-IDE-adapter workspace
  crate, `.github/workflows/**`, `.claw/**`, other IDE host
  directories).
- forbidden-control enumeration (approve / apply / apply-bundle /
  run / approve-and-apply / automatic-approval / automatic-apply /
  batch approval / preapproval / one-click continue / trust-this-
  workspace / ignore/mute/dismiss/hide STOP / inline approval-line
  input / terminal prefill / write-step command-palette /
  keybinding / context-menu / gutter / lens / hover / status-bar /
  drag-target actions).

No contract gap surfaced during scope-card authoring (PRs #49 and
#50). The scope is bounded enough to enter implementation with the
PR's diff strictly governed by §§7–8 of the implementation scope
card.

## 4. First Host Decision

The first per-host implementation lane targets **VS Code** under
`ide/vscode/claw-status-panel/`.

Justification (per IDE adapter implementation scope card §1, §6):

- broadest operator-reachable footprint of any single IDE host
- mature integration-test harness (`@vscode/test-electron`)
  capable of headless host-driven test execution
- single-manifest (`package.json`) forbidden-affordance audit
  surface — every command, keybinding, and context-menu binding
  is one regex-able place
- TypeScript native to the host — the small `a2-l2d-status.v1`
  envelope schema is trivially re-deriveable in TypeScript
  without taking a Cargo dependency on the harness adapter crate
  (which the IDE adapter scope card §4 explicitly forbids)

JetBrains plugin and language-server-backed panel hosts follow in
**separate per-host implementation lanes**, each its own PR. The
first lane MUST NOT implement more than one host (§22 STOP gate).

## 5. Operator Value

A read-only IDE panel surfacing `claw plan status` envelope state
materially improves operator workflow over the current terminal-
only flow:

- the operator no longer re-types `claw plan status <workspace>`
  on every chain transition; one click in the panel refreshes the
  rendered envelope
- `next_operator_command` is a copyable string the operator can
  paste into their terminal without retyping
- `evidence_paths` become clickable file links opening in the
  operator's editor
- STOP conditions surface visually at parity with non-STOP state,
  which is harder to miss than a single line in a terminal scroll-
  back

The marginal value is modest (the terminal command already works)
but real. The IDE panel does not replace the terminal — it
surfaces chain state in the place the operator is already editing.

## 6. Harness Consumer Signal

The harness adapter has been **consumed externally** by this
readiness review through a fresh `cargo test -p a2-harness-adapter`
invocation. Results:

```text
56 tests across 10 test binaries, all passing.

Test binary breakdown:
- unit tests (lib): 15 passed
- argv_audit: 3 passed
- config_refusal: 5 passed
- disposable_classifier: 6 passed
- expected_matrix: 5 passed
- idempotency: 3 passed
- network_sentinel: 2 passed
- no_write: 2 passed
- pr43_preservation: 7 passed
- schema_drift: 8 passed
```

Coverage exercised:

- every closed `phase` enum value
- every closed `stop_condition` enum value
- every closed `next_operator_command` shape including the
  drift case
- the refusal envelope (`EXIT_STATUS_REFUSED == 12`) including the
  presence and absence of the `a2-l2d-status-refused` marker
- the unknown-enum synthetic fixtures for `phase`, `stop_condition`,
  and `next_operator_command`
- the schema-drift cases (missing/substituted `read_only_invariant`,
  schema-version mismatch, invalid JSON, unknown audit marker)
- idempotency (byte-identical paired stdout, non-byte-identical
  paired stdout raising STOP, independent-subprocess invariant)
- network-sentinel propagation to the spawned subprocess
- no-write guarantees (marker file unchanged after classifier,
  fixture tree byte-identical before and after cycle)
- caller-expectation matrix (expected continue vs observed STOP,
  expected STOP vs observed continue, wrong STOP value)
- disposable-workspace classifier (AND-semantics over four
  signals, refusal path, authorisation-doc-recorded path)
- config refusal (chain-write subcommand references refused at
  parse time)
- argv audit (only `claw plan status` spawned, no flags, at most
  two positional arguments)
- PR43 preservation (every fixture from the post-merge correction
  cycle)

This is not equivalent to a full operator-in-CI consumption
signal, but it is sufficient evidence that the harness adapter
crate's API is stable, its STOP taxonomy is correct, its no-write
guarantees hold, and its parser handles every drift case. The
gap from the prior decision gate
("the harness adapter has not yet been consumed externally") is
**closed for the purpose of authorizing the IDE implementation
lane**.

A real operator-in-CI consumption is still desirable and remains
an open opportunity, but it is not a blocking dependency for the
IDE adapter implementation. The IDE adapter is independent of the
harness adapter crate by IDE adapter scope card §4 (no Cargo
dependency, no shared parser), so any future harness API
adjustment is isolated to the harness surface.

## 7. Remaining Risks

The risks that remain at this gate, in order of materiality:

1. **Visual workflow-control pressure.** An IDE panel with a
   `next_operator_command` field is one accidental refactor away
   from "click here to run that command for you." The IDE adapter
   implementation scope card §22 pins this as a STOP gate via
   static-grep guards on the package manifest, the source, and
   the host-affordance bindings. The first-host implementation
   lane must hold to those guards.
2. **VS Code-specific affordance vocabulary drift.** VS Code's
   extension API exposes file-change events
   (`vscode.workspace.onDidSaveTextDocument`,
   `onDidChangeTextDocument`, etc.) that the implementation lane
   could accidentally subscribe to. The implementation scope card
   §13 pins static-grep guards against each of these explicitly.
3. **Marketplace / telemetry SDK temptation.** A VS Code
   extension package can integrate with `@vscode/extension-
   telemetry`, error-reporting SDKs, and marketplace endpoints
   trivially. The IDE adapter implementation scope card §16
   pins dependency-audit and static-grep guards refusing each.
4. **Cross-host re-implementation overhead** (future lanes). The
   IDE adapter scope card §4 forbids the harness adapter as a
   Cargo dependency; each per-host lane re-derives parser /
   STOP taxonomy in the host-native language. This is intentional
   (sibling adapters, not parent-child), but it does mean
   maintaining `a2-l2d-status.v1` schema parity across hosts is
   the operator's job. JetBrains and LSP-backed panels follow in
   their own lanes.

None of these risks block the first per-host implementation lane;
each is mitigated by an explicit static-grep guard or CI matrix
check in the IDE adapter implementation scope card.

## 8. Decision

**READY_FOR_IDE_IMPLEMENTATION.**

The first A2-L3 IDE adapter implementation lane (VS Code,
`ide/vscode/claw-status-panel/`) is authorized to open as a
separate, fresh-worktree PR bounded strictly by:

- the IDE adapter scope card
  ([`a2-l3-ide-adapter-scope-card.md`](./a2-l3-ide-adapter-scope-card.md))
- the IDE adapter implementation scope card
  ([`a2-l3-ide-adapter-implementation-scope-card.md`](./a2-l3-ide-adapter-implementation-scope-card.md))
- the A2-L3 adapter boundary scope card
  ([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md))
- the safety invariants pinned in every A2-L2b / A2-L2c / A2-L2d
  card

This readiness review **does not** authorize:

- IDE implementation in the same PR as its scope card
- multi-host implementation in a single PR
- harness adapter modification
- A2-L2b / A2-L2c / A2-L2d source or schema modification
- approve / apply / apply-bundle / run UI controls
- approve / apply composition
- autonomous workspace-write execution
- background polling, filesystem watchers, daemon channels
- direct `.claw/**` parsing
- broker / model / Ollama / telemetry / analytics traffic
- workspace mutation
- `.claw/**` mutation

The implementation lane MUST open with a top-of-PR comment that
enumerates the exact subset of IDE adapter implementation scope
card §7 it touches. Touching anything beyond that enumeration is
a STOP gate (IDE adapter implementation scope card §22).

## 9. Next Lane Recommendation

The recommended next lane is:

> **First per-host IDE adapter implementation lane (code-bearing,
> VS Code only)** — open a single PR that creates
> `ide/vscode/claw-status-panel/` with the package manifest,
> TypeScript source re-deriving the `a2-l2d-status.v1` envelope
> parser and STOP taxonomy in the host-native language, the
> integration test suite covering every check in the IDE adapter
> implementation scope card §20 CI matrix, the STOP rendering
> golden fixtures per §11, the evidence-path rendering tests
> per §12, the refresh-boundary tests per §13, the copy-to-
> clipboard tests per §14, the no-write / no-approve / no-apply
> tests per §15, the no-network / no-broker / no-model tests
> per §16, the no-`.claw/**` parsing tests per §17, the
> security/secrets tests per §18, and the disposable-workspace
> handling tests per §19. The PR's diff is bounded strictly by
> §§7–8 of the IDE adapter implementation scope card.

The implementation lane is a separate PR. JetBrains plugin and
LSP-backed panel hosts follow in their own per-host PRs after the
VS Code lane lands.

The implementation lane does not permit autonomous workspace-write
execution. It does not permit approve / apply / apply-bundle UI
controls. It does not permit approve / apply composition. It
remains bounded by every A2-L2b / A2-L2c / A2-L2d / A2-L3 safety
property.
