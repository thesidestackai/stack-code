# A2-L2b `run_plan` Preview-to-Apply — Operator Handoff

This document captures the runtime-proven operator path for the A2-L2b
`run_plan --workspace-write-preview` → `approve` → `apply-bundle` → `apply`
chain. It is **documentation only** — it does not change any runtime
behavior. It is the operator-facing source-of-truth for what is now
proven, what is explicitly out of scope, and the STOP gates that bound
every safe invocation.

## 1. Executive Summary

The A2-L2b `run_plan` preview-to-apply proof chain is runtime-proven in a
disposable temp workspace against binary SHA `1d0500e`. A complete
preview → approval → apply-bundle → apply cycle executed end-to-end with:

- exactly one workspace-local file mutated,
- the mutated target byte-equal to the operator-supplied `after_file`,
- no rollback markers,
- no broker, model, or Ollama traffic,
- no real repo changes (status and `HEAD` unchanged on both
  `/home/suki/sidestackai` and `/home/suki/stack-code`).

The chain is operator-gated at every transition. Apply only proceeds when
the approval record cryptographically binds to the preview bundle and the
live target still matches the recorded `before_sha256`.

## 2. What Is Proven

The following properties are observed under the smoke at SHA `1d0500e`:

- `claw plan run --workspace-write-preview` produces a preview bundle,
  a preview-generator result, a payload artifact, a payload SHA file,
  and a checkpoint manifest.
- The preview bundle is accepted by `claw plan approve`, which returns
  an approval result bound to the preview's `step_id` and
  `preview_sha256`.
- `claw plan apply-bundle` consumes the preview-generator result plus
  the approval result and emits a structured apply bundle. Generation
  fails closed if either input is missing fields, has mismatched
  identifiers, or has invalid JSON.
- `claw plan apply` consumes the apply bundle, performs a preflight,
  writes to a temp file, atomically replaces the target, validates the
  post-write hash, and emits `outcome: applied` with markers
  `a2-l2b-write-preflight-ok`, `a2-l2b-write-temp-created`,
  `a2-l2b-write-applied`, `a2-l2b-write-validated`.
- The post-apply target SHA is byte-equal to the operator's recorded
  `after_file` SHA.
- Real repository status is unchanged across both the host control
  checkout and the build worktree. No commits, pushes, or merges occur
  inside the apply chain.

## 3. What Is Not Proven / Not Allowed

The following are explicitly **outside** the proven envelope:

- No autonomous `run_plan apply`. The runner halts after
  `write_preview_ready` (exit `7`). Apply is a separate operator action.
- No multi-file or batch workspace writes. The proven chain applies
  exactly one target per apply bundle.
- No `--yes`, `--auto`, preapproval, or batch approval. Approval is
  TTY-enforced and must be reissued per preview.
- No model-generated `after` bytes. The `after_file` is operator-supplied
  and hashed at preview time; the apply step refuses to substitute any
  other content.
- No broker, model, or Ollama involvement at any phase. Both
  `apply-bundle` generation and `apply` are offline by construction.
- No commits, pushes, or merges performed by the chain itself.
- No write to real repository trees. The proven smoke wrote only to a
  disposable workspace under `/tmp/...`.

## 4. Operator Command Flow

The canonical sequence is:

```text
claw plan run <plan.yaml> --workspace-root <workspace> --workspace-write-preview
claw plan approve <preview-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan apply <apply-bundle.json>
```

Operator notes:

- `--workspace-write-preview` halts the runner immediately after the
  preview artifacts are written. Exit `7` (`write_preview_ready`) is the
  intended success state for this step, not a failure.
- `claw plan approve` is TTY-enforced. The approval line is
  `apply <step_id> <preview_sha256>`. Depending on the terminal driver
  in use, an explicit EOF after that line may be required for the CLI
  to consume the input.
- `claw plan apply-bundle` is offline. It cross-validates the approval
  result against the preview bundle before emitting an apply bundle.
- `claw plan apply` runs preflight, write-to-temp, atomic replace, and
  post-write validation. On any mid-write failure, the runtime attempts
  rollback to the checkpoint baseline.

## 5. Artifact Lifecycle

Each preview run produces a self-contained set of artifacts rooted at
`<workspace>/.claw/`:

```text
.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-bundle.json
.claw/l2b-preview-bundles/<run-id>/<step-id>/preview-generator-result.json
.claw/l2b-preview-bundles/<run-id>/<step-id>/apply-bundle.json
.claw/l2b-payloads/<run-id>/<step-id>/after.bin
.claw/l2b-payloads/<run-id>/<step-id>/after.sha256
.claw/l2b-checkpoints/<run-id>/<step-id>/manifest.json
.claw/l2b-checkpoints/<run-id>/<step-id>/before.bin
.claw/l2b-runs/<run-id>/run-manifest.json
.claw/l2b-runs/<run-id>/status.json
```

The `apply-bundle.json` is created only by `claw plan apply-bundle` and
binds the approval to the preview record. The `before.bin` checkpoint
is the authoritative pre-write baseline used by rollback.

## 6. Authority Chain

`claw plan apply` is only safe because every input is independently
revalidated at apply time. The runtime composes:

- `ResolvedWriteTarget` — path-safety-resolved workspace-relative
  target.
- `CheckpointHandle` — references the pre-write `before.bin` baseline.
- `PreviewRecord` — the canonical preview hash and `step_id`.
- `ApprovalDecision::Approved` — TTY-enforced operator decision bound
  to the preview record.
- `ApprovedWritePayload` — the operator-supplied `after.bin` whose
  SHA must match the preview's recorded `payload_sha256`.

At apply time, the runtime re-verifies:

- payload SHA equals `preview_record.payload_sha256`,
- checkpoint baseline still matches the on-disk pre-write target,
- live target SHA still matches `preview_record.before_sha256`,
- post-write target SHA matches `preview_record.after_sha256`.

Any of these mismatches fails apply closed and triggers rollback.

## 7. Exit Codes / Status

The following exit codes are pinned in source today and are the only
ones documented here. Operators should treat any other code as
unenumerated and stop.

| Code | Source                                          | Meaning                                                         |
|------|-------------------------------------------------|-----------------------------------------------------------------|
| `0`  | `write_executor::EXIT_WRITE_APPLIED`            | Apply succeeded; post-write validation passed.                  |
| `5`  | `report::EXIT_PARSE_ERROR` and slice mirrors    | Input parse / bundle / generator rejection (closed refusal).    |
| `6`  | `write_runtime::EXIT_WRITE_PATH_REFUSED`        | L2b write-target path safety refused the request.               |
| `7`  | `approval::EXIT_APPROVAL_DENIED` / `runner::EXIT_RUN_PLAN_WRITE_PREVIEW_READY` | Two distinct uses of `7`: preview-ready halt **or** approval refusal. Operators must read `status` / `outcome` to disambiguate. |
| `8`  | `write_executor::EXIT_ROLLBACK_FAILED`          | Rollback could not restore the baseline. Workspace is in an uncertain state and requires manual recovery. |
| `9`  | `checkpoint::EXIT_CHECKPOINT_FAILED` / `write_executor::EXIT_BASELINE_MISMATCH` | Checkpoint failure or baseline drift. Apply did not run. |
| `10` | `write_executor::EXIT_WRITE_IO_FAILED`          | Write I/O failed before atomic replace. Target unchanged.       |
| `11` | `write_executor::EXIT_VALIDATION_ROLLED_BACK`   | Post-write validation failed and rollback succeeded.            |

Exit `7` is intentionally overloaded between "halted, awaiting approval"
(from `claw plan run --workspace-write-preview`) and "approval refused"
(from `claw plan approve`). Disambiguate via the `status` /
`outcome` / `decision` fields in the structured JSON output.

## 8. STOP Gates

The operator must stop and escalate, not retry, on any of the following:

- Live target SHA changed between preview and apply.
- Preview SHA does not match between preview bundle and approval result.
- Payload SHA on disk does not match `preview_record.payload_sha256`.
- Approval decision is anything other than `approved`.
- Apply bundle schema is not `a2-l2b-apply-bundle.v1`.
- Apply exits `8` (`EXIT_ROLLBACK_FAILED`) — target may be partially
  modified.
- Real repo status (`/home/suki/sidestackai`, `/home/suki/stack-code`,
  or any non-disposable repo) changes during the apply chain.
- Any traffic appears to `:11434`, `:11435`, or any model/broker
  endpoint during apply or apply-bundle stages.
- Any `apply-bundle.json` is constructed by hand or patched outside the
  CLI — only `claw plan apply-bundle` is authorized to produce it.
- `git apply`, `git commit`, `git push`, `git merge` are invoked inside
  the apply chain.

## 9. Smoke Evidence

The proof of the chain is the smoke at:

```text
root:                /tmp/a2-l2b-run-plan-preview-smoke-20260528T223411Z
binary SHA:          1d0500e
schema (preview):    a2-l2b-preview-bundle.v1
schema (apply):      a2-l2b-apply-bundle.v1
schema (apply result): a2-l2b-apply-result.v1
target before SHA:   19acba7e87843bf3631359568ed357f2e3e74b3935b1488602fe66a363b64437
after / target SHA:  9340874dd74f9ce2cfc85f035338ec174dbf92a5a3856b3c5ab0add387873235
apply exit:          0
outcome:             applied
classification:      FULL_PASS
```

Raw payload contents are intentionally not included here. The operator
artifacts under the smoke `logs/` directory are the authoritative
record:

- `apply_chain_summary.json`
- `apply_chain_verification_results.txt`
- `apply_from_run_plan_stdout.json`
- `apply_bundle_from_run_plan_stdout.json`

## 10. Recommended Next Lanes

The default recommendation is to **pause before A2-L2c autonomous write
integration** and first land operator-facing documentation/README
references that point at this handoff and the existing
`docs/a2-plan-schema.md`. The proven chain is safe but the operator
ergonomics around exit-code overloading on `7`, TTY approval EOF
handling, and per-step artifact navigation are still implicit. Closing
those operator gaps before adding any autonomous behavior keeps the
"operator-gated by design" property load-bearing.

A2-L2c design is the next lane only after operator docs/readme
integration. It is explicitly out of scope of this handoff.

See [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md) for the A2-L2c operator quick reference (exit-code `7` disambiguation, TTY approval EOF note, per-step artifact map). Docs-only; does not authorize autonomous workspace-write execution.

See [`a2-l2d-status-schema.md`](./a2-l2d-status-schema.md) for the A2-L2d `a2-l2d-status.v1` read-only state schema and the `claw plan status <workspace> [<approval-result.json>]` command surface. Read-only by construction; does not authorize autonomous workspace-write execution, approval bypass, or IDE write controls.
