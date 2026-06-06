# A2-L4 — Option C: Approval-Result Persistence Scope (Docs-Only)

> Status: **DOCS-ONLY SCOPE CARD — NOT AN IMPLEMENTATION LANE.** This card scopes a future,
> separately-token-gated engineering lane that adds a **guard-preserving** approval-result
> persistence path to `claw plan approve`, so a legitimate persisted `a2-l2b-approval-result.v1`
> artifact can exist for S2E apply. This docs lane writes no code, runs no approval/apply/preview,
> and touches no model/broker/runtime. Created 2026-06-06.

---

## 1. Executive Summary

S2E apply is blocked: `claw plan approve` emits the approval-result JSON to **stdout only**, and S2E
apply requires a **persisted** `a2-l2b-approval-result.v1`. The previously-merged remediation
(PR #86) source-confirmed that `tee`/redirect cannot capture it — the approval TTY guard requires
**both** stdin and stdout to be terminals, so any stdout redirection forces the non-interactive
fallback and the approvable bundle is refused (exit 7). Faking a TTY / `script` / `expect` / `pty`
wrappers are prohibited, and hand-authoring the JSON is fabrication.

**Option C** is the clean fix: add an explicit, opt-in output-file flag to `claw plan approve` that
writes the **same** approval-result the CLI already emits, **without weakening** the existing
real-TTY confirmation. This card scopes that change. It implements nothing.

---

## 2. Problem Statement

```text
claw plan approve emits approval-result JSON to stdout only (no file).
S2E apply (`claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`,
  or `claw plan apply <apply-bundle.json>` embedding it) requires a PERSISTED approval-result.
The TTY guard (input.rs:141) requires stdin().is_terminal() AND stdout().is_terminal();
  redirecting stdout (tee/>) makes stdout non-terminal → non-interactive fallback → approvable bundle refused (exit 7).
=> there is no source-safe automated capture today; S2E remains blocked.
```

---

## 3. Source of Truth

```text
rust/crates/rusty-claude-cli/src/main.rs
    fn run_plan_approve<R,W1,W2>(bundle_path, stdin_is_tty, stdin, stdout, stderr) -> i32   (~line 1306)
      - non-TTY guard for approvable previews (emit_non_tty_refusal "approval-stdin-not-tty")
      - run_approval_interaction(...) reads the approval line
      - emit_approval_result(&preview_record, &interaction, baseline_unchanged, stdout)  <-- stdout emission site
    "END A2-L2b Slice 3d — scope sentinel"  (source-grep tests bound the forbidden-API scan to this region)
    CliAction::PlanApprove { bundle_path }  (arg parsing — currently only a bundle path; no output flag)
rust/crates/rusty-claude-cli/src/input.rs:141   (TTY guard: stdin().is_terminal() && stdout().is_terminal())
rust/crates/rusty-claude-cli/tests/plan_approve.rs  (TTY refusal exit 7; --yes/--auto/--force rejected; approval-line grammar)
rust/crates/a2-plan-runner/src/approval_ux.rs   (approval-line grammar `apply <step-id> <preview_sha256>`; approval-result shape)
rust/crates/rusty-claude-cli/tests/plan_apply.rs  (apply-bundle consumes approval-result a2-l2b-approval-result.v1; hash re-validation)
handoffs/persist_s2d_approval_result_remediation_DRAFT_2026-06-06.md  (merged PR #86 — the blocker + Option ranking)
docs/a2-l4-s2e-apply-gate-scope.md  (merged PR #85 — S2E apply contract)
```

> The implementation lane MUST re-read `main.rs` / `approval_ux.rs` before editing; line numbers
> here are indicative and may drift.

---

## 4. Current Approval Behavior

```text
`claw plan approve <preview-bundle.json>`:
  - TTY-enforced: refuses non-TTY stdin on an approvable bundle (exit 7, "approval-stdin-not-tty").
  - reads the operator's approval line `apply <step_id> <preview_sha256>` from stdin.
  - re-derives/validates preview_sha256; rejects mismatch.
  - emits the approval-result JSON (schema a2-l2b-approval-result.v1, decision=approved on success)
    to STDOUT via emit_approval_result(...). Writes NO file. Has NO output-file flag.
  - never accepts --yes/--auto/--force/--allow-write/--preapproved/--batch.
```

---

## 5. Current Apply Requirement

```text
S2E apply reads a PERSISTED approval-result from disk:
  `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>`  (reads ONE approval-result JSON file)
  or `claw plan apply <apply-bundle.json>`  (apply-bundle EMBEDS the approval_result)
Apply re-validates: schema a2-l2b-apply-bundle.v1; approval_result.decision==approved;
  step_id/preview_sha256 bind to the embedded PreviewRecord; payload.after_sha256 == preview.after_sha256.
```

---

## 6. Rejected Workarounds

```text
NO tee / stdout redirection (defeats the dual-terminal TTY guard → refused, exit 7).
NO fake TTY.
NO script / expect / pty wrapper.
NO piping the approval line.
NO --yes / auto / batch.
NO manual fabrication / hand-authoring of approval-result JSON (fabricated evidence).
```

---

## 7. Option C Objective

Add a **guard-preserving, opt-in** persistence path so that, on a *successful* real-TTY approval,
`claw plan approve` ALSO writes the exact approval-result JSON to an operator-specified file — with
**no change** to the TTY requirement, the approval-line grammar, or the refusal behavior.

---

## 8. Proposed CLI Contract

```text
claw plan approve <preview-bundle.json> --approval-result-output <path>
```

```text
canonical flag (recommended): --approval-result-output <path>
  (implementation lane may choose a different name ONLY if source/CLI convention strongly suggests
   one; if so, justify in the report. Pick ONE canonical flag.)
the flag is OPTIONAL and OPT-IN; absent it, behavior is byte-for-byte unchanged (stdout only).
the flag takes a single filesystem path argument.
the flag is NOT a pre-approval flag and must NOT relax the TTY guard or the approval-line requirement.
```

---

## 9. TTY Guard Preservation

```text
The flag must NOT alter the input.rs:141 guard: stdin AND stdout must still be terminals for the
  interactive approval path; an approvable bundle on non-TTY stdin must still refuse (exit 7).
The flag must NOT pipe/redirect stdout; the approval-result still prints to stdout as today, AND is
  additionally written to the file. (Stdout stays a terminal; the file write is a separate fd.)
No --yes/--auto/--force/--allow-write/--preapproved/--batch; the operator still types the exact line.
```

---

## 10. Approval-Result Output Contract

```text
the file is written ONLY after a SUCCESSFUL approval (decision == approved); on rejection/mismatch/
  refusal/non-TTY, NO file is written.
the file content is the SAME approval-result the CLI emits to stdout (schema a2-l2b-approval-result.v1),
  semantically identical — ideally the exact bytes emitted by emit_approval_result(...).
the write should be atomic where practical (temp + rename) and must not partially write on error.
```

---

## 11. Hash / Step Binding

```text
the persisted approval-result must carry:
  schema_version = a2-l2b-approval-result.v1
  decision       = approved
  step_id        = preview_target_update
  preview_sha256 = 1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10  (for this proven preview)
binding is enforced by the existing approve path (re-derive/validate preview_sha256); Option C does
  not change binding logic — it only persists the already-validated result.
```

---

## 12. File Safety / Path Safety

```text
the output path is operator-supplied; treat it as untrusted input.
do NOT overwrite an existing file unless a safe, explicitly-defined behavior is chosen (recommend:
  refuse if the file already exists, OR write atomically only after success — implementation lane
  decides and documents).
the parent directory must exist or be safely creatable within scoped expectations; do not create
  arbitrary deep trees silently.
never write outside the operator-supplied path; never write the live target; never touch the
  preview bundle, checkpoint, or payload.
no symlink following into unexpected locations; no path traversal.
```

---

## 13. No-Apply Boundary

```text
Option C persists APPROVAL evidence only. It does NOT apply, does NOT run apply-bundle, does NOT write
  the live target, does NOT modify the preview bundle or apply-bundle behavior (except adding tests).
S2E apply remains a separate, token-gated lane. A2 remains the only write path.
```

---

## 14. Tests Required

```text
output file is written on a successful approval; content schema == a2-l2b-approval-result.v1;
  decision == approved; step_id matches; preview_sha256 matches the record.
the persisted file is semantically identical to the stdout approval-result.
NO output file is written on a rejected/mismatched approval line.
NO output file is written when the TTY guard fails (non-TTY approvable bundle → exit 7, no file).
--yes / --auto / --force / --allow-write / --preapproved / --batch remain refused.
absent the flag, behavior is unchanged (stdout-only; no file).
existing plan_approve and plan_apply tests still pass.
the source-grep/scope-sentinel tests still pass (no forbidden APIs introduced in the bounded region).
```

---

## 15. Validation Plan

```text
cargo fmt
cargo test -p rusty-claude-cli plan_approve
cargo test -p rusty-claude-cli plan_apply
cargo test --workspace
cargo clippy --workspace
(exact package/test filters may be adjusted by the implementation lane after discovery, with justification.)
optionally: an offline end-to-end check that the persisted file is accepted by
  `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>` in a DRY/scoped test
  — without running a real apply against the live target.
```

---

## 16. Rejection / STOP Conditions

The future implementation lane must STOP if:

```text
the implementation token is missing
the change would relax the TTY guard, the approval-line requirement, or refusal behavior
the change would add or enable a pre-approval/batch flag
the persisted file would be written on a non-approved / refused / non-TTY path
the persisted approval-result would differ semantically from the stdout result
the change would write the live target, modify the preview bundle, or call model/broker/runtime
raw localhost:11434 is introduced; secrets appear
the change would touch files outside the tightly-scoped approve/test surface
the scope-sentinel / forbidden-API source-grep tests would break
```

---

## 17. Follow-On Lanes

```text
1. Option C Approval-Result Persistence Scope exact-head merge gate (merge this card + the impl prompt)
2. Option C Approval-Result Persistence Implementation (separately token-gated CODE lane; "APPROVED: Execute S2D approval-result persistence implementation")
3. Build the updated claw + run the new approve with --approval-result-output at a real TTY (operator) to PRODUCE the persisted approval-result
4. S2E apply execution prompt draft / apply gate (still token-gated; first/only target write)
5. Apply evidence capture / closeout
```

> **Reminder:** Docs-only scope card. No code, no approval, no apply, no preview, no
> model/broker/runtime. Option C is the clean unblock; it must be implemented in a separate,
> token-gated code lane that preserves the TTY guard. S2E apply stays BLOCKED until a legitimate
> persisted approval-result exists.
