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
`a2-l1-accepted-workspace-write`.

| Condition                                                           | Marker                                  |
| ------------------------------------------------------------------- | --------------------------------------- |
| workspace-write step missing `write_target`                         | `a2-l1-write-missing-target`            |
| workspace-write step missing `Write` in `tools`                     | `a2-l1-write-tool-missing`              |
| `write_target.path` is absolute or contains `..`                    | `a2-l1-write-path-refused`              |
| `write_target.path` matches deny-glob (`.git`, `.env`, `*.pem`, …)  | `a2-l1-write-path-denyglob`             |
| read-only step declares `Write` in `tools`                          | `a2-l1-write-tool-on-readonly`          |
| read-only step declares `write_target`                              | `a2-l1-write-target-on-readonly`        |
| read-only step declares `expected_post_write`                       | `a2-l1-expected-post-write-on-readonly` |

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

- `a2_l2a_valid_workspace_write_plan.yaml` — passes validation
- `a2_l2a_refused_write_missing_target.yaml` — refused via
  `a2-l1-write-missing-target`
- `a2_l2a_refused_write_path_escape.yaml` — refused via
  `a2-l1-write-path-refused`
- `a2_l2a_refused_write_denyglob.yaml` — refused via
  `a2-l1-write-path-denyglob`

These files are `include_str!`'d into the unit and integration tests, so
renaming or deleting them breaks the build by design.

## Out of scope for L2a

- L1b: read-only plan runner (already shipped)
- L2 runner / write lane: actual workspace writes, filesystem canonicalization,
  symlink-escape detection, parent-dir existence checks, write-time TOCTOU
  checks, checkpointing, rollback, approval prompts
- DEEP tier wiring
