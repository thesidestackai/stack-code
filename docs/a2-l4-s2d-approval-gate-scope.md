# A2-L4-S2D Scope Card — Approval Gate (bind operator approval to the proven preview_sha256) (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT AN EXECUTION LANE.** This card scopes a future,
> separately-token-gated lane that **approves** the already-proven S2C-1d A2-L2b preview by binding
> operator intent to the exact `preview_sha256` via the existing `claw plan approve` command.
> **Approval only** — it never applies, never re-runs preview, never writes the live target, never
> fabricates or alters a hash, and runs no model/broker/runtime. This docs lane itself runs no
> approve/apply. Created 2026-06-05.
>
> Docs location note: Stack-Code keeps scope cards flat under `docs/` (e.g.
> `docs/a2-l4-s2c1d-preview-execution-scope.md`), so this card lands at `docs/a2-l4-s2d-…`.

---

## 1. Executive Summary

The S2C-1d preview ran successfully and produced a real, approval-binding `preview_sha256`. S2D
scopes the next link in the A2-L2b authority chain: a future, token-gated lane that runs the
**existing** `claw plan approve <preview-bundle.json>`, supplying the source-confirmed TTY approval
line `apply <step_id> <preview_sha256>`, to record an **approval decision bound to that exact hash**.

Approval is **not** apply: `claw plan approve` writes no target files; it emits an approval-result
artifact bound to the `preview_sha256`. The live target is only ever written later, by the separate
S2E apply lane. This card defines that approval contract, the inputs, the hash binding, the exact
approval line, the rejection gates, and the boundary — and **stops there**. It runs no approval.

---

## 2. Status and Scope

**DOCS-ONLY SCOPE CARD — NOT AUTHORIZED TO RUN APPROVE/APPLY.**

In scope: the inputs, preconditions, the exact (source-confirmed) approval command and TTY approval
line, the `preview_sha256` binding, the approval token, the approval artifact, the
approval/apply boundary, and the rejection/STOP conditions for a future S2D approval lane.

Out of scope (this card authorizes none): running `claw plan approve`/`apply`, re-running preview,
A2 apply, writing the live target, fabricating/altering `preview_sha256`, editing runner/CLI/schema/
apply code, model/broker calls, runtime/Vault mutation, raw `localhost:11434` app inference,
deleting the preview bundle or the build worktree.

---

## 3. Source of Truth

```text
docs/a2-l2b-run-plan-preview-operator-handoff.md      (approval command + TTY approval line + authority chain)
rust/crates/rusty-claude-cli/src/main.rs              (run_plan_approve dispatch; re-derives preview_sha256 and rejects mismatch)
rust/crates/a2-plan-runner/src/approval.rs            (approval-line grammar `apply <step-id> <preview_sha256>`; refuses preapproval/batch)
docs/a2-l4-s2c1d-preview-execution-scope.md + handoffs/s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md  (the preview this approves)
<preview bundle> (the proven preview-bundle.json; schema a2-l2b-preview-bundle.v1)
```

---

## 4. Proven Preview Evidence

```text
preview_bundle:  /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json
schema_version:  a2-l2b-preview-bundle.v1
step_id:         preview_target_update
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b   (live target pre-preview)
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5   (operator-reviewed after_file)
payload_sha256:  8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10   (the approval-binding hash)
preview exit:    7 (write_preview_ready); target verified UNCHANGED; no approve/apply occurred
```

---

## 5. Approval Objective

The future S2D lane approves **exactly this preview, and no other**:

```text
preview_bundle:  <the proven preview-bundle.json above>
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
step_id:         preview_target_update
```

It records an approval decision bound to that `preview_sha256`. It does not apply. It does not
re-run preview. It does not write the target.

---

## 6. Approval Command Contract

Source-confirmed (handoff + main.rs `run_plan_approve` + the preview's own `next_operator_command`):

```text
claw plan approve <preview-bundle.json>
```

`claw plan approve` is **TTY-enforced** and reads the approval line from stdin. It re-derives
`preview_sha256` from the canonical preview record + display and **rejects on mismatch** with
`record.preview_sha256` (main.rs preview-binding check). It **writes no target files** — it emits an
approval-result artifact only. The future lane runs the **built** binary by exact path
(`…/stack-code-a2-l2b-preview-cli-build-20260605/rust/target/debug/claw`), not the stale `claw` on
PATH.

> TTY note: `claw plan approve` is TTY-enforced; depending on the terminal driver, the approval line
> may require an explicit EOF after it (see `a2-l2c-operator-quickref.md`). The future execution lane
> must handle this, not guess.

---

## 7. Approval Token

The future S2D approval prompt must require this **exact** token, and STOP if missing:

```text
APPROVED: Execute S2D A2-L2b approval gate
```

Absent the exact token, the future lane reports `BLOCKED: missing required approval token.` and runs
nothing.

---

## 8. Inputs

```text
- the proven preview-bundle.json (the exact path above)
- the built claw binary (by exact path)
- the workspace root (/tmp/s2c1d_ready_to_preview_20260605_142019/workspace)
- the step_id (preview_target_update) and the expected preview_sha256 (1c856762…)
- the exact approval line (§11)
```

---

## 9. Preconditions

The future lane must verify ALL before approving (else STOP — §15):

```text
the exact approval token (§7) is present
the preview bundle exists and is unmodified (schema a2-l2b-preview-bundle.v1; preview_sha256 == 1c856762…; step_id == preview_target_update)
the bundle's checkpoint_baseline_unchanged is true
the live target is STILL unchanged since preview (sha256 == before_sha256 d646ebba…)
no prior approval/apply has occurred for this preview
no `preview_sha256` is fabricated or altered; the value used is exactly the proven one
the built claw binary is used by exact path (not the stale PATH claw)
no model/broker/runtime is required; no raw :11434
```

---

## 10. Preview Hash Binding

```text
read the preview bundle and extract preview_record.preview_sha256
compare to the expected proven hash 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
REJECT if it differs, is missing, or the bundle path/content appears modified
REJECT if the live target changed since preview (before_sha256 mismatch)
claw plan approve ADDITIONALLY re-derives preview_sha256 from the canonical record + display and
  rejects any record/display mismatch — approval is cryptographically bound, not merely trusting the file
NEVER approve any hash other than the proven 1c856762…; NEVER fabricate a hash
```

---

## 11. Operator Approval Line

Source-confirmed grammar (a2-plan-runner approval.rs: `apply <step-id> <preview_sha256>`):

```text
apply preview_target_update 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

Refused by the approval parser (do not attempt):

```text
preapproval / --yes / --auto / auto-apply
batch approval (more than one `apply ` line)
any decision other than `approved`
a step_id or preview_sha256 that does not bind to the preview record
```

The line is entered on stdin to the TTY-enforced `claw plan approve`; it must be reissued per
preview and never preapproved.

---

## 12. Output / Approval Artifact

```text
claw plan approve emits an approval-result artifact (ApprovalDecision::Approved) bound to step_id +
  preview_sha256. It writes NO target files.
The approval-result is the input to a LATER, separate S2E apply lane; it is not an apply.
The approval is reversible by simply not applying — approval alone changes no workspace file.
```

---

## 13. Approval Boundary

```text
Approval does NOT write the live target.
Approval does NOT apply changes.
Approval only binds operator intent to the exact preview_sha256.
Approval output is an approval artifact / decision record only.
Approval is reversible by not applying.
Apply remains blocked until the separate S2E lane.
```

---

## 14. Apply Boundary

```text
S2D must NEVER run apply (`claw plan apply` / `claw plan apply-bundle`).
S2D must NEVER create an apply bundle.
S2D must NEVER modify the live target file.
The S2E apply gate is a separate, separately-token-gated lane that re-verifies payload/before/after
  sha, atomically replaces exactly one file, and fails closed with rollback on mismatch.
A2 remains the only write path; the model proposes, the operator approves, A2 applies.
```

---

## 15. Rejection / STOP Conditions

The future S2D lane must STOP — escalate, never reframe — if:

```text
the exact approval token is absent
the preview bundle is missing
preview_sha256 is missing
preview_sha256 mismatches the proven 1c856762…
step_id mismatches preview_target_update
the preview bundle appears modified unexpectedly
the live target changed since preview (before_sha256 mismatch)
the approval command surface is ambiguous (use the source-confirmed `claw plan approve`)
apply is requested
a live-target write is requested
a model/broker call would be required
a raw localhost:11434 reference appears
secrets appear in the bundle/logs/output
approval would require fabricating a hash
approval would use any hash other than the proven one
```

---

## 16. Validation Plan

This scope card is validated (in review) by confirming:

```text
it is docs-only (no scripts/tests/schemas/rust/src/runtime changes)
it authorizes no approve/apply from THIS lane (the approval lane is separately token-gated)
it binds approval to the exact proven preview_sha256 1c856762… (no fabrication, no other hash)
it preserves approval != apply, and apply as a separate S2E lane
the approval command and approval line are source-confirmed (claw plan approve; apply <step_id> <preview_sha256>)
raw :11434 appears only as a rejection/prohibition pattern
```

A future S2D **approval execution prompt** (separately scoped/approved) operationalizes this card;
only that prompt, with the exact token and on the proven bundle, may run `claw plan approve`.

---

## 17. Follow-On Lanes

```text
1. Read-only review of THIS scope card, then push/merge (docs-only).
2. S2D Approval Gate Scope exact-head merge gate.
3. S2D approval execution PROMPT draft (token-gated; approval-only).
4. S2D approval execution APPROVAL gate (run the existing `claw plan approve` on the proven bundle; approval-only).
5. S2E apply gate scope/prompt draft.
6. S2E apply approval gate (A2 apply re-validates the chain and writes exactly one file).

Do NOT run approval/apply until this scope card is reviewed and merged and the approval execution
prompt is separately scoped and approved (with the exact token).
```

---

## 18. Status Block

```text
status: DOCS-ONLY SCOPE CARD — NOT AUTHORIZED TO RUN APPROVE/APPLY

This card authorizes design only.
It does not run claw plan approve/apply or any A2 apply, and does not re-run preview.
It does not write the live target and never modifies the preview bundle.
It never fabricates or alters a preview_sha256; approval binds to the exact proven 1c856762….
It does not call a model or the broker, load/switch/evict models, or clear VRAM.
It does not introduce a raw localhost:11434 app-inference path.

The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only write authority.
The model proposes; the operator reviews after-bytes; A2-L2b previews; the operator approves (S2D);
A2 applies (S2E). Approval is not apply.
Next gate: read-only operator review of this scope card.
```
