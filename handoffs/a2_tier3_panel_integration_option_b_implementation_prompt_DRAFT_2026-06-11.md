# DRAFT — A2 Tier 3 Panel Integration Option B — Implementation Prompt (token-gated)

Status: DRAFT. This is a future implementation prompt. **Do not execute it from
this design lane.** It is produced by, and must be read alongside,
`docs/a2-tier3-panel-integration-option-b-design-scope.md`.

---

## 1. Required Approval Token

This lane MUST NOT begin (MUST NOT create a worktree, MUST NOT edit source) unless
the operator's prompt contains this EXACT token, verbatim:

```
APPROVED: Execute A2 Tier 3 panel integration Option B implementation
```

Without that exact token, STOP before creating a worktree and report BLOCKED.

This prompt additionally requires (all must hold or STOP):

```
Do not start without exact token.
Do not implement write-capable executor behavior.
Do not run collector/orchestrator/validate/apply (as control actions / lanes).
Do not add claw command execution.
Do not add arbitrary subcommand execution.
Do not add shell:true.
Do not add filesystem access in panel/webview.
Do not add network/model/broker/runtime/Vault paths.
Do not add raw :11434 app inference.
Do not add apply/approve/create/write/cleanup controls.
```

---

## 2. Role

You are a careful Stack-Code implementer operating under the A2 panel safety
model (`docs/a2-l4-ide-extension-panel-scope.md` §7, §14, §16). You add the
minimal read-only Option B refresh path and nothing else. You treat every guard
as a hard invariant, not a hurdle.

---

## 3. Objective

Add a read-only in-panel Tier 3 evidence **refresh** path so the operator can
pull the existing `a2-tier3-evidence-snapshot.v0` without manual paste, by:

- adding exactly one read-only helper subcommand: `print-tier3-evidence`;
- adding exactly that subcommand (and its narrow flag set) to the existing
  `helperRunner.ts` allowlist;
- routing its stdout through the EXISTING pure parser
  (`parseEvidenceSnapshot`) and the EXISTING fail-closed renderer;
- adding a read-only "Refresh Tier 3 Evidence Snapshot" command;
- keeping the operator-paste path as a manual fallback.

No new panel `fs`/`spawn`/network surface. Guards stay GREEN.

---

## 4. Source of Truth

```
docs/a2-tier3-panel-integration-option-b-design-scope.md   (this lane's design)
Option A merge: bcf7c8a5a945e6260b54189492d7afeb79dcbda8 (PR #133) on main
docs/a2-l4-ide-extension-panel-scope.md  §7, §14, §16
ide/vscode/a2-harness-panel/src/helperRunner.ts
ide/vscode/a2-harness-panel/src/tier3EvidenceSnapshot.ts
ide/vscode/a2-harness-panel/scripts/run-guards.js
scripts/a2-ide-harness.sh
rust/crates/a2-evidence-collector
```

---

## 5. Hard Boundaries

Do NOT:

```
turn the helper into a claw executor (no claw plan run/approve/apply-bundle/apply)
add any executor verb to ALLOWED_SUBCOMMANDS
add shell:true / exec / eval / execSync / spawnSync anywhere
add fs.* / workspace.fs / read*Sync / write*Sync to panel src
add network / fetch / broker / ollama / :11434 / openExternal to panel src
add SecretStorage / context.secrets / Vault / secret access
add apply/approve/create/write/cleanup controls to the UI
mutate .claw artifacts; create worktrees; write target files
call a model/broker/runtime; run raw :11434 inference
broaden the allowlist beyond the single print-tier3-evidence subcommand
edit the reused tier3EvidenceSnapshot.ts to add IO (it must stay pure)
run live smoke / collector / orchestrator / validate-lane / apply-lane as control actions
delete or force-modify any branch/worktree; run git clean / rm -rf / reset --hard
```

---

## 6. Clean Worktree Setup

```
control checkout: clean, on main, fast-forwarded to origin/main
fresh isolated worktree from origin/main (NOT /home/suki/stack-code)
new feature branch, e.g. feat/a2-tier3-panel-integration-optionB-YYYYMMDD
preflight: APPROVED_WORKTREE / APPROVED_BRANCH strict preflight; STOP on dirty/staged state
```

---

## 7. Discovery (read-only first)

```
re-read helperRunner.ts (ALLOWED_SUBCOMMANDS, ALLOWED_FLAGS, HELPER_BASENAME, refusals)
re-read tier3EvidenceSnapshot.ts (schema, fail-closed view, zero controls)
re-read run-guards.js + guards.test.ts (the guard set you must keep GREEN)
re-read extension.ts runSubcommand()/pasteEvidenceSnapshot() (wiring pattern)
re-read scripts/a2-ide-harness.sh (no-claw / no-runtime invariant; subcommand pattern)
read rust/crates/a2-evidence-collector (read-only producer; confirm not claw)
confirm acquisition decision B-1 (read existing .claw snapshot) vs B-2 (invoke collector)
```

---

## 8. Implementation Scope

```
1. scripts/a2-ide-harness.sh: add cmd_print_tier3_evidence (read-only print only;
   B-1: read existing snapshot artifact under .claw; B-2: invoke read-only collector
   array-argv, no shell, never claw). Preserve no-claw/no-runtime invariant.
2. helperRunner.ts: add "print-tier3-evidence" to ALLOWED_SUBCOMMANDS and
   "print-tier3-evidence": ["workspace"] to ALLOWED_FLAGS. Nothing else.
3. extension.ts: register a read-only "Refresh Tier 3 Evidence Snapshot" command
   that calls runSubcommand("print-tier3-evidence") and routes stdout into
   parseEvidenceSnapshot(...). Keep pasteEvidenceSnapshot as fallback.
4. render.ts: surface would_create_worktree:no / would_write_files:no rows; no controls.
5. tests: argv-audit + parser fail-closed (see §12). Keep guards GREEN.
```

The reused `tier3EvidenceSnapshot.ts` must stay pure (preferred output keeps it
untouched; if a wrapper schema is added, add a NEW module — do not add IO here).

---

## 9. Helper Subcommand Contract

```
print-tier3-evidence --workspace <path>
  read-only; prints existing Tier 3 evidence/status as JSON/text only
  emits a2-tier3-evidence-snapshot.v0-compatible JSON (+ would_* safety fields)
  would_create_worktree: false
  would_write_files: false
  writes no file; creates no .claw; creates no worktree; never executes claw
  fail-closed: missing/invalid source → fail-closed object/notice, no fabrication
```

---

## 10. HelperRunner Allowlist Contract

```
ALLOWED_SUBCOMMANDS gains EXACTLY: print-tier3-evidence
ALLOWED_FLAGS gains EXACTLY:        print-tier3-evidence -> ["workspace"]
no executor verb added (no run/approve/apply-bundle/apply)
HELPER_BASENAME unchanged (a2-ide-harness.sh)
CHAIN_WRITE_FRAGMENTS refusal unchanged
flag-shape / chain-write value refusals unchanged
array-argv only; shell:false; no exec/eval/spawnSync
```

---

## 11. UI Contract

```
command name: "Refresh Tier 3 Evidence Snapshot" (read-only; not run/apply/approve/create)
on invoke: runSubcommand("print-tier3-evidence") -> parseEvidenceSnapshot(stdout)
renders: status, subjects, rows, caveats, links, next safe action,
         would-create-worktree: no, would-write-files: no
forbidden controls: Run Collector/Orchestrator/Validate/Apply/Preview/Approval/
                    Apply Bundle/Apply, Create Worktree, Write Files, Clean Up, Approve
operator-paste path remains as a manual fallback
```

---

## 12. Tests Required

```
argv-audit:
  builds ["print-tier3-evidence","--workspace",<ws>] exactly
  shell remains false; flag-shaped + chain-write values still refused
  unapproved subcommand still refused pre-spawn
  exact-allowlist test updated to the new 10-entry sorted list
  no executor verb / no claw forwarded
parser/fail-closed:
  schema mismatch / invalid JSON / null → unsupported notice
  would_create_worktree != false or would_write_files != false → not rendered ready
guards:
  node scripts/run-guards.js PASS (unchanged guard set)
  npm test GREEN (unit + render + guards)
offline helper smoke:
  print-tier3-evidence --workspace <tmp> prints JSON, writes nothing, no .claw, no worktree,
  emits no `claw`
```

---

## 13. Guard Scans Required

```
node scripts/run-guards.js                 -> PASS
npm test                                   -> GREEN
grep: no fs./network/:11434/broker/ollama/shell:true added to panel src
grep: no claw plan run/approve/apply-bundle/apply literal in live panel code
confirm ONLY helperRunner.ts spawns
```

---

## 14. STOP Gates

```
STOP if exact token absent.
STOP if control checkout dirty / not on main / not fast-forwarded.
STOP if the change would broaden the allowlist beyond print-tier3-evidence.
STOP if any executor verb, shell:true, exec/eval/spawnSync is required.
STOP if any panel fs/network/broker/model/runtime/Vault path is required.
STOP if a `claw` command would be emitted/forwarded.
STOP if would_create_worktree / would_write_files cannot both be false.
STOP if any apply/approve/create/write/cleanup control would appear in the UI.
STOP if guards (run-guards.js) cannot stay GREEN without relaxation.
```

---

## 15. Commit Rules

```
exact-path staging only (no git add . / -A)
no destructive git (no clean / reset --hard / branch -D / worktree remove --force /
                    fetch --prune / push origin --delete)
do not delete the retained Option A refs
                    (feat/a2-tier3-panel-integration-optionA-20260611, local + remote)
one lane = one worktree = one branch = one PR
commit message scoped to the Option B refresh path
```

---

## 16. Final Report Template

```
CLASSIFICATION: PASS | PASS_WITH_NOTES | BLOCKED | FAIL
MODE: A2_TIER3_PANEL_INTEGRATION_OPTION_B_IMPLEMENTATION
TOKEN PRESENT: yes/no
BRANCH / WORKTREE / BASE / COMMIT:
FILES CHANGED:
HELPER SUBCOMMAND: print-tier3-evidence (read-only)
ALLOWLIST CHANGE: exactly print-tier3-evidence + ["workspace"]
UI: refresh command added; controls: none
ACQUISITION: B-1 (read existing) | B-2 (read-only collector invoke)
TESTS: argv-audit / parser fail-closed / guards / offline smoke
GUARDS: run-guards.js PASS; npm test GREEN
would_create_worktree: false ; would_write_files: false
SAFETY: source/helper edited as scoped; runtime untouched; no model/broker call;
        no Vault/secret; no raw 11434; no live smoke/collector/orchestrator/validate/apply lane;
        no branch/worktree deleted; no destructive commands
STOP GATES HIT: none | details
NEXT BEST LANE:
```
