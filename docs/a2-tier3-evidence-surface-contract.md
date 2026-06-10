# A2 Tier 3 Evidence Surface Contract — 2026-06-10

> **Docs-only design contract.** This document defines how Tier 3 write-capable
> orchestrator evidence *should* be surfaced to an operator cockpit / IDE / HQ-style
> status view. It defines a **read-only evidence surface** only. It does not implement
> a UI, does not add execution controls, and describes no action this lane performed.
> No live smoke run, no `apply-lane` run, no worktree creation, and no cleanup occurred
> in authoring this contract.

---

## 1. Executive Summary

Tier 3 has crossed from *theoretical readiness* to **one successful throwaway-worktree
live apply proof**. The orchestrator drove the existing `claw plan run → approve →
apply-bundle → apply` chain to write exactly one declared file (`SMOKE_NOTES.md`) inside
a single disposable worktree, at a real operator terminal, with the control checkout
staying clean.

The next strategic need is **visibility, not more power**. The operator should be able to
answer — entirely **read-only** — questions like: *Is Tier 3 ready? What was the last
proven run? Which disposable worktree did it touch? What file was written? Where is the
evidence? Is the control checkout clean? What is safe to do next? What is explicitly not
allowed?*

This contract specifies the fields, statuses, display rules, and forbidden behaviors a
future read-only evidence surface must honor. It is the prerequisite design for any
collector or panel work; those remain **future lanes**, not part of this document.

---

## 2. Why This Exists

A live write-capable path is only safe to operate if its outcome is **legible**. Without a
disciplined evidence surface, an operator cockpit risks two failure modes:

1. **False confidence** — presenting a stale or partial run as if it were the current proven
   state, or implying a persisted artifact that does not exist (e.g. a persisted
   `apply-result.json` file — see §4).
2. **Capability creep** — a status view that quietly grows buttons (Run / Apply / Approve /
   Create Worktree) and becomes an execution surface, defeating the read-only safety model.

This contract fixes the vocabulary and the boundary *before* any pixels are drawn, so the
surface can only ever **report** evidence, never **act** on it.

---

## 3. Current Proven Tier 3 State

The following foundations exist on `origin/main` (verified read-only from merged docs and
git history at authoring time):

```text
Tier 3 orchestrator                         exists (scripts/a2-tier3-write-orchestrator.sh)
write_target.path exact-path gate           hardened
preview rc=7 artifact-gated handling        exists (accepted as write-preview-ready only with artifacts)
approval stdin/result diagnostics           exist (per-exit-code cause surfaced)
Enter-to-Approve source fix                 landed (approve completes on Enter; off-TTY still refuses)
successful live apply smoke closure         exists (one end-to-end proof, PASS_WITH_NOTES)
```

The surface treats this block as the **baseline capability inventory**. It is descriptive
only; the surface never re-derives capability by executing anything.

---

## 4. Canonical Successful Smoke Evidence

There is exactly **one** canonical successful smoke worktree. Its evidence is the
authoritative "last proven run" the surface should reference.

```text
worktree path        /mnt/vast-data/git-worktrees/stack-code-a2-tier3-live-smoke-20260609_201950-4441
branch               smoke/a2-tier3-live-smoke-20260609_201950-4441   (base origin/main)
run-id               01KTQRQHAPZN9RANMT78MQ2B45
step-id              write-smoke-notes
last successful at   2026-06-09 (drive) / 2026-06-10 (closure verification)
```

Evidence fields (paths relative to the canonical worktree):

```text
SMOKE_NOTES.md (the one written file)   SMOKE_NOTES.md   (sha256 cde471a9…79aa8)
approval-result.json                    .claw/approval-result.json
preview-bundle.json                     .claw/l2b-preview-bundles/<run>/<step>/preview-bundle.json
preview-generator-result.json           .claw/l2b-preview-bundles/<run>/<step>/preview-generator-result.json
apply-bundle.json                       .claw/l2b-preview-bundles/<run>/<step>/apply-bundle.json
checkpoint manifest                     .claw/l2b-checkpoints/<run>/<step>/manifest.json
payload after.bin                       .claw/l2b-payloads/<run>/<step>/after.bin
payload after.sha256                    .claw/l2b-payloads/<run>/<step>/after.sha256
run manifest                            .claw/l2b-runs/<run>/run-manifest.json
run status                              .claw/l2b-runs/<run>/status.json
```

**Apply-result caveat (load-bearing).** `claw plan apply` emitted an
`a2-l2b-apply-result.v1` JSON object to **stdout** (operator-captured: `exit_code 0`,
`outcome "applied"`, markers including `a2-l2b-write-applied` and
`a2-l2b-write-validated`). **No persisted `apply-result.json` file was observed.** The
surface MUST represent the apply outcome as `apply_result_mode = stdout_only` and MUST NOT
claim, link to, or imply a persisted `apply-result.json` file unless a future build
actually creates one.

**Control checkout posture.** The control checkout `/home/suki/stack-code` remained clean
throughout; the only untracked items (`.claw/` and `SMOKE_NOTES.md`) lived **inside** the
disposable worktree. The surface should report the control checkout independently of any
worktree evidence.

---

## 5. Partial Smoke Worktree Evidence Posture

At closure there were **12 total** smoke worktrees under `/mnt/vast-data/git-worktrees/`:

```text
1   canonical successful evidence worktree   (…_201950-4441)
11  partial preview/approval-failure worktrees (earlier dated attempts)
```

The partial smoke worktrees are **historical attempts** that did not reach a written file
(they stopped at preview or approval). The surface classifies them as `PARTIAL` and:

- counts them in a single `partial_smoke_count` field (do not enumerate as "runs");
- never presents a partial worktree as the canonical success;
- never proposes destructive cleanup as an automatic next step — retirement of partial
  worktrees is an explicitly operator-approved, non-force-only activity outside this surface.

This contract documents only **how to classify** these worktrees. It does not retire,
modify, or clean any smoke worktree.

---

## 6. Read-Only Evidence Classification Model

The surface assigns exactly one status per evidence subject, drawn from this fixed set:

```text
READY              proven, current, and complete
READY_WITH_NOTES   proven and current, with a documented caveat (e.g. apply_result_mode = stdout_only)
BLOCKED            a precondition fails (e.g. control checkout dirty) — do not proceed
PARTIAL            evidence exists but the run did not reach a written-file outcome
STALE              evidence exists but a freshness precondition has drifted (e.g. origin/main advanced)
UNKNOWN            the subject cannot be observed read-only right now
DO_NOT_RUN         a hard safety boundary applies — execution is not an option from this surface
```

Applied to each subject:

```text
control checkout                 READY (clean) | BLOCKED (dirty) | UNKNOWN
orchestrator script availability READY (present) | UNKNOWN (not found)
approval gate state              READY (off-TTY refusal intact) | DO_NOT_RUN (never auto-approved here)
successful smoke evidence        READY_WITH_NOTES (apply_result_mode = stdout_only)
partial smoke evidence           PARTIAL
current disposable worktree      READY | STALE | UNKNOWN  (per dated worktree, read-only)
```

`DO_NOT_RUN` on the approval gate is intentional: from this surface, approval is never an
available action — it is a human-typed step at a real terminal, surfaced as evidence only.

---

## 7. Read-Only Dashboard / Status Fields

A future UI / HQ / IDE surface may render the following fields. All are **read-only
projections** of on-disk evidence and git state; none implies an action.

| field | meaning | example value |
|---|---|---|
| `tier3_status` | overall classification | `READY_WITH_NOTES` |
| `last_successful_smoke_at` | timestamp of canonical proof | `2026-06-09` |
| `canonical_success_worktree` | path of the proven worktree | `/mnt/vast-data/git-worktrees/stack-code-a2-tier3-live-smoke-20260609_201950-4441` |
| `last_written_file` | the one declared file written | `SMOKE_NOTES.md` |
| `approval_result_path` | persisted approval-result.json | `.claw/approval-result.json` |
| `apply_bundle_path` | persisted apply-bundle.json | `.claw/l2b-preview-bundles/<run>/<step>/apply-bundle.json` |
| `checkpoint_manifest_path` | persisted checkpoint manifest | `.claw/l2b-checkpoints/<run>/<step>/manifest.json` |
| `payload_sha256` | written payload digest | `cde471a9…79aa8` |
| `apply_result_mode` | how the apply outcome is evidenced | `stdout_only` \| `persisted_file` \| `unknown` |
| `control_checkout_status` | control checkout cleanliness | `clean` \| `dirty` \| `unknown` |
| `partial_smoke_count` | number of partial smoke worktrees | `11` |
| `next_safe_action` | fixed-label guidance (see §10) | `Review evidence` |
| `blocked_reason` | populated only when status is BLOCKED | `control checkout dirty` |

`apply_result_mode` defaults to `stdout_only` for the current proven build and MUST NOT be
shown as `persisted_file` without a corresponding observed file.

---

## 8. UI / IDE / HQ Display Model

**What the surface MAY display:**

```text
- a single status summary (tier3_status + last_successful_smoke_at)
- the evidence paths from §4 and §7 (as text and as doc/runbook links)
- the last known smoke result, including the stdout apply-result caveat
- partial smoke count (one number, not an action list)
- warning / blocked banners (see §9)
- next-safe-action text using only the fixed labels in §10
- links to the closure doc and the runbook
```

**What the surface MUST NOT do** (hard boundary — these are forbidden controls):

```text
- no Run button
- no Apply button
- no Approve button
- no Create Worktree button
- no branch deletion control
- no worktree cleanup / retirement control
- no shell execution of any kind
- no model / broker / runtime calls
- no Vault / secret access (and never render a secret)
- no raw :11434 app inference
```

The surface is a **mirror**, not a console. If a field cannot be populated read-only, it
renders `UNKNOWN` — it never executes anything to discover the value.

---

## 9. Warning and Blocked States

The surface should recognize these conditions and render them as warnings or `BLOCKED`,
with a populated `blocked_reason` where applicable:

```text
control checkout dirty                         -> BLOCKED  (blocked_reason: "control checkout dirty")
origin/main advanced past tested base          -> STALE    (freshness drift; re-verify before trusting)
canonical smoke worktree missing               -> UNKNOWN  (evidence not observable read-only)
approval-result.json missing                   -> PARTIAL  (run did not reach approval persistence)
apply-bundle.json missing                      -> PARTIAL  (run did not reach apply-bundle persistence)
SMOKE_NOTES.md missing in canonical worktree   -> UNKNOWN/PARTIAL (written-file evidence absent)
only partial smoke evidence exists             -> PARTIAL  (no canonical success to reference)
persisted apply-result file expected but absent-> READY_WITH_NOTES (expected on this build: stdout-only)
partial worktrees require retirement           -> note only; retirement needs explicit operator approval
```

A missing persisted apply-result **file** is the expected steady state for the current
build and is a `READY_WITH_NOTES` caveat, **not** a failure.

---

## 10. Next-Safe-Action Language (fixed labels)

The next safe action shown to the operator is a guidance string, never an executable
verb. The `next_safe_action` field may only ever contain one of these exact strings. The surface never
invents free-form guidance and never offers an executable verb:

```text
"Review evidence"
"Open runbook"
"Run operator-terminal smoke"          (instruction to the human at a real terminal — NOT a button)
"Do not run — evidence incomplete"
"Cleanup requires explicit operator approval"
"Proceed to read-only collector design"
```

"Run operator-terminal smoke" is **descriptive guidance to a human**, pointing to the
runbook; it is never wired to a control on the surface.

---

## 11. Explicit Non-Goals

This contract and any surface built from it explicitly do **not**:

```text
- implement a collector or a panel (design only)
- introduce any execution control (Run / Apply / Approve / Create Worktree)
- weaken the approval gate (off-TTY refusal, exact step-id + preview_sha256 binding all stay)
- introduce any preapproval / non-interactive / fake-TTY approval path
- run live A2, the orchestrator, validate-lane, or apply-lane
- create, modify, retire, or force-remove any worktree
- write any target file or mutate any .claw artifact
- call model / broker / runtime / network / Vault, or use raw :11434 app inference
- push, open a PR, merge, or delete any branch
```

Authoring this document performed none of the above; it only read merged evidence and
created this one docs file.

---

## 12. Future Implementation Lanes (recommended order)

```text
1. Evidence surface contract docs        (this document)
2. Read-only evidence collector design   (how to gather §7 fields read-only; design before code)
3. IDE / HQ read-only Tier 3 status panel (renders §7/§8; strictly no controls)
4. Operator review workflow polish        (links, freshness checks, partial-count surfacing)
5. Tier 4 design                          (only AFTER read-only observability is stable)
```

Each lane stays within the read-only boundary until an explicitly approved, separately
scoped lane proposes otherwise. Tier 4 (any expansion of capability) is gated behind stable
observability, not the reverse.

---

## 13. Validation Checklist

```text
[ ] surface presents exactly one canonical success (…_201950-4441), not a partial worktree
[ ] apply_result_mode shows stdout_only; no persisted apply-result.json file is claimed
[ ] all evidence paths match §4 (approval-result, apply-bundle, checkpoint, payload, run manifest/status)
[ ] partial_smoke_count is a single number; partials are classified PARTIAL, never canonical
[ ] control_checkout_status is reported independently of worktree evidence
[ ] every status is drawn from the §6 fixed set; blocked_reason populated only when BLOCKED
[ ] next_safe_action uses only the §10 fixed labels
[ ] no execution control (Run / Apply / Approve / Create Worktree) appears anywhere
[ ] no model / broker / runtime / Vault / raw :11434 reference is wired into the surface
[ ] freshness drift (origin/main advanced) renders STALE, not READY
```

---

*Authored read-only from merged evidence:
`handoffs/a2_tier3_orchestrator_live_apply_smoke_closure_2026-06-10.md` and
`handoffs/a2_tier3_orchestrator_live_smoke_runbook_2026-06-09.md`. Docs-only; no source,
script, Rust, runtime, or panel code was touched.*
