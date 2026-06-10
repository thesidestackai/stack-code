# A2 Tier 3 Scope Card — Read-Only Status Panel (Docs-Only)

## 1. Executive Summary

This scope card pins, **docs-only**, the boundaries for a future lane that will build a
**read-only** Tier 3 status panel (IDE / HQ surface) which *renders* the
`a2-tier3-evidence-snapshot.v0` produced by the merged collector
(`rust/crates/a2-evidence-collector`). It implements nothing: no panel code, no execution
control, and it describes no action this lane performed.

The panel is a **mirror**, not a console. It displays the snapshot and nothing more. The
panel implementation lane this card scopes is a **docs→IDE-surface crossing** and must not
begin without explicit operator approval; this card exists so that, when it does begin, its
surface and behavior are already bounded.

## 2. Problem Statement

The evidence chain is complete and verified: the contract defines WHAT to display
(`docs/a2-tier3-evidence-surface-contract.md`), the design defines HOW the collector
gathers it (`docs/a2-tier3-evidence-collector-design.md`), and the collector is merged and
verified-from-main, emitting a deterministic `a2-tier3-evidence-snapshot.v0`. What remains
underspecified for a safe panel lane is the precise rendering surface, the snapshot-as-sole
input rule, and the forbidden controls. Pinning these first prevents the panel from drifting
into an execution surface (Run / Apply / Approve buttons) or re-gathering evidence itself.

## 3. Relationship to the Tier 3 Chain

```text
docs/a2-tier3-evidence-surface-contract.md            (#123) = WHAT may be displayed
docs/a2-tier3-evidence-collector-design.md            (#124) = HOW evidence is gathered read-only
docs/a2-tier3-evidence-collector-impl-scope-card.md   (#125) = BOUNDARIES for the collector code
rust/crates/a2-evidence-collector                     (#126) = the read-only collector (emits the snapshot)
docs/a2-tier3-status-panel-scope-card.md              (this) = BOUNDARIES for the read-only panel
future: read-only status panel (CODE)                        = renders the snapshot within these bounds
```

The panel introduces no new field, status, or label; the contract remains the single source
of truth, and the snapshot is the single input.

## 4. Recommended Scope (for the future panel lane)

```text
- A read-only view that consumes ONE a2-tier3-evidence-snapshot.v0 object and renders:
  - the overall tier3_status summary + last_successful_smoke_at;
  - the evidence paths (approval-result, apply-bundle, checkpoint manifest, payload sha256);
  - apply_result_mode (incl. the stdout_only caveat), control_checkout_status, partial_smoke_count;
  - the per-subject status list and the caveats[] strings;
  - the next_safe_action text (display only) and doc/runbook links.
- The panel obtains the snapshot read-only (see §8); it never gathers evidence itself.
```

## 5. Non-Goals

```text
- no evidence gathering in the panel (that is the collector's job; the panel renders its output)
- no execution of claw / orchestrator / validate-lane / apply-lane
- no approval, and no preapproval / non-interactive / fake-TTY approval path
- no worktree create / modify / retire / force-remove
- no target write, no .claw mutation
- no model / broker / runtime / network call; no raw :11434 app inference
- no Vault / secret read or render
- no new field/status vocabulary beyond the contract
```

## 6. Allowed Future Touched Surfaces (panel lane only)

```text
- a new read-only panel/view under the IDE surface (ide/) OR an HQ read-only view, chosen in
  the panel lane, not here.
- its tests (per the repo's existing test conventions for that surface).
- docs updates cross-referencing this card.
```

## 7. Forbidden Surfaces

```text
- any control that triggers execution; explicitly: no Run, no Apply, no Approve,
  no Create Worktree, no Cleanup control of any kind
- any write-capable handle to a target or .claw artifact
- any process spawn that runs claw / orchestrator
- runtime / services execution wiring
- broker / model / network / :11434 calls
- Vault / secrets
- GitHub Actions changes beyond running the new read-only panel tests
```

## 8. Pinned Snapshot Input Rule

The panel's ONLY input is one `a2-tier3-evidence-snapshot.v0` object:

```text
- The panel reads the snapshot read-only — either the collector's stdout (operator-invoked)
  or a snapshot file the operator points it at. The panel does NOT shell out to claw and
  does NOT re-derive any field.
- The panel treats the snapshot as immutable: it renders fields verbatim; it never writes back.
- Unknown / null fields render as "UNKNOWN" / "—"; the panel never fabricates a value.
- schema_version MUST equal "a2-tier3-evidence-snapshot.v0"; a mismatch renders a single
  "unsupported snapshot version" notice and nothing else (fail-closed, no guessing).
```

## 9. Rendering Map (snapshot field → display)

```text
tier3_status            -> top status badge (READY / READY_WITH_NOTES / BLOCKED / PARTIAL / STALE / UNKNOWN / DO_NOT_RUN)
last_successful_smoke_at -> "Last proven run" timestamp (or "—")
canonical_success_worktree -> evidence location (path text; link only to docs, never an action)
last_written_file       -> "Written file" text
approval_result_path / apply_bundle_path / checkpoint_manifest_path -> evidence path list (text)
payload_sha256          -> digest text
apply_result_mode       -> badge; when stdout_only, show the caveat string from caveats[]
control_checkout_status -> badge (clean/dirty/unknown)
partial_smoke_count     -> single count
next_safe_action        -> guidance text ONLY (never a clickable action)
blocked_reason          -> shown only when present
subjects[]              -> per-subject status rows
links{}                 -> doc/runbook links (read-only navigation)
caveats[]               -> caveat notices
```

## 10. Warning / Blocked Rendering

```text
- tier3_status BLOCKED      -> blocked banner + blocked_reason; no action offered.
- tier3_status STALE        -> "re-verify freshness" notice; no auto-refresh control.
- apply_result_mode stdout_only -> caveat notice (expected; not an error).
- unsupported schema_version -> single "unsupported snapshot version" notice, nothing else.
- missing snapshot          -> "no snapshot available — run the read-only collector" guidance text
                               (instruction to the human; NOT a button that runs it).
```

## 11. Safety Invariants

```text
- read-only always; the panel renders a snapshot and exposes no execution path.
- snapshot is the sole input; the panel never gathers evidence or calls claw.
- next_safe_action is display-only text; "Run operator-terminal smoke" is guidance, never wired.
- fail-closed on version mismatch / missing snapshot; never fabricate readiness.
- no model / broker / runtime / Vault / :11434 wiring, ever.
```

## 12. Validation Plan (for the future panel lane)

```text
[ ] panel consumes only a a2-tier3-evidence-snapshot.v0 object; no claw/orchestrator invocation in the panel
[ ] no execution control (Run / Apply / Approve / Create Worktree / Cleanup) anywhere in the panel
[ ] no write-capable handle, no .claw mutation, no model/broker/runtime/:11434/Vault reference
[ ] every rendered value maps to a §9 field; unknown/null render as UNKNOWN/— (no fabrication)
[ ] schema_version mismatch renders the fail-closed notice and nothing else
[ ] next_safe_action and "Run operator-terminal smoke" are display-only text, not controls
[ ] tests run under the surface's existing test conventions; network-free; no claw
[ ] docs cross-reference contract + design + collector scope card + this card
```

## 13. STOP Gates (for the future panel lane)

```text
- STOP before adding any execution control or any process spawn that runs claw / orchestrator.
- STOP before any write-capable handle, .claw mutation, or model/broker/runtime/:11434/Vault reference.
- STOP before wiring the panel to gather evidence itself (it must consume the snapshot only).
- STOP before push/PR/merge pending explicit operator approval.
- The panel lane crosses the docs-only boundary into the IDE/HQ surface: it requires explicit
  operator approval to BEGIN.
```

## 14. Options Considered and Rejected

```text
- "Let the panel run the collector / claw to refresh live" — REJECTED: the panel renders a
  snapshot read-only; invoking tools from the panel would make it an execution surface.
- "Add a one-click Approve/Apply from the panel" — REJECTED outright: approval is a human-typed
  terminal step (DO_NOT_RUN on this surface); no execution control may exist in the panel.
- "Add an auto-refresh that re-derives freshness" — REJECTED: freshness is reported by the
  snapshot; the panel does not re-compute or fetch.
```

## 15. Definition of Done (this docs lane)

```text
- one docs file committed pinning the panel's snapshot-only input, rendering map, and forbidden controls
- docs-only; no panel/IDE/source/runtime touched
- no execution claim; no approval-gate weakening
```

## 16. Next Lane Recommendation

```text
A2 Tier 3 read-only status PANEL IMPLEMENTATION — the docs→IDE-surface crossing, gated on
explicit operator approval, bounded by this card (TDD: render-from-snapshot tests first, then
the read-only view). Begin only after the merged collector remains smoke-verified from main.
```

## 17. References

```text
docs/a2-tier3-evidence-surface-contract.md            (#123)
docs/a2-tier3-evidence-collector-design.md            (#124)
docs/a2-tier3-evidence-collector-impl-scope-card.md   (#125)
rust/crates/a2-evidence-collector                     (#126, emits a2-tier3-evidence-snapshot.v0)
handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md
```

## 18. Status

```text
DOCS-ONLY SCOPE CARD — design-gated. No panel was built in this lane; the panel implementation
lane is not started and requires explicit operator approval to begin.
```
