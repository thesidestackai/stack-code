# A2 Tier 3 Read-Only Evidence Collector — Design — 2026-06-10

> **Docs / design-only.** This document specifies **how** a read-only collector should
> gather the fields defined by the Tier 3 Evidence Surface Contract
> (`docs/a2-tier3-evidence-surface-contract.md`). It defines a design only. It implements
> no collector, adds no panel, introduces no execution control, and describes no action
> this lane performed. No live smoke run, no `apply-lane` run, no worktree creation, and no
> cleanup occurred in authoring this design.

---

## 1. Executive Summary

The evidence surface contract defines *what* an operator cockpit may display about Tier 3.
This design defines *how* a future **read-only collector** assembles those fields from
on-disk evidence and git state, and emits a single immutable snapshot that a surface
(IDE / HQ / cockpit) can render without ever executing anything.

The collector is a **pure observer**: it reads git state, the `.claw` artifact tree inside
a named worktree, and merged docs; it computes statuses; it writes one snapshot object to
stdout (and optionally a snapshot file in a future lane). It never runs claw, never runs
the orchestrator, never approves, never writes a target, and never calls a model, broker,
runtime, or Vault.

This document is the second lane in the contract's roadmap (after the contract itself and
before any status-panel work). It is design-only; the collector implementation is a
separate, explicitly-approved future lane.

---

## 2. Relationship to the Evidence Surface Contract

```text
docs/a2-tier3-evidence-surface-contract.md   = WHAT may be displayed (fields, statuses, boundaries)
docs/a2-tier3-evidence-collector-design.md   = HOW those fields are gathered read-only (this doc)
future: read-only collector implementation   = code that realizes this design (separate lane)
future: read-only status panel               = renders the snapshot (separate lane)
```

The collector MUST NOT introduce any field, status, label, or display that the contract
does not already define. If a new field is needed, the contract is amended first (docs
lane), then this design, then the implementation — never the reverse.

---

## 3. Design Principles

```text
1. Read-only always        — the collector opens nothing write-capable and spawns no mutating process.
2. Fail to UNKNOWN         — any field that cannot be observed read-only resolves to UNKNOWN, never a guess.
3. No execution            — the collector never runs claw / orchestrator / validate-lane / apply-lane.
4. No capability surface   — the collector emits data; it exposes no Run / Apply / Approve / Create Worktree path.
5. Honest caveats          — stdout-only apply-result is reported as apply_result_mode = stdout_only, never as a persisted file.
6. Deterministic snapshot  — given the same on-disk state, the collector yields the same snapshot.
7. Path-scoped             — the collector reads only the control checkout and an explicitly-named worktree path.
```

---

## 4. Allowed Read-Only Data Sources

The collector may read **only** these sources, and only via non-mutating operations:

```text
A. Control checkout git state
   - branch / HEAD / clean-vs-dirty (e.g. `git status --porcelain`, `git rev-parse`) — read-only inspection.
   - origin/main tip for freshness comparison (read-only; no network mutation beyond a fetch the operator already ran).

B. Named worktree .claw artifact tree (the canonical success worktree, or an operator-named worktree)
   - JSON artifacts: approval-result.json, preview-bundle.json, preview-generator-result.json,
     apply-bundle.json, checkpoint manifest.json, run-manifest.json, status.json.
   - payload digests: after.bin presence + after.sha256 contents.
   - the written file (e.g. SMOKE_NOTES.md) presence + its sha256.

C. git worktree inventory
   - the list of worktrees (read-only) to count partial smoke worktrees and locate the canonical success.

D. Merged docs (for links + provenance)
   - the closure doc and runbook paths, surfaced as links/text only.
```

Explicitly **not** data sources: any model, broker, runtime endpoint, `:11434`, `/v1/chat/completions`,
`/status/vram`, Vault, or any secret store. The collector never reads or emits a secret.

---

## 5. Field-by-Field Derivation (read-only)

Each contract §7 field, and how the collector derives it without executing anything:

| field | read-only derivation |
|---|---|
| `tier3_status` | computed from the §6 model over the subjects in §6 of the contract (see §6 below). |
| `last_successful_smoke_at` | timestamp parsed from the canonical worktree's `run-manifest.json` / `status.json`, or the closure doc date; UNKNOWN if absent. |
| `canonical_success_worktree` | the worktree path whose `.claw` tree contains a complete success artifact set (see §6); UNKNOWN if none. |
| `last_written_file` | the written-file path recorded in the apply evidence (e.g. `target_relative_path` in the stdout apply-result capture or apply-bundle); UNKNOWN if absent. |
| `approval_result_path` | path to `approval-result.json` if present under the worktree `.claw`; UNKNOWN if absent. |
| `apply_bundle_path` | path to the per-step `apply-bundle.json` if present; UNKNOWN if absent. |
| `checkpoint_manifest_path` | path to the per-step checkpoint `manifest.json` if present; UNKNOWN if absent. |
| `payload_sha256` | contents of `after.sha256` (and/or recomputed read-only digest of the written file for cross-check); UNKNOWN if absent. |
| `apply_result_mode` | `persisted_file` only if an `apply-result*.json` file is found; else `stdout_only` when the written file + preflight artifacts exist; else `unknown`. |
| `control_checkout_status` | `clean` / `dirty` from a read-only porcelain check of the control checkout; `unknown` if not observable. |
| `partial_smoke_count` | count of smoke worktrees that exist but lack a complete success artifact set. |
| `next_safe_action` | mapped from `tier3_status` to one of the contract §10 fixed labels (see §7 mapping). |
| `blocked_reason` | populated only when a subject resolves to BLOCKED (e.g. control checkout dirty). |

`apply_result_mode` defaults to `stdout_only` for the current proven build; the collector
emits `persisted_file` only on an actually-observed file, never by assumption.

---

## 6. Status Derivation Logic

The collector computes each subject's status from observed evidence using the contract's
fixed set (READY / READY_WITH_NOTES / BLOCKED / PARTIAL / STALE / UNKNOWN / DO_NOT_RUN):

```text
control checkout:
  clean                -> READY
  dirty                -> BLOCKED (blocked_reason = "control checkout dirty")
  not observable       -> UNKNOWN

orchestrator script availability:
  script file present   -> READY
  absent                -> UNKNOWN

approval gate:
  always               -> DO_NOT_RUN  (approval is a human-typed terminal step; never an action of this surface)

canonical success evidence (per named worktree):
  complete success set present AND no persisted apply-result file
        -> READY_WITH_NOTES (apply_result_mode = stdout_only)
  complete success set present AND a persisted apply-result file present
        -> READY
  some-but-not-all success artifacts present
        -> PARTIAL
  none present
        -> UNKNOWN

current disposable worktree (freshness):
  base origin/main unchanged since evidence captured -> READY (or READY_WITH_NOTES)
  origin/main advanced past captured base            -> STALE
  not observable                                     -> UNKNOWN
```

**"Complete success artifact set"** (the bar for a canonical success) =
`approval-result.json` + per-step `apply-bundle.json` + `preview-bundle.json` +
`preview-generator-result.json` + checkpoint `manifest.json` + payload `after.bin` +
`after.sha256` + run `run-manifest.json` + `status.json` + the written file present, with
sha256 cross-check passing. Missing any one demotes the worktree to PARTIAL.

`tier3_status` is the roll-up: BLOCKED if any blocking subject is BLOCKED; else STALE if a
freshness subject is STALE; else READY_WITH_NOTES if the canonical success carries the
stdout-only caveat; else READY; else PARTIAL/UNKNOWN as evidence allows.

---

## 7. Snapshot Output Schema (read-only)

The collector emits one JSON snapshot object the surface consumes verbatim. Proposed shape
(`a2-tier3-evidence-snapshot.v0`, design draft — the version is pinned when implemented):

```json
{
  "schema_version": "a2-tier3-evidence-snapshot.v0",
  "generated_from": { "control_checkout": "<path>", "named_worktree": "<path-or-null>" },
  "tier3_status": "READY_WITH_NOTES",
  "fields": {
    "last_successful_smoke_at": "2026-06-09",
    "canonical_success_worktree": "<path>",
    "last_written_file": "SMOKE_NOTES.md",
    "approval_result_path": ".claw/approval-result.json",
    "apply_bundle_path": ".claw/l2b-preview-bundles/<run>/<step>/apply-bundle.json",
    "checkpoint_manifest_path": ".claw/l2b-checkpoints/<run>/<step>/manifest.json",
    "payload_sha256": "<digest-or-null>",
    "apply_result_mode": "stdout_only",
    "control_checkout_status": "clean",
    "partial_smoke_count": 11,
    "next_safe_action": "Review evidence",
    "blocked_reason": null
  },
  "subjects": [ { "subject": "control checkout", "status": "READY" } ],
  "links": { "closure_doc": "handoffs/...closure_2026-06-10.md", "runbook": "handoffs/...runbook_2026-06-09.md" },
  "caveats": [ "apply-result evidenced on stdout only; no persisted apply-result.json file on this build" ]
}
```

`next_safe_action` mapping (status → contract §10 fixed label):

```text
READY / READY_WITH_NOTES -> "Review evidence"  (or "Open runbook")
STALE                    -> "Do not run — evidence incomplete"  (re-verify freshness first)
BLOCKED                  -> "Do not run — evidence incomplete"  (with blocked_reason)
PARTIAL                  -> "Do not run — evidence incomplete"
UNKNOWN                  -> "Review evidence"
(retirement prompts)     -> "Cleanup requires explicit operator approval"
(roadmap)                -> "Proceed to read-only collector design"
```

The snapshot is **immutable output**: the surface renders it; it never writes back through
the collector, and the collector exposes no callback that performs an action.

---

## 8. Freshness / Staleness Rules

```text
- The collector records the base (origin/main tip) the evidence was captured against.
- If the current origin/main tip differs, the relevant subject is STALE — the evidence is
  still real, but the operator must re-verify before trusting "readiness".
- Freshness is reported, never auto-resolved: the collector does not fetch-and-rebase, does
  not re-run anything, and does not mutate state to "refresh".
```

---

## 9. Failure & Degradation Behavior

```text
- Missing artifact            -> that field = UNKNOWN; the subject may demote to PARTIAL.
- Unreadable / malformed JSON -> that field = UNKNOWN; record a caveat string; never crash the surface.
- Worktree path absent        -> canonical_success_worktree = UNKNOWN; partial_smoke_count still computed if listable.
- Control checkout unreadable -> control_checkout_status = unknown; tier3_status conservative (not READY).
- Any ambiguity              -> prefer the safer status (UNKNOWN/BLOCKED over READY).
```

The collector never invents a value, never "best-guesses" readiness, and never escalates
its own permissions to discover a value.

---

## 10. Hard Safety Boundaries

The collector (and this design) explicitly do **not**, and an implementation MUST NOT:

```text
- run claw / orchestrator / validate-lane / apply-lane, or run live A2
- approve anything, or introduce any preapproval / non-interactive / fake-TTY approval path
- create, modify, retire, or force-remove any worktree
- write any target file, or mutate any .claw artifact
- call a model, broker, runtime, or network endpoint; no raw :11434 app inference
- read or emit Vault / secret material
- expose any Run / Apply / Approve / Create Worktree control
- push, open a PR, merge, or delete a branch
```

The collector's only outputs are a read-only snapshot object and a non-zero/zero exit code
indicating whether it could observe the requested subjects.

---

## 11. Non-Goals

```text
- not a collector implementation (design only; code is a separate approved lane)
- not a status panel (rendering is a separate lane)
- not a new field/status vocabulary (the contract is the single source of truth)
- not an execution surface of any kind
- not a freshness auto-resolver (freshness is reported, not fixed)
```

Authoring this document performed none of the above; it only read the merged contract +
evidence and created this one design doc.

---

## 12. Future Implementation Lanes (recommended order)

```text
1. Evidence surface contract docs            (done — on main)
2. Read-only evidence collector design        (this document)
3. Read-only evidence collector implementation (realizes §5–§9; emits the §7 snapshot; strictly read-only)
4. Read-only Tier 3 status panel              (renders the snapshot; no controls)
5. Operator review workflow polish            (links, freshness banners, partial-count surfacing)
6. Tier 4 design                              (only after read-only observability is stable)
```

Each lane stays within the read-only boundary until a separately-scoped, explicitly-approved
lane proposes otherwise.

---

## 13. Validation Checklist (for the future implementation)

```text
[ ] collector opens nothing write-capable and spawns no mutating process
[ ] every field maps to a §5 read-only derivation; no value is guessed
[ ] missing/unreadable evidence resolves to UNKNOWN (never a fabricated READY)
[ ] apply_result_mode = stdout_only unless a persisted apply-result file is actually observed
[ ] "complete success artifact set" is enforced before declaring a canonical success
[ ] partial_smoke_count counts non-complete smoke worktrees; never presents one as canonical
[ ] control checkout status is independent of worktree evidence
[ ] origin/main drift renders STALE, not READY
[ ] next_safe_action uses only contract §10 fixed labels
[ ] snapshot exposes no execution control and no model/broker/runtime/Vault/:11434 reference
[ ] output is a deterministic, immutable snapshot for a given on-disk state
```

---

*Authored read-only from `docs/a2-tier3-evidence-surface-contract.md` and the merged
evidence docs. Design-only; no source, script, Rust, runtime, or panel code was touched,
and no collector was built in this lane.*
