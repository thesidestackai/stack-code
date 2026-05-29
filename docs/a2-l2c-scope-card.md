# A2-L2c Scope Card — Operator Quick Reference (Docs-Only)

This document is a **design-only scope card** for the A2-L2c lane. It
describes what A2-L2c is, what it must not become, and the validation
required before any implementation lane is allowed to land. This file
itself authorizes **no runtime change, no CLI change, and no autonomous
workspace-write behavior**.

A2-L2c is the operator-facing documentation continuation explicitly
recommended by section 10 of
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md):

> The default recommendation is to pause before A2-L2c autonomous write
> integration and first land operator-facing documentation/README
> references that point at this handoff and the existing
> `docs/a2-plan-schema.md`. The proven chain is safe but the operator
> ergonomics around exit-code overloading on `7`, TTY approval EOF
> handling, and per-step artifact navigation are still implicit.

PRs #35 (`a207a91`) and #36 (`86dc37f`) closed the README and schema
cross-link layer. A2-L2c closes the operator ergonomics layer — still
docs-only.

## 1. Recommended A2-L2c Name

`A2-L2c Operator Quick Reference (docs-only)`

## 2. Problem

The A2-L2b handoff is comprehensive but reads as a proof record, not as
an at-the-keyboard operator path. Three implicit gaps remain:

1. **Exit code `7` is overloaded.** Both
   `EXIT_RUN_PLAN_WRITE_PREVIEW_READY` (runner: preview-ready halt) and
   `EXIT_APPROVAL_DENIED` (approval: refusal) return `7`. An operator
   reading only the exit code cannot disambiguate. The
   `status` / `outcome` / `decision` JSON fields disambiguate, but the
   handoff does not present a copy-pasteable disambiguation matrix.
2. **TTY approval EOF handling is implicit.** `claw plan approve`
   consumes the line `apply <step_id> <preview_sha256>`, but the
   handoff notes only that "an explicit EOF after that line may be
   required for the CLI to consume the input." Operators on different
   terminals hit this differently and the failure mode looks identical
   to a hung approval prompt.
3. **Per-step artifact navigation is implicit.** Section 5 lists the
   `.claw/...` paths but does not connect each path to the operator
   command that produces or consumes it. An operator midway through
   the chain has to read section 5 and section 4 together to know
   which artifact to hand which command.

## 3. Non-Goals

A2-L2c must not:

- edit Rust source under `rust/crates/**`
- edit `Cargo.toml` or `Cargo.lock`
- edit wrappers, `bin/`, or shell launchers
- add, rename, or change any CLI command, subcommand, or flag
- change `run_plan`, `claw plan approve`, `claw plan apply-bundle`,
  or `claw plan apply` behavior
- introduce new exit codes or change existing exit-code semantics
- alter `a2-l2b-*` markers, schema versions, or JSON field shapes
- call broker, model, or Ollama at any phase
- run live smokes against a real workspace
- mutate runtime state on `/home/suki/stack-code`,
  `/home/suki/sidestackai`, or any non-disposable repo
- weaken any A2-L2b STOP gate
- authorize autonomous writes, `--yes`, `--auto`, preapproval, batch
  approval, or model-initiated writes (these remain forbidden by
  A2-L2b and are out of scope for A2-L2c)

## 4. Allowed Future Touched Surfaces

When A2-L2c is later executed as an implementation lane, the only
surfaces it may touch are:

- `docs/a2-l2c-operator-quickref.md` (new) — the operator quick
  reference itself.
- `README.md` — one cross-link line in the existing "Documentation
  map" section.
- `docs/a2-l2b-run-plan-preview-operator-handoff.md` — one cross-link
  line in section 10 ("Recommended Next Lanes") pointing at the new
  quick reference.

No other file in the repository may be modified by the A2-L2c
implementation lane.

## 5. Forbidden Surfaces

The A2-L2c implementation lane is forbidden from touching:

- `rust/crates/**`
- `rust/Cargo.toml`, `rust/Cargo.lock`
- `examples/`
- `tests/`, `**/tests/**`
- `.github/workflows/**`
- `.github/scripts/**` (the doc source-of-truth script is read-only
  input, not a target)
- shell wrappers, launchers, or `bin/`
- any schema, marker, or JSON field definition

If A2-L2c implementation touches any of the above, the lane has
exceeded its scope and must be reopened as a separate, explicitly
authorized lane.

## 6. Operator Flow (Unchanged)

A2-L2c does not change the operator flow. The canonical sequence
documented in
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
section 4 remains authoritative:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

The quick reference re-presents this flow as a copy-pasteable
operator path next to a per-step artifact map and an exit-code
disambiguation matrix. It does not introduce any other path.

## 7. Safety Invariants

The A2-L2c implementation lane must preserve, verbatim:

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

The quick reference must explicitly echo each of these as boundaries,
not soften them with convenience language.

## 8. Forbidden Language

The A2-L2c implementation lane's quick reference is forbidden from
containing any of the following, except when explicitly framing them
as out-of-scope or refused:

- "autonomous write"
- "autonomous apply"
- `--yes`, `--auto`, `--skip-approval`, `--no-prompt`
- "preapproval", "batch approval"
- "apply without approval"
- "model-initiated write"
- "automatic workspace-write"

Any mention of these terms in the quick reference must read as a
prohibition, not as a feature.

## 9. Validation Plan (for the future A2-L2c implementation lane)

When the A2-L2c implementation lane runs, it must verify all of:

- `git diff --name-only` lists **only** the three allowed surfaces
  from section 4. Any other path fails the lane.
- `git diff --check` is clean.
- `python3 .github/scripts/check_doc_source_of_truth.py` passes
  (the script walks `docs/**/*.md` so the new file is linted
  automatically).
- Forbidden-language sniff against the staged diff finds no
  language authorizing autonomous writes (use the
  `Phase 6` regex from the scope-card validation phase as the
  contract).
- Exit-code table in the quick reference must match the constants
  pinned in `rust/crates/a2-plan-runner/src/{approval,checkpoint,
  report,runner,write_executor}.rs`. The quick reference may not
  document an exit code that has no constant in source.
- README "Documentation map" cross-link must point at the new file
  and use the same line style as the existing
  `a2-l2b-run-plan-preview-operator-handoff.md` entry.
- Handoff section 10 must gain exactly one cross-link line. No other
  edit to the handoff is permitted.
- The quick reference must not run any command, smoke, or live test
  as part of its authoring.

## 10. STOP Gates

The A2-L2c implementation lane must STOP and refuse to land if any
of the following are true:

- A non-docs file is staged.
- The quick reference introduces a new CLI command, flag, exit code,
  marker, schema, or JSON field.
- The quick reference suggests or authorizes an autonomous write,
  `--yes`, preapproval, or any A2-L2b STOP-gate weakening.
- The quick reference's exit-code table drifts from the source
  constants in `rust/crates/a2-plan-runner/src/*.rs`.
- The quick reference repeats a STOP gate from the handoff in a
  weaker form.
- The lane requires touching `rust/crates/**` to "make the docs
  align" — the docs must align to source, not the other way.
- The lane requires running a live smoke, broker call, or model
  call as part of authoring.
- The handoff doc gains any edit beyond the single cross-link line.
- `origin/main` advances during the lane in a way that invalidates
  the exit-code table or the handoff text the quick reference
  references — in that case the lane must re-base and re-verify.

## 11. Definition of Done

The A2-L2c implementation lane is done when **all** of the
following hold:

- `docs/a2-l2c-operator-quickref.md` exists on `origin/main` and
  contains:
  - the canonical 4-command operator path verbatim
  - an exit-code disambiguation table covering at minimum the
    overloaded `7` (preview-ready vs approval-denied) with the
    JSON fields that disambiguate
  - a TTY approval EOF note describing the observed behavior and
    the `apply <step_id> <preview_sha256>` line semantics
  - a per-step artifact map connecting each `.claw/...` path to
    the command that produces or consumes it
  - an explicit "What A2-L2c is not" section restating that A2-L2c
    does not authorize autonomous writes, `--yes`, or preapproval
- `README.md` "Documentation map" has exactly one new cross-link
  line pointing at the quick reference.
- `docs/a2-l2b-run-plan-preview-operator-handoff.md` section 10 has
  exactly one new cross-link line pointing at the quick reference.
- All validation-plan checks (section 9) pass.
- No file outside the three allowed surfaces is modified.
- No Rust source, schema, or test is touched.
- No live runtime is touched.
- No broker, model, or Ollama traffic is generated by the lane.

## 12. Why This Scope and Not a Larger One

Three larger candidate scopes were considered and rejected for A2-L2c:

- **`A2-L2c Apply Artifact Inspection / Status UX` (CLI lane).**
  Would require a new `claw plan status` or `claw plan inspect`
  subcommand. Out of scope because it touches `rust/crates/**`
  before the operator-ergonomics docs gap is closed. Revisit only
  after the quick reference is live and operator pain is still
  evident.
- **`A2-L2c Orchestration Convenience Wrapper`.**
  Would package the 4-command chain into a single operator
  invocation. Rejected: any convenience wrapper that calls
  `claw plan approve` and `claw plan apply` in the same process
  weakens the TTY-enforced approval boundary and the explicit-apply
  boundary that A2-L2b's safety derives from.
- **`A2-L2c Autonomous Workspace-Write Integration`.**
  Explicitly forbidden by the handoff. The L2b safety property is
  "operator-gated at every transition" and autonomous writes
  invalidate it. Out of scope for A2-L2c and for any near-term lane.

A2-L2c is intentionally the smallest reversible step that closes the
three named operator gaps. It is testable by inspection, requires no
runtime, and does not loosen any A2-L2b gate.

## 13. References

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md) — runtime-proven A2-L2b operator chain; section 10 names the gaps this scope card targets.
- [`a2-plan-schema.md`](./a2-plan-schema.md) — A2 plan YAML schema; the L1a/L2a offline validator surface.
- PR #35 (`a207a91`) — A2-L2b handoff doc.
- PR #36 (`86dc37f`) — README and schema cross-links to the handoff.

## 14. Status

- Mode: **design-only**.
- Implementation: **not started**.
- Runtime touched: **no**.
- Autonomous-write authorization: **no**.
- Next gate before implementation: operator review of this scope card.
