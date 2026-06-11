# A2 Tier 3 Panel Integration — Option B Design Scope (docs-only)

Status: DESIGN SCOPE ONLY — Option B is **not implemented** in this lane.
Date: 2026-06-11
Base: `origin/main` (Option A merge `bcf7c8a5a945e6260b54189492d7afeb79dcbda8`, PR #133)
Lane branch: `docs/a2-tier3-panel-integration-option-b-design-20260611`

---

## 1. Executive Summary

Option A is live on `main` and renders **operator-provided** Tier 3 evidence
snapshots in the A2 Harness panel. The operator runs the read-only
`a2-evidence-collector` binary themselves, then pastes the resulting
`a2-tier3-evidence-snapshot.v0` JSON into the panel
(`a2HarnessPanel.pasteEvidenceSnapshot`); the pure, fail-closed
`tier3EvidenceSnapshot.ts` view model renders it read-only with zero controls.

Option B should **only design** a read-only in-panel *refresh* path so the
operator can pull the existing Tier 3 evidence/status snapshot without manual
copy/paste — while preserving every strict read-only boundary Option A
established.

**This design does not implement Option B.** It commits two docs only.

Any `helperRunner.ts` allowlist change is a **future implementation risk** and
requires its **own approval token** before a single line of source is touched.
The exact token is defined in §15 and in the companion implementation-prompt
draft.

---

## 2. Current State: Option A (verified read-only this lane)

Files wired by the Option A merge `bcf7c8a` (read-only review):

- `ide/vscode/a2-harness-panel/src/extension.ts`
  - `pasteEvidenceSnapshot()` — `showInputBox` captures operator-pasted text into
    `session.evidenceSnapshotText`; **spawns nothing, reads no file, runs no
    helper subcommand**.
  - `buildEvidenceSnapshotView()` — sole input is the session text; calls
    `parseEvidenceSnapshot(text)`; returns `null` (muted placeholder) when empty.
  - Command registered: `a2HarnessPanel.pasteEvidenceSnapshot`.
- `ide/vscode/a2-harness-panel/src/render.ts`
  - Renders the snapshot section; empty state instructs the operator to run the
    collector themselves and paste output. "The panel obtains nothing on its own
    and shows no control here."
- `ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts` (reused **unchanged**)
  - PURE, read-only: no fs, no spawn, no network, no watcher, no timer.
  - `parseEvidenceSnapshot(raw)` → `viewFromSnapshot()` → fail-closed view.
  - `EVIDENCE_SNAPSHOT_SCHEMA = "a2-tier3-evidence-snapshot.v0"`; a missing or
    mismatched `schema_version` renders a single "unsupported snapshot" notice.
  - Exposes ZERO execution controls; `next_safe_action` is display-only text.

Established Option A invariants (carried forward as Option B constraints):

```
operator-provided snapshot only        read-only
fail-closed                            zero-control section
single spawn boundary unchanged        helperRunner.ts untouched
panel.ts untouched                     tier3EvidenceSnapshot.ts reused unchanged
no executor spawn                      no file writes
no worktree creation                   no approval/apply/create controls
```

### Single spawn boundary (the surface Option B builds on)

`ide/vscode/a2-harness-panel/src/helperRunner.ts` is the ONLY module in the
package allowed to spawn a process (enforced by `scripts/run-guards.js` and
`test/guards.test.ts`: every non-`helperRunner.ts` source file is
process-spawn-free; the package uses no `fs` at all). It is already wired and
used at runtime for `help`, `audit-workspace`, and the `print-*` subcommands via
`runSubcommand()` in `extension.ts`. Its boundaries:

- `ALLOWED_SUBCOMMANDS` — exact read-only/print allowlist (no
  run/approve/apply-bundle/apply executor verb).
- `ALLOWED_FLAGS[sub]` — per-subcommand flag allowlist; unknown flags refused.
- `HELPER_BASENAME = "a2-ide-harness.sh"` — only this basename may spawn.
- array-argv only, `shell: false`, no exec/eval/sync-spawn (even inside
  `helperRunner.ts`, per `HELPER_RUNNER_FORBIDDEN`).
- `CHAIN_WRITE_FRAGMENTS` refused in helper path and any flag value.

### Helper script boundary

`scripts/a2-ide-harness.sh` is print/validate-only by design: it "makes NO
model/broker/runtime call" and "never executes claw". It has **no** Tier 3
evidence subcommand today; its subcommands are the A2-L2b chain
(`validate-input`, `print-preview`, `find-artifacts`, `print-approval`,
`print-apply-bundle`, `print-apply`, `verify-final`, `audit-workspace`).

### Implementation gap (what Option B must close)

There is no in-panel path that obtains the Tier 3 snapshot. The collector
(`rust/crates/a2-evidence-collector`, read-only, emits
`a2-tier3-evidence-snapshot.v0` to stdout, **not** `claw`) must be run manually
and pasted. Option B adds a read-only refresh that fetches the existing snapshot
text through the **existing** spawn boundary and feeds it into the **existing**
pure parser — adding no new spawn/fs surface to the panel.

---

## 3. Option B Goal

```
refresh existing Tier 3 read-only evidence in-panel
avoid operator paste
use a helper script subcommand that prints JSON/text only
do not run collector/orchestrator/validate/apply  (as control actions in the panel)
do not create worktrees
do not write files
do not mutate .claw artifacts
do not add apply/approve/create controls
```

Desired future operator experience:

```
Open A2 Harness panel
→ click/read a safe refresh path
→ helper prints existing Tier 3 evidence/status snapshot
→ panel parses existing snapshot (existing pure parser)
→ panel renders Tier 3 evidence status read-only
→ no write/create/apply/approve controls appear
```

Option B must remain read-only. It must not become a write-capable executor.

---

## 4. Source of Truth

- Option A merge: `bcf7c8a5a945e6260b54189492d7afeb79dcbda8` (PR #133) on `main`.
- `docs/a2-l4-ide-extension-panel-scope.md` §7, §14, §16 (panel safety model).
- `docs/a2-tier3-status-panel-scope-card.md` (snapshot schema + fail-to-UNKNOWN).
- `ide/vscode/a2-harness-panel/src/helperRunner.ts` (allowlist boundary).
- `ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts` (`a2-tier3-evidence-snapshot.v0`).
- `scripts/a2-ide-harness.sh` (print/validate-only helper, no-claw invariant).
- `rust/crates/a2-evidence-collector` (read-only snapshot producer).

---

## 5. Proposed Helper Subcommand

A single new read-only/print subcommand:

```
print-tier3-evidence   --workspace <path>
```

It must be read-only and must only print existing evidence/status information.
It must NOT call any of:

```
collector (as a write/mutating action)   orchestrator
validate-lane                            apply-lane
claw plan run                            claw plan approve
claw plan apply-bundle                   claw plan apply
```

It must preserve the helper's existing invariants: no `claw` execution, no
model/broker/runtime call, no target write, no `.claw` mutation.

### Acquisition sub-decision (future implementer must pick ONE, guard-reviewed)

- **B-1 (lowest-risk increment, recommended default):** `print-tier3-evidence`
  reads an **already-generated** snapshot artifact under the workspace `.claw`
  read-only (the operator runs the collector once) and prints it verbatim. Adds
  **no** new binary-spawn surface inside the helper — pure read + print.
- **B-2 (best removes paste, higher review cost):** `print-tier3-evidence`
  invokes the read-only `a2-evidence-collector` binary (which is **not** `claw`)
  and prints its stdout. This introduces a *new* binary invocation inside the
  helper and therefore requires explicit guard review that (a) the collector is
  read-only, (b) it is invoked array-argv with no shell, (c) it is never `claw`,
  (d) `would_write_files`/`would_create_worktree` stay false. If chosen, the
  collector binary basename must itself be allowlisted in the helper.

Either way, the **panel** gains no new spawn/fs surface — the snapshot text
arrives as helper stdout through the existing `helperRunner` boundary.

---

## 6. Proposed Output Contract

The helper must print a fail-closed snapshot that the existing pure parser can
consume **without editing `tier3EvidenceSnapshot.ts`**. Two compatible options;
the future implementer picks one and documents it:

- **Preferred:** emit the existing `a2-tier3-evidence-snapshot.v0` object
  verbatim (so `parseEvidenceSnapshot` renders it unchanged), **plus** two
  additive read-only safety-assertion fields the panel may surface:
  `would_create_worktree` and `would_write_files`. Extra keys are tolerated by
  the existing tolerant parser, so the reused module stays untouched.
- **Alternative:** define a new wrapper schema `a2-tier3-evidence-print.v0` whose
  envelope carries the prompt-required fields below; this requires a future
  parser addition (a new module, NOT an edit to the reused one) and its own
  fail-closed test.

Prompt-required envelope fields (fail-closed shape):

```
schema_version
status
generated_at
source
would_create_worktree     # MUST be false
would_write_files          # MUST be false
subjects
rows
caveats
links
next_safe_action
```

Hard requirements:

```
would_create_worktree: false
would_write_files: false
```

If the underlying snapshot is missing, unparseable, or has a mismatched
`schema_version`, the helper prints a fail-closed object/notice and the panel
renders the existing "unsupported snapshot" notice — never fabricated readiness.

---

## 7. Proposed HelperRunner Allowlist Change

The ONLY future change to `helperRunner.ts` is **additive and minimal**:

```ts
// ALLOWED_SUBCOMMANDS: add exactly one entry
"print-tier3-evidence",

// ALLOWED_FLAGS: add exactly one entry, narrowest flag set
"print-tier3-evidence": ["workspace"],
```

Constraints on the change (future implementation, token-gated):

```
add EXACTLY one subcommand: print-tier3-evidence
add EXACTLY its flag set (workspace only, unless B-2 needs a read-only source flag)
add NO executor verb (no run/approve/apply-bundle/apply)
do NOT relax HELPER_BASENAME
do NOT relax CHAIN_WRITE_FRAGMENTS
do NOT remove the flag-shape / chain-write value refusals
do NOT introduce exec/eval/spawnSync/shell:true
```

This change must not appear in this design lane; it is named here only so the
future argv-audit tests can pin it.

---

## 8. Required Argv-Audit Tests

Future tests (in `test/helper_runner.test.ts` and/or a sibling) must prove:

```
helperRunner allowlist includes ONLY the exact new subcommand (print-tier3-evidence)
the new subcommand builds array-based argv: ["print-tier3-evidence","--workspace",<ws>]
shell remains false (no shell:true anywhere)
no shell-metachar path: flag-shaped and chain-write-shaped values still refused
no arbitrary subcommand forwarding (unapproved subcommand still refused pre-spawn)
no claw command forwarded (basename + chain-write fragment refusals intact)
no collector/orchestrator/validate/apply executor verb added to the allowlist
no fs/network/runtime path added to the webview (run-guards.js still PASS)
the exact-allowlist assertion test is updated to the new 10-entry sorted list
```

---

## 9. UI Behavior

A future read-only command, e.g.:

```
Refresh Tier 3 Evidence Snapshot
```

It must NOT be named like a run/apply/approve/create command. It feeds the
existing pure renderer. It must show:

```
status
subjects
rows
caveats
links
next safe action
would-create-worktree: no
would-write-files: no
```

Mechanically it is a read-only button that calls
`runSubcommand("print-tier3-evidence")` and routes `result.stdout` into
`parseEvidenceSnapshot(...)` (replacing the manual paste as the acquisition
source). The existing `pasteEvidenceSnapshot` paste path may remain as a manual
fallback. No new panel `fs`/`spawn`/network is introduced.

---

## 10. Forbidden Controls

The Option B panel section must never present:

```
Run Collector            Run Orchestrator
Run Validate Lane        Run Apply Lane
Run Preview              Run Approval
Run Apply Bundle         Run Apply
Create Worktree          Write Files
Clean Up                 Approve
```

---

## 11. Parser / Fail-Closed Behavior

```
schema_version mismatch / missing  → "unsupported snapshot" notice, nothing else
invalid JSON                       → fail-closed notice (no partial render)
null / non-object input            → fail-closed notice
missing fields                     → UNKNOWN (status) / "—" (paths); never fabricate
would_create_worktree != false     → treat as unsupported/blocked; do NOT render as ready
would_write_files != false         → treat as unsupported/blocked; do NOT render as ready
no execution control rendered under any branch
```

The reused `tier3EvidenceSnapshot.ts` already enforces the first four. The
`would_*` safety-flag enforcement is the only new fail-closed rule and must be
added in a NEW module/test, not by editing the reused one (preferred B-1/B-6
output keeps the reused parser untouched and surfaces the flags as display rows).

---

## 12. Guard Requirements

The future change must keep `scripts/run-guards.js` / `test/guards.test.ts`
GREEN with no relaxation:

```
no network/telemetry/broker/ollama/:11434 egress in panel src
no filesystem-watcher / polling / background refresh
no fs.* anywhere in panel src (helper does the read-only inspection)
no SecretStorage / context.secrets
no chain-write literal in live code
no approval-line composition
ONLY helperRunner.ts spawns; no exec/eval/spawnSync/shell:true
```

If B-2 is chosen (helper invokes the collector), the **helper-side** guard story
(no-claw, read-only, array-argv) must be documented and, if a helper guard test
exists, extended — but the **panel** guard set above must remain unchanged.

---

## 13. Validation Plan

Future implementation lane validation (no live broker/model/runtime):

```
npm test            (panel unit + render + guards tests GREEN)
node scripts/run-guards.js   (PASS, unchanged guard set)
argv-audit tests    (new print-tier3-evidence assertions GREEN)
parser fail-closed tests  (would_* flags + schema mismatch)
offline helper smoke: print-tier3-evidence --workspace <dir> prints JSON, writes nothing,
                      creates no .claw, creates no worktree, never emits `claw`
manual UI check: refresh renders read-only, zero controls
```

No collector/orchestrator/validate/apply lane is run. No model/broker/:11434
call. No Vault/secret access.

---

## 14. Non-Goals

```
NOT implementing Option B in this lane
NOT editing any source / test / package.json
NOT editing helperRunner.ts / panel.ts / render.ts / extension.ts / tier3EvidenceSnapshot.ts
NOT editing scripts/a2-ide-harness.sh
NOT adding write/apply/approve/create/cleanup controls — ever
NOT adding panel fs/network/spawn surface
NOT turning the helper into a claw executor
NOT removing the operator-paste fallback
```

---

## 15. STOP Gates

The future implementation lane must STOP (before creating a worktree) unless:

```
the exact approval token is present (see §15 token below)
control checkout is clean on main
a fresh isolated worktree from origin/main is used (not /home/suki/stack-code)
```

It must STOP and refuse mid-lane if any of these is required to proceed:

```
broadening the allowlist beyond the single print-tier3-evidence subcommand
adding any executor verb (run/approve/apply-bundle/apply)
adding shell:true / exec / eval / spawnSync
adding fs/network/broker/model/runtime/Vault path to the panel/webview
emitting/forwarding a `claw` command
a would_create_worktree / would_write_files value that is not false
any apply/approve/create/write/cleanup control appearing in the UI
```

Required approval token (exact):

```
APPROVED: Execute A2 Tier 3 panel integration Option B implementation
```

---

## 16. Risk Assessment

```
Risk: allowlist broadening creep
  → Mitigation: argv-audit exact-list test; add exactly one subcommand.

Risk: helper gains a binary-invocation surface (B-2 collector path)
  → Mitigation: prefer B-1 (read existing artifact); if B-2, array-argv + no-shell
    + collector-basename allowlist + read-only proof + would_* false assertions.

Risk: snapshot source could imply a write (e.g. regenerating evidence)
  → Mitigation: contract requires would_write_files:false, would_create_worktree:false;
    parser treats non-false as unsupported/blocked.

Risk: panel acquires its own IO (fs/spawn) and breaks the single-boundary model
  → Mitigation: reuse the existing helperRunner spawn boundary only; run-guards.js
    stays unchanged and GREEN.

Risk: a refresh button reads as an action/executor
  → Mitigation: read-only naming ("Refresh … Snapshot"), zero controls, display-only
    next_safe_action.

Risk: schema drift between collector output and parser
  → Mitigation: keep a2-tier3-evidence-snapshot.v0 compatibility; fail-closed on
    mismatch; reused parser untouched.
```

---

## 17. Recommended Future Implementation Lane

```
Name:      A2 Tier 3 Panel Integration Option B — Implementation (token-gated)
Objective: Add read-only print-tier3-evidence refresh path; reuse spawn boundary
           and pure parser; zero new controls; guards stay GREEN.
Artifact:  handoffs/a2_tier3_panel_integration_option_b_implementation_prompt_DRAFT_2026-06-11.md
Gate:      Requires exact token: APPROVED: Execute A2 Tier 3 panel integration Option B implementation
Do NOT start implementation until this design scope and the token-gated prompt are
reviewed and merged.
```
