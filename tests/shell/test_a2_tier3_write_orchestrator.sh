#!/usr/bin/env bash
# Offline gate-matrix test for scripts/a2-tier3-write-orchestrator.sh.
#
# No real `claw` invocation. No network. No live A2. No git mutation. No target
# write. Every case stages JSON fixtures under a temp dir and asserts on the
# orchestrator's REFUSAL behavior:
#   - validate-lane: the pure gate matrix (scope, denials win, plan targets,
#     dry-run-ready, operator approval, worktree-plan rules).
#   - apply-lane: refuses in a non-interactive context (TTY guard, exit 7) and
#     creates NO worktree — proving writes never start without a real terminal.
#
# This test runs nothing that writes a target. The denied-command fixtures use
# `claw plan apply ...` (live-A2 family) and `echo hi` (non-allowlisted) so the
# test source carries no destructive/runtime literal of its own.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
ORCH="${REPO_ROOT}/scripts/a2-tier3-write-orchestrator.sh"

if [[ ! -x "${ORCH}" ]]; then
  printf 'test setup: orchestrator missing or not executable at %s\n' "${ORCH}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t a2-tier3-orch.XXXXXX)"
cleanup() { rm -r -f "${WORK_DIR}"; }   # removes only this test's own temp dir
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

# A disposable-worktree-root path that does NOT exist (apply-lane must never
# create it; we assert its absence after the TTY refusal).
WTROOT="/mnt/vast-data/git-worktrees/__a2_tier3_orch_test_nonexistent__"

# write_lane <file> <operatorApproved> <base> <branch> <wtpath> <declared-json> <writes-json> <cmds-json>
write_lane() {
  python3 - "$@" <<'PY'
import json, sys
(f, approved, base, branch, wt, declared, writes, cmds) = sys.argv[1:9]
obj = {
  "objective": "test lane",
  "worktreePlan": {"worktreePath": wt, "branch": branch, "base": base},
  "declaredPaths": json.loads(declared),
  "proposedWrites": json.loads(writes),
  "proposedCommands": json.loads(cmds),
  "operatorApproved": approved == "true",
}
with open(f, "w") as fh: json.dump(obj, fh)
PY
}

# write_evidence <file> <ready> <wtpath>
write_evidence() {
  python3 - "$@" <<'PY'
import json, sys
(f, ready, wt) = sys.argv[1:4]
obj = {"ready": ready == "true", "worktreePath": wt,
       "summary": "test evidence", "wouldCreateWorktree": False, "wouldWriteFiles": False}
with open(f, "w") as fh: json.dump(obj, fh)
PY
}

# write_plan <file> <write_target.path> [after_file]
# Emits a realistic workspace-write plan. The file actually written is
# write_target.path; after_file is the byte source.
write_plan() {
  local f=$1 target=$2 after=${3:-materialized/x.after}
  {
    printf 'name: t\nmode: read-only\nmodel_tier: FAST\nsteps:\n'
    printf '  - id: w\n    mode: workspace-write\n    tools: [Write]\n'
    printf '    write_target:\n      path: %s\n      create_if_absent: true\n' "$target"
    printf '    after_file: %s\n' "$after"
  } >"$f"
}

# write_plan_no_target <file> — a read-only plan with NO write_target.
write_plan_no_target() {
  printf 'name: t\nmode: read-only\nmodel_tier: FAST\nsteps:\n  - id: r\n    description: read only\n    tools: [Read]\n' >"$1"
}

# run_case <name> <expected-exit> <subcmd...> -- runs the orchestrator, compares $?.
run_case() {
  local name=$1 expect=$2; shift 2
  local rc=0
  "${ORCH}" "$@" >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$expect" ]]; then
    PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS  %-52s (exit %s)\n' "$name" "$rc"
  else
    FAIL_COUNT=$((FAIL_COUNT + 1)); printf 'FAIL  %-52s (got %s, want %s)\n' "$name" "$rc" "$expect"
  fi
}

WT="${WTROOT}"
D="$WORK_DIR"
DECL="[\"$WT/src/a.ts\",\"$WT/src/b.ts\"]"

# ---- a GOOD baseline lane + ready evidence + valid plan --------------------
write_lane     "$D/good.json"     true  origin/main feat/x "$WT" "$DECL" "[\"$WT/src/a.ts\"]" '["npm test"]'
write_evidence "$D/ready.json"    true  "$WT"
write_plan     "$D/plan_ok.yaml"  "src/a.ts"
run_case "validate good lane"                0 validate-lane --approved-lane "$D/good.json"  --dry-run-evidence "$D/ready.json" --plan "$D/plan_ok.yaml"

# ---- operator approval gate ------------------------------------------------
write_lane "$D/noappr.json" false origin/main feat/x "$WT" "$DECL" '[]' '[]'
run_case "refuse: operatorApproved=false"    4 validate-lane --approved-lane "$D/noappr.json" --dry-run-evidence "$D/ready.json"

# ---- dry-run-ready gate ----------------------------------------------------
write_evidence "$D/notready.json" false "$WT"
run_case "refuse: dry-run evidence not ready" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/notready.json"

# ---- empty declared set ----------------------------------------------------
write_lane "$D/nodecl.json" true origin/main feat/x "$WT" '[]' '[]' '[]'
run_case "refuse: empty declared set"        4 validate-lane --approved-lane "$D/nodecl.json" --dry-run-evidence "$D/ready.json"

# ---- write outside declared set --------------------------------------------
write_lane "$D/woutside.json" true origin/main feat/x "$WT" "$DECL" "[\"$WT/src/c.ts\"]" '[]'
run_case "refuse: write not in declared set" 4 validate-lane --approved-lane "$D/woutside.json" --dry-run-evidence "$D/ready.json"

# ---- write under control checkout ------------------------------------------
write_lane "$D/wctrl.json" true origin/main feat/x "$WT" "$DECL" '["/home/suki/stack-code/x.ts"]' '[]'
run_case "refuse: write under control checkout" 4 validate-lane --approved-lane "$D/wctrl.json" --dry-run-evidence "$D/ready.json"

# ---- write traversal escaping the worktree ---------------------------------
write_lane "$D/wesc.json" true origin/main feat/x "$WT" "$DECL" "[\"$WT/../escape.ts\"]" '[]'
run_case "refuse: write traversal escapes worktree" 4 validate-lane --approved-lane "$D/wesc.json" --dry-run-evidence "$D/ready.json"

# ---- denied command (live-A2 chain family) — denials win -------------------
write_lane "$D/cdeny.json" true origin/main feat/x "$WT" "$DECL" "[\"$WT/src/a.ts\"]" '["claw plan apply /x"]'
run_case "refuse: denied command (denials win)" 4 validate-lane --approved-lane "$D/cdeny.json" --dry-run-evidence "$D/ready.json"

# ---- non-allowlisted command ----------------------------------------------
write_lane "$D/cnotallow.json" true origin/main feat/x "$WT" "$DECL" "[\"$WT/src/a.ts\"]" '["echo hi"]'
run_case "refuse: command not in allowlist"  4 validate-lane --approved-lane "$D/cnotallow.json" --dry-run-evidence "$D/ready.json"

# ---- worktree-plan rules ---------------------------------------------------
write_lane "$D/badbase.json" true main feat/x "$WT" "$DECL" '[]' '[]'
run_case "refuse: base not origin/main"      4 validate-lane --approved-lane "$D/badbase.json" --dry-run-evidence "$D/ready.json"

write_lane "$D/wtctrl.json" true origin/main feat/x "/home/suki/stack-code" '["/home/suki/stack-code/a.ts"]' '[]' '[]'
run_case "refuse: worktree is control checkout" 4 validate-lane --approved-lane "$D/wtctrl.json" --dry-run-evidence "$D/ready.json"

write_lane "$D/brmain.json" true origin/main main "$WT" "$DECL" '[]' '[]'
run_case "refuse: mutation branch is main"   4 validate-lane --approved-lane "$D/brmain.json" --dry-run-evidence "$D/ready.json"

# ---- plan write_target gates (the file actually written is write_target.path) ---
write_plan "$D/plan_wtabs.yaml" "/etc/evil.conf"
run_case "refuse: plan write_target absolute" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_wtabs.yaml"

write_plan "$D/plan_outside.yaml" "src/zzz.ts"
run_case "refuse: plan write_target not in declared set" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_outside.yaml"

write_plan "$D/plan_wtesc.yaml" "../escape.ts"
run_case "refuse: plan write_target traversal escape" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_wtesc.yaml"

# after_file (byte source) must be workspace-relative even when the target is OK.
write_plan "$D/plan_afabs.yaml" "src/a.ts" "/etc/passwd"
run_case "refuse: plan after_file (source) absolute" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_afabs.yaml"

# a write lane whose plan declares no write_target writes nothing.
write_plan_no_target "$D/plan_notarget.yaml"
run_case "refuse: plan declares no write_target" 4 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_notarget.yaml"

# the GOOD plan's write_target (src/a.ts) IS in the declared set -> accepted.
run_case "accept: plan write_target in declared set" 0 validate-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_ok.yaml"

# ---- apply-lane TTY guard (non-interactive context) ------------------------
# stdin/stdout are not a TTY under the test runner, so apply-lane must refuse at
# the TTY gate (exit 7) AFTER the pure gates pass, and create NO worktree.
run_case "apply-lane refuses off-TTY (exit 7)" 7 apply-lane --approved-lane "$D/good.json" --dry-run-evidence "$D/ready.json" --plan "$D/plan_ok.yaml"

if [[ -e "$WTROOT" ]]; then
  FAIL_COUNT=$((FAIL_COUNT + 1)); printf 'FAIL  %-52s (worktree was created!)\n' "apply-lane created no worktree"
else
  PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS  %-52s (no worktree)\n' "apply-lane created no worktree"
fi

# ---- static invariants: approval gate must not be weakened by UX changes ---
# These guard the real-TTY human-typed approval against accidental regression:
# the drive step must never pipe into claw approve, never compose the approval
# line, and the apply-lane TTY gate + clear "not stuck" UX must remain.
static_assert() {
  local name=$1 pattern=$2 want=$3   # want = present|absent
  if grep -nEq -- "$pattern" "$ORCH"; then got=present; else got=absent; fi
  if [[ "$got" == "$want" ]]; then
    PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS  %-52s (%s)\n' "$name" "$got"
  else
    FAIL_COUNT=$((FAIL_COUNT + 1)); printf 'FAIL  %-52s (got %s, want %s)\n' "$name" "$got" "$want"
  fi
}

static_assert "real-TTY gate present (apply-lane)"        '! -t 0 \|\| ! -t 1'                            present
static_assert "TTY refusal returns EXIT_TTY"              'return \$EXIT_TTY'                             present
static_assert "no pipe of echo/printf/yes into claw"      '(echo|printf|yes)[^|]*\|[[:space:]]*"\$A2_CLAW"' absent
static_assert "no composed apply <id> <hex> line"         'appl[y][[:space:]]+[^[:space:]<]+[[:space:]]+[0-9a-f]{16,}' absent
static_assert "approval not auto-typed (explicit note)"   'never types, pipes, or composes'              present
static_assert "interactive 'not stuck' banner present"    'it is NOT stuck'                              present
static_assert "per-step approve exit diagnostics present" 'approval REFUSED by claw'                     present

# ---- summary ---------------------------------------------------------------
printf -- '----\n%d passed, %d failed\n' "$PASS_COUNT" "$FAIL_COUNT"
[[ "$FAIL_COUNT" -eq 0 ]]
