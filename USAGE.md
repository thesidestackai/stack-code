# Claw Code Usage

This guide covers the current Rust workspace under `rust/` and the `claw` CLI binary. If you are brand new, make the doctor health check your first run: start `claw`, then run `/doctor`.

## Quick-start health check

Run this before prompts, sessions, or automation:

```bash
cd rust
cargo build --workspace
cargo run -p rusty-claude-cli --
# first command inside the REPL
/doctor
```

`/doctor` is the built-in setup and preflight diagnostic. Once you have a saved session, you can rerun it with `cargo run -p rusty-claude-cli -- --resume latest /doctor`.

## Prerequisites

- Rust toolchain with `cargo`
- One of:
  - `ANTHROPIC_API_KEY` for direct API access
  - `ANTHROPIC_AUTH_TOKEN` for bearer-token auth
- Optional: `ANTHROPIC_BASE_URL` when targeting a proxy or local service

## Install / build the workspace

```bash
cd rust
cargo build --workspace
```

The CLI binary is available under Cargo's active `target_directory` after a debug build. Make the doctor check above your first post-build step.

On this workstation, `rust/.cargo/config.toml` may redirect Cargo build artifacts to the 18TB build-artifacts drive. Use `cargo metadata --format-version 1 --no-deps` to confirm `target_directory` instead of assuming `rust/target`.

`./install.sh` is a **build helper, not a host installer**. It detects the OS, checks the toolchain, builds the workspace, verifies the artifact, and prints where it landed. It does not copy anything into `~/.local/bin`, does not modify PATH, and does not touch the canonical SideStackAI `claw` executable.

### The canonical SideStackAI `claw` executable

The SideStackAI operator entrypoint is `~/.local/bin/claw`, and it is a **regular file** — never a symlink, and never a Cargo `target/debug/claw` or `target/release/claw` artifact. A Cargo build cache must never be the live operator binary. `cargo install` / `~/.cargo/bin/claw` remain valid generic upstream options, but they are not SideStack's canonical entrypoint and SideStack tooling never executes them.

```bash
# read-only freshness report: no build, no network, no writes
./scripts/claw-canonical-status

# explicit opt-in refresh: rebuild from this worktree, atomically activate
./scripts/claw-canonical-refresh
```

`claw-canonical-status` exits `0` CURRENT, `10` STALE, `11` MISSING, `12` INVALID_TOPOLOGY, `13` UNKNOWN_BASE (no locally known `origin/main`), and reports the canonical path, its topology, the installed Git SHA, and the `origin/main` SHA it compared against.

`claw-canonical-refresh` runs only from a clean worktree whose `HEAD` equals the locally known `origin/main` — it never fetches, so you decide when the base moves. It refuses a dirty source, refuses a symlinked canonical path rather than writing through it, forces `CARGO_TARGET_DIR` inside the worktree so a repo-local `.cargo/config.toml` cannot redirect the build, backs up the existing executable before building, requires the built binary to report the source Git SHA, activates by same-directory atomic rename, and restores the previous executable if post-activation verification fails.

## Quick start

### First-run doctor check

```bash
cd rust
cargo run -p rusty-claude-cli --
/doctor
```

Or run doctor directly with JSON output for scripting:

```bash
cd rust
cargo run -p rusty-claude-cli -- doctor --output-format json
```

**Note:** Diagnostic verbs (`doctor`, `status`, `sandbox`, `version`) support `--output-format json` for machine-readable output. Invalid suffix arguments (e.g., `--json`) are now rejected at parse time rather than falling through to prompt dispatch.

### Initialize a repository

Set up a new repository with `.claw` config, `.claw.json`, `.gitignore` entries, and a `CLAUDE.md` guidance file:

```bash
cd /path/to/your/repo
cargo run --manifest-path /path/to/claw-code/rust/Cargo.toml -p rusty-claude-cli -- init
```

Text mode (human-readable) shows artifact creation summary with project path and next steps. Idempotent — running multiple times in the same repo marks already-created files as "skipped".

JSON mode for scripting:
```bash
cargo run --manifest-path /path/to/claw-code/rust/Cargo.toml -p rusty-claude-cli -- init --output-format json
```

Returns structured output with `project_path`, `created[]`, `updated[]`, `skipped[]` arrays (one per artifact), and `artifacts[]` carrying each file's `name` and machine-stable `status` tag. The legacy `message` field preserves backward compatibility.

**Why structured fields matter:** Claws can detect per-artifact state (`created` vs `updated` vs `skipped`) without substring-matching human prose. Use the `created[]`, `updated[]`, and `skipped[]` arrays for conditional follow-up logic (e.g., only commit if files were actually created, not just updated).

### Interactive REPL

```bash
cd rust
cargo run -p rusty-claude-cli --
```

### One-shot prompt

```bash
cd rust
cargo run -p rusty-claude-cli -- prompt "summarize this repository"
```

### Shorthand prompt mode

```bash
cd rust
cargo run -p rusty-claude-cli -- "explain rust/crates/runtime/src/lib.rs"
```

### JSON output for scripting

```bash
cd rust
cargo run -p rusty-claude-cli -- --output-format json prompt "status"
```

### Inspect worker state

The `claw state` command reads `.claw/worker-state.json`, which is written by the interactive REPL or a one-shot prompt when a worker executes a task. This file contains the worker ID, session reference, model, and permission mode.

Prerequisite: You must run `claw` (interactive REPL) or `claw prompt <text>` at least once in the repository to produce the worker state file.

```bash
cd rust
cargo run -p rusty-claude-cli -- state
```

JSON mode:
```bash
cargo run -p rusty-claude-cli -- state --output-format json
```

If you run `claw state` before any worker has executed, you will see a helpful error:
```
error: no worker state file found at .claw/worker-state.json
  Hint: worker state is written by the interactive REPL or a non-interactive prompt.
  Run:   claw               # start the REPL (writes state on first turn)
  Or:    claw prompt <text> # run one non-interactive turn
  Then rerun: claw state [--output-format json]
```

## Advanced slash commands (Interactive REPL only)

These commands are available inside the interactive REPL (`claw` with no args). They extend the assistant with workspace analysis, planning, and navigation features.

### `/ultraplan` — Deep planning with multi-step reasoning

**Purpose:** Break down a complex task into steps using extended reasoning.

```bash
# Start the REPL
claw

# Inside the REPL
/ultraplan refactor the auth module to use async/await
/ultraplan design a caching layer for database queries
/ultraplan analyze this module for performance bottlenecks
```

Output: A structured plan with numbered steps, reasoning for each step, and expected outcomes. Use this when you want the assistant to think through a problem in detail before coding.

### `/teleport` — Jump to a file or symbol

**Purpose:** Quickly navigate to a file, function, class, or struct by name.

```bash
# Jump to a symbol
/teleport UserService
/teleport authenticate_user
/teleport RequestHandler

# Jump to a file
/teleport src/auth.rs
/teleport crates/runtime/lib.rs
/teleport ./ARCHITECTURE.md
```

Output: The file content, with the requested symbol highlighted or the file fully loaded. Useful for exploring the codebase without manually navigating directories. If multiple matches exist, the assistant shows the top candidates.

### `/bughunter` — Scan for likely bugs and issues

**Purpose:** Analyze code for common pitfalls, anti-patterns, and potential bugs.

```bash
# Scan the entire workspace
/bughunter

# Scan a specific directory or file
/bughunter src/handlers
/bughunter rust/crates/runtime
/bughunter src/auth.rs
```

Output: A list of suspicious patterns with explanations (e.g., "unchecked unwrap()", "potential race condition", "missing error handling"). Each finding includes the file, line number, and suggested fix. Use this as a first pass before a full code review.

## Model and permission controls

```bash
cd rust
cargo run -p rusty-claude-cli -- --model sonnet prompt "review this diff"
cargo run -p rusty-claude-cli -- --permission-mode read-only prompt "summarize Cargo.toml"
cargo run -p rusty-claude-cli -- --permission-mode workspace-write prompt "update README.md"
cargo run -p rusty-claude-cli -- --allowedTools read,glob "inspect the runtime crate"
```

Supported permission modes:

- `read-only`
- `workspace-write`
- `danger-full-access`

Model aliases currently supported by the CLI:

- `opus` → `claude-opus-4-6`
- `sonnet` → `claude-sonnet-4-6`
- `haiku` → `claude-haiku-4-5-20251213`

## Authentication

### API key

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### OAuth

```bash
cd rust
export ANTHROPIC_AUTH_TOKEN="anthropic-oauth-or-proxy-bearer-token"
```

### Which env var goes where

`claw` accepts two Anthropic credential env vars and they are **not interchangeable** — the HTTP header Anthropic expects differs per credential shape. Putting the wrong value in the wrong slot is the most common 401 we see.

| Credential shape | Env var | HTTP header | Typical source |
|---|---|---|---|
| `sk-ant-*` API key | `ANTHROPIC_API_KEY` | `x-api-key: sk-ant-...` | [console.anthropic.com](https://console.anthropic.com) |
| OAuth access token (opaque) | `ANTHROPIC_AUTH_TOKEN` | `Authorization: Bearer ...` | an Anthropic-compatible proxy or OAuth flow that mints bearer tokens |
| OpenRouter key (`sk-or-v1-*`) | `OPENAI_API_KEY` + `OPENAI_BASE_URL=https://openrouter.ai/api/v1` | `Authorization: Bearer ...` | [openrouter.ai/keys](https://openrouter.ai/keys) |

**Why this matters:** if you paste an `sk-ant-*` key into `ANTHROPIC_AUTH_TOKEN`, Anthropic's API will return `401 Invalid bearer token` because `sk-ant-*` keys are rejected over the Bearer header. The fix is a one-line env var swap — move the key to `ANTHROPIC_API_KEY`. Recent `claw` builds detect this exact shape (401 + `sk-ant-*` in the Bearer slot) and append a hint to the error message pointing at the fix.

**If you meant a different provider:** if `claw` reports missing Anthropic credentials but you already have `OPENAI_API_KEY`, `XAI_API_KEY`, or `DASHSCOPE_API_KEY` exported, you most likely forgot to prefix the model name with the provider's routing prefix. Use `--model openai/gpt-4.1-mini` (OpenAI-compat / OpenRouter / Ollama), `--model grok` (xAI), or `--model qwen-plus` (DashScope) and the prefix router will select the right backend regardless of the ambient credentials. The error message now includes a hint that names the detected env var.

## Local Models

`claw` can talk to local servers and provider gateways through either Anthropic-compatible or OpenAI-compatible endpoints. Use `ANTHROPIC_BASE_URL` with `ANTHROPIC_AUTH_TOKEN` for Anthropic-compatible services, or `OPENAI_BASE_URL` with `OPENAI_API_KEY` for OpenAI-compatible services.

### SideStackAI broker (local-first)

If you are running the SideStackAI local stack, route `claw` through the broker on `127.0.0.1:11435` rather than talking to the underlying app inference port directly. This keeps every call observable in the broker logs and lets the gateway apply its quotas, retries, and routing rules.

```bash
export OPENAI_BASE_URL="http://127.0.0.1:11435/v1"
export OPENAI_API_KEY="local-dev-token"

cd rust
./target/debug/claw \
  --permission-mode read-only \
  --model "openai/qwen3.5:9b" \
  prompt "reply with the word ready"
```

Notes:

- Use the `openai/<model>` prefix (for example `openai/qwen3.5:9b`). Bare model names like `qwen3.5:9b` fail CLI validation because the prefix router cannot disambiguate them from a cloud provider.
- `claw` now defaults to `read-only` when neither `RUSTY_CLAUDE_PERMISSION_MODE` nor a project-level `.claw/settings.json` overrides it. Passing `--permission-mode read-only` explicitly is still recommended as defense-in-depth — it makes the intent visible at the call site and survives accidental env-var changes.
- Need writes? Use `--permission-mode workspace-write` for that single invocation, or set `RUSTY_CLAUDE_PERMISSION_MODE=workspace-write` in the shell where you intend to do work.

#### Optional broker ergonomics

When `OPENAI_BASE_URL` points at the SideStackAI broker, `claw` will additionally honour a few opt-in environment variables that make local usage more observable and ergonomic. None of them are required — omit any line below and the corresponding feature simply stays off.

```bash
export OPENAI_BASE_URL="http://127.0.0.1:11435/v1"
export OPENAI_API_KEY="local"
export RUSTY_CLAUDE_LLM_CALLER="stack-code"
export RUSTY_CLAUDE_TASK_TYPE="code"
export RUSTY_CLAUDE_MODEL_ALIAS__FAST="qwen3:14b"
export RUSTY_CLAUDE_MODEL_ALIAS__DEEP="qwen3.5:27b"
```

These same exports are also shipped as a sourceable file at [`examples/sidestack-local.env`](examples/sidestack-local.env), so a typical SideStackAI session is just:

```bash
source examples/sidestack-local.env
claw --model fast prompt "reply with the word ready"
```

**Broker caller headers (`RUSTY_CLAUDE_LLM_CALLER`, `RUSTY_CLAUDE_TASK_TYPE`)**

- When set to a non-empty, non-whitespace value, these env vars are forwarded on every OpenAI-compatible request as the `X-LLM-Caller` and `X-Task-Type` headers. The broker uses them to attribute traffic in its logs and dashboards.
- Both are optional metadata. Blank or whitespace-only values are dropped so the headers are not sent.
- They have no effect on routing or model selection — they are purely observational.

**Local model aliases (`RUSTY_CLAUDE_MODEL_ALIAS__<NAME>`)**

Define short aliases for the upstream model strings your local broker actually serves. For the example above, `--model fast` resolves to `qwen3:14b` and `--model deep` resolves to `qwen3.5:27b`. The model values shown are illustrative for a SideStackAI setup — substitute whatever your broker has loaded.

Alias matching rules:

- The env-var suffix after `RUSTY_CLAUDE_MODEL_ALIAS__` may contain ASCII letters, digits, and underscores.
- Alias lookup is case-insensitive: `--model fast`, `--model Fast`, and `--model FAST` all resolve through `RUSTY_CLAUDE_MODEL_ALIAS__FAST`. Define the env var with the uppercase suffix.
- A blank or whitespace-only alias value is ignored, and the requested model passes through unchanged.
- Only bare, ASCII-alphanumeric/underscore model strings are eligible for alias lookup. Anything with a slash, colon, dot, or dash (for example `openai/gpt-4.1-mini`, `qwen3.5:9b`, `claude-sonnet-4-6`) is treated as an explicit upstream model and forwarded verbatim, so existing routing keeps working.

###### Even easier: the `claw-sidestack-local` wrapper

For local sessions you can skip the `source` step entirely and use the opt-in wrapper at [`scripts/claw-sidestack-local`](scripts/claw-sidestack-local):

```bash
./scripts/claw-sidestack-local --model fast prompt "reply with the word ready"
```

The wrapper:

- sources [`examples/sidestack-local.env`](examples/sidestack-local.env) so every invocation starts from the canonical broker profile, even if the surrounding shell had a stale `OPENAI_BASE_URL`;
- validates the final effective `OPENAI_BASE_URL` against an allowlist of local SideStackAI broker URLs (`http://127.0.0.1:11435` or `http://localhost:11435`, optionally with a path);
- refuses to launch `claw` if `OPENAI_BASE_URL` contains the raw Ollama port `:11434` — this is the wrapper's LAW 1 and trips even if the profile itself is edited to point there;
- prints the active non-secret profile (`OPENAI_BASE_URL`, `RUSTY_CLAUDE_LLM_CALLER`, `RUSTY_CLAUDE_TASK_TYPE`, and the names of any `RUSTY_CLAUDE_MODEL_ALIAS__*` exports) to stderr before exec'ing `claw`;
- does not probe the broker for liveness. Use `claw doctor` or a separate runtime check (for example a small `curl` against the broker's health endpoint) when you need that signal;
- executes the canonical `~/.local/bin/claw` directly (override with `CLAW_CANONICAL_PATH`) instead of resolving `claw` from PATH, so it can never silently fall through to `~/.cargo/bin/claw` or a Cargo target artifact;
- delegates topology and freshness to [`scripts/claw-canonical-status`](scripts/claw-canonical-status) — offline, read-only, no provider call — and refuses to launch when the canonical executable is missing (exit 4), is a symlink or otherwise not a regular executable (exit 5), or is stale relative to the locally known `origin/main` (exit 6). Set `CLAW_SIDESTACK_ALLOW_STALE=1` to override the stale refusal deliberately; when `origin/main` is not locally known the wrapper warns loudly and continues. LAW 1 is evaluated before any of these checks.
- asks the broker whether starting a local coding session is safe, and refuses if it is not — see the next section.

###### Automatic broker readiness preflight (N6)

Stack-Code and the Hyperliquid trading lane are both tier 3 on the broker. Equal priority prevents preemption only while Hyperliquid is *actively inferring*; an idle-but-resident Hyperliquid model can still be evicted by a Stack-Code model swap, and one such swap has already cost a Hyperliquid analyst run. So for every inference-capable invocation the wrapper first calls

```
GET http://127.0.0.1:11435/status/n6_planner_ready?requested_model=<model>
```

and only execs `claw` when that endpoint answers `ready: true`.

- **The readiness endpoint is the authority, not `/status`.** `/status` reports which model holds the slot; it says nothing about whether taking the slot is safe. Only `/status/n6_planner_ready` applies the 300-second Hyperliquid quiet window, the holder policy, and the VRAM arithmetic.
- **`ready: false` means REFUSE, not wait.** The wrapper exits 8 immediately, prints the broker's `reason_code` (for example `HYPERLIQUID_LANE_RECENTLY_ACTIVE`, `HYPERLIQUID_HOLDER_PROTECTED`, `BROKER_BUSY`, `INSUFFICIENT_VRAM`), and never queues, retries, or sleeps. Nothing was loaded and nothing is pending — re-run when the lane is quiet.
- **Anything unusable fails closed with exit 9**: the endpoint unreachable, a timeout, a non-2xx response, malformed JSON, a missing or non-boolean `ready`, duplicate keys, or an answer that does not correspond to the model that was asked about.
- **There is no bypass switch.** No environment variable disables this gate; that is the point of it. `CLAW_SIDESTACK_ALLOW_STALE=1` still overrides only the *staleness* refusal and has no effect on readiness.
- The readiness origin is derived from the already-LAW-1-validated `OPENAI_BASE_URL`, so the query can only ever reach the same allowlisted broker on `:11435`. No credential is sent — the endpoint is unauthenticated and the API key never appears in the request.

**The model must be exact.** The wrapper has to ask about the same upstream model `claw` will actually request, so it resolves the model itself and refuses (exit 9) rather than guessing:

- `--model fast` and `--model=fast` both resolve through `RUSTY_CLAUDE_MODEL_ALIAS__FAST`, so the profile's `qwen3:14b` is what gets queried; `--model deep` likewise queries `qwen3.5:27b`. Alias lookup is case-insensitive, matching the CLI. Resolving the env alias is not on its own enough to admit the result: the wrapper then checks whether a claw config alias redefines that resolved value, and **refuses (exit 9) before any readiness GET** if one does — with `RUSTY_CLAUDE_MODEL_ALIAS__FAST=qwen3:14b` and a config alias mapping `qwen3:14b` to something else, `--model fast` refuses rather than preflight the intermediate `qwen3:14b`. This is fail-closed refusal, not recursive alias emulation: the wrapper never follows the second hop, it declines to guess. A value that survives both checks is proven terminal — non-bare, and not config-aliased — so the CLI's own second alias pass cannot change it.
- An explicit upstream string is forwarded as-is, with a routing prefix stripped exactly as the provider layer strips it: `--model openai/qwen3:14b` queries `qwen3:14b`.
- **No `--model` at all refuses.** Without the flag `claw` falls back to its compiled-in cloud default, so there is no local model to ask about. This includes the bare interactive REPL: use `./scripts/claw-sidestack-local --model fast` to start one.
- A bare name with no `RUSTY_CLAUDE_MODEL_ALIAS__*` export refuses, because it would resolve through a repo config alias or claw's built-in cloud alias table, neither of which is determinable from the wrapper. That covers `--model sonnet` and friends.
- A model string that a `.claw/settings.json` (or `.claw.json`) `aliases` entry redefines also refuses, for the same reason — whether that string was written on the command line or arrived as the result of a `RUSTY_CLAUDE_MODEL_ALIAS__*` lookup.

**Commands that skip the gate.** Only invocations proven not to issue a provider request bypass it: `--help`, `--version`, and the local subcommands `version`, `status`, `sandbox`, `doctor`, `acp`, `state`, `init`, `config`, `diff`, `export`, `system-prompt`, `dump-manifests`, `bootstrap-plan`, `agents`, `mcp`, and `plugins`. Anything not on that list is treated as inference and gated — including a shorthand prompt that merely happens to contain `--help`.

**The tail decides, not the verb.** Several subcommands dispatch locally *or* to a provider depending on what follows them, so classifying on the leading word alone is wrong:

- `skills` mirrors `classify_skills_slash_command`: only `skills`, `skills list`, `skills help`, `skills -h`, `skills --help`, `skills install`, and `skills install <target>` stay local. Every other form — `skills help overview`, `skills list extra`, `skills <skill>`, `skills <skill> <args>` — is a `CliAction::Prompt` and **is gated**. Note the prefix trap: `skills installer` is a skill invocation, not the `install` subcommand.
- `-p` makes the CLI join the entire remaining argv into a prompt, so `-p status` is a *prompt reading "status"*, not the local status report. Any `-p` is gated.
- `--resume` continues a real session, so it is gated regardless of the session reference — `--resume status` resumes a session named `status`.

**Invocations that refuse outright (exit 9).** Two shapes can reach a provider but do not let the wrapper prove *which* upstream model will be used, so it refuses rather than gate on the wrong one:

- **`claw task run <spec>`.** `CliAction::TaskRun` carries only the spec path; the global `--model` is never propagated into it. The bridge reads the spec's `model` field (defaulting to `fast-default`), and the *effective* model is whatever the broker resolves that request to — a value first observable in the broker's response, i.e. after inference has already happened. Gating on `--model` here would query a model the task will never use. Run the task's own lane once the broker is quiet.
- **`claw plan run` whose model-bearing steps are not proven to come back here.** `plan run` itself issues no provider request (its only network operation is a read-only `GET /models` probe); the gate depends entirely on each model-bearing step being spawned back through *this* wrapper as `--model fast … prompt …`, where it is gated on its own. The wrapper refuses (exit 9) whenever it cannot prove that:
    - **With an explicit `--wrapper`,** the effective wrapper must resolve to this wrapper. Repeated `--wrapper` assignments follow the CLI's last-assignment-wins semantics, so only the last one is judged; a trailing valueless `--wrapper` is recorded as an empty assignment and refuses rather than falling back to an earlier value. A foreign effective wrapper refuses. **Which file the effective value names depends on its path shape**, because the runner stores it verbatim as the child's program and applies the workspace-root chdir *before* the exec:
        - **An absolute path** is immune to that chdir, so it is judged as written. This is why an absolute path to this wrapper is the stable escape hatch whenever workspace routing would otherwise be unprovable.
        - **A relative path containing a slash** is resolved by the child *after* the chdir, so it is judged against the execution base of the effective mode — under `--workspace-write-preview` that is the effective workspace root, not the directory you invoked from. `--wrapper scripts/claw-sidestack-local --workspace-root <other-tree>` therefore refuses even when your own current directory holds this very wrapper at that same relative path: the file that would actually run is `<other-tree>/scripts/claw-sidestack-local`. A normal `plan run` cannot be relocated this way (`--workspace-root` is a parse error without `--workspace-write-preview`), so there the base stays the current directory.
        - **A bare name with no slash** is searched for on `PATH` and never in the current directory. This wrapper does not fall back to `PATH`, and nothing lets it prove which entry the child would select, so a slash-less `--wrapper` refuses.
    - **Omitting `--wrapper` is not automatically trusted.** The plan runner's default is the *relative* path `scripts/claw-sidestack-local`, and nothing binds that to the tree this wrapper lives in. The step executor chdirs to the effective workspace root and the spawned child resolves the relative program *after* that chdir, so the executable that actually runs is `<workspace-root>/scripts/claw-sidestack-local`. Running the plan from another tree by absolute path, or pointing `--workspace-root` at a tree with its own `scripts/claw-sidestack-local`, therefore refuses instead of silently handing every step to an ungated executable.
    - **Which bases must be proven depends on the execution mode**, because the two spawn-capable branches of `plan run` do not share a precheck:
        - **A normal `plan run`** resolves its default against the current directory and runs an existence precheck there *before* any spawn; when that path is absent the runner reports substrate-unavailable and spawns nothing. `--workspace-root` is a parse error without `--workspace-write-preview`, so the current directory is the only base that can carry a spawn. A missing default is therefore not a bypass **in this mode**: there is no model-bearing child to gate and the invocation stays local.
        - **`--workspace-write-preview` takes a different branch with no wrapper-existence precheck at all**, and it still spawns every read-only step preceding its lone workspace-write step through `<workspace-root>/scripts/claw-sidestack-local`. A missing current-directory default proves nothing here, so the wrapper judges the effective workspace-root wrapper on its own and refuses when it is foreign *or* unresolvable. To preview into another tree, pass `--wrapper` pointing at this wrapper: an absolute program path is immune to the child's chdir.
    - **`--dry-run` is not gated on a wrapper at all.** The dry-run branch builds its report from the plan validator and precheck only — it never binds a wrapper path, never probes the substrate and never spawns a subprocess, so a foreign `--wrapper` names an executable that cannot run. Combining `--dry-run` with `--workspace-write-preview` is refused by the CLI in either order (both are order-independent booleans checked after the argument loop), so that pairing cannot spawn either. Canonical `claw` still runs locally to perform the dry run. Note that a `--dry-run` occupying another flag's value slot — as in `--fast-model --dry-run` — is that flag's *value*, leaves the run live, and is gated normally.

    In short: run the plan from the tree this wrapper lives in, or pass `--wrapper` pointing at this wrapper.

**What this does not give you.** This is a client-side refusal, not an atomic broker admission reservation. A time-of-check/time-of-use window remains between the readiness GET and the first inference POST `claw` issues, and the broker does not hold anything on your behalf in between. The 300-second Hyperliquid quiet window makes that residual race acceptable under the current operating contract, but it is a narrowed window, not a closed one. A raw POST sent straight to the broker — by any tool that is not this wrapper — bypasses the gate entirely; the protection is in the wrapper, not in the broker.

The readiness preflight needs `curl` and `python3` on `PATH`. Neither is a new requirement: `python3` is already unconditional for the local model-coding workflow this wrapper serves — the runnable task bridge spawns `python3 scripts/invoke_planner_model_via_broker.py` directly, and the A2/N6 planner and validator scripts are all Python — while [`scripts/claw-canonical-refresh`](scripts/claw-canonical-refresh) and [`scripts/claw-canonical-status`](scripts/claw-canonical-status) fall back to `python3` when GNU `realpath`/`sha256sum` are unavailable. If either tool is missing the wrapper fails closed with exit 9. The readiness GET is size-bounded (`--max-filesize 65536`), ignores `~/.curlrc` (`--disable`), and never uses a proxy (`--noproxy '*'`).

**`curl` 8.4.0 or newer is required for inference-capable wrapper calls.** The readiness response is capped at 64 KiB, but `--max-filesize` only *guarantees* that cap from curl 8.4.0 onward. curl's own manual is explicit: "before curl 8.4.0, when the file size is not known prior to download, for such files this option has no effect even if the file transfer ends up being larger than this given limit", and "starting with curl 8.4.0, this option aborts the transfer if it reaches the threshold during transfer". An unknown-length response is exactly the chunked shape an oversized readiness body would arrive in. Older curl still accepts the option, so its presence distinguishes nothing — only the reported version does.

Before it issues the readiness GET, the wrapper therefore asks `curl` for its version and refuses with exit 9 if that version is below 8.4.0, unparseable, or unobtainable. On such a `curl` the contract is **refuse before transfer**, not *transfer with an unreliable bound* — no request is made and canonical `claw` never executes.

These three layers are worth keeping distinct, because the last one is a trust assumption rather than an enforced guarantee.

**What is guaranteed, for a trusted and compliant `curl`.** Readiness-gated inference requires curl >= 8.4.0; an older or unproven-version curl refuses with exit 9 before anything reaches the wire; the readiness body is capped at 64 KiB via `--max-filesize`; and from 8.4.0 that cap is enforced even for an unknown-length transfer already in progress. A hostile `~/.curlrc` is ignored (`--disable`, which curl documents as taking effect only "if used as the first parameter on the command line", and is passed first here), proxy environment variables are neutralised (`--noproxy '*'`), redirects are not followed, and the request is a single bounded GET carrying no credential.

**What path selection buys you.** The wrapper resolves the `curl` command **once** and reuses that same resolved command path for both the version probe and the readiness GET. It does not walk `PATH` a second time, so a `PATH` change made after resolution cannot substitute a different command. That is a guarantee about *which path is selected*, and nothing more — it is **not** executable pinning, and the wrapper makes no claim to have pinned a binary, an inode, or a symlink target.

**The trust boundary, and what remains outside it.** The local `curl` program, and the same-user filesystem and environment containing it, are a **trusted dependency** of this wrapper. That trust is assumed, not verified. It follows that a malicious `curl` can report any version it likes and then ignore `--max-filesize` and return an unbounded body; and that a same-user process — including the probed program itself, if the resolved path is a symlink — can replace or retarget that path between the version probe and the GET, so a different executable serves the request. Both residuals are real and have been demonstrated under controlled test. This client-side wrapper does not defend against same-user tampering with its own dependencies, and such tampering is outside its threat model.

This is the same kind of boundary as the readiness-GET-to-inference-POST race described above: both are *client-side* residuals, not gaps in a broker-side reservation. An attacker who can replace your `curl` can equally POST straight to the broker and bypass this wrapper entirely. The broker does not inherit either weakness when callers use ordinary trusted system dependencies.

This is a prerequisite of the SideStackAI inference wrapper only, not of the repository as a whole. Locally dispatched invocations — `--help`, `--version`, `version`, `status`, `doctor`, `plan run`, and the other non-inference subcommands — never perform a readiness GET, are never version-checked, and keep working on an older `curl` or on a host with no `curl` at all.

###### Two-part enforcement: the startup gate and the in-process gate

Everything above describes the **startup shell gate**, and it can only judge what exists before `exec`:

- it resolves the model from the command line and the profile, refusing (exit 9) rather than guessing when the resolution is not provably terminal;
- it validates the effective `OPENAI_BASE_URL` against the local broker allowlist and refuses `:11434` outright;
- it performs one bounded readiness GET for that resolved model and refuses (exit 8) if the broker declines;
- it applies the same wrapper contract to the subprocesses a `plan run` spawns.

Once the canonical binary is running, none of that is consulted again. A REPL `/model` switch, an Agent primary or fallback selection, a config-supplied primary, and every provider retry all pick a model *after* the wrapper is gone. A startup-only gate says nothing about any of them.

So the wrapper exports one marker immediately before `exec`:

```
CLAW_SIDESTACK_N6_ENFORCE=1
```

It is set by the wrapper and nowhere else, carries no secret, and is deliberately not a reuse of `RUSTY_CLAUDE_LLM_CALLER` — that variable carries broker telemetry semantics and comes from a user-editable env file, so enforcement keyed off it could drift and fail open.

That marker turns on the **in-process gate** inside `claw` itself:

- **Every outbound broker inference HTTP attempt performs its own fresh readiness check**, for the exact model string the request payload will carry. This sits inside the provider retry loop, so retry number four is gated exactly as tightly as the first attempt, however long the backoff was.
- **There is no admission cache.** Not per process, not per model, not time-based. A readiness answer admits exactly the one attempt that follows it; the next attempt asks again. Caching would turn a momentary yes into a standing permission, and the broker issues no lease, epoch, or residency token that a client could legitimately hold.
- The model the broker is asked to admit and the model the payload carries come from a single shared resolver, so a `fast` alias or an `openai/` routing prefix cannot cause the guard to vet one model while another is sent.
- A refusal is **non-retryable and non-fallbackable**. It does not trigger a provider retry, and it does not cause the Agent to try the next model in its fallback chain — each fallback entry that does run gets its own readiness decision immediately before its own attempt.

**LAW 1, in process.** While the marker is active, off-broker model-bearing traffic is refused *before any network call*:

- direct Anthropic inference is refused;
- ordinary cloud OpenAI-compatible endpoints (OpenAI, DashScope, xAI, and anything else reached over a non-broker base URL) are refused;
- **raw `:11434` application inference is refused**, matching the wrapper's startup LAW 1.

Only an OpenAI-compatible provider whose base URL validates as the local broker — plain `http`, host exactly `127.0.0.1` or `localhost`, port explicitly `11435`, no userinfo, query, or fragment, and one of the canonical `/`, `/v1`, `/v1/chat/completions` path shapes — may carry inference at all, and it is then subject to the per-attempt readiness check above.

When the marker is absent, the in-process routing gate is inert and ordinary upstream provider behaviour is unchanged, with no readiness traffic generated. The per-attempt readiness check does **not** depend on the marker, so a canonical `claw` pointed directly at `:11435` without the wrapper is still admission-gated.

**Transport of an admitted request.** Validating the destination and refreshing readiness would both be worth less if the transport could then move the request somewhere else on its own, so the HTTP client that carries an admitted broker inference `POST` bypasses proxies and follows no redirects:

- **Proxies are bypassed.** `HTTP_PROXY` / `HTTPS_PROXY` (either spelling) and a programmatic `proxy_url` are all ignored for this request, and reqwest's own system-proxy detection stays off. You do not have to set `NO_PROXY` for the broker, and setting it is not what provides this.
- **Redirects are not followed.** A `301`, `302`, `303`, `307`, or `308` from the broker origin is surfaced as an ordinary provider error rather than followed. `307` and `308` matter most because they would otherwise re-send the method and the whole model-bearing body to the redirect target. No second destination is contacted, and because the resulting error is not retryable it also cannot fall back to another model.

Together these mean an admitted request is one direct, non-proxied, non-redirecting send to the origin Layer A validated and Layer B admitted — the client cannot voluntarily route it to another HTTP origin after approval.

This applies **only** to a destination that validates as the local broker. Ordinary upstream OpenAI-compatible providers keep their existing proxy and redirect behaviour; see [HTTP proxy support](#http-proxy-support).

**Residual race, restated.** The in-process gate narrows the time-of-check/time-of-use window; it does not close it. Client readiness is still not a broker admission reservation: the readiness `GET` and the inference `POST` remain two separate requests, and the broker's state can change between them. This is the same residual described above, now measured per attempt rather than once per process. Nothing here pins a provider or an executable beyond the trust model already stated, and `http_request` is not affected — it is a general-purpose HTTP tool, not a model-bearing inference path.


###### Editor integration: VS Code task wrapper

A committed `.vscode/tasks.json` exposes the broker-routed wrapper through VS Code's Command Palette (`Tasks: Run Task`). Tasks are read-only by default and delegate every invocation to `scripts/claw-sidestack-local`, so LAW 1 stays centralized in one place. See [`docs/editor-vscode.md`](docs/editor-vscode.md) for the task list, offline validation, and the live-broker validation gate.

### Anthropic-compatible endpoint

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
export ANTHROPIC_AUTH_TOKEN="local-dev-token"

cd rust
cargo run -p rusty-claude-cli -- --model "claude-sonnet-4-6" prompt "reply with the word ready"
```

### OpenAI-compatible endpoint

```bash
export OPENAI_BASE_URL="http://127.0.0.1:8000/v1"
export OPENAI_API_KEY="local-dev-token"

cd rust
cargo run -p rusty-claude-cli -- --model "qwen2.5-coder" prompt "reply with the word ready"
```

### Ollama

```bash
export OPENAI_BASE_URL="http://127.0.0.1:11434/v1"
unset OPENAI_API_KEY

cd rust
cargo run -p rusty-claude-cli -- --model "llama3.2" prompt "summarize this repository in one sentence"
```

### OpenRouter

```bash
export OPENAI_BASE_URL="https://openrouter.ai/api/v1"
export OPENAI_API_KEY="sk-or-v1-..."

cd rust
cargo run -p rusty-claude-cli -- --model "openai/gpt-4.1-mini" prompt "summarize this repository in one sentence"
```

### Alibaba DashScope (Qwen)

For Qwen models via Alibaba's native DashScope API (higher rate limits than OpenRouter):

```bash
export DASHSCOPE_API_KEY="sk-..."

cd rust
cargo run -p rusty-claude-cli -- --model "qwen/qwen-max" prompt "hello"
# or bare:
cargo run -p rusty-claude-cli -- --model "qwen-plus" prompt "hello"
```

Model names starting with `qwen/` or `qwen-` are automatically routed to the DashScope compatible-mode endpoint (`https://dashscope.aliyuncs.com/compatible-mode/v1`). You do **not** need to set `OPENAI_BASE_URL` or unset `ANTHROPIC_API_KEY` — the model prefix wins over the ambient credential sniffer.

Reasoning variants (`qwen-qwq-*`, `qwq-*`, `*-thinking`) automatically strip `temperature`/`top_p`/`frequency_penalty`/`presence_penalty` before the request hits the wire (these params are rejected by reasoning models).

## Supported Providers & Models

`claw` has three built-in provider backends. The provider is selected automatically based on the model name, falling back to whichever credential is present in the environment.

### Provider matrix

| Provider | Protocol | Auth env var(s) | Base URL env var | Default base URL |
|---|---|---|---|---|
| **Anthropic** (direct) | Anthropic Messages API | `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN` | `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` |
| **xAI** | OpenAI-compatible | `XAI_API_KEY` | `XAI_BASE_URL` | `https://api.x.ai/v1` |
| **OpenAI-compatible** | OpenAI Chat Completions | `OPENAI_API_KEY` | `OPENAI_BASE_URL` | `https://api.openai.com/v1` |
| **DashScope** (Alibaba) | OpenAI-compatible | `DASHSCOPE_API_KEY` | `DASHSCOPE_BASE_URL` | `https://dashscope.aliyuncs.com/compatible-mode/v1` |

The OpenAI-compatible backend also serves as the gateway for **OpenRouter**, **Ollama**, and any other service that speaks the OpenAI `/v1/chat/completions` wire format — just point `OPENAI_BASE_URL` at the service.

**Model-name prefix routing:** If a model name starts with `openai/`, `gpt-`, `qwen/`, or `qwen-`, the provider is selected by the prefix regardless of which env vars are set. This prevents accidental misrouting to Anthropic when multiple credentials exist in the environment.

### Tested models and aliases

These are the models registered in the built-in alias table with known token limits:

| Alias | Resolved model name | Provider | Max output tokens | Context window |
|---|---|---|---|---|
| `opus` | `claude-opus-4-6` | Anthropic | 32 000 | 200 000 |
| `sonnet` | `claude-sonnet-4-6` | Anthropic | 64 000 | 200 000 |
| `haiku` | `claude-haiku-4-5-20251213` | Anthropic | 64 000 | 200 000 |
| `grok` / `grok-3` | `grok-3` | xAI | 64 000 | 131 072 |
| `grok-mini` / `grok-3-mini` | `grok-3-mini` | xAI | 64 000 | 131 072 |
| `grok-2` | `grok-2` | xAI | — | — |

Any model name that does not match an alias is passed through verbatim. This is how you use OpenRouter model slugs (`openai/gpt-4.1-mini`), Ollama tags (`llama3.2`), or full Anthropic model IDs (`claude-sonnet-4-20250514`).

### User-defined aliases

You can add custom aliases in any settings file (`~/.claw/settings.json`, `.claw/settings.json`, or `.claw/settings.local.json`):

```json
{
  "aliases": {
    "fast": "claude-haiku-4-5-20251213",
    "smart": "claude-opus-4-6",
    "cheap": "grok-3-mini"
  }
}
```

Local project settings override user-level settings. Aliases resolve through the built-in table, so `"fast": "haiku"` also works.

### How provider detection works

1. If the resolved model name starts with `claude` → Anthropic.
2. If it starts with `grok` → xAI.
3. Otherwise, `claw` checks which credential is set: `ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN` first, then `OPENAI_API_KEY`, then `XAI_API_KEY`.
4. If nothing matches, it defaults to Anthropic.

## FAQ

### What about Codex?

The name "codex" appears in the Claw Code ecosystem but it does **not** refer to OpenAI Codex (the code-generation model). Here is what it means in this project:

- **`oh-my-codex` (OmX)** is the workflow and plugin layer that sits on top of `claw`. It provides planning modes, parallel multi-agent execution, notification routing, and other automation features. See [PHILOSOPHY.md](./PHILOSOPHY.md) and the [oh-my-codex repo](https://github.com/Yeachan-Heo/oh-my-codex).
- **`.codex/` directories** (e.g. `.codex/skills`, `.codex/agents`, `.codex/commands`) are legacy lookup paths that `claw` still scans alongside the primary `.claw/` directories.
- **`CODEX_HOME`** is an optional environment variable that points to a custom root for user-level skill and command lookups.

`claw` does **not** support OpenAI Codex sessions, the Codex CLI, or Codex session import/export. If you need to use OpenAI models (like GPT-4.1), configure the OpenAI-compatible provider as shown above in the [OpenAI-compatible endpoint](#openai-compatible-endpoint) and [OpenRouter](#openrouter) sections.

## HTTP proxy support

`claw` honours the standard `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` environment variables (both upper- and lower-case spellings are accepted) when issuing outbound requests to Anthropic, OpenAI-, and xAI-compatible endpoints. Set them before launching the CLI and the underlying `reqwest` client will be configured automatically.

### Environment variables

```bash
export HTTPS_PROXY="http://proxy.corp.example:3128"
export HTTP_PROXY="http://proxy.corp.example:3128"
export NO_PROXY="localhost,127.0.0.1,.corp.example"

cd rust
cargo run -p rusty-claude-cli -- prompt "hello via the corporate proxy"
```

### Programmatic `proxy_url` config option

As an alternative to per-scheme environment variables, the `ProxyConfig` type exposes a `proxy_url` field that acts as a single catch-all proxy for both HTTP and HTTPS traffic. When `proxy_url` is set it takes precedence over the separate `http_proxy` and `https_proxy` fields.

```rust
use api::{build_http_client_with, ProxyConfig};

// From a single unified URL (config file, CLI flag, etc.)
let config = ProxyConfig::from_proxy_url("http://proxy.corp.example:3128");
let client = build_http_client_with(&config).expect("proxy client");

// Or set the field directly alongside NO_PROXY
let config = ProxyConfig {
    proxy_url: Some("http://proxy.corp.example:3128".to_string()),
    no_proxy: Some("localhost,127.0.0.1".to_string()),
    ..ProxyConfig::default()
};
let client = build_http_client_with(&config).expect("proxy client");
```

### Notes

- When both `HTTPS_PROXY` and `HTTP_PROXY` are set, the secure proxy applies to `https://` URLs and the plain proxy applies to `http://` URLs.
- `proxy_url` is a unified alternative: when set, it applies to both `http://` and `https://` destinations, overriding the per-scheme fields.
- `NO_PROXY` accepts a comma-separated list of host suffixes (for example `.corp.example`) and IP literals.
- Empty values are treated as unset, so leaving `HTTPS_PROXY=""` in your shell will not enable a proxy.
- If a proxy URL cannot be parsed, `claw` falls back to a direct (no-proxy) client so existing workflows keep working; double-check the URL if you expected the request to be tunnelled.
- **Exception — local broker inference.** An inference request whose destination validates as the local SideStack broker never traverses a proxy and never follows a redirect, whatever these variables say. This is a deliberate containment property of the readiness gate, not a bug; see [the in-process gate](#two-part-enforcement-the-startup-gate-and-the-in-process-gate). Every other outbound request, including inference to any non-broker provider, is unaffected.

## Common operational commands

```bash
cd rust
cargo run -p rusty-claude-cli -- status
cargo run -p rusty-claude-cli -- sandbox
cargo run -p rusty-claude-cli -- agents
cargo run -p rusty-claude-cli -- mcp
cargo run -p rusty-claude-cli -- skills
cargo run -p rusty-claude-cli -- system-prompt --cwd .. --date 2026-04-04
```

## Session management

REPL turns are persisted under `.claw/sessions/` in the current workspace.

```bash
cd rust
cargo run -p rusty-claude-cli -- --resume latest
cargo run -p rusty-claude-cli -- --resume latest /status /diff
```

Useful interactive commands include `/help`, `/status`, `/cost`, `/config`, `/session`, `/model`, `/permissions`, and `/export`.

## Config file resolution order

Runtime config is loaded in this order, with later entries overriding earlier ones:

1. `~/.claw.json`
2. `~/.config/claw/settings.json`
3. `<repo>/.claw.json`
4. `<repo>/.claw/settings.json`
5. `<repo>/.claw/settings.local.json`

## Mock parity harness

The workspace includes a deterministic Anthropic-compatible mock service and parity harness.

```bash
cd rust
./scripts/run_mock_parity_harness.sh
```

Manual mock service startup:

```bash
cd rust
cargo run -p mock-anthropic-service -- --bind 127.0.0.1:0
```

## Verification

```bash
cd rust
cargo test --workspace
```

## Workspace overview

Current Rust crates:

- `api`
- `commands`
- `compat-harness`
- `mock-anthropic-service`
- `plugins`
- `runtime`
- `rusty-claude-cli`
- `telemetry`
- `tools`
