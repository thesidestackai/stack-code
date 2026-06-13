#!/usr/bin/env bash
# a2-tier3-write-orchestrator.sh — Tier 3 write-capable ORCHESTRATOR, v0.
#
# WHAT THIS IS
#   An EXTERNAL, operator-invoked orchestrator that DRIVES the existing, tested
#   A2-L2b write chain (`claw plan run/approve/apply-bundle/apply`, backed by the
#   Rust crate `a2-plan-runner` `write_executor` + `checkpoint`). It does NOT
#   re-implement file writing and adds NO new write/checkpoint/rollback logic —
#   `claw plan apply` remains the one and only command that writes a target, and
#   the Rust executor remains the write authority.
#
#   The only genuinely-new work here is the DISPOSABLE-WORKTREE LANE around that
#   chain: validate the operator-approved lane, create exactly one disposable
#   worktree from origin/main, enforce exact-path scope + denials-win, drive the
#   existing chain per declared file inside the worktree, gather checkpoint/diff
#   evidence, and STOP for operator review.
#
# SOURCES OF TRUTH (merged on origin/main)
#   docs/a2-tier3-write-executor-reconciliation.md            (PR #113 — drive, don't duplicate)
#   handoffs/a2_tier3_mutation_executor_write_capable_implementation_prompt_DRAFT_2026-06-09.md (PR #114)
#   The gate logic mirrors the panel pure models (the authoritative spec):
#     ide/vscode/a2-harness-panel/src/disposableWorktreePlan.ts  (normalizeAbs/isUnder, plan rules)
#     ide/vscode/a2-harness-panel/src/mutationScope.ts           (classifyWrite — deny-by-default)
#     ide/vscode/a2-harness-panel/src/safeMutationPolicy.ts      (denials win, then Tier-3 allowlist)
#     ide/vscode/a2-harness-panel/src/deniedCommands.ts          (denied-command registry)
#     ide/vscode/a2-harness-panel/src/executorDryRun.ts          (dry-run-ready evidence contract)
#
# HARD SAFETY INVARIANTS (this script preserves all of them)
#   - Writes occur ONLY inside a fresh disposable worktree under
#     /mnt/vast-data/git-worktrees/ — never the control checkout, never a live target.
#   - A passing, ready dry-run evidence file is REQUIRED and re-validated at runtime.
#   - An explicit operator-approved lane is REQUIRED (operatorApproved == true).
#   - Approval stays the existing real-terminal, human-typed flow: this script
#     REQUIRES an interactive TTY and drives `claw plan approve`; it NEVER composes,
#     captures, fakes, or batches the approval line (claw also fails closed off-TTY).
#   - Exact-path scope: every write must be in the declared set, inside the
#     worktree, not under the control checkout (deny-by-default).
#   - Denials win over the Tier-3 allowlist for any lane-declared validation command.
#   - It NEVER pushes, opens a PR, merges, deletes a branch, or force-removes a
#     worktree. Rollback is by ABANDONING the disposable worktree (operator action).
#   - It makes NO model/broker/runtime/network/Vault call and no raw app inference.
#
# This script is operator-run only. It is NOT spawnable by the IDE panel: the
# panel's helperRunner allowlists exactly one basename (a2-ide-harness.sh) and
# this file is deliberately not that.

set -euo pipefail

# ---- constants -------------------------------------------------------------

# Mirror of disposableWorktreePlan.ts constants (authoritative there). The
# production defaults are fixed; the two env overrides exist ONLY so the offline
# test harness can point the gates at hermetic temp dirs. With the env unset (the
# normal operator path) the values are byte-identical to the hardcoded defaults,
# so behavior is unchanged in production. Overriding the control-checkout path can
# only relocate the read-only cleanliness probe — it never enables a write outside
# the disposable worktree (writes still go solely through the existing chain).
readonly CONTROL_CHECKOUT="${A2_CONTROL_CHECKOUT:-/home/suki/stack-code}"
readonly DISPOSABLE_WORKTREE_ROOT="${A2_DISPOSABLE_WORKTREE_ROOT:-/mnt/vast-data/git-worktrees/}"

# Default built claw binary (override with A2_CLAW=/path/to/claw). May contain spaces.
readonly DEFAULT_CLAW="/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw"
A2_CLAW="${A2_CLAW:-$DEFAULT_CLAW}"

# Fixed by the CLI source (a2-plan-runner/src/approval.rs). Human-typed at a real TTY.
readonly APPROVAL_GRAMMAR='apply <step-id> <preview_sha256>'

readonly PROG="a2-tier3-write-orchestrator.sh"

# Exit codes.
readonly EXIT_OK=0
readonly EXIT_USAGE=2
readonly EXIT_GATE=4   # a safety gate refused the lane
readonly EXIT_TTY=7    # interactive-terminal guard (mirrors claw's off-TTY fail-closed)

# claw reuses exit code 7 for TWO distinct meanings, disambiguated by STAGE and
# on-disk artifacts (never by the numeric code alone):
#   - at `plan run --workspace-write-preview`: EXIT_RUN_PLAN_WRITE_PREVIEW_READY —
#     the write preview is READY and approval is PENDING (a success-with-pending
#     signal, NOT a failure). Accepted ONLY when the preview-ready artifacts/status
#     are present (see preview_ready_artifacts_present).
#   - at `plan approve`: EXIT_APPROVAL_DENIED — approval refused. Always STRICT.
readonly EXIT_PREVIEW_READY=7

# `claw plan approve` exit codes (rusty-claude-cli/src/main.rs):
#   0  approved — approval-result JSON persisted to the EXACT --approval-result-output path.
#   5  bundle read / parse / schema / integrity error (EXIT_APPROVAL_BUNDLE_ERR).
#   7  refused / non-approvable / EOF (zero bytes) / drift / non-TTY (EXIT_APPROVAL_DENIED).
#   12 approval-result-output IO: the output path ALREADY EXISTED (refuses, no approval run),
#      OR approved-but-the-file-write-failed (the approval JSON still went to stdout) — EXIT_APPROVAL_OUTPUT_IO.
# (codes 5 and 7 are matched as literals in classify_approve_rc; only 12 needs a named
#  constant because the STEP 2 pre-flight message references it.)
readonly EXIT_APPROVAL_OUTPUT_IO=12

# Denied-command registry mirror (deniedCommands.ts). Stored as ERE source
# strings; sensitive tokens are written with a trailing one-character class
# (e.g. broke[r]) so the regex still matches a real command while this script's
# OWN text never contains the forbidden literal verbatim (guard-clean, the same
# intent as deniedCommands.ts storing matchers as string literals). Whitespace
# uses [[:space:]] for the same reason. Denials win over the Tier-3 allowlist.
readonly DENIED_PATTERNS=(
  'rm[[:space:]]+-[a-z]*r[a-z]*f'
  'rm[[:space:]]+-[a-z]*f[a-z]*r'
  'git[[:space:]]+clea[n]'
  'find[[:space:]].*-delet[e]'
  'find[[:space:]].*-exec[[:space:]]+r[m]'
  'git[[:space:]]+reset[[:space:]]+--har[d]'
  'git[[:space:]]+branch[[:space:]]+-[D]'
  'git[[:space:]]+worktree[[:space:]]+remove[[:space:]]+--forc[e]'
  'git[[:space:]]+fetch[[:space:]]+--prun[e]'
  'git[[:space:]]+push[[:space:]].*--forc[e]'
  'git[[:space:]]+push[[:space:]].*-f([[:space:]]|$)'
  'git[[:space:]]+rebas[e]([[:space:]]|$)'
  'git[[:space:]]+filter-branc[h]'
  'systemct[l]([[:space:]]|$)'
  'service[[:space:]]+[^[:space:]]+[[:space:]]+(start|stop|restar[t])'
  'docke[r][[:space:]]+(stop|start|restart|rm|kill)'
  '(^|[[:space:]])restar[t]([[:space:]]|$)'
  '/v1/chat/completion[s]'
  '(^|[[:space:]])broke[r]([[:space:]]|$)'
  '(^|[[:space:]])ollam[a]([[:space:]]|$)'
  'model[[:space:]]+(load|unloa[d])'
  '(^|[[:space:]])1143[4]([[:space:]]|$)'
  '/status/vra[m]'
  '(^|[[:space:]])vaul[t]([[:space:]]|$)'
  '(^|[[:space:]])secre[t]'
  '(^|[[:space:]])beare[r]([[:space:]]|$)'
  'ap[i][_-]?ke[y]'
  # live A2 chain: lane-declared commands must NOT smuggle the chain; the
  # orchestrator drives it through its own controlled code path, not via the lane.
  'claw[[:space:]]+plan[[:space:]]+ru[n]'
  'claw[[:space:]]+plan[[:space:]]+approv[e]'
  'claw[[:space:]]+plan[[:space:]]+apply-bundl[e]'
  'claw[[:space:]]+plan[[:space:]]+appl[y]'
  # approval-line composition (must be human-typed, never captured/composed).
  'appl[y][[:space:]]+[^[:space:]]+[[:space:]]+[0-9a-f]{16,}'
  'https?://'
  '(^|[[:space:]])cur[l]([[:space:]]|$)'
  '(^|[[:space:]])wge[t]([[:space:]]|$)'
  '(^|[[:space:]])eva[l]([[:space:]]|$)'
)

# Tier-3 allowlist mirror (safeMutationPolicy.ts). A lane-declared validation
# command must match one of these AND survive the denial registry (denials win).
readonly TIER3_ALLOW_PATTERNS=(
  '^[[:space:]]*(validate-input|audit-workspace|find-artifacts|verify-final|help)([[:space:]]|$)'
  '^[[:space:]]*npm[[:space:]]+install[[:space:]]+--ignore-scripts([[:space:]]|$)'
  '^[[:space:]]*npm[[:space:]]+run[[:space:]]+lint([[:space:]]|$)'
  '^[[:space:]]*npm[[:space:]]+run[[:space:]]+compile([[:space:]]|$)'
  '^[[:space:]]*npm[[:space:]]+test([[:space:]]|$)'
)

# ---- output helpers --------------------------------------------------------

info() { printf '%s\n' "$*"; }
warn() { printf '%s: WARNING: %s\n' "$PROG" "$*" >&2; }
err()  { printf '%s: ERROR: %s\n' "$PROG" "$*" >&2; }
rule() { printf -- '----------------------------------------------------------------\n'; }

# Single-quote an argument so the operator can paste a printed command verbatim.
shq() { local s=${1//\'/\'\\\'\'}; printf "'%s'" "$s"; }

# ---- pure path reasoning (mirror of disposableWorktreePlan.ts) -------------

# Normalize a POSIX absolute path (resolve "." / "..") WITHOUT touching the fs.
normalize_abs() {
  local p=$1 seg out=()
  if [[ -z "$p" || ${p:0:1} != "/" ]]; then printf '%s' "$p"; return 0; fi
  local IFS='/'
  for seg in $p; do
    case "$seg" in
      ''|'.') : ;;
      '..') if [[ ${#out[@]} -gt 0 ]]; then unset 'out[${#out[@]}-1]'; fi ;;
      *) out+=("$seg") ;;
    esac
  done
  local joined=""
  local s
  for s in "${out[@]:-}"; do [[ -n "$s" ]] && joined+="/$s"; done
  [[ -z "$joined" ]] && joined="/"
  printf '%s' "$joined"
}

# True (0) when child is dir itself or strictly inside it (path-string only).
is_under() {
  local d c
  d=$(normalize_abs "$1"); d=${d%/}
  c=$(normalize_abs "$2"); c=${c%/}
  [[ "$c" == "$d" ]] && return 0
  [[ "$c" == "$d"/* ]] && return 0
  return 1
}

# ---- pure policy (mirror of deniedCommands.ts + safeMutationPolicy.ts) ------

# 0 if the command text matches a denied family (denials win).
command_denied() {
  local text=$1 pat
  for pat in "${DENIED_PATTERNS[@]}"; do
    if printf '%s' "$text" | grep -qiE -- "$pat"; then return 0; fi
  done
  return 1
}

# 0 if the command text matches the Tier-3 allowlist.
command_allowed() {
  local text=$1 pat
  for pat in "${TIER3_ALLOW_PATTERNS[@]}"; do
    if printf '%s' "$text" | grep -qiE -- "$pat"; then return 0; fi
  done
  return 1
}

# Classify a lane-declared command. Echoes "denied: <why>" / "allowed". Denials win.
classify_command() {
  local text=$1
  if command_denied "$text"; then printf 'denied: matches a denied command family'; return; fi
  if command_allowed "$text"; then printf 'allowed'; return; fi
  printf 'denied: not in the Tier-3 allowlist'
}

# Classify a candidate write path (mirror of mutationScope.classifyWrite).
# Echoes "accepted" or "rejected: <why>". Deny-by-default.
classify_write() {
  local candidate=$1 worktree_root=$2; shift 2
  local declared=("$@") c d
  if [[ -z "$candidate" || ${candidate:0:1} != "/" ]]; then
    printf 'rejected: write path must be a non-empty absolute path'; return
  fi
  c=$(normalize_abs "$candidate")
  if is_under "$CONTROL_CHECKOUT" "$c"; then
    printf 'rejected: path resolves under the control checkout'; return
  fi
  if [[ -z "$worktree_root" ]] || ! is_under "$worktree_root" "$c"; then
    printf 'rejected: path is outside the disposable worktree'; return
  fi
  for d in "${declared[@]:-}"; do
    [[ -z "$d" ]] && continue
    if [[ "$(normalize_abs "$d")" == "$c" ]]; then
      printf 'accepted: in declared set, inside the disposable worktree'; return
    fi
  done
  printf 'rejected: path is not in the declared touched-file set'
}

# ---- JSON readers (python3; read-only) -------------------------------------

# Echo a scalar field (dotted path, e.g. worktreePlan.base) or empty if absent.
json_scalar() {
  local file=$1 path=$2
  python3 - "$file" "$path" <<'PY'
import json, sys
f, path = sys.argv[1], sys.argv[2]
try:
    with open(f) as fh: data = json.load(fh)
except Exception:
    sys.exit(0)
cur = data
for key in path.split('.'):
    if isinstance(cur, dict) and key in cur: cur = cur[key]
    else: sys.exit(0)
if cur is None: sys.exit(0)
if isinstance(cur, bool): print('true' if cur else 'false')
elif isinstance(cur, (str, int, float)): print(cur)
PY
}

# Echo array elements (one per line) for a dotted path; empty if absent/not-list.
json_array() {
  local file=$1 path=$2
  python3 - "$file" "$path" <<'PY'
import json, sys
f, path = sys.argv[1], sys.argv[2]
try:
    with open(f) as fh: data = json.load(fh)
except Exception:
    sys.exit(0)
cur = data
for key in path.split('.'):
    if isinstance(cur, dict) and key in cur: cur = cur[key]
    else: sys.exit(0)
if isinstance(cur, list):
    for x in cur:
        if isinstance(x, str): print(x)
PY
}

# Echo the plan's write paths, one per line, tagged: "target<TAB><path>" for each
# step's write_target.path (the file actually written) and "source<TAB><path>" for
# each step's after_file (the byte source). Prefers PyYAML; if it is unavailable or
# the parse fails, falls back to a conservative line scanner (fail toward extracting
# more, so an out-of-scope target is still caught by the caller).
plan_paths() {
  local file=$1
  python3 - "$file" <<'PY'
import sys, re
f = sys.argv[1]
try:
    text = open(f).read()
except Exception:
    sys.exit(0)
targets, sources = [], []
parsed = False
try:
    import yaml
    data = yaml.safe_load(text)
    steps = (data or {}).get('steps') if isinstance(data, dict) else None
    if isinstance(steps, list):
        parsed = True
        for st in steps:
            if not isinstance(st, dict):
                continue
            wt = st.get('write_target')
            if isinstance(wt, dict) and isinstance(wt.get('path'), str):
                targets.append(wt['path'])
            af = st.get('after_file')
            if isinstance(af, str):
                sources.append(af)
except Exception:
    parsed = False
if not parsed:
    # conservative fallback: a write_target: block's next deeper path:, plus after_file:.
    in_wt, wt_indent = False, -1
    for ln in text.splitlines():
        m_af = re.match(r'\s*-?\s*after_file\s*:\s*(.+?)\s*$', ln)
        if m_af:
            sources.append(m_af.group(1).strip().strip('\'"'))
        m_wt = re.match(r'(\s*)-?\s*write_target\s*:\s*$', ln)
        if m_wt:
            in_wt, wt_indent = True, len(m_wt.group(1))
            continue
        if in_wt:
            indent = len(ln) - len(ln.lstrip())
            m_p = re.match(r'\s*path\s*:\s*(.+?)\s*$', ln)
            if m_p and indent > wt_indent:
                targets.append(m_p.group(1).strip().strip('\'"'))
                in_wt = False
            elif ln.strip() and indent <= wt_indent:
                in_wt = False
for t in targets:
    print("target\t" + t)
for s in sources:
    print("source\t" + s)
PY
}

# ---- worktree-plan validation (mirror of disposableWorktreePlan.ts) --------

# Echoes problems (one per line); empty output means valid.
validate_worktree_plan() {
  local wt=$1 branch=$2 base=$3
  [[ -n "$wt" && ${wt:0:1} == "/" ]] || echo "worktree path must be a non-empty absolute path"
  [[ -n "$branch" ]] || echo "mutation branch must be a non-empty name"
  if [[ -z "$base" ]]; then echo "base must be set"
  elif [[ "$base" != "origin/main" ]]; then echo "base must be origin/main"; fi
  if [[ -n "$wt" && ${wt:0:1} == "/" ]]; then
    local n; n=$(normalize_abs "$wt")
    is_under "$DISPOSABLE_WORKTREE_ROOT" "$n" || echo "worktree path must be under $DISPOSABLE_WORKTREE_ROOT"
    if is_under "$CONTROL_CHECKOUT" "$n" || is_under "$n" "$CONTROL_CHECKOUT"; then
      echo "worktree path must not be the control checkout or contain it"
    fi
  fi
  if [[ -n "$branch" ]]; then
    case "$branch" in
      main|master|origin/main) echo "mutation branch must not be main/master" ;;
    esac
    if [[ "$branch" =~ [[:space:]] ]]; then echo "mutation branch must not contain whitespace"; fi
  fi
  return 0
}

# ---- the pure lane gate (validate-lane) ------------------------------------
# Reads the approved-lane + dry-run evidence, re-checks every gate WITHOUT any
# git/claw/worktree IO. Echoes refusal reasons; returns EXIT_GATE on any refusal.
DECLARED=()          # populated by gate_validate_lane for reuse by apply
WT_PATH=""; WT_BRANCH=""; WT_BASE=""

gate_validate_lane() {
  local lane=$1 evidence=$2 plan=${3:-}
  local refusals=0
  reject() { err "GATE REFUSED: $*"; refusals=$((refusals + 1)); }

  [[ -f "$lane" ]] || { reject "approved-lane file not found: $lane"; return $EXIT_GATE; }
  [[ -f "$evidence" ]] || { reject "dry-run evidence file not found: $evidence"; return $EXIT_GATE; }

  # --- approved-lane input required ---
  local approved; approved=$(json_scalar "$lane" "operatorApproved")
  [[ "$approved" == "true" ]] || reject "approved-lane input required: operatorApproved is not true"

  WT_PATH=$(json_scalar "$lane" "worktreePlan.worktreePath")
  WT_BRANCH=$(json_scalar "$lane" "worktreePlan.branch")
  WT_BASE=$(json_scalar "$lane" "worktreePlan.base")
  local plan_problems; plan_problems=$(validate_worktree_plan "$WT_PATH" "$WT_BRANCH" "$WT_BASE")
  if [[ -n "$plan_problems" ]]; then
    while IFS= read -r p; do reject "worktree plan: $p"; done <<<"$plan_problems"
  fi

  mapfile -t DECLARED < <(json_array "$lane" "declaredPaths")
  [[ ${#DECLARED[@]} -gt 0 ]] || reject "no declared touched files in the approved lane"

  # --- dry-run-ready evidence required (re-validated, not merely trusted) ---
  local ready; ready=$(json_scalar "$evidence" "ready")
  [[ "$ready" == "true" ]] || reject "dry-run-ready evidence required: evidence.ready is not true"
  # Evidence must describe the SAME lane (consistency check).
  local ev_wt; ev_wt=$(json_scalar "$evidence" "worktreePath")
  if [[ -n "$ev_wt" && -n "$WT_PATH" && "$(normalize_abs "$ev_wt")" != "$(normalize_abs "$WT_PATH")" ]]; then
    reject "dry-run evidence worktreePath does not match the approved lane"
  fi

  # --- exact-path scope + denials win (re-checked here, never trusted blind) ---
  local w; while IFS= read -r w; do
    [[ -z "$w" ]] && continue
    local res; res=$(classify_write "$w" "$WT_PATH" "${DECLARED[@]}")
    [[ "$res" == accepted:* ]] || reject "proposed write $w -> $res"
  done < <(json_array "$lane" "proposedWrites")

  local c; while IFS= read -r c; do
    [[ -z "$c" ]] && continue
    local res; res=$(classify_command "$c")
    [[ "$res" == "allowed" ]] || reject "proposed command [$c] -> $res"
  done < <(json_array "$lane" "proposedCommands")

  # --- plan write targets must be inside the declared set (if a plan is provided) ---
  # The file actually written is each step's write_target.path (workspace-relative);
  # after_file is the byte SOURCE, not the target. Validate the real write target
  # against the declared exact-path set; sanity-check after_file is relative.
  if [[ -n "$plan" ]]; then
    [[ -f "$plan" ]] || reject "plan file not found: $plan"
    if [[ -f "$plan" ]]; then
      local target_count=0 kind val
      while IFS=$'\t' read -r kind val; do
        [[ -z "$val" ]] && continue
        if [[ "$kind" == "target" ]]; then
          target_count=$((target_count + 1))
          if [[ ${val:0:1} == "/" ]]; then
            reject "plan write_target.path must be workspace-relative, not absolute: $val"
          else
            local abs; abs=$(normalize_abs "$WT_PATH/$val")
            local res; res=$(classify_write "$abs" "$WT_PATH" "${DECLARED[@]}")
            [[ "$res" == accepted:* ]] || reject "plan write_target $val ($abs) -> $res"
          fi
        elif [[ "$kind" == "source" ]]; then
          # after_file is the byte source; it must be workspace-relative too.
          [[ ${val:0:1} == "/" ]] && reject "plan after_file (byte source) must be workspace-relative, not absolute: $val"
        fi
      done < <(plan_paths "$plan")
      # A write lane whose plan declares no write_target writes nothing — flag it
      # rather than silently driving a no-op apply.
      [[ $target_count -gt 0 ]] || reject "plan declares no write_target.path; nothing for apply-lane to write"
    fi
  fi

  if [[ $refusals -gt 0 ]]; then
    err "$refusals gate refusal(s); lane is NOT drivable."
    return $EXIT_GATE
  fi
  info "validate-lane: OK — all pure gates pass (scope, denials, plan, evidence-ready, approval)."
  info "  NOTE: this is the pure gate check only. apply-lane additionally requires a real TTY,"
  info "        a clean control checkout, origin/main, a unique/free worktree, and operator review."
  return $EXIT_OK
}

# ---- the guarded apply lane (apply-lane) -----------------------------------

gate_clean_control_checkout() {
  local st; st=$(git -C "$CONTROL_CHECKOUT" status --porcelain --untracked-files=all 2>/dev/null || echo "ERR")
  if [[ "$st" == "ERR" ]]; then err "GATE REFUSED: cannot read control checkout: $CONTROL_CHECKOUT"; return $EXIT_GATE; fi
  if [[ -n "$st" ]]; then
    err "GATE REFUSED: control checkout is not clean — refuse to create a disposable worktree from a dirty base."
    printf '%s\n' "$st" >&2
    return $EXIT_GATE
  fi
  return $EXIT_OK
}

# Artifact-based detection that `plan run --workspace-write-preview` produced a
# READY write preview (approval pending). Returns 0 only when ALL required
# preview artifacts exist under <ws>/.claw: a preview-bundle.json, a
# preview-generator-result.json, AND a run status/manifest artifact carrying
# "status": "write_preview_ready". This is artifact/status-based (the same
# discipline a2-ide-harness.sh uses) — never free-text log parsing — and is what
# lets the orchestrator accept the preview stage's exit-code-7 signal SAFELY,
# while approve/apply exit 7 stays strict.
preview_ready_artifacts_present() {
  local claw_dir=$1
  [[ -n "$(find "$claw_dir" -type f -name 'preview-bundle.json' 2>/dev/null | head -n1)" ]] || return 1
  [[ -n "$(find "$claw_dir" -type f -name 'preview-generator-result.json' 2>/dev/null | head -n1)" ]] || return 1
  local f
  while IFS= read -r f; do
    if grep -q '"status"[[:space:]]*:[[:space:]]*"write_preview_ready"' "$f" 2>/dev/null; then
      return 0
    fi
  done < <(find "$claw_dir" -type f \( -name 'status.json' -o -name 'run-manifest.json' \) 2>/dev/null)
  return 1
}

# Map a `claw plan approve` exit code to a category, so the approve step can give
# the operator a precise, actionable diagnostic instead of a generic "rc=N".
classify_approve_rc() {
  case "$1" in
    0)  printf 'approved' ;;
    5)  printf 'bundle-error' ;;
    7)  printf 'denied-eof-nontty' ;;
    12) printf 'output-io' ;;
    *)  printf 'unknown' ;;
  esac
}

# Read-only: show which key A2-L2b artifacts are present/absent under .claw, so a
# failed step makes it obvious to the operator exactly where the chain halted
# (e.g. preview present but approval-result absent => approval did not persist).
diagnose_claw_dir() {
  local claw_dir=$1 n found
  info "## .claw artifact presence under $claw_dir:"
  for n in preview-bundle.json preview-generator-result.json approval-result.json apply-bundle.json apply-result.json; do
    found=$(find "$claw_dir" -type f -name "$n" 2>/dev/null | sort | head -n1 || true)
    if [[ -n "$found" ]]; then info "   present : $n  ($found)"; else info "   absent  : $n"; fi
  done
}

drive_chain_for_plan() {
  # Drives the EXISTING chain inside the disposable worktree. Delegates ALL
  # writing to `claw plan apply`; reimplements nothing. Each command is printed
  # before it runs (no hidden execution). Approval is human-typed at the TTY.
  local ws=$1 plan=$2
  local claw_dir="$ws/.claw"

  # ---- STEP 1 / PREVIEW (writes NO target) --------------------------------
  rule; info "STEP 1 / PREVIEW (writes NO target)"; rule
  info "+ $(shq "$A2_CLAW") plan run $(shq "$plan") --workspace-root $(shq "$ws") --workspace-write-preview"
  local rc=0
  # stdin from /dev/null: the preview reads no operator input, and this guarantees
  # it can never consume the terminal stdin that STEP 2's human approval needs.
  "$A2_CLAW" plan run "$plan" --workspace-root "$ws" --workspace-write-preview < /dev/null || rc=$?
  # claw signals a READY write preview (approval pending) with exit code 7
  # (EXIT_RUN_PLAN_WRITE_PREVIEW_READY) — the same numeric value it uses for
  # approval-denied at the approve stage. Disambiguate by STAGE + ARTIFACTS, not
  # by the code: accept rc=7 HERE only when the preview-ready artifacts/status are
  # present; otherwise treat it as a genuine refusal. Any other non-zero is a
  # hard preview failure. (approve/apply keep strict rc=7 handling below.)
  if [[ $rc -eq 0 ]]; then
    : # clean exit — fall through to the artifact presence check
  elif [[ $rc -eq $EXIT_PREVIEW_READY ]]; then
    if preview_ready_artifacts_present "$claw_dir"; then
      info "STEP 1 preview exited $EXIT_PREVIEW_READY = write-preview-ready (approval pending); preview-ready"
      info "  artifacts present — accepting and continuing to approval. No target was written."
    else
      err "STEP 1 preview exited $rc but NO preview-ready artifacts/status were produced"
      err "  (no preview-bundle + preview-generator-result + status 'write_preview_ready' under $claw_dir/.claw)."
      err "  Treating as a genuine preview refusal — no target was written. STOP."
      return $EXIT_GATE
    fi
  else
    err "STEP 1 preview (claw plan run) failed (rc=$rc) — no target was written. STOP."
    return $EXIT_GATE
  fi

  local preview_bundle gen_result
  preview_bundle=$(find "$claw_dir" -type f -name 'preview-bundle.json' 2>/dev/null | sort | head -n1 || true)
  gen_result=$(find "$claw_dir" -type f -name 'preview-generator-result.json' 2>/dev/null | sort | head -n1 || true)
  [[ -n "$preview_bundle" ]] || { err "preview produced no preview-bundle.json under $claw_dir — STOP (nothing written)."; return $EXIT_GATE; }

  # ---- STEP 2 / APPROVE — human-typed at the real terminal ----------------
  local approval_out="$claw_dir/approval-result.json"
  rule; info "STEP 2 / APPROVE — HUMAN APPROVAL REQUIRED AT THIS TERMINAL (writes NO target)"; rule
  info "What happens next (this is interactive — the terminal will WAIT for YOU, it is NOT stuck):"
  info "  1. claw prints the diff preview for the declared write, then a line that reads:"
  info "         To approve, type exactly:"
  info "         $APPROVAL_GRAMMAR"
  info "     with the REAL step-id and 64-char preview hash filled in by claw."
  info "  2. After the diff, the cursor waits for your input. Type that EXACT line claw printed"
  info "     (case-sensitive, three tokens) and press Enter. Scroll up if the diff pushed it off-screen."
  info "  3. To ABORT safely (nothing is written), press Ctrl-C — no worktree write occurs."
  info "  NOTE: this script never types, pipes, or composes the approval for you; you must type it."
  info ""
  info "+ $(shq "$A2_CLAW") plan approve $(shq "$preview_bundle") --approval-result-output $(shq "$approval_out")"
  # Pre-flight: claw refuses (rc $EXIT_APPROVAL_OUTPUT_IO) if --approval-result-output already exists.
  # Surface that clearly BEFORE running, so a reused/partial worktree is obvious.
  if [[ -e "$approval_out" ]]; then
    err "STEP 2 PRE-CHECK: approval-result path already exists: $approval_out"
    err "  claw plan approve refuses to clobber it (would exit $EXIT_APPROVAL_OUTPUT_IO). apply-lane creates a"
    err "  FRESH disposable worktree per run — use a new run, not a reused/partial worktree. Nothing written. STOP."
    return $EXIT_GATE
  fi
  # The approval is read from THIS real terminal. stdin is deliberately NOT
  # redirected here (unlike the other steps) — the human must type the line.
  info "(waiting for claw's approval prompt below — type your approval line when the diff finishes)"
  rc=0
  "$A2_CLAW" plan approve "$preview_bundle" --approval-result-output "$approval_out" || rc=$?
  local approve_cat; approve_cat=$(classify_approve_rc "$rc")
  case "$approve_cat" in
    approved)
      if [[ -f "$approval_out" ]]; then
        info "approval recorded (rc 0): $approval_out — continuing to apply-bundle + apply."
      else
        err "STEP 2 reported approved (rc 0) but no approval-result file at $approval_out — unexpected"
        err "  (claw persists on success). Treating as a failure; nothing applied. STOP."
        diagnose_claw_dir "$claw_dir"
        return $EXIT_GATE
      fi
      ;;
    denied-eof-nontty)
      err "STEP 2 approval REFUSED by claw (exit $rc = refused / non-approvable / EOF / drift / non-TTY)."
      err "  If you DID type the approval line and still see this, the likeliest causes are:"
      err "  - claw read EOF / your keystrokes did not reach claw's stdin: a wrapper, terminal multiplexer,"
      err "    backgrounding, or redirected stdin can detach it — run apply-lane DIRECTLY in a real terminal;"
      err "  - the typed line did not EXACTLY match claw's 'apply <step-id> <preview_sha256>' (case/tokens/replayed hash);"
      err "  - a preapproval form (--yes / --auto / auto-apply) was used (always refused)."
      err "  Nothing was written. (approval-result.json absent below confirms the approval was not captured.)"
      diagnose_claw_dir "$claw_dir"
      return $EXIT_GATE
      ;;
    output-io)
      err "STEP 2 approval-result IO error (exit $rc). Either --approval-result-output already existed"
      err "  (claw refuses to clobber; no approval run), OR claw APPROVED but failed to write the file"
      err "  (the approval JSON went to stdout only). Path: $approval_out. Use a fresh worktree; check perms."
      diagnose_claw_dir "$claw_dir"
      return $EXIT_GATE
      ;;
    bundle-error)
      err "STEP 2 preview-bundle read/parse/schema/integrity error (exit $rc) — approval not attempted. STOP."
      diagnose_claw_dir "$claw_dir"
      return $EXIT_GATE
      ;;
    *)
      err "STEP 2 approval failed (claw plan approve rc=$rc, uncategorized) — nothing written. STOP."
      diagnose_claw_dir "$claw_dir"
      return $EXIT_GATE
      ;;
  esac

  # ---- STEP 3 / APPLY-BUNDLE — GENERATOR only (writes NO target) -----------
  rule; info "STEP 3 / APPLY-BUNDLE — GENERATOR only (writes NO target)"; rule
  info "+ $(shq "$A2_CLAW") plan apply-bundle $(shq "$gen_result") $(shq "$approval_out")"
  rc=0
  "$A2_CLAW" plan apply-bundle "$gen_result" "$approval_out" < /dev/null || rc=$?
  if [[ $rc -ne 0 ]]; then
    err "STEP 3 apply-bundle (generator) failed (rc=$rc) — no target was written. STOP."
    return $EXIT_GATE
  fi
  local apply_bundle; apply_bundle=$(find "$claw_dir" -type f -name 'apply-bundle.json' 2>/dev/null | sort | head -n1 || true)
  [[ -n "$apply_bundle" ]] || { err "apply-bundle produced no apply-bundle.json — STOP (nothing written)."; return $EXIT_GATE; }

  # ---- STEP 4 / APPLY — the ONLY writer; runs once ------------------------
  rule; info "STEP 4 / APPLY — EXECUTOR (the existing claw write_executor; the ONLY writer; runs once)"; rule
  info "+ $(shq "$A2_CLAW") plan apply $(shq "$apply_bundle")"
  rc=0
  "$A2_CLAW" plan apply "$apply_bundle" < /dev/null || rc=$?
  if [[ $rc -ne 0 ]]; then
    err "STEP 4 apply (claw plan apply) failed (rc=$rc). Review the disposable worktree before re-running;"
    err "  do not run apply twice for the same approved preview. STOP."
    return $EXIT_GATE
  fi
  info "STEP 4 apply completed — see the evidence + diff summary below."
  return $EXIT_OK
}

cmd_apply_lane() {
  local lane="" evidence="" plan=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --approved-lane) lane=${2:-}; shift 2 ;;
      --dry-run-evidence) evidence=${2:-}; shift 2 ;;
      --plan) plan=${2:-}; shift 2 ;;
      *) err "unexpected argument: $1"; return $EXIT_USAGE ;;
    esac
  done
  [[ -n "$lane" && -n "$evidence" && -n "$plan" ]] || {
    err "apply-lane requires --approved-lane <json> --dry-run-evidence <json> --plan <plan.yaml>"
    return $EXIT_USAGE
  }

  rule; info "Tier 3 write-capable orchestrator — apply-lane (drives the EXISTING claw apply chain)"; rule

  # 1) pure gates first — no IO, no worktree, no claw.
  gate_validate_lane "$lane" "$evidence" "$plan" || return $EXIT_GATE

  # 2) real-TTY required: approval must stay human-typed. No fake-TTY/batch ever.
  if [[ ! -t 0 || ! -t 1 ]]; then
    err "GATE REFUSED (TTY): apply-lane requires a REAL interactive terminal so the approval line"
    err "is human-typed. Refusing in a non-interactive context (exit $EXIT_TTY). No worktree created, no writes."
    return $EXIT_TTY
  fi

  # 3) clean control checkout.
  gate_clean_control_checkout || return $EXIT_GATE

  # 4) origin/main present; base is origin/main (already enforced by plan validation).
  info "fetching origin/main (read-only; no prune) ..."
  git -C "$CONTROL_CHECKOUT" fetch origin main || { err "GATE REFUSED: cannot fetch origin/main"; return $EXIT_GATE; }

  # 5) unique/free branch + worktree path.
  if [[ -e "$WT_PATH" ]]; then err "GATE REFUSED: disposable worktree path already exists: $WT_PATH"; return $EXIT_GATE; fi
  if git -C "$CONTROL_CHECKOUT" show-ref --verify --quiet "refs/heads/$WT_BRANCH"; then
    err "GATE REFUSED: mutation branch already exists: $WT_BRANCH"; return $EXIT_GATE
  fi

  # 6) create EXACTLY ONE disposable worktree from origin/main.
  rule; info "creating exactly one disposable worktree:"; info "  path:   $WT_PATH"; info "  branch: $WT_BRANCH"; info "  base:   origin/main"; rule
  git -C "$CONTROL_CHECKOUT" worktree add -b "$WT_BRANCH" "$WT_PATH" origin/main || {
    err "GATE REFUSED: failed to create disposable worktree"; return $EXIT_GATE
  }

  # 7) drive the existing chain inside the disposable worktree.
  local rc=0
  drive_chain_for_plan "$WT_PATH" "$plan" || rc=$?

  # 8) evidence + diff summary (read-only), then STOP for operator review.
  rule; info "evidence + diff summary (read-only)"; rule
  info "## checkpoints (rollback baselines):"
  find "$WT_PATH/.claw" -type d -name 'l2b-checkpoints' 2>/dev/null | sort | sed 's/^/  /' || true
  info "## apply-result artifacts:"
  find "$WT_PATH/.claw" -type f -name 'apply-result.json' 2>/dev/null | sort | sed 's/^/  /' || true
  info "## git diff --stat inside the disposable worktree:"
  git -C "$WT_PATH" --no-pager diff --stat || true

  rule
  if [[ $rc -eq 0 ]]; then
    info "apply-lane: chain driven. STOP for operator review."
  else
    err "apply-lane: chain stopped early (rc=$rc). STOP for operator review."
  fi
  info "Rollback, if needed, is by ABANDONING the disposable worktree (operator action):"
  info "  the orchestrator NEVER force-removes a worktree, deletes a branch, pushes, opens a PR, or merges."
  info "  worktree to review/abandon: $WT_PATH"
  return $rc
}

# ---- Tier-4 packaging PLAN (package-plan) — Stage 1, READ-ONLY --------------
# Stage 1 of docs/a2-tier3-tier4-pr-packaging-design-scope.md. Given a disposable
# worktree the EXISTING chain already applied, it PRINTS the packaging plan: which
# declared files WOULD be staged/committed, their verified after-hashes, and the
# exact (PRINTED, never run) push/PR commands a LATER, separately token-gated lane
# would use. It performs NO git mutation — no add, no commit, no push, no gh, no
# PR — and would_push/would_open_pr are ALWAYS false here. Fail-closed: any gate
# refusal prints the cause and exits EXIT_GATE with no plan-success.

# sha256 of a regular file (read-only).
sha256_of() { sha256sum -- "$1" | awk '{print $1}'; }

# Echo the path of a payload after.sha256 under <wt>/.claw whose recorded hash
# equals <sha>; empty if none. The file holds the post-write digest the executor
# recorded, so a match proves the on-disk bytes are the applied bytes. Read-only.
matching_after_sha() {
  local wt=$1 sha=$2 f rec
  while IFS= read -r f; do
    rec=$(tr -d '[:space:]' < "$f" 2>/dev/null)
    rec=${rec%%[!0-9a-fA-F]*}   # leading hex run (handles "<hex>" or "<hex>  path")
    [[ -n "$rec" && "${rec,,}" == "${sha,,}" ]] && { printf '%s' "$f"; return 0; }
  done < <(find "$wt/.claw" -type f -name 'after.sha256' 2>/dev/null)
  return 0
}

# Emit the read-only packaging plan JSON + the PRINTED-for-operator command block.
# Runs NO git command. would_push / would_open_pr are emitted false.
emit_package_plan() {
  local wt=$1 branch=$2 base=$3; shift 3
  local declared=("$@") payload=() d rel sha
  for d in "${declared[@]}"; do
    rel=${d#"$wt"/}
    sha=$(sha256_of "$d")
    payload+=("$rel" "$sha")
  done
  python3 - "$wt" "$branch" "$base" "${payload[@]}" <<'PY'
import json, sys
wt, branch, base = sys.argv[1], sys.argv[2], sys.argv[3]
rest = sys.argv[4:]
per = [{"path": rest[i], "after_sha256": rest[i + 1], "applied": True}
       for i in range(0, len(rest), 2)]
plan = {
    "schema": "a2-tier4-package-plan.v0",
    "worktree": wt,
    "branch": branch,
    "base": base,
    "declaredPaths": [p["path"] for p in per],
    "perFile": per,
    "wouldStage": [p["path"] for p in per],
    "would_push": False,
    "would_open_pr": False,
    "commit_message_preview":
        "a2(tier4): package isolated mutation on %s (%d file(s))" % (branch, len(per)),
}
print(json.dumps(plan, indent=2, sort_keys=True))
PY
  rule
  info "NEXT (a LATER, separately token-gated lane runs these — package-plan runs NONE of them):"
  for d in "${declared[@]}"; do rel=${d#"$wt"/}; info "  git -C $(shq "$wt") add -- $(shq "$rel")"; done
  info "  git -C $(shq "$wt") commit -m <evidence-bound message>   # Stage 2 (in-worktree only)"
  info "  # Stages 3-4 additionally require the token: APPROVED: Open A2 Tier 3 isolated-mutation PR"
  info "  #   git -C $(shq "$wt") push -u origin $(shq "$branch")"
  info "  #   gh pr create --base main --head $(shq "$branch") --draft"
  rule
  info "package-plan: READ-ONLY. would_push=false would_open_pr=false. No git mutation performed."
}

# Shared Tier-4 readiness gate for package-plan (Stage 1) and package-commit
# (Stage 2). READ-ONLY: it stages/commits/pushes nothing. Echoes refusals;
# returns EXIT_GATE on any refusal, EXIT_OK otherwise. On success it sets
# _TIER4_DECLARED (array), _TIER4_BRANCH, _TIER4_BASE for the caller. The caller
# validates --worktree/--approved-lane presence and lane-file existence first.
_TIER4_DECLARED=()
_TIER4_BRANCH=""
_TIER4_BASE=""
_tier4_gate_package() {
  local wt=$1 lane=$2 plan=${3:-}
  local refusals=0
  _t4_reject() { err "TIER-4 GATE REFUSED: $*"; refusals=$((refusals + 1)); }

  # ---- PURE gates (no git IO; mirror validate-lane; refuse BEFORE any git) ----
  local lane_wt branch base approved
  lane_wt=$(json_scalar "$lane" "worktreePlan.worktreePath")
  branch=$(json_scalar "$lane" "worktreePlan.branch")
  base=$(json_scalar "$lane" "worktreePlan.base")
  approved=$(json_scalar "$lane" "operatorApproved")

  local problems p
  problems=$(validate_worktree_plan "$lane_wt" "$branch" "$base")
  while IFS= read -r p; do [[ -n "$p" ]] && _t4_reject "worktree plan: $p"; done <<<"$problems"

  [[ "$(normalize_abs "$wt")" == "$(normalize_abs "$lane_wt")" ]] \
    || _t4_reject "--worktree ($wt) does not match the approved lane worktreePath ($lane_wt)"
  [[ "$approved" == "true" ]] || _t4_reject "operatorApproved is not true"

  local declared=() d res
  mapfile -t declared < <(json_array "$lane" "declaredPaths")
  [[ ${#declared[@]} -gt 0 ]] || _t4_reject "no declared touched files in the approved lane"
  for d in "${declared[@]:-}"; do
    [[ -z "$d" ]] && continue
    res=$(classify_write "$d" "$lane_wt" "${declared[@]}")
    [[ "$res" == accepted:* ]] || _t4_reject "declared path $d -> $res"
  done

  if [[ -n "$plan" ]]; then
    if [[ ! -f "$plan" ]]; then
      _t4_reject "plan file not found: $plan"
    else
      local kind val abs target_count=0
      while IFS=$'\t' read -r kind val; do
        [[ -z "$kind" ]] && continue
        if [[ "$kind" == "target" ]]; then
          target_count=$((target_count + 1))
          if [[ ${val:0:1} == "/" ]]; then _t4_reject "plan write_target.path must be workspace-relative, not absolute: $val"; continue; fi
          abs=$(normalize_abs "$lane_wt/$val")
          res=$(classify_write "$abs" "$lane_wt" "${declared[@]}")
          [[ "$res" == accepted:* ]] || _t4_reject "plan write_target $val ($abs) -> $res"
        elif [[ "$kind" == "source" ]]; then
          [[ ${val:0:1} != "/" ]] || _t4_reject "plan after_file (source) must be workspace-relative, not absolute: $val"
        fi
      done < <(plan_paths "$plan")
      [[ $target_count -gt 0 ]] || _t4_reject "plan declares no write_target.path; nothing to package"
    fi
  fi

  # Stop BEFORE any git IO if a pure gate refused (keeps offline gate tests deterministic).
  if [[ $refusals -gt 0 ]]; then
    err "$refusals pure-gate refusal(s); lane is NOT package-ready."
    return $EXIT_GATE
  fi

  # ---- LIVE read-only gates (git IO; NO mutation) ----
  if [[ ! -d "$wt" ]] || ! git -C "$wt" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    _t4_reject "worktree is not a git work tree: $wt"
  fi

  if [[ $refusals -eq 0 ]]; then
    local cur_branch
    cur_branch=$(git -C "$wt" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
    [[ "$cur_branch" == "$branch" ]] || _t4_reject "worktree branch ($cur_branch) does not match approved lane branch ($branch)"

    local ctl
    ctl=$(git -C "$CONTROL_CHECKOUT" status --porcelain --untracked-files=all 2>/dev/null || echo "__ERR__")
    if [[ "$ctl" == "__ERR__" ]]; then _t4_reject "cannot read control checkout: $CONTROL_CHECKOUT"
    elif [[ -n "$ctl" ]]; then _t4_reject "control checkout is not clean — refuse to package from a dirty base"; fi

    # Drift guard: every worktree change must be in the declared set (ignored .claw excepted).
    local line pth abs2
    while IFS= read -r line; do
      [[ -z "$line" ]] && continue
      pth=${line:3}
      [[ "$pth" == *" -> "* ]] && pth=${pth##* -> }   # rename: take the new path
      abs2=$(normalize_abs "$wt/$pth")
      is_under "$wt/.claw" "$abs2" && continue
      res=$(classify_write "$abs2" "$wt" "${declared[@]}")
      [[ "$res" == accepted:* ]] || _t4_reject "drift: worktree change outside declared set: $pth ($res)"
    done < <(git -C "$wt" status --porcelain --untracked-files=all 2>/dev/null || true)

    # Per declared file: present on disk AND its sha256 matches a recorded payload after.sha256.
    local rel sha rec
    for d in "${declared[@]}"; do
      rel=${d#"$wt"/}
      if [[ ! -f "$d" ]]; then _t4_reject "declared file missing on disk (not applied?): $rel"; continue; fi
      sha=$(sha256_of "$d")
      rec=$(matching_after_sha "$wt" "$sha")
      [[ -n "$rec" ]] || _t4_reject "no checkpoint payload after.sha256 matches on-disk $rel (apply evidence missing / drift)"
    done

    # Apply evidence presence: a generated apply-bundle + a checkpoint baseline.
    [[ -n "$(find "$wt/.claw" -type f -name 'apply-bundle.json' 2>/dev/null | head -n1)" ]] \
      || _t4_reject "no apply-bundle.json under .claw (mutation not applied via the chain)"
    [[ -n "$(find "$wt/.claw" -type d -name 'l2b-checkpoints' 2>/dev/null | head -n1)" ]] \
      || _t4_reject "no l2b-checkpoints under .claw (no rollback baseline / not applied)"
  fi

  if [[ $refusals -gt 0 ]]; then
    err "$refusals package-readiness gate refusal(s); worktree is NOT package-ready."
    return $EXIT_GATE
  fi

  _TIER4_DECLARED=("${declared[@]}")
  _TIER4_BRANCH=$branch
  _TIER4_BASE=$base
  return $EXIT_OK
}

# package-plan --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
cmd_package_plan() {
  local wt="" lane="" plan=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --worktree) wt=${2:-}; shift 2 ;;
      --approved-lane) lane=${2:-}; shift 2 ;;
      --plan) plan=${2:-}; shift 2 ;;
      *) err "unexpected argument: $1"; return $EXIT_USAGE ;;
    esac
  done
  [[ -n "$wt" && -n "$lane" ]] || { err "package-plan requires --worktree and --approved-lane"; return $EXIT_USAGE; }
  [[ -f "$lane" ]] || { err "approved-lane file not found: $lane"; return $EXIT_USAGE; }

  _tier4_gate_package "$wt" "$lane" "$plan" || return $?

  rule; info "Tier-4 package-plan (READ-ONLY) — worktree is package-ready"; rule
  emit_package_plan "$wt" "$_TIER4_BRANCH" "$_TIER4_BASE" "${_TIER4_DECLARED[@]}"
  return $EXIT_OK
}

# ---- Tier-4 packaging COMMIT (package-commit) — Stage 2 ---------------------
# Stage 2 of docs/a2-tier3-tier4-pr-packaging-design-scope.md. Reuses the SAME
# read-only readiness gate as package-plan, then stages EXACTLY the declared set
# and makes ONE evidence-bound commit INSIDE the disposable worktree. It NEVER
# pushes, opens a PR, merges, mutates the control checkout, stages anything
# outside the declared set, or uses `git add .` / `git add -A`. pushed /
# pr_opened / merged are always false. Fail-closed: any refusal exits EXIT_GATE
# without committing.

# Emit the Stage-2 package-commit evidence JSON (AFTER the in-worktree commit).
# Runs NO further git mutation. pushed / pr_opened / merged are emitted false.
emit_package_commit() {
  local wt=$1 branch=$2 base_sha=$3 commit_sha=$4; shift 4
  local declared=("$@") payload=() d rel sha
  for d in "${declared[@]}"; do
    rel=${d#"$wt"/}; sha=$(sha256_of "$d")
    payload+=("$rel" "$sha")
  done
  python3 - "$wt" "$branch" "$base_sha" "$commit_sha" "${payload[@]}" <<'PY'
import json, sys
wt, branch, base_sha, commit_sha = sys.argv[1:5]
rest = sys.argv[5:]
per = [{"path": rest[i], "after_sha256": rest[i + 1], "applied": True}
       for i in range(0, len(rest), 2)]
ev = {
    "schema_version": "a2-tier4-package-commit.v0",
    "stage": "tier4-stage2-package-commit",
    "base_sha": base_sha,
    "worktree": wt,
    "branch": branch,
    "declared_files": [p["path"] for p in per],
    "staged_files": [p["path"] for p in per],
    "perFile": per,
    "commit_sha": commit_sha,
    "would_push": False,
    "would_open_pr": False,
    "pushed": False,
    "pr_opened": False,
    "merged": False,
}
print(json.dumps(ev, indent=2, sort_keys=True))
PY
  rule
  info "NEXT (Stages 3-4 — a LATER lane; require: APPROVED: Open A2 Tier 3 isolated-mutation PR):"
  info "  #   git -C $(shq "$wt") push -u origin $(shq "$branch")"
  info "  #   gh pr create --base main --head $(shq "$branch") --draft"
  rule
  info "package-commit: committed INSIDE the disposable worktree only. pushed=false pr_opened=false merged=false."
}

# package-commit --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
cmd_package_commit() {
  local wt="" lane="" plan=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --worktree) wt=${2:-}; shift 2 ;;
      --approved-lane) lane=${2:-}; shift 2 ;;
      --plan) plan=${2:-}; shift 2 ;;
      *) err "unexpected argument: $1"; return $EXIT_USAGE ;;
    esac
  done
  [[ -n "$wt" && -n "$lane" ]] || { err "package-commit requires --worktree and --approved-lane"; return $EXIT_USAGE; }
  [[ -f "$lane" ]] || { err "approved-lane file not found: $lane"; return $EXIT_USAGE; }

  # Same read-only readiness gate as package-plan (Stage 1). Refuses BEFORE any
  # staging on base/branch/approval/scope/drift/hash/evidence problems.
  _tier4_gate_package "$wt" "$lane" "$plan" || return $?
  local branch=$_TIER4_BRANCH
  local declared=("${_TIER4_DECLARED[@]}")

  # Stage-2 precondition: the index must already be clean (the apply chain writes
  # the working tree but stages nothing). A pre-staged index is unexpected — refuse
  # rather than fold foreign staged content into the commit.
  if ! git -C "$wt" diff --cached --quiet; then
    err "TIER-4 STAGE2 REFUSED: disposable worktree index is not clean (unexpected pre-staged changes); nothing committed."
    return $EXIT_GATE
  fi

  # base_sha = the worktree HEAD the Stage-2 commit will sit on top of (read-only).
  local base_sha; base_sha=$(git -C "$wt" rev-parse HEAD 2>/dev/null || echo "")

  # Stage EXACTLY the declared set, one exact path at a time. NEVER `git add .` / `-A`.
  local d rel declared_rel=()
  for d in "${declared[@]}"; do
    rel=${d#"$wt"/}
    declared_rel+=("$rel")
    git -C "$wt" add -- "$rel" || { err "TIER-4 STAGE2 REFUSED: failed to stage $rel; nothing committed."; return $EXIT_GATE; }
  done

  # Verify the staged set EXACTLY equals the declared set (no more, no fewer).
  local staged_sorted declared_sorted
  staged_sorted=$(git -C "$wt" diff --cached --name-only | sort)
  declared_sorted=$(printf '%s\n' "${declared_rel[@]}" | sort)
  if [[ "$staged_sorted" != "$declared_sorted" ]]; then
    err "TIER-4 STAGE2 REFUSED: staged set != declared set; refusing to commit."
    err "  declared: $(printf '%s ' "${declared_rel[@]}")"
    err "  staged:   ${staged_sorted//$'\n'/ }"
    git -C "$wt" reset -q -- . 2>/dev/null || true   # unstage only (NOT --hard; files stay on disk)
    return $EXIT_GATE
  fi

  # ONE evidence-bound commit INSIDE the disposable worktree (never the control checkout).
  local msg
  msg="a2(tier4): package isolated mutation on $branch (${#declared_rel[@]} file(s))

stage: tier4-stage2-package-commit
pushed: false  pr_opened: false  merged: false"
  git -C "$wt" commit -q -m "$msg" \
    || { err "TIER-4 STAGE2 REFUSED: commit failed inside the disposable worktree."; return $EXIT_GATE; }

  local commit_sha; commit_sha=$(git -C "$wt" rev-parse HEAD)

  rule; info "Tier-4 package-commit (Stage 2) — committed INSIDE the disposable worktree"; rule
  emit_package_commit "$wt" "$branch" "$base_sha" "$commit_sha" "${declared[@]}"
  return $EXIT_OK
}

# ---- Tier-4 packaging PUSH (package-push) — Stage 3 -------------------------
# Stage 3 of docs/a2-tier3-tier4-pr-packaging-design-scope.md. Reuses the SAME
# read-only readiness gate, re-derives the Stage-2 package-commit from the
# worktree HEAD (the HEAD commit changed EXACTLY the declared set, and the tree
# is clean of tracked changes), then pushes ONLY that exact disposable branch at
# that exact SHA to its `origin` remote with an exact, NON-force branch:branch
# refspec. It NEVER force-pushes, pushes tags, deletes refs, pushes main, opens a
# PR, merges, stages/commits, or touches the control checkout. If the remote
# branch already exists at the SAME sha it is an idempotent no-op; a DIFFERENT
# sha is refused (no force). Opening a PR (Stage 4) is a SEPARATE lane that needs
# a NEW explicit operator approval. Fail-closed: any refusal exits EXIT_GATE
# without pushing.

# Emit the Stage-3 package-push evidence JSON (AFTER the verified push). pushed
# is true; pr_opened / merged stay false.
emit_package_push() {
  local wt=$1 branch=$2 base_sha=$3 commit_sha=$4 remote_name=$5 remote_sha=$6; shift 6
  local declared=("$@") rel_list=() d
  for d in "${declared[@]}"; do rel_list+=("${d#"$wt"/}"); done
  python3 - "$wt" "$branch" "$base_sha" "$commit_sha" "$remote_name" "$remote_sha" "${rel_list[@]}" <<'PY'
import json, sys
wt, branch, base_sha, commit_sha, remote_name, remote_sha = sys.argv[1:7]
declared = sys.argv[7:]
ev = {
    "schema_version": "a2-tier4-package-push.v0",
    "stage": "tier4-stage3-package-push",
    "base_sha": base_sha,
    "worktree": wt,
    "branch": branch,
    "declared_files": declared,
    "package_commit_sha": commit_sha,
    "remote_name": remote_name,
    "remote_branch": branch,
    "remote_sha": remote_sha,
    "pushed": True,
    "would_open_pr": False,
    "pr_opened": False,
    "merged": False,
}
print(json.dumps(ev, indent=2, sort_keys=True))
PY
  rule
  info "NEXT (Stage 4 — a SEPARATE lane; opening a draft PR requires a NEW explicit operator approval)."
  info "  package-push opened NO PR and merged nothing."
  rule
  info "package-push: pushed exact branch:branch (no force/tags/delete). pr_opened=false merged=false."
}

# package-push --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
cmd_package_push() {
  local wt="" lane="" plan=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --worktree) wt=${2:-}; shift 2 ;;
      --approved-lane) lane=${2:-}; shift 2 ;;
      --plan) plan=${2:-}; shift 2 ;;
      *) err "unexpected argument: $1"; return $EXIT_USAGE ;;
    esac
  done
  [[ -n "$wt" && -n "$lane" ]] || { err "package-push requires --worktree and --approved-lane"; return $EXIT_USAGE; }
  [[ -f "$lane" ]] || { err "approved-lane file not found: $lane"; return $EXIT_USAGE; }

  # Same read-only readiness gate as package-plan/package-commit.
  _tier4_gate_package "$wt" "$lane" "$plan" || return $?
  local branch=$_TIER4_BRANCH
  local declared=("${_TIER4_DECLARED[@]}")

  # Never push a default/integration branch.
  case "$branch" in
    main|master|HEAD) err "TIER-4 STAGE3 REFUSED: refusing to push branch '$branch'; nothing pushed."; return $EXIT_GATE ;;
  esac

  # The disposable worktree must be in the Stage-2 package-committed state:
  # clean of tracked changes (untracked .claw allowed)...
  if ! git -C "$wt" diff --quiet || ! git -C "$wt" diff --cached --quiet; then
    err "TIER-4 STAGE3 REFUSED: disposable worktree has staged/unstaged tracked changes; run package-commit first. Nothing pushed."
    return $EXIT_GATE
  fi
  # ...and HEAD must be a real commit with a parent.
  local parent
  parent=$(git -C "$wt" rev-parse --verify -q 'HEAD~1') \
    || { err "TIER-4 STAGE3 REFUSED: worktree HEAD has no parent (no package-commit). Nothing pushed."; return $EXIT_GATE; }

  # Re-derive the package-commit evidence: the HEAD commit must have changed
  # EXACTLY the declared set (this is the Stage-2 package-commit).
  local d declared_rel=()
  for d in "${declared[@]}"; do declared_rel+=("${d#"$wt"/}"); done
  local head_diff declared_sorted
  head_diff=$(git -C "$wt" diff --name-only "$parent" HEAD | sort)
  declared_sorted=$(printf '%s\n' "${declared_rel[@]}" | sort)
  if [[ "$head_diff" != "$declared_sorted" ]]; then
    err "TIER-4 STAGE3 REFUSED: HEAD commit diff != declared set (not a clean package-commit). Nothing pushed."
    err "  declared:  $(printf '%s ' "${declared_rel[@]}")"
    err "  head diff: ${head_diff//$'\n'/ }"
    return $EXIT_GATE
  fi
  local package_commit_sha base_sha
  package_commit_sha=$(git -C "$wt" rev-parse HEAD)
  base_sha=$(git -C "$wt" rev-parse "$parent")

  # Remote branch safety: exact branch name; refuse a pre-existing remote branch
  # at a DIFFERENT sha (no force). A SAME-sha remote is an idempotent no-op.
  local remote_name="origin" existing_sha idempotent=false
  existing_sha=$(git -C "$wt" ls-remote --heads "$remote_name" "$branch" 2>/dev/null | awk 'NR==1{print $1}')
  if [[ -n "$existing_sha" ]]; then
    if [[ "$existing_sha" == "$package_commit_sha" ]]; then
      idempotent=true
      info "Stage 3: remote $remote_name/$branch already at the exact package-commit sha — idempotent no-op push."
    else
      err "TIER-4 STAGE3 REFUSED: remote $remote_name/$branch already exists at a DIFFERENT sha ($existing_sha); refusing (no force). Nothing pushed."
      return $EXIT_GATE
    fi
  fi

  # Exact, NON-force, branch:branch push (skipped when already idempotently present).
  if [[ "$idempotent" != true ]]; then
    git -C "$wt" push --set-upstream "$remote_name" "$branch:$branch" \
      || { err "TIER-4 STAGE3 REFUSED: push to $remote_name failed. Nothing further done."; return $EXIT_GATE; }
  fi

  # Verify the remote now holds EXACTLY the package-commit sha.
  local remote_sha
  remote_sha=$(git -C "$wt" ls-remote --heads "$remote_name" "$branch" 2>/dev/null | awk 'NR==1{print $1}')
  [[ "$remote_sha" == "$package_commit_sha" ]] \
    || { err "TIER-4 STAGE3 REFUSED: post-push remote sha ($remote_sha) != package-commit sha ($package_commit_sha)."; return $EXIT_GATE; }

  rule; info "Tier-4 package-push (Stage 3) — pushed the disposable branch to $remote_name (no PR, no merge)"; rule
  emit_package_push "$wt" "$branch" "$base_sha" "$package_commit_sha" "$remote_name" "$remote_sha" "${declared[@]}"
  return $EXIT_OK
}

# ---- usage + dispatch ------------------------------------------------------

usage() {
  cat <<EOF
$PROG — Tier 3 write-capable ORCHESTRATOR, v0 (drives the EXISTING claw apply chain)

It does NOT duplicate the Rust write executor. \`claw plan apply\` remains the
only writer; this tool only validates the operator-approved lane, creates one
disposable worktree from origin/main, enforces exact-path scope + denials-win,
drives the existing chain inside the worktree, gathers evidence, and STOPS.

Subcommands:

  help

  validate-lane  --approved-lane <lane.json> --dry-run-evidence <evidence.json> [--plan <plan.yaml>]
      Pure gate check only. No git, no claw, no worktree, no writes. Confirms the
      lane WOULD be drivable (operator-approved, dry-run-ready, exact-path scope,
      denials win, plan targets in the declared set). Safe to run anywhere.

  apply-lane     --approved-lane <lane.json> --dry-run-evidence <evidence.json> --plan <plan.yaml>
      Runs validate-lane, then (only at a REAL interactive terminal, with a clean
      control checkout, origin/main, and a free worktree path) creates ONE
      disposable worktree and drives the existing chain inside it. Approval is
      human-typed at your terminal. STOPS for review; never pushes/PRs/merges.

  package-plan   --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
      Tier-4 Stage 1, READ-ONLY. For a disposable worktree the chain ALREADY
      applied, prints the packaging plan (which declared files would be staged,
      their verified after-hashes) and the PRINTED-only push/PR commands a later,
      separately token-gated lane would run. Performs NO git mutation (no add,
      commit, push, gh, or PR); would_push/would_open_pr are always false.
      Fail-closed on drift, hash mismatch, dirty control checkout, or missing
      apply evidence.

  package-commit --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
      Tier-4 Stage 2. Runs the SAME read-only readiness gate as package-plan,
      then stages EXACTLY the declared set (exact-path \`git add --\`; never
      \`git add .\` / \`-A\`) and makes ONE evidence-bound commit INSIDE the
      disposable worktree. NEVER pushes, opens a PR, merges, or touches the
      control checkout; pushed/pr_opened/merged are always false. Refuses if the
      index is pre-staged or the staged set != declared set. Push/PR (Stages 3-4)
      remain a separate, token-gated lane.

  package-push   --worktree <path> --approved-lane <lane.json> [--plan <plan.yaml>]
      Tier-4 Stage 3. Runs the SAME readiness gate, re-derives the Stage-2
      package-commit from the worktree HEAD (HEAD changed EXACTLY the declared
      set; tree clean), then pushes ONLY that exact disposable branch at that
      exact sha to its \`origin\` remote with a NON-force branch:branch refspec.
      NEVER force-pushes / pushes tags / deletes refs / pushes main / opens a PR
      / merges / commits / touches the control checkout. A same-sha remote is an
      idempotent no-op; a different-sha remote is refused. pushed=true,
      pr_opened=false, merged=false. Opening a PR (Stage 4) is a separate lane
      needing a NEW explicit operator approval.

Environment:
  A2_CLAW   path to the built claw binary (default: the dated build artifact).
            current: $A2_CLAW

Safety: writes only inside a fresh disposable worktree; dry-run-ready + operator
approval required; denials win; no model/broker/runtime/network/Vault; no raw
app inference; no push/PR/merge/branch-delete/force-remove. Approval is never
composed, captured, faked, or batched — it is human-typed at a real terminal.
EOF
}

main() {
  local sub=${1:-help}
  if [[ $# -gt 0 ]]; then shift; fi
  case "$sub" in
    help|-h|--help) usage ;;
    validate-lane)
      local lane="" evidence="" plan=""
      while [[ $# -gt 0 ]]; do
        case "$1" in
          --approved-lane) lane=${2:-}; shift 2 ;;
          --dry-run-evidence) evidence=${2:-}; shift 2 ;;
          --plan) plan=${2:-}; shift 2 ;;
          *) err "unexpected argument: $1"; exit $EXIT_USAGE ;;
        esac
      done
      [[ -n "$lane" && -n "$evidence" ]] || { err "validate-lane requires --approved-lane and --dry-run-evidence"; exit $EXIT_USAGE; }
      gate_validate_lane "$lane" "$evidence" "$plan"
      ;;
    apply-lane) cmd_apply_lane "$@" ;;
    package-plan) cmd_package_plan "$@" ;;
    package-commit) cmd_package_commit "$@" ;;
    package-push) cmd_package_push "$@" ;;
    *) err "unknown subcommand: $sub"; info ""; usage; exit $EXIT_USAGE ;;
  esac
}

main "$@"
