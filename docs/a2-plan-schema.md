# A2 Plan Schema (L1a + L2a)

The `a2-plan-schema` crate is the offline first step of the A2 operator
workflow primitives. It parses a static plan YAML file into a typed `Plan`
and runs an offline validator that enforces the L1a/L2a safety contract.

This crate is intentionally **not** a runner. It performs no I/O of any kind:

- no broker calls
- no Ollama calls
- no tool execution
- no workspace mutation (no writes happen at this layer)
- no DEEP tier
- no filesystem canonicalization, parent-dir checks, or symlink-escape detection

Filesystem canonicalization, symlink-escape detection, parent-dir existence,
write-time TOCTOU checks, checkpointing, rollback, and approval prompts are
**deferred to a later runner/write lane**. L2a is schema acceptance only.

## Substrate

Stack-Code is wired against the local Stack substrate:

- broker route: `http://127.0.0.1:11435/v1` (informational only — this crate
  does not connect to it)
- FAST model: `qwen3:14b`

The validator does not depend on the substrate being up. It is a pure
function over a parsed `Plan`.

## Schema

```yaml
name: my-plan
mode: read-only          # top-level default (read-only | workspace-write)
model_tier: FAST         # top-level default (FAST | DEEP)
steps:
  - id: s1
    description: a step
    mode: read-only      # optional override
    model_tier: FAST     # optional override
    tools: [Read, Grep]  # required, non-empty
    expected_output:     # optional, advisory only
      must_contain: ["found"]

  - id: s2
    description: a workspace-write step
    mode: workspace-write
    tools: [Write]            # workspace-write steps MUST include `Write`
    write_target:             # required on workspace-write
      path: notes/scratch.md  # workspace-relative, lexically safe
      create_if_absent: true  # optional
    after_file: materialized/notes_scratch.after  # required on workspace-write
    expected_post_write:      # optional, advisory only
      must_contain: ["summary"]
      must_not_contain: ["TODO"]
```

## L1a acceptance rules (existing)

| Condition                            | Result   | Marker                       |
| ------------------------------------ | -------- | ---------------------------- |
| `mode: read-only` (clean shape)      | accepted | `a2-l1-accepted-readonly`    |
| `model_tier: FAST`                   | accepted | (no dedicated marker)        |
| `model_tier: DEEP`                   | refused  | `a2-l1-refused-deep`         |
| empty / missing `tools`              | refused  | `a2-l1-missing-tools`        |

## L2a workspace-write rules (new)

L2a accepts `mode: workspace-write` structurally when the step declares the
`Write` tool and a well-formed `write_target`. All path checks are **lexical
only**:

- Absolute paths are refused.
- Paths containing a `..` component are refused.
- Path components named `.git`, `.claw`, or `.claude` are denied anywhere in
  the path.
- Final-component deny patterns are applied: `.env`, `.env*`, `secret*`,
  `credentials*`, `*.pem`, `*.key`.

A workspace-write step that satisfies these rules emits
`a2-l1-accepted-workspace-write` AND `a2-l2a-after-file-shape-accepted`.

| Condition                                                           | Marker                                  |
| ------------------------------------------------------------------- | --------------------------------------- |
| workspace-write step missing `write_target`                         | `a2-l1-write-missing-target`            |
| workspace-write step missing `Write` in `tools`                     | `a2-l1-write-tool-missing`              |
| `write_target.path` is absolute or contains `..`                    | `a2-l1-write-path-refused`              |
| `write_target.path` matches deny-glob (`.git`, `.env`, `*.pem`, …)  | `a2-l1-write-path-denyglob`             |
| read-only step declares `Write` in `tools`                          | `a2-l1-write-tool-on-readonly`          |
| read-only step declares `write_target`                              | `a2-l1-write-target-on-readonly`        |
| read-only step declares `expected_post_write`                       | `a2-l1-expected-post-write-on-readonly` |

## L2a `after_file` rules (new in this lane)

`after_file` is the workspace-root-relative path of the file whose bytes
are the exact after-bytes for the workspace write. It is a top-level
`PlanStep` field — **not** nested under `write_target`.

- **Required** on every `mode: workspace-write` step.
- **Forbidden** on every `mode: read-only` step.
- Validated **lexically only** — the schema never opens, stat-s, or
  canonicalizes the path. Runtime file checks (existence,
  regular-file-ness, symlink rejection, size cap, byte read) are the
  future runner/materializer lane's responsibility.
- Same path-safety rule set as `write_target.path` (absolute / `..` /
  `.git` / `.claw` / `.claude` / `.env*` / `secret*` / `credentials*` /
  `*.pem` / `*.key` are refused). Note that `.claw` is denied
  unconditionally at L2a — there is currently no carveout for
  `.claw/l2b-materialized/…` and any future carveout must be a separate,
  deliberate lane.
- **`after_file` must not equal `write_target.path`** — a workspace-write
  step that names the live target as its own after-bytes source is
  incoherent and refused.

| Condition                                                           | Marker                              |
| ------------------------------------------------------------------- | ----------------------------------- |
| workspace-write step missing `after_file`                           | `a2-l2a-after-file-missing`         |
| read-only step declares `after_file`                                | `a2-l2a-after-file-on-readonly`     |
| `after_file` empty / absolute / contains `..` / same as `write_target.path` | `a2-l2a-after-file-path-refused`    |
| `after_file` matches deny-glob (`.git`, `.claw`, `.env`, `*.pem`, …) | `a2-l2a-after-file-path-denyglob`   |
| workspace-write step's `after_file` is lexically valid              | `a2-l2a-after-file-shape-accepted`  |

The `a2-l2a-` prefix on the new markers is intentional: existing log
scrapers that key on `a2-l1-` see unchanged behavior, and tooling that
wants to opt in to the L2a after-bytes contract can grep
`^a2-l2a-after-file-` independently.

Plan-level markers:

| Plan classification | Marker                            |
| ------------------- | --------------------------------- |
| all steps accepted  | `a2-l1-plan-validation-pass`      |
| any step refused    | `a2-l1-plan-validation-refused`   |

The `a2-l1-*` prefix is intentionally preserved for L2a markers because they
belong to the offline schema validator, not a future runner.

## Examples

The canonical L1a corpus lives in `examples/`:

- `a2_l1a_valid_readonly_plan.yaml` — passes validation
- `a2_l1a_refused_workspace_write.yaml` — refused (now via the L2a
  structural markers because the step has neither `Write` in `tools` nor a
  `write_target`)
- `a2_l1a_refused_deep.yaml` — refused via `a2-l1-refused-deep`
- `a2_l1a_missing_tools.yaml` — refused via `a2-l1-missing-tools`

The L2a corpus lives alongside it in `examples/`:

- `a2_l2a_valid_workspace_write_plan.yaml` — passes validation (now
  includes `after_file`; emits both `a2-l1-accepted-workspace-write` and
  `a2-l2a-after-file-shape-accepted`)
- `a2_l2a_refused_write_missing_target.yaml` — refused via
  `a2-l1-write-missing-target` (also now picks up
  `a2-l2a-after-file-missing` because the step has no `after_file`)
- `a2_l2a_refused_write_path_escape.yaml` — refused via
  `a2-l1-write-path-refused`
- `a2_l2a_refused_write_denyglob.yaml` — refused via
  `a2-l1-write-path-denyglob`
- `a2_l2a_refused_write_missing_after_file.yaml` — new; refused via
  `a2-l2a-after-file-missing` (workspace-write step with valid target
  but no `after_file`)

These files are `include_str!`'d into the unit and integration tests, so
renaming or deleting them breaks the build by design.

## Out of scope for L2a

- L1b: read-only plan runner (already shipped)
- L2 runner / write lane: actual workspace writes, filesystem canonicalization,
  symlink-escape detection, parent-dir existence checks, write-time TOCTOU
  checks, checkpointing, rollback, approval prompts
- DEEP tier wiring

## See also

- [`a2-l2b-run-plan-preview-operator-handoff.md`](./a2-l2b-run-plan-preview-operator-handoff.md) — operator-facing handoff for the runtime-proven A2-L2b `claw plan run --workspace-write-preview` → `approve` → `apply-bundle` → `apply` chain. Documents the gated operator flow only; does not authorize autonomous workspace-write execution from this schema crate.
