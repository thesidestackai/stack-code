# Buildout Status - 2026-04-26

## Done

- Priority 1, item 1 session compaction parity slice closed at commit `e008b36`.
- MCP runtime lifecycle depth slice started.
- CLI model-facing legacy MCP built-ins now prefer live `RuntimeMcpState` when the CLI has MCP runtime state.
- The legacy global MCP registry remains the fallback path when no CLI runtime MCP state exists.
- CLI MCP degraded reports now preserve discovery failure phase, context, and recoverability from `McpServerManager`.
- Deterministic CLI tests assert live runtime behavior for legacy `ListMcpResources`, `ReadMcpResource`, `MCP`, and `McpAuth`.

## In Progress

- MCP runtime lifecycle parity remains the active buildout slice pending review.

## Blocked

- Remote transports, OAuth/auth UX, and deeper direct CLI inventory lifecycle work remain out of scope for this slice.
- Full CI green is not claimed; only the validation commands below were run.

## Files changed

- `rust/crates/rusty-claude-cli/src/main.rs`
- `docs/BUILDOUT_STATUS_2026-04-26.md`

## Validation commands

- `cargo fmt --all --check`
- `git diff --check`
- `cargo test -p rusty-claude-cli build_runtime_plugin_state_discovers_mcp_tools_and_surfaces_pending_servers`
- `cargo test -p rusty-claude-cli build_runtime_plugin_state_surfaces_unsupported_mcp_servers_structurally`
- `cargo test -p rusty-claude-cli mcp`

## Commit hash if committed

- Current baseline: `e008b36`
- MCP slice commit: pending.
