# A2 Option B Validation Helper Path Fix

## Outcome

No canonical repo docs/handoffs currently contain the stale panel-local helper path.

A repo-wide search (docs, handoffs, and tracked files outside them) for the stale
panel-local helper path — a copy under `ide/vscode/a2-harness-panel/scripts/` —
returned **zero** matches. Every canonical reference already uses the correct
repo-root path `scripts/a2-ide-harness.sh`.

The post-merge validation discrepancy (PR #135, merge `dc929d4`) was therefore a
**prompt-path bug from the session text**, not a defect in any committed file.

## Correct vs. stale path

The correct helper path for future Option B validation is:

`scripts/a2-ide-harness.sh`

Do not reference a panel-local copy of the helper (i.e. one nested under the panel's
own `ide/vscode/a2-harness-panel/scripts/` directory): no such copy exists. The helper
is tracked only at repo root. The panel's `helperRunner` allowlists the basename
`a2-ide-harness.sh` and spawns it (array-argv, `shell:false`) as its single spawn
boundary.

## Why this matters

The Option B post-merge validation prompt's expected-surface allowlist and helper-smoke
invocation both assumed the panel-local path. Against that wrong path, the literal
surface scan falsely flags the (correct, reviewed) repo-root helper as "unexpected",
risking a false STOP. Future Option B validation prompts must reference
`scripts/a2-ide-harness.sh`.

## Safety

No source files, helper scripts, tests, package files, runtime files, branches,
worktrees, or remote refs were changed. This lane is docs/handoff-only.
