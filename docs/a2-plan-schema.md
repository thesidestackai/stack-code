# A2 Read-Only Plan Schema (L1a)

The `a2-plan-schema` crate is the offline first step of the A2 operator
workflow primitives. It parses a static plan YAML file into a typed `Plan`
and runs an offline validator that enforces the L1 safety contract.

L1a is intentionally **not** a runner:

- no broker calls
- no Ollama calls
- no tool execution
- no workspace mutation
- no DEEP tier

## Substrate

Stack-Code is wired against the local Stack substrate:

- broker route: `http://127.0.0.1:11435/v1` (informational only — this crate
  does not connect to it)
- FAST model: `qwen3:14b`

The L1a validator does not depend on the substrate being up. It is a pure
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
```

## L1 acceptance rules

| Condition                            | Result   | Marker                       |
| ------------------------------------ | -------- | ---------------------------- |
| `mode: read-only`                    | accepted | `a2-l1-accepted-readonly`    |
| `mode: workspace-write`              | refused  | `a2-l1-refused-write`        |
| `model_tier: FAST`                   | accepted | `a2-l1-accepted-readonly`    |
| `model_tier: DEEP`                   | refused  | `a2-l1-refused-deep`         |
| empty / missing `tools`              | refused  | `a2-l1-missing-tools`        |

Plan-level markers:

| Plan classification | Marker                            |
| ------------------- | --------------------------------- |
| all steps accepted  | `a2-l1-plan-validation-pass`      |
| any step refused    | `a2-l1-plan-validation-refused`   |

## Examples

The canonical L1a corpus lives in `examples/`:

- `a2_l1a_valid_readonly_plan.yaml` — passes validation
- `a2_l1a_refused_workspace_write.yaml` — refused via `a2-l1-refused-write`
- `a2_l1a_refused_deep.yaml` — refused via `a2-l1-refused-deep`
- `a2_l1a_missing_tools.yaml` — refused via `a2-l1-missing-tools`

These files are `include_str!`'d into the unit tests, so renaming or
deleting them breaks the build by design.

## Out of scope for L1a

- L1b: read-only plan runner (executes accepted steps against the local FAST
  model)
- L2: workspace-write plans with rollback and approval gates
- DEEP tier wiring
