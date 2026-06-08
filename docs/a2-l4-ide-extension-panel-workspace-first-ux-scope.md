# A2-L4 IDE Extension Panel — Workspace-First UX Scope — 2026-06-08

> **Docs-only scope.** This card designs the next product layer for the A2 IDE Extension Panel.
> It implements nothing, runs no A2 command, makes no model / broker / runtime call, and touches no
> extension source. It is the source of truth a separately-approved implementation lane would build to.
> The panel stays **print/validate-only**: no live apply, no auto-approval, no hidden apply, approval
> stays **real-terminal** and human-typed.

---

## 1. Executive Summary

The A2 IDE Extension Panel v1 is **operator-ready for print/validate-only use** (PRs #96 / #98 / #99 on
`main`). It is a visual, button-driven VS Code / Cursor panel that drives the print/validate-only A2 IDE
harness helper (`scripts/a2-ide-harness.sh`) through an argv-bounded wrapper, renders the helper stdout
verbatim, and copies the helper-printed chain commands. It never spawns `claw`, never runs
preview / approval / apply-bundle / apply, and has no `Run-*` buttons.

The next gap is **workspace-first usability**. Today the panel is safe and correct but still demands too
much manual setup: the operator must know the artifact names, set every field by hand, and read raw
helper stdout to understand where they are in the chain. The goal of this scope is to move the panel from
a *safe operator tool* toward a *Codex / Claude Code / Cursor-like* experience — **open a workspace and
immediately see setup status, discovered plans/artifacts, and the next safe step** — without weakening a
single safety gate.

The enabling insight from discovery (§4): the helper *already* computes chain state and next-step
guidance read-only (`detect_chain_state`, `print_next_step_hint`, `find-artifacts`, `audit-workspace`).
The workspace-first layer is therefore mostly a **presentation + read-only detection** layer over
machinery that already exists — not a reimplementation of chain logic in the extension.

---

## 2. Current State

```text
Merged on main:
  PR #96  feat(a2): add IDE extension panel v1                377e25e
  PR #98  fix(a2): expose panel artifact field controls       6267f42
  PR #99  docs(a2): operator-ready handoff                    fe5614b

Package:        ide/vscode/a2-harness-panel/
Runbook:        docs/runbooks/a2-ide-extension-panel.md
Handoff:        handoffs/a2_ide_extension_panel_v1_operator_ready_handoff_2026-06-08.md
Helper driven:  scripts/a2-ide-harness.sh   (print/validate-only; the ONLY binary the panel spawns)
```

What works today:

- Field-setter controls for every input (workspace, plan, target, after-sha, preview-bundle,
  generator-result, approval-result, approval-output, apply-bundle) — each **sets a field only**.
- Read-only / print actions: Validate Input, Audit Workspace, Find Artifacts, Show/Copy
  Preview / Approval / Apply-Bundle / Apply Command, Verify Final Target, Open Runbook, Export Evidence.
- Always-on Safety / Stop Gates banner; no `Run-*` button exists; 49/49 unit tests green.

What is structurally true in the current source (from discovery):

- `extension.ts` holds a single `session` with `inputs` / `output` / `notice`; every field is set by an
  explicit input-box gesture (`pickPath`); nothing is auto-detected on panel open.
- `render.ts` is a pure renderer; it "computes no chain state of its own — the helper's stdout … is the
  state surface, rendered as text."
- `helperRunner.ts` is the single spawn boundary: array-argv only, no shell, basename must equal
  `a2-ide-harness.sh`, per-subcommand flag allowlist, chain-write fragments refused.

---

## 3. Product Problem

```text
- Too much manual configuration remains.
- The operator must already understand paths, artifact names, and the chain sequence.
- The panel is safe but not yet assistant-like.
```

Concretely:

- **Cold open is blank.** Opening the panel shows empty fields and an empty output area. The operator
  gets no read on whether the workspace is even set up (helper present? claw on PATH? is there a plan?).
- **State is buried in stdout.** Chain state (preview-ready / approval-ready / …) only appears after the
  operator manually runs `find-artifacts` or `audit-workspace` and reads the text. There is no structured
  "you are here" surface.
- **Artifacts are typed by hand.** `Find Artifacts` lists `.claw` artifacts but does not populate the
  fields; the operator copies paths manually. The handoff explicitly lists "Find Artifacts read-only
  auto-population (Option B)" as deferred future work.
- **Next step is implicit.** The helper prints a next-step hint, but the panel does not translate it into
  a single, obvious "next safe step" recommendation tied to a button.

---

## 4. Source of Truth

The workspace-first layer must build on these existing, already-merged, read-only mechanisms — not
duplicate them:

| Mechanism | Where | What it already gives us (read-only) |
| --- | --- | --- |
| `detect_chain_state` | `scripts/a2-ide-harness.sh` | `not-started \| preview-ready \| approval-ready \| apply-bundle-ready \| applied \| unknown` |
| `print_next_step_hint` | `scripts/a2-ide-harness.sh` | per-state operator next-step guidance text |
| `find-artifacts` | helper subcommand | lists `.claw` artifacts + next-step hint |
| `audit-workspace` | helper subcommand | read-only artifact/hash audit; target hash MATCH / MISMATCH |
| `verify-final` | helper subcommand | read-only `after_sha256` MATCH / MISMATCH (exit 0 / 3) |
| argv-bounded spawn | `src/helperRunner.ts` | the single, allowlisted, no-shell spawn boundary |

**Design rule:** the panel computes setup status and *presents* chain state; it does **not** re-derive
chain semantics. Chain state comes from the helper's read-only output. The panel's own detection is
limited to **one-shot read-only existence/stat checks** (helper path, claw binary, workspace root, plan
candidates, artifact files) — never a watcher, never polling.

---

## 5. Workspace-First UX Goal

```text
Open workspace
  → inspect setup            (helper path, claw binary, workspace root)
  → discover plan candidates (read-only)
  → discover A2 artifacts    (read-only)
  → show next safe step      (from helper state + setup status)
  → print/validate commands  (existing read-only/print buttons)
  → export evidence          (existing unsaved summary)
```

The panel should make the **first 30 seconds** answer "what is this workspace, and what do I safely do
next?" without the operator typing a single path — while every *mutating* step (approval at a real
terminal, apply at a real terminal) remains exactly as manual and human as it is today.

---

## 6. Operator Journey

Target flow:

1. Operator opens a VS Code / Cursor workspace.
2. Operator opens the **A2 Harness** panel.
3. Panel automatically (on open, and on an explicit **Refresh** gesture) shows:
   - helper configured / missing
   - claw binary detected / missing
   - plan candidates found / none
   - `.claw` artifacts found / none
   - target / after_sha known / unknown
   - the next safe step
4. Operator selects or confirms the plan (auto-selected when exactly one candidate; never silently
   guessed when ambiguous).
5. Panel shows a **read-only** state — no command has run beyond the allowlisted read-only/print helper
   subcommands.
6. Operator clicks a safe action: print the next command, or validate fields.

No step in this journey runs preview / approval / apply-bundle / apply, calls a model or broker, or
writes a target.

---

## 7. Setup Status Model

The panel renders a **setup status** section near the top, computed from one-shot read-only checks:

```text
Workspace status:
- helper path:        found / missing
- claw binary:        found / missing
- workspace root:     detected / not detected
- plan.yaml:          found / select needed
- target:             known / unknown
- after_sha:          known / unknown
- preview bundle:     found / not found
- approval result:    found / not found
- apply bundle:       found / not found
- final verification: match / mismatch / not checked
```

Detection rules:

- **helper path** — resolve `a2HarnessPanel.helperPath` (relative → workspace root) and stat it; the
  basename must be `a2-ide-harness.sh` (the existing wrapper invariant). Found / missing only.
- **claw binary** — read-only presence check on PATH / configured location. This is a *capability*
  signal for the operator; the panel still never spawns `claw`.
- **workspace root** — the active VS Code workspace folder (today's `defaultWorkspace()`), confirmed to
  contain a plausible `.claw` dir and/or target. Shown, never assumed silently.
- **plan / artifacts / target / after_sha** — from read-only discovery (§8) and the helper's
  `find-artifacts` / `audit-workspace` output, never from inference.

Every status line maps to one of three honest states: a positive (found/known/match), a negative
(missing/unknown/not-found), or **not-checked** (the operator has not yet run the read-only action that
would establish it). Status is never "green by default."

---

## 8. Plan Discovery & Artifact Discovery Model

**Plan discovery** and **artifact discovery** are the same safe, read-only auto-discovery mechanism
applied to two name sets. Plan discovery locates `plan.yaml` candidate(s) under the workspace root;
artifact discovery (the deferred "Option B") locates the `.claw` chain artifacts. Both follow the rules
below; both are bounded to a known name set:

```text
preview-bundle.json
preview-generator-result.json
approval-result.json
apply-bundle.json
apply-result.json
target file
after_sha   / before_sha
preview_sha256
step_id
```

Rules (hard):

- **Read-only only.** Stat / read; never write, never create a `.claw` artifact, never mutate a target.
- **Show every discovered path before use.** Discovery proposes; the operator confirms. A discovered path
  is shown in the field table; auto-population fills the field but the operator can still see and override
  it.
- **Never silently infer unsafe paths.** Exactly-one match → may auto-select. Zero or many matches →
  surface the candidates and require an explicit pick ("select needed"). Never guess.
- **Reuse the helper.** Discovery is driven by the existing read-only `find-artifacts` / `audit-workspace`
  output where possible, so the panel parses helper stdout rather than re-walking `.claw` with its own
  logic. Any direct fs check is a one-shot existence/stat, not a watcher or poll.
- **No filesystem watching, no polling, no auto-refresh.** Discovery runs on panel open and on an explicit
  Refresh gesture only — preserving the existing "every action is one explicit gesture" invariant.

Hashes (`after_sha`, `preview_sha256`, `step_id`) are *read from* discovered artifacts to populate
read-only fields and to drive `verify-final`; they are never composed into an approval line and never
typed into the panel as approval input.

---

## 9. Next-Step State Machine

A **read-only** state machine that *guides* the operator. It runs no live A2 command; it only maps the
current setup status + helper-reported chain state to a recommended safe action.

States:

```text
NO_WORKSPACE
WORKSPACE_SELECTED
PLAN_SELECTED
INPUT_VALIDATED
NO_PREVIEW_ARTIFACTS
PREVIEW_READY
APPROVAL_RESULT_MISSING
APPROVAL_RESULT_FOUND
APPLY_BUNDLE_MISSING
APPLY_BUNDLE_FOUND
FINAL_VERIFY_READY
FINAL_MATCH
FINAL_MISMATCH
```

Each state recommends exactly one **next safe step**, drawn only from the existing safe action set:

```text
Next safe step:
- Select Plan
- Validate Input
- Print Preview Command
- Set Approval Output
- Print Approval Command       (REAL terminal; human-typed)
- Print Apply-Bundle Command
- Print Apply Command          (printed only; operator runs it themselves at a real terminal)
- Verify Final Target
```

Mapping (illustrative, not exhaustive):

| State | Source signal | Recommended next safe step |
| --- | --- | --- |
| `NO_WORKSPACE` | no workspace folder | (open a workspace) |
| `WORKSPACE_SELECTED` | workspace set, no plan | Select Plan |
| `PLAN_SELECTED` | plan set, not validated | Validate Input |
| `INPUT_VALIDATED` / `NO_PREVIEW_ARTIFACTS` | helper `not-started` | Print Preview Command |
| `PREVIEW_READY` | helper `preview-ready` | Set Approval Output → Print Approval Command |
| `APPROVAL_RESULT_FOUND` / `APPROVAL_RESULT_MISSING` | helper `approval-ready` | Print Apply-Bundle Command |
| `APPLY_BUNDLE_FOUND` / `APPLY_BUNDLE_MISSING` | helper `apply-bundle-ready` | Print Apply Command |
| `FINAL_VERIFY_READY` | helper `applied` | Verify Final Target |
| `FINAL_MATCH` / `FINAL_MISMATCH` | `verify-final` exit 0 / 3 | done / STOP and investigate |

**The state machine never executes a chain command.** A `Print *` recommendation prints/copies the
command; the operator runs preview/approval/apply themselves at a real terminal. The machine's job ends
at "here is the next safe step."

---

## 10. Visual Diff / Preview Readiness

A read-only **visual diff / preview readiness** affordance, to make "is this preview safe to act on?"
legible without running anything:

- When a `preview-bundle.json` / `preview-generator-result.json` is discovered, the panel may render a
  **read-only** view of the proposed change (the after_file relative path, `before_sha` / `after_sha`,
  `preview_sha256`, `step_id`) parsed from the existing artifacts.
- A read-only visual diff of `before` vs `after` content *may* be shown if (and only if) those contents
  are already present in discovered artifacts — the panel reads them; it never regenerates a preview and
  never calls a model/broker to produce one.
- "Preview readiness" is a status, not an action: it tells the operator whether the artifacts needed for
  the next print step exist and are internally consistent (hashes present, after_file relative). It never
  approves, never applies.

This stays inside the print/validate-only boundary: rendering an existing artifact is read-only;
producing a new preview is out of scope (that is a live chain step the operator runs themselves).

---

## 11. Evidence Timeline

Extend the existing Export Evidence Summary toward an **evidence timeline** — a read-only, session-local
record of the safe actions taken:

```text
Evidence timeline (read-only, session-local):
  [t0] workspace detected:        <path>
  [t1] helper path:               found
  [t2] plan selected:             <plan.yaml>
  [t3] validate-input:            exit 0
  [t4] find-artifacts:            preview-ready
  [t5] print-preview:             command printed (not run)
  [t6] verify-final:              MATCH / MISMATCH / not checked
```

Rules:

- Built only from helper subcommand results and field-set gestures already captured in `session`.
- Exported as an **unsaved** untitled markdown document (today's behavior); the panel writes no file.
- Records that print steps were *printed, not run*; never fabricates a live-apply event.
- No timestamps are required to be wall-clock-accurate; ordering and exit codes are the load-bearing
  evidence.

---

## 12. Non-Goals

```text
- No live apply.
- No auto-approval.
- No hidden apply.
- No model / broker / runtime call.
- No raw :11434 app inference.
- No approval phrase capture from the webview.
- No production target writes.
- No replacement for the real-terminal approval gate.
- No filesystem watcher, no polling, no background auto-refresh.
- No claw spawn (the panel's only spawned binary remains the helper).
- No .vsix packaging in this layer (its own future lane).
```

---

## 13. Safety Boundaries

Invariants the workspace-first layer must preserve, unchanged:

- The panel spawns **only** the helper, with an allowlisted read-only/print subcommand. It never spawns
  `claw`.
- `claw plan apply` is the only command that writes the target; the panel only **prints** it.
- Approval is human, at a **real-terminal**. The panel never composes the approval line and never captures
  it from the webview.
- No auto-approval, no hidden apply, no `--yes` / fake-TTY.
- No model / broker / runtime / `:11434` call. No secrets read or printed.
- Read-only detection is one-shot and explicit (open + Refresh); no watcher, no polling.
- Discovery never writes a `.claw` artifact and never mutates a target.
- STOP conditions stay loud: missing preview/approval-result, hash mismatch, target drift, prior apply
  marker, or an absolute / unreviewed `after_file`.

---

## 14. Candidate Implementation Surface

Likely future files (a separately-approved implementation lane only):

```text
ide/vscode/a2-harness-panel/src/*        (setup-status detection, discovery parse, state machine, render)
ide/vscode/a2-harness-panel/test/*       (unit tests for detection / state machine / discovery parsing)
docs/runbooks/a2-ide-extension-panel.md  (operator runbook update)
handoffs/a2_ide_extension_panel_workspace_first_ux_implementation_report_2026-06-08.md
```

Most of the new logic belongs in pure, testable modules (a `setupStatus` detector, a `discovery` parser
over helper stdout, a `stateMachine` mapping state → next safe step) so the existing test discipline
(49/49 unit tests, structural guards) extends cleanly. The single spawn boundary in `helperRunner.ts`
stays untouched in shape: still array-argv, still helper-basename-bounded, still read-only/print
subcommands only.

Explicitly **out of scope** (do not touch):

```text
- claw-status-panel
- Rust code
- runtime
- schemas
- services
```

---

## 15. Validation Plan

A future implementation lane should validate (offline, no live chain):

- **Unit tests** for: setup-status detection (helper found/missing, claw found/missing, workspace
  detected/not), discovery parsing (exactly-one vs zero vs many → auto-select vs select-needed),
  state-machine mapping (each state → exactly one next safe step), and a guard that the state machine can
  never recommend a `Run-*`/chain-write action.
- **Structural guards** (existing style): no `Run-*` button, no chain-write fragment in any built argv,
  helper basename invariant intact, no `fs` write, no model/broker/`:11434` reference.
- **Headless render check**: setup-status + next-safe-step sections render; Safety / Stop Gates banner
  still always-on; no `Run-*` markup.
- **Operator GUI click-through** (read-only / print only), in a **disposable** workspace, confirming:
  open → status populates → discovery proposes paths (shown before use) → next safe step shown → print
  buttons print only → target unchanged, no `.claw` written.

PASS only if no live preview/approval/apply ran, no target was modified, no `.claw` artifact was created,
and every safety gate from §13 held.

---

## 16. STOP Conditions

The implementation lane must STOP and surface (not work around) if:

- Artifact discovery would require **unsafe inference** (guessing a path that isn't an unambiguous match).
- Setup-status detection would require **spawning `claw`** or any non-helper binary.
- Any change would require the panel to **write** a `.claw` artifact, a target, or any file other than the
  unsaved evidence document.
- Any change would route an approval phrase through the webview, or add a `Run-*` / live-apply path.
- Preserving workspace-first UX would require a filesystem watcher or polling loop.
- The helper's read-only output is insufficient to determine chain state and the only alternative is the
  extension re-deriving chain semantics itself.

---

## 17. Future Lanes

```text
1. (this card) Workspace-First UX scope — docs-only.
2. Workspace-First UX implementation — separately token-gated (DRAFT prompt accompanies this card).
3. Disposable live-chain GUI artifact-backed smoke — optional, separately token-gated.
4. .vsix packaging / install flow — optional, its own lane (adds a vsce dependency).
5. Cursor-host verification — confirm the same extension format on a Cursor build host.
```

---

## 18. Final Recommendation

Adopt this workspace-first scope as the source of truth for the next A2 IDE Extension Panel layer. The
work is high-value and low-risk **because** it is predominantly a read-only presentation layer over chain
detection the helper already performs: setup status, safe artifact discovery (the deferred Option B), a
read-only next-step state machine, a read-only preview-readiness view, and an evidence timeline. None of
it adds a mutating capability; all of it reduces the manual setup that keeps the panel from feeling
assistant-like.

Proceed to a review/merge of this scope card and the accompanying DRAFT implementation prompt. Do **not**
start implementation until both are reviewed and merged, and the future lane is invoked with its exact
approval token (§ accompanying prompt). The panel must remain print/validate-only with a real-terminal,
human-typed approval gate.
