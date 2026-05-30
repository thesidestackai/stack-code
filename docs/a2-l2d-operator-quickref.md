# A2-L2d Operator Quick Reference

This document is the at-the-keyboard companion to
[`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) and the
[`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md).
It re-presents the shipped `claw plan status` command as a
copy-pasteable operator path for inspecting where a workspace is in
the A2-L2b preview-to-apply chain.

It is **documentation only**. It introduces no new CLI command,
subcommand, flag, exit code, marker, schema, or JSON field. It does
not change any runtime behavior and it does not weaken any A2-L2b,
A2-L2c, or A2-L2d STOP gate. Section 9 ("What A2-L2d Status Does Not
Authorize") restates the explicit prohibitions in non-softening form.

For the canonical contract (envelope shape, closed enums, exit codes,
refusal envelope, idempotency and read-only invariants), the schema
doc remains authoritative; this quick reference defers to it on any
text or behavior question.

## 1. Purpose

`claw plan status` is a **read-only inspection command**. It answers
"where am I in the A2-L2b chain and what may I run next?" by
aggregating state from existing `<workspace>/.claw/l2b-*` artifacts
and emitting a stable `a2-l2d-status.v1` JSON envelope on stdout.

It does not approve, apply, retry, roll back, clean, or mutate
anything. Two successive invocations against an unchanged workspace
produce byte-identical stdout. The command makes no network egress
and spawns no subprocess. See
[`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) sections 10
and 11 for the idempotency and read-only invariants in full.

## 2. Command

```text
claw plan status <workspace> [<approval-result.json>]
```

* `<workspace>` — required; absolute or relative path to the
  workspace root.
* `<approval-result.json>` — optional positional; the only permitted
  read outside `<workspace>/.claw/**`. When supplied, the file is
  parsed read-only, included in `evidence_paths`, and never modified.

No flags. Every write-adjacent flag (`--apply`, `--approve`, `--yes`,
`--auto`, `--clean`, `--rollback`, `--mutate`, `--all-runs`,
`--no-prompt`, `--skip-approval`, `--cache`) is refused outright.

## 3. When To Use It

Run `claw plan status` whenever you need a read-only view of chain
state:

- **After preview generation** to confirm the preview exists, the
  payload SHA sidecar matches, and the chain is `awaiting_approval`.
- **Before approval** to verify the preview record is approvable
  (not binary, not redacted, not truncated) and to read the exact
  `step_id` and `preview_sha256` you will need on the approval line.
- **After approval is captured** (by passing the captured
  `approval-result.json` as the optional second positional) to
  confirm phase has advanced to `approval_captured`.
- **Before generating the apply bundle** to confirm the approval
  result's `step_id` and `preview_sha256` still bind to the preview
  record.
- **Before apply** to confirm phase is `apply_bundle_ready`, the
  payload SHA sidecar matches `preview_record.after_sha256`, the
  live target is still at its pre-write baseline, and no
  `stop_condition` is set.
- **When returning to a workspace mid-chain** (new terminal session,
  different operator, post-context-switch) to recover state in a
  single read without manually parsing `.claw/l2b-runs/`,
  `.claw/l2b-preview-bundles/`, `.claw/l2b-checkpoints/`, and
  `.claw/l2b-payloads/`.

## 4. Key Output Fields

The full envelope contract is pinned in
[`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) section 3.
The fields you read most at the keyboard are:

| Field                   | What to read it for                                                                                              |
|-------------------------|------------------------------------------------------------------------------------------------------------------|
| `phase`                 | Which of the 9 chain states the workspace is in (see section 5).                                                 |
| `next_operator_command` | The literal canonical-chain command to run next, or `STOP — escalate`, or `(no run found — start with claw plan run …)`. |
| `is_approvable`         | False whenever the preview record is binary, redacted, or truncated, or when no preview exists.                  |
| `is_apply_ready`        | True only when `apply-bundle.json` exists, validates against `a2-l2b-apply-bundle.v1`, the payload SHA sidecar matches, AND `stop_condition` is null. |
| `stop_condition`        | One of the 11 closed values from schema doc section 6, or null. When non-null, `next_operator_command` is `STOP — escalate`. |
| `evidence_paths`        | Every artifact path that was read to produce this envelope; sorted and deduplicated. Inspect these directly when a STOP fires. |
| `read_only_invariant`   | Always the literal `"this command does not mutate state"`. Present on every emission, success and refusal alike. |

Always also read `schema_version` (must be `a2-l2d-status.v1`),
`run_id`, `step_id`, and the SHA fields (`before_sha256`,
`after_sha256`, `payload_sha256`, `live_target_sha256`) when
diagnosing — they make most STOP causes self-evident.

## 5. Phase Meanings

The `phase` enum is closed at 9 values
([schema doc section 4](./a2-l2d-status-schema.md#4-closed-phase-enum)).
What each one means in operator terms:

| Phase                  | Meaning                                                                                                                              | Next operator action                                                                              | Continue / STOP |
|------------------------|--------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------|-----------------|
| `no_run_found`         | `<workspace>/.claw/l2b-runs/` is missing or holds no run with a `run-manifest.json`.                                                 | Start the chain with `claw plan run … --workspace-write-preview`.                                 | Continue        |
| `preview_ready`        | A preview exists; an `<approval-result.json>` was supplied to status but did not advance the chain (e.g. unparseable, schema-mismatched). Chain is back at preview time. | Re-capture a fresh approval result by running `claw plan approve <preview-bundle.json>`.          | Continue        |
| `awaiting_approval`    | Preview exists, is approvable, no `<approval-result.json>` was supplied to status, no `apply-bundle.json` exists yet.                | Run `claw plan approve <preview-bundle.json>`; the approval line is `apply <step_id> <preview_sha256>`. | Continue        |
| `approval_captured`    | The supplied `<approval-result.json>` validates against the preview record; no `apply-bundle.json` yet.                              | Run `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`.              | Continue        |
| `apply_bundle_ready`   | `apply-bundle.json` exists, validates against `a2-l2b-apply-bundle.v1`, payload SHA sidecar matches `preview_record.after_sha256`, live target still at the pre-write baseline. | Run `claw plan apply <apply-bundle.json>`.                                                        | Continue        |
| `applied`              | Live target file's SHA matches `preview_record.after_sha256`. The chain for this step is done.                                       | Move on to the next step or run, or close out the chain.                                          | Continue        |
| `rolled_back`          | Apply-bundle exists AND the operator supplied an approved `<approval-result.json>` AND the live target SHA matches `preview_record.before_sha256`. Filesystem-distinguishable rollback signal. | `STOP — escalate`. Inspect `evidence_paths` to determine why a rollback occurred before any retry. | STOP            |
| `non_approvable`       | `preview_record.is_binary`, `is_redacted`, or `is_truncated` is true. Operator cannot approve.                                       | `STOP — escalate`. Do not attempt to coerce an approval on a non-approvable preview.              | STOP            |
| `unknown`              | None of the above matched cleanly (e.g. live target SHA diverges from both `before_sha256` and `after_sha256`, or a refusal envelope without an A2-L2b context). | `STOP — escalate`. Inspect `evidence_paths` and any `stop_condition` value.                       | STOP            |

`apply_bundle_ready` vs `rolled_back`: when the apply-bundle exists
and the live target SHA equals `before_sha256`, the ambiguous case
biases to `apply_bundle_ready`. `rolled_back` requires an operator-
supplied approved `<approval-result.json>` as well — that is the only
filesystem-distinguishable rollback signal A2-L2d can derive without
a new L2b artifact.

## 6. STOP Conditions

If `next_operator_command` is `STOP — escalate`:

- **Inspect `evidence_paths`.** Every artifact the command read is
  listed there. Open those files directly to confirm the diagnosis.
- **Read `stop_condition`.** When non-null, it names the exact
  read-time STOP detected (one of the 11 closed values in
  [schema doc section 6](./a2-l2d-status-schema.md#6-closed-stop_condition-enum)).
- **Do not retry blindly.** Re-running `claw plan status` is safe
  (it is read-only and idempotent) but will produce the same STOP
  until the underlying condition is resolved.
- **Do not hand-build apply bundles.** `apply-bundle.json` is only
  produced by `claw plan apply-bundle`. Hand-constructed apply
  bundles are a STOP condition in the A2-L2b handoff
  ([section 8](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates))
  and this quick reference does not relax that.
- **Do not bypass approval.** No flag, environment variable, or
  alternate channel approves a preview. The approval line is
  TTY-enforced. See A2-L2c
  [section 3](./a2-l2c-operator-quickref.md#3-tty-approval-eof-note)
  for the TTY approval EOF note.

A refusal envelope (exit `EXIT_STATUS_REFUSED == 12`) is itself a
valid `a2-l2d-status.v1` document — the envelope shape is identical,
`phase` is `unknown`, `next_operator_command` is `STOP — escalate`,
and the `audit_markers` include `a2-l2d-status-refused`. See
[schema doc section 9](./a2-l2d-status-schema.md#9-refusal-envelope)
for the refusal-envelope contract.

## 7. Optional Approval-Result Argument

```text
claw plan status <workspace> <approval-result.json>
```

The optional second positional is the only permitted read outside
`<workspace>/.claw/**`. When supplied, the file is parsed read-only,
included in `evidence_paths`, and never modified.

This argument is **read-only**:

- It may **improve phase determination** — for example, it is what
  lets the command distinguish `awaiting_approval` from
  `approval_captured`, or `apply_bundle_ready` from `rolled_back`.
- It **does not approve anything.** Approval is exclusively
  performed by `claw plan approve`, which is TTY-enforced.
- It **does not apply anything.** Apply is exclusively performed by
  `claw plan apply`, which consumes a separately generated
  `apply-bundle.json`.
- It **does not authorize bypassing any A2-L2b gate.** If the
  supplied approval result is unparseable, schema-mismatched,
  step-id-mismatched, sha-mismatched, or decision-not-approved, the
  command surfaces the corresponding closed `stop_condition` or
  reverts the reported phase to `preview_ready`. It never elevates
  state.

## 8. Relationship To The Original Chain

The canonical A2-L2b chain is unchanged
([A2-L2b handoff section 4](./a2-l2b-run-plan-preview-operator-handoff.md#4-operator-command-flow),
[A2-L2c section 1](./a2-l2c-operator-quickref.md#1-at-the-keyboard-operator-path)):

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

`claw plan status <workspace> [<approval-result.json>]` is an
**inspection helper between those steps**, not a replacement for any
of them and not a new step in the chain. It reads only artifacts
those four commands already produce; it adds no new on-disk artifact
by default
([schema doc section 12](./a2-l2d-status-schema.md#12-non-goals-from-scope-card-5)).
Each canonical-chain command remains separately operator-invoked, and
the TTY-enforced approval boundary and explicit-apply boundary they
derive their safety from are untouched.

## 9. What A2-L2d Status Does Not Authorize

`claw plan status` is read-only by construction. It does not
authorize, enable, or provide a path to any of the following — all of
which remain forbidden by the A2-L2b chain and by
[`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
sections 5, 7, and 10:

- autonomous workspace-write execution is **not** authorized by this
  document
- `--yes`, `--auto`, `--skip-approval`, and `--no-prompt` are **not**
  authorized; no such flag exists on `claw plan status`, and the
  command refuses every write-adjacent flag listed in section 2
- approval bypass is **not** authorized; supplying an
  `<approval-result.json>` to status is a read, not an approval
- IDE approve/apply buttons are **not** authorized by exposing a
  read surface; any future IDE/harness consumer of
  `a2-l2d-status.v1` is bound by
  [scope-card section 10](./a2-l2d-readonly-inspection-scope-card.md#10-ide--harness-boundary)
- approve/apply composition (a single invocation that internally
  performs both `approve` and `apply`) is **not** authorized and
  would invalidate the TTY-enforced approval boundary
- model-initiated writes are **not** authorized; no broker, model,
  or Ollama call participates in the status command at any phase
- broker / model / Ollama traffic is **not** generated by `claw plan
  status`; the command pulls no networking crate and makes no HTTP
  call
- automatic rollback is **not** authorized; `phase == rolled_back`
  is a read-only diagnosis, not an action
- cleanup of stale or rolled-back runs is **not** authorized; the
  status command never writes, renames, or deletes any file
- `.claw/l2d-status` caches are **not** authorized; the default
  emission is pure-stdout
  ([schema doc section 12](./a2-l2d-status-schema.md#12-non-goals-from-scope-card-5))

Any near-term lane that proposes any of the above must be opened as
a separate, explicitly-authorized lane and must clear its own
review; this quick reference is **not** prior authorization for it.

## 10. References

- [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) — A2-L2d
  `a2-l2d-status.v1` schema-of-record; the canonical contract for
  every field, enum, exit code, refusal envelope, and invariant.
- [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — scope card defining the boundary this quick reference operates
  within (non-goals, forbidden surfaces, IDE/harness boundary,
  safety invariants).
- [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) —
  A2-L2c operator quick reference; the at-the-keyboard companion for
  the A2-L2b write chain that this read-only command inspects.
- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — runtime-proven A2-L2b operator chain (authoritative).
