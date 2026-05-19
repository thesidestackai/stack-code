# VS Code task wrapper for Stack-Code

This is a thin Command Palette wrapper around the existing `claw` CLI. It is **not** a VS Code extension, daemon, language server, or ACP/MCP service — it is just a committed `.vscode/tasks.json` that drives the `claw` binary through `scripts/claw-sidestack-local`.

## Intent

Give Suki a non-terminal-first entry point to Stack-Code from VS Code without taking on extension-scaffold maintenance and without bypassing the LAW 1 broker routing centralized in `scripts/claw-sidestack-local`.

## Hard guarantees

- **Read-only by default.** Every prompt task passes `--permission-mode read-only`. No task uses `workspace-write`. No task uses `danger-full-access`.
- **LAW 1 routing is delegated, not duplicated.** Tasks invoke `scripts/claw-sidestack-local`. That wrapper sources `examples/sidestack-local.env`, sets `OPENAI_BASE_URL=http://127.0.0.1:11435/v1`, and refuses to exec `claw` if the effective base URL points at `:11434` (raw Ollama). `tasks.json` itself never inlines `OPENAI_BASE_URL`, broker ports, or `RUSTY_CLAUDE_MODEL_ALIAS__*`.
- **No daemon.** This wrapper does not start an HTTP server, WebSocket, MCP server, or ACP daemon. ACP entrypoints are intentionally not referenced from these tasks.

## How to use

1. Open this repository in VS Code.
2. Open the Command Palette and run `Tasks: Run Task`.
3. Pick one of the tasks listed below.

The integrated terminal will show wrapper diagnostics on stderr (active profile, allowlisted broker URL) followed by `claw` output on stdout.

## Available tasks

| Task | What it runs | Notes |
| --- | --- | --- |
| Claw: Doctor (JSON) | `claw doctor --output-format json` | Health check; first run after a build. |
| Claw: Status (JSON) | `claw status --output-format json` | Current CLI/session status. |
| Claw: Sandbox (JSON) | `claw sandbox --output-format json` | Sandbox configuration snapshot. |
| Claw: State (JSON) | `claw state --output-format json` | Local state inspector. |
| Claw: Init Project (JSON) | `claw init --output-format json` | **Run in a disposable scratch directory only — not in `~/stack-code` and not in `~/sidestackai`.** It writes `.claw/`, `.claw.json`, `.gitignore` entries, and `CLAUDE.md`. |
| Claw: Prompt (read-only, FAST) | `claw --model fast --permission-mode read-only prompt "<text>" --output-format json` | Prompts for input via the Command Palette. FAST alias resolves through the broker profile. |
| Claw: Prompt (read-only, DEEP) | `claw --model deep --permission-mode read-only prompt "<text>" --output-format json` | DEEP alias resolves through the broker profile. |
| Claw: Resume Latest (REPL) | `claw --resume latest` | Interactive REPL in a dedicated terminal panel. |

## Model routing

The FAST and DEEP aliases are not defined in `tasks.json`. They come from `examples/sidestack-local.env`, which the wrapper sources before invoking `claw`. To change which concrete model FAST or DEEP resolves to, edit that env file, not the task wrapper.

## Offline validation

You can verify the task surface without calling the live broker:

1. Open this worktree in VS Code.
2. `Tasks: Run Task` → confirm every `Claw: …` task is listed.
3. Inspect the generated task definitions in the Command Palette UI.
4. Optionally run `Claw: Init Project (JSON)` **inside a disposable scratch directory** opened as a separate VS Code workspace.

Do not run prompt tasks during offline validation — they hit the broker.

## Live broker validation

Live broker validation (running a real `Claw: Prompt` against `:11435`) is a separate gated lane and requires explicit operator approval. See the next-lane note in the lane handoff for the smoke-gate prompt.

## Why no extension

A VS Code extension would force ongoing maintenance of a TypeScript scaffold, a publisher identity, and a marketplace presence for a surface that is currently three Command Palette buttons. Tasks already give us labels, inputs, terminal integration, and per-workspace scoping with zero install steps beyond opening the folder.
