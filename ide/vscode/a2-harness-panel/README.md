# A2 Harness Panel (VS Code / Cursor)

A visual, button-driven panel for the **A2 IDE harness v0**. It drives the
merged print/validate-only helper
[`scripts/a2-ide-harness.sh`](../../../scripts/a2-ide-harness.sh) in its
read-only / print subcommands through an argv-bounded wrapper, renders the
helper's stdout, and offers copy-to-clipboard for the helper-printed commands.

This package is the v1 evolution of the A2-L4 IDE harness. It is a **separate
sibling** of [`../claw-status-panel`](../claw-status-panel) (the A2-L3 status
observer) and does not modify it. Bounded by:

- [`docs/a2-l4-ide-extension-panel-scope.md`](../../../docs/a2-l4-ide-extension-panel-scope.md)
- [`docs/runbooks/a2-ide-harness-workflow.md`](../../../docs/runbooks/a2-ide-harness-workflow.md)

## What it does

- Exposes one command, **A2 Harness: Open Panel**, that opens a webview with
  workflow sections (Workspace/Plan selection, Actions, Helper output, and an
  always-on Safety / Stop Gates banner).
- Each **helper button** runs exactly one read-only/print helper subcommand
  (`validate-input`, `audit-workspace`, `find-artifacts`, `print-preview`,
  `print-approval`, `print-apply-bundle`, `print-apply`, `verify-final`) via the
  argv-bounded runner and renders its stdout verbatim.
- The `print-*` buttons are labelled **Show/Copy … Command**: they display and
  let you copy the command the helper printed, for you to run yourself at a real
  terminal.
- **Open Runbook** opens the operator runbook; **Export Evidence Summary** opens
  an unsaved markdown document with the current inputs + last helper output (the
  panel writes no file).

## What it does NOT do

- No **Run Preview / Run Approval / Run Apply-Bundle / Run Apply** button. The
  panel shows/copies commands; it never executes a chain-write step.
- It never spawns `claw`; the only binary it spawns is the harness helper, and
  only with a read-only/print subcommand from a fixed allowlist (no shell).
- It never composes the approval line `apply <step_id> <preview_sha256>` and
  never captures an approval line from the webview. Approval stays at a **real
  terminal**, human-typed.
- `apply-bundle` is the generator (writes no target); `claw plan apply` is the
  only command that writes the target — and the panel only prints it.
- No auto-approval, no hidden apply. No model / broker / runtime / `:11434`
  call. No secrets. No `fs` access (the helper does the read-only `.claw`
  inspection). No filesystem watcher, no polling, no background refresh — every
  action is one explicit operator gesture.

## Build and test

```bash
cd ide/vscode/a2-harness-panel
npm install --ignore-scripts
npm run lint        # static-grep guards (network / fs / spawn-boundary / approval-compose)
npm run compile     # tsc -p .
npm test            # mocha unit tests (helper runner, buttons, render, clipboard, guards, manifest)
```

The package is not packaged as a `.vsix` by this lane.

## Files

- `src/helperRunner.ts` — the **single** process-spawn boundary: argv-bounded
  wrapper that spawns only the helper with an allowlisted subcommand + flags.
- `src/buttons.ts` — the safe button catalog (helper buttons + UI buttons); no
  Run-* execution buttons.
- `src/render.ts` — pure HTML renderer incl. the always-on Safety / Stop Gates
  section.
- `src/panel.ts` — webview lifecycle + click wiring.
- `src/extension.ts` — activation, command registration, message handling.
- `src/clipboard.ts` — single-field verbatim copy helper.
- `scripts/run-guards.js` — static-grep guards (`npm run lint`).
- `test/` — mocha tests + fixtures (helper stdout fixtures; never live A2).
