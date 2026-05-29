# A2-L2c Operator Quick Reference

This document is the at-the-keyboard companion to
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md).
It re-presents the runtime-proven A2-L2b chain as a copy-pasteable
operator path, an exit-code disambiguation matrix for the overloaded
`7`, a TTY approval EOF note, and a per-step artifact map.

It is **documentation only**. It introduces no new CLI command,
subcommand, flag, exit code, marker, schema, or JSON field. It does
not change any runtime behavior and it does not weaken any A2-L2b
STOP gate. Section 5 ("What A2-L2c Is Not") restates the explicit
prohibitions in non-softening form.

For the canonical proof record, STOP gates, and authority-chain
derivation, the handoff remains authoritative; this quick reference
defers to it on any text or behavior question.

## 1. At-the-Keyboard Operator Path

The proven chain from
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md#4-operator-command-flow),
section 4, verbatim:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

Each command is invoked separately by the operator. There is no
single-invocation orchestration of this chain, and there is no
in-process composition of `approve` and `apply`; the TTY-enforced
approval boundary and the explicit-apply boundary that the chain's
safety derives from would not survive such composition.

Step semantics (mirrored from the handoff):

- `claw plan run … --workspace-write-preview` halts the runner
  immediately after the preview artifacts are written. Exit `7`
  here is the **preview-ready halt** (see section 2), not a failure
  or a denial.
- `claw plan approve <preview-bundle.json>` is TTY-enforced. It
  parses a single approval line (see section 3) and emits an
  approval result bound to the preview's `step_id` and
  `preview_sha256`.
- `claw plan apply-bundle …` is offline. It cross-validates the
  approval result against the preview-generator result and emits a
  structured apply bundle.
- `claw plan apply <apply-bundle.json>` runs preflight, writes to a
  temp file, performs an atomic replace, and validates the
  post-write hash. On any mid-write failure, the runtime attempts
  rollback to the checkpoint baseline.

## 2. Exit-Code Disambiguation

The handoff's exit-code table is the source of truth; this section
adds the disambiguation matrix for code `7`. The quick reference
introduces no new code and contains none not already pinned in
`rust/crates/a2-plan-runner/src/{approval,checkpoint,report,runner,
write_executor}.rs` and `rust/crates/a2-plan-runner/src/write_runtime.rs`.

| Code | Source constant                                         | Emitted by                                                       | Meaning                                                                                  |
|------|---------------------------------------------------------|------------------------------------------------------------------|------------------------------------------------------------------------------------------|
| `0`  | `write_executor::EXIT_WRITE_APPLIED`                    | `claw plan apply`                                                | Apply succeeded; post-write validation passed.                                           |
| `5`  | `report::EXIT_PARSE_ERROR`                              | `claw plan approve`, `apply-bundle`, `apply`                     | Input parse / bundle / generator rejection (closed refusal).                             |
| `6`  | `write_runtime::EXIT_WRITE_PATH_REFUSED`                | `claw plan apply`                                                | L2b write-target path safety refused the request.                                        |
| `7`  | `runner::EXIT_RUN_PLAN_WRITE_PREVIEW_READY`             | `claw plan run --workspace-write-preview`                        | **Preview-ready halt** — preview artifacts written; runner stopped before any approval.  |
| `7`  | `approval::EXIT_APPROVAL_DENIED`                        | `claw plan approve`                                              | **Approval refused** — operator did not produce a valid approval line, or preview is non-approvable. |
| `8`  | `write_executor::EXIT_ROLLBACK_FAILED`                  | `claw plan apply`                                                | Rollback could not restore the baseline; workspace is in an uncertain state.             |
| `9`  | `checkpoint::EXIT_CHECKPOINT_FAILED`                    | `claw plan run`                                                  | Checkpoint write failed; apply did not run.                                              |
| `9`  | `write_executor::EXIT_BASELINE_MISMATCH`                | `claw plan apply`                                                | Baseline drift detected at apply time; apply did not run.                                |
| `10` | `write_executor::EXIT_WRITE_IO_FAILED`                  | `claw plan apply`                                                | Write I/O failed before atomic replace; target unchanged.                                |
| `11` | `write_executor::EXIT_VALIDATION_ROLLED_BACK`           | `claw plan apply`                                                | Post-write validation failed and rollback succeeded.                                     |

### Disambiguating `7`

Exit `7` is overloaded between **preview-ready halt** (from
`claw plan run --workspace-write-preview`) and **approval refused**
(from `claw plan approve`). The structured JSON output on stdout
disambiguates without inspecting which command was just invoked:

| Command                                       | Disambiguating JSON field | Value indicating `7`                                                                 |
|-----------------------------------------------|----------------------------|--------------------------------------------------------------------------------------|
| `claw plan run --workspace-write-preview`     | `status`                   | `write_preview_ready`                                                                |
| `claw plan run --workspace-write-preview`     | `outcome`                  | `write_preview_ready`                                                                |
| `claw plan approve`                           | `decision`                 | anything other than `approved` (refusal reason carried in adjacent fields)           |

If `status` / `outcome` show `write_preview_ready`, the runner halted
intentionally and the operator should proceed to
`claw plan approve`. If `decision` is anything other than `approved`,
the chain stops; the approval result records the refusal reason and
no apply bundle should be generated from it. Any other exit `7`
shape is unenumerated — STOP and escalate, do not retry.

## 3. TTY Approval EOF Note

`claw plan approve <preview-bundle.json>` enforces a TTY check
before consuming stdin and accepts exactly one approval line in the
exact form rendered by the operator prompt
([`approval_ux.rs`](../rust/crates/a2-plan-runner/src/approval_ux.rs)):

```text
apply <step_id> <preview_sha256>
```

Both `<step_id>` and `<preview_sha256>` are surfaced verbatim in the
approval prompt header (`Step: ...`, `Preview SHA256: ...`) and must
be reproduced exactly.

Observed semantics:

- **TTY guard scope.** The non-TTY fail-closed guard only triggers
  for approvable previews. A non-approvable preview (binary,
  redacted, or truncated, as flagged in the preview record) never
  reads stdin: `claw plan approve` short-circuits to a non-approvable
  summary and never accepts an approval command for it. This means a
  non-TTY invocation against a non-approvable preview surfaces a
  non-approvable refusal, not the non-TTY refusal — the same exit
  code `7`, but a different reason.
- **Non-TTY refusal reason.** When the TTY guard does trigger, the
  refusal reason is the stable identifier `approval-stdin-not-tty`.
  Operators reading this in the approval result should treat it as
  a tooling/environment issue (the prompt was not delivered to a
  real terminal), not as an operator denial.
- **EOF pitfall.** The handoff notes that "an explicit EOF after
  [the approval] line may be required for the CLI to consume the
  input"
  ([`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md#4-operator-command-flow),
  section 4). In practice this presents as a hung-looking prompt
  immediately after the operator types the `apply <step_id>
  <preview_sha256>` line. The terminal driver is buffering the
  line until end-of-input is signalled. The operator workaround is
  to deliver an explicit end-of-input (e.g. `Ctrl-D` on a typical
  Unix line-buffered TTY) after the approval line; the CLI then
  consumes the buffered line and emits the approval result on
  stdout. There is no programmatic auto-EOF and the quick
  reference does not propose one — the behavior is a known
  operator hazard.

The quick reference adds no new approval format, no alternate
approval line, and no second-channel approval input. The approval
line above is the only operator input the CLI consumes for this
step.

## 4. Per-Step Artifact Map

Each preview run produces a self-contained artifact set rooted at
`<workspace>/.claw/` (see
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md#5-artifact-lifecycle),
section 5). This table connects each path to the command that
produces it and the command that consumes it.

| Artifact path                                                                                       | Produced by                                                | Consumed by                                                |
|-----------------------------------------------------------------------------------------------------|------------------------------------------------------------|------------------------------------------------------------|
| `.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json`                                  | `claw plan run --workspace-write-preview`                  | `claw plan approve`                                        |
| `.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-generator-result.json`                        | `claw plan run --workspace-write-preview`                  | `claw plan apply-bundle` (first argument)                  |
| `.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json`                                    | `claw plan apply-bundle`                                   | `claw plan apply`                                          |
| `.claw/l2b-payloads/<run-id>/<step-id>/after.bin`                                                   | `claw plan run --workspace-write-preview`                  | `claw plan apply` (payload SHA re-verified against preview)|
| `.claw/l2b-payloads/<run-id>/<step-id>/after.sha256`                                                | `claw plan run --workspace-write-preview`                  | `claw plan apply` (payload-SHA cross-check)                |
| `.claw/l2b-checkpoints/<run-id>/<step-id>/manifest.json`                                            | `claw plan run --workspace-write-preview`                  | `claw plan apply` (checkpoint baseline reference)          |
| `.claw/l2b-checkpoints/<run-id>/<step-id>/before.bin`                                               | `claw plan run --workspace-write-preview`                  | `claw plan apply` (rollback baseline)                      |
| `.claw/l2b-runs/<run-id>/run-manifest.json`                                                         | `claw plan run --workspace-write-preview`                  | operator inspection (not piped to a downstream CLI step)   |
| `.claw/l2b-runs/<run-id>/status.json`                                                               | `claw plan run --workspace-write-preview`                  | operator inspection (not piped to a downstream CLI step)   |
| approval result (operator-captured stdout of `claw plan approve`)                                   | `claw plan approve`                                        | `claw plan apply-bundle` (second argument)                 |

The approval result is captured by the operator from
`claw plan approve` stdout; the chain does not write it under
`.claw/` automatically. `apply-bundle.json` is created **only** by
`claw plan apply-bundle`; hand-constructed apply bundles are a STOP
condition in the handoff (section 8) and the quick reference does
not relax that.

## 5. What A2-L2c Is Not

A2-L2c is operator documentation. It does not authorize, enable, or
provide a path to any of the following — all of which remain
forbidden by the A2-L2b chain and by
[`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md), sections 3, 7, and
8:

- autonomous workspace-write execution is **not** authorized by this
  document
- `--yes`, `--auto`, `--skip-approval`, and `--no-prompt` are
  **not** authorized; no such flag exists on `claw plan approve`,
  `claw plan apply-bundle`, or `claw plan apply`, and this quick
  reference does not propose one
- preapproval and batch approval are **not** authorized; approval
  is per-preview and TTY-enforced
- apply without approval is **not** authorized; the apply chain
  refuses any apply bundle whose approval result does not bind to
  the preview record
- model-initiated writes are **not** authorized; no broker, model,
  or Ollama call participates in the chain at any phase
- model-generated `after` bytes are **not** authorized; the
  `after_file` is operator-supplied and SHA-bound at preview time
- automatic workspace-write composition (a single invocation that
  internally calls both `approve` and `apply`) is **not**
  authorized and would invalidate the TTY-enforced approval
  boundary
- weakening of any A2-L2b STOP gate (see the handoff, section 8) is
  **not** authorized

Any near-term lane that proposes any of the above must be opened as
a separate, explicitly-authorized lane and must clear its own
review; the A2-L2c quick reference is **not** prior authorization
for it.

## 6. References

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
- [`a2-l2c-scope-card.md`](./a2-l2c-scope-card.md) — A2-L2c scope
  card; defines this quick reference's allowed surfaces, forbidden
  language, and validation plan.
- [`a2-plan-schema.md`](./a2-plan-schema.md) — A2 plan YAML schema
  (the L1a/L2a offline validator surface).
