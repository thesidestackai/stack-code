# DRAFT — S2D A2-L2b Approval Execution Prompt

> **DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL**
>
> This file is a *future* execution prompt. It is not authorization to run anything.
> Drafted 2026-06-05 by the S2D approval-execution-prompt drafting lane, which itself ran
> **no** approval, **no** apply, **no** preview, and touched no model/broker/runtime.
>
> The lane that eventually runs this prompt approves the **already-proven** A2-L2b preview by
> binding operator approval to the exact `preview_sha256`. It runs **approval only** — it never
> applies, never re-runs preview, never writes the target file, never fabricates or alters a hash.

---

## 1. Status and Approval Requirement

**STATUS: GATED FUTURE EXECUTION PROMPT — APPROVAL-ONLY.**

This prompt may be executed only when the operator supplies, in the invoking message, this **exact**
approval token:

```text
APPROVED: Execute S2D A2-L2b approval gate
```

If that exact token is **absent**, the executing agent must immediately STOP and report:

```text
BLOCKED: missing required approval token.
```

No preflight, no read, no command may run before the token is confirmed present and exact. The token
is required **in addition to** every precondition gate in §8.

This is an **approval** lane, not an apply lane. Approval records an operator decision bound to a
preview hash; it writes **no** target files. The live target is only ever written later, by the
separate S2E apply lane (§14).

---

## 2. Role

You are operating as a careful Stack-Code / A2-L2b **approval executor**.

Follow:

OBSERVE → VERIFY TOKEN → VERIFY PRECONDITIONS → HASH-BIND → STOP-BEFORE-APPROVAL → APPROVE (ONCE) → CAPTURE → VALIDATE → REPORT

You run at most **one** approval command. You never apply, never re-run preview, never edit code,
never call a model/broker, and never touch runtime.

---

## 3. Objective

Approve the proven A2-L2b preview by binding operator approval to the exact preview hash:

```text
1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

The single permitted command is:

```bash
<BUILT_CLAW> plan approve <preview-bundle.json>
```

with the exact built binary and bundle paths in §7. It must run **approval only**:

* It must **not** run `claw plan apply` or `claw plan apply-bundle`.
* It must **not** re-run preview (`claw plan run` / `claw plan preview-bundle`).
* It must **not** modify the target file `sample/preview_target.txt`.

---

## 4. Source of Truth

```text
docs/a2-l4-s2d-approval-gate-scope.md
    (merged S2D scope card — PR #83, merge commit e9617c1cbc707177c7c9920d2663cb5a489db03a)
handoffs/s2c1d_a2_l2b_preview_execution_prompt_DRAFT_2026-06-05.md
    (the S2C-1d lane that produced the proven preview this approves)
rust/crates/rusty-claude-cli/src/main.rs            (run_plan_approve dispatch; re-derives preview_sha256, rejects mismatch)
rust/crates/rusty-claude-cli/tests/plan_approve.rs  (TTY enforcement; exit 7 on non-TTY; --yes/--auto/--force rejected; approval line grammar)
rust/crates/a2-plan-runner/src/approval_ux.rs       (approval line grammar `apply <step-id> <preview_sha256>`; Preview SHA256 display)
rust/crates/rusty-claude-cli/tests/plan_apply.rs    (apply is a SEPARATE command consuming an approval-result; schema a2-l2b-approval-result.v1)
<preview bundle>                                    (the proven preview-bundle.json; schema a2-l2b-preview-bundle.v1)
```

**Source-confirmed CLI surface** (from the built binary's own usage strings):

```text
claw plan approve <preview-bundle.json>
claw plan apply <apply-bundle.json>
claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
claw plan status <workspace> [<approval-result.json>]
```

**Source-confirmed safety properties:**

* `claw plan approve` is **TTY-enforced**. A non-TTY (piped) stdin on an approvable bundle is
  **refused** with **exit 7**, JSON `reason: "approval-stdin-not-tty"`
  (`tests/plan_approve.rs::plan_approve_non_tty_stdin_for_approvable_bundle_refuses_exit_seven`).
  → The executor must therefore run approval on a **real TTY** and type the approval line; it must
  **not** attempt to satisfy approval by piping the line via a non-TTY stdin. If a real TTY is not
  available, STOP (§10/§15) — do **not** work around the TTY guard.
* `claw plan approve` re-derives `preview_sha256` from the canonical preview record + display and
  **rejects on mismatch** with `record.preview_sha256`.
* Approval **writes no target files**; it emits an approval-result artifact
  (`schema a2-l2b-approval-result.v1`, `ApprovalDecision::Approved`) bound to `step_id` +
  `preview_sha256`.
* `--yes` / `--auto` / `--force` / `--allow-write` / `--preapproved` / `--batch` **do not exist** and
  are rejected as "unsupported flag".
* Apply is a **separate** command requiring an apply-bundle that embeds the approval-result.

---

## 5. Proven Preview Evidence

```text
preview_bundle:  /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json
schema_version:  a2-l2b-preview-bundle.v1
preview_id:      01KTCYZV5HYES43YSX604NSE2R
step_id:         preview_target_update
target (rel):    sample/preview_target.txt
target (abs):    /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/sample/preview_target.txt
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b   (live target pre-preview)
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5   (operator-reviewed after_file)
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10   (the approval-binding hash)
checkpoint_run_id:   01KTCYZV5B1TYF45QX0ZRRWTNG
checkpoint_baseline_unchanged: true
target verified UNCHANGED at draft time (sha == before_sha256); no approve/apply has occurred
```

The future lane approves **exactly this preview, and no other**.

---

## 6. Hard Boundaries

The executing agent must **not**:

```text
run claw plan apply / claw plan apply-bundle / claw plan run / claw plan preview-bundle
re-run or regenerate the preview
modify, regenerate, or delete the preview bundle or the ready-to-preview workspace
modify the target file sample/preview_target.txt
compute, fabricate, alter, or substitute any preview_sha256
use --yes / --auto / --force / --allow-write / --preapproved / --batch
bypass or work around the TTY approval guard
edit runner/CLI/schema/apply code or any source
call a model, call broker, call /v1/chat/completions, call /status/vram
touch runtime, restart services, touch Vault/secrets, print secrets
introduce a raw localhost:11434 app-inference path
run more than one approval command
git clean / rm -rf / git reset --hard / git add -A
delete the build worktree or any branch/worktree
```

Allowed:

```text
read the preview bundle, scope card, and approval source references (read-only)
run exactly one `claw plan approve <preview-bundle.json>` on a real TTY (only after token + all preconditions)
capture the approval-result artifact and read it back (read-only)
```

---

## 7. Input Artifact Requirements

Use these **exact** paths. Use the **built** binary by exact path — never the stale `claw` on PATH.

```text
BUILT_CLAW:
/mnt/vast-data/git-worktrees/stack-code-a2-l2b-preview-cli-build-20260605/rust/target/debug/claw

PREVIEW_BUNDLE:
/tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json

WORKSPACE_ROOT:
/tmp/s2c1d_ready_to_preview_20260605_142019/workspace

TARGET (abs):
/tmp/s2c1d_ready_to_preview_20260605_142019/workspace/sample/preview_target.txt

step_id:
preview_target_update

expected preview_sha256:
1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

The executor must verify:

* `BUILT_CLAW` exists and is executable.
* `PREVIEW_BUNDLE` exists and its path matches the expected path above.
* the bundle's `schema_version` is `a2-l2b-preview-bundle.v1`.
* the bundle's `preview_record.step_id` is `preview_target_update`.
* the bundle's `preview_record.preview_sha256` is `1c856762…805da10`.
* the bundle's `checkpoint_baseline_unchanged` is `true`.
* the target file currently hashes to `before_sha256` (`d646ebba…cee2b`) — i.e. unchanged since preview.

---

## 8. Approval Preconditions

The executor must verify **ALL** of the following before approving (else STOP — §10/§15):

```text
the exact approval token (§1) is present in the invoking message
the built CLI (§7) exists and is executable
the preview bundle exists, is at the expected path, and is unmodified
  (schema a2-l2b-preview-bundle.v1; step_id preview_target_update; preview_sha256 == 1c856762…)
checkpoint_baseline_unchanged == true
the live target still hashes to before_sha256 d646ebba… (unchanged since preview)
no apply has occurred for this preview
no approval-result artifact already exists for this preview (unless explicitly expected/provided)
no preview rerun is needed
a real TTY is available for the TTY-enforced approval prompt
no model/broker/runtime is required; no raw :11434
```

---

## 9. Preview Hash Binding

The executor must:

```text
read the preview bundle and extract preview_record.preview_sha256
compare it byte-for-byte to the expected proven hash:
  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
REJECT (STOP) if it differs, is missing, or the bundle path/content appears modified
REJECT (STOP) if the live target changed since preview (current sha != before_sha256 d646ebba…)
NEVER fabricate, compute-and-substitute, or approve any hash other than the proven 1c856762…
```

`claw plan approve` **additionally** re-derives `preview_sha256` from the canonical record + display
and rejects any record/display mismatch — approval is cryptographically bound, not merely trusting
the file. The executor's own check above is a pre-gate, not a replacement for the CLI's binding.

---

## 10. STOP Before Approval

Immediately before running the approval command, the executor must print this **exact** message and
obtain explicit operator go-ahead for S2D approval execution:

```text
STOP BEFORE S2D APPROVAL EXECUTION:
About to run the existing A2-L2b approval command.
This will bind operator approval to preview_sha256:
1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
It will not apply changes.
Proceed only with explicit operator approval for S2D approval execution.
```

---

## 11. Execute Approval Only

Run **no more than one** approval command, using the built binary by exact path:

```bash
/mnt/vast-data/git-worktrees/stack-code-a2-l2b-preview-cli-build-20260605/rust/target/debug/claw plan approve /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json
```

When the TTY-enforced prompt asks for the approval decision, supply **exactly** this line (grammar
`apply <step-id> <preview_sha256>`):

```text
apply preview_target_update 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

Constraints:

* Enter the line on a **real TTY**. The binary refuses non-TTY stdin with exit 7
  (`approval-stdin-not-tty`); if you cannot provide a TTY, STOP — do **not** work around the guard.
* Some terminal drivers require an explicit EOF after the line; handle this, do not guess.
* Do **not** use `--yes`, `--auto`, `--force`, `--allow-write`, `--preapproved`, or `--batch`.
* Do **not** issue more than one `apply ` line (batch approval is refused).
* Approve **only** the proven hash; never an alternative or fabricated hash.
* If the CLI prompt/flow does not match this expected TTY approval flow, STOP (§15).

---

## 12. Approval Output Capture

* Capture the approval command's **exit code**.
* Capture the emitted **approval-result artifact** (`schema a2-l2b-approval-result.v1`,
  `ApprovalDecision::Approved`) — record its path/contents read-only.
* Confirm the artifact is bound to `step_id == preview_target_update` and
  `preview_sha256 == 1c856762…805da10`.
* Preserve raw output for operator review; redact nothing relevant, expose no secrets.

---

## 13. Post-Approval Validation

The executor must verify:

```text
approval command exit code is the success code for an approved decision
approval-result artifact exists and is bound to the exact preview_sha256 1c856762…
approval decision is Approved (schema a2-l2b-approval-result.v1)
NO apply occurred (no claw plan apply / apply-bundle was run)
the target file sample/preview_target.txt is UNCHANGED (sha still d646ebba… before_sha256)
the preview bundle is unchanged
no model/broker call occurred
runtime untouched; no services restarted
no secrets exposed in output
output is operator-reviewable
apply remains a later, separate lane (§14)
```

---

## 14. Apply Boundary

```text
Approval is NOT apply.
S2D must NEVER run `claw plan apply`.
S2D must NEVER run `claw plan apply-bundle`.
S2D must NEVER modify target files (sample/preview_target.txt stays at before_sha256).
The live target is written only by the separate S2E apply lane, which consumes the approval-result
  inside an apply-bundle (schema a2-l2b-apply-bundle.v1). That lane is out of scope here.
A2 remains the only write path: the model proposes, the operator approves (S2D), A2 applies (S2E).
```

---

## 15. Failure Handling

The executor must **STOP** (and report BLOCKED/FAIL with the reason) if any of:

```text
the exact approval token is missing
the built CLI is missing or not executable
the preview bundle is missing or not at the expected path
the preview bundle is modified (schema/step_id/hash/path mismatch)
preview_sha256 is missing
preview_sha256 mismatches the proven 1c856762…
step_id mismatches preview_target_update
checkpoint_baseline_unchanged is not true
the target changed since preview (current sha != before_sha256 d646ebba…)
an approval-result already exists unexpectedly, or an apply already occurred
the approval command/flow is unclear or does not match the expected TTY approval flow
no real TTY is available (non-TTY would refuse with exit 7 — do not work around it)
the CLI unexpectedly accepts preapproval / --yes / batch
approval would require a fabricated or alternative hash
apply is requested, or a target write is requested
a model/broker call would be required, or a raw localhost:11434 reference appears
secrets would be exposed
more than one approval command would be needed
```

No retries except **reading** already-written artifacts (the approval-result). Never re-run approval,
never re-run preview, never apply as a "fix".

---

## 16. Final Report Template

```text
CLASSIFICATION:
PASS | PASS_WITH_NOTES | BLOCKED | FAIL

MODE:
S2D_A2_L2B_APPROVAL_EXECUTION_ONLY

TOKEN:
required token present:
token exact match:

INPUTS:
built claw path:
built claw executable:
preview bundle path:
preview bundle path matches expected:
schema_version:
step_id:
preview_sha256 (bundle):
preview_sha256 == expected 1c856762…:
checkpoint_baseline_unchanged:
target sha before approval:
target == before_sha256 d646ebba…:
real TTY available:

STOP-BEFORE-APPROVAL:
exact STOP message printed:
operator go-ahead:

APPROVAL:
command run (count):
command (exact):
approval line supplied (exact):
exit code:

APPROVAL OUTPUT:
approval-result artifact path:
schema_version:
decision:
bound step_id:
bound preview_sha256:

POST-APPROVAL VALIDATION:
apply run:
target unchanged after approval:
preview bundle unchanged:
model call:
broker call:
runtime touched:
secrets exposed:

APPLY BOUNDARY:
apply attempted:
apply-bundle attempted:
S2E remains separate:
A2 authority preserved:

STOP GATES HIT:
none | details

NEXT BEST LANE:
Name: S2E A2-L2b Apply Execution Prompt (draft)
Objective: scope/draft the separate apply lane that consumes this approval-result via an apply-bundle
Recommended tool: Claude Code, prompt-draft-only lane (no apply)
Why: apply is the next link after a recorded approval; it remains separately token-gated
Touched surfaces: docs/handoff only
Mutation risk: NONE (draft only)
STOP gate: do not run apply until the apply prompt is reviewed/merged and explicitly approved
First prompt/command: (draft S2E apply prompt; no execution)
```

---

> **Reminder:** This artifact is a DRAFT of a future prompt. Drafting it executed no approval, no
> apply, no preview, and no model/broker/runtime call. Do not execute the prompt above without the
> exact operator approval token and explicit go-ahead.
