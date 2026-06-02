# Claw Status Panel (VS Code)

Read-only VS Code observer for `a2-l2d-status.v1` envelopes emitted by
`claw plan status <workspace> [<approval-result.json>]`.

This package is the first per-host A2-L3 IDE adapter implementation. It is
bounded by:

- [`docs/a2-l3-ide-adapter-scope-card.md`](../../../docs/a2-l3-ide-adapter-scope-card.md)
- [`docs/a2-l3-ide-adapter-implementation-scope-card.md`](../../../docs/a2-l3-ide-adapter-implementation-scope-card.md)
- [`docs/a2-l3-adapter-boundary-scope-card.md`](../../../docs/a2-l3-adapter-boundary-scope-card.md)
- [`docs/a2-l2d-status-schema.md`](../../../docs/a2-l2d-status-schema.md)
- [`docs/a2-l2d-operator-quickref.md`](../../../docs/a2-l2d-operator-quickref.md)

## What it does

- Exposes one command, `Claw Status: Refresh`, that invokes
  `claw plan status <workspace> [<approval-result.json>]` exactly once per
  operator gesture and renders the resulting envelope in a webview panel.
- Renders every closed-enum envelope field verbatim. Unknown enum values,
  schema-version drift, missing `read_only_invariant`, and unparseable
  stdout are surfaced as STOP signals.
- Renders `next_operator_command` as copyable text only.
- Renders each `evidence_paths` entry as a clickable link; out-of-workspace
  entries are flagged with a warning. The panel itself never reads the
  file's contents — clicks delegate to the IDE host's standard editor.
- Provides a collapsible raw-envelope view.

## What it does NOT do

- No approve / apply / apply-bundle / run / approve-and-apply controls.
- No command-palette / keybinding / context-menu / gutter / lens / hover /
  status-bar action that triggers a chain-write step.
- No filesystem watcher, no background polling, no auto-refresh on save /
  focus / Git events. Every status invocation requires an explicit
  operator gesture.
- No telemetry, analytics, error-reporting, marketplace dashboard, or
  broker / model / Ollama traffic.
- No direct `.claw/**` reads. Chain state is consumed only through
  `claw plan status` stdout.
- No on-disk envelope cache, no IDE workspace/global/secret storage
  writes, no workspace mutation.
- No cross-workspace aggregation, no disposable-workspace classification
  override.

See `docs/a2-l3-ide-adapter-scope-card.md` §§7–9, §12, §14–15 for the full
enumeration.

## Build and test

```bash
cd ide/vscode/claw-status-panel
npm install
npm run lint        # static-grep guards (forbidden API / network / .claw)
npm run compile     # tsc -p .
npm test            # unit tests against parser, render, manifest, etc.
```

The package is not packaged as a `.vsix` by this implementation lane. A
packaging / marketplace lane is out of scope per the implementation
scope card.

## Files

- `src/envelope.ts` — `a2-l2d-status.v1` type definitions
- `src/parser.ts` — envelope parser, STOP classifier
- `src/stop.ts` — closed-enum literals + STOP detection helpers
- `src/render.ts` — pure-string HTML/render-model builder for the webview
- `src/subprocess.ts` — argv-bounded wrapper around `claw plan status`
- `src/clipboard.ts` — single-field copy helper
- `src/evidencePath.ts` — in-workspace / out-of-workspace classification
- `src/manifestAudit.ts` — static audit of `package.json` contributions
- `src/extension.ts` — VS Code activation, command registration
- `src/panel.ts` — VS Code webview lifecycle
- `scripts/run-guards.js` — CLI entry for the static-grep guards used by
  `npm run lint` (the same logic is exercised from `test/guards.test.ts`)
- `test/` — mocha tests + golden fixtures
