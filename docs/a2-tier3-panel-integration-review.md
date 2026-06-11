# A2 Tier 3 Status Panel — Operator-Facing Integration Review (Docs-Only)

> **Docs / design-only.** This reviews **how** the merged read-only status panel renderer
> (`ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts`, #128) should be surfaced in
> the live VS Code extension, consuming the collector's `a2-tier3-evidence-snapshot.v0`
> (#126). It implements nothing — no panel wiring, no acquisition code, no execution
> control — and describes no action this lane performed. It exists to bound a future,
> separately-approved integration lane before any code is written.

## 1. Executive Summary

Two halves of the read-only status panel are shipped and on `main`:

```text
- the collector (rust/crates/a2-evidence-collector) emits a deterministic
  a2-tier3-evidence-snapshot.v0 read-only;
- the panel renderer (tier3EvidenceSnapshot.ts) turns that snapshot into a read-only,
  fail-closed, zero-control HTML fragment.
```

What is **not** yet wired: the live extension does not acquire a snapshot or render it. The
renderer is a pure module that nothing in the live panel calls yet. This review defines the
two integration concerns — **acquisition** (how a snapshot reaches the panel) and
**render-wiring** (how the fragment reaches the webview) — within the panel's existing
guard model, and recommends the lowest-risk path.

## 2. Current State (verified read-only from `main`)

```text
- panel.ts: show(model: RenderModel) → renderHtml(model) → webview.html. The live panel
  renders a RenderModel built by extension.ts.
- extension.ts: command → runHelper(...) (read-only) → builds a RenderModel → panel.show().
- helperRunner.ts: the ONLY module allowed to spawn a process. It spawns ONLY the
  print/validate-only helper (scripts/a2-ide-harness.sh) with an allowlisted read-only
  subcommand (help, validate-input, print-preview, find-artifacts, print-approval,
  print-apply-bundle, print-apply, verify-final, audit-workspace). NEVER claw, NEVER a shell.
- run-guards.js (now enforced in CI via #129): forbids fs/network/process/secret/approval-
  compose in every module except the single spawn in helperRunner.ts.
- tier3EvidenceSnapshot.ts (#128): pure parse + render; snapshot-only; fail-closed; zero
  execution controls. NOT referenced by render.ts / panel.ts / extension.ts yet.
```

## 3. The Integration Gap

```text
A. Acquisition: how does an a2-tier3-evidence-snapshot.v0 reach the panel, read-only?
   (The collector is a SEPARATE rust binary; the panel's only spawn point currently
    allowlists ONLY the helper script — not the collector.)
B. Render-wiring: how does the renderer's fragment become part of the webview HTML the
   panel already produces via render.ts / RenderModel?
```

## 4. Acquisition Options

### Option A — Operator-provided snapshot (no new spawn capability)

```text
- The operator runs the collector themselves (outside the panel) and provides the snapshot
  to the panel as text or a file path the panel reads via the existing helper path.
- The panel gains NO new ability to spawn the collector.
- Pros: smallest guard surface; the panel never spawns a second binary; acquisition stays
  fully operator-driven; matches status-panel scope card §8 ("collector stdout or a snapshot
  file the operator points it at").
- Cons: more operator steps; the snapshot can go stale between collector run and render
  (mitigated: the snapshot itself carries freshness — STALE — and the panel shows it).
```

### Option B — Helper-subcommand-mediated collector (single spawn point preserved)

```text
- Add a NEW read-only subcommand to the helper (scripts/a2-ide-harness.sh), e.g.
  `print-tier3-evidence`, that runs the read-only collector and prints the snapshot to
  stdout; add that subcommand to helperRunner.ALLOWED_SUBCOMMANDS.
- The panel still spawns ONLY the helper (one binary basename), preserving the guard model;
  the collector is read-only, so this does not add execution capability.
- Pros: in-panel refresh; keeps the "only helperRunner spawns, only the helper binary" rule;
  the collector's read-only guarantee is inherited.
- Cons: touches the helper script + the allowlist (a guard-relevant change requiring its own
  scoped lane + tests); slightly larger surface than Option A.
```

### Recommendation

```text
Start with Option A (operator-provided snapshot): it ships the operator-visible value with
the smallest guard surface and zero new spawn capability. Option B (helper-subcommand) is a
reasonable later enhancement IF in-panel refresh is wanted — but it is a separate,
guard-reviewed lane (helper allowlist change + argv-audit tests), not a prerequisite.
Either way, acquisition NEVER runs claw and NEVER spawns from a guard-safe module.
```

## 5. Render-Wiring (design only)

```text
- Add a Tier3EvidenceView (or reuse the renderer's view model) as an OPTIONAL field on
  RenderModel, mirroring how Tier3View / FoundationView are optional today.
- extension.ts parses the acquired snapshot via parseEvidenceSnapshot(...) into the view and
  sets it on the RenderModel; render.ts includes renderEvidenceSnapshotHtml(view) output in
  a new read-only section (id "tier3-evidence-snapshot").
- When no snapshot is present, the section renders a muted "no snapshot available — run the
  read-only collector" placeholder (guidance text, NOT a button) — mirroring the existing
  "muted placeholder when no Tier 3 view is provided" pattern.
```

## 6. Hard Constraints (carried from the contract + scope card #127)

```text
- read-only always: no execution control anywhere — no Run, no Apply, no Approve, no Create
  Worktree, no Cleanup.
- snapshot-only: the panel renders a a2-tier3-evidence-snapshot.v0; it never gathers evidence
  in a guard-safe module and never re-derives a field.
- single spawn point: if a snapshot is fetched by spawning, it goes ONLY through
  helperRunner, ONLY the helper binary, ONLY a read-only subcommand — never claw, never a shell.
- fail-closed: schema_version mismatch / unparseable input → the existing "unsupported
  snapshot" notice; unknown/null → UNKNOWN/—; never fabricate readiness.
- no model / broker / runtime / network / Vault; no raw :11434 app inference.
- approval stays a human-typed terminal step (DO_NOT_RUN on the surface).
```

## 7. Test Plan (for the future integration lane)

```text
[ ] render.ts: with a Tier3EvidenceView present, the webview HTML includes the snapshot
    section; with none, the muted placeholder (no controls) renders.
[ ] extension.ts: parses an acquired snapshot string via parseEvidenceSnapshot and passes the
    view to the RenderModel; a bad/mismatched snapshot yields the fail-closed notice.
[ ] guards (run-guards.js) still PASS for all modules; if Option B, helperRunner argv-audit
    tests cover the new read-only subcommand + its flags, and prove no claw/shell.
[ ] no execution control appears in the integrated webview (no button/onclick/command/
    postMessage in the snapshot section).
[ ] panel test suite green; tsc build clean; CI (#129 a2-harness-panel.yml) runs it.
```

## 8. Non-Goals

```text
- no panel implementation in this lane (design only)
- no execution controls; no autonomous/preapproval/fake-TTY approval path
- no evidence gathering inside guard-safe modules
- no helper allowlist change here (that is Option B's own scoped lane)
- no model/broker/runtime/Vault/:11434 wiring
- no Tier 4 packaging
```

## 9. STOP Gates (for the future integration lane)

```text
- STOP before adding any execution control to the panel.
- STOP before spawning anything other than the allowlisted read-only helper from
  helperRunner (and never claw / never a shell).
- STOP before wiring evidence-gathering into a guard-safe module.
- STOP before any model/broker/runtime/Vault/:11434 reference.
- STOP before push/PR/merge pending explicit operator approval.
- The integration lane edits live IDE/panel code: it requires explicit operator approval to BEGIN.
```

## 10. Recommended Next Lanes (in order)

```text
1. Read-only panel integration — Option A (operator-provided snapshot): wire render.ts +
   extension.ts to render an acquired snapshot; tests-first; CI-gated by #129. (approval-gated)
2. (Optional) Option B helper subcommand: add a read-only print-tier3-evidence helper
   subcommand + allowlist entry + argv-audit tests, for in-panel refresh. (separate, guard-reviewed)
3. Tier 3 review workflow polish (freshness banner, partial-count surfacing).
4. Tier 4 packaging DESIGN ONLY — after explicit approval.
```

## 11. References

```text
docs/a2-tier3-evidence-surface-contract.md            (#123)
docs/a2-tier3-evidence-collector-design.md            (#124)
docs/a2-tier3-status-panel-scope-card.md              (#127)
rust/crates/a2-evidence-collector                     (#126, emits a2-tier3-evidence-snapshot.v0)
ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts (#128, the renderer to wire)
ide/vscode/a2-harness-panel (panel.ts / extension.ts / helperRunner.ts / render.ts; CI #129)
docs/a2-tier3-readonly-observability-closeout-20260610.md (#130)
```

## 12. Status

```text
DOCS-ONLY DESIGN REVIEW — integration-gated. No panel was wired in this lane; the integration
lane is not started and requires explicit operator approval to begin. Recommendation:
Option A first (smallest guard surface), Option B only if in-panel refresh is wanted.
```
