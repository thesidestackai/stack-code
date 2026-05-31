# A2-L3 Scope Card — Harness Adapter Implementation (Docs-Only)

This document is a **design-only implementation scope card** for the
future A2-L3 Harness Adapter. It is bounded by the merged A2-L3
Harness Adapter Scope Card
([`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md))
and the merged A2-L3 Adapter Boundary Scope Card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)).

This card defines, in design only, the concrete future
implementation constraints for the harness adapter — recommended
shape, allowed touched surfaces, forbidden touched surfaces, the
validation matrix the future implementation lane must hold to, and
the STOP gates that future implementation review must enforce. It
itself authorizes **no** runtime change, **no** new code, **no**
harness execution, **no** test runs, **no** broker/model/Ollama
traffic, **no** IDE work, and **no** harness execution against a
non-disposable workspace.

A2-L3 progression as of this card:

```text
safe write chain (A2-L2b, runtime-proven)
  → operator docs (A2-L2c, copy-pasteable)
    → read-only status / inspection contract (A2-L2d, shipped)
      → IDE / harness adapter boundary (A2-L3, scope card shipped, PR #42)
        → harness adapter per-adapter scope card (A2-L3, shipped, PR #44)
          → harness adapter IMPLEMENTATION scope card (THIS DOCUMENT)
            → future harness adapter implementation (separate, future)
```

The IDE adapter per-adapter scope card is a separate, future lane.
This card does not author, authorize, or pre-empt it.

## 1. Executive Summary

A2-L3 Harness Adapter Implementation is the next lane after the
harness adapter scope card landed at
[`f63d5ac` on `origin/main` (PR #44)](https://github.com/thesidestackai/stack-code/pull/44).
This card is the per-implementation scope card that
[`a2-l3-harness-adapter-scope-card.md` §17](./a2-l3-harness-adapter-scope-card.md#17-future-implementation-constraints)
requires before any harness implementation code is authored.

The recommended future implementation shape is a **small dedicated
Rust crate at `rust/crates/a2-harness-adapter/`** that exposes a
read-only library API for parsing and asserting against
`a2-l2d-status.v1` envelopes emitted by the shipped
`claw plan status` command. The crate spawns `claw plan status` as
a subprocess (its only spawned process), parses stdout as the pinned
envelope schema, runs caller-declared assertions, and emits
structured pass/fail output at full envelope fidelity. It performs
no other subprocess execution, no filesystem write, no network
egress, and no envelope persistence as authoritative state.

This card declares this shape as the **default future
implementation shape** because:

- A library-first crate keeps the harness consumable from tests, CI
  steps, and operator scripts without forcing a CLI surface that
  could grow into a workflow controller.
- A dedicated crate gives the future implementation lane a clean
  forbidden-vs-allowed enforcement target: any change to
  `rust/crates/a2-plan-runner/src/status.rs` (the producer) or to
  any A2-L2b module is a category violation visible in the diff.
- Tests live as Cargo integration tests inside the crate, where
  network-egress sentinels, disposable-tempdir workspaces, and STOP
  golden fixtures can be enforced in isolation from the runtime
  chain.

This card defines the boundary the future harness adapter
implementation lane must hold to. The next gate before that
implementation is operator review of this scope card. This card
authorizes **only** the future implementation lane to be *opened
under these constraints*; it does not itself author code, run tests,
or merge anything.

## 2. Why This Scope Card Exists

The merged A2-L3 harness adapter scope card §14 and §17 explicitly
require a per-implementation scope card before any code is written.
That requirement exists because:

1. **Behavioral boundaries are not implementation boundaries.** The
   harness adapter scope card defines what the harness adapter *is
   and is not* in behavioral terms. The implementation lane needs
   answers to different questions: which files may be touched, which
   crates may be modified, which Cargo manifests grow, which tests
   are golden fixtures vs runtime assertions, how the CI pipeline
   validates the boundary. A separate scope card is the right place
   for those answers, because the wrong answer (e.g. modifying
   `status.rs` to add a "convenience field" the harness wants) would
   leak the adapter boundary back into the producer.

2. **Implementation lanes drift without a pinned scope card.** A
   harness implementation lane that opened without a pinned
   touched-surface enumeration would have no way to refuse a
   plausible scope creep ("just add one helper to a2-plan-runner so
   the harness doesn't have to re-parse"). With the enumeration
   pinned here, scope creep is visible in the diff.

3. **STOP-rendering coverage is non-trivial.** The harness must
   correctly render every closed `stop_condition` value, every
   closed `phase` value, the refusal envelope, and at least one
   unknown-enum-value synthetic fixture per closed enum. That golden
   matrix is large enough that defining it here — before code is
   written — prevents the implementation lane from shipping with
   partial coverage and discovering gaps in operator review.

4. **Disposable-workspace classification is a design question.** The
   harness must refuse non-disposable workspaces by default. The
   exact classifier (a marker file, a workspace-config field, a
   tempdir prefix, a path-prefix allowlist) is an implementation
   choice this card pins as a design requirement so it is not
   omitted under time pressure.

5. **CI validation matrix needs to exist before code does.** The
   future implementation lane will be reviewed against the CI
   matrix; the matrix needs to be written down first so the review
   is not "did the author remember every check" but "does the diff
   exercise every pinned check."

## 3. Relationship To A2-L3 Harness Adapter Scope Card

The merged harness adapter scope card
([`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md))
pinned:

- responsibilities and non-responsibilities (§§5–6)
- allowed reads and forbidden actions (§§7–8)
- input and output/reporting contracts (§§9–10)
- STOP-handling rules (§11)
- idempotency-and-repeatability rules (§12)
- CI / test-harness boundary (§13)
- disposable-workspace requirement (§14)
- safety invariants (§15)
- non-goals (§16)
- future-implementation constraints (§17)
- definition of done and next-lane recommendation (§§18–19)

This implementation scope card is **subordinate** to those pins. It
does not modify, soften, or expand them. Where this card adds
detail (touched-surface enumeration, CI matrix, golden-fixture
coverage, classifier design), the detail must remain consistent with
the harness adapter scope card; any tension is resolved in favor of
the harness adapter scope card, and any contract gap discovered
during implementation is escalated as a separate scope-card lane
rather than worked around here.

## 4. Relationship To A2-L3 Adapter Boundary

The A2-L3 adapter boundary scope card
([`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md))
pinned the overall adapter boundary, including its four cross-
adapter invariants (read-only, STOP-visibility, no-write-surface,
no-state-invention, no-shadow-contract). The harness adapter
implementation lane bounded by this card preserves every one of
those invariants verbatim. This card adds harness-specific
mechanical constraints (touched surfaces, CI matrix) but does not
introduce any cross-adapter contract. Any future implementation
finding that suggests modifying the boundary card must be opened as
a separate scope-card lane against the boundary card.

## 5. Relationship To A2-L2d Status Contract

The A2-L2d `a2-l2d-status.v1` schema-of-record
([`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md)) is the
contract the harness adapter consumes. This card does **not**:

- modify `a2-l2d-status.v1` (no field additions, no field removals,
  no field-order changes)
- add new `phase`, `stop_condition`, `next_operator_command`, or
  marker values
- add new exit codes
- change `EXIT_STATUS_REFUSED == 12` semantics
- add new CLI subcommands, flags, or positional arguments to
  `claw plan status`
- change `rust/crates/a2-plan-runner/src/status.rs` or its tests

If the implementation lane discovers that a missing field, marker,
or enum value is required to honor the harness behavioral contract,
that discovery is escalated as a separate scope-card lane against
A2-L2d. The harness implementation does not work around a contract
gap; it surfaces the gap and waits.

## 6. Recommended Implementation Shape

The recommended future implementation shape, subject to operator
review and modification in the implementation lane itself, is:

- **A new Rust crate at `rust/crates/a2-harness-adapter/`** with a
  library `lib.rs` and Cargo integration tests under
  `rust/crates/a2-harness-adapter/tests/`.
- **No new CLI binary by default.** The implementation lane MAY add
  a small Cargo `[[bin]]` target inside the crate (e.g. `claw-
  harness-status` or similar) if a CLI entry point is required for
  external consumers, but the binary is a thin wrapper over the
  library API and adds no flags or affordances beyond what the
  library exposes. The default expectation is **library-only**; a
  binary is opt-in for the implementation lane to justify.
- **The workspace Cargo manifest gains one member entry** for the
  new crate. No other workspace-level change.
- **The new crate depends only on `serde`, `serde_json`, `sha2`
  (read-only digest verification for idempotency comparisons), and
  the project's existing `Cargo.lock`-pinned dependency set.** It
  does **not** depend on `a2-plan-runner`, `rusty-claude-cli`, or
  any other workspace crate; it interacts with the chain
  exclusively through the `claw plan status` subprocess.
- **The new crate's library API exposes** a `HarnessRunCycle`
  function (or equivalently-named entry point) that accepts a
  caller-supplied `HarnessAssertionConfig`, invokes
  `claw plan status` as a subprocess, parses stdout, runs the
  assertion set, and returns a `HarnessRunReport`. Concrete names,
  field layouts, and error types are deferred to the implementation
  lane; the surface must remain bounded to read + parse + assert +
  report.
- **The new crate's test suite contains golden fixtures** for every
  closed enum value, refusal envelope, and unknown-enum synthetic
  case (§12), plus idempotency-mismatch fixtures (§14) and disposable-
  workspace classification fixtures (§11).
- **A small documentation file at `docs/a2-l3-harness-adapter-
  usage.md`** explaining the library API, the disposable-workspace
  requirement, and the CI pattern for using the harness in a test
  pipeline. This is a future docs deliverable, not authored here.

Uncertainty markers (the implementation lane is encouraged to
deviate with justification):

- *crate name*: `a2-harness-adapter` is the default; alternative
  names that preserve the `a2-` prefix and convey "read-only
  harness" are acceptable in the implementation lane scope card.
- *binary inclusion*: opt-in, default no binary.
- *dependency on `sha2`*: justified by idempotency comparison; if
  the implementation lane finds an alternative SHA digest crate
  already in `Cargo.lock`, that may substitute.

## 7. Allowed Future Touched Surfaces

The future harness adapter implementation lane MAY touch only the
files in the explicit enumeration below. The implementation lane
MUST NOT touch any file outside this enumeration.

- `rust/crates/a2-harness-adapter/Cargo.toml` (new file)
- `rust/crates/a2-harness-adapter/src/lib.rs` (new file)
- `rust/crates/a2-harness-adapter/src/<submodules>.rs` (new files,
  module names left to the implementation lane within the
  `a2-harness-adapter` crate's source tree)
- `rust/crates/a2-harness-adapter/tests/<test files>.rs` (new files,
  Cargo integration tests scoped to this crate only)
- `rust/crates/a2-harness-adapter/tests/fixtures/<golden files>`
  (new files, deterministic JSON fixtures for the STOP/phase/refusal
  matrix and the unknown-enum synthetic cases)
- Workspace `Cargo.toml` — exactly one addition: the new crate's
  path under the workspace `members` array. No other workspace-
  level edit.
- `Cargo.lock` — only the deltas required by the crate's declared
  dependencies, generated by Cargo at build time and committed
  alongside the crate addition.
- `docs/a2-l3-harness-adapter-usage.md` (new file, optional in the
  implementation lane; if the implementation lane defers usage docs
  to a follow-up lane, that is acceptable).
- Optionally one new line in `README.md` cross-linking the crate
  and this scope card, if an obvious location exists. The
  implementation lane MAY omit this cross-link; no further README
  edits are within scope.

The implementation lane MUST enumerate, in its own implementation
scope card preamble (a top-of-PR comment is acceptable), the exact
subset of these files it touches. Touching a file outside the
enumeration is a STOP gate (§20).

## 8. Forbidden Future Touched Surfaces

The future harness adapter implementation lane MUST NOT touch any
of the following files. Each forbidden surface maps to a named
safety property the chain depends on.

- `rust/crates/a2-plan-runner/src/status.rs` — the A2-L2d producer.
  The harness adapter consumes its stdout; modifying the producer
  to accommodate the harness is a category violation.
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
  remain authoritative; the harness adapter MUST NOT add tests
  there.
- `rust/crates/rusty-claude-cli/src/**` — no new CLI subcommand,
  flag, or argument is in scope.
- `rust/crates/rusty-claude-cli/tests/**` — existing
  `plan_status.rs` and friends remain authoritative; the harness
  adapter MUST NOT add CLI integration tests there.
- `rust/crates/api/**`, `rust/crates/commands/**`,
  `rust/crates/compat-harness/**`,
  `rust/crates/mock-anthropic-service/**`,
  `rust/crates/plugins/**`, `rust/crates/runtime/**`,
  `rust/crates/telemetry/**`, `rust/crates/tools/**` — none of
  these are part of the harness adapter surface.
- `wrappers/**`, `bin/**`, `examples/**` — no operator-facing
  wrapper or example is in scope; usage examples (if any) live
  inside the new crate's own README or in the optional usage docs.
- `.github/**` — no new workflow file or workflow change. Existing
  CI workflows will pick up the new crate automatically through
  the workspace member addition; no workflow editing is required
  or in scope.
- `scripts/**`, `Makefile`, `justfile`, or any other top-level
  build script — none of these are touched.
- `SideStackAI/**` — out of scope by the cross-project boundary
  the operator pinned.
- `.claw/**` in any repository, including any workspace the
  harness operates against — the harness writes nothing under
  `.claw/`.

The IDE adapter's per-adapter scope card, when authored, will add
its own forbidden-surface list that will overlap with this one. The
harness adapter implementation lane MUST NOT depend on, or
anticipate, any IDE-adapter file path.

## 9. Input Contract

The future harness adapter accepts the following caller-supplied
inputs. This restates the harness adapter scope card §9 in
implementation terms; the constraints themselves are unchanged.

- **Workspace root** (required, owned by caller). The harness
  forwards this path verbatim to the `claw plan status` subprocess.
  The harness MUST NOT canonicalize, expand, normalize, or
  substitute the path before invocation.
- **Optional approval-result path** (optional, owned by caller).
  Passed verbatim as the second positional argument to
  `claw plan status` when supplied. The harness MUST NOT read,
  write, generate, or mutate the file's contents itself.
- **Expected `phase`** (optional). Closed-enum value; the harness
  refuses unknown expected values at config parse time.
- **Expected `stop_condition`** (optional, nullable). Closed-enum
  value or `null`. The harness refuses unknown expected values at
  config parse time.
- **Expected `read_only_invariant`** (optional). Defaults to the
  literal `"this command does not mutate state"`. Supplying a
  different expected value is a misuse the harness MUST refuse at
  config parse time.
- **Expected evidence-path patterns** (optional). The implementation
  lane chooses match semantics (exact, glob, or regex) and documents
  the choice; whichever semantics are chosen MUST NOT permit a
  STOP-relevant evidence path to be matched away by a permissive
  pattern.
- **Repeat-invocation policy** (optional). Names the count and
  ordering of caller-initiated re-invocations within one assertion
  cycle (e.g. for idempotency assertions). The harness MUST NOT
  implicitly repeat invocations the caller did not request.
- **Disposable-workspace assertion** (required, see §11). The
  caller MUST supply an assertion that classifies the workspace as
  disposable; the harness refuses to operate against a non-
  disposable workspace unless the caller has supplied a per-
  deployment scope-card reference (a doc path the harness records
  in its report; the harness does not parse the doc).

The harness implementation lane MUST refuse any input that would
direct it to invoke `claw plan run`, `claw plan approve`,
`claw plan apply-bundle`, `claw plan apply`, or any non-status
subprocess. Refusal happens at config-parse time, not at invocation
time.

## 10. Output / Reporting Contract

The future harness adapter emits the following reporting output per
invocation cycle. This restates the harness adapter scope card §10
in implementation terms; the constraints themselves are unchanged.

- **Pass/fail classification.** A single per-cycle result.
- **Parsed `a2-l2d-status.v1` envelope.** Every field preserved.
- **Raw stdout capture.** The byte string the subprocess emitted,
  preserved exactly for byte-identical idempotency comparison.
- **Exit code.** The integer exit code from the status subprocess.
- **Per-assertion summary.** Name, expected, observed, pass/fail
  per assertion.
- **Full-fidelity `stop_condition`, `evidence_paths`,
  `audit_markers`.** Verbatim, no redaction or summarization, at
  every supported log level.
- **Diagnostic message.** Supplementary, non-load-bearing.
- **Disposable-workspace classification record.** The classifier's
  decision (`disposable` / `non-disposable-but-authorized-by:<doc-
  ref>`) and the inputs it received (path, marker file, etc.).
- **Per-cycle invocation metadata.** Subprocess argv (excluding any
  caller-supplied secrets), invocation timestamp, exit code, and a
  pointer to the raw stdout capture. No PID, no hostname, no
  broker metadata; the harness emits nothing the A2-L2d producer
  itself does not already emit on stdout.

The implementation lane chooses the output container format (e.g.
JSON, NDJSON, structured-log sink) and pins it in the
implementation scope card; whichever container is chosen, STOP-
relevant content remains at full fidelity at every log level.

## 11. Disposable Workspace Classification Design

The disposable-workspace requirement pinned in the harness adapter
scope card §14 requires the future implementation to surface a
runtime classifier. This card pins the classifier's design.

The classifier:

- accepts the workspace path the caller supplied;
- accepts an optional caller-supplied per-deployment scope-card
  doc reference (a path string and a short rationale);
- emits a `WorkspaceClassification` decision: `disposable`,
  `non-disposable-and-refused`, or `non-disposable-but-authorized-
  by:<doc-ref>`;
- records the decision in the harness report;
- refuses to invoke `claw plan status` when the decision is
  `non-disposable-and-refused`.

The classifier MUST decide via a combination of the following
signals. The implementation lane chooses which subset to enforce
and pins that choice in its own scope card; the default expectation
is to enforce *all* of them with AND semantics:

1. **Path-prefix allowlist.** The workspace path MUST be under a
   caller-configured allowlist of disposable roots (e.g. the
   system tempdir, a CI-runner workdir, a per-test tempdir). The
   default allowlist is empty; the caller must configure it.
2. **Marker file.** The workspace MUST contain a marker file at a
   pinned relative path (e.g. `.claw/harness-disposable.marker`)
   whose contents the classifier reads. The marker file's
   existence does not by itself imply disposability; it is one
   AND-signal among several.
3. **Workspace owner.** The workspace's containing directory MUST
   be owned by the harness-running user (or the CI runner user)
   rather than the operator's primary user. Implementation lane
   pins the exact check.
4. **Explicit caller declaration.** The caller's
   `HarnessAssertionConfig` MUST include an explicit
   `workspace_is_disposable: true` flag for the classifier to
   classify as disposable. The flag is mandatory; omission
   classifies as `non-disposable-and-refused`.

Forbidden classifier behaviors:

- The classifier MUST NOT silently default to `disposable` when
  signals are missing.
- The classifier MUST NOT accept "the caller said so" alone; the
  explicit-declaration signal is one AND-signal, not the whole
  classifier.
- The classifier MUST NOT reclassify a workspace mid-cycle.
- The classifier MUST NOT write any file as a side effect of
  classification.

The non-disposable-but-authorized path is reserved for future
deployment-specific scope cards. The harness adapter implementation
lane MAY surface the path's existence in its API; it MUST NOT
exercise the path in its default tests.

## 12. STOP Rendering Golden-Test Matrix

The future implementation lane's Cargo integration tests MUST
include golden fixtures for the following cases. Each fixture is a
deterministic JSON envelope (or refusal envelope) that exercises
exactly one closed-enum value and asserts the harness emits the
value verbatim, classifies the cycle correctly, and emits the
expected per-assertion entries.

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
  subprocess invocation, classified as a STOP cycle, with the
  refusal envelope emitted verbatim

**Unknown-enum synthetic fixtures**:

- one fixture per closed enum (`phase`, `stop_condition`,
  `next_operator_command`, marker) carrying an unknown value. Each
  fixture MUST cause the harness to classify the cycle as a STOP
  in its own right and emit the unknown literal verbatim.

**Idempotency-pair fixtures**:

- one pair of byte-identical envelope captures, asserted equal by
  the harness's idempotency assertion (PASS).
- one pair of non-byte-identical envelope captures (e.g. one byte
  differs in a SHA field), asserted by the harness's idempotency
  assertion as a STOP signal in its own right (FAIL, classified
  STOP). The harness MUST emit both captures at full fidelity.

**Caller-expectation matrix**:

- `expected continue` × `observed continue` → PASS
- `expected continue` × `observed STOP` → FAIL classified STOP
- `expected STOP` × `observed continue` → FAIL classified
  `unexpected continue` (the harness reports the mismatch; it does
  NOT silently downgrade)
- `expected STOP` × `observed STOP` → PASS only if the observed
  `stop_condition` matches the expected value; otherwise FAIL
  classified `wrong-STOP`

The implementation lane MAY add additional fixtures beyond this
minimum. It MUST NOT ship with fewer.

## 13. Unknown Enum / Schema Drift Handling

The harness adapter implementation MUST handle the following
schema-drift cases at parse time. Each case is itself a STOP signal
the harness emits.

- **`schema_version` literal not `a2-l2d-status.v1`.** The harness
  refuses the envelope at parse time, emits the observed literal
  verbatim, classifies the cycle as a STOP, and exits the cycle.
  No best-effort parse is attempted.
- **Unknown `phase` value.** The harness emits the unknown literal
  verbatim, classifies the cycle as a STOP, and does not coerce to
  `unknown`. (The closed enum already includes `unknown` as a
  legitimate value; that is a known-good observation. An *unknown
  unknown* — an enum value not in the closed list at all — is the
  schema-drift case this rule addresses.)
- **Unknown `stop_condition` value.** Verbatim emission, STOP
  classification, no coercion to `null` ("unknown ok" coercion is
  explicitly forbidden).
- **Unknown `next_operator_command` shape.** Verbatim emission,
  STOP classification. A `next_operator_command` not matching any
  of the three closed shapes in
  [`a2-l2d-status-schema.md` §5](./a2-l2d-status-schema.md#5-closed-next_operator_command-shapes)
  is a STOP signal.
- **Unknown `audit_markers` member.** The harness emits the unknown
  marker verbatim, classifies the cycle as a STOP, and does not
  drop the unknown marker. (Marker drift is the most likely real-
  world drift mode; the harness must surface it loudly.)
- **Missing required field.** Any required field absent from the
  envelope is a parse-time failure. The harness reports the missing
  field name verbatim and classifies the cycle as a STOP.
- **`read_only_invariant` absent or altered.** The harness
  classifies absence or substitution as a STOP signal in its own
  right (per the harness adapter scope card §11 and §15).
- **JSON parse error.** The harness emits the raw stdout capture,
  the parse-error diagnostic, and classifies the cycle as a STOP.
  It does NOT retry the parse, fall back to a sloppier parser, or
  attempt envelope-shape inference.

The harness MUST NOT use any heuristic that "fixes" a drifted
envelope. The producer is authoritative; the harness reports drift
and stops.

## 14. Idempotency And Repeatability Tests

The harness adapter scope card §12 pinned idempotency requirements
in behavioral terms. This card pins the implementation tests that
prove those requirements hold.

The crate's integration tests MUST include:

- **Byte-identical pair PASS test.** Two `claw plan status` mock
  invocations against an unchanged disposable tempdir produce byte-
  identical stdout. The harness's idempotency assertion returns
  PASS.
- **Non-byte-identical pair FAIL test.** Mock the second
  invocation to return a single-byte-different envelope. The
  harness's idempotency assertion returns a STOP classification.
- **Independent-subprocess invariant test.** A test that asserts
  the harness invokes the subprocess twice (not once with cached
  stdout reuse). Implementation lane chooses the mechanism (process-
  count counter in the mock, or two distinct mock-subprocess
  invocations); the assertion is the same.
- **Per-cycle in-memory cache lifetime test.** A test that asserts
  a parsed envelope cached during one cycle is not visible to a
  subsequent cycle. The cache lifetime MUST be bounded to a single
  cycle's scope.
- **No-on-disk-cache test.** A test that asserts the harness emits
  no file to disk under any tempdir other than its own configured
  report destination. Implementation lane pins the exact tempdir
  the test snapshots before and after the cycle.

These tests are part of the CI matrix (§17). They run only against
disposable tempdirs (§11 disposable-workspace classification
applies in tests just as in production use).

## 15. No-Network / No-Broker / No-Model Validation

The harness adapter scope card §8 forbids broker, model, Ollama,
telemetry, analytics, error-reporting, and any HTTP traffic. The
implementation lane MUST prove this property by:

- **Dependency audit.** The crate's `Cargo.toml` MUST NOT depend on
  `reqwest`, `hyper`, `ureq`, `surf`, `isahc`, `awc`, or any other
  HTTP client crate. The implementation scope card MUST list the
  exact dependency set and the review MUST confirm no networking
  crate is in the transitive closure beyond what `Cargo.lock`
  already pins for the workspace.
- **Network-sentinel test.** A Cargo integration test that sets
  `HTTP_PROXY`, `HTTPS_PROXY`, and `OLLAMA_HOST` to unreachable
  sentinels (mirroring the A2-L2d invariants in
  [`a2-l2d-status-schema.md` §11](./a2-l2d-status-schema.md#11-read-only-invariants))
  and runs a full harness cycle. The test asserts the cycle
  completes successfully against a mock `claw plan status`
  subprocess and the sentinels are never resolved.
- **Subprocess-bounded test.** A test that asserts the harness
  spawns *only* the `claw plan status` subprocess with at most the
  two A2-L2d positional arguments, no flags. The implementation
  lane chooses the mechanism (process-spawn audit log, mock-
  subprocess wrapper); the assertion is the same.
- **Static-grep guard.** The implementation lane's CI step MUST
  run a grep over the crate source for `reqwest|hyper|ureq|surf|
  isahc|awc|http://|https://|ollama_host|broker_url|telemetry_url`
  and refuse the lane if any match appears in source code. The
  implementation scope card MAY adjust the exact regex; the
  expectation that the guard exists does not change.

The harness adapter MUST NOT depend on `tokio` or any async runtime
unless the implementation lane justifies the addition in writing.
The default expectation is **synchronous I/O only**: spawn
subprocess, read stdout, parse, return. Async is opt-in for the
implementation lane to justify against the no-network invariant.

## 16. No-Write / No-Approve / No-Apply Validation

The harness adapter scope card §§5–8 forbid every chain-write
operation. The implementation lane MUST prove this property by:

- **Filesystem-write sentinel test.** A Cargo integration test
  that snapshots the disposable tempdir's full content tree before
  and after a harness cycle, asserts byte-identical equality, and
  fails the lane if any byte differs. Implementation lane pins the
  exact snapshot mechanism.
- **`claw plan run|approve|apply-bundle|apply` refusal test.** A
  test that supplies an input deliberately constructed to direct
  the harness to invoke each of those commands and asserts the
  harness refuses the input at config-parse time.
- **Subprocess argv audit test.** A test that captures the argv of
  every subprocess the harness spawns during a cycle and asserts
  the only program name observed is the configured
  `claw plan status` binary path. Implementation lane pins the
  mock-subprocess wrapper.
- **Static-grep guard.** The implementation lane's CI step MUST
  run a grep over the crate source for `claw plan run|claw plan
  approve|claw plan apply-bundle|claw plan apply|approval-result\.
  json|apply-bundle\.json|fs::write|fs::create_dir|fs::remove|fs::
  rename|fs::set_permissions|File::create|OpenOptions::write|
  OpenOptions::append|OpenOptions::create` and refuse the lane if
  any match appears in non-test, non-comment source code. The
  implementation scope card MAY adjust the exact regex; the
  expectation that the guard exists does not change.
- **No-`.claw/`-write test.** A test that runs a full cycle against
  a workspace whose `.claw/**` tree is snapshotted before and after
  and asserts byte-identical equality of the `.claw/**` tree.

The harness adapter MUST NOT write under `.claw/`, the workspace
tree, the operator's home directory, or anywhere outside its own
configured report destination. The configured report destination
defaults to stdout; an opt-in file destination is acceptable if the
implementation lane justifies it and the destination is under the
disposable tempdir or under a caller-supplied report path that is
NOT inside `.claw/**`.

## 17. CI Validation Matrix

The harness adapter implementation lane MUST pass the following CI
matrix before merge. The matrix is enforced by existing workspace
CI (cargo clippy, cargo fmt, cargo test, docs source-of-truth, shell
tests) plus new in-crate tests and grep guards.

| Check | Mechanism | Mandatory |
|-------|-----------|-----------|
| `cargo fmt` workspace-clean | existing workflow | yes |
| `cargo clippy --workspace -- -D warnings` clean | existing workflow | yes |
| `cargo test --workspace` includes new crate tests | existing workflow auto-discovers new member | yes |
| docs source-of-truth | existing workflow | yes |
| shell tests | existing workflow | yes |
| STOP golden matrix (§12) | new in-crate tests | yes |
| Idempotency tests (§14) | new in-crate tests | yes |
| Network-sentinel test (§15) | new in-crate test | yes |
| Dependency audit (§15) | implementation scope card review + new in-crate test asserting `cargo metadata` excludes HTTP clients | yes |
| Subprocess-bounded test (§15) | new in-crate test | yes |
| Filesystem-write sentinel test (§16) | new in-crate test | yes |
| `claw plan run|approve|apply-bundle|apply` refusal test (§16) | new in-crate test | yes |
| `.claw/**` no-write test (§16) | new in-crate test | yes |
| Static-grep no-network guard (§15) | new in-crate test or CI step | yes |
| Static-grep no-write guard (§16) | new in-crate test or CI step | yes |
| Disposable-workspace classifier test (§11) | new in-crate test | yes |
| Schema-drift / unknown-enum tests (§13) | new in-crate tests | yes |
| Caller-expectation matrix (§12) | new in-crate tests | yes |

Each check is a hard gate. Skipping any check in the implementation
lane is a STOP gate.

The implementation lane MUST NOT introduce a new CI workflow file
unless the existing workflows cannot exercise a required check; if
a workflow change is needed, that is a separate scope-card lane
against `.github/workflows/**`, not a side-effect of the harness
implementation lane.

## 18. Security / Secrets Boundary

The harness adapter MUST NOT read, log, persist, or relay any of:

- environment variables other than the network-sentinel variables
  the harness explicitly sets for its own subprocess invocation
  (`HTTP_PROXY`, `HTTPS_PROXY`, `OLLAMA_HOST`)
- the operator's shell history
- the operator's terminal state
- the operator's home directory
- any secret material from `.claw/**` (none should exist there; the
  harness still MUST NOT emit such material if it does)
- the operator's git config or credentials
- the operator's SSH keys, GPG keys, or other key material
- broker, model, or Ollama API keys or tokens
- caller-supplied secrets the caller passes in error (the harness
  redacts any field marked `secret` in its assertion config and
  MUST log a STOP signal naming the redaction-occurred event)

The implementation lane MUST ensure the harness's report output is
deterministic, environment-independent (modulo the workspace and
the envelope), and free of any caller-secret material. A test in
the matrix MUST assert that running the harness with no environment
variables set produces a report identical (modulo workspace path)
to running it with the operator's full environment set.

## 19. Non-Goals

The harness adapter implementation must not:

- implement an IDE adapter
- implement an IDE adapter per-adapter scope card
- introduce a CLI subcommand on `claw plan` for harness operations
- introduce a new CLI binary alongside `claw` (the optional in-
  crate `[[bin]]` target, if added, is a separate program, not a
  `claw plan harness …` subcommand)
- introduce or imply autonomous workspace-write execution
- introduce harness controls that approve, that apply, that apply-
  bundle, or that compose any combination of those
- introduce harness-driven retry, remediation, or rollback of any
  chain step
- introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  pre-approval, batch approval, or any approval-bypass affordance
- introduce a "fast", "shadow", "what-if", or "dry-run" mode that
  simulates downstream chain commands
- modify `claw plan run`, `claw plan approve`, `claw plan apply-
  bundle`, `claw plan apply`, or `claw plan status` behavior, exit
  codes, schemas, markers, or JSON field shapes
- modify `a2-l2b-*` or `a2-l2d-status.v1` schema versions or marker
  constants
- introduce an `a2-l3-*` schema, marker, exit code, or CLI surface
- call broker, model, or Ollama at any phase
- introduce filesystem watchers, daemon channels, or background
  refresh
- introduce on-disk caches of envelope contents as authoritative
  state
- introduce cross-run inventory, cross-workspace dashboards, or
  history rollups
- introduce a harness assertion library that "remediates" STOP
  signals
- weaken any A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter boundary, or
  A2-L3 harness adapter STOP gate
- run against `/home/suki/stack-code`, `/home/suki/sidestackai`, or
  any production repository under any circumstance
- depend on the stale sibling worktree at
  `/mnt/vast-data/git-worktrees/stack-code-a2-l3-harness-adapter-
  scope-card-20260530_114057` or any other unmerged work tree

Any of the above must be opened as a separate, explicitly-
authorized lane.

## 20. Future Implementation STOP Gates

The implementation lane is a STOP gate failure if any of the
following hold at PR review time:

- a file outside the §7 allowed enumeration is touched
- any §8 forbidden surface is touched
- any §12 STOP golden matrix case is absent or coverage is
  partial
- any §14 idempotency test is absent
- any §15 no-network validation step is absent
- any §16 no-write/no-approve/no-apply validation step is absent
- any §17 CI matrix check is skipped, disabled, or guarded behind
  an env var
- any §18 security boundary is weakened
- the harness invokes any subprocess other than `claw plan status`
- the harness writes any file outside its configured report
  destination
- the harness depends on an HTTP client crate
- the harness depends on `tokio` or another async runtime without
  written justification against the no-network invariant
- the disposable-workspace classifier defaults to `disposable` for
  any signal absence
- the disposable-workspace classifier accepts caller declaration
  alone (without the other AND-signals)
- the harness operates against `/home/suki/stack-code`,
  `/home/suki/sidestackai`, or any production repository in any
  test
- the harness emits a report that reframes its role as a workflow
  controller (e.g. headers like "Chain Manager", "Apply
  Coordinator", "Approval Helper")
- the implementation scope card omits the touched-surface
  enumeration the lane will commit to
- the harness implementation references the stale sibling worktree
  at `/mnt/vast-data/git-worktrees/stack-code-a2-l3-harness-
  adapter-scope-card-20260530_114057`

Hitting any of these gates blocks the implementation lane. The
review path is to refuse the lane and to open a separate scope-card
lane addressing the underlying issue.

## 21. Definition Of Done

This **implementation scope card** is done when:

- `docs/a2-l3-harness-adapter-implementation-scope-card.md` exists
  and matches the sectional structure of this card.
- The card defines the recommended implementation shape.
- The card pins the allowed and forbidden future touched surfaces.
- The card pins the input contract, output/reporting contract, and
  disposable-workspace classifier design.
- The card pins the STOP rendering golden-test matrix, the unknown-
  enum drift handling, the idempotency tests, and the no-network /
  no-broker / no-model and no-write / no-approve / no-apply
  validation steps.
- The card pins the CI validation matrix and the security/secrets
  boundary.
- The card declares the harness adapter implementation as docs-only
  at this scope-card stage.
- No Rust source, no Cargo manifest, no test, no wrapper, no
  workflow, no script, no runtime config is touched.
- No A2-L2b, A2-L2c, A2-L2d, A2-L3 boundary, or A2-L3 harness STOP
  gate is weakened.
- The stale sibling worktree at
  `/mnt/vast-data/git-worktrees/stack-code-a2-l3-harness-adapter-
  scope-card-20260530_114057` is not inspected, modified, or
  removed.
- A single cross-link line MAY be added to the A2-L3 harness
  adapter scope card, the A2-L3 adapter boundary scope card, the
  A2-L2d scope card, the A2-L2d status schema, or the A2-L2d
  operator quick reference if an obvious location exists, but no
  such cross-link is required for this scope card itself to land.
  *(This scope card is authored without cross-links to keep the
  lane strictly limited to a single new docs file; cross-links may
  be added in a follow-up lane.)*
- The card is reviewed by the operator before any harness adapter
  implementation lane is opened.

The harness adapter **implementation lane** is out of scope for
this card. Definition of done for that lane will be authored by the
implementation lane itself, bounded by §§6–20 above.

## 22. Next Lane Recommendation

The recommended next lane after this scope card is reviewed is:

> **Harness adapter implementation lane (code-bearing)** — open a
> single PR that creates `rust/crates/a2-harness-adapter/` with the
> library API, the integration test suite covering every check in
> the §17 CI matrix, the disposable-workspace classifier per §11,
> the STOP rendering golden fixtures per §12, the idempotency tests
> per §14, the no-network / no-broker / no-model validation steps
> per §15, and the no-write / no-approve / no-apply validation
> steps per §16. The PR's diff is bounded strictly by §§7–8 of this
> card.

The implementation lane MUST open with a top-of-PR comment that
enumerates the exact subset of §7 it touches; touching anything
beyond that enumeration is a STOP gate.

Lanes that follow the implementation lane, in order:

> **Harness adapter usage documentation lane (docs-only)** —
> author `docs/a2-l3-harness-adapter-usage.md` and cross-link from
> the README. May fold into the implementation lane if the
> implementation lane's reviewer accepts the docs additions
> alongside the crate.

> **IDE adapter per-adapter scope card lane (docs-only)** — author
> the IDE-side equivalent of the harness adapter scope card,
> bounded by the A2-L3 adapter boundary card. Separate from the
> harness adapter pipeline.

None of these lanes permit autonomous workspace-write execution.
All remain bounded by the A2-L2b, A2-L2c, A2-L2d, A2-L3 adapter
boundary, and A2-L3 harness adapter safety properties.

## 23. References

- [`a2-l3-harness-adapter-scope-card.md`](./a2-l3-harness-adapter-scope-card.md)
  — A2-L3 Harness Adapter Scope Card; the parent card this
  per-implementation card refines into concrete touched-surface and
  validation constraints.
- [`a2-l3-adapter-boundary-scope-card.md`](./a2-l3-adapter-boundary-scope-card.md)
  — A2-L3 Adapter Boundary Scope Card; the cross-adapter
  constraints that any per-adapter implementation must hold to.
- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record; authoritative on the
  contract the harness consumes.
- [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md) —
  A2-L2d operator quick reference for `claw plan status`.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — A2-L2d scope card; section 10 ("IDE / Harness Boundary") is
  the upstream preamble that A2-L3 expanded.
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference; TTY approval EOF note in §3 is
  load-bearing for the approval boundary the harness must never
  compose around.
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- PR #34 (`1d0500e`) — A2-L2b `run_plan --workspace-write-preview`.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the
  handoff.
- PR #37 (`9cedbb0`) — A2-L2c scope card.
- PR #38 (`17967e6`) — A2-L2c operator quick reference.
- PR #39 (`12fff14`) — A2-L2d scope card.
- PR #40 (`0f75800`) — A2-L2d read-only `claw plan status` command
  + `a2-l2d-status.v1`.
- PR #41 (`4c2b15e`) — A2-L2d operator quick reference.
- PR #42 (`21d9b5b`) — A2-L3 adapter boundary scope card.
- PR #44 (`f63d5ac`) — A2-L3 harness adapter scope card.

## 24. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Broker / model / Ollama touched: **no**.
- Harness adapter implementation: **not started; not authorized by
  this card**.
- IDE adapter implementation: **not started; not authorized by
  this card**.
- IDE adapter scope card authored: **no** (separate future per-
  adapter lane).
- Autonomous-write authorization: **none granted**.
- Approval / apply boundary weakened: **no**.
- A2-L2b / A2-L2c / A2-L2d / A2-L3-boundary / A2-L3-harness STOP
  gate weakened: **no**.
- Status-contract (`a2-l2d-status.v1`) modified: **no**.
- A2-L3 adapter boundary card or A2-L3 harness adapter scope card
  modified: **no**.
- Stale sibling worktree
  (`stack-code-a2-l3-harness-adapter-scope-card-20260530_114057`)
  touched: **no**.
- Next gate before implementation: operator review of this scope
  card, followed by the harness adapter implementation lane bounded
  by §§6–20 above.
