# A2-L4 IDE Extension / Panel — Scope (Docs-Only) — 2026-06-07

> Docs-only scope card. It implements NOTHING: no extension code, no `package.json`, no TypeScript,
> no JavaScript, no Rust, no schema, no runtime change, no preview/approval/apply/apply-bundle run,
> no model/broker/runtime/Vault call, no IDE install, no user-settings change. It scopes a FUTURE,
> separately-token-gated implementation lane for a visual VS Code / Cursor **extension panel** that
> wraps the already-merged, print/validate-only A2 IDE harness v0 in a safer button-driven surface
> **without weakening a single A2 safety gate**.

---

## 1. Executive Summary

The A2-L2b CLI write chain is functionally complete and proven end-to-end. Two consumer surfaces are
already merged on `main`:

```text
A2-L4 IDE harness v0 (workflow lineage):
  scripts/a2-ide-harness.sh                 print/validate-only helper that PRINTS each chain command
  .vscode/tasks.json                        8 "A2:" command-palette tasks that call the helper
  docs/runbooks/a2-ide-harness-workflow.md  operator runbook

A2-L3 IDE adapter (status-observer lineage):
  ide/vscode/claw-status-panel/             a read-only VS Code webview over `claw plan status`
```

The v0 harness is print/validate-only by design: it never runs an A2 command. It validates paths,
locates `.claw` artifacts read-only, shows hashes, and **prints** the exact command the operator runs
manually. It is the proven safety baseline this scope must preserve.

This card scopes **v1**: a real VS Code / Cursor **extension panel** with visible workflow sections and
buttons (Select Workspace, Select Plan, Validate Input, Generate Preview Command, Find Artifacts,
Audit Workspace, Show Preview State, Print Approval Command, Show Approval Result, Generate
Apply-Bundle Command, Print Apply Command, Verify Final Target, Export Evidence). The panel makes the
same chain visually understandable. It must **not** secretly bypass the A2 safety model.

**Recommended v1 direction (read this first):** build the smallest safe panel as a thin webview that
drives the EXISTING `scripts/a2-ide-harness.sh` helper in its read-only / print-only subcommands via an
argv-bounded subprocess wrapper, and renders the helper's stdout. The panel itself spawns **only the
helper** (never `claw plan run/approve/apply-bundle/apply`), generates commands as **copyable text**
only, and includes **no** dangerous execution buttons in v1. Approval stays at a **real terminal**,
human-typed. Apply stays explicit, visibly target-writing, one-time, and operator-run. The panel
reuses the proven `claw-status-panel` safety scaffolding (argv-bounding, static-grep guards,
no-network, no-watcher, no-polling, no-write, explicit-gesture-per-action). A full broad extension
scaffold is **not** authorized here; the implementation lane discovers the surface and STOPs if no
repo convention supports it.

---

## 2. Current v0 Baseline

The merged v0 harness (`scripts/a2-ide-harness.sh`, print/validate-only) exposes these read-only
subcommands, all of which the v1 panel buttons map onto:

```text
help                 prints the chain + safety rules
validate-input       --workspace --plan         read-only checks; refuses absolute after_file
print-preview        --workspace --plan         STEP 1 command text; writes NO target
find-artifacts       --workspace                lists .claw artifacts + sha + next-step hint (read-only)
print-approval       --workspace --preview-bundle --approval-output   STEP 2 text; REAL-TTY required
print-apply-bundle   --preview-generator-result --approval-result     STEP 3 text; GENERATOR only
print-apply          --apply-bundle             STEP 4 text; the only target writer
verify-final         --workspace --target --after-sha                 read-only hash check
audit-workspace      --workspace [--target --after-sha]               read-only chain-state audit
```

Hard invariants the v0 helper already preserves (the panel must preserve them identically):

```text
- Preview does NOT write target.
- Approval does NOT write target; it requires a REAL interactive terminal.
- apply-bundle is the GENERATOR; it writes NO target.
- `claw plan apply` is the EXECUTOR; it is the ONLY command that writes the target. It runs once.
- No auto-approval, no hidden apply, no batch/--yes/fake-TTY.
- The helper calls NO model / NO broker / NO runtime; it never executes `claw`.
- Chain state + "applied" evidence come from .claw ARTIFACTS + target HASH, never free-text logs.
```

The v0 helper has no exec mode on purpose. The v1 panel keeps that property: it adds visual structure
and copyable commands; it does not add an execution mode for any chain-write step.

---

## 3. Product Goal

Give a non-terminal-first operator a **visual, button-driven** way to drive the proven A2-L2b chain
inside VS Code / Cursor, with clearly-labeled workflow stages, live read-only state, and copyable
commands — while making it structurally impossible for the panel to approve or apply on the operator's
behalf. The panel is an **observer + command-generator**, never a chain controller.

---

## 4. User Problem

The v0 harness solved memorization (it prints each command) but the surface is still task-list-driven:
the operator runs separate command-palette tasks, re-types workspace/plan/artifact paths into prompt
inputs for each task, reads scattered stdout, and threads artifact paths between stages by hand. There
is no single visual surface that shows "where am I in the chain, what is the next step, what command do
I copy, and what is the evidence so far." The v1 panel provides that single surface without changing
the safety model: one panel, labeled sections, one button per read-only/print action, live state from
`audit-workspace` / `find-artifacts`, and copy-to-clipboard for each printed command.

---

## 5. Source of Truth

Read read-only in this lane and used as the design basis:

```text
Merged v0 harness (origin/main @ c8f27af / c5986e8 / 2907147):
  scripts/a2-ide-harness.sh
  .vscode/tasks.json
  docs/runbooks/a2-ide-harness-workflow.md
  docs/a2-l4-ide-harness-workflow-scope.md
  docs/a2-l4-ide-harness-v0-ux-polish-scope.md
  handoffs/a2_ide_harness_workflow_implementation_report_2026-06-07.md
  handoffs/a2_ide_harness_v0_ux_polish_implementation_report_2026-06-07.md

Existing extension convention + safety scaffolding (A2-L3 IDE adapter):
  ide/vscode/claw-status-panel/                     existing VS Code extension package
  ide/vscode/claw-status-panel/src/subprocess.ts    argv-bounded spawn wrapper (the pattern to reuse)
  ide/vscode/claw-status-panel/scripts/run-guards.js static-grep guards (no-network/-watcher/-write/-claw)
  ide/vscode/claw-status-panel/package.json          one-command-per-gesture contributes model
  ide/vscode/claw-status-panel/README.md             "what it does NOT do" boundary
  docs/a2-l3-ide-adapter-scope-card.md               IDE adapter doctrine (forbidden affordances)
  docs/a2-l3-adapter-boundary-scope-card.md          adapter boundary (read-only observer contract)
  docs/a2-l3-ide-adapter-implementation-scope-card.md guard §13-§18 source of record
  docs/editor-vscode.md                              editor integration notes
```

Canonical approval grammar is fixed by CLI source (`a2-plan-runner/src/approval.rs`,
`a2-plan-runner/src/approval_ux.rs`):

```text
apply <step-id> <preview_sha256>
```

The panel never composes this line itself (see §10).

---

## 6. Extension / Panel Options

```text
Option A  VS Code / Cursor extension panel (webview) that drives the EXISTING v0 helper in
          read-only/print modes and renders its stdout. Buttons = read-only/print subcommands +
          copy-to-clipboard of helper-printed commands. No chain-write spawn from the panel.
          → RECOMMENDED for v1. Lowest risk, reuses claw-status-panel scaffolding, no command
            re-implementation, no approval/apply execution.

Option B  Standalone local web panel (served outside the IDE) over the same helper.
          → Higher surface (a server), no IDE-native diff/terminal affordances, more moving parts.
            Defer; not needed when the IDE host already provides a webview.

Option C  Helper-script panel only (status quo v0 + a TUI/menu in bash).
          → Lowest UX gain; does not deliver the requested visual panel.

Option D  Extend the existing claw-status-panel extension to add chain-workflow buttons.
          → REJECTED. claw-status-panel is bound by the A2-L3 IDE adapter contract, which forbids
            approve/apply/run/apply-bundle affordances, forbids composing the approval line, forbids
            printing chain-write commands, and forbids direct .claw reads. Its static guards reject
            `claw plan run/approve/apply-bundle/apply` references in source. Adding workflow buttons
            there would force relaxing those guards on a surface explicitly designed to never carry
            them. The workflow panel must be a SEPARATE package under its own contract.
```

**Why D is rejected and A is chosen:** the status observer (A2-L3) and the workflow harness (A2-L4)
are different contracts. The observer renders `claw plan status` and must never reference chain-write
commands. The workflow panel must surface the full preview→approval→apply chain as copyable commands.
Keeping them as separate packages preserves the observer's strict guards and lets the workflow panel
carry its own (still strict, but different) guard set.

---

## 7. Recommended v1 Architecture

A new, separate VS Code / Cursor extension package, a **sibling** of `claw-status-panel` under
`ide/vscode/` (candidate name `ide/vscode/a2-harness-panel/`, to be confirmed by implementation
discovery — see §15 and the STOP in §17). It reuses the claw-status-panel scaffolding *patterns* by
re-deriving them, NOT by taking a dependency:

```text
Panel (webview)
  ├─ renders WORKFLOW SECTIONS (§8) and BUTTONS (§9)
  ├─ each button = ONE explicit operator gesture
  └─ shows helper stdout as opaque, read-only text

Subprocess wrapper (argv-bounded, claw-status-panel pattern)
  ├─ spawns ONLY: scripts/a2-ide-harness.sh <read-only-subcommand> [--flag value ...]
  ├─ subcommand allowlist: help, validate-input, print-preview, find-artifacts,
  │                        print-approval, print-apply-bundle, print-apply,
  │                        verify-final, audit-workspace
  ├─ NEVER spawns `claw`, `claw plan run/approve/apply-bundle/apply`, or any other binary
  ├─ rejects flag-shaped / write-shaped arguments (reuse SubprocessRefusal pattern)
  └─ injectable spawn impl so tests audit argv without touching the OS

Clipboard
  └─ copies a single helper-produced command string verbatim (no composition, no decoration)

Static-grep guards (claw-status-panel run-guards.js pattern, adjusted)
  ├─ FORBIDDEN-NETWORK / -WATCHER / -POLLING / -WRITE / -SECRET-API  (kept identical)
  ├─ FORBIDDEN-CHAIN-WRITE-SPAWN: panel source must not spawn `claw plan run/approve/
  │   apply-bundle/apply` (the helper prints them; the panel renders that text, never executes it)
  ├─ FORBIDDEN-APPROVAL-COMPOSE: panel source must not compose `apply ${step} ${sha}`
  └─ ONLY-HELPER-SPAWN: the single spawned binary is the harness helper with an allowlisted subcommand
```

Key property: chain-write command strings (`claw plan apply`, the approval line) exist **only** in the
helper's stdout, rendered by the panel as opaque text. The panel's own TypeScript never constructs or
executes them. This is what lets the panel offer "Print Apply Command" / "Print Approval Command"
buttons while still passing strict no-chain-write-spawn and no-approval-compose guards.

The panel sources chain state exclusively from the helper's read-only `audit-workspace` /
`find-artifacts` / `verify-final` output. It does not read `.claw/**` directly; the bash helper does
the read-only artifact inspection (as it already does in v0).

---

## 8. Panel UX Layout

The panel renders these visual sections, top to bottom, mapping 1:1 to the proven chain:

```text
[ Workspace / Plan Selection ]   pick <workspace-root> and <plan.yaml>           (UI input only)
[ Input Validation ]             validate-input result (refuses absolute after_file)
[ Preview State ]                print-preview command (copyable); preview_sha256/step_id when present
[ Artifact Browser ]             find-artifacts: .claw artifacts + sha + next-step hint (read-only)
[ Approval State ]               print-approval command (copyable); REAL-terminal note + grammar
[ Show Approval Result ]         read-only display that an approval-result.json exists (from artifacts)
[ Apply-Bundle State ]           print-apply-bundle command (copyable); labeled GENERATOR, writes NO target
[ Apply State ]                  print-apply command (copyable); labeled EXECUTOR / only target writer
[ Final Verification ]           verify-final: target hash vs expected after_sha256 (read-only)
[ Evidence Export ]              export a read-only evidence summary of the above (text/markdown)
[ Safety / Stop Gates ]          always-visible STOP banner: the §17 conditions, never collapsed
```

Each section shows the relevant helper stdout verbatim and (where a command is printed) a single
copy-to-clipboard control for that exact string. A persistent "Audit Workspace" refresh re-runs the
read-only `audit-workspace` subcommand on explicit operator gesture and updates section state.

---

## 9. Button Behavior Matrix

```text
BUTTON                       MAPS TO (read-only helper subcommand)          EFFECT                       CLASS
Select Workspace             (UI only)                                      sets workspace path          safe
Select Plan                  (UI only)                                      sets plan.yaml path          safe
Validate Input               validate-input --workspace --plan              shows read-only result       safe
Generate Preview Command     print-preview --workspace --plan               prints STEP 1 text; copyable safe
Find Artifacts               find-artifacts --workspace                     lists .claw + hashes (RO)    safe
Audit Workspace              audit-workspace --workspace [...]              chain-state audit (RO)       safe
Show Preview State           audit-workspace / find-artifacts (parsed RO)   renders preview_sha/step_id  safe
Print Approval Command       print-approval --workspace --preview-bundle    prints STEP 2 text; copyable safe
                               --approval-output
Show Approval Result         find-artifacts / audit-workspace (RO)          shows approval-result exists safe
Generate Apply-Bundle Cmd    print-apply-bundle --preview-generator-result  prints STEP 3 text; copyable safe
                               --approval-result
Print Apply Command          print-apply --apply-bundle                     prints STEP 4 text; copyable safe
Verify Final Target          verify-final --workspace --target --after-sha  read-only hash check         safe
Open Runbook                 (UI only) open docs/runbooks/a2-ide-harness-…  opens file in editor         safe
Export Evidence Summary      (assembles RO helper output into a summary)    text/markdown export         safe

EXPLICITLY EXCLUDED FROM v1 (dangerous execution buttons — NOT present):
Run Preview                  would spawn `claw plan run …`                  EXCLUDED                     dangerous
Run Approval                 would spawn `claw plan approve …`              EXCLUDED                     dangerous
Run Apply-Bundle             would spawn `claw plan apply-bundle …`         EXCLUDED                     dangerous
Run Apply                    would spawn `claw plan apply …`                EXCLUDED                     dangerous
```

The "Generate/Print … Command" buttons emit copyable text from the helper; they never execute. The
default v1 scope does **not** include the dangerous execution buttons. A future lane may revisit them
only behind a separately-gated, independently-reviewed safe design — and even then, approval must stay
real-terminal and human-typed (§10), and apply must stay explicit and operator-run (§11).

---

## 10. Approval UX Boundary

```text
- Approval is a REAL-terminal, human-typed action. The panel NEVER executes `claw plan approve`.
- The panel NEVER composes the approval line `apply <step-id> <preview_sha256>` in its own source.
  The line appears only in the helper's print-approval stdout, rendered as opaque text.
- The "Print Approval Command" button shows the helper's `claw plan approve …
  --approval-result-output …` command and offers copy-to-clipboard of that verbatim string.
- The panel MUST NOT capture the operator's approval line in a webview input and forward it anywhere.
  A webview input is not a TTY. There is no approval modal, no inline approval field, no "approve"
  button.
- The panel MAY display, read-only, that an approval-result.json artifact exists (from find-artifacts /
  audit-workspace). It MUST NOT create, edit, or fabricate an approval-result.
- A non-interactive runner fail-closes (exit 7) at approval — that is the TTY guard, surfaced as
  guidance, never worked around.
- No auto-approve, no batch, no --yes, no fake-TTY, no preapproval, no "approve when X".
```

This is human approval, unchanged from v0: the panel makes it easier to find and copy the exact
command, never easier to skip.

---

## 11. Apply UX Boundary

```text
- apply-bundle is the generator, not the executor: `claw plan apply-bundle <gen-result> <approval>`
  assembles apply-bundle.json and writes NO target. The panel labels it GENERATOR.
- `claw plan apply <apply-bundle.json>` is the EXECUTOR — the only command that writes the target,
  and it runs once. The panel labels it EXECUTOR / "the only target writer".
- The panel NEVER executes apply-bundle or apply. "Generate Apply-Bundle Command" and "Print Apply
  Command" emit copyable text only.
- There is no hidden apply and no auto-apply. Apply is explicit, visibly target-writing, operator-run
  at their own terminal, once per approved preview.
- Running apply twice for an already-applied preview is a STOP condition (§17), surfaced read-only from
  artifact/hash state, never auto-retried by the panel.
```

---

## 12. Artifact Model

The panel observes the existing `.claw` artifact layout **only through the helper's read-only
subcommands** (the helper performs the bash-level read; the panel does not read `.claw/**` directly):

```text
preview-bundle.json / preview-generator-result.json   → preview evidence (preview_sha256, step_id)
approval-result.json                                  → persisted human approval (presence only)
apply-bundle.json                                     → generated bundle (writes no target)
apply-result.json                                     → executor result; presence == applied
l2b-checkpoints/ (before.bin) , l2b-payloads/ (after) → rollback baseline / payload (display only)
<target>                                              → the single target write; hashed read-only
```

Detection is artifact/hash-based, never free-text-log-based (this is the v0 UX-polish fix): "applied"
is decided from the executor-written `apply-result.json` plus a target hash matching the expected
`after_sha256`. Marker names (`a2-l2b-write-applied`, etc.) are shown as operator guidance, never
treated as evidence. The panel never fabricates or edits any `.claw` artifact.

---

## 13. Evidence Export

```text
- "Export Evidence Summary" assembles the read-only helper output (validate-input, find-artifacts /
  audit-workspace chain state, verify-final hash result, the printed commands) into a single
  text/markdown summary for the operator's records.
- Export is READ-ONLY of already-produced helper stdout + the operator's selected paths/hashes.
- Export MUST NOT read .claw/** directly, MUST NOT read file contents beyond what the helper already
  surfaced, MUST NOT include secrets, and MUST NOT relay anything to a network endpoint.
- Where export writes a file at all, it writes ONLY to an operator-chosen path outside .claw and
  outside the target tree; v1 MAY restrict export to clipboard / an unsaved editor document to avoid
  any filesystem write from the panel (preferred, to keep the no-write guard absolute).
```

---

## 14. Safety Invariants

The panel wraps the proven A2 chain. It must NOT replace it with an unsafe shortcut. All v0 invariants
hold, plus panel-surface invariants:

```text
Chain invariants (unchanged from v0 / A2-L2b):
  Preview writes no target. Approval writes no target. apply-bundle generation writes no target.
  Only `plan apply` writes the target, once per approved preview.
  No auto-approval. No hidden apply. No apply without a validated approval-result.
  No apply if the target hash differs from before_sha. No repeated apply per approved preview.

Panel-surface invariants (new, this card):
  PANEL-SPAWN-BOUNDED      : the only binary the panel spawns is scripts/a2-ide-harness.sh with an
                            allowlisted read-only/print subcommand; it never spawns `claw` or any
                            chain-write command.
  PANEL-NO-CHAIN-WRITE     : panel source contains no `claw plan run/approve/apply-bundle/apply`
                            spawn; those strings appear only in rendered helper stdout.
  PANEL-NO-APPROVAL-COMPOSE: panel source never composes `apply <step-id> <preview_sha256>`.
  PANEL-COPY-BOUNDED       : copy-to-clipboard places a single verbatim helper-produced string; no
                            composite payload, no decoration, no "copy and run".
  PANEL-NO-WRITE           : the panel writes no file under .claw, the workspace, or the target tree
                            (evidence export prefers clipboard / unsaved doc; see §13).
  PANEL-GESTURE-BOUNDED    : every helper invocation is one explicit operator gesture; no watcher, no
                            polling, no auto-refresh, no refresh-on-save/focus/git.
  PANEL-NO-NETWORK         : no model, no broker, no Ollama, no raw :11434 inference, no telemetry,
                            no analytics, no network egress at any phase.
  PANEL-STOP-LOUD          : the Safety / Stop Gates section is always visible and never collapsed,
                            debounced, snoozed, or down-classified.
  PANEL-NO-SECRET          : the panel reads no secrets, env, shell history, or Vault material, and
                            renders none.
```

---

## 15. Implementation Surfaces

Candidate surfaces for the FUTURE implementation lane (to be DISCOVERED and confirmed, not assumed):

```text
ide/vscode/a2-harness-panel/            NEW sibling package (name candidate; confirm in discovery)
  package.json                          extension manifest: one command per button; no write commands
  tsconfig.json / tsconfig.test.json    build config (mirror claw-status-panel)
  src/extension.ts                      activation + command registration (read-only/print only)
  src/panel.ts                          webview lifecycle + section rendering
  src/subprocess.ts                     argv-bounded helper spawn wrapper (allowlist + refusals)
  src/render.ts                         pure-string render of helper stdout into sections
  src/clipboard.ts                      single-field verbatim copy
  scripts/run-guards.js                 static-grep guards (no-network/-watcher/-write/-chain-write-
                                        spawn/-approval-compose; only-helper-spawn)
  test/                                 mocha tests + fixtures (fixture helper stdout, never live A2)
docs/runbooks/a2-ide-harness-workflow.md   MAY gain a v1 panel section (separate doc lane)
```

Forbidden surfaces for the implementation lane (must STOP if touched): any Rust
(`rust/crates/**`), any A2-L2b/L2d producer module, any schema, any `.service`/systemd unit, the
broker/runtime/HQ, Vault/secrets, and the existing `ide/vscode/claw-status-panel/` package (the
workflow panel is a separate package; it must not relax the status observer's guards).

The implementation lane MUST STOP before scaffolding a broad new extension framework if discovery does
not confirm the `ide/vscode/<package>/` convention as the right home; the convention exists today
(claw-status-panel), so a single sibling package is the expected, minimal shape — not a monorepo of
new tooling.

---

## 16. Validation Requirements

A future implementation must demonstrate (no live A2 commands; fixtures only):

```text
- Every button maps to exactly one read-only/print helper subcommand (or a UI-only action) and nothing
  else — asserted by test.
- The panel spawns ONLY scripts/a2-ide-harness.sh with an allowlisted subcommand; spawn of `claw` or
  any chain-write command is impossible by construction — asserted by argv-audit test + static guard.
- Panel source composes no approval line and no chain-write spawn — asserted by run-guards.js.
- The panel performs zero filesystem writes under any input — asserted by test.
- The panel performs zero network egress — asserted by static guard + test.
- Preview/approval/apply-bundle/apply are never executed by the panel — asserted by no-exec test.
- Chain state shown by the panel is artifact/hash-based (from helper stdout), never free-text-log-based.
- STOP / Safety section renders the §17 conditions and is never collapsed/debounced.
- Tests run against FIXTURE helper stdout; no live preview/approval/apply runs; target unchanged; repo
  clean. Stack-Code tests use stdlib mechanisms consistent with the existing package (claw-status-panel
  uses mocha; match the chosen package's test runner — do not assume pytest).
- All existing A2-L2b/L2c/L2d/L3 and v0 harness tests still pass unchanged.
```

---

## 17. STOP Conditions

The panel (and any future implementation) must STOP / surface-and-halt if:

```text
- the panel would spawn `claw` or any chain-write command (run/approve/apply-bundle/apply)
- the panel would compose the approval line or capture it from a webview input
- any auto-approval / hidden apply / batch / --yes / fake-TTY path is introduced
- a "Run Preview/Approval/Apply-Bundle/Apply" execution button is added in v1
- missing preview bundle, missing approval-result, preview hash mismatch, target drift
  (target hash != before_sha256 before apply), or a prior apply marker for this preview
- apply attempted twice for an already-applied preview
- unreviewed or absolute after_file; unsafe target path (outside workspace / runtime / service /
  secret path)
- the panel reads .claw/** directly instead of through the helper, or reads secrets/env/Vault
- any model / broker / runtime / Ollama / :11434 call is introduced
- discovery does not confirm a repo convention for the panel package → STOP before scaffolding
- the implementation tries to relax claw-status-panel's guards or fold workflow buttons into it
```

---

## 18. Future Lanes

```text
1. A2 IDE Extension Panel Scope Review / Push PR (this card + the DRAFT implementation prompt).
2. A2 IDE Extension Panel Scope exact-head merge gate.
3. (token-gated) A2 IDE Extension Panel Implementation — v1 Option A build, smallest safe panel over
   the existing helper, per
   handoffs/a2_ide_extension_panel_implementation_prompt_DRAFT_2026-06-07.md, gated by token:
     APPROVED: Execute A2 IDE extension panel implementation
4. (later, separate) Revisit dangerous execution buttons ONLY behind a dedicated, independently-
   reviewed safe-gating design — approval stays real-terminal, apply stays explicit and operator-run.
5. (later, separate) Optional read-only diff viewer / richer evidence export, each its own scope lane.
```

---

## 19. Final Recommendation

```text
Proceed with v1 as the SMALLEST safe panel: a separate VS Code / Cursor extension package that drives
the EXISTING print/validate-only v0 helper in read-only/print modes and renders its stdout, with
buttons that generate/copy commands rather than execute them. Build on v0; do not replace it. Reuse
the proven claw-status-panel safety scaffolding (argv-bounding, static guards, no-network/-watcher/
-write, explicit-gesture). No dangerous execution buttons in v1. No auto-approval. No hidden apply. No
model/broker/runtime calls. No raw :11434 inference. Approval requires human review at a real terminal
and the exact `apply <step-id> <preview_sha256>` phrase typed there. apply-bundle is the generator;
`plan apply` is the only command that writes the target. Review and merge this scope and its DRAFT
implementation prompt before implementing.
```
