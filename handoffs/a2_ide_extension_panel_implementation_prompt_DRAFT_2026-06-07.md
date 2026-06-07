# DRAFT Implementation Prompt — A2 IDE Extension Panel (v1) — 2026-06-07

> DRAFT. This is a future implementation prompt, not authorization to build. It is gated by an exact
> approval token (below). Do not act on it until the scope card
> (docs/a2-l4-ide-extension-panel-scope.md) and this prompt are reviewed and merged, and the operator
> supplies the token on a standalone line.

---

## Approval Token (required, exact)

Implementation MUST NOT begin unless the operator provides this exact line, standalone:

```text
APPROVED: Execute A2 IDE extension panel implementation
```

If the token is absent, STOP and do nothing but report that the gate is unsatisfied.

---

## Role

You are operating as a careful Stack-Code IDE/extension implementer. Follow:

```text
OBSERVE → VERIFY → DISCOVER → DESIGN-CHECK → IMPLEMENT (smallest safe) → VALIDATE → COMMIT → REPORT
```

Build the **smallest safe** VS Code / Cursor extension panel that wraps the EXISTING, merged,
print/validate-only A2 IDE harness v0 (`scripts/a2-ide-harness.sh`) in a visual, button-driven surface.
The panel is an observer + command-generator. It must NOT weaken any A2 safety gate.

---

## Objective

Implement v1 Option A from the scope card: a separate VS Code / Cursor extension package (sibling of
`ide/vscode/claw-status-panel/`) whose buttons map to the helper's read-only / print subcommands and
whose only spawned binary is the helper itself. Buttons generate/copy commands; they do not execute the
chain. No dangerous execution buttons. Approval stays real-terminal and human-typed; apply stays
explicit and operator-run.

---

## Source of Truth (read first, read-only)

```text
docs/a2-l4-ide-extension-panel-scope.md                 (this lane's scope card — authoritative)
scripts/a2-ide-harness.sh                               (the v0 helper the panel drives)
.vscode/tasks.json                                      (existing A2 tasks; button parity reference)
docs/runbooks/a2-ide-harness-workflow.md                (operator runbook)
docs/a2-l4-ide-harness-workflow-scope.md                (v0 workflow scope)
docs/a2-l4-ide-harness-v0-ux-polish-scope.md            (artifact/hash detection rationale)
ide/vscode/claw-status-panel/                           (scaffolding to re-derive: subprocess, guards)
ide/vscode/claw-status-panel/src/subprocess.ts          (argv-bounded spawn pattern)
ide/vscode/claw-status-panel/scripts/run-guards.js      (static-grep guard pattern)
docs/a2-l3-ide-adapter-scope-card.md                    (forbidden-affordance doctrine to honor)
docs/a2-l3-adapter-boundary-scope-card.md               (read-only observer boundary)
```

---

## Hard Boundaries (do NOT)

```text
- execute any A2 chain command: claw plan run / approve / apply-bundle / apply
- run preview, run approval, run apply-bundle, run apply
- spawn `claw` from the panel at all (the panel spawns ONLY scripts/a2-ide-harness.sh)
- add a "Run Preview/Approval/Apply-Bundle/Apply" execution button (v1 has none)
- compose the approval line `apply <step-id> <preview_sha256>` in panel source
- capture an approval line from a webview input and forward it anywhere (a webview is not a TTY)
- introduce auto-approval, hidden apply, batch, --yes, fake-TTY, preapproval, or "approve when X"
- call a model / broker / runtime / Ollama / raw :11434 / Vault / secrets / telemetry / analytics
- read .claw/** directly from the panel (read only through the helper's read-only subcommands)
- write any file under .claw, the workspace, or the target tree (prefer clipboard / unsaved doc export)
- watch the filesystem, poll, or auto-refresh (every helper call is one explicit operator gesture)
- edit Rust, schemas, runtime, services, HQ, systemd units, or the broker
- edit or relax the existing ide/vscode/claw-status-panel/ package or its guards
- install/modify VS Code or Cursor extensions or user IDE settings
- push, open a PR, or merge
- run destructive commands: git clean, rm -rf, find ... -delete, find ... -exec rm, git reset --hard,
  git add . , git add -A , git branch -D , git worktree remove --force, git fetch --prune
```

## Allowed

```text
- read repository files
- create a NEW sibling extension package under ide/vscode/ (confirmed by discovery)
- write extension code, manifest, tests, and guards for that NEW package only
- run the package's OFFLINE checks: tsc build, static guards, mocha tests against FIXTURES
- run the helper's read-only subcommands against a DISPOSABLE fixture workspace for manual sanity only
  (never a live preview/approval/apply; never against /home/suki/stack-code or /home/suki/sidestackai)
- commit locally on the lane branch
```

---

## Phase 0 — Preflight

```bash
set -euo pipefail
STACK_SOURCE=/home/suki/stack-code
# fresh isolated worktree from origin/main (a NEW dated branch/worktree for the implementation lane)
git -C "$STACK_SOURCE" status -sb
git -C "$STACK_SOURCE" diff --cached --stat
git -C "$STACK_SOURCE" diff --stat
test -z "$(git -C "$STACK_SOURCE" diff --cached --name-only)" || { echo "STOP: staged changes"; exit 1; }
test -z "$(git -C "$STACK_SOURCE" diff --name-only)"        || { echo "STOP: unstaged changes"; exit 1; }
git -C "$STACK_SOURCE" fetch origin main
# verify the scope card + helper exist on origin/main before building
for f in docs/a2-l4-ide-extension-panel-scope.md scripts/a2-ide-harness.sh \
         ide/vscode/claw-status-panel/src/subprocess.ts \
         ide/vscode/claw-status-panel/scripts/run-guards.js ; do
  git -C "$STACK_SOURCE" show "origin/main:$f" >/dev/null || { echo "STOP: missing $f on origin/main"; exit 1; }
done
```

Then create a fresh worktree/branch from `origin/main` (do NOT edit `/home/suki/stack-code`):

```text
branch:   feat/a2-ide-extension-panel-<YYYYMMDD>
worktree: /mnt/vast-data/git-worktrees/stack-code-a2-ide-extension-panel-<YYYYMMDD>
```

---

## Phase 1 — Discovery (DISCOVER the surface; do not assume it)

```text
1. Confirm the panel package home convention. Inspect ide/vscode/ for the existing package layout.
   - If ide/vscode/claw-status-panel/ is present (expected), a NEW sibling package
     ide/vscode/a2-harness-panel/ is the minimal, convention-matching home.
   - STOP and report if no such convention exists — do NOT invent a broad new extension framework or a
     monorepo of tooling.
2. Re-derive (do NOT import or depend on) the claw-status-panel patterns:
   - argv-bounded subprocess wrapper (src/subprocess.ts) with SubprocessRefusal-style rejections
   - static-grep guards (scripts/run-guards.js) and the npm run lint/compile/test scripts
   - one-command-per-gesture contributes model (package.json)
   - mocha test layout + golden fixtures
3. Enumerate the v0 helper's read-only subcommands (the button → subcommand map) from
   scripts/a2-ide-harness.sh. Confirm each is print/validate-only and writes no target.
4. Confirm the approval grammar string is produced ONLY by the helper (print-approval), never composed
   in panel source.
```

Answer in build notes before writing code: which package directory; which subcommand each button calls;
which guards to keep identical vs add; how evidence export avoids any filesystem write.

---

## Phase 2 — Implement (smallest safe panel first)

```text
- Create ONLY the new sibling package files (manifest, tsconfig, src/, scripts/run-guards.js, test/).
- Buttons/sections per scope §8–§9: Workspace/Plan selection, Validate Input, Generate Preview Command,
  Find Artifacts, Audit Workspace, Show Preview State, Print Approval Command, Show Approval Result,
  Generate Apply-Bundle Command, Print Apply Command, Verify Final Target, Open Runbook,
  Export Evidence Summary, and an always-visible Safety / Stop Gates section.
- Subprocess wrapper: allowlist exactly { help, validate-input, print-preview, find-artifacts,
  print-approval, print-apply-bundle, print-apply, verify-final, audit-workspace }; spawn ONLY
  scripts/a2-ide-harness.sh; reject flag-shaped/write-shaped args; injectable spawn impl for tests.
- Clipboard: copy a single verbatim helper-produced command string; no composition, no decoration.
- Guards (run-guards.js): keep FORBIDDEN-NETWORK/-WATCHER/-POLLING/-WRITE/-SECRET-API; add
  FORBIDDEN-CHAIN-WRITE-SPAWN (panel must not spawn claw plan run/approve/apply-bundle/apply),
  FORBIDDEN-APPROVAL-COMPOSE (no `apply ${step} ${sha}`), and ONLY-HELPER-SPAWN (single allowlisted
  binary). The helper's chain-write command STRINGS appearing in rendered stdout are NOT spawns and are
  not a violation; the guard targets panel-source spawns/composition only.
- Do NOT add any execution path for a chain-write step. No dangerous buttons.
- Avoid a broad rewrite: build only what the smallest safe panel needs.
```

---

## Phase 3 — Validate (offline; fixtures only; no live A2)

```text
- npm run compile (tsc) — clean
- npm run lint (run-guards.js) — PASS (no forbidden network/watcher/polling/write/secret/chain-write-
  spawn/approval-compose; only-helper-spawn holds)
- npm test (mocha against FIXTURE helper stdout) — green:
    * each button maps to exactly one read-only subcommand (or UI-only action)
    * subprocess argv audit: only scripts/a2-ide-harness.sh + an allowlisted subcommand is ever built;
      `claw`/chain-write spawn is impossible by construction
    * zero filesystem writes under any input
    * zero network egress
    * no chain command executed by the panel
    * STOP / Safety section always rendered, never collapsed
- bash -n scripts/a2-ide-harness.sh still OK (helper unchanged)
- existing claw-status-panel tests + all A2-L2b/L2d/L3 + v0 harness tests still pass unchanged
- git status: only the new package files changed; git diff --check clean
- NO live preview/approval/apply ran; target unchanged; repo clean
```

---

## Phase 4 — Commit Locally

```text
- Exact-path stage ONLY the new package files.
- Commit message: feat(a2): add IDE extension panel (v1, observer + command-generator)
- Do NOT push, do NOT open a PR, do NOT merge.
```

---

## Phase 5 — Report

Return a closeout report covering: approval token satisfied; package created; button→subcommand map;
guard results; safety attestation (no preview/approval/apply-bundle/apply run; no model/broker/runtime
call; no chain-write spawn; no approval compose; no filesystem write; no .claw direct read; target
unchanged; repo clean); validation results; STOP gates hit (if any); and the recommended next lane
(panel review / push PR — do not auto-push).

---

## Safety Recap (the panel must hold ALL of these)

```text
- apply-bundle is the generator; `plan apply` is the only command that writes the target, run once.
- No auto-approval. No hidden apply. Approval requires human review at a real terminal and the exact
  `apply <step-id> <preview_sha256>` phrase typed there.
- The panel spawns ONLY the helper, generates/copies commands as text, executes no chain command, makes
  no model/broker/runtime/:11434 call, writes no file, reads no secrets, and never auto-refreshes.
- STOP before scaffolding if the package convention is not confirmed in discovery.
```
