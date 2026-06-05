# DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL

⚠️ REVIEW REQUIRED: This is a **future build prompt**, authored 2026-06-05 by the A2-L2b preview
CLI build-scope lane. It has **not** been run. It builds a fresh Stack-Code `rusty-claude-cli`
(`claw`) binary from current `origin/main` so the `claw plan run … --workspace-write-preview`
surface exists — because the installed binary is stale. It must be reviewed and merged, then
invoked only with the exact operator token below, before any build runs. It builds **only**; it
never installs globally and never runs preview/approve/apply.

Addresses: the A2-L2b Preview Command Availability RCA (read-only) that BLOCKED the S2C-1d preview
execution lane. Source of truth: merged S2C-1d execution prompt
`handoffs/s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md` (PR #81) and
`docs/a2-l2b-run-plan-preview-operator-handoff.md`.

> Convention note: Stack-Code keeps build/execution handoff prompts under `handoffs/` (e.g.
> `s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md`). This draft follows that convention.

---

# CLAUDE CODE PROMPT — A2-L2b Preview CLI Build (rusty-claude-cli / claw)

## 1. Status and Approval Requirement

Builds a fresh `claw` (rusty-claude-cli) binary from current `origin/main` and verifies it exposes
the A2-L2b write-preview surface (`claw plan run … --workspace-write-preview`). **Build only** — it
never installs globally, never modifies PATH, and never runs preview/approve/apply.

Do **not** begin unless the operator has provided this **exact** token in the current instruction:

```text
APPROVED: Execute A2-L2b preview CLI build
```

If that exact token is missing, STOP immediately and report:

```text
BLOCKED: missing required approval token.
```

This prompt is DRAFT ONLY until reviewed and merged. Approval is mandatory and never optional.

## 2. Role

You are a careful Stack-Code Rust build operator. Follow:
OBSERVE → VERIFY → BUILD (isolated) → VERIFY SURFACE → REPORT. Build in an isolated worktree; do
not install; do not preview.

## 3. Objective

Produce a freshly-built `claw` binary that contains the write-preview command surface, and report
its **exact path** for a later, separately-approved S2C-1d preview retry. Run no preview here.

## 4. RCA Summary

```text
Installed claw: /home/suki/.local/bin/claw -> /media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw
Type:           Stack-Code rusty-claude-cli ("Claw Code"); debug build dated 2026-05-21 (STALE).
Problem:        strings(installed binary) == 0 for all of: workspace-write-preview, write_preview_ready,
                EXIT_RUN_PLAN_WRITE_PREVIEW_READY, preview-bundle, apply-bundle.
Source (origin/main): rusty-claude-cli/src/main.rs contains the write-preview surface (22 flag refs;
                dispatches `plan run` --workspace-write-preview -> a2_plan_runner::run_plan_with_write_preview),
                and tests/plan_run_write_preview.rs is present. a2-plan-runner implements the preview chain.
Conclusion:     build-required — rebuild rusty-claude-cli from current origin/main.
```

## 5. Source of Truth

```text
rust/crates/rusty-claude-cli/Cargo.toml            ([[bin]] name = "claw")
rust/crates/rusty-claude-cli/src/main.rs           (plan run/approve/apply/preview-bundle/status dispatch; --workspace-write-preview)
rust/crates/rusty-claude-cli/tests/plan_run_write_preview.rs  (write-preview contract test)
rust/crates/a2-plan-runner/src/{runner,write_preview,diff_preview,markers,lib}.rs  (preview chain + EXIT_RUN_PLAN_WRITE_PREVIEW_READY)
docs/a2-l2b-run-plan-preview-operator-handoff.md   (exit codes; artifact layout; exit-7 overload note)
docs/a2-l4-s2c1d-preview-execution-scope.md + handoffs/s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md  (the preview lane this unblocks)
```

## 6. Hard Boundaries

The build lane MUST NOT:

```text
run claw plan run / preview / approve / apply, or A2 apply
run preview of any kind
call a model or the broker (:11435), or reference raw :11434
touch runtime / services / Vault / secrets
install or overwrite the global `claw` (do NOT copy into /home/suki/.local/bin or ~/.cargo/bin)
modify PATH
edit any source/test/schema file (build only; no code changes)
build from a dirty or ambiguous checkout
mutate the ready-to-preview bundle under /tmp/s2c1d_ready_to_preview_*
```

Allowed: build in an isolated worktree from origin/main; inspect the built binary with `--help` /
`--version` / `strings` (read-only); report its exact path. LAW 1: no app inference; raw `:11434`
may appear only as a rejection pattern.

## 7. Clean Worktree Setup

One lane = one worktree = one branch. Create a fresh Stack-Code worktree from `origin/main` under
`/mnt/vast-data/git-worktrees/...`; do not work in `/home/suki/stack-code`. The build writes only to
that worktree's `rust/target/`.

## 8. Build Preflight

Verify ALL before building (else STOP):

```text
the exact approval token (§1) is present
the worktree is clean and based on current origin/main
rust/crates/rusty-claude-cli/Cargo.toml has [[bin]] name = "claw"
rust/crates/rusty-claude-cli/src/main.rs contains "workspace-write-preview" (source surface present)
tests/plan_run_write_preview.rs is present
cargo and a Rust toolchain are available (cargo --version, read-only)
the build target dir is inside the worktree (not the shared build-artifacts dir, unless the operator confirms)
```

## 9. Build Command

Build only the required binary in the isolated worktree:

```text
cargo build -p rusty-claude-cli
```

(If repo discovery shows a more exact package/binary invocation — e.g. an explicit `--bin claw` or a
workspace feature — the prompt may use it, but MUST justify the deviation in the report.) Do not run
`cargo test`/`cargo run`. The built binary is expected at:

```text
<worktree>/rust/target/debug/claw
```

A release build (`cargo build -p rusty-claude-cli --release` → `…/release/claw`) is optional and, if
used, must be reported as the exact path. Do not install either binary.

## 10. Post-Build Surface Verification

The built binary MUST contain all of (via `strings <new-claw> | grep -F …`):

```text
workspace-write-preview
write_preview_ready
EXIT_RUN_PLAN_WRITE_PREVIEW_READY
preview-bundle
apply-bundle
```

And the subcommand surface must be present, verified WITHOUT running a preview:

```text
<new-claw> --version
<new-claw> --help
<new-claw> plan --help            # must show the plan subcommand
<new-claw> plan run --help        # must list --workspace-write-preview (and --workspace-root)
```

Note: only `--help`/`--version` are run — never `plan run` without `--help`, never `--workspace-write-preview`
as an actual run. Confirm the help text lists the flag; do not execute the preview.

## 11. Binary Selection / No Install Boundary

```text
Leave the built binary in the isolated worktree's target dir.
Do NOT install it globally, copy it into /home/suki/.local/bin or ~/.cargo/bin, or modify PATH.
Report the EXACT built binary path; the later S2C-1d preview retry must use that exact path
  (e.g. `<worktree>/rust/target/debug/claw plan run …`), NOT whatever `claw` is on PATH.
```

## 12. No Preview Boundary

```text
This lane builds and verifies surface presence only.
It does NOT run claw plan run / --workspace-write-preview / approve / apply.
It does NOT produce a PreviewRecord or preview_sha256.
Preview execution remains the separate, token-gated S2C-1d lane.
```

## 13. Failure Handling

STOP — escalate, never reframe — and run no preview if:

```text
the exact approval token is absent
the worktree/source checkout is dirty or not on current origin/main
the build fails (capture the error; do not retry blindly or "fix" source to force a build)
`strings` shows any required write-preview symbol still MISSING after build
`<new-claw> plan run --help` does not list --workspace-write-preview (or `plan` is missing)
the built binary appears to route unknown args into a model/agent prompt flow
the generated binary cannot be unambiguously identified by path
any attempt would install globally, modify PATH, call a model/broker, or touch runtime
```

No retries except re-reading already-produced build artifacts / re-running read-only `--help`/`strings`.

## 14. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | PARTIAL | BLOCKED | FAIL
MODE: A2_L2B_PREVIEW_CLI_BUILD
APPROVAL: token present / exact:
BRANCH / WORKTREE / BASE:
BUILD: command / package / profile / exit code / build log:
BUILT_BINARY: exact path / size / mtime:
SURFACE_VERIFICATION (strings): workspace-write-preview / write_preview_ready /
  EXIT_RUN_PLAN_WRITE_PREVIEW_READY / preview-bundle / apply-bundle:
HELP_VERIFICATION: --version / --help / plan --help / plan run --help (lists --workspace-write-preview?):
NO_INSTALL: global claw untouched / PATH unmodified / binary left in worktree target:
NO_PREVIEW: preview run / approve-apply run / PreviewRecord / preview_sha256 / model|broker / runtime:
SAFETY: source edited / dirty-checkout build / global install / PATH modified / live target touched:
STOP GATES HIT: none | details
NEXT BEST LANE:
```

After a successful build with the surface verified, the recommended next lane is:

```text
S2C-1d Preview Execution Retry With Built CLI
```

That lane still requires the exact token `APPROVED: Execute S2C-1d A2-L2b preview execution`, the
corrected ready-to-preview bundle paths
(`/tmp/s2c1d_ready_to_preview_20260605_142019/handoff/plan.yaml` +
`--workspace-root /tmp/s2c1d_ready_to_preview_20260605_142019/workspace`), and it MUST invoke the
**freshly-built binary by exact path**, not the stale `claw` on PATH.

A2-L2b remains the only write authority. This lane only builds the CLI; the model proposes, the
operator approves, A2 applies — none of which happens here.
