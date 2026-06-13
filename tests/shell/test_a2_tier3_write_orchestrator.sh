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

# shellcheck disable=SC2016
# The static_assert calls below pass single-quoted ERE patterns that are matched
# LITERALLY against the orchestrator's source (e.g. '\$EXIT_TTY', '"\$wt"'). The
# `$` must stay un-expanded — it is part of the regex that matches the script
# text — so SC2016 ("expressions don't expand in single quotes") is intentional
# here, file-wide.

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
# preview rc=7 (write-preview-ready) handling must be artifact-gated; approve/apply stay strict.
static_assert "preview rc=7 accepted only via artifacts"  'preview_ready_artifacts_present'              present
static_assert "preview rc=7 uses EXIT_PREVIEW_READY"      'rc -eq \$EXIT_PREVIEW_READY'                   present
# approval stdin/result diagnostics: non-approval steps isolate stdin; approve keeps the real TTY.
static_assert "preview step isolates stdin (/dev/null)"   'plan run .*--workspace-write-preview < /dev/null' present
static_assert "apply-bundle step isolates stdin"          'plan apply-bundle .*< /dev/null'              present
static_assert "apply step isolates stdin"                 'plan apply "\$apply_bundle" < /dev/null'       present
static_assert "approve step does NOT redirect stdin"      'plan approve .*< /dev/null'                    absent
static_assert "approve classifies rc (classify_approve_rc)" 'classify_approve_rc'                         present
static_assert "approve names EOF / non-TTY cause"         'EOF / drift / non-TTY'                         present
static_assert "approve handles output-io (rc 12)"         'approval-result IO error'                      present
static_assert "approve pre-checks output path exists"     'approval-result path already exists'          present
static_assert "failure shows .claw artifact presence"     'diagnose_claw_dir'                            present

# ---- preview rc=7 artifact-detection (accept/reject decision) --------------
# claw signals a READY write preview (approval pending) with exit code 7. The
# orchestrator must accept that ONLY when the preview-ready artifacts/status are
# present, and reject a bare rc=7 with no artifacts. We unit-test the pure
# detector by loading the orchestrator's functions (without dispatching main).
# shellcheck disable=SC1090
eval "$(sed '/^main "\$@"$/d' "$ORCH")"

# preview_ready_case <name> <want 0|1> <bundle 0|1> <gen 0|1> <status 0|1>
preview_ready_case() {
  local name=$1 want=$2 b=$3 g=$4 s=$5
  local d c rc=0
  d="$(mktemp -d -p "$WORK_DIR")"; c="$d/.claw"
  mkdir -p "$c/l2b-preview-bundles/r/s" "$c/l2b-runs/r"
  if [[ "$b" == 1 ]]; then : > "$c/l2b-preview-bundles/r/s/preview-bundle.json"; fi
  if [[ "$g" == 1 ]]; then : > "$c/l2b-preview-bundles/r/s/preview-generator-result.json"; fi
  if [[ "$s" == 1 ]]; then printf '{"status": "write_preview_ready"}\n' > "$c/l2b-runs/r/status.json"; fi
  preview_ready_artifacts_present "$c" || rc=1
  if [[ "$rc" == "$want" ]]; then
    PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS  %-52s (rc %s)\n' "$name" "$rc"
  else
    FAIL_COUNT=$((FAIL_COUNT + 1)); printf 'FAIL  %-52s (got %s, want %s)\n' "$name" "$rc" "$want"
  fi
}

preview_ready_case "accept: preview rc=7 with bundle+gen+status" 0 1 1 1
preview_ready_case "reject: preview rc=7 with no artifacts"      1 0 0 0
preview_ready_case "reject: preview rc=7 bundle+gen, no status"  1 1 1 0
preview_ready_case "reject: preview rc=7 status, no bundle"      1 0 0 1
# (the "unchanged off-TTY approval refusal" case is the apply-lane exit-7 test above.)

# ---- approve exit-code classification (precise diagnostics) ----------------
# claw plan approve exits 0=approved, 5=bundle-error, 7=denied/EOF/non-TTY,
# 12=approval-result-output IO. The orchestrator must categorize each distinctly.
approve_rc_case() {
  local name=$1 code=$2 want=$3 got
  got=$(classify_approve_rc "$code")
  if [[ "$got" == "$want" ]]; then
    PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS  %-52s (%s)\n' "$name" "$got"
  else
    FAIL_COUNT=$((FAIL_COUNT + 1)); printf 'FAIL  %-52s (got %s, want %s)\n' "$name" "$got" "$want"
  fi
}
approve_rc_case "approve rc=0  -> approved"           0  approved
approve_rc_case "approve rc=5  -> bundle-error"       5  bundle-error
approve_rc_case "approve rc=7  -> denied/eof/non-tty" 7  denied-eof-nontty
approve_rc_case "approve rc=12 -> output-io"          12 output-io
approve_rc_case "approve rc=3  -> unknown"            3  unknown

# ============================================================================
# package-plan (Tier-4 Stage 1, READ-ONLY) — design:
# docs/a2-tier3-tier4-pr-packaging-design-scope.md
# Pure gates refuse offline (no git); live gates use hermetic git fixtures wired
# via A2_CONTROL_CHECKOUT / A2_DISPOSABLE_WORKTREE_ROOT (test-only env overrides).
# package-plan must NEVER mutate git (no add/commit/push/PR); would_push=false.
# ============================================================================

# ---- pure-gate refusals (offline; refuse BEFORE any git IO) -----------------
# Use default-root paths so these are independent of the live fixtures/env.
PWT="/mnt/vast-data/git-worktrees/__a2_tier4_pp__"
PDECL="[\"$PWT/notes.md\"]"

run_case "package-plan: missing args -> usage(2)"     2 package-plan --worktree "$PWT"

write_lane "$D/pp_base.json"  true  main        feat/x "$PWT" "$PDECL" '[]' '[]'
run_case "package-plan: base not origin/main(4)"      4 package-plan --worktree "$PWT" --approved-lane "$D/pp_base.json"

write_lane "$D/pp_appr.json"  false origin/main feat/x "$PWT" "$PDECL" '[]' '[]'
run_case "package-plan: operatorApproved=false(4)"    4 package-plan --worktree "$PWT" --approved-lane "$D/pp_appr.json"

write_lane "$D/pp_nodecl.json" true origin/main feat/x "$PWT" '[]' '[]' '[]'
run_case "package-plan: empty declared set(4)"        4 package-plan --worktree "$PWT" --approved-lane "$D/pp_nodecl.json"

write_lane "$D/pp_brmain.json" true origin/main main   "$PWT" "$PDECL" '[]' '[]'
run_case "package-plan: branch is main(4)"            4 package-plan --worktree "$PWT" --approved-lane "$D/pp_brmain.json"

write_lane "$D/pp_good.json"  true origin/main feat/x "$PWT" "$PDECL" '[]' '[]'
run_case "package-plan: --worktree mismatches lane(4)" 4 package-plan --worktree "/mnt/vast-data/git-worktrees/__other__" --approved-lane "$D/pp_good.json"

# ---- live read-only gates (hermetic git fixtures) --------------------------
PKG_ROOT="$(mktemp -d -p "$WORK_DIR")"
PKG_CTL="$PKG_ROOT/control"; PKG_WTR="$PKG_ROOT/wtroot"
mkdir -p "$PKG_CTL"
git -C "$PKG_CTL" init -q
git -C "$PKG_CTL" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init

# build_applied_wt <name> <branch> — applied disposable worktree fixture:
# history + uncommitted declared file (notes.md) + matching .claw payload
# after.sha256 + apply-bundle + checkpoint dir. Echoes the worktree path.
build_applied_wt() {
  local name=$1 branch=$2
  local wt="$PKG_WTR/$name" sha
  mkdir -p "$wt"
  git -C "$wt" init -q
  git -C "$wt" config user.email t@t   # persistent identity so an in-worktree commit (Stage 2) works
  git -C "$wt" config user.name t
  git -C "$wt" checkout -q -b "$branch"
  git -C "$wt" commit -q --allow-empty -m base
  printf 'hello tier4\n' > "$wt/notes.md"
  sha=$(sha256sum "$wt/notes.md" | awk '{print $1}')
  mkdir -p "$wt/.claw/l2b-payloads/RUN/STEP" "$wt/.claw/l2b-preview-bundles/RUN/STEP" "$wt/.claw/l2b-checkpoints/RUN/STEP"
  printf '%s\n' "$sha" > "$wt/.claw/l2b-payloads/RUN/STEP/after.sha256"
  printf '{}' > "$wt/.claw/l2b-preview-bundles/RUN/STEP/apply-bundle.json"
  printf '%s' "$wt"
}

# pkg_lane <file> <wt> <branch> — minimal approved lane for the fixture.
pkg_lane() {
  python3 - "$1" "$2" "$3" <<'PY'
import json,sys
f,wt,branch=sys.argv[1:4]
json.dump({"objective":"t","worktreePlan":{"worktreePath":wt,"branch":branch,"base":"origin/main"},
           "declaredPaths":[wt+"/notes.md"],"operatorApproved":True}, open(f,"w"))
PY
}

# run_pkg <name> <expect> <args...> — package-plan with the hermetic env.
run_pkg() {
  local name=$1 expect=$2; shift 2
  local rc=0
  A2_CONTROL_CHECKOUT="$PKG_CTL" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
    "${ORCH}" "$@" >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$expect" ]]; then
    PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (exit %s)\n' "$name" "$rc"
  else
    FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (got %s, want %s)\n' "$name" "$rc" "$expect"
  fi
}

WT_OK="$(build_applied_wt ok feat/x)"
pkg_lane "$PKG_ROOT/lane_ok.json" "$WT_OK" feat/x
run_pkg "package-plan: happy path package-ready(0)"   0 package-plan --worktree "$WT_OK" --approved-lane "$PKG_ROOT/lane_ok.json"

# proof of NO git mutation after the happy-path plan: nothing staged/committed,
# HEAD unchanged, no new commit, notes.md still untracked.
HEAD_BEFORE="$(git -C "$WT_OK" rev-parse HEAD)"
A2_CONTROL_CHECKOUT="$PKG_CTL" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-plan --worktree "$WT_OK" --approved-lane "$PKG_ROOT/lane_ok.json" >/dev/null 2>&1 || true
HEAD_AFTER="$(git -C "$WT_OK" rev-parse HEAD)"
STAGED="$(git -C "$WT_OK" diff --cached --name-only)"
UNTRACKED_NOTES="$(git -C "$WT_OK" status --porcelain -- notes.md)"
if [[ "$HEAD_BEFORE" == "$HEAD_AFTER" && -z "$STAGED" && "$UNTRACKED_NOTES" == '?? notes.md' ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (no git mutation)\n' "package-plan: performed zero git mutation"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (HEAD/%s staged/%s notes/%s)\n' "package-plan: performed zero git mutation" "$HEAD_AFTER" "$STAGED" "$UNTRACKED_NOTES"
fi

# drift: an out-of-declared-set untracked file -> refuse(4).
WT_DRIFT="$(build_applied_wt drift feat/x)"; printf 'x' > "$WT_DRIFT/EXTRA.txt"
pkg_lane "$PKG_ROOT/lane_drift.json" "$WT_DRIFT" feat/x
run_pkg "package-plan: drift outside declared set(4)" 4 package-plan --worktree "$WT_DRIFT" --approved-lane "$PKG_ROOT/lane_drift.json"

# hash mismatch: on-disk bytes differ from recorded after.sha256 -> refuse(4).
WT_HASH="$(build_applied_wt hash feat/x)"; printf 'tampered\n' > "$WT_HASH/notes.md"
pkg_lane "$PKG_ROOT/lane_hash.json" "$WT_HASH" feat/x
run_pkg "package-plan: on-disk hash mismatch(4)"      4 package-plan --worktree "$WT_HASH" --approved-lane "$PKG_ROOT/lane_hash.json"

# missing declared file on disk (not applied) -> refuse(4).
WT_MISS="$(build_applied_wt miss feat/x)"; rm -f "$WT_MISS/notes.md"
pkg_lane "$PKG_ROOT/lane_miss.json" "$WT_MISS" feat/x
run_pkg "package-plan: declared file missing(4)"      4 package-plan --worktree "$WT_MISS" --approved-lane "$PKG_ROOT/lane_miss.json"

# wrong branch: worktree on a different branch than the lane -> refuse(4).
WT_BR="$(build_applied_wt br feat/y)"
pkg_lane "$PKG_ROOT/lane_br.json" "$WT_BR" feat/x   # lane says feat/x, worktree on feat/y
run_pkg "package-plan: worktree branch mismatch(4)"   4 package-plan --worktree "$WT_BR" --approved-lane "$PKG_ROOT/lane_br.json"

# missing apply evidence: remove apply-bundle.json -> refuse(4).
WT_NOEV="$(build_applied_wt noev feat/x)"; find "$WT_NOEV/.claw" -name 'apply-bundle.json' -delete
pkg_lane "$PKG_ROOT/lane_noev.json" "$WT_NOEV" feat/x
run_pkg "package-plan: missing apply-bundle evidence(4)" 4 package-plan --worktree "$WT_NOEV" --approved-lane "$PKG_ROOT/lane_noev.json"

# dirty control checkout -> refuse(4) (uses its own dirty control dir).
PKG_CTL_DIRTY="$PKG_ROOT/control_dirty"
mkdir -p "$PKG_CTL_DIRTY"; git -C "$PKG_CTL_DIRTY" init -q
git -C "$PKG_CTL_DIRTY" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
printf 'dirty' > "$PKG_CTL_DIRTY/dirty.txt"
WT_DC="$(build_applied_wt dc feat/x)"
pkg_lane "$PKG_ROOT/lane_dc.json" "$WT_DC" feat/x
rc_dc=0
A2_CONTROL_CHECKOUT="$PKG_CTL_DIRTY" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-plan --worktree "$WT_DC" --approved-lane "$PKG_ROOT/lane_dc.json" >/dev/null 2>&1 || rc_dc=$?
if [[ "$rc_dc" -eq 4 ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (exit %s)\n' "package-plan: dirty control checkout(4)" "$rc_dc"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (got %s, want 4)\n' "package-plan: dirty control checkout(4)" "$rc_dc"
fi

# ---- package-plan static invariants (read-only / no-mutation by construction) ---
static_assert "package-plan subcommand present"          'cmd_package_plan\(\)'                         present
static_assert "package-plan read-only would_push=false"  '"would_push": False'                          present
static_assert "package-plan read-only would_open_pr=false" '"would_open_pr": False'                     present
static_assert "package-plan prints, never runs, push/PR" 'package-plan runs NONE of them'               present
# Stage 2 legitimately stages+commits INSIDE the disposable worktree, so the file
# now CONTAINS exact-path `git add --` / `git commit`. The invariants worth pinning
# are: only exact-path staging (never `git add .`/`-A`/`commit -a`), and NEVER an
# executed push/PR/merge anywhere in the orchestrator.
static_assert "orchestrator never uses git add . / -A"   'git[[:space:]]+add[[:space:]]+(\.|-A)([[:space:]]|$)' absent
static_assert "orchestrator never uses git commit -a"    'git[[:space:]].*commit[[:space:]].*[[:space:]]-a([[:space:]]|$)' absent
# Stage 3 legitimately performs ONE exact, non-force branch:branch push inside the
# disposable worktree. The invariants worth pinning: the only push is that exact
# non-force refspec, and NEVER a force/mirror/tags push or any ref-delete refspec.
static_assert "orchestrator's only push is exact non-force refspec" 'git -C "\$wt" push --set-upstream "\$remote_name" "\$branch:\$branch"' present
static_assert "orchestrator never force/mirror/tags-pushes" 'push[[:space:]]+(--force|-f|--mirror|--tags)([[:space:]]|$)' absent
static_assert "orchestrator never deletes remote refs"     'push[^"]*(--delete|--prune| :)' absent
static_assert "orchestrator executes NO gh"              '^[[:space:]]*gh[[:space:]]+pr'                 absent

# ============================================================================
# package-commit (Tier-4 Stage 2) — stages EXACTLY the declared set + ONE commit
# INSIDE the disposable worktree; never push/PR/merge; never touches the control
# checkout. Reuses the Stage-1 gate, so pure-gate refusals are covered above.
# ============================================================================

# happy path: commits exactly the declared set inside the disposable worktree.
WT_C_OK="$(build_applied_wt cok feat/x)"
pkg_lane "$PKG_ROOT/lane_cok.json" "$WT_C_OK" feat/x
C_HEAD_BEFORE="$(git -C "$WT_C_OK" rev-parse HEAD)"
C_NCOMMITS_BEFORE="$(git -C "$WT_C_OK" rev-list --count HEAD)"
run_pkg "package-commit: happy path commits(0)"       0 package-commit --worktree "$WT_C_OK" --approved-lane "$PKG_ROOT/lane_cok.json"

# verify EXACTLY one new commit, containing exactly the declared set, working tree
# now clean except the ignored .claw tree, and the control checkout untouched.
C_HEAD_AFTER="$(git -C "$WT_C_OK" rev-parse HEAD)"
C_NCOMMITS_AFTER="$(git -C "$WT_C_OK" rev-list --count HEAD)"
C_COMMIT_FILES="$(git -C "$WT_C_OK" show --name-only --pretty=format: HEAD | sed '/^$/d' | sort | tr '\n' ' ')"
C_WT_LEFT="$(git -C "$WT_C_OK" status --porcelain | sort | tr '\n' '|')"
C_CTL_DIRTY="$(git -C "$PKG_CTL" status --porcelain)"
if [[ "$C_HEAD_AFTER" != "$C_HEAD_BEFORE" \
      && "$C_NCOMMITS_AFTER" -eq "$((C_NCOMMITS_BEFORE + 1))" \
      && "$C_COMMIT_FILES" == "notes.md " \
      && "$C_WT_LEFT" == '?? .claw/|' \
      && -z "$C_CTL_DIRTY" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (1 commit, declared only, ctl clean)\n' "package-commit: exactly one in-worktree commit"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (head=%s n=%s files=[%s] left=[%s] ctl=[%s])\n' "package-commit: exactly one in-worktree commit" "$C_HEAD_AFTER" "$C_NCOMMITS_AFTER" "$C_COMMIT_FILES" "$C_WT_LEFT" "$C_CTL_DIRTY"
fi

# emitted evidence carries the read-only-for-stages-3-4 flags.
WT_C_EV="$(build_applied_wt cev feat/x)"
pkg_lane "$PKG_ROOT/lane_cev.json" "$WT_C_EV" feat/x
C_JSON="$(A2_CONTROL_CHECKOUT="$PKG_CTL" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-commit --worktree "$WT_C_EV" --approved-lane "$PKG_ROOT/lane_cev.json" 2>/dev/null || true)"
if printf '%s' "$C_JSON" | grep -q '"pushed": false' \
   && printf '%s' "$C_JSON" | grep -q '"pr_opened": false' \
   && printf '%s' "$C_JSON" | grep -q '"merged": false' \
   && printf '%s' "$C_JSON" | grep -q '"commit_sha"'; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (pushed/pr_opened/merged=false + commit_sha)\n' "package-commit: evidence contract"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (evidence JSON missing flags)\n' "package-commit: evidence contract"
fi

# refusal: pre-staged index (apply chain never stages) -> refuse(4), no commit.
WT_C_IDX="$(build_applied_wt cidx feat/x)"
git -C "$WT_C_IDX" add -- notes.md   # foreign pre-staging
pkg_lane "$PKG_ROOT/lane_cidx.json" "$WT_C_IDX" feat/x
IDX_NB="$(git -C "$WT_C_IDX" rev-list --count HEAD)"
run_pkg "package-commit: pre-staged index refused(4)" 4 package-commit --worktree "$WT_C_IDX" --approved-lane "$PKG_ROOT/lane_cidx.json"
if [[ "$(git -C "$WT_C_IDX" rev-list --count HEAD)" -eq "$IDX_NB" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (no commit on refusal)\n' "package-commit: pre-staged refusal makes no commit"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (commit happened!)\n' "package-commit: pre-staged refusal makes no commit"
fi

# refusal: drift -> refuse(4); refusal: hash mismatch -> refuse(4).
WT_C_DRIFT="$(build_applied_wt cdrift feat/x)"; printf 'x' > "$WT_C_DRIFT/EXTRA.txt"
pkg_lane "$PKG_ROOT/lane_cdrift.json" "$WT_C_DRIFT" feat/x
run_pkg "package-commit: drift refused(4)"            4 package-commit --worktree "$WT_C_DRIFT" --approved-lane "$PKG_ROOT/lane_cdrift.json"

WT_C_HASH="$(build_applied_wt chash feat/x)"; printf 'tampered\n' > "$WT_C_HASH/notes.md"
pkg_lane "$PKG_ROOT/lane_chash.json" "$WT_C_HASH" feat/x
run_pkg "package-commit: hash mismatch refused(4)"    4 package-commit --worktree "$WT_C_HASH" --approved-lane "$PKG_ROOT/lane_chash.json"

# refusal: missing declared file -> refuse(4); refusal: branch main -> refuse(4).
WT_C_MISS="$(build_applied_wt cmiss feat/x)"; rm -f "$WT_C_MISS/notes.md"
pkg_lane "$PKG_ROOT/lane_cmiss.json" "$WT_C_MISS" feat/x
run_pkg "package-commit: declared file missing(4)"    4 package-commit --worktree "$WT_C_MISS" --approved-lane "$PKG_ROOT/lane_cmiss.json"

WT_C_BR="$(build_applied_wt cbr main)"
pkg_lane "$PKG_ROOT/lane_cbr.json" "$WT_C_BR" main
run_pkg "package-commit: branch main refused(4)"      4 package-commit --worktree "$WT_C_BR" --approved-lane "$PKG_ROOT/lane_cbr.json"

# refusal: dirty control checkout -> refuse(4), commits nothing.
WT_C_DC="$(build_applied_wt cdc feat/x)"
pkg_lane "$PKG_ROOT/lane_cdc.json" "$WT_C_DC" feat/x
C_DC_NB="$(git -C "$WT_C_DC" rev-list --count HEAD)"
rc_cdc=0
A2_CONTROL_CHECKOUT="$PKG_CTL_DIRTY" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-commit --worktree "$WT_C_DC" --approved-lane "$PKG_ROOT/lane_cdc.json" >/dev/null 2>&1 || rc_cdc=$?
if [[ "$rc_cdc" -eq 4 && "$(git -C "$WT_C_DC" rev-list --count HEAD)" -eq "$C_DC_NB" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (refused, no commit)\n' "package-commit: dirty control checkout(4)"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (rc=%s)\n' "package-commit: dirty control checkout(4)" "$rc_cdc"
fi

# usage refusals.
run_case "package-commit: missing args -> usage(2)"   2 package-commit --worktree "$PWT"

# ---- package-commit static invariants --------------------------------------
static_assert "package-commit subcommand present"        'cmd_package_commit\(\)'                       present
static_assert "package-commit stages exact-path only"    'git -C "\$wt" add -- "\$rel"'                  present
static_assert "package-commit commits in-worktree"       'git -C "\$wt" commit -q -m'                    present
static_assert "package-commit evidence pushed=false"     '"pushed": False'                              present
static_assert "package-commit evidence merged=false"     '"merged": False'                              present
static_assert "package-commit verifies staged==declared" 'staged set != declared set'                   present

# ============================================================================
# package-push (Tier-4 Stage 3) — pushes ONLY the exact disposable branch at the
# exact package-commit sha to its `origin` remote (a HERMETIC temp bare repo in
# these tests — never GitHub); non-force; no PR/merge/tags/delete; never touches
# the control checkout. Reuses the Stage-1 gate, so its pure-gate refusals carry.
# ============================================================================

# build_pushable_wt <name> <branch> <bare> — an applied+committed worktree (Stage-2
# state) whose `origin` remote is the given hermetic bare repo. Echoes the path.
build_pushable_wt() {
  local name=$1 branch=$2 bare=$3
  local wt="$PKG_WTR/$name" sha
  mkdir -p "$wt"
  git -C "$wt" init -q
  git -C "$wt" config user.email t@t
  git -C "$wt" config user.name t
  git -C "$wt" remote add origin "$bare"
  git -C "$wt" checkout -q -b "$branch"
  git -C "$wt" commit -q --allow-empty -m base
  printf 'hello tier4\n' > "$wt/notes.md"
  sha=$(sha256sum "$wt/notes.md" | awk '{print $1}')
  mkdir -p "$wt/.claw/l2b-payloads/RUN/STEP" "$wt/.claw/l2b-preview-bundles/RUN/STEP" "$wt/.claw/l2b-checkpoints/RUN/STEP"
  printf '%s\n' "$sha" > "$wt/.claw/l2b-payloads/RUN/STEP/after.sha256"
  printf '{}' > "$wt/.claw/l2b-preview-bundles/RUN/STEP/apply-bundle.json"
  git -C "$wt" add -- notes.md            # simulate the Stage-2 package-commit
  git -C "$wt" commit -q -m "a2(tier4): package isolated mutation on $branch (1 file(s))"
  printf '%s' "$wt"
}

# new_bare <name> — create a hermetic empty bare repo; echo its path.
new_bare() { local b="$PKG_ROOT/$1.git"; git init --bare -q "$b"; printf '%s' "$b"; }

# happy path: pushes the exact branch at the package-commit sha to the bare origin.
BARE_OK="$(new_bare origin_pok)"
WT_P_OK="$(build_pushable_wt pok feat/x "$BARE_OK")"
pkg_lane "$PKG_ROOT/lane_pok.json" "$WT_P_OK" feat/x
P_LOCAL="$(git -C "$WT_P_OK" rev-parse HEAD)"
run_pkg "package-push: happy path pushes(0)"          0 package-push --worktree "$WT_P_OK" --approved-lane "$PKG_ROOT/lane_pok.json"
P_REMOTE="$(git -C "$WT_P_OK" ls-remote --heads "$BARE_OK" feat/x | awk 'NR==1{print $1}')"
P_OTHER_REFS="$(git -C "$WT_P_OK" ls-remote --heads "$BARE_OK" | awk '{print $2}' | { grep -v '^refs/heads/feat/x$' || true; } | tr '\n' ',')"
P_CTL_DIRTY="$(git -C "$PKG_CTL" status --porcelain)"
if [[ "$P_REMOTE" == "$P_LOCAL" && -z "$P_OTHER_REFS" && -z "$P_CTL_DIRTY" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (remote==HEAD, only feat/x, ctl clean)\n' "package-push: pushed exact sha, only the branch"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (remote=%s local=%s other=[%s] ctl=[%s])\n' "package-push: pushed exact sha, only the branch" "$P_REMOTE" "$P_LOCAL" "$P_OTHER_REFS" "$P_CTL_DIRTY"
fi

# evidence: pushed=true, pr_opened=false, merged=false, remote_sha present.
BARE_EV="$(new_bare origin_pev)"
WT_P_EV="$(build_pushable_wt pev feat/x "$BARE_EV")"
pkg_lane "$PKG_ROOT/lane_pev.json" "$WT_P_EV" feat/x
P_JSON="$(A2_CONTROL_CHECKOUT="$PKG_CTL" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-push --worktree "$WT_P_EV" --approved-lane "$PKG_ROOT/lane_pev.json" 2>/dev/null || true)"
if printf '%s' "$P_JSON" | grep -q '"pushed": true' \
   && printf '%s' "$P_JSON" | grep -q '"pr_opened": false' \
   && printf '%s' "$P_JSON" | grep -q '"merged": false' \
   && printf '%s' "$P_JSON" | grep -q '"remote_sha"'; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (pushed=true pr_opened/merged=false)\n' "package-push: evidence contract"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (evidence JSON missing flags)\n' "package-push: evidence contract"
fi

# idempotent: remote already at the exact sha -> no-op success(0).
BARE_IDEM="$(new_bare origin_pidem)"
WT_P_IDEM="$(build_pushable_wt pidem feat/x "$BARE_IDEM")"
git -C "$WT_P_IDEM" push -q origin feat/x:feat/x       # pre-push the exact HEAD
pkg_lane "$PKG_ROOT/lane_pidem.json" "$WT_P_IDEM" feat/x
run_pkg "package-push: idempotent same-sha no-op(0)"  0 package-push --worktree "$WT_P_IDEM" --approved-lane "$PKG_ROOT/lane_pidem.json"

# refuse: remote branch exists at a DIFFERENT sha -> refuse(4), no force.
BARE_COLL="$(new_bare origin_pcoll)"
WT_P_COLL="$(build_pushable_wt pcoll feat/x "$BARE_COLL")"
git -C "$WT_P_COLL" push -q origin "HEAD~1:refs/heads/feat/x"   # remote feat/x = parent (different sha)
P_COLL_REMOTE_BEFORE="$(git -C "$WT_P_COLL" ls-remote --heads "$BARE_COLL" feat/x | awk 'NR==1{print $1}')"
pkg_lane "$PKG_ROOT/lane_pcoll.json" "$WT_P_COLL" feat/x
run_pkg "package-push: remote collision diff-sha refused(4)" 4 package-push --worktree "$WT_P_COLL" --approved-lane "$PKG_ROOT/lane_pcoll.json"
P_COLL_REMOTE_AFTER="$(git -C "$WT_P_COLL" ls-remote --heads "$BARE_COLL" feat/x | awk 'NR==1{print $1}')"
if [[ "$P_COLL_REMOTE_AFTER" == "$P_COLL_REMOTE_BEFORE" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (remote unchanged, no force)\n' "package-push: collision left remote unchanged"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (remote changed!)\n' "package-push: collision left remote unchanged"
fi

# refuse: dirty disposable worktree (uncommitted tracked change) -> refuse(4), no push.
BARE_DWT="$(new_bare origin_pdwt)"
WT_P_DWT="$(build_pushable_wt pdwt feat/x "$BARE_DWT")"
printf 'edited\n' >> "$WT_P_DWT/notes.md"   # uncommitted tracked change
pkg_lane "$PKG_ROOT/lane_pdwt.json" "$WT_P_DWT" feat/x
run_pkg "package-push: dirty disposable worktree refused(4)" 4 package-push --worktree "$WT_P_DWT" --approved-lane "$PKG_ROOT/lane_pdwt.json"
if [[ -z "$(git -C "$WT_P_DWT" ls-remote --heads "$BARE_DWT" feat/x)" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (nothing pushed)\n' "package-push: dirty worktree pushed nothing"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (pushed anyway!)\n' "package-push: dirty worktree pushed nothing"
fi

# refuse: HEAD diff != declared set (HEAD is not a clean package-commit) -> refuse(4).
# Build a worktree whose HEAD commit changes an EXTRA file beyond the declared set.
BARE_NDC="$(new_bare origin_pndc)"
WT_P_NDC="$(build_pushable_wt pndc feat/x "$BARE_NDC")"
printf 'y\n' > "$WT_P_NDC/OTHER.md"; git -C "$WT_P_NDC" add -- OTHER.md
git -C "$WT_P_NDC" commit -q --amend --no-edit    # HEAD now changes notes.md + OTHER.md
pkg_lane "$PKG_ROOT/lane_pndc.json" "$WT_P_NDC" feat/x
run_pkg "package-push: HEAD diff != declared refused(4)" 4 package-push --worktree "$WT_P_NDC" --approved-lane "$PKG_ROOT/lane_pndc.json"

# refuse: dirty control checkout -> refuse(4), no push.
BARE_PDC="$(new_bare origin_ppdc)"
WT_P_PDC="$(build_pushable_wt ppdc feat/x "$BARE_PDC")"
pkg_lane "$PKG_ROOT/lane_ppdc.json" "$WT_P_PDC" feat/x
rc_ppdc=0
A2_CONTROL_CHECKOUT="$PKG_CTL_DIRTY" A2_DISPOSABLE_WORKTREE_ROOT="$PKG_WTR" \
  "${ORCH}" package-push --worktree "$WT_P_PDC" --approved-lane "$PKG_ROOT/lane_ppdc.json" >/dev/null 2>&1 || rc_ppdc=$?
if [[ "$rc_ppdc" -eq 4 && -z "$(git -C "$WT_P_PDC" ls-remote --heads "$BARE_PDC" feat/x)" ]]; then
  PASS_COUNT=$((PASS_COUNT+1)); printf 'PASS  %-52s (refused, nothing pushed)\n' "package-push: dirty control checkout(4)"
else
  FAIL_COUNT=$((FAIL_COUNT+1)); printf 'FAIL  %-52s (rc=%s)\n' "package-push: dirty control checkout(4)" "$rc_ppdc"
fi

# refuse: branch main -> refuse(4) (also never pushes main).
BARE_BM="$(new_bare origin_pbm)"
WT_P_BM="$(build_pushable_wt pbm main "$BARE_BM")"
pkg_lane "$PKG_ROOT/lane_pbm.json" "$WT_P_BM" main
run_pkg "package-push: branch main refused(4)"        4 package-push --worktree "$WT_P_BM" --approved-lane "$PKG_ROOT/lane_pbm.json"

# usage refusal.
run_case "package-push: missing args -> usage(2)"     2 package-push --worktree "$PWT"

# ---- package-push static invariants ----------------------------------------
static_assert "package-push subcommand present"          'cmd_package_push\(\)'                          present
static_assert "package-push verifies HEAD diff==declared" 'HEAD commit diff != declared set'             present
static_assert "package-push refuses diff-sha remote collision" 'already exists at a DIFFERENT sha'        present
static_assert "package-push evidence pushed=true"        '"pushed": True'                               present
static_assert "package-push evidence pr_opened=false"    '"pr_opened": False'                           present
static_assert "package-push opens no PR (Stage 4 separate)" 'opening a draft PR requires a NEW explicit operator approval' present

# ---- summary ---------------------------------------------------------------
printf -- '----\n%d passed, %d failed\n' "$PASS_COUNT" "$FAIL_COUNT"
[[ "$FAIL_COUNT" -eq 0 ]]
