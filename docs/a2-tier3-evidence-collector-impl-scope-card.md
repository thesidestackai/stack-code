# A2 Tier 3 Scope Card — Read-Only Evidence Collector Implementation (Docs-Only)

## 1. Executive Summary

This scope card pins, **docs-only**, the exact boundaries for a future lane that will
implement the read-only Tier 3 evidence collector designed in
`docs/a2-tier3-evidence-collector-design.md`. It fixes the allowed read-only command set,
the snapshot schema version, and the test plan **before** any source or script is written.
It implements nothing: no collector, no panel, no execution control, and it describes no
action this lane performed.

The implementation lane this card scopes is the **first lane that crosses from docs into
source code**. It must not begin without explicit operator approval; this card exists so
that, when it does begin, its surface and behavior are already bounded.

## 2. Problem Statement

The contract (`docs/a2-tier3-evidence-surface-contract.md`) defines WHAT to display; the
collector design defines HOW to gather it. What remains underspecified for a safe code lane
is the *precise* read-only command surface, the pinned snapshot version, and a deterministic
test plan. Without pinning these first, an implementation could drift toward executing claw,
shelling out broadly, or fabricating readiness.

## 3. Relationship to the Contract and Collector Design

```text
docs/a2-tier3-evidence-surface-contract.md   (#123, on main)  = WHAT may be displayed
docs/a2-tier3-evidence-collector-design.md   (#124, on main)  = HOW it is gathered read-only
docs/a2-tier3-evidence-collector-impl-scope-card.md (this)    = BOUNDARIES for the code lane
future: collector implementation (CODE)                       = realizes the design within these bounds
```

This card introduces no new field, status, or label; the contract remains the single source
of truth.

## 4. Recommended Scope (for the future implementation lane)

```text
- A single read-only collector entry point (CLI subcommand or small script) that:
  - accepts a control-checkout path and an optional named worktree path as arguments;
  - emits ONE a2-tier3-evidence-snapshot.v0 JSON object to stdout;
  - exits 0 when it could observe the requested subjects, non-zero only on its own I/O error.
- Pure read-only derivation of every contract field per the collector design §5.
- Status computation per the collector design §6 (fixed status set; fail-to-UNKNOWN).
```

## 5. Non-Goals

```text
- no panel / UI rendering (separate lane)
- no execution of claw / orchestrator / validate-lane / apply-lane
- no approval, and no preapproval / non-interactive / fake-TTY approval path
- no worktree create / modify / retire / force-remove
- no target write, no .claw mutation
- no model / broker / runtime / network call; no raw :11434 app inference
- no Vault / secret read or emit
- no new field/status vocabulary beyond the contract
```

## 6. Allowed Future Touched Surfaces (implementation lane only)

```text
- a new read-only collector module/binary subcommand under rust/ (preferred), OR a read-only script
  under scripts/ if that is the lighter fit — chosen in the implementation lane, not here.
- its tests (unittest-style for scripts; stdlib test target for Rust, per repo CI conventions).
- docs updates cross-referencing this card.
```

## 7. Forbidden Surfaces

```text
- any write-capable file handle to a target or .claw artifact
- any process spawn that mutates state or runs claw/orchestrator
- runtime / services / IDE-panel execution wiring
- GitHub Actions changes beyond running the new read-only tests
- Vault / secrets
```

## 8. Pinned Read-Only Command Set

The implementation may use ONLY these read-only operations (no mutating flags):

```text
git, read-only:
  git -C <checkout> rev-parse --abbrev-ref HEAD
  git -C <checkout> rev-parse HEAD
  git -C <checkout> status --porcelain                 (clean vs dirty)
  git -C <checkout> rev-parse origin/main              (freshness compare; no fetch performed by the collector)
  git -C <checkout> worktree list --porcelain          (inventory + partial count)

filesystem, read-only:
  stat/exists + read of named-worktree .claw JSON artifacts (approval-result, preview-bundle,
    preview-generator-result, apply-bundle, checkpoint manifest, run-manifest, status)
  read of payload after.sha256; existence of after.bin
  read of the written file + sha256 cross-check (read-only digest)
  read of merged docs paths for links

EXPLICITLY DISALLOWED:
  any git write/mutating verb (commit, add, merge, rebase, hard reset, working-tree clean,
    checkout -B, branch delete, worktree add/remove, push, prune-fetch, prune-gc)
  any claw / orchestrator / validate-lane / apply-lane invocation
  any curl/wget/http client, any :11434, any broker/runtime/model endpoint
```

The collector does NOT perform a network fetch; it compares against whatever `origin/main`
ref already exists locally, and reports drift as STALE.

## 9. Pinned Snapshot Schema

```text
schema_version: "a2-tier3-evidence-snapshot.v0"   (pinned; bump only on wire-incompatible change)
shape:          as in collector design §7 (generated_from, tier3_status, fields{...}, subjects[],
                links{}, caveats[])
field set:      EXACTLY the contract §7 fields — no additions in v0.
apply_result_mode default: "stdout_only" unless a persisted apply-result file is actually observed.
unknown policy: any unobservable field serializes as null (fields) or "UNKNOWN" (statuses).
```

## 10. Test Plan (deterministic, read-only)

```text
Fixtures: synthesize a temp directory tree mimicking a worktree .claw artifact set (complete
  and intentionally-incomplete variants). Do NOT use or mutate any real smoke worktree.

Cases:
  T1 complete success set + no persisted apply-result file -> tier3 READY_WITH_NOTES, apply_result_mode stdout_only
  T2 complete success set + persisted apply-result file    -> READY, apply_result_mode persisted_file
  T3 missing approval-result.json                          -> PARTIAL; approval_result_path UNKNOWN
  T4 missing apply-bundle.json                             -> PARTIAL
  T5 malformed JSON artifact                               -> field UNKNOWN + caveat; no crash
  T6 control checkout dirty                                -> BLOCKED; blocked_reason "control checkout dirty"
  T7 origin/main ref ahead of captured base               -> STALE
  T8 no canonical worktree present                         -> canonical_success_worktree UNKNOWN; partial_smoke_count still computed
  T9 snapshot determinism                                  -> same on-disk state yields identical snapshot bytes
  T10 next_safe_action                                     -> only contract §10 fixed labels emitted

CI: tests run under the repo's existing test runner conventions (Rust stdlib test target, or
  unittest for a script) — no pytest; no network; no claw invocation.
```

## 11. Safety Invariants

```text
- read-only always; the collector opens nothing write-capable and spawns no mutating process.
- fail-to-UNKNOWN; never fabricate READY.
- honest caveats; stdout-only apply-result never reported as a persisted file.
- deterministic snapshot for a given on-disk state.
- no execution control, ever; the output is data, not a console.
```

## 12. Validation Plan (for the future implementation lane)

```text
[ ] only the pinned §8 read-only commands appear in the code (source-grep guard for disallowed verbs)
[ ] no claw/orchestrator/validate-lane/apply-lane/:11434/broker/Vault reference in the collector module
[ ] snapshot validates against the §9 pinned schema; field set == contract §7
[ ] all §10 test cases pass; T9 proves byte-determinism
[ ] tests are network-free and invoke no claw
[ ] docs cross-reference contract + design + this card
```

## 13. STOP Gates (for the future implementation lane)

```text
- STOP before adding any write-capable handle, mutating git verb, or process spawn that runs claw.
- STOP before any model/broker/runtime/:11434/Vault reference.
- STOP before introducing any execution control or panel wiring.
- STOP before push/PR/merge pending explicit operator approval.
- The implementation crosses the docs-only boundary: it requires explicit operator approval to BEGIN.
```

## 14. Options Considered and Rejected

```text
- "Have the collector run `claw plan status` to derive readiness" — REJECTED: that is execution; the
  collector must observe artifacts read-only, never drive claw.
- "Let the collector fetch origin/main to refresh freshness" — REJECTED: freshness is reported, not
  auto-resolved; the collector performs no network operation.
- "Add richer fields now (timings, per-marker detail)" — REJECTED for v0: the field set is pinned to
  the contract; richer fields require a contract amendment first.
```

## 15. Definition of Done (this docs lane)

```text
- one docs file committed pinning the read-only command set, snapshot version, and test plan
- docs-only; no source/script/Rust/runtime/panel touched
- no execution claim; no approval-gate weakening
```

## 16. Next Lane Recommendation

```text
A2 Tier 3 read-only evidence collector IMPLEMENTATION — the first non-docs lane, gated on explicit
operator approval, bounded by this card (TDD: write the §10 tests first, then the read-only collector).
```

## 17. References

```text
docs/a2-tier3-evidence-surface-contract.md            (#123, main b1a6518 history)
docs/a2-tier3-evidence-collector-design.md            (#124, main b1a6518)
handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md
handoffs/a2_tier3_orchestrator_live_smoke_runbook_2026-06-09.md
```

## 18. Status

```text
DOCS-ONLY SCOPE CARD — design-gated. No collector was built in this lane; the implementation
lane is not started and requires explicit operator approval to begin.
```
