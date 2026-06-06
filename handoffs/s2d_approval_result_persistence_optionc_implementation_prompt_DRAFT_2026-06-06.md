# DRAFT — Option C Approval-Result Persistence Implementation Prompt

> **DRAFT ONLY — DO NOT EXECUTE WITHOUT EXPLICIT OPERATOR APPROVAL**
>
> This is a *future* implementation prompt for a CODE lane. It is not authorization to write code.
> Drafting it touched no rust/src/tests, ran no approval/apply/preview, and touched no
> model/broker/runtime. Created 2026-06-06.

---

## 1. Status and Approval Requirement

Executable only when the operator supplies, in the invoking message, this **exact** token:

```text
APPROVED: Execute S2D approval-result persistence implementation
```

If absent, STOP and report:

```text
BLOCKED: missing required approval token.
```

This is the **only** lane in the chain so far permitted to edit Rust code, and only within the
tightly-scoped surface in §7. It does not run approval/apply/preview and does not touch
model/broker/runtime.

---

## 2. Role

You are a careful Stack-Code Rust implementer. Follow:
OBSERVE → VERIFY TOKEN → DISCOVER → DESIGN → TDD → IMPLEMENT (scoped) → TEST → VALIDATE → REPORT.

You preserve the approval TTY guard, add no pre-approval flags, and never weaken approval safety.

---

## 3. Objective

Add a **guard-preserving, opt-in** `--approval-result-output <path>` flag to `claw plan approve`
that, on a **successful** real-TTY approval, writes the **same** approval-result JSON (schema
`a2-l2b-approval-result.v1`) the CLI already emits to stdout, to the operator-specified file — with
no change to the TTY requirement, approval-line grammar, or refusal behavior. Absent the flag,
behavior is byte-for-byte unchanged.

See the scope card `docs/a2-l4-s2d-approval-result-persistence-optionc-scope.md` (merged) for the
full contract.

---

## 4. Source of Truth

```text
rust/crates/rusty-claude-cli/src/main.rs
    fn run_plan_approve(...) (~1306) — emits via emit_approval_result(&preview_record, &interaction, baseline_unchanged, stdout)
    "END A2-L2b Slice 3d — scope sentinel"  (source-grep tests bound forbidden-API scan here — do NOT break)
    CliAction::PlanApprove { bundle_path }  (arg parsing — extend to carry the optional output path)
rust/crates/rusty-claude-cli/src/input.rs:141  (TTY guard: stdin AND stdout terminals — do NOT relax)
rust/crates/rusty-claude-cli/tests/plan_approve.rs  (existing approve tests — extend, don't weaken)
rust/crates/rusty-claude-cli/tests/plan_apply.rs   (apply consumes approval-result — keep green)
rust/crates/a2-plan-runner/src/approval_ux.rs   (approval-result shape; approval-line grammar)
docs/a2-l4-s2d-approval-result-persistence-optionc-scope.md  (the scope this implements)
```

Re-read these before editing; line numbers may drift. Discover the exact `emit_approval_result`
signature and the `CliAction::PlanApprove` parse site before designing.

---

## 5. Hard Boundaries

```text
Do NOT relax the TTY guard (input.rs:141) or the approval-line grammar.
Do NOT add/enable --yes/--auto/--force/--allow-write/--preapproved/--batch or any pre-approval/batch flag.
Do NOT write the persisted file on any non-approved / refused / non-TTY / mismatched path.
Do NOT run claw plan approve/apply/apply-bundle/run; do NOT rerun preview; do NOT run A2 apply.
Do NOT write the live target; do NOT modify the preview bundle, checkpoint, payload, or apply-bundle behavior (tests excepted).
Do NOT call a model/broker; no /v1/chat/completions, /status/vram; no runtime/Vault/secrets; no raw :11434.
Do NOT break the scope-sentinel / source-grep forbidden-API tests.
Do NOT touch files outside the scoped surface (§7). Exact-path staging only. No git add -A / add .
Do NOT fake a TTY / use script/expect/pty to "test" the interactive path — use the existing test
  injection seam (run_plan_approve takes stdin_is_tty: bool; tests inject true).
```

---

## 6. Clean Worktree Setup

```text
Create a fresh worktree off origin/main (do NOT work in /home/suki/stack-code):
  branch:   feat/s2d-approval-result-output-flag-<date>
  worktree: /mnt/vast-data/git-worktrees/stack-code-s2d-approval-result-output-flag-<date>
Run APPROVED_WORKTREE/APPROVED_BRANCH preflight per the session operating rule before mutation.
```

---

## 7. Implementation Scope (tightly bounded)

```text
ALLOWED edits (only what discovery proves necessary):
  rust/crates/rusty-claude-cli/src/main.rs
    - extend CliAction::PlanApprove to carry an optional output path (parse --approval-result-output <path>)
    - thread the optional path into run_plan_approve
    - after a SUCCESSFUL emit_approval_result on the approved path, ALSO write the same bytes to the file
      (atomic temp+rename; only on decision==approved)
  rust/crates/rusty-claude-cli/tests/plan_approve.rs
    - add tests per §11
  (ONLY if discovery proves necessary) a small helper in main.rs or a2-plan-runner for the file write,
    kept outside the scope-sentinel forbidden-API region, or with the sentinel/tests updated deliberately and justified.
DISALLOWED: any other crate/file; rust/crates/a2-plan-runner write/apply logic; schemas/; scripts/;
  src/ (non-rust); services/; runtime; Dockerfiles.
Prefer capturing the exact bytes passed to stdout by emit_approval_result so stdout and file are identical.
```

---

## 8. TTY Guard Preservation

```text
Keep the input.rs:141 dual-terminal guard intact; the approvable-bundle non-TTY refusal (exit 7,
  "approval-stdin-not-tty") must still fire BEFORE any file is written.
The approval-result still prints to stdout exactly as today; the file is an ADDITIONAL sink.
The operator still types `apply <step_id> <preview_sha256>` at a real TTY.
```

---

## 9. CLI Flag Contract

```text
--approval-result-output <path>  (optional, opt-in, single path arg)
absent → unchanged behavior (stdout only, no file).
not a pre-approval flag; does not relax any guard.
choose ONE canonical flag name; if not --approval-result-output, justify against source/CLI convention.
define and document the existing-file behavior (recommend: refuse to overwrite, or atomic-write-on-success only).
```

---

## 10. Approval-Result File Contract

```text
written ONLY on decision==approved.
content == the stdout approval-result (schema a2-l2b-approval-result.v1), semantically identical (ideally exact bytes).
atomic write (temp + rename); no partial file on error; never writes outside the supplied path.
carries: schema_version, decision=approved, step_id=preview_target_update,
  preview_sha256=1c856762e360397ecac2f8f64a0ac2ac1fb968963ba832e562892d424805da10 (for the proven preview).
```

---

## 11. Test Plan

```text
file written on successful approval; schema a2-l2b-approval-result.v1; decision approved; step_id + preview_sha256 match record.
persisted file semantically identical to stdout approval-result.
NO file on rejected/mismatched approval line.
NO file when TTY guard fails (non-TTY approvable bundle → exit 7).
--yes/--auto/--force/--allow-write/--preapproved/--batch remain refused.
absent the flag → unchanged (stdout-only, no file).
existing plan_approve + plan_apply tests pass.
scope-sentinel / source-grep forbidden-API tests pass.
(optional, offline) the persisted file is accepted by `claw plan apply-bundle <preview-generator-result.json> <approval-result.json>` in a scoped/dry test — NOT a live apply.
```

---

## 12. Validation Plan

```bash
cargo fmt
cargo test -p rusty-claude-cli plan_approve
cargo test -p rusty-claude-cli plan_apply
cargo test --workspace
cargo clippy --workspace
```

(Exact filters may be adjusted after discovery, with justification. CI runs the full suite — keep it green.)

---

## 13. No-Apply Boundary

```text
This lane persists approval evidence only. It does NOT apply, does NOT run apply-bundle, does NOT
  write the live target. S2E apply remains a separate, token-gated lane. A2 remains the only write path.
```

---

## 14. Failure Handling

```text
STOP if: token missing; discovery shows the change cannot be done without relaxing the TTY guard or
  the approval-line requirement; a pre-approval flag would be needed; the file would be written on a
  non-approved/refused/non-TTY path; the persisted result would differ from stdout; the change would
  touch out-of-scope files; the scope-sentinel tests would break; model/broker/runtime/:11434/secrets
  would be involved. No retries that weaken safety. Prefer to STOP-and-report over a risky edit.
```

---

## 15. Final Report Template

```text
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: S2D_APPROVAL_RESULT_PERSISTENCE_IMPLEMENTATION
TOKEN: present / exact:
WORKTREE / BRANCH:
FLAG: name / optional / opt-in:
TTY GUARD: preserved: / non-TTY refusal still fires before file write:
FILE CONTRACT: written only on approved: / atomic: / semantically identical to stdout: / existing-file behavior:
SCOPE: files changed (exact list): / out-of-scope touched (must be none):
TESTS: plan_approve: / plan_apply: / workspace: / clippy: / scope-sentinel: / new persistence tests:
NO-APPLY: apply run (must be NO): / target written (must be NO): / model-broker-runtime (must be NO):
STOP GATES HIT: none | details
NEXT BEST LANE: build claw + operator runs approve --approval-result-output at a real TTY to PRODUCE the persisted approval-result; then S2E apply gate (still token-gated)
```

---

> **Reminder:** DRAFT of a future CODE lane. It edits Rust only within the scoped approve/test
> surface, preserves the TTY guard, adds no pre-approval flags, and writes the persisted file only on
> a successful real-TTY approval. It runs no apply and writes no target. S2E apply stays BLOCKED
> until a legitimate persisted approval-result actually exists.
