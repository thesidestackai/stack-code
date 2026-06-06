# DRAFT — Persist S2D Approval-Result Artifact (Remediation Prompt)

> **DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL**
>
> This file is a *future* remediation prompt. It is not authorization to run anything. Drafted
> 2026-06-06 by the approval-result persistence remediation drafting lane, which itself ran **no**
> approval, **no** apply, **no** preview, and touched no model/broker/runtime.
>
> Honest evidence note: this drafting lane could NOT independently confirm that S2D `claw plan
> approve` was ever run, and a read-only scan of the workspace `.claw` tree found **no** persisted
> `a2-l2b-approval-result.v1` artifact (only checkpoints, payloads, preview-bundle.json,
> preview-generator-result.json, run manifests). The remediation below is written to that real
> state: it must produce a *legitimate*, source-traceable persisted approval-result — not fabricate
> one.

---

## 1. Status and Approval Requirement

**STATUS: GATED FUTURE REMEDIATION PROMPT — PRODUCES APPROVAL EVIDENCE ONLY, NO APPLY.**

Executable only when the operator supplies, in the invoking message, this **exact** token:

```text
APPROVED: Execute S2D approval-result persistence remediation
```

If absent, STOP immediately and report:

```text
BLOCKED: missing required approval token.
```

This lane records/persists approval evidence; it does **not** apply, does **not** write the target,
and does **not** re-run preview.

---

## 2. Role

You are a careful Stack-Code / A2 approval-evidence remediation operator.

Follow: OBSERVE → VERIFY TOKEN → VERIFY PRECONDITIONS → CHOOSE SOURCE-SAFE METHOD → PRODUCE PERSISTED APPROVAL-RESULT → VALIDATE → REPORT.

You never apply, never write the target, never bypass the TTY guard, never fabricate evidence.

---

## 3. Objective

Produce a **legitimate, persisted** `a2-l2b-approval-result.v1` JSON file for the proven preview, so
the future S2E apply lane (`claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`,
or an apply-bundle embedding it) has the persisted approval evidence it requires.

The persisted JSON must bind:

```text
schema_version:  a2-l2b-approval-result.v1
decision:        approved
step_id:         preview_target_update
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
```

Target persistence path (or a source-confirmed equivalent):

```text
/tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-approval-results/preview_target_update/approval-result.json
```

> Note: the `.claw/l2b-approval-results/...` path is a *proposed* location — the executing lane must
> source-confirm the canonical location (if any) that `apply-bundle` expects, or treat the path as a
> freely-chosen operator file passed explicitly to `apply-bundle`.

---

## 4. Current Blocker

```text
S2D `claw plan approve` emits the approval-result JSON to STDOUT only; it writes no file
  (main.rs: "read approval line from stdin, emit approval-result JSON on stdout … never writes target files").
`claw plan approve` has NO output-file/persist flag (arg parsing is PlanApprove { bundle_path } only).
No persisted a2-l2b-approval-result.v1 artifact exists in the workspace .claw tree.
S2E apply REQUIRES a persisted approval-result (embedded in an apply-bundle, or as a standalone file
  read by `apply-bundle` — main.rs: "Reads ONE approval-result JSON file from disk").
=> S2E apply execution remains BLOCKED until a legitimate persisted approval-result exists.
```

---

## 5. Source of Truth

```text
docs/a2-l4-s2e-apply-gate-scope.md                                  (merged S2E apply scope; PR #85, 4cdb4f1)
handoffs/s2d_a2_l2b_approval_execution_prompt_DRAFT_2026-06-05.md   (merged S2D approval prompt; PR #84, 3e20750)
docs/a2-l4-s2d-approval-gate-scope.md                              (merged S2D scope; PR #83, e9617c1)
rust/crates/rusty-claude-cli/src/main.rs                           (run_plan_approve: stdout-only emit; run_plan_apply_bundle: reads ONE approval-result JSON from disk)
rust/crates/rusty-claude-cli/src/input.rs:141                      (TTY guard: requires BOTH stdin().is_terminal() AND stdout().is_terminal())
rust/crates/rusty-claude-cli/tests/plan_approve.rs                 (non-TTY stdin refuses exit 7 "approval-stdin-not-tty"; no --yes/--auto/--force)
rust/crates/rusty-claude-cli/tests/plan_apply.rs                   (apply-bundle consumes approval-result; hash re-validation; rejection cases)
<preview bundle> + <preview-generator-result.json>                 (the proven preview inputs; both present)
```

---

## 6. Proven Preview / Approval Evidence

```text
preview_bundle:  /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/.claw/l2b-preview-bundles/01KTCYZV5B1TYF45QX0ZRRWTNG/preview_target_update/preview-bundle.json
preview-generator-result: …/preview_target_update/preview-generator-result.json   (PRESENT)
step_id:         preview_target_update
preview_sha256:  1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
before_sha256:   d646ebba4db098532e48b4627afd3170471ff5f6c9937853a6c8bee8c53cee2b
after_sha256:    8a7b6e954e4f1b1612df27868aba21b335d5fa7da20586736b5fafbf05de67d5
target:          /tmp/s2c1d_ready_to_preview_20260605_142019/workspace/sample/preview_target.txt   (currently == before_sha256; UNCHANGED)
built claw:      /mnt/vast-data/git-worktrees/stack-code-a2-l2b-preview-cli-build-20260605/rust/target/debug/claw

APPROVAL EVIDENCE STATE (honest): no persisted a2-l2b-approval-result.v1 found; approval not independently verified as run.
```

---

## 7. Hard Boundaries

The executing remediation lane must **not**:

```text
run claw plan apply / apply-bundle / run / preview; run A2 apply
modify the target file, preview artifacts, the ready-to-preview bundle, or any approval artifact except to CREATE the new persisted approval-result via a legitimate method
fabricate approval evidence; hand-author a decision that the CLI did not actually emit
fabricate, compute-substitute, or alter preview_sha256 / before_sha256 / after_sha256
fake a TTY; use script/expect/pty wrappers; pipe the approval line; redirect/tee stdout in a way that defeats the TTY guard; use --yes/--auto/--force/--allow-write/--preapproved/--batch
edit runner/CLI/schema/apply code (a CLI persist-flag is a SEPARATE engineering lane, §8 Option C)
call a model/broker; call /v1/chat/completions or /status/vram; touch runtime/Vault/secrets; print secrets; raw localhost:11434
delete the preview bundle or the build worktree; git clean / rm -rf / reset --hard / add -A
```

Allowed: read docs/source/bundle/.claw; run the source-safe persistence method chosen in §10 (only
after token + preconditions); validate and read back the persisted file.

---

## 8. Remediation Strategy (source-confirmed)

Phase-3 questions, answered from source:

```text
1. Does `claw plan approve` write approval-result JSON to a file?            NO. Stdout only; no output-file flag.
2. Safe to rerun approve and persist stdout via shell redirection / tee?     NO — see (3).
3. Does redirecting stdout break TTY enforcement?                            YES. The interactive guard (input.rs:141)
     requires BOTH stdin AND stdout to be terminals. `approve <bundle> | tee f` or `> f` makes stdout a non-terminal,
     forcing the non-interactive fallback, which refuses an approvable bundle (exit 7). tee/redirect is NOT viable.
4. Workflow that captures stdout while preserving a real TTY?                 None without a pty wrapper (forbidden). A
     terminal that is itself a real TTY shows the JSON on screen, but there is no built-in, guard-preserving file capture.
5. Can prior on-screen stdout JSON be reconstructed into a file?             Technically yes (manual transcription), but see (6).
6. Is manual reconstruction allowed or is it fabrication?                    GREY. Transcribing the CLI's OWN emitted bytes is
     not fabrication; INVENTING fields the CLI never emitted IS. Manual transcription is error-prone and not cryptographically
     traceable, so it is a CAUTIONED fallback (Option B) that must be validated byte-for-byte against the preview binding.
7. Validation that proves a persisted file is legitimate and hash-bound?     §11.
8. Does `apply-bundle <preview-generator-result.json> <approval-result.json>` accept the persisted file?  YES — apply-bundle
     reads ONE approval-result JSON file from disk (main.rs:2519) and re-validates its preview_sha256 against the preview.
```

**Options, ranked:**

```text
Option C (RECOMMENDED, cleanest, but a SEPARATE code lane): add a source-confirmed persist path to
  `claw plan approve` — e.g. an `--output-file <path>` / `--write-approval-result <path>` flag that
  writes the SAME approval-result the CLI emits, atomically, alongside stdout. This makes persisted
  approval evidence first-class and guard-preserving. Requires a rust/CLI change + tests + PR; OUT OF
  SCOPE for this remediation lane but is the correct long-term fix. This remediation prompt should
  RECOMMEND opening that engineering lane.

Option B (CAUTIONED fallback, no code change): operator runs `claw plan approve <preview-bundle.json>`
  in a real terminal (stdin AND stdout are real TTYs), types the exact approval line, and the CLI
  prints the approval-result JSON on screen. The operator then SAVES the CLI's emitted JSON verbatim
  into the persisted file. This is "operator-captured stdout evidence." It MUST be validated
  byte/field-for-field against the preview binding (§11) before any apply-bundle use, and labeled as
  operator-captured. If the operator cannot capture the exact emitted bytes (only a re-typed
  approximation), STOP — that crosses into fabrication.

Option A (REJECTED): `approve | tee` / `approve > file`. Defeats the TTY guard; refused. Do not use.
```

The executing lane MUST pick Option C (recommend the code lane and STOP here, producing no file) or
Option B (operator-captured, fully validated). It must NOT use Option A and must NOT fabricate.

---

## 9. Operator TTY Requirement

```text
Any approve invocation runs in a REAL terminal: stdin AND stdout are real TTYs (input.rs:141).
No fake TTY. No script. No expect. No pty wrapper. No piping the approval line. No stdout redirection
  during the approve invocation. No --yes / --auto / batch. The operator types the exact approval line:
    apply preview_target_update 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10
If a real TTY is unavailable, STOP and report "BLOCKED: missing real TTY".
```

---

## 10. Approval-Result Persistence Method

```text
PREFERRED: Option C — do NOT produce a file in this lane; instead recommend and hand off a CLI
  engineering lane to add a guard-preserving approval-result persist flag, then re-run S2D approve
  with that flag at a real TTY. This avoids transcription risk entirely.

FALLBACK: Option B — operator-captured stdout, only if:
  - approve runs at a real TTY and exits 0 with decision=approved on screen, AND
  - the operator saves the CLI's EXACT emitted JSON bytes to the persisted file (no re-typing, no edits), AND
  - the persisted file passes every §11 validation.

NEVER Option A (tee/redirect). NEVER fabricate fields the CLI did not emit.
```

---

## 11. Validation of Persisted Approval Result

The executing lane must verify the persisted file:

```text
file exists and parses as JSON
schema_version == a2-l2b-approval-result.v1
decision == approved
step_id == preview_target_update
preview_sha256 == 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10  (exact; reject any other/extra hash)
binds to the proven preview bundle (preview_sha256 matches preview_record.preview_sha256)
no secrets present
target still hashes to before_sha256 d646ebba…  (unchanged; no apply occurred)
the file is usable as the standalone approval-result input to a FUTURE apply-bundle lane — but apply-bundle is NOT run here
(Option B only) the saved bytes equal the CLI's emitted bytes (operator-attested verbatim capture)
```

---

## 12. No-Apply Boundary

```text
This remediation does NOT apply. It does NOT run claw plan apply / apply-bundle. It does NOT modify
  the target file. S2E apply remains a SEPARATE, token-gated lane. A2 remains the only write path.
```

---

## 13. Failure Handling

STOP (report BLOCKED/FAIL) if any of:

```text
the exact remediation token is missing
no real TTY is available
persistence would require fake TTY / script / pty / piping / stdout redirection (Option A)
the approve flow / prompt differs from the expected S2D flow
preview_sha256 mismatch; step_id mismatch; an extra/alternative hash appears
the target changed since preview (sha != before_sha256)
the persisted JSON cannot be validated (parse/schema/decision/binding)
the approval-result appears manually fabricated rather than CLI-emitted (no verbatim source trace)
apply is requested, or a target write is requested
a model/broker/runtime call would be required; raw :11434; secrets appear
```

No retries except reading already-written artifacts. Never "fix" by fabricating or by applying.

---

## 14. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: S2D_APPROVAL_RESULT_PERSISTENCE_REMEDIATION_NO_APPLY
TOKEN: present / exact:
METHOD CHOSEN: Option C (recommend code lane, no file) | Option B (operator-captured, validated)
TTY: real tty available:
PERSISTED FILE: path / exists / parses:
  schema_version: / decision: / step_id: / preview_sha256:
  binds to preview bundle: / verbatim-capture attested (Option B):
VALIDATION: target unchanged: / no apply: / no model-broker: / no secrets:
APPLY BOUNDARY: apply run (must be NO): / apply-bundle run (must be NO): / target modified (must be NO):
STOP GATES HIT: none | details
NEXT BEST LANE:
  Name: (Option C) Add guard-preserving approval-result persist flag to `claw plan approve` (engineering lane)
        OR (Option B done) S2E Apply Execution Prompt Draft
  First prompt/command: (none until persisted+validated approval-result exists)
```

---

> **Reminder:** DRAFT of a future prompt. Drafting it produced no approval, no apply, no preview, no
> model/broker/runtime call, and persisted no file. The recommended clean fix (Option C) is a
> separate CLI engineering lane; the no-code fallback (Option B) is operator-captured stdout that
> must be validated verbatim and never fabricated. S2E apply stays BLOCKED until a legitimate
> persisted approval-result exists.
