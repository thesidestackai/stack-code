# Stack-Code N6A — helperRunner / a2-ide-harness.sh Execution Subcommand Allowlist Design
# 2026-07-07

> **Docs-only design.** This document does not implement N6A.
> This document does not authorize live execution.
> This document does not authorize package-plan / package-commit / package-push / package-pr runs.
> This document does not authorize apply.
> This document does not authorize merge.
> Implementation requires a separate exact activation token.
> Execution still requires separate N6 sub-token approval.

---

## 1. Executive Summary

Phase N5 established a read-only gated-execution readiness board. Phase N6 scoped the first
execution-capable panel controls — per-rung buttons gated by runtime sub-tokens — but operator
decision D6 determined that **no execution subcommands exist yet** in the panel's helper layer.
The current `helperRunner.ts` ALLOWED_SUBCOMMANDS list is exclusively print/validate-only (10
entries); `scripts/a2-ide-harness.sh` explicitly states it "NEVER runs an A2 command."

Before any N6 execution button can dispatch a rung, the helper layer must gain four new
**execution-capable** subcommands: `package-plan`, `package-commit`, `package-push`,
`package-pr`. This N6A design document defines those subcommands — their command mappings,
flag models, safety constraints, guard updates, and tests — without implementing any of them.

---

## 2. Current Baseline

```text
Main head:    ea723ce (docs(a2): scope northstar ux phase n6)

helperRunner.ts
  Path:   ide/vscode/a2-harness-panel/src/helperRunner.ts
  Model:  print/validate-only; never executes claw; array-argv spawn only
  HELPER_BASENAME: "a2-ide-harness.sh"
  ALLOWED_SUBCOMMANDS (10):
    help, validate-input, print-preview, find-artifacts, print-approval,
    print-apply-bundle, print-apply, verify-final, audit-workspace,
    print-tier3-evidence
  ALLOWED_FLAGS: per-subcommand (workspace, plan, preview-bundle, approval-output,
                 preview-generator-result, approval-result, apply-bundle, target,
                 after-sha)
  CHAIN_WRITE_FRAGMENTS guard: refuses any flag value containing
    "claw plan run" | "claw plan approve" | "claw plan apply-bundle" | "claw plan apply"
  Spawn: cp.spawn(binary, args, {shell:false}); no exec/eval; lazy import

scripts/a2-ide-harness.sh
  Declared: "print/validate ONLY"; "NEVER runs an A2 command"
  9 active subcommands (same as ALLOWED_SUBCOMMANDS minus "help" default):
    validate-input, print-preview, find-artifacts, print-approval,
    print-apply-bundle, print-apply, verify-final, audit-workspace,
    print-tier3-evidence
  ONE bounded exec exception:
    print-tier3-evidence invokes a2-evidence-collector (read-only, writes nothing,
    no claw/model/broker/runtime) by exact basename, array-argv, no shell.
  Exit codes: EXIT_OK=0, EXIT_USAGE=2, EXIT_VALIDATION=3

Guards (scripts/run-guards.js):
  Audits all src/*.ts files after stripping comments and string literals.
  8 check categories:
    NETWORK, WATCHER, POLLING, FS, SECRET_API, CHAIN_WRITE, APPROVAL_COMPOSE,
    PROCESS_SPAWN (all files except helperRunner.ts)
  helperRunner.ts-specific: HELPER_RUNNER_FORBIDDEN (no exec/eval/sync/shell:true)
  Structural: asserts helperRunner.ts exists as single spawn boundary.
  Wired: npm run lint → node scripts/run-guards.js
  Test:  test/guards.test.ts (spawnSync node scripts/run-guards.js; asserts exit 0)
```

---

## 3. Critical D6 Finding

```text
D6 FINDING (verified 2026-07-07 by direct source inspection):

  helperRunner.ts ALLOWED_SUBCOMMANDS contains NO package-plan, package-commit,
  package-push, or package-pr entries.

  a2-ide-harness.sh dispatch (main() case statement, line 535) contains NO
  package-plan, package-commit, package-push, or package-pr cases.

  The helper is currently fully print/validate-only.

  Any N6 execution button dispatching package-plan (or any other execution rung)
  through helperRunner.ts will receive a HelperRunnerRefusal:
    "refused: subcommand is not in the read-only/print allowlist: package-plan"

  Consequence: N6 execution buttons CANNOT fire until N6A is implemented and
  merged. N6 controls correctly degrade to N5 display-only mode in the interim.
```

---

## 4. Why N6A Exists

The N6 scope doc (§6, §8, §11) specified per-rung execution via helperRunner.ts, assuming the
four package subcommands already existed. Discovery revealed they do not. N6A fills this gap by:

1. Defining the exact command each subcommand runs.
2. Defining the flag model (required + forbidden).
3. Specifying the safety model change the helper must declare.
4. Updating `CHAIN_WRITE_FRAGMENTS` scope to match execution-capable posture.
5. Specifying new guard rules that cover execution-specific risks.
6. Providing the test plan that must pass before the N6A implementation PR merges.

---

## 5. Non-Goals

```text
- No implementation of any subcommand (this document only).
- No modification of helperRunner.ts, a2-ide-harness.sh, or any source file.
- No modification of run-guards.js, package.json, or CI.
- No live A2 run, no preview, no approval, no apply-bundle, no apply.
- No PR open, no push, no merge.
- No runtime / model / broker / Vault call.
- No extension of N6 state machine, render, or view (those remain N6's scope).
- No apply subcommand (apply remains out of scope for N6A and N6).
- No merge subcommand (merge is human-only at all levels).
- No PR mark-ready subcommand.
- No force-push subcommand.
- No N7+ scope.
```

---

## 6. Threat Model

```text
T1 — Execution via print-only path: an operator or test tricks the helper into
     running a command that was formerly only printed. Guard: execution subcommands
     have distinct names (package-*) and the dispatch is strict case-match only.

T2 — Flag injection: a malicious value in --workspace or --plan causes the helper
     to execute unexpected commands. Guard: the existing parse_opts+require_opt
     framework; CHAIN_WRITE_FRAGMENTS refusal on panel side; shell-quoting (shq)
     on shell side; no eval, no shell: true.

T3 — Force-push through package-push: the push rung uses --force or --force-with-lease.
     Guard: package-push must pass --no-force-flag (or equivalent) and the helper
     must validate that the push command it runs contains no --force flag.

T4 — PR auto-approve / auto-merge: package-pr silently marks PR ready or merges.
     Guard: only `gh pr create --draft` is produced; no --ready, no merge command.
     A new guard rule asserts `gh pr merge` must not appear in execution subcommands.

T5 — Apply via package-plan: package-plan is a preview runner; it must not write
     the target. Guard: package-plan calls `claw plan run ...--workspace-write-preview`
     only; it never calls `claw plan apply`; the target file is not touched.

T6 — Chain escalation: package-commit follows package-plan without operator review.
     Guard: helperRunner.ts never auto-sequences rungs; the extension wiring in
     extension.ts must not auto-trigger the next rung on completion.

T7 — Staging of unexpected files: package-commit runs `git add` beyond declared scope.
     Guard: the `--files` flag accepts an explicit list only; no `git add .` or
     `git add -A` is allowed in the helper or runner.

T8 — Sub-token bypass: a button fires without the matching sub-token being active.
     Guard: helperRunner.ts allowlist gates on subcommand name; the N6 state machine
     gates on sub-token; both layers must pass independently.

T9 — Secrets in flag values: a --workspace path resolves to a Vault or secret surface.
     Guard: warn_if_sensitive_path() in the helper covers vault/secret/.env paths;
     this must be called for all execution subcommand flag values.

T10 — Law 1 bypass: an execution subcommand introduces an implicit :11434 call.
      Guard: the helper must call claw via the A2_CLAW env var (existing mechanism);
      claw's own model routing must go through the SideStack broker (:11435), never
      directly to :11434. The guards' NETWORK_PATTERNS already catch /\b11434\b/ in
      TypeScript; a new shell-script guard must cover the a2-ide-harness.sh surface.
```

---

## 7. Source Files Inspected

```text
Inspected (read-only, 2026-07-07):
  ide/vscode/a2-harness-panel/src/helperRunner.ts        (206 lines)
  scripts/a2-ide-harness.sh                              (556 lines)
  ide/vscode/a2-harness-panel/scripts/run-guards.js      (236 lines)
  ide/vscode/a2-harness-panel/test/guards.test.ts         (19 lines)
  docs/stack-code-northstar-ux-phase-n6-execution-boundary-scope.md
  handoffs/stack_code_northstar_ux_phase_n6_implementation_prompt_DRAFT_2026-07-07.md

No source file was modified.
```

---

## 8. Existing helperRunner.ts Behavior

```text
Safety declaration (header comment, lines 1-13):
  "Argv-bounded wrapper around the print/validate-only A2 IDE harness helper"
  "This module is the ONLY place this package spawns any process."
  "It accepts ONLY an allowlisted read-only/print subcommand"
  "NEVER a chain-write command, NEVER `claw`, and NEVER a shell."

Key mechanisms:
  buildHelperRequest(inv):
    1. Refuses empty or flag-shaped helper path.
    2. Refuses helper basename != "a2-ide-harness.sh".
    3. Refuses subcommand not in ALLOWED_SUBCOMMANDS.
    4. Refuses flag not in ALLOWED_FLAGS[subcommand].
    5. Refuses non-string or flag-shaped flag values.
    6. Refuses flag values containing any CHAIN_WRITE_FRAGMENT.
    Returns: { binary: helperPath, args: [subcommand, --key, value, ...] }

  defaultSpawnImpl():
    cp.spawn(binary, args, { stdio: ["ignore", "pipe", "pipe"], shell: false })
    No shell, no eval. stdout/stderr captured as strings. Exit code propagated.

Critical implication for N6A:
  CHAIN_WRITE_FRAGMENTS checks flag *values* (e.g., a workspace path that somehow
  contains "claw plan run"). For execution subcommands, the panel passes a workspace
  path and plan path — neither would contain a claw command fragment. So the existing
  CHAIN_WRITE_FRAGMENTS check does NOT block execution subcommands at the invocation
  level. However, the safety model comment ("never builds chain-write commands") must
  be updated when execution subcommands are added.
```

---

## 9. Existing a2-ide-harness.sh Behavior

```text
Header declares:
  "This script NEVER runs an A2 command."
  "This script calls NO model / NO broker / NO runtime; it never executes `claw`."
  "v0 is print/validate only, with ONE bounded read-only exec exception: print-tier3-evidence"

Structure:
  set -euo pipefail
  parse_opts(): --key value parser into associative array OPT
  require_opt(): fails if OPT[name] is absent
  A2_CLAW="${A2_CLAW:-$DEFAULT_CLAW}": claw binary reference (never called in v0)
  shq(): single-quote-safe printer for paths with spaces
  warn_if_sensitive_path(): warns (does not fail) on vault/secret/env paths

Dispatch (main() case statement, line 535):
  help|validate-input|print-preview|find-artifacts|print-approval|
  print-apply-bundle|print-apply|verify-final|audit-workspace|print-tier3-evidence

The ONLY execution in v0 (print-tier3-evidence):
  "$collector" "$ws"  — invokes a2-evidence-collector with basename guard, array-argv,
                        no shell, no claw. Fail-closed (non-zero → no stdout).

For N6A, execution subcommands would break the "never executes claw" invariant and
would require an updated safety model statement.
```

---

## 10. Existing Allowed Subcommands

```text
ALLOWED_SUBCOMMANDS (complete list, verified from source):
  1.  help
  2.  validate-input
  3.  print-preview
  4.  find-artifacts
  5.  print-approval
  6.  print-apply-bundle
  7.  print-apply
  8.  verify-final
  9.  audit-workspace
  10. print-tier3-evidence

Execution-capable subcommands in ALLOWED_SUBCOMMANDS: NONE.

N6A will add 4 new entries (proposed):
  11. package-plan
  12. package-commit
  13. package-push
  14. package-pr
```

---

## 11. Existing Flag / Argument Model

```text
ALLOWED_FLAGS (verified from helperRunner.ts lines 39-50):
  help:                      []
  validate-input:            [workspace, plan]
  print-preview:             [workspace, plan]
  find-artifacts:            [workspace]
  print-approval:            [workspace, preview-bundle, approval-output]
  print-apply-bundle:        [preview-generator-result, approval-result]
  print-apply:               [apply-bundle]
  verify-final:              [workspace, target, after-sha]
  audit-workspace:           [workspace, target, after-sha]
  print-tier3-evidence:      [workspace]

Flag model rules (from buildHelperRequest):
  - All flags must be in the subcommand's ALLOWED_FLAGS list.
  - Flag values must be strings.
  - Flag values must not start with '-'.
  - Flag values must not contain any CHAIN_WRITE_FRAGMENT.
  - Passed as --key value pairs in array argv (never shell-interpolated by the runner).
```

---

## 12. Architecture Decision

### Option A — Extend Existing helperRunner.ts and a2-ide-harness.sh

**Approach:** Add the 4 new execution subcommands to the existing `ALLOWED_SUBCOMMANDS` list in
`helperRunner.ts` and add the corresponding `cmd_*` functions and case entries to
`a2-ide-harness.sh`. The HELPER_BASENAME guard and single-spawn-boundary remain unchanged.

**Pros:**
- Preserves the single spawn boundary. No new binary is added to `HELPER_BASENAME`.
- helperRunner.ts already enforces basename, subcommand, flag, and value guards. Extending
  the allowlist reuses all existing machinery.
- The existing parse_opts + require_opt + warn_if_sensitive_path framework works unchanged.
- Guards test continues to pass (helperRunner.ts remains the only file with spawn calls).
- Simpler to audit: one helper, one allowlist, one dispatch table.

**Cons:**
- Changes the helper's safety model from "print/validate-only" to "print/validate + controlled
  execution". The header comment and usage() block must be updated.
- Increases the consequence of a flag-injection bug in the execution subcommands (a bug in
  package-commit could stage wrong files).
- The a2-ide-harness.sh grows in surface area.

### Option B — New Dedicated Execution Helper

**Approach:** Create `scripts/a2-ide-harness-exec.sh`. helperRunner.ts would need a second
HELPER_BASENAME or a second runner module.

**Pros:**
- Clean separation: print helper stays print-only forever; exec helper is a new, clearly-named surface.

**Cons:**
- Violates the single-spawn-boundary invariant. Either a second `HelperRunnerExec` module must be
  added (a new spawn site), or helperRunner.ts's HELPER_BASENAME guard must be weakened to a set.
- The guards' structural assertion ("helperRunner.ts must exist as single spawn boundary") would
  need to be revised to include the new runner.
- Two binary basenames to audit; harder to verify that the execution helper can't be reached
  through the print-only path.
- More implementation surface for the same outcome.

**What evidence would change this recommendation:**
- If the print-only helper gains a security-critical invariant that cannot be expressed as a
  subcommand-level flag (e.g., a hardware TPM gate), a separate helper with its own trust chain
  would be warranted.
- If the shell script grows to >1000 lines and becomes unmaintainable as a single file, splitting
  into logical halves (print + exec) with shared functions becomes justified.

### Recommendation

**Option A — Extend existing helperRunner.ts and a2-ide-harness.sh.**

Rationale: The single-spawn-boundary is the strongest architectural safety guarantee the panel
has. The existing refusal machinery (basename, subcommand, flag, value guards) must cover
execution subcommands anyway; adding them to the same file extends existing protection rather
than creating a second, parallel one. The safety model update is a documentation change, not a
weakening of the guard mechanics.

---

## 13. Proposed Subcommand Matrix

```text
Subcommand     | Mutation level  | What it runs                  | Required-by-N6-sub-token
---------------|-----------------|-------------------------------|----------------------------
package-plan   | A2 preview      | claw plan run (preview only)  | APPROVED: N6 Package Plan Only
package-commit | Git local       | git add <files> + git commit  | APPROVED: N6 Package Commit Only
package-push   | Git remote      | git push (non-force)          | APPROVED: N6 Package Push Only
package-pr     | GitHub          | gh pr create --draft          | APPROVED: N6 Draft PR Only
```

```text
Mutation ordering (cumulative, each rung requires the previous one DONE in session):
  package-plan → package-commit → package-push → package-pr
```

```text
Key distinctions:
  package-plan   is the ONLY rung that calls claw. All others are pure git/gh.
  package-commit is the ONLY rung that writes to the local git object store.
  package-push   is the ONLY rung that sends data to the remote.
  package-pr     is the ONLY rung that creates a GitHub resource.
```

---

## 14. Proposed Subcommand: package-plan

```text
Purpose:
  Run the A2 preview chain for a package plan (claw plan run). Produces a preview
  bundle and preview_sha256. Does NOT write the target file. Equivalent to Step 1
  of the existing A2-L2b chain, but executed by the helper rather than printed.

Command produced by the helper:
  "$A2_CLAW" plan run "$PLAN" --workspace-root "$WS" --workspace-write-preview

  (Same command as print-preview prints, now actually executed.)

Required flags (from the panel):
  --workspace   <path>    absolute path to the A2 workspace root
  --plan        <path>    absolute path to the plan.yaml file

Forbidden flags (helper must refuse if supplied):
  --force | --yes | --batch | --no-tty | --approval | --apply | --target

What the helper does:
  1. parse_opts; require workspace, plan.
  2. warn_if_sensitive_path on workspace and plan.
  3. Validate workspace is a directory, plan is a file. Fail on missing (EXIT_VALIDATION).
  4. Validate plan.yaml has no absolute after_file (reuse validate-input logic).
  5. Execute: "$A2_CLAW" plan run "$PLAN" --workspace-root "$WS" --workspace-write-preview
     Array-argv, no shell. Pipe stdout+stderr. Propagate exit code.
  6. On non-zero exit: print stderr to stderr; exit non-zero (helper exits non-zero; panel
     transitions rung to N6_PACKAGE_PLAN_FAILED).

What the helper must NOT do:
  - Write the target file (claw plan run with --workspace-write-preview does NOT write target).
  - Call claw plan approve / apply-bundle / apply.
  - Accept --yes / --batch / --force flags.
  - Make a model / broker / runtime / Vault call directly.

Safety assertion:
  package-plan calls claw, but claw plan run --workspace-write-preview is a PREVIEW-ONLY
  command: it writes a preview bundle, not the target. The target is only written by
  claw plan apply (which is not called by any N6A subcommand).
```

---

## 15. Proposed Subcommand: package-commit

```text
Purpose:
  Stage an explicit, operator-declared file list and create a Git commit. This rung
  commits the package produced by package-plan. It implements the exact-path staging
  policy (no git add . or git add -A).

Commands produced by the helper:
  git -C "$WORKSPACE_GIT_ROOT" add -- <file1> <file2> ...
  git -C "$WORKSPACE_GIT_ROOT" commit -m "$MESSAGE"

  (Two commands, sequenced; if git add fails, git commit does not run.)

Required flags (from the panel):
  --workspace   <path>    absolute path to the git workspace root (same as package-plan)
  --files       <list>    comma-separated relative paths of files to stage (exact list only)
  --message     <string>  commit message string (operator-supplied; single-line enforced)

Forbidden flags (helper must refuse if supplied):
  --amend | --force | --all | --no-verify | --gpg-sign | --no-gpg-sign

What the helper does:
  1. parse_opts; require workspace, files, message.
  2. warn_if_sensitive_path on workspace.
  3. Validate workspace is a git repository (git -C "$WS" rev-parse --git-dir).
  4. Split files by comma; validate each is a non-empty relative path (no leading /;
     no .. traversal that escapes the workspace).
  5. Refuse if files list is empty or if --all / -A is somehow derived.
  6. Validate commit message: non-empty; single-line (no embedded newlines).
  7. Execute: git -C "$WS" add -- <file1> <file2> ...
  8. Execute: git -C "$WS" commit -m "$MESSAGE"
  9. Propagate exit code; print stdout/stderr from both commands.

What the helper must NOT do:
  - Run git add . or git add -A.
  - Amend an existing commit.
  - Skip hooks (--no-verify).
  - Force-sign or skip-sign unconditionally (use the repo's default gpg behavior).
  - Stage files outside the declared --files list.
  - Accept a multi-line commit message (guard against newline injection).
```

---

## 16. Proposed Subcommand: package-push

```text
Purpose:
  Push the committed package branch to the remote. Non-force only.

Command produced by the helper:
  git -C "$WS" push "$REMOTE" "$BRANCH"

  (No --force, no --force-with-lease, no --delete.)

Required flags (from the panel):
  --workspace   <path>    absolute path to the workspace (must be a git repo)
  --remote      <string>  remote name (e.g., origin); must match [a-zA-Z0-9_.-]+
  --branch      <string>  branch name to push; must match [a-zA-Z0-9/_.-]+
                          (the currently-checked-out branch, confirmed by the helper)

Forbidden flags (helper must refuse if supplied):
  --force | --force-with-lease | --delete | --all | --mirror | --tags | --no-verify

What the helper does:
  1. parse_opts; require workspace, remote, branch.
  2. Validate workspace is a git repo.
  3. Validate remote name matches safe pattern; refuse if it contains spaces or shell chars.
  4. Validate branch name matches safe pattern.
  5. Confirm that the local branch HEAD matches the declared branch (git branch --show-current).
     Refuse if there is a mismatch (prevents pushing an unexpected branch).
  6. Execute: git -C "$WS" push "$REMOTE" "$BRANCH"
     (not git push --force, not git push --delete, not git push -u alone without branch)
  7. Propagate exit code.

What the helper must NOT do:
  - Force-push under any flag spelling.
  - Push to a protected branch name (main, master) without operator having explicitly
    declared it as the target branch in --branch (this is a warning, not a hard block,
    since feature branches are the expected target).
  - Delete remote branches.
  - Push all branches or tags.
```

---

## 17. Proposed Subcommand: package-pr

```text
Purpose:
  Open a DRAFT GitHub PR from the pushed package branch. Draft only. No mark-ready,
  no approve, no merge.

Command produced by the helper:
  gh pr create --draft --base "$BASE" --head "$HEAD" --title "$TITLE" --body "$BODY"

Required flags (from the panel):
  --workspace   <path>    absolute path to the workspace (used to detect gh repo)
  --base        <string>  base branch name (e.g., main); must match safe pattern
  --head        <string>  head branch name (the pushed package branch)
  --title       <string>  PR title; single-line; non-empty; max 256 chars
  --body-file   <path>    path to a file containing the PR body (to avoid shell injection
                          on long strings; body is passed to gh via file redirect or stdin)

Alternative body delivery (if gh supports it):
  gh pr create --draft ... --body-file "$BODY_FILE"
  or pipe: gh pr create --draft ... < "$BODY_FILE"
  (the exact mechanism is for the N6A implementation to determine based on gh version)

Forbidden flags (helper must refuse if supplied):
  --ready | --approve | --merge | --squash | --rebase | --fill | --no-maintainer-edit

What the helper does:
  1. parse_opts; require workspace, base, head, title, body-file.
  2. Validate workspace is a git repo with a detected gh remote.
  3. Validate base and head match safe branch name patterns.
  4. Validate title: non-empty, single-line, max 256 chars.
  5. Validate body-file exists and is a regular file (max 65536 bytes).
  6. Execute: gh pr create --draft --base "$BASE" --head "$HEAD" --title "$TITLE" \
                            --body-file "$BODY_FILE"
     Array-argv, no shell. Propagate exit code and stdout (gh outputs PR URL on success).
  7. Capture and return the PR URL from stdout.

What the helper must NOT do:
  - Open a non-draft PR.
  - Mark any PR ready, approve, or merge.
  - Accept --fill (which would use commit messages without operator review).
  - Make a model / broker / Vault call.
```

---

## 18. Proposed ALLOWED_SUBCOMMANDS Entries

```typescript
// Proposed additions to helperRunner.ts ALLOWED_SUBCOMMANDS:
export const ALLOWED_SUBCOMMANDS = [
  // ... existing 10 entries ...
  "package-plan",    // NEW: executes claw plan run (preview only; no target write)
  "package-commit",  // NEW: executes git add + git commit (exact-path staging only)
  "package-push",    // NEW: executes git push (non-force only)
  "package-pr",      // NEW: executes gh pr create --draft (draft only; no mark-ready/merge)
] as const;
```

```text
Note: The comment on each new entry must declare its mutation class and forbidden actions.
The header comment on helperRunner.ts must be updated to replace:
  "It accepts ONLY an allowlisted read-only/print subcommand"
with:
  "It accepts ONLY allowlisted subcommands: read-only/print subcommands (existing) and
   controlled-execution subcommands (package-*) that are each explicitly gated by a
   runtime N6 sub-token before the panel will invoke them."
The new header must also update:
  "this runner never builds `claw plan run/approve/apply-bundle/apply`"
to:
  "this runner never builds `claw plan approve/apply-bundle/apply`. package-plan may
   dispatch claw plan run for the preview phase only (not the apply phase)."
```

---

## 19. Proposed ALLOWED_FLAGS Entries

```typescript
// Proposed additions to helperRunner.ts ALLOWED_FLAGS:
export const ALLOWED_FLAGS: Record<HelperSubcommand, readonly string[]> = {
  // ... existing 10 entries ...
  "package-plan":   ["workspace", "plan"],
  "package-commit": ["workspace", "files", "message"],
  "package-push":   ["workspace", "remote", "branch"],
  "package-pr":     ["workspace", "base", "head", "title", "body-file"],
};
```

---

## 20. Per-Subcommand Required Flags

```text
package-plan:   workspace (directory), plan (file path)
package-commit: workspace (git repo), files (comma-separated relative paths), message (string)
package-push:   workspace (git repo), remote (safe name), branch (safe name)
package-pr:     workspace (git repo), base (branch), head (branch), title (string), body-file (file path)
```

---

## 21. Per-Subcommand Forbidden Flags

```text
package-plan:
  --force, --yes, --batch, --no-tty, --approval, --apply, --target
  (any flag that could route to apply or interactive approval)

package-commit:
  --amend, --force, --all, --no-verify, --patch, --interactive
  (anything that bypasses exact-path staging or hook enforcement)

package-push:
  --force, --force-with-lease, --force-if-includes, --delete, --all, --mirror, --tags
  (any form of force-push or non-targeted push)

package-pr:
  --ready, --approve, --merge, --squash, --rebase, --fill, --no-maintainer-edit,
  --auto-merge
  (anything beyond draft creation)
```

The helper must validate absent forbidden flags. Since helperRunner.ts passes only declared
ALLOWED_FLAGS and refuses anything else, the "forbidden flag" check is implicit: any of the
above would cause a HelperRunnerRefusal at the runner level. The shell script should additionally
validate that its input does not somehow contain these through environment inheritance.

---

## 22. Command Mapping

```text
N6 rung        | Helper subcommand | Shell command(s) the helper runs
---------------|-------------------|------------------------------------------------------
package-plan   | package-plan      | "$A2_CLAW" plan run "$PLAN" \
               |                   |   --workspace-root "$WS" --workspace-write-preview
package-commit | package-commit    | git -C "$WS" add -- <file1> <file2> ...
               |                   | git -C "$WS" commit -m "$MESSAGE"
package-push   | package-push      | git -C "$WS" push "$REMOTE" "$BRANCH"
package-pr     | package-pr        | gh pr create --draft --base "$BASE" --head "$HEAD" \
               |                   |   --title "$TITLE" --body-file "$BODY_FILE"
```

```text
No N6A subcommand runs:
  claw plan approve
  claw plan apply-bundle
  claw plan apply
  git push --force (or any --force variant)
  gh pr merge / gh pr ready / gh pr review --approve
  git add . / git add -A
  git commit --amend
```

---

## 23. Workspace / Target Boundaries

```text
package-plan:
  - Writes into WS/.claw/ (preview bundle, preview_sha256) — this is expected and OK.
  - Does NOT write the target file.
  - Does NOT write outside the workspace's .claw/ subdirectory.
  - The helper must confirm after execution that no target-shaped file was created
    (optional post-exec check: stat the declared target path if known; fail if modified).

package-commit:
  - Modifies the git object store of the workspace git repo.
  - Does NOT write outside the declared --files paths (each must be relative to workspace).
  - Does NOT create .claw artifacts or target files during commit.

package-push / package-pr:
  - No local filesystem writes.

All subcommands:
  - Must never write to runtime/, services/, hq/, vault/, or any path outside the
    workspace root (for package-plan/commit) or git repo boundary (for push/pr).
  - warn_if_sensitive_path() must be called for the workspace and any file path flag.
```

---

## 24. Git Mutation Boundaries

```text
package-plan:   NO git mutation (claw plan run modifies .claw/, not git history).
package-commit: MUTATES git history (local only; adds one commit to the current branch).
package-push:   MUTATES remote git (pushes local commit to remote; non-force only).
package-pr:     NO git mutation (gh pr create does not modify any ref).

Rules:
  - package-commit must only commit files declared in --files.
  - package-commit must not amend.
  - package-commit must not skip pre-commit hooks (--no-verify is forbidden).
  - package-push must push to exactly the declared --branch at the declared --remote.
  - package-push must never push to a wildcard ref or all-branches.
  - No subcommand may run git reset, git clean, git checkout ., git restore ., or
    any destructive git command.
```

---

## 25. Push Boundary

```text
Rule: git push ONLY. No --force, --force-with-lease, --force-if-includes, --delete,
      --mirror, --all, --tags.
Enforcement:
  1. helperRunner.ts ALLOWED_FLAGS for package-push does not include force-related flags.
     Any attempt to pass a --force flag through the panel is refused by buildHelperRequest.
  2. a2-ide-harness.sh cmd_package_push() must hard-code the push command without --force.
     It must NOT interpolate any operator flag into the git push invocation that could
     introduce a force variant.
  3. A new guard rule in run-guards.js must scan the helper's output (if testable) or the
     shell script source for forbidden push flags (see §28 Guard Rules).
```

---

## 26. PR Draft Boundary

```text
Rule: gh pr create --draft ONLY.
  - No --ready or -R flag.
  - No gh pr ready <PR> call.
  - No gh pr merge call.
  - No gh pr review --approve call.
  - No --auto-merge.
  - No --fill (which bypasses operator PR body review).

The PR remains a draft until the operator manually marks it ready through the GitHub UI.
This is a human-only action at all N6 levels.

Enforcement:
  cmd_package_pr() in the shell script hard-codes --draft and no ready/merge flags.
  The ALLOWED_FLAGS for package-pr does not include ready, merge, squash, or rebase flags.
```

---

## 27. Merge Boundary

```text
Merge is human-only. No N6A subcommand triggers, approaches, or implies a merge.

- MERGED and PR_APPROVED remain N6_FORBIDDEN_TARGETS (N6 scope doc §22).
- gh pr merge, git merge, squash-merge, and rebase-merge are not implemented and
  must not be added to any helper subcommand.
- The presence of a DRAFT PR (from package-pr) does not imply or enable merge.
```

---

## 28. Apply Boundary

```text
Apply is explicitly out of scope for N6A and N6.

- No subcommand calls claw plan apply, claw plan apply-bundle, or claw plan approve.
- package-plan calls claw plan run --workspace-write-preview (preview only).
- The distinction: claw plan run writes the preview bundle; claw plan apply writes the target.
- N6A must not add apply, apply-bundle, or approve subcommands.
- Apply remains a separately-scoped higher-gated future lane (N7+).

The existing CHAIN_WRITE_FRAGMENTS in helperRunner.ts already covers:
  "claw plan approve", "claw plan apply-bundle", "claw plan apply"
These fragments must remain in CHAIN_WRITE_FRAGMENTS and continue to be refused for
all flag values even after N6A execution subcommands are added.
```

---

## 29. Runtime / Model / Broker / Vault Boundary

```text
N6A subcommands call:
  claw (via package-plan)
  git  (via package-commit, package-push)
  gh   (via package-pr)

N6A subcommands must NOT call:
  - Any model endpoint (/v1/chat/completions or equivalent).
  - The SideStack broker (:11435 or any port) directly.
  - Any /status/vram or runtime-status endpoint.
  - Vault (vault kv get, vault read, etc.).
  - Any credential manager or secret storage API.

claw plan run is expected to call the broker internally (as part of the A2 preview
chain). This is N6A's indirect model exposure: the helper calls claw, which calls
the broker, which calls Ollama. The panel itself makes no direct model/broker call.

The existing NETWORK_PATTERNS guard (run-guards.js) catches direct broker/ollama/
:11434 calls in TypeScript. The shell script surface is NOT currently guarded by
run-guards.js — a new guard for the shell script is specified in §31.
```

---

## 30. Raw :11434 Law 1 Boundary

```text
Law 1: raw :11434 app inference is unconditionally forbidden.

For N6A:
  - helperRunner.ts: the existing NETWORK_PATTERNS guard catches /\b11434\b/ in
    TypeScript source. No change needed for this guard.
  - a2-ide-harness.sh: the shell script is NOT currently checked by run-guards.js.
    package-plan calls claw, which handles model routing internally. The shell script
    itself must NOT hardcode :11434 or any raw Ollama endpoint.
  - Proposed new guard (§31): a static grep on a2-ide-harness.sh for "11434" and
    direct Ollama/broker URLs, run as part of CI or the guards suite.

The SideStack broker (:11435) handles all app inference. claw's internal routing
must route through the broker; this is a claw-level constraint, not a helper-level one.
The helper cannot enforce claw's internal routing, but it must not introduce a bypass.
```

---

## 31. Token / Sub-Token Relationship

```text
Level 1 (implementation token — unlocks N6A implementation lane):
  APPROVED: Implement Stack-Code N6A helper execution allowlist
  Scope: authorizes adding package-plan/commit/push/pr to helperRunner.ts ALLOWED_SUBCOMMANDS
         and the corresponding cmd_* functions to a2-ide-harness.sh.
  Does NOT authorize: live package execution, N6 state machine changes, or any src/ edit
                      beyond helperRunner.ts + a2-ide-harness.sh.

Level 2 (N6 sub-tokens — runtime operator input, per session):
  APPROVED: N6 Package Plan Only     → enables package-plan button in panel
  APPROVED: N6 Package Commit Only   → enables package-commit button in panel
  APPROVED: N6 Package Push Only     → enables package-push button in panel
  APPROVED: N6 Draft PR Only         → enables package-pr button in panel

  These tokens are NOT implied by the Level 1 token.
  These tokens are NOT active until the operator supplies them in the live VS Code session.
  These tokens require the corresponding subcommand to be in ALLOWED_SUBCOMMANDS (N6A must
  have merged first) AND the rung to be READY (N6 state machine) before the button appears.
```

---

## 32. UI Relationship To N5/N6

```text
N5 (current):
  Display-only. Execution buttons do not exist. All rungs show N5 readiness only.

N6 (future, awaiting N6A + N6 implementation):
  D3 decision: execution controls appear in a SEPARATE N6 SECTION below the N5 board.
  D1 decision: operator supplies sub-tokens via VS Code input box.

  N6 section appears when ANY sub-token is active in the session.
  Without a sub-token, the section shows: "Supply an N6 execution sub-token to enable
  execution controls." The N5 board above remains always visible.

  Per-rung behavior:
    - Sub-token absent:      N5 display-only (AWAITING_TOKEN state)
    - Sub-token active + rung NOT READY: button hidden (AWAITING_READINESS state)
    - Sub-token active + rung READY:     "Run package-plan" button shown
    - RUNNING:               spinner shown; button hidden (no double-dispatch)
    - DONE (exit 0):         output section shown; next rung shows its state
    - FAILED (non-zero):     stop banner shown; no button; explicit "Retry" available

  N6 render section must not appear until N6A subcommands exist in the helper
  AND the panel build includes the N6 state + view + render implementation.
  Until then: N5 display-only mode is the correct behavior.
```

---

## 33. Guard Rules

### Existing rules (unchanged, must remain):
```text
FORBIDDEN-NETWORK:       catch all network / broker / :11434 / ollama calls in TypeScript
FORBIDDEN-WATCHER:       catch file-system watchers
FORBIDDEN-POLLING:       catch setInterval / setTimeout / setImmediate
FORBIDDEN-FS:            catch fs module use (panel reads no files directly)
FORBIDDEN-SECRET-API:    catch SecretStorage / context.secrets
FORBIDDEN-CHAIN-WRITE:   catch claw plan run/approve/apply-bundle/apply in live TypeScript
FORBIDDEN-APPROVAL-COMPOSE: catch approval-line composition
FORBIDDEN-PROCESS-SPAWN: catch child_process outside helperRunner.ts
FORBIDDEN-HELPER-RUNNER-API: catch exec/eval/sync/shell:true even in helperRunner.ts
```

### New rules required for N6A (proposed for run-guards.js):

```text
FORBIDDEN-FORCE-PUSH-IN-HELPER:
  Scan: any execution-related TypeScript or the transpiled args for the package-push
  subcommand must not include "--force", "--force-with-lease", "--force-if-includes".
  Implementation: assert that ALLOWED_FLAGS["package-push"] does not include any
  force-family string. (Simple array-membership check in run-guards.js.)

FORBIDDEN-PR-MARK-READY-IN-HELPER:
  Assert that ALLOWED_FLAGS["package-pr"] does not include "ready", "approve", or
  "merge". (Array membership check.)

FORBIDDEN-COMMIT-AMEND-IN-HELPER:
  Assert that ALLOWED_FLAGS["package-commit"] does not include "amend" or "all".

REQUIRED-SINGLE-SPAWN-BOUNDARY (already exists, must remain):
  Assert helperRunner.ts exists and no other src/*.ts file contains spawn calls.
```

### New shell-script guard (N6A-specific):

```text
SHELL-SCRIPT-SAFETY-GREP (proposed as a standalone check script or CI step):
  Target: scripts/a2-ide-harness.sh
  Checks:
    - "11434" must not appear as a non-comment literal (grep -v '#' | grep '11434')
    - "git push --force" / "--force-with-lease" must not appear in package-push cases
    - "gh pr ready" / "gh pr merge" / "gh pr review --approve" must not appear
    - "claw plan approve" / "claw plan apply-bundle" / "claw plan apply" must not appear
      in the execution subcommand cases (package-plan/commit/push/pr)
    - "git add ." / "git add -A" must not appear in package-commit case
    - "git commit --amend" must not appear in package-commit case
  Implementation: a small script (e.g., scripts/check-harness-exec-safety.sh) that runs
  the above greps and exits non-zero on any match; wired to CI.
```

---

## 34. Safety Model Statement

```text
PROPOSED UPDATED SAFETY MODEL for a2-ide-harness.sh v1:

  # SAFETY (hard invariants this script preserves):
  #   - Preview does NOT write target.
  #   - package-plan calls claw plan run (preview only) — it writes the preview bundle,
  #     not the target. claw plan apply is never called by this script.
  #   - package-commit uses exact-path git add only; git add . and git add -A are forbidden.
  #   - package-push is non-force only; --force and all variants are forbidden.
  #   - package-pr opens a DRAFT PR only; --ready, --approve, and --merge are forbidden.
  #   - Approval does NOT write target; it requires a REAL interactive terminal (print-approval).
  #   - apply-bundle is the GENERATOR; it writes NO target (print-apply-bundle).
  #   - `claw plan apply` is the EXECUTOR; it is NOT called by this script in any mode.
  #   - No auto-approval, no hidden apply, no batch/--yes/fake-TTY.
  #   - This script calls NO model / NO broker / NO runtime directly.
  #   - This script calls claw (via package-plan only) for the preview phase.
  #     claw's internal model routing goes through the SideStack broker; raw :11434 is
  #     never referenced by this script.
  #   - Execution subcommands (package-*) require a matching N6 sub-token to be active
  #     in the calling VS Code panel session before the panel will invoke them.
  #   - Merge is human-only; no merge subcommand exists or will be added in N6A.

PROPOSED UPDATED helperRunner.ts HEADER:

  // Argv-bounded wrapper around the A2 IDE harness helper (scripts/a2-ide-harness.sh).
  // This module is the ONLY place this package spawns any process. It accepts ONLY
  // allowlisted subcommands: print/validate subcommands (10 existing) and controlled-
  // execution subcommands (package-plan/commit/push/pr, added by N6A). Print/validate
  // subcommands never execute A2 commands. Execution subcommands may execute claw (plan
  // run preview only), git add/commit, git push (non-force), or gh pr create (draft only).
  // The runner never builds claw plan approve/apply-bundle/apply. No exec/eval. No shell.
```

---

## 35. Required Tests For N6A Implementation

```text
test/n6aHelperAllowlist.test.ts

  helperRunner ALLOWED_SUBCOMMANDS:
    - "package-plan" is in ALLOWED_SUBCOMMANDS.
    - "package-commit" is in ALLOWED_SUBCOMMANDS.
    - "package-push" is in ALLOWED_SUBCOMMANDS.
    - "package-pr" is in ALLOWED_SUBCOMMANDS.
    - Total ALLOWED_SUBCOMMANDS count is exactly 14 (10 existing + 4 new).

  helperRunner ALLOWED_FLAGS:
    - package-plan flags = exactly ["workspace", "plan"].
    - package-commit flags = exactly ["workspace", "files", "message"].
    - package-push flags = exactly ["workspace", "remote", "branch"].
    - package-pr flags = exactly ["workspace", "base", "head", "title", "body-file"].
    - No force/amend/all/ready/approve/merge flag appears in any new entry.

  buildHelperRequest for new subcommands:
    - package-plan with valid flags → argv = ["package-plan", "--workspace", <ws>, "--plan", <plan>].
    - package-plan with unknown flag (e.g., --force) → throws HelperRunnerRefusal.
    - package-plan with CHAIN_WRITE_FRAGMENT in workspace value → throws HelperRunnerRefusal.
    - package-commit with valid flags → correct argv (subcommand + files + message).
    - package-commit with --amend flag → throws HelperRunnerRefusal.
    - package-push with valid flags → correct argv (no --force in output).
    - package-push with --force flag → throws HelperRunnerRefusal.
    - package-pr with valid flags → correct argv (includes --draft).
    - package-pr with --ready flag → throws HelperRunnerRefusal.

  Guards (n6aHelperAllowlist-specific):
    - ALLOWED_FLAGS["package-push"] does not include any element matching /force/.
    - ALLOWED_FLAGS["package-pr"] does not include any element matching /ready|approve|merge/.
    - ALLOWED_FLAGS["package-commit"] does not include "amend" or "all".

test/guards.test.ts (existing, must still pass after N6A):
    - scripts/run-guards.js exits 0 on the updated src/ tree.
    - (If new guard rules are added to run-guards.js, test they trigger on synthetic violations.)

Shell script integration test (proposed, separate from TypeScript tests):
    - package-plan dry-run: invoke the helper with a known workspace/plan; assert exit code and
      that the claw plan run command is executed (mock claw or check argv via A2_CLAW=cat).
    - package-commit: assert that exactly the declared --files are staged and committed.
    - package-push: assert that git push is called without --force.
    - package-pr: assert that gh pr create --draft is called and --ready is absent.
```

---

## 36. Validation Commands For Future Implementation

```text
After N6A implementation in its isolated worktree:

Guards check (mandatory):
  cd ide/vscode/a2-harness-panel
  node scripts/run-guards.js

Shell-script safety grep (new, see §33):
  scripts/check-harness-exec-safety.sh   # to be created in N6A implementation

TypeScript compile (mandatory):
  npx tsc -p ide/vscode/a2-harness-panel/tsconfig.json --noEmit

Test suite (mandatory, all must pass):
  npx tsc -p ide/vscode/a2-harness-panel/tsconfig.test.json
  npx mocha --require out-test/test-setup.js 'out-test/test/**/*.test.js'

Safety scans (mandatory):
  grep -r "11434" ide/vscode/a2-harness-panel/src/        → NONE
  grep -r "force" ide/vscode/a2-harness-panel/src/helperRunner.ts | grep ALLOWED_FLAGS → NONE
  grep -n "package-plan\|package-commit\|package-push\|package-pr" \
    ide/vscode/a2-harness-panel/src/helperRunner.ts        → exactly 8 lines (4 in ALLOWED_SUBCOMMANDS, 4 in ALLOWED_FLAGS)
  grep -n "git push --force\|--force-with-lease\|gh pr ready\|gh pr merge\|claw plan apply\b" \
    scripts/a2-ide-harness.sh                              → NONE

Forbidden surface check (mandatory):
  git diff HEAD --name-only | grep -vE \
    '^(ide/vscode/a2-harness-panel/src/helperRunner\.ts|scripts/a2-ide-harness\.sh|ide/vscode/a2-harness-panel/scripts/run-guards\.js|ide/vscode/a2-harness-panel/test/n6a.*\.test\.ts|scripts/check-harness-exec-safety\.sh)$'
  → NONE (no files outside the allowlist)
```

---

## 37. STOP Gates

```text
STOP if package-plan produces the target file (claw plan apply was called, not claw plan run).
STOP if package-commit stages files not in the declared --files list.
STOP if package-push uses --force, --force-with-lease, or --force-if-includes.
STOP if package-pr opens a non-draft PR or includes --ready, --approve, or --merge.
STOP if any execution subcommand calls claw plan approve/apply-bundle/apply.
STOP if any execution subcommand calls apply, merge, or gh pr ready.
STOP if any execution subcommand introduces a raw :11434 reference.
STOP if any execution subcommand makes a direct model/broker/Vault call.
STOP if helperRunner.ts gains a second spawn site beyond defaultSpawnImpl.
STOP if a2-ide-harness.sh grows a second execution binary beyond a2-evidence-collector
     and the N6A-defined claw/git/gh callsites.
STOP if run-guards.js exits non-zero after N6A changes.
STOP if the forbidden surface check finds any file outside the declared N6A scope.
STOP if N6 state machine, render, or view files are modified in the N6A lane.
STOP if the implementation request arrives without the exact Level 1 activation token.
STOP if N7+ behavior is introduced.
```

---

## 38. Future Implementation Token

```text
This document does not authorize implementation.

Implementation of N6A helper execution allowlist requires the following exact activation
token as the FIRST NON-EMPTY LINE of the implementation prompt:

  APPROVED: Implement Stack-Code N6A helper execution allowlist

This token authorizes ONLY:
  (a) Adding package-plan / package-commit / package-push / package-pr to
      helperRunner.ts ALLOWED_SUBCOMMANDS and ALLOWED_FLAGS.
  (b) Adding the corresponding cmd_package_plan / cmd_package_commit / cmd_package_push /
      cmd_package_pr functions to scripts/a2-ide-harness.sh.
  (c) Updating the safety model header comments in both files.
  (d) Adding the required tests (test/n6aHelperAllowlist.test.ts) and the new shell-script
      safety check (scripts/check-harness-exec-safety.sh).
  (e) Extending run-guards.js with the new N6A guard rules (§33).

This token does NOT authorize:
  - Live package-plan / package-commit / package-push / package-pr execution.
  - N6 state machine, render, view, or extension changes.
  - Any source file outside the 5 named N6A surfaces above.
  - N7+ scope.
  - apply / merge / mark-ready.

Live execution still requires separate N6 sub-tokens (operator-supplied at runtime):
  APPROVED: N6 Package Plan Only
  APPROVED: N6 Package Commit Only
  APPROVED: N6 Package Push Only
  APPROVED: N6 Draft PR Only
```

---

## 39. Out Of Scope

```text
The following are NOT in N6A scope:

- N6 state machine (N6State, assertN6Safe, N6_FORBIDDEN_TARGETS).
- N6 trust levels (EXECUTION_OBSERVED, EXECUTION_FAILED).
- N6 view (buildN6View) or render section (n6Block, n6RungHtml).
- N6 extension wiring (recomputeViews, model()).
- Sub-token delivery UI (VS Code input box for D1).
- apply / apply-gate / apply-bundle.
- gh pr merge / gh pr ready / gh pr review --approve.
- Any model / broker / runtime / Vault call.
- Raw :11434 inference.
- Force-push of any kind.
- Merge automation.
- Any CI/CD pipeline changes beyond adding check-harness-exec-safety.sh.
- Dependency changes (no new npm packages).
- HQ / SideStack services / runtime config.
- N7+ candidates.
```

---

## 40. Residual Operator Decisions

```text
R1 — A2_CLAW env var:
  The current a2-ide-harness.sh uses A2_CLAW to reference the claw binary, defaulting to
  the dated build artifact path. For N6A, the panel's extension.ts will need a way to
  supply A2_CLAW to the helper invocation. Proposed mechanism: a new ALLOWED_FLAGS entry
  (claw-binary) for package-plan only, or a workspace-level configuration file.
  Decision required before N6A implementation begins.

R2 — body-file delivery for package-pr:
  gh pr create accepts --body-file in gh >= 2.x. Operator must confirm the gh version
  in the runtime environment supports --body-file before N6A implementation commits to
  this flag. Fallback: pipe body via stdin using spawn's stdin pipe option.
  Decision required before N6A implementation begins.

R3 — Multi-file staging format for package-commit:
  The --files flag is proposed as comma-separated relative paths. This must not contain
  paths with commas in their names (unusual but possible). Alternative: repeated --file
  flags (ALLOWED_FLAGS would include "file" and the runner would collect all --file values).
  Decision required before N6A implementation begins.

R4 — Shell-script safety check script name/location:
  Proposed: scripts/check-harness-exec-safety.sh. If this conflicts with an existing
  script naming convention, rename before implementation.

R5 — N6 implementation sequencing after N6A:
  Once N6A merges, the N6 implementation must be updated to reflect D1–D7 decisions
  (already recorded), and must re-read this N6A design doc as the source of truth for
  the helperRunner subcommand names and flag models.
```

---

## 41. Next Lane Recommendation

```text
Name:      Stack-Code N6A Helper Execution Allowlist Design Review / Push PR
Objective: Review this local N6A design commit, push the docs branch, and open a
           GitHub PR against main (same pattern as N5 scope PR #153 and N6 scope PR #155).
Scope:     Read-only review + push + PR open. No file edits. No merge.
Mutation:  Push only (non-force). PR open only (no merge).
STOP gate: Do NOT push or open PR without reviewing:
           (1) R1–R5 residual decisions above — confirm or escalate each.
           (2) All STOP gates in §37 are not triggered by this docs commit.
           (3) The forbidden surface check (§36) shows only docs/stack-code-n6a-*.md.
           (4) The implementation token (§38) is stated correctly and inertly.
After PR reviewed and merged:
  Resolve R1–R5 and deliver the N6A implementation prompt:
  "APPROVED: Implement Stack-Code N6A helper execution allowlist"
  as the first non-empty line of the implementation session.
```

---

> **This document does not implement N6A.**
> **This document does not authorize live execution.**
> **This document does not authorize package-plan / package-commit / package-push / package-pr runs.**
> **This document does not authorize apply.**
> **This document does not authorize merge.**
> **Implementation requires: `APPROVED: Implement Stack-Code N6A helper execution allowlist`**
> **Execution still requires separate N6 sub-token approval.**
