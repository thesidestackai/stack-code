# A2-L4-S2E Scope Card — Apply Gate (write the approved preview to the live target) (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT AN EXECUTION LANE.** This card scopes a future,
> separately-token-gated lane that **applies** the proven, operator-approved A2-L2b preview by
> writing the one target file via the existing `claw plan apply` path. **Apply is the first and
> only lane in this chain permitted to write the target.** This docs lane runs no apply, no
> approval, no preview, and no model/broker/runtime. Created 2026-06-05.
>
> Docs location note: Stack-Code keeps scope cards flat under `docs/` (e.g.
> `docs/a2-l4-s2d-approval-gate-scope.md`), so this card lands at `docs/a2-l4-s2e-…`.

---

## 1. Executive Summary

S2C-1d produced a proven preview (`preview_sha256` `1c856762…805da10`); S2D scopes/executes the
operator approval that binds intent to that hash. S2E is the **final** link: it writes the
operator-reviewed `after` bytes to the live target file, but only after re-validating the full
hash chain. Apply is **not** approval and **not** preview — it is the single mutation of the chain.

This card defines the source-confirmed apply contract, its inputs, the approval-evidence
requirement, the hash bindings, the target-state validation, the apply/rollback boundary, and the
STOP conditions for a future S2E apply lane — and **stops there**. It runs no apply.

**Central finding (source-confirmed, see §6/§10):** both apply entry points require a **persisted
approval-result** (`schema a2-l2b-approval-result.v1`) — either embedded inside an apply-bundle
(`claw plan apply <apply-bundle.json>`) or supplied as a standalone file
(`claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`). There is **no**
stdout-only apply path. As of this card's creation, **no persisted approval-result artifact exists**
in the workspace `.claw` tree. Therefore S2E apply execution is **BLOCKED** until a persisted
approval-result (and an assembled apply-bundle, if using `plan apply`) exists.

---

## 2. Status and Scope

**DOCS-ONLY SCOPE CARD — NOT AUTHORIZED TO RUN APPLY/APPROVE.**

In scope: the source-confirmed apply command(s), the input artifacts apply consumes, the
approval-evidence requirement, the preview/approval/target hash bindings, target-state validation,
the apply execution boundary, the apply-completion artifact, the rollback/recovery boundary, and the
rejection/STOP conditions for a future S2E apply lane.

Out of scope (this card authorizes none): running `claw plan apply` / `claw plan apply-bundle` /
`claw plan approve` / `claw plan run`, re-running preview, writing the target, assembling/persisting
an apply-bundle or approval-result, fabricating approval evidence or any hash, editing
runner/CLI/schema/apply code, model/broker calls, runtime/Vault mutation, raw `localhost:11434`
inference, deleting the preview bundle or the build worktree.

---

## 3. Source of Truth

```text
docs/a2-l4-s2d-approval-gate-scope.md                                  (merged S2D scope; PR #83, e9617c1)
handoffs/s2d_a2_l2b_approval_execution_prompt_DRAFT_2026-06-05.md      (merged S2D approval execution prompt; PR #84, 3e20750)
handoffs/s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md     (the preview this chain applies)
rust/crates/rusty-claude-cli/src/main.rs                               (run_plan_apply / apply-bundle dispatch — the executing lane must re-read at execution time)
rust/crates/rusty-claude-cli/tests/plan_apply.rs                       (apply contract: schemas, hash re-validation, rejection cases, success markers)
rust/crates/a2-plan-runner/src/*                                       (write executor, checkpoint/payload handling)
<preview bundle> + <preview-generator-result.json>                     (the proven preview inputs)
```

**Source-confirmed CLI surface** (from the built binary's own usage strings):

```text
claw plan apply <apply-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan approve <preview-bundle.json>     (S2D — emits approval-result to stdout)
claw plan status <workspace> [<approval-result.json>]
```

**Source-confirmed apply behaviors** (`tests/plan_apply.rs`):

```text
apply-bundle schema is a2-l2b-apply-bundle.v1; an apply-bundle EMBEDS an approval_result
  (schema a2-l2b-approval-result.v1) and a payload carrying after_sha256.
the approval_result embeds preview_sha256.
apply REJECTS: wrong apply-bundle schema_version; payload.after_sha256 disagreeing with
  preview_record.after_sha256; payload bytes that do not hash to preview.after_sha256;
  approval_result.preview_sha256 mismatching the preview.
apply REJECTS pre-approval/batch flags (--yes/--auto/--force/--allow-write/--preapproved/--batch).
apply success: outcome == "applied"; markers include "a2-l2b-write-applied"; echoes preview_sha256.
```

---

## 4. Proven Preview and Approval Evidence

```text
preview_bundle:  /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json
preview-generator-result: /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-generator-result.json   (PRESENT)
schema_version:  a2-l2b-preview-bundle.v1
step_id:         preview_target_update
target (rel):    sample/preview_target.txt
target (abs):    /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/sample/preview_target.txt
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
checkpoint_run_id: 01KTCYZV5B1TYF45QX0ZRRWTNG

target current state (this card's creation): sha256 == d646ebba…cee2b  (== before_sha256; UNCHANGED; no apply has occurred)

APPROVAL EVIDENCE STATE — HONEST, AS OBSERVED:
  Upstream context claimed S2D `claw plan approve` exited 0 and printed an approval-result
  (schema a2-l2b-approval-result.v1, decision=approved, preview_sha256=1c856762…) to STDOUT.
  However: the agent lanes attempting S2D approval were BLOCKED on missing TTY and did NOT execute
  approval; AND a read-only inventory of the workspace .claw tree found NO persisted
  a2-l2b-approval-result artifact. Artifacts present are checkpoints (before.bin/manifest.json),
  payloads (after.bin/after.sha256), preview-bundle.json + preview-generator-result.json, and run
  manifests/status — but NO approval-result and NO apply artifacts.
  => Treat persisted approval evidence as ABSENT until proven otherwise (§10).
```

---

## 5. Apply Objective

The future S2E lane applies **exactly this approved preview, and no other**:

```text
preview_bundle:  <the proven preview-bundle.json above>
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
step_id:         preview_target_update
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
target:          sample/preview_target.txt  (single file, inside the disposable workspace only)
```

It writes the `after` bytes to the one target file and records an apply-completion artifact. It
applies once. It does not approve, does not re-run preview, does not write any other path.

---

## 6. Apply Command Contract

Source-confirmed; the executing lane must re-read `main.rs` to confirm exact internal field layout.

Two entry points exist:

```text
A) claw plan apply <apply-bundle.json>
   - consumes ONE apply-bundle JSON (schema a2-l2b-apply-bundle.v1).
   - the apply-bundle EMBEDS:
       * approval_result   (schema a2-l2b-approval-result.v1, embedding preview_sha256)
       * payload           (carrying after_sha256, bound to preview_record.after_sha256)
       * authority-chain fields binding to the proven preview
   - requires the apply-bundle to already be assembled and persisted.

B) claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
   - consumes the preview-generator-result.json (PRESENT in .claw) AND a standalone, persisted
     approval-result.json (schema a2-l2b-approval-result.v1).
```

**Both paths require persisted approval evidence.** Path A needs the approval-result embedded in an
assembled apply-bundle; Path B needs a standalone approval-result.json file. Neither path can
consume an approval that exists only as ephemeral stdout. `claw plan apply` is run with the built
binary by exact path (`…/stack-code-a2-l2b-preview-cli-build-20260605/rust/target/debug/claw`),
never the stale PATH `claw`. No `--yes`/`--auto`/`--force`/`--allow-write`/`--preapproved`/`--batch`.

> If, at S2E execution time, the apply command/input contract is ambiguous against `main.rs`, the
> S2E lane MUST classify apply execution as **BLOCKED pending RCA** and run nothing.

---

## 7. Apply Token

The future S2E apply prompt must require this **exact** token, and STOP if missing:

```text
APPROVED: Execute S2E A2-L2b apply gate
```

Absent the exact token, the future lane reports `BLOCKED: missing required apply token.` and runs
nothing. This token is distinct from, and additional to, the S2D approval token.

---

## 8. Inputs

```text
- the built claw binary (by exact path)
- the proven preview-bundle.json (exact path, §4)
- the preview-generator-result.json (exact path, §4 — PRESENT)
- a PERSISTED approval-result (schema a2-l2b-approval-result.v1) bound to preview_sha256 1c856762…
    (Path A: embedded in an assembled apply-bundle.json; Path B: standalone approval-result.json)
- the workspace root (/tmp/s2c1d_ready_to_preview_20260605_142019/workspace)
- the target file sample/preview_target.txt
- the expected hashes: before d646ebba…, after 8a7b6e95…, preview 1c856762…
```

---

## 9. Preconditions

The future lane must verify ALL before applying (else STOP — §16):

```text
the exact apply token (§7) is present
a persisted approval-result (schema a2-l2b-approval-result.v1, decision=approved, preview_sha256==1c856762…) exists (§10)
the apply input is assembled per the chosen source-confirmed path (§6) — apply-bundle.json (A) or preview-generator-result.json + approval-result.json (B)
the preview bundle exists, is unmodified, and binds step_id preview_target_update + preview_sha256 1c856762…
the live target currently hashes to before_sha256 d646ebba… (unchanged since preview)
the payload after_sha256 equals preview_record.after_sha256 8a7b6e95…
exactly one target, and it is inside the disposable workspace
the built claw binary is used by exact path
no model/broker/runtime is required; no raw :11434
no secrets appear in inputs/artifacts/output
```

---

## 10. Approval Evidence Requirement

```text
REQUIRED: a persisted approval-result (schema a2-l2b-approval-result.v1) bound to preview_sha256 1c856762…
STDOUT-ONLY approval is NOT sufficient for apply — both apply paths read a persisted approval-result.
CURRENT STATE: no persisted approval-result artifact found in the workspace .claw tree.
CONSEQUENCE: until a persisted approval-result exists (and, for Path A, an assembled apply-bundle),
  S2E apply execution is BLOCKED.
REMEDIATION (outside this docs lane): the operator runs S2D `claw plan approve` at a real TTY and
  CAPTURES the emitted approval-result JSON to a persisted file (or assembles an apply-bundle from
  it). The S2E execution lane must then re-verify the persisted approval-result binds to the exact
  preview_sha256 before applying. Do NOT fabricate an approval-result.
```

---

## 11. Preview / Approval Hash Binding

The future lane must, and the apply command additionally enforces:

```text
preview_record.preview_sha256 == 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
approval_result.preview_sha256 == preview_record.preview_sha256   (apply REJECTS mismatch)
payload.after_sha256 == preview_record.after_sha256 8a7b6e95…     (apply REJECTS mismatch)
payload bytes hash to after_sha256                                (apply REJECTS non-hashing bytes)
apply-bundle schema_version == a2-l2b-apply-bundle.v1             (apply REJECTS other versions)
NEVER fabricate or alter preview_sha256 / before_sha256 / after_sha256; use only the proven values.
```

---

## 12. Target State Validation

```text
BEFORE apply: read sample/preview_target.txt; REJECT/STOP unless sha256 == before_sha256 d646ebba…
AFTER apply:  re-read sample/preview_target.txt; expect sha256 == after_sha256 8a7b6e95…
              (the applied state is the operator-reviewed after bytes)
the apply-completion artifact/output must echo preview_sha256 1c856762… and outcome "applied".
```

A target that does not match `before_sha256` at apply time means the world changed since preview →
STOP (re-preview/re-approve is a separate future decision, not an S2E action).

---

## 13. Apply Execution Boundary

```text
S2E is the FIRST and ONLY lane in this chain allowed to write the target.
S2E writes exactly ONE file: sample/preview_target.txt, inside the disposable workspace only.
S2E must NEVER call a model, the broker, or any runtime/service; no raw localhost:11434.
S2E must NEVER mutate anything outside the disposable workspace.
S2E must verify the target equals before_sha256 BEFORE writing, and after_sha256 AFTER writing.
S2E runs apply AT MOST ONCE; on failure it does not retry except to READ already-written artifacts.
A2 remains the only write path: the model proposes; the operator reviews after-bytes; A2-L2b previews
  (S2C-1d); the operator approves (S2D); A2 applies (S2E).
```

---

## 14. Output / Apply Artifact

```text
on success the apply emits outcome == "applied" and a write-applied marker ("a2-l2b-write-applied").
the S2E lane must capture: exit code; the apply-result/output; the bound preview_sha256; the
  apply-completion artifact path under the workspace .claw tree (read-only capture to /tmp evidence).
the captured output must be operator-reviewable and expose no secrets.
```

---

## 15. Rollback / Recovery Boundary

```text
This trial target (sample/preview_target.txt) lives in a DISPOSABLE /tmp workspace; rollback is
  OPTIONAL here and, if performed, must be EXPLICIT and limited to restoring this one file from the
  preserved checkpoint before.bin / before_sha256.
For any NON-disposable future target, a backup of the target is MANDATORY before apply.
NO broad rollback. NO destructive cleanup. NEVER run git reset --hard, git clean, rm -rf,
  find -delete/-exec rm, or remove worktrees/bundles as "recovery".
Recovery from a failed apply is: read artifacts, report, and escalate — not re-apply, not force-clean.
```

---

## 16. Rejection / STOP Conditions

The future S2E lane must STOP (report BLOCKED/FAIL) if any of:

```text
the exact apply token (§7) is missing
the apply command/input contract is ambiguous against main.rs
persisted approval evidence is insufficient or absent (§10)            <-- current state
a required approval-result artifact is missing
the preview bundle is missing or modified
preview_sha256 mismatch (record or approval_result)
before_sha256 mismatch (target changed since preview)
after_sha256 mismatch (payload disagrees with preview, or bytes do not hash)
more than one target, or a target path outside the disposable workspace
a model/broker call would be required, or a raw localhost:11434 reference appears
a runtime/service/secret path would be touched
secrets appear in inputs/artifacts/output
rollback is unclear for a non-disposable target
apply would need to run more than once, or any pre-approval/batch flag would be used
```

No retries except reading already-written artifacts.

---

## 17. Validation Plan

```text
docs-only: this card changes only docs/a2-l4-s2e-apply-gate-scope.md (no scripts/tests/schemas/rust/src/runtime).
the apply command is source-confirmed from the built binary usage + tests/plan_apply.rs (or, if
  ambiguous at execution time, explicitly BLOCKED pending RCA).
exact hashes present: preview 1c856762…, before d646ebba…, after 8a7b6e95….
apply token present: "APPROVED: Execute S2E A2-L2b apply gate".
approval-evidence caveat documented as a hard precondition + STOP gate.
target-write confined to S2E and to the single disposable-workspace file.
model/broker/runtime prohibited; raw :11434 appears only as a rejection pattern.
rollback/destructive cleanup prohibited.
```

---

## 18. Follow-On Lanes

```text
1. S2E Apply Gate Scope Review / Push PR              (docs-only PR for this card)
2. S2E exact-head merge gate                           (merge this card)
3. (Prereq) Persist S2D approval-result at a real TTY  (operator captures approve stdout to a file / assembles apply-bundle)
4. S2E apply execution prompt draft                    (docs-only future apply prompt)
5. S2E apply execution APPLY gate                      (token-gated, single apply; first/only target write)
6. Apply evidence capture / closeout                   (read-only verification target==after_sha256, outcome applied)
7. Retire disposable build/workspace artifacts ONLY after closeout
```

> **Reminder:** This is a DOCS-ONLY scope card. It runs no apply, no approval, no preview, and no
> model/broker/runtime. S2E apply execution remains BLOCKED until a persisted approval-result exists
> and the apply input is assembled per the source-confirmed contract — and even then requires the
> exact S2E apply token and explicit operator go-ahead.
