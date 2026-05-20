# A2 L1a example: valid read-only FAST plan.
# Expected validator result: PASS.
name: read-only-discovery
mode: read-only
model_tier: FAST
steps:
  - id: locate-config
    description: Find the project config file
    tools: [Read, Grep]
  - id: summarize-readme
    description: Summarize the top-level README
    tools: [Read]
