# Claw Code

<p align="center">
  <a href="https://github.com/ultraworkers/claw-code">ultraworkers/claw-code</a>
  ·
  <a href="./USAGE.md">Usage</a>
  ·
  <a href="./rust/README.md">Rust workspace</a>
  ·
  <a href="./PARITY.md">Parity</a>
  ·
  <a href="./ROADMAP.md">Roadmap</a>
  ·
  <a href="https://discord.gg/5TUQKqFWd">UltraWorkers Discord</a>
</p>

<p align="center">
  <a href="https://star-history.com/#ultraworkers/claw-code&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=ultraworkers/claw-code&type=Date&theme=dark" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=ultraworkers/claw-code&type=Date" />
      <img alt="Star history for ultraworkers/claw-code" src="https://api.star-history.com/svg?repos=ultraworkers/claw-code&type=Date" width="600" />
    </picture>
  </a>
</p>

<p align="center">
  <img src="assets/claw-hero.jpeg" alt="Claw Code" width="300" />
</p>

Claw Code is the public Rust implementation of the `claw` CLI agent harness.
The canonical implementation lives in [`rust/`](./rust), and the current source of truth for this repository is **ultraworkers/claw-code**.

> [!IMPORTANT]
> Start with [`USAGE.md`](./USAGE.md) for build, auth, CLI, session, and parity-harness workflows. Make `claw doctor` your first health check after building, use [`rust/README.md`](./rust/README.md) for crate-level details, read [`PARITY.md`](./PARITY.md) for the current Rust-port checkpoint, and see [`docs/container.md`](./docs/container.md) for the container-first workflow.
>
> **ACP / Zed status:** `claw-code` does not ship an ACP/Zed daemon entrypoint yet. Run `claw acp` (or `claw --acp`) for the current status instead of guessing from source layout; `claw acp serve` is currently a discoverability alias only, and real ACP support remains tracked separately in `ROADMAP.md`.

## Current repository shape

- **`rust/`** — canonical Rust workspace and the `claw` CLI binary
- **`USAGE.md`** — task-oriented usage guide for the current product surface
- **`PARITY.md`** — Rust-port parity status and migration notes
- **`ROADMAP.md`** — active roadmap and cleanup backlog
- **`PHILOSOPHY.md`** — project intent and system-design framing
- **`src/` + `tests/`** — companion Python/reference workspace and audit helpers; not the primary runtime surface

## Quick start

> [!NOTE]
> [!WARNING]
> **`cargo install claw-code` installs the wrong thing.** The `claw-code` crate on crates.io is a deprecated stub that places `claw-code-deprecated.exe` — not `claw`. Running it only prints `"claw-code has been renamed to agent-code"`. **Do not use `cargo install claw-code`.** Either build from source (this repo) or install the upstream binary:
> ```bash
> cargo install agent-code   # upstream binary — installs 'agent.exe' (Windows) / 'agent' (Unix), NOT 'agent-code'
> ```
> This repo (`ultraworkers/claw-code`) is **build-from-source only** — follow the steps below.

```bash
# 1. Clone and build
git clone https://github.com/ultraworkers/claw-code
cd claw-code/rust
cargo build --workspace

# 2. Set your API key (Anthropic API key — not a Claude subscription)
export ANTHROPIC_API_KEY="sk-ant-..."

# 3. Verify everything is wired correctly
cargo run -p rusty-claude-cli -- doctor

# 4. Run a prompt
cargo run -p rusty-claude-cli -- prompt "say hello"
```

> [!NOTE]
> **Windows (PowerShell):** the binary is `claw.exe`, not `claw`. Run `cargo run -- prompt "say hello"` to skip direct binary path lookup.

### Windows setup

**PowerShell is a supported Windows path.** Use whichever shell works for you. The common onboarding issues on Windows are:

1. **Install Rust first** — download from <https://rustup.rs/> and run the installer. Close and reopen your terminal when it finishes.
2. **Verify Rust is on PATH:**
   ```powershell
   cargo --version
   ```
   If this fails, reopen your terminal or run the PATH setup from the Rust installer output, then retry.
3. **Clone and build** (works in PowerShell, Git Bash, or WSL):
   ```powershell
   git clone https://github.com/ultraworkers/claw-code
   cd claw-code/rust
   cargo build --workspace
   ```
4. **Run** (PowerShell — note `.exe` and backslash):
   ```powershell
   $env:ANTHROPIC_API_KEY = "sk-ant-..."
   cargo run -p rusty-claude-cli -- prompt "say hello"
   ```

**Git Bash / WSL** are optional alternatives, not requirements. If you prefer bash-style paths (`/c/Users/you/...` instead of `C:\Users\you\...`), Git Bash (ships with Git for Windows) works well. In Git Bash, the `MINGW64` prompt is expected and normal — not a broken install.

## Post-build: locate the binary and verify

After running `cargo build --workspace`, the `claw` binary is built but **not** automatically installed to your system. Do not assume Cargo wrote it under `rust/target`; local config, `CARGO_TARGET_DIR`, or `--target-dir` may redirect build artifacts.

### Binary location

After `cargo build --workspace` in `claw-code/rust/`, ask Cargo for the active target directory:

```bash
TARGET_DIR="$(
  cargo metadata --format-version 1 --no-deps |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
```

**Debug build (default, faster compile):**
- **macOS/Linux:** `$TARGET_DIR/debug/claw`
- **Windows:** `$TARGET_DIR/debug/claw.exe`

**Release build (optimized, slower compile):**
- **macOS/Linux:** `$TARGET_DIR/release/claw`
- **Windows:** `$TARGET_DIR/release/claw.exe`

If you ran `cargo build` without `--release`, the binary is in the `debug/` folder.

On Suki's workstation, `rust/.cargo/config.toml` redirects Cargo build artifacts to the 18TB build-artifacts drive. Use `cargo metadata --format-version 1 --no-deps` to confirm `target_directory` instead of assuming a workspace-local target directory.

### Verify the build succeeded

Test the binary directly using its path:

```bash
# macOS/Linux (debug build)
"$TARGET_DIR/debug/claw" --help
"$TARGET_DIR/debug/claw" doctor

# Windows PowerShell (debug build)
cargo run -p rusty-claude-cli -- --help
cargo run -p rusty-claude-cli -- doctor
```

If these commands succeed, the build is working. `claw doctor` is your first health check — it validates your API key, model access, and tool configuration.

### Optional: Add to PATH

If you want to run `claw` from any directory without the full path, choose one of these approaches:

**Option 1: Link the active Cargo-built binary (macOS/Linux)**
```bash
TARGET_DIR="$(
  cd rust &&
  cargo metadata --format-version 1 --no-deps |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
ln -s "$TARGET_DIR/debug/claw" /usr/local/bin/claw
```
Then reload your shell and test:
```bash
claw --help
```

**Option 2: Use `cargo install` (all platforms)**

Build and install to Cargo's default location (`~/.cargo/bin/`, which is usually on PATH):
```bash
# From the claw-code/rust/ directory
cargo install --path . --force

# Then from anywhere
claw --help
```

**Option 3: Update shell profile (bash/zsh)**

Add this line to `~/.bashrc` or `~/.zshrc`:
```bash
TARGET_DIR="$(
  cd /path/to/claw-code/rust &&
  cargo metadata --format-version 1 --no-deps |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
export PATH="$TARGET_DIR/debug:$PATH"
```

Reload your shell:
```bash
source ~/.bashrc  # or source ~/.zshrc
claw --help
```

### Troubleshooting

- **"command not found: claw"** — Ask Cargo for `target_directory`, then use `$TARGET_DIR/debug/claw` or link/install as above.
- **"permission denied"** — On macOS/Linux, you may need `chmod +x "$TARGET_DIR/debug/claw"` if the executable bit isn't set (rare).
- **Debug vs. release** — If the build is slow, you're in debug mode (default). Add `--release` to `cargo build` for faster runtime, but the build itself will take 5–10 minutes.

> [!NOTE]
> **Auth:** claw requires an **API key** (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.) — Claude subscription login is not a supported auth path.

Run the workspace test suite after verifying the binary works:

```bash
cd rust
cargo test --workspace
```

## Documentation map

- [`USAGE.md`](./USAGE.md) — quick commands, auth, sessions, config, parity harness
- [`rust/README.md`](./rust/README.md) — crate map, CLI surface, features, workspace layout
- [`PARITY.md`](./PARITY.md) — parity status for the Rust port
- [`rust/MOCK_PARITY_HARNESS.md`](./rust/MOCK_PARITY_HARNESS.md) — deterministic mock-service harness details
- [`ROADMAP.md`](./ROADMAP.md) — active roadmap and open cleanup work
- [`PHILOSOPHY.md`](./PHILOSOPHY.md) — why the project exists and how it is operated
- [`docs/editor-vscode.md`](./docs/editor-vscode.md) — read-only VS Code task wrapper (Command Palette entry points; no extension required)
- [`docs/a2-plan-schema.md`](./docs/a2-plan-schema.md) — A2 plan YAML schema (L1a/L2a offline validator surface)
- [`docs/a2-l2b-run-plan-preview-operator-handoff.md`](./docs/a2-l2b-run-plan-preview-operator-handoff.md) — runtime-proven A2-L2b `claw plan run --workspace-write-preview` → `approve` → `apply-bundle` → `apply` operator flow. Documents the gated operator chain only; does not authorize autonomous workspace-write execution.
- [`docs/a2-l2c-operator-quickref.md`](./docs/a2-l2c-operator-quickref.md) — A2-L2c operator quick reference: copy-pasteable A2-L2b chain, exit-code `7` disambiguation, TTY approval EOF note, and per-step artifact map. Docs-only; does not authorize autonomous workspace-write execution.
- [`docs/a2-l2d-status-schema.md`](./docs/a2-l2d-status-schema.md) — A2-L2d `a2-l2d-status.v1` schema-of-record for the read-only `claw plan status <workspace> [<approval-result.json>]` command. Read-only by construction; does not authorize autonomous workspace-write execution, approval bypass, or IDE write controls.
- [`docs/a2-l2d-operator-quickref.md`](./docs/a2-l2d-operator-quickref.md) — A2-L2d operator quick reference: copy-pasteable `claw plan status` usage, phase meanings, STOP-condition handling, and the optional `<approval-result.json>` read. Docs-only; does not authorize autonomous workspace-write execution, approval bypass, or IDE write controls.
- [`docs/a2-l3-harness-adapter-usage.md`](./docs/a2-l3-harness-adapter-usage.md) — A2-L3 harness adapter usage guide: purpose, what the adapter consumes, disposable-workspace AND-semantics classifier, STOP-signal taxonomy, CI consumption pattern, and explicit non-authorisations for the merged read-only crate at `rust/crates/a2-harness-adapter/`. Docs-only; does not authorize autonomous workspace-write execution, approval bypass, or IDE write controls.
- [`docs/a2-l3-ide-adapter-usage.md`](./docs/a2-l3-ide-adapter-usage.md) — A2-L3 IDE adapter usage guide: purpose, what the panel reads and does not read, installation, refresh/status fields, STOP-condition handling, evidence-path and copy-to-clipboard rules, and the security/safety model for the merged read-only VS Code Claw Status Panel at `ide/vscode/claw-status-panel/`. Docs-only; does not authorize autonomous workspace-write execution, approval bypass, or IDE write controls.
- [`docs/a2-l4-local-model-coding-loop-scope-card.md`](./docs/a2-l4-local-model-coding-loop-scope-card.md) — A2-L4 design-only scope card for a local-model coding loop: the local model as an advisory proposer (read/summarize/propose-plan/propose-patch/request-tests/request-preview/explain-risk), the broker `:11435`-only routing boundary with the raw `:11434` app-inference prohibition, the VRAM safety boundary, the inert patch-proposal contract, the optional ChatGPT/Claude external-verifier role, and the implementation slices. Design-only; does not authorize implementation, model execution, direct writes, approval bypass, or raw `localhost:11434` app inference. The A2-L2b preview/approve/apply chain remains the only write authority.
- [`docs/a2-l4-s2-readonly-local-model-task-planner-scope-card.md`](./docs/a2-l4-s2-readonly-local-model-task-planner-scope-card.md) — A2-L4-S2 design-only scope card for the read-only local-model task planner (the parent card's L4-S2 slice): the planner as an advisory, read-only, non-mutating role; allowed inputs (task text, workspace root, optional hints) and inert outputs (task_summary, candidate_files, plan_steps, risk_notes, test_suggestions, patch_intent, preview_request, external_verifier_handoff); the broker `:11435`-only routing boundary with the raw `:11434` app-inference prohibition; the VRAM safety boundary; the planner/patch, planner/preview, and planner/verifier boundaries; the conceptual task-plan contract (no schema file created); and the S2A–S2F sub-slices. Design-only; does not authorize implementation, model execution, direct writes, approval bypass, model-generated approval lines, or raw `localhost:11434` app inference. The A2-L2b preview/approve/apply chain remains the only write authority.
- [`docs/a2-l4-s2a-planner-output-contract-scope-card.md`](./docs/a2-l4-s2a-planner-output-contract-scope-card.md) — A2-L4-S2A design-only scope card for the planner output contract (the S2 card's S2A sub-slice): the conceptual, inert task-plan object shape; required fields (schema_version, task_id, workspace_root, task_summary, plan_steps, risk_notes, operator_next_steps), optional fields (repo_context_summary, candidate_files, test_suggestions, patch_intent, preview_request, external_verifier_handoff, status_snapshot), and forbidden fields (approval_line, apply_command, run_command, shell_command, autonomous_apply, auto_approve, raw_11434_endpoint, secret_value, token_value, private_key, …); per-field semantics; the patch-intent, preview-request, test-suggestion, and external-verifier boundaries; the inertness and operator-routability requirements; and the S2A-1–S2A-5 sub-slices. Design-only; does not authorize implementation, schema creation, model execution, direct writes, approval bypass, model-generated approval lines, or raw `localhost:11434` app inference. The A2-L2b preview/approve/apply chain remains the only write authority.

## Ecosystem

Claw Code is built in the open alongside the broader UltraWorkers toolchain:

- [clawhip](https://github.com/Yeachan-Heo/clawhip)
- [oh-my-openagent](https://github.com/code-yeongyu/oh-my-openagent)
- [oh-my-claudecode](https://github.com/Yeachan-Heo/oh-my-claudecode)
- [oh-my-codex](https://github.com/Yeachan-Heo/oh-my-codex)
- [UltraWorkers Discord](https://discord.gg/5TUQKqFWd)

## Ownership / affiliation disclaimer

- This repository does **not** claim ownership of the original Claude Code source material.
- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
