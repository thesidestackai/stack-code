# DRAFT — Implementation Prompt — A2 IDE Extension Panel Workspace-First UX — 2026-06-08

> **DRAFT, token-gated, not yet authorized.** This is the future implementation lane's prompt. It must
> NOT be executed until the operator supplies the exact approval token in §1. The scope source of truth
> is [`docs/a2-l4-ide-extension-panel-workspace-first-ux-scope.md`](../docs/a2-l4-ide-extension-panel-workspace-first-ux-scope.md).

---

## 1. Required Operator Approval

This lane is **STOPPED** until the operator's message contains, verbatim, the exact token:

```
APPROVED: Execute A2 IDE extension panel workspace-first UX implementation
```

Without that exact token, do **not** create a worktree, do **not** edit any file, do **not** run any
command. Stop and ask for the token.

---

## 2. Role

You are a careful Stack-Code implementation engineer. You implement the **workspace-first UX** layer of
the A2 IDE Extension Panel as scoped, and nothing more. You keep the panel **print/validate-only**. You
follow: OBSERVE → VERIFY → DISCOVER → IMPLEMENT (TDD) → VALIDATE → COMMIT → REPORT.

---

## 3. Objective

Make the A2 IDE Extension Panel **workspace-first**: open a workspace, and the panel immediately shows
**setup status**, discovered **plan** / **artifact** candidates, and the **next safe step** — without the
operator manually typing paths — while preserving every safety gate. Deliver:

- a setup-status section (helper path, claw binary, workspace root, plan/artifacts/target/after_sha),
- safe read-only **artifact discovery** with auto-population (the deferred "Option B"),
- a read-only **next-step state machine** (state → one next safe step),
- a read-only **visual diff / preview readiness** view over existing artifacts,
- an **evidence timeline** extension of the unsaved evidence summary,
- updated runbook + a final implementation report.

Non-objectives: no live apply, no auto-approval, no hidden apply, no model/broker/runtime call, no
`Run-*` button, no `.vsix` packaging.

---

## 4. Source of Truth

- Scope card: `docs/a2-l4-ide-extension-panel-workspace-first-ux-scope.md` (this lane builds to it).
- Operator-ready handoff: `handoffs/a2_ide_extension_panel_v1_operator_ready_handoff_2026-06-08.md`.
- Runbook: `docs/runbooks/a2-ide-extension-panel.md`.
- Package: `ide/vscode/a2-harness-panel/`.
- Helper (read-only/print; the only spawned binary): `scripts/a2-ide-harness.sh`.

Build on the helper's existing read-only machinery (`detect_chain_state`, `print_next_step_hint`,
`find-artifacts`, `audit-workspace`, `verify-final`). Do **not** re-derive chain semantics in the
extension; the panel **presents** chain state and computes only one-shot read-only setup/existence checks.

---

## 5. Hard Boundaries

Do NOT:

- run `claw plan run` / `approve` / `apply-bundle` / `apply`; run any live preview / approval / apply.
- add any `Run-*` / live-apply button or code path.
- spawn `claw` or any binary other than the helper (`a2-ide-harness.sh`, array-argv, no shell).
- call a model, a broker, a runtime, `/v1/chat/completions`, `/status/vram`, or raw `:11434`.
- read, print, or route a secret; touch Vault.
- capture or compose an approval phrase from the webview.
- write a `.claw` artifact, write/mutate a target file, or write any file other than the unsaved evidence
  document.
- add a filesystem watcher, polling, or background auto-refresh (detection is one-shot: open + Refresh).
- touch `claw-status-panel`, Rust code, schemas, services, or runtime.
- run destructive commands: `git clean`, `rm -rf`, `find … -delete`, `find … -exec rm`,
  `git reset --hard`, `git add .`, `git add -A`, `git branch -D`, `git worktree remove --force`,
  `git fetch --prune`.
- push, open a PR, or merge (unless a separate, explicit approval says so).

---

## 6. Clean Worktree Setup

Use a fresh isolated worktree from `origin/main`. Do not edit `/home/suki/stack-code`.

```text
STACK_SOURCE=/home/suki/stack-code            # control checkout, read-only
BRANCH=feat/a2-panel-workspace-first-ux-<date>
WT=/mnt/vast-data/git-worktrees/stack-code-a2-panel-workspace-first-ux-<date>
```

Preflight (read-only): control checkout clean (no staged/unstaged tracked changes), fetch `origin/main`,
confirm panel package + runbook + scope card present on `origin/main`, confirm branch/worktree paths are
free. STOP on any unexpected tracked or staged change. Snapshot untracked state with
`scripts/safe_untracked_snapshot.sh` before any move/restore. Then
`git -C "$STACK_SOURCE" worktree add -b "$BRANCH" "$WT" origin/main`.

---

## 7. Discovery

Read-only first. Do not execute the extension against a live A2 chain. Read:

- the scope card (every section is a requirement),
- `ide/vscode/a2-harness-panel/src/*` (`extension.ts` session/inputs, `render.ts` pure render,
  `buttons.ts` catalog, `helperRunner.ts` spawn boundary) and `test/*`,
- the helper's read-only subcommands (`find-artifacts`, `audit-workspace`, `verify-final`) and its
  `detect_chain_state` / `print_next_step_hint` output shape, so the discovery parser matches real output.

Map exactly which existing modules change and which new pure modules are added before writing code.

---

## 8. Implementation Scope

Prefer **read-only state detection first**, then presentation. Add new logic as pure, testable modules:

1. **Setup-status detector** — one-shot read-only checks → the status model in scope §7. Helper path
   (stat; basename `a2-ide-harness.sh`), claw binary presence (read-only PATH/stat, never spawned),
   workspace root, plan/artifacts/target/after_sha known/unknown/not-checked.
2. **Artifact discovery (Option B)** — parse the helper's read-only `find-artifacts` / `audit-workspace`
   output (and one-shot stat where needed) to propose paths. Exactly-one → auto-select; zero/many →
   "select needed". Always show discovered paths before use; never silently infer; never write.
3. **Next-step state machine** — pure mapping from setup status + helper-reported chain state to exactly
   one next safe step (scope §9). Add a guard test: it can never recommend a `Run-*` / chain-write action.
4. **Visual diff / preview readiness** — read-only render of existing preview/generator artifacts
   (after_file, before/after sha, `preview_sha256`, `step_id`); read-only diff only if contents already
   present; never regenerate a preview.
5. **Evidence timeline** — extend the unsaved evidence summary with an ordered, session-local timeline of
   safe actions + exit codes; still writes no file.
6. **Render + runbook** — surface setup status + next safe step in the panel; keep the Safety / Stop Gates
   banner always-on; update `docs/runbooks/a2-ide-extension-panel.md`.

Keep `helperRunner.ts` shape intact: array-argv, no shell, helper-basename-bounded, read-only/print
subcommands only.

---

## 9. State Model

Implement the read-only state machine from scope §9 exactly:

```text
NO_WORKSPACE → WORKSPACE_SELECTED → PLAN_SELECTED → INPUT_VALIDATED →
NO_PREVIEW_ARTIFACTS / PREVIEW_READY → APPROVAL_RESULT_MISSING / APPROVAL_RESULT_FOUND →
APPLY_BUNDLE_MISSING / APPLY_BUNDLE_FOUND → FINAL_VERIFY_READY → FINAL_MATCH / FINAL_MISMATCH
```

Each state recommends exactly one next safe step from the existing safe action set (Select Plan, Validate
Input, Print Preview Command, Set Approval Output, Print Approval Command, Print Apply-Bundle Command,
Print Apply Command, Verify Final Target). The state machine **guides only**; it runs no live A2 command.

---

## 10. Artifact Discovery Rules

```text
- Read-only only: stat / read; never write, never create .claw, never mutate a target.
- Show every discovered path before use; auto-populate but keep the path visible and overridable.
- Exactly-one match may auto-select; zero or many → require explicit pick ("select needed").
- Never silently infer an unsafe path.
- Prefer parsing the helper's read-only find-artifacts / audit-workspace output over re-walking .claw.
- One-shot detection only (open + Refresh). No watcher, no polling, no background refresh.
- Hashes are read from artifacts to drive verify-final; never composed into an approval line.
```

---

## 11. Validation Plan

Offline, no live chain:

- TDD: write failing unit tests first for setup-status detection, discovery parsing (one/zero/many),
  state-machine mapping, and the "never recommends Run-*/chain-write" guard; then implement.
- Structural guards (existing style): no `Run-*` button; no chain-write fragment in any built argv; helper
  basename invariant intact; no `fs` write; no model/broker/`:11434`/secret reference.
- Headless render check: setup-status + next-safe-step render; Safety / Stop Gates banner always-on; no
  `Run-*` markup.
- Operator GUI click-through (read-only / print only) in a **disposable** workspace: open → status
  populates → discovery proposes paths (shown before use) → next safe step shown → print buttons print
  only → target unchanged, no `.claw` written.
- All existing unit tests must stay green; new tests added for every new module.

---

## 12. No-Live-A2 Boundary

This lane does not run the A2 chain. No live preview, no live approval, no live apply-bundle, no live
apply. Approval stays **real-terminal** and human-typed; the panel never composes or captures it. Apply
stays a printed command the operator runs themselves at a real terminal. Any live (even disposable) chain
exercise is a **separate**, explicitly-approved lane — not this one.

**Stop conditions** (surface, do not work around): artifact discovery would require unsafe inference;
detection would require spawning `claw`; a change would write a `.claw` artifact / target / non-evidence
file; an approval phrase would route through the webview; workspace-first UX would require a watcher or
polling; or helper read-only output is insufficient and the only alternative is the extension re-deriving
chain semantics.

---

## 13. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_IDE_EXTENSION_PANEL_WORKSPACE_FIRST_UX_IMPLEMENTATION

BRANCH / WORKTREE / BASE / COMMIT(S):
FILES CHANGED:

WHAT SHIPPED:
  setup-status detector:
  artifact discovery (Option B):
  next-step state machine:
  visual diff / preview readiness:
  evidence timeline:
  render + runbook update:

SAFETY:
  live preview / approval / apply-bundle / apply run:  no
  claw spawned:                                        no
  model / broker / runtime / :11434 call:              no
  approval phrase captured from webview:               no
  Run-* button added:                                  no
  .claw artifact written / target modified:            no
  filesystem watcher / polling added:                  no

VALIDATION:
  unit tests (existing + new):
  structural guards:
  headless render check:
  operator GUI click-through (read-only):
  target unchanged / no .claw written:

STOP GATES HIT: none | details
NEXT BEST LANE:
```

Do not push, open a PR, or merge unless a separate, explicit approval authorizes it.
