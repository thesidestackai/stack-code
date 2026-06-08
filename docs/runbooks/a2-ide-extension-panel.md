# A2 IDE Extension Panel — Operator Runbook (v1)

> v1 is a **visual VS Code / Cursor panel** that drives the print/validate-only A2 IDE harness v0
> ([`scripts/a2-ide-harness.sh`](../../scripts/a2-ide-harness.sh)). It does **not** run any A2 chain
> command and it does **not** weaken any safety gate. You still run preview / approval / apply
> yourself, with approval at a **real terminal**.

Scope source of truth: [`a2-l4-ide-extension-panel-scope.md`](../a2-l4-ide-extension-panel-scope.md).
Package: [`ide/vscode/a2-harness-panel/`](../../ide/vscode/a2-harness-panel/).

---

## What it gives you

A single panel with labeled sections and buttons, instead of separate command-palette tasks:

```text
[ Safety / Stop Gates ]   always-on banner of the invariants + STOP conditions
[ Workspace / Plan / Artifact selection ]  set the paths the helper needs
[ Actions ]               one button per read-only/print helper subcommand
[ Helper output ]         the helper's stdout, verbatim, with a Copy button
```

Each **helper button** runs exactly one read-only/print subcommand through an argv-bounded wrapper and
shows its stdout. The `Show/Copy … Command` buttons display the command the helper printed so you can
copy it and run it yourself at a real terminal.

The panel never executes the A2 chain. It shows/copies commands; it does not run preview, approval,
apply-bundle, or apply.

---

## Open the panel

1. Build the package once (see below) or install it from source.
2. Command Palette → **A2 Harness: Open Panel**.
3. In the **Workspace / Plan / Artifact selection** section, use the field-setter controls to set the
   fields each action needs. These controls set fields only — they never run a chain command.
4. Click the action buttons in chain order.

### Field-setter controls (set fields only)

The selection section exposes one control per input field, shown next to the field table:

| Control | Field it sets |
| --- | --- |
| Select Workspace | workspace root (contains `.claw` + the target) |
| Select Plan | `plan.yaml` (after_file must be relative) |
| Select Target | the target file `plan apply` writes (for Verify Final) |
| Set After SHA | expected `after_sha256` of the target (for Verify Final) |
| Select Preview Bundle | `preview-bundle.json` (for Show/Copy Approval Command) |
| Select Generator Result | `preview-generator-result.json` (for Show/Copy Apply-Bundle Command) |
| Select Approval Result | persisted `approval-result.json` (for Show/Copy Apply-Bundle Command) |
| Set Approval Output | path to write the new `approval-result.json` (for Show/Copy Approval Command) |
| Select Apply Bundle | `apply-bundle.json` (for Show/Copy Apply Command) |

Each control only stores a path/hash in the panel session; nothing is executed. Verify Final, Show/Copy
Approval, and Show/Copy Apply stay blocked (with a notice) until their fields are set — set them here.

| Button | Helper subcommand | Runs an A2 command? |
| --- | --- | --- |
| Validate Input | `validate-input` | No |
| Audit Workspace | `audit-workspace` | No (read-only artifact/hash audit) |
| Find Artifacts | `find-artifacts` | No |
| Show/Copy Preview Command | `print-preview` | No (prints only) |
| Show/Copy Approval Command | `print-approval` | No (prints only; REAL-terminal note) |
| Show/Copy Apply-Bundle Command | `print-apply-bundle` | No (prints generator command) |
| Show/Copy Apply Command | `print-apply` | No (prints executor command) |
| Verify Final Target | `verify-final` | No (read-only hash check) |
| Open Runbook | — | No (opens this/the v0 runbook) |
| Export Evidence Summary | — | No (opens an unsaved summary doc) |

There is intentionally **no** Run Preview / Run Approval / Run Apply-Bundle / Run Apply button.

---

## The chain you still run yourself

```text
1. PREVIEW   claw plan run <plan.yaml> --workspace-root <ws> --workspace-write-preview
2. APPROVE   claw plan approve <preview-bundle.json> --approval-result-output <out.json>
             (REAL terminal; at the prompt type:  apply <step-id> <preview_sha256>)
3. BUNDLE    claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
4. APPLY     claw plan apply <apply-bundle.json>
```

Copy each command from its `Show/Copy …` button and run it at a real terminal. Approval must be typed
by you at a real TTY — the panel never composes the approval line and never captures it.

---

## Safety rules this panel preserves

```text
- The panel spawns ONLY the helper, with a read-only/print subcommand. It never spawns `claw`.
- Preview/approval/apply-bundle write no target; only `claw plan apply` writes, once, run by you.
- No auto-approval. No hidden apply. No batch/--yes/fake-TTY.
- No model / broker / runtime / :11434 call. No secrets. No `fs` access from the panel.
- No filesystem watcher, no polling, no auto-refresh — every action is one explicit gesture.
```

---

## Build and test (from source)

```bash
cd ide/vscode/a2-harness-panel
npm install --ignore-scripts
npm run lint     # static guards
npm run compile  # tsc -p .
npm test         # mocha unit tests
```

The package is not packaged as a `.vsix` in this lane.
