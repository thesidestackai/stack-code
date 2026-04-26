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

- Current baseline: `d53623e`
- MCP built-ins runtime state slice: `d53623e`

## Implementation Update — MCP Lifecycle Event Taxonomy / Inventory Consistency

Status: Done

### What changed

- Added a canonical runtime MCP lifecycle status shape for configured server status reporting.
- Direct `claw mcp` list/show surfaces now use live `RuntimeMcpState` lifecycle status when applicable.
- Model-facing `McpAuth` status now reports the same lifecycle status shape while remaining status-only.
- Preserved failure phase, message, context, and recoverability in lifecycle status output.
- Added deterministic tests for healthy stdio, unsupported transport, initialize failure, degraded startup, and recoverable tool discovery failure reporting.

### Files changed

- `rust/crates/rusty-claude-cli/src/main.rs`
- `docs/BUILDOUT_STATUS_2026-04-26.md`

### Validation

- command: `cargo fmt --all --check`
  result: pass
- command: `git diff --check`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp_inventory_and_tool_status_match_for_healthy_server -- --nocapture`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp_inventory_and_tool_status_match_for_unsupported_transport -- --nocapture`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp_inventory_and_tool_status_preserve_initialize_failure_phase -- --nocapture`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp_inventory_and_tool_status_match_for_degraded_startup -- --nocapture`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp_lifecycle_status_preserves_context_and_recoverability -- --nocapture`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp -- --nocapture`
  result: pass
- command: `cargo test -p runtime mcp -- --nocapture`
  result: pass
- command: `cargo test --workspace`
  result: fail; `resume_latest_restores_the_most_recent_managed_session` failed in the full run, then passed when rerun by name.
- command: `cargo test -p rusty-claude-cli --test resume_slash_commands resume_latest_restores_the_most_recent_managed_session -- --nocapture`
  result: pass
- command: `cargo clippy --workspace --all-targets -- -D warnings`
  result: fail; pre-existing `rust/crates/rusty-claude-cli/build.rs` clippy warnings (`map_unwrap_or`, `uninlined_format_args`) are outside this MCP slice.

### Remaining MCP gaps

- Remote transports still unsupported.
- OAuth/auth UX still not implemented.
- E2E parity harness coverage still open.
- Direct inventory is now runtime-backed for CLI list/show, but deeper shutdown/reset event history is still limited to existing runtime state.

### Commit

- committed locally; no push performed

## Implementation Update — E2E MCP Parity Harness Coverage

Status: Done

### What changed

- Added scripted mock Anthropic scenarios that exercise external MCP lifecycle behavior through the real CLI harness.
- Covered configured stdio MCP startup, `tools/list` discovery, dynamic model-facing `mcp__server__tool` exposure, server-routed tool calls, `resources/list`, and `resources/read`.
- Added degraded lifecycle coverage for a healthy server alongside an initialize failure and unsupported remote transport.
- Added deterministic harness assertions for request tool inventory, dynamic MCP tool results, direct MCP inventory, degraded startup reporting, and unsupported transport status.

### Files changed

- `rust/crates/mock-anthropic-service/src/lib.rs`
- `rust/crates/rusty-claude-cli/tests/mock_parity_harness.rs`
- `rust/mock_parity_scenarios.json`
- `docs/BUILDOUT_STATUS_2026-04-26.md`

### Validation

- command: `cargo fmt --all --check`
  result: pass
- command: `git diff --check`
  result: pass
- command: `cargo test -p rusty-claude-cli --test mock_parity_harness clean_env_cli_reaches_mock_anthropic_service_across_scripted_parity_scenarios -- --nocapture`
  result: pass
- command: `cargo test -p mock-anthropic-service`
  result: pass
- command: `cargo test -p rusty-claude-cli mcp -- --nocapture`
  result: pass
- command: `cargo test -p runtime mcp -- --nocapture`
  result: pass

### Remaining MCP gaps

- Remote transports still unsupported.
- OAuth/auth UX still not implemented.
- Direct inventory shutdown/reset event history remains limited to existing runtime state.
- Full workspace CI was not rerun for this slice.

### Commit

- committed locally; no push performed
