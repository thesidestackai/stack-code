# A2 Tier 3 Write-Capable Orchestrator v0 — Implementation Report (2026-06-09)

## What was implemented

An EXTERNAL, operator-invoked orchestrator at the separately-approved path:

```text
scripts/a2-tier3-write-orchestrator.sh
```

It DRIVES the existing, tested A2-L2b write chain (`claw plan run/approve/apply-bundle/apply`, backed
by the Rust crate `a2-plan-runner` `write_executor` + `checkpoint`). It re-implements no file writing
and adds no new write/checkpoint/rollback logic. The only genuinely-new work is the disposable-worktree
lane around that chain.

Two subcommands:

- `validate-lane` — pure gate check (no git, no claw, no worktree, no writes). Safe anywhere.
- `apply-lane` — runs validate-lane, then (only at a real interactive terminal, clean control checkout,
  origin/main, free worktree path) creates exactly one disposable worktree and drives the existing
  chain inside it, prints checkpoint/diff evidence, and STOPS for operator review.

## Approved script path

`scripts/a2-tier3-write-orchestrator.sh` — approved by the operator token
"APPROVED: Use external orchestrator path: scripts/a2-tier3-write-orchestrator.sh".

## Why it wraps the existing write_executor instead of duplicating it

Per the merged reconciliation (`docs/a2-tier3-write-executor-reconciliation.md`, PR #113), a proven,
hardened write surface already exists in `rust/crates/a2-plan-runner/src/write_executor.rs` (~1725
lines) + `checkpoint.rs` (~864 lines): full authority chain (resolved target + checkpoint + preview +
approval + payload, hash-bound), atomic temp+rename write, post-write re-hash, and bounded rollback.
`claw plan apply` is the one and only command that writes a target. Building a parallel executor would
duplicate and risk diverging from that hardened surface. The orchestrator therefore shells out to the
existing `claw` chain (via `A2_CLAW`) and contributes only the worktree lifecycle + per-lane gating.

## Gates enforced

```text
- operator-approved lane required (operatorApproved == true)
- dry-run-ready evidence required AND re-validated (evidence.ready == true; worktreePath consistency)
- exact-path scope: every write must be in the declared set, inside the disposable worktree, never
  under the control checkout (deny-by-default; mirror of mutationScope.classifyWrite)
- denials win over the Tier-3 allowlist for any lane-declared validation command
  (mirror of safeMutationPolicy + deniedCommands)
- worktree-plan rules: base origin/main; under /mnt/vast-data/git-worktrees/; never the control
  checkout; branch != main/master and whitespace-free (mirror of disposableWorktreePlan)
- plan after_file targets must be workspace-relative AND inside the declared set
- real interactive TTY required for apply-lane (exit 7 off-TTY) so approval stays human-typed
- clean control checkout required before creating a disposable worktree
- origin/main fetched; base is origin/main
- unique/free branch and worktree path
- exactly one disposable worktree created
- STOP for operator review after apply; rollback-by-abandon
```

## What it refuses

```text
- a lane that is not operator-approved, or whose dry-run evidence is not ready
- a write outside the declared set / under the control checkout / escaping the worktree via traversal
- a lane-declared command that matches a denied family (denials win) or is not on the Tier-3 allowlist
- a worktree plan not based on origin/main, not under the disposable root, or that is the control checkout
- a mutation branch of main/master
- a plan after_file that is absolute or not in the declared set
- running apply-lane in a non-interactive context (TTY guard, exit 7) — no worktree, no writes
- a dirty control checkout; an already-existing branch or worktree path
- (by construction) push / PR / merge / branch deletion / force worktree removal — none are implemented
- (by construction) model / broker / runtime / network / Vault / raw app inference — none are implemented
```

## Validation run

```text
- bash -n scripts/a2-tier3-write-orchestrator.sh                      -> SYNTAX_OK
- bash -n tests/shell/test_a2_tier3_write_orchestrator.sh             -> SYNTAX_OK
- shellcheck -S warning (both files)                                  -> CLEAN
- bash tests/shell/test_a2_tier3_write_orchestrator.sh                -> 16 passed, 0 failed
```

The gate-matrix test covers: a good lane (accept); operator-approval, dry-run-ready, empty-declared,
out-of-scope write, control-checkout write, traversal-escape write, denied command (denials win),
non-allowlisted command, non-origin/main base, control-checkout worktree, main branch, absolute plan
target, and out-of-declared plan target (all refuse with exit 4); and `apply-lane` off-TTY refusal
(exit 7) WITH an explicit assertion that NO disposable worktree was created.

## Whether any disposable test fixture was created

Yes — only ephemeral JSON/plan fixtures under a `mktemp -d` temp dir, removed on exit. No `.claw`
artifacts, no disposable git worktree, and no target file were created by the tests. The `apply-lane`
case ran in a non-interactive context and refused at the TTY gate before any worktree/claw step.

## Whether any real write happened

No. No `claw plan apply` was executed, no target was written, no disposable worktree was created, the
control checkout was never written, and no runtime/model/broker/Vault/network call was made. The live
write path was exercised by neither implementation nor tests.

## What remains unproven

```text
- The LIVE drive path (apply-lane at a real terminal actually invoking claw run/approve/apply-bundle/
  apply against a built `claw` binary) is NOT exercised here. It is gated behind a real TTY + operator
  approval and must be proven by the operator in a controlled disposable-worktree smoke, against a built
  claw (A2_CLAW). The exact .claw artifact filenames/locations the driver greps for
  (preview-bundle.json, preview-generator-result.json, approval-result.json, apply-bundle.json,
  apply-result.json) are taken from the existing helper a2-ide-harness.sh and the merged scope, not from
  a live run in this lane.
- CI does not run the orchestrator or its test: scripts/a2-tier3-write-orchestrator.sh and
  tests/shell/test_a2_tier3_write_orchestrator.sh are outside rust-ci.yml's path filter (it lists
  scripts/claw-sidestack-local + tests/shell/test_claw_sidestack_local.sh by name, not scripts/** or a
  glob). Wiring them into CI is a separate, explicitly-approved change to .github/ (not done here).
```

## Safety summary

```text
control checkout written            : no
real/live target written            : no
new write executor created          : no (drives existing claw plan apply / a2-plan-runner chain)
a2-plan-runner Rust modified        : no
panel modified / helperRunner edited : no (panel stays read-only; not spawnable by the panel)
live A2 outside disposable worktree : no (no live A2 run at all this lane)
model / broker / runtime / :11434   : no
Vault / secret read / network egress : no
push / PR / merge / branch-delete / force-remove : no
install-smoke 448d7ea touched       : no
```
