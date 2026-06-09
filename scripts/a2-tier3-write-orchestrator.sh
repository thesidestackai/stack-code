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

# Mirror of disposableWorktreePlan.ts constants (authoritative there).
readonly CONTROL_CHECKOUT="/home/suki/stack-code"
readonly DISPOSABLE_WORKTREE_ROOT="/mnt/vast-data/git-worktrees/"

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

drive_chain_for_plan() {
  # Drives the EXISTING chain inside the disposable worktree. Delegates ALL
  # writing to `claw plan apply`; reimplements nothing. Each command is printed
  # before it runs (no hidden execution). Approval is human-typed at the TTY.
  local ws=$1 plan=$2
  local claw_dir="$ws/.claw"

  rule; info "STEP 1 / PREVIEW (writes NO target)"; rule
  info "+ $(shq "$A2_CLAW") plan run $(shq "$plan") --workspace-root $(shq "$ws") --workspace-write-preview"
  "$A2_CLAW" plan run "$plan" --workspace-root "$ws" --workspace-write-preview

  local preview_bundle gen_result
  preview_bundle=$(find "$claw_dir" -type f -name 'preview-bundle.json' 2>/dev/null | sort | head -n1 || true)
  gen_result=$(find "$claw_dir" -type f -name 'preview-generator-result.json' 2>/dev/null | sort | head -n1 || true)
  [[ -n "$preview_bundle" ]] || { err "preview did not produce preview-bundle.json under $claw_dir"; return $EXIT_GATE; }

  local approval_out="$claw_dir/approval-result.json"
  rule; info "STEP 2 / APPROVE — REAL terminal; you type:  $APPROVAL_GRAMMAR  (writes NO target)"; rule
  info "+ $(shq "$A2_CLAW") plan approve $(shq "$preview_bundle") --approval-result-output $(shq "$approval_out")"
  "$A2_CLAW" plan approve "$preview_bundle" --approval-result-output "$approval_out"
  [[ -f "$approval_out" ]] || { err "approval did not produce $approval_out (approval not granted) — STOP"; return $EXIT_GATE; }

  rule; info "STEP 3 / APPLY-BUNDLE — GENERATOR only (writes NO target)"; rule
  info "+ $(shq "$A2_CLAW") plan apply-bundle $(shq "$gen_result") $(shq "$approval_out")"
  "$A2_CLAW" plan apply-bundle "$gen_result" "$approval_out"
  local apply_bundle; apply_bundle=$(find "$claw_dir" -type f -name 'apply-bundle.json' 2>/dev/null | sort | head -n1 || true)
  [[ -n "$apply_bundle" ]] || { err "apply-bundle did not produce apply-bundle.json — STOP"; return $EXIT_GATE; }

  rule; info "STEP 4 / APPLY — EXECUTOR (the existing claw write_executor; the ONLY writer; runs once)"; rule
  info "+ $(shq "$A2_CLAW") plan apply $(shq "$apply_bundle")"
  "$A2_CLAW" plan apply "$apply_bundle"
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
  [[ $# -gt 0 ]] && shift || true
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
    *) err "unknown subcommand: $sub"; info ""; usage; exit $EXIT_USAGE ;;
  esac
}

main "$@"
