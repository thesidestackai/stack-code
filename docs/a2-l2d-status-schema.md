# A2-L2d `a2-l2d-status.v1` Schema

This document is the schema-of-record for the A2-L2d Read-Only Artifact
Inspector / Status Contract, scoped by
[`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md).
It pins the on-disk-and-stdout shape `claw plan status <workspace>`
emits, the closed enums it carries, the new exit code it introduces,
and the read-only invariants every implementation must hold.

This file documents implemented behavior. It does **not** authorize
any new write affordance, IDE write control, approve/apply
composition, autonomous-write execution, or any other A2-L2b STOP-gate
weakening. The A2-L2b operator-gated chain remains authoritative.

## 1. Pinned Schema Version

```text
schema_version = "a2-l2d-status.v1"
```

Bumping this literal requires a separate scope-card amendment.
`claw plan status` always emits this exact literal as the first field
of every envelope (success and refusal).

## 2. Canonical Invocation

```text
claw plan status <workspace> [<approval-result.json>]
```

* `<workspace>` — required; absolute or relative path to the workspace
  root.
* `<approval-result.json>` — optional positional; the **only**
  permitted read outside `<workspace>/.claw/**`. When supplied, the
  file is parsed read-only, included in `evidence_paths`, and never
  modified. The implementation routes this read through a distinct
  code branch from automatic artifact discovery so the two read
  sources are never conflated.

No flags. Every write-adjacent flag (`--apply`, `--approve`, `--yes`,
`--auto`, `--clean`, `--rollback`, `--mutate`, `--all-runs`,
`--no-prompt`, `--skip-approval`, `--cache`) is refused outright. See
the integration tests under
`rust/crates/rusty-claude-cli/tests/plan_status.rs` for the closed list
of refused flags.

## 3. Output Envelope

The envelope is serialized as pretty JSON with the field order below.
Field order is part of the contract: two successive calls against an
unchanged workspace MUST produce byte-identical stdout.

| Field                  | Type                | Nullability | Derivation                                                                                                                                  |
|------------------------|---------------------|-------------|---------------------------------------------------------------------------------------------------------------------------------------------|
| `schema_version`       | string (literal)    | required    | Always the literal `a2-l2d-status.v1`.                                                                                                      |
| `workspace_root`       | string (path)       | required    | The operator-supplied workspace root, canonicalized via `fs::canonicalize`. If canonicalization fails the envelope is a refusal and this field carries the original operator-supplied path string. |
| `run_id`               | string \| null      | nullable    | Latest run directory name under `<workspace>/.claw/l2b-runs/`, selected by `run-manifest.json` mtime (lexicographically larger name breaks ties). Null when no run exists or in any refusal envelope. |
| `step_id`              | string \| null      | nullable    | `pending_step_id` from the latest `run-manifest.json`. Null when no run exists or in any refusal envelope.                                   |
| `phase`                | enum string         | required    | One of the 9 values in the closed `phase` enum (§4).                                                                                        |
| `next_operator_command`| string              | required    | One of the 3 closed `next_operator_command` shapes (§5).                                                                                    |
| `is_approvable`        | bool                | required    | Derived from `preview_record.is_binary` / `is_redacted` / `is_truncated`. False whenever any of those is true, or when no preview exists.   |
| `is_apply_ready`       | bool                | required    | True only when `apply-bundle.json` exists, validates against `a2-l2b-apply-bundle.v1`, the payload SHA sidecar matches `preview_record.after_sha256`, AND `stop_condition` is null. |
| `before_sha256`        | string \| null      | nullable    | `preview_record.before_sha256` (lowercase hex). Null when the preview record carries the empty string (target did not exist at preview time) or when no preview exists. |
| `after_sha256`         | string \| null      | nullable    | `preview_record.after_sha256` (lowercase hex). Null when no preview exists.                                                                 |
| `payload_sha256`       | string \| null      | nullable    | Contents of `<workspace>/.claw/l2b-payloads/<run-id>/<step-id>/after.sha256` (trimmed). Null when the sidecar is missing or unreadable.     |
| `live_target_sha256`   | string \| null      | nullable    | Lowercase hex SHA-256 of the live target file resolved from `preview_record.target_relative_path_sanitized` joined under the workspace root. Null when the live target is missing or unreadable. |
| `stop_condition`       | enum string \| null | nullable    | One of the 11 closed `stop_condition` values (§6) or null. Whenever non-null, `next_operator_command` is `"STOP — escalate"`.               |
| `evidence_paths`       | array of strings    | required    | Every artifact path that was read to produce the envelope, sorted lexicographically with duplicates removed.                                |
| `audit_markers`        | array of strings    | required    | One or more values drawn ONLY from the closed `a2-l2d-*` marker list (§7), sorted lexicographically with duplicates removed.                |
| `read_only_invariant`  | string (literal)    | required    | Always the literal `"this command does not mutate state"`, present on EVERY emission (success and refusal).                                 |

## 4. Closed `phase` Enum

Exactly these 9 values. New phases require a separate scope-card
amendment.

| Value                | Meaning                                                                                                                                                       |
|----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `no_run_found`       | `<workspace>/.claw/l2b-runs/` is missing or contains no run with a `run-manifest.json`.                                                                       |
| `preview_ready`      | Preview exists and the operator supplied an `<approval-result.json>` that did not advance the chain (e.g. unparseable, schema-mismatched); chain is back at preview time. |
| `awaiting_approval`  | Preview exists, is approvable, no `<approval-result.json>` supplied to the status command, no apply-bundle.json.                                              |
| `approval_captured`  | Preview exists, the operator-supplied `<approval-result.json>` validates against the preview record, no apply-bundle.json.                                    |
| `apply_bundle_ready` | `apply-bundle.json` exists, validates against `a2-l2b-apply-bundle.v1`, payload SHA sidecar matches `preview_record.after_sha256`, live target still at pre-write baseline. |
| `applied`            | Live target file's SHA matches `preview_record.after_sha256`.                                                                                                 |
| `rolled_back`        | Apply-bundle exists AND the operator supplies an approved `<approval-result.json>` AND the live target SHA matches `preview_record.before_sha256`. This is the only filesystem-distinguishable rollback signal A2-L2d can derive without a new L2b artifact. |
| `non_approvable`     | `preview_record.is_binary`, `is_redacted`, or `is_truncated` is true. Operator cannot approve; next operator command is `"STOP — escalate"`.                  |
| `unknown`            | None of the above match cleanly (e.g. live target SHA diverges from both before and after, or a refusal envelope without an A2-L2b context).                  |

## 5. Closed `next_operator_command` Shapes

Exactly these three shapes. The literal command string forms reuse the
canonical A2-L2b chain commands verbatim.

| Shape                                                          | When                                                                                  |
|----------------------------------------------------------------|---------------------------------------------------------------------------------------|
| `(no run found — start with claw plan run …)`                  | `phase == no_run_found`.                                                              |
| `STOP — escalate`                                              | `stop_condition` is non-null OR `phase` is one of `non_approvable`, `rolled_back`, `unknown`. |
| A literal canonical-chain command string                       | All other phases. The command name is `claw plan approve`, `claw plan apply-bundle`, `claw plan apply`, or `claw plan run --workspace-write-preview`, depending on the phase. Argument positions match the L2b operator handoff verbatim. |

## 6. Closed `stop_condition` Enum

Exactly these 11 values. Each maps to a named STOP gate in
[`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md#8-stop-gates)
section 8, or to a read-time STOP the status command can detect
without mutation.

| Value                              | Detection rule (read-only)                                                                                                              |
|------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------|
| `workspace-root-invalid`           | `fs::canonicalize` on `<workspace>` failed, or the canonical path is not a directory.                                                   |
| `run-manifest-unreadable`          | `<run-manifest.json>` exists but cannot be deserialized as the minimal subset.                                                          |
| `preview-bundle-unreadable`        | `<preview-bundle.json>` is missing or its `schema_version` is not `a2-l2b-preview-bundle.v1`.                                          |
| `payload-sha-mismatch`             | `<after.sha256>` sidecar value differs from `preview_record.after_sha256`.                                                              |
| `live-target-missing`              | `preview_record.before_sha256` is non-empty (target existed at preview time) but the live target file is missing now.                  |
| `live-target-sha-changed`          | Live target SHA matches neither `preview_record.before_sha256` nor `preview_record.after_sha256`.                                       |
| `approval-decision-not-approved`   | Operator-supplied approval-result has `decision != "approved"`.                                                                         |
| `approval-sha-mismatch`            | Operator-supplied approval-result `preview_sha256` differs from `preview_record.preview_sha256` (or the file is unparseable).           |
| `approval-step-id-mismatch`        | Operator-supplied approval-result `step_id` differs from `preview_record.step_id`.                                                      |
| `apply-bundle-schema-mismatch`     | `apply-bundle.json` exists but its `schema_version` is not `a2-l2b-apply-bundle.v1`.                                                    |
| `apply-bundle-target-path-mismatch`| `apply-bundle.json` `target_relative_path` does not match `preview_record.target_relative_path_sanitized`.                              |

## 7. Closed Audit Marker List

Exactly these 6 markers. The status command emits one or more of them
on every envelope. Production code MUST NOT reuse `a2-l1-*` or
`a2-l2b-*` markers. New markers require a separate scope-card
amendment.

| Marker                                  | When                                                                                       |
|-----------------------------------------|--------------------------------------------------------------------------------------------|
| `a2-l2d-status-read`                    | Always present on any envelope the command emits.                                          |
| `a2-l2d-status-no-run-found`            | `phase == no_run_found`.                                                                   |
| `a2-l2d-status-non-approvable`          | `phase == non_approvable`.                                                                 |
| `a2-l2d-status-stop-condition-detected` | `stop_condition` is non-null (success or refusal).                                         |
| `a2-l2d-status-idempotent-emit`         | The command emitted a non-refusal envelope.                                                |
| `a2-l2d-status-refused`                 | The command emitted a refusal envelope (exit `EXIT_STATUS_REFUSED`).                       |

## 8. Exit Codes

| Code | Constant                | Source                                              | Meaning                                                                                  |
|------|-------------------------|-----------------------------------------------------|------------------------------------------------------------------------------------------|
| `0`  | (no constant)           | `rusty-claude-cli/src/main.rs::run_plan_status`     | Status envelope emitted successfully. May still include a `stop_condition` indicating a STOP detected mid-chain. |
| `12` | `EXIT_STATUS_REFUSED`   | `a2-plan-runner/src/status.rs`                      | Read-time refusal envelope (workspace root invalid, run manifest unreadable, preview bundle unreadable). |

**Collision audit (2026-05-29 / `origin/main @ 12fff14`).** A2-L2b
already uses 0 (`EXIT_WRITE_APPLIED`), 5 (`EXIT_PARSE_ERROR` /
`EXIT_INVALID_REQUEST` and CLI mirrors), 6 (`EXIT_WRITE_PATH_REFUSED`),
7 (`EXIT_APPROVAL_DENIED` / `EXIT_APPROVAL_REFUSED` /
`EXIT_RUN_PLAN_WRITE_PREVIEW_READY`), 8 (`EXIT_ROLLBACK_FAILED`), 9
(`EXIT_CHECKPOINT_FAILED` / `EXIT_BASELINE_MISMATCH`), 10
(`EXIT_WRITE_IO_FAILED`), 11 (`EXIT_VALIDATION_ROLLED_BACK`). `12` is
unused and outside the L2b cluster.

## 9. Refusal Envelope

When the command emits a refusal (exit `EXIT_STATUS_REFUSED == 12`)
the envelope is still a valid `a2-l2d-status.v1` document and carries:

* `schema_version`: `"a2-l2d-status.v1"`
* `workspace_root`: the operator-supplied path (original string, not
  canonicalized)
* `run_id`, `step_id`: `null`
* `phase`: `"unknown"`
* `next_operator_command`: `"STOP — escalate"`
* `is_approvable`, `is_apply_ready`: `false`
* every SHA field: `null`
* `stop_condition`: one of the closed values from §6
* `evidence_paths`: `[]`
* `audit_markers`: lexicographically sorted with at least
  `a2-l2d-status-read`, `a2-l2d-status-refused`,
  `a2-l2d-status-stop-condition-detected`
* `read_only_invariant`: `"this command does not mutate state"`

## 10. Idempotency Rules

The status command MUST produce byte-identical stdout on two
successive invocations against an unchanged workspace. The
implementation enforces this by:

1. Iterating directory entries via `fs::read_dir` and sorting by
   `(mtime, name)` so run-selection is deterministic.
2. Sorting `evidence_paths` lexicographically and deduplicating.
3. Sorting `audit_markers` lexicographically and deduplicating.
4. Emitting JSON with fixed key order (defined by the struct field
   order in `rust/crates/a2-plan-runner/src/status.rs`).
5. Never including non-deterministic fields (timestamps, mtimes,
   PIDs, hostnames, broker metadata).

## 11. Read-Only Invariants

Pinned in `rust/crates/a2-plan-runner/src/status.rs` and enforced by
integration tests in `rust/crates/a2-plan-runner/tests/l2d_status.rs`
and `rust/crates/rusty-claude-cli/tests/plan_status.rs`:

* No filesystem mutation. Production code uses only `fs::read`,
  `fs::read_to_string`, `fs::read_dir`, `fs::metadata`, `Path::is_file`,
  `Path::is_dir`, and SHA-256 hashing of file contents.
* No network egress. The module pulls no networking crates and makes
  no HTTP / broker / Ollama call. Tests run with `HTTP_PROXY`,
  `HTTPS_PROXY`, `OLLAMA_HOST` set to unreachable sentinels.
* No subprocess execution.
* Read scope: only `<workspace>/.claw/**` (recursively under the
  L2b-owned roots) and the live target file resolved from the preview
  record. The optional `<approval-result.json>` operator-supplied
  positional argument is the only permitted read outside that scope
  and is on a distinct code branch.
* Idempotency: two successive calls on an unchanged workspace produce
  byte-identical stdout.

## 12. Non-Goals (from Scope Card §5)

A2-L2d does **not**:

* introduce or imply autonomous workspace-write execution
* introduce `--yes`, `--auto`, `--skip-approval`, `--no-prompt`,
  preapproval, batch approval, or any approval-bypass affordance
* merge `approve` and `apply` into a single command
* introduce any new write subcommand or write flag on any existing
  subcommand
* modify `claw plan run`, `claw plan approve`,
  `claw plan apply-bundle`, or `claw plan apply` behavior, exit codes,
  schemas, markers, or JSON field shapes
* modify `a2-l2b-*` schema versions or marker constants
* call broker, model, or Ollama at any phase
* write, rename, or delete any file under `.claw/` or the workspace
  tree from inside the status command
* implement `--all-runs` (deferred)
* implement the `.claw/l2d-status/<run-id>/last-read.json` cache
  (not authorized; default emission is pure-stdout)
* weaken any A2-L2b or A2-L2c STOP gate
* introduce an IDE/GUI surface
* implement a harness adapter

Any of the above must be opened as a separate, explicitly-authorized
lane.

## 13. References

* [`a2-l2d-readonly-inspection-scope-card.md`](./a2-l2d-readonly-inspection-scope-card.md)
  — scope card defining the boundary this schema doc implements.
* [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md)
  — A2-L2b runtime-proven chain (authoritative).
* [`a2-l2c-operator-quickref.md`](./a2-l2c-operator-quickref.md)
  — A2-L2c operator quick reference; the operator-facing surface
  A2-L2d augments with read-only state recovery.
* [`a2-l2d-operator-quickref.md`](./a2-l2d-operator-quickref.md)
  — A2-L2d operator quick reference; the at-the-keyboard companion
  to this schema-of-record for `claw plan status`.
* `rust/crates/a2-plan-runner/src/status.rs` — implementation
  source-of-record.
* `rust/crates/a2-plan-runner/tests/l2d_status.rs` — library
  invariant + phase + STOP coverage tests.
* `rust/crates/rusty-claude-cli/tests/plan_status.rs` — CLI
  end-to-end and flag-refusal tests.
