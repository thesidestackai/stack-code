#!/usr/bin/env bash
# a2-ide-harness.sh — IDE-adjacent A2-L2b harness, v1.
#
# Purpose: give a non-terminal-first operator a safe, visual way to drive the
# proven A2-L2b chain from VS Code / Cursor / Claude Code / Codex-local without
# weakening any safety gate.
#
# Source of truth (merged on main):
#   docs/a2-l4-ide-harness-workflow-scope.md
#   docs/stack-code-n6a-helper-exec-allowlist-design.md   (N6A execution subcommands)
#
# SAFETY (hard invariants this script preserves):
#   - Preview does NOT write target.
#   - package-plan calls claw plan run --workspace-write-preview (preview phase ONLY).
#     It writes the .claw/ preview bundle, NOT the target file. claw plan apply is
#     never called by this script in any mode.
#   - package-commit uses exact-path staging only (git add -- <declared files>).
#     git add . and git add -A are unconditionally forbidden.
#   - package-push is non-force only (git push <remote> <branch>).
#     --force, --force-with-lease, --force-if-includes, and --delete are forbidden.
#   - package-pr opens a DRAFT PR only (gh pr create --draft).
#     --ready, --approve, --merge, --fill, and --auto-merge are forbidden.
#   - Approval does NOT write target; it requires a REAL interactive terminal.
#   - apply-bundle is the GENERATOR; it writes NO target.
#   - `claw plan apply` is the EXECUTOR; it is the ONLY command that writes the target.
#     It is NOT called by this script.
#   - No auto-approval, no hidden apply, no batch/--yes/fake-TTY.
#   - This script calls NO model / NO broker / NO runtime directly.
#     package-plan calls claw, which routes through the SideStack broker internally.
#     This script never references :11434 or any raw Ollama endpoint.
#   - Merge is human-only; no merge subcommand exists in this script.
#
# Bounded exec exceptions:
#   print-tier3-evidence: invokes a2-evidence-collector (read-only, writes nothing,
#     no claw/orchestrator/model/broker/runtime) by exact basename, array-argv, no shell.
#   package-plan:         invokes claw plan run --workspace-write-preview --wrapper by
#     absolute paths (--claw-binary + derived workspace wrapper), array-argv, no shell.
#     --wrapper is derived from the workspace root so the process CWD is irrelevant.
#     Preview only; no target write.
#   package-commit:       invokes git add -- <declared files> + git commit. Exact-path
#     staging only; no git add . or git add -A.
#   package-push:         invokes git push <remote> <branch>. Non-force only.
#   package-pr:           invokes gh pr create --draft. Draft only.

set -euo pipefail

# ---- constants -------------------------------------------------------------

# Canonical local Claw entrypoint. This symlink is maintained by the controlled
# Stack-Code build/deployment lane and avoids silently selecting stale debug
# artifacts. Override explicitly with A2_CLAW=/absolute/path/to/claw. The path
# may contain spaces; always quote it when printing the command for the
# operator. Fails closed (via `set -u`) rather than falling back to a debug
# artifact if $HOME is unset.
DEFAULT_CLAW="${HOME}/.local/bin/claw"
A2_CLAW="${A2_CLAW:-$DEFAULT_CLAW}"

# Default built read-only evidence collector (override with
# A2_EVIDENCE_COLLECTOR=/path/to/a2-evidence-collector). Used ONLY by
# print-tier3-evidence. The collector is read-only: it writes nothing and runs
# no claw/orchestrator/model/broker/runtime. Its basename must be exactly
# a2-evidence-collector or print-tier3-evidence refuses to invoke it.
DEFAULT_EVIDENCE_COLLECTOR="/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/a2-evidence-collector"
A2_EVIDENCE_COLLECTOR="${A2_EVIDENCE_COLLECTOR:-$DEFAULT_EVIDENCE_COLLECTOR}"
EVIDENCE_COLLECTOR_BASENAME="a2-evidence-collector"

# Approval grammar is fixed by the CLI source (a2-plan-runner/src/approval.rs).
APPROVAL_GRAMMAR='apply <step-id> <preview_sha256>'

PROG="a2-ide-harness.sh"

# Exit codes
EXIT_OK=0
EXIT_USAGE=2
EXIT_VALIDATION=3

# ---- output helpers --------------------------------------------------------

info()  { printf '%s\n' "$*"; }
warn()  { printf '%s: WARNING: %s\n' "$PROG" "$*" >&2; }
err()   { printf '%s: ERROR: %s\n' "$PROG" "$*" >&2; }
rule()  { printf -- '----------------------------------------------------------------\n'; }

# Print a shell-safe single-quoted form of an argument (so operators can paste
# the printed command verbatim even when paths contain spaces).
shq() {
  local s=${1//\'/\'\\\'\'}
  printf "'%s'" "$s"
}

# Warn (do not fail) if a path looks like a runtime/service/secret surface that
# this docs/IDE harness must never touch.
warn_if_sensitive_path() {
  local label=$1 p=$2
  case "$p" in
    *vault*|*secret*|*secrets*|*.env|*/run/*|*/etc/*|*systemd*|*.service)
      warn "$label path looks like a runtime/service/secret surface: $p"
      warn "the A2 harness must only target reviewed workspace files, never runtime/secret paths."
      ;;
  esac
}

# Resolve a --plan value against --workspace: an absolute plan is returned
# unchanged; a relative plan is anchored to the workspace root rather than the
# process CWD (the panel's extension host CWD is not guaranteed to be the
# workspace — the same problem the wrapper derivation below already solves).
resolve_plan_for_workspace() {
  local workspace=$1 supplied_plan=$2
  if [[ "$supplied_plan" == /* ]]; then
    printf '%s' "$supplied_plan"
  else
    printf '%s' "$workspace/$supplied_plan"
  fi
}

# Read-only sha256 of a file, or a placeholder if absent/unreadable.
sha_or_note() {
  local p=$1
  if [[ -f "$p" ]] && command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$p" | awk '{print $1}'
  else
    printf '(unavailable — file missing or sha256sum not found)'
  fi
}

# Echo the first matching artifact path under .claw (read-only), or empty.
_first_artifact() {
  local claw_dir=$1 name=$2
  find "$claw_dir" -type f -name "$name" 2>/dev/null | sort | head -n1
}

# Detect the chain state from .claw ARTIFACTS (not free-text logs). This is the
# reliable, artifact-based evidence the smoke false-positive note calls for: an
# apply-result.json is written by the executor, so its presence (and the
# a2-l2b-write-applied marker inside that artifact) is real evidence of an apply.
# Echoes exactly one of:
#   not-started | preview-ready | approval-ready | apply-bundle-ready | applied | unknown
detect_chain_state() {
  local ws=$1
  local claw_dir="$ws/.claw"
  if [[ ! -d "$claw_dir" ]]; then
    printf 'not-started'
    return 0
  fi
  local apply_result apply_bundle approval_result preview_bundle
  apply_result=$(_first_artifact "$claw_dir" 'apply-result.json')
  apply_bundle=$(_first_artifact "$claw_dir" 'apply-bundle.json')
  approval_result=$(_first_artifact "$claw_dir" 'approval-result.json')
  preview_bundle=$(_first_artifact "$claw_dir" 'preview-bundle.json')

  # An apply-result artifact (written by the executor) is the strongest signal.
  if [[ -n "$apply_result" ]]; then printf 'applied'; return 0; fi
  if [[ -n "$apply_bundle" ]]; then printf 'apply-bundle-ready'; return 0; fi
  if [[ -n "$approval_result" ]]; then printf 'approval-ready'; return 0; fi
  if [[ -n "$preview_bundle" ]]; then printf 'preview-ready'; return 0; fi
  printf 'unknown'
}

# Print the operator's next-step hint for a chain state. Read-only guidance; it
# never executes claw and never decides state from free-text logs.
print_next_step_hint() {
  local state=$1
  info ""
  info "## next-step hint (state: $state)"
  case "$state" in
    not-started)        info "  No .claw yet. Next: print-preview, then run the preview command yourself." ;;
    preview-ready)      info "  Preview present, approval-result missing. Next: print-approval (REAL terminal required)." ;;
    approval-ready)     info "  Approval-result present, apply-bundle missing. Next: print-apply-bundle (GENERATOR; writes no target)." ;;
    apply-bundle-ready) info "  Apply-bundle present, no apply-result yet. Next: print-apply (the target-writing EXECUTOR; runs once)." ;;
    applied)            info "  Chain appears applied (apply-result present). Next: verify-final with the expected after_sha256." ;;
    *)                  info "  .claw exists but no recognized artifacts found. Re-check the workspace, or print-preview to start." ;;
  esac
}

# ---- usage -----------------------------------------------------------------

usage() {
  cat <<EOF
$PROG — IDE-adjacent A2-L2b harness, v0 (PRINT / VALIDATE ONLY)

This helper NEVER runs an A2 command. It validates paths, locates .claw
artifacts, shows hashes, and PRINTS the exact command for you to run manually.

The proven A2-L2b chain (run each command yourself):

  1. PREVIEW   claw plan run <plan.yaml> --workspace-root <ws> --workspace-write-preview
               -> produces the preview bundle + preview_sha256. Writes NO target.
  2. APPROVE   claw plan approve <preview-bundle.json> --approval-result-output <out.json>
               -> REAL-TTY human approval. Persists approval-result. Writes NO target.
               -> at the prompt you type:  $APPROVAL_GRAMMAR
  3. BUNDLE    claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
               -> GENERATOR. Assembles apply-bundle.json. Writes NO target.
  4. APPLY     claw plan apply <apply-bundle.json>
               -> EXECUTOR. The ONLY command that writes the target. Runs once.

Subcommands (all read-only / print-only):

  help
  validate-input    --workspace <path> --plan <path>
  print-preview     --workspace <path> --plan <path>
  find-artifacts    --workspace <path>                 (lists .claw artifacts + a next-step hint)
  print-approval    --workspace <path> --preview-bundle <path> --approval-output <path>
  print-apply-bundle --preview-generator-result <path> --approval-result <path>
  print-apply       --apply-bundle <path>
  verify-final      --workspace <path> --target <path> --after-sha <sha>
  audit-workspace   --workspace <path> [--target <path> --after-sha <sha>]
                    (read-only chain-state audit from .claw ARTIFACTS + optional target hash check)
  print-tier3-evidence --workspace <path>
                    (Option B refresh: prints the existing a2-tier3-evidence-snapshot.v0 by
                     running the READ-ONLY, writes-nothing, non-claw a2-evidence-collector;
                     stdout is JSON only. Override the binary with A2_EVIDENCE_COLLECTOR.)

Detection note: chain state and "applied" evidence come from .claw ARTIFACTS
(apply-result.json, apply-bundle.json, approval-result.json, preview-bundle.json)
and the target HASH — never from grepping free-text logs. Marker names such as
a2-l2b-write-applied are printed as human guidance, not treated as evidence.

Environment:
  A2_CLAW   path to the built claw binary (default: the dated build artifact).
            current: $A2_CLAW

Safety: no auto-approval, no hidden apply, no batch/--yes/fake-TTY. Approval
must happen at a REAL terminal. This script makes NO model/broker/runtime call.
EOF
}

# ---- tiny arg parser -------------------------------------------------------
# Usage: parse_opts "$@"; then read OPT_<name> for each --name <value> seen.
declare -A OPT
parse_opts() {
  OPT=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --*)
        local key=${1#--}
        if [[ $# -lt 2 ]]; then err "missing value for --$key"; exit $EXIT_USAGE; fi
        OPT["$key"]=$2
        shift 2
        ;;
      *)
        err "unexpected argument: $1"
        exit $EXIT_USAGE
        ;;
    esac
  done
}

require_opt() {
  local name=$1
  if [[ -z "${OPT[$name]:-}" ]]; then
    err "required option missing: --$name"
    exit $EXIT_USAGE
  fi
}

# ---- subcommands -----------------------------------------------------------

cmd_validate_input() {
  parse_opts "$@"
  require_opt workspace
  require_opt plan
  local ws="${OPT[workspace]}" supplied_plan="${OPT[plan]}"
  local plan
  plan=$(resolve_plan_for_workspace "$ws" "$supplied_plan")
  local rc=$EXIT_OK

  rule; info "A2 validate-input (read-only)"; rule
  info "workspace     : $ws"
  info "plan (supplied): $supplied_plan"
  info "plan (resolved): $plan"

  if [[ ! -d "$ws" ]]; then err "workspace is not a directory: $ws"; rc=$EXIT_VALIDATION; fi
  if [[ ! -f "$plan" ]]; then err "plan.yaml not found: $plan"; rc=$EXIT_VALIDATION; fi

  warn_if_sensitive_path "workspace" "$ws"

  # Inspect plan.yaml for after_file references; refuse absolute after_file.
  if [[ -f "$plan" ]]; then
    local after_lines
    after_lines=$(grep -nE '^\s*after_file\s*:' "$plan" || true)
    if [[ -n "$after_lines" ]]; then
      info "after_file references found in plan:"
      printf '%s\n' "$after_lines"
      # Extract values and check for absolute paths.
      while IFS= read -r line; do
        local val
        val=$(printf '%s' "$line" | sed -E 's/^[0-9]+:\s*after_file\s*:\s*//; s/^["'\'']//; s/["'\'']\s*$//')
        if [[ "$val" == /* ]]; then
          err "absolute after_file path is not allowed: $val"
          err "after_file must be a reviewed path relative to the workspace."
          rc=$EXIT_VALIDATION
        fi
        warn_if_sensitive_path "after_file" "$val"
      done <<<"$after_lines"
    else
      info "no after_file: field found in plan (informational)."
    fi
  fi

  rule
  if [[ $rc -eq $EXIT_OK ]]; then
    info "validate-input: OK (next step: print-preview)"
  else
    err "validate-input: FAILED — fix the issues above before proceeding."
  fi
  return $rc
}

cmd_print_preview() {
  parse_opts "$@"
  require_opt workspace
  require_opt plan
  local ws=${OPT[workspace]} plan=${OPT[plan]}

  rule; info "STEP 1 / PREVIEW — produces preview bundle + preview_sha256. Writes NO target."; rule
  [[ -d "$ws" ]]   || warn "workspace does not exist yet: $ws"
  [[ -f "$plan" ]] || warn "plan.yaml does not exist yet: $plan"
  info "Run this yourself:"
  info ""
  info "  $(shq "$A2_CLAW") plan run $(shq "$plan") --workspace-root $(shq "$ws") --workspace-write-preview"
  info ""
  info "Then locate the artifacts with:  $PROG find-artifacts --workspace $(shq "$ws")"
  return $EXIT_OK
}

cmd_find_artifacts() {
  parse_opts "$@"
  require_opt workspace
  local ws=${OPT[workspace]}
  local claw_dir="$ws/.claw"

  rule; info "A2 find-artifacts (read-only) under: $claw_dir"; rule
  if [[ ! -d "$claw_dir" ]]; then
    warn "no .claw directory found yet at: $claw_dir"
    warn "run STEP 1 (print-preview) first, then re-run find-artifacts."
    print_next_step_hint "$(detect_chain_state "$ws")"
    return $EXIT_OK
  fi

  local names=(preview-bundle.json preview-generator-result.json approval-result.json apply-bundle.json)
  local n
  for n in "${names[@]}"; do
    info ""
    info "## $n"
    local found
    found=$(find "$claw_dir" -type f -name "$n" 2>/dev/null | sort || true)
    if [[ -z "$found" ]]; then
      info "  (none found)"
    else
      while IFS= read -r f; do
        info "  path : $f"
        info "  sha  : $(sha_or_note "$f")"
      done <<<"$found"
    fi
  done

  info ""
  info "## checkpoints (rollback baselines) and payloads (read-only):"
  find "$claw_dir" -type d \( -name 'l2b-checkpoints' -o -name 'l2b-payloads' \) 2>/dev/null | sort | sed 's/^/  /' || true

  print_next_step_hint "$(detect_chain_state "$ws")"
  return $EXIT_OK
}

cmd_print_approval() {
  parse_opts "$@"
  require_opt workspace
  require_opt preview-bundle
  require_opt approval-output
  local ws=${OPT[workspace]} pb=${OPT[preview-bundle]} out=${OPT[approval-output]}

  rule; info "STEP 2 / APPROVE — REAL terminal required. Persists approval-result. Writes NO target."; rule
  [[ -f "$pb" ]] && info "preview-bundle : $pb" || warn "preview-bundle not found yet: $pb"
  [[ -f "$pb" ]] && info "preview sha    : $(sha_or_note "$pb")"
  if [[ -e "$out" ]]; then
    warn "approval-output already exists: $out"
    warn "the approve command refuses to overwrite an existing approval-result path; choose a new path."
  fi
  info ""
  info "Run this yourself, AT A REAL INTERACTIVE TERMINAL (not inside a command runner):"
  info ""
  info "  $(shq "$A2_CLAW") plan approve $(shq "$pb") --approval-result-output $(shq "$out")"
  info ""
  info "At the approval prompt, type the exact line (no --yes, no batch, no auto):"
  info "  $APPROVAL_GRAMMAR"
  info ""
  info "Note: a non-interactive runner will fail-closed (exit 7) — that is the TTY guard, not a bug."
  return $EXIT_OK
}

cmd_print_apply_bundle() {
  parse_opts "$@"
  require_opt preview-generator-result
  require_opt approval-result
  local gen=${OPT[preview-generator-result]} appr=${OPT[approval-result]}

  rule; info "STEP 3 / APPLY-BUNDLE — GENERATOR only. Assembles apply-bundle.json. Writes NO target."; rule
  [[ -f "$gen" ]]  && info "generator-result : $gen"  || warn "preview-generator-result not found yet: $gen"
  [[ -f "$appr" ]] && info "approval-result  : $appr" || warn "approval-result not found yet: $appr"
  info ""
  info "Run this yourself (it does NOT write the target):"
  info ""
  info "  $(shq "$A2_CLAW") plan apply-bundle $(shq "$gen") $(shq "$appr")"
  info ""
  info "This produces apply-bundle.json next to the preview bundle. The TARGET is written only in STEP 4."
  return $EXIT_OK
}

cmd_print_apply() {
  parse_opts "$@"
  require_opt apply-bundle
  local ab=${OPT[apply-bundle]}

  rule; info "STEP 4 / APPLY — EXECUTOR. The ONLY command that writes the target. Runs ONCE."; rule
  [[ -f "$ab" ]] && info "apply-bundle : $ab" || warn "apply-bundle not found yet: $ab"
  [[ -f "$ab" ]] && info "bundle sha   : $(sha_or_note "$ab")"
  info ""
  info "Run this yourself ONCE, only after you reviewed the diff and approved:"
  info ""
  info "  $(shq "$A2_CLAW") plan apply $(shq "$ab")"
  info ""
  info "Do not run apply twice for the same approved preview. A second run for an already-applied"
  info "preview is a STOP condition — start a new proof chain instead."
  return $EXIT_OK
}

cmd_verify_final() {
  parse_opts "$@"
  require_opt workspace
  require_opt target
  require_opt after-sha
  local ws=${OPT[workspace]} target=${OPT[target]} after=${OPT[after-sha]}
  local rc=$EXIT_OK

  rule; info "STEP 5 / VERIFY-FINAL (read-only) — confirm the target landed at after_sha256."; rule
  info "workspace : $ws"
  info "target    : $target"
  info "expected  : $after"

  if [[ ! -f "$target" ]]; then
    err "target file not found: $target"
    return $EXIT_VALIDATION
  fi
  local actual
  actual=$(sha_or_note "$target")
  info "actual    : $actual"
  if [[ "$actual" == "$after" ]]; then
    info "MATCH — target is at the expected after_sha256."
  else
    err "MISMATCH — target hash does not equal the expected after_sha256."
    rc=$EXIT_VALIDATION
  fi

  info ""
  info "Apply-result evidence to look for in the apply output / log:"
  info "  schema  : a2-l2b-apply-result.v1   outcome: applied"
  info "  markers : a2-l2b-write-preflight-ok, a2-l2b-write-temp-created,"
  info "            a2-l2b-write-applied, a2-l2b-write-validated"
  return $rc
}

cmd_audit_workspace() {
  parse_opts "$@"
  require_opt workspace
  local ws=${OPT[workspace]}
  local target=${OPT[target]:-}
  local after=${OPT[after-sha]:-}
  local rc=$EXIT_OK
  local claw_dir="$ws/.claw"

  rule; info "A2 audit-workspace (read-only; artifact/hash-based) — workspace: $ws"; rule

  local state
  state=$(detect_chain_state "$ws")
  info "chain state: $state"

  # Artifact presence map (real .claw files — this IS the evidence, not free-text logs).
  if [[ -d "$claw_dir" ]]; then
    local n found
    for n in preview-bundle.json preview-generator-result.json approval-result.json apply-bundle.json apply-result.json; do
      found=$(_first_artifact "$claw_dir" "$n")
      if [[ -n "$found" ]]; then
        info "  present : $n  ($found)"
      else
        info "  absent  : $n"
      fi
    done
  else
    info "  no .claw directory under $ws"
  fi

  print_next_step_hint "$state"

  # Optional read-only target hash check. Both flags are required together.
  if [[ -n "$target" || -n "$after" ]]; then
    info ""
    info "## target hash check"
    if [[ -z "$target" || -z "$after" ]]; then
      err "both --target and --after-sha are required together for the hash check."
      rc=$EXIT_VALIDATION
    elif [[ ! -f "$target" ]]; then
      err "target file not found: $target"
      rc=$EXIT_VALIDATION
    else
      local actual
      actual=$(sha_or_note "$target")
      info "  target   : $target"
      info "  expected : $after"
      info "  actual   : $actual"
      if [[ "$actual" == "$after" ]]; then
        info "  MATCH — target is at the expected after_sha256."
      else
        err "MISMATCH — target hash does not equal the expected after_sha256."
        rc=$EXIT_VALIDATION
      fi
    fi
  fi

  rule
  info "Note: this audit inspects .claw ARTIFACTS and target HASH only — never free-text logs, and it"
  info "never executes claw. Marker names printed as guidance are NOT treated as execution evidence."
  return $rc
}

# print-tier3-evidence — Option B read-only refresh. Prints the EXISTING Tier 3
# evidence snapshot (a2-tier3-evidence-snapshot.v0) by running the read-only,
# writes-nothing, non-claw a2-evidence-collector. stdout carries ONLY the
# collector's JSON (so the panel parser can consume it); all diagnostics go to
# stderr. This creates no worktree, writes no file, and runs no claw / model /
# broker / runtime. Fail-closed: any guard failure prints nothing to stdout and
# returns non-zero, so the panel renders its fail-closed notice.
cmd_print_tier3_evidence() {
  parse_opts "$@"
  require_opt workspace
  local ws=${OPT[workspace]}
  local collector=$A2_EVIDENCE_COLLECTOR

  warn_if_sensitive_path "workspace" "$ws"

  # Basename guard: only the evidence collector may be invoked here (mirrors the
  # panel-side HELPER_BASENAME bound; refuses any other binary, incl. claw).
  if [[ "$(basename -- "$collector")" != "$EVIDENCE_COLLECTOR_BASENAME" ]]; then
    err "refused: evidence collector basename must be exactly $EVIDENCE_COLLECTOR_BASENAME: $collector"
    return $EXIT_VALIDATION
  fi
  if [[ ! -d "$ws" ]]; then
    err "workspace is not a directory: $ws"
    return $EXIT_VALIDATION
  fi
  if [[ ! -x "$collector" ]]; then
    err "evidence collector not found or not executable: $collector"
    err "build it (cargo build -p a2-evidence-collector) or set A2_EVIDENCE_COLLECTOR."
    return $EXIT_VALIDATION
  fi

  # Run the collector READ-ONLY with array argv and NO shell. It emits one
  # a2-tier3-evidence-snapshot.v0 JSON object to stdout and writes nothing. We
  # pass it through verbatim; we never execute claw and make no runtime call.
  "$collector" "$ws"
}

# ---- N6A controlled-execution subcommands ----------------------------------
# These subcommands execute bounded operations (claw preview, git, gh). They
# are gated at the panel level by N6 runtime sub-tokens. See §13-17 of
# docs/stack-code-n6a-helper-exec-allowlist-design.md for the full boundary.

cmd_package_plan() {
  parse_opts "$@"
  require_opt workspace
  require_opt plan
  require_opt claw-binary
  local ws="${OPT[workspace]}" supplied_plan="${OPT[plan]}" claw_bin="${OPT[claw-binary]}"
  local plan
  plan=$(resolve_plan_for_workspace "$ws" "$supplied_plan")

  warn_if_sensitive_path "workspace" "$ws"
  warn_if_sensitive_path "plan" "$plan"

  [[ -d "$ws" ]]       || { err "workspace is not a directory: $ws"; exit $EXIT_VALIDATION; }
  [[ -f "$plan" ]]     || { err "plan file not found: $plan"; exit $EXIT_VALIDATION; }
  [[ "$claw_bin" == /* ]] || { err "claw-binary must be an absolute path: $claw_bin"; exit $EXIT_VALIDATION; }
  [[ -f "$claw_bin" ]] || { err "claw binary not found: $claw_bin"; exit $EXIT_VALIDATION; }
  [[ -x "$claw_bin" ]] || { err "claw binary not executable: $claw_bin"; exit $EXIT_VALIDATION; }

  # Derive the step-executor wrapper from the workspace so claw plan run does not
  # depend on process CWD to locate scripts/claw-sidestack-local. When the panel
  # spawns this script, its CWD is whatever VS Code inherited from the terminal —
  # not necessarily the workspace root. Passing --wrapper with an absolute path
  # makes the step executor find the broker profile regardless of CWD.
  local wrapper="$ws/scripts/claw-sidestack-local"
  [[ -f "$wrapper" ]] || { err "workspace wrapper not found: $wrapper"; exit $EXIT_VALIDATION; }
  [[ -x "$wrapper" ]] || { err "workspace wrapper not executable: $wrapper"; exit $EXIT_VALIDATION; }

  # Execute claw plan run (preview only; writes .claw/ preview bundle, NOT the target)
  "$claw_bin" plan run "$plan" --workspace-root "$ws" --workspace-write-preview \
    --wrapper "$wrapper"
}

cmd_package_commit() {
  # --file is a repeatable flag and is incompatible with the single-value OPT[]
  # parser. Parse manually to accumulate all --file values into FILES array.
  local ws="" msg=""
  local -a FILES=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --workspace)
        [[ $# -ge 2 ]] || { err "missing value for --workspace"; exit $EXIT_USAGE; }
        ws="$2"; shift 2 ;;
      --file)
        [[ $# -ge 2 ]] || { err "missing value for --file"; exit $EXIT_USAGE; }
        FILES+=("$2"); shift 2 ;;
      --message)
        [[ $# -ge 2 ]] || { err "missing value for --message"; exit $EXIT_USAGE; }
        msg="$2"; shift 2 ;;
      --*)
        err "unknown flag for package-commit: $1"; exit $EXIT_USAGE ;;
      *)
        err "unexpected argument for package-commit: $1"; exit $EXIT_USAGE ;;
    esac
  done

  [[ -n "$ws" ]]             || { err "required option missing: --workspace"; exit $EXIT_USAGE; }
  [[ "${#FILES[@]}" -gt 0 ]] || { err "required option missing: --file (supply at least one)"; exit $EXIT_USAGE; }
  [[ -n "$msg" ]]            || { err "required option missing: --message"; exit $EXIT_USAGE; }

  warn_if_sensitive_path "workspace" "$ws"

  git -C "$ws" rev-parse --git-dir >/dev/null 2>&1 \
    || { err "workspace is not a git repository: $ws"; exit $EXIT_VALIDATION; }

  # Commit message: single-line only (guard against newline injection)
  if [[ "$msg" == *$'\n'* ]]; then
    err "commit message must be a single line (no embedded newlines)"; exit $EXIT_VALIDATION
  fi

  # Validate each declared file: relative path, no leading /, no .. escape
  local f
  for f in "${FILES[@]}"; do
    [[ -n "$f" ]] || { err "empty --file value"; exit $EXIT_VALIDATION; }
    [[ "$f" != /* ]] || { err "--file must be a relative path, not absolute: $f"; exit $EXIT_VALIDATION; }
    case "$f" in ../*)
      err "--file must not traverse above the workspace root: $f"; exit $EXIT_VALIDATION ;;
    esac
    warn_if_sensitive_path "file" "$f"
  done

  # Exact-path staging only: git add -- <file1> <file2> ...
  git -C "$ws" add -- "${FILES[@]}"

  # Commit with operator-supplied single-line message
  git -C "$ws" commit -m "$msg"
}

cmd_package_push() {
  parse_opts "$@"
  require_opt workspace
  require_opt remote
  require_opt branch
  local ws="${OPT[workspace]}" remote="${OPT[remote]}" branch="${OPT[branch]}"

  warn_if_sensitive_path "workspace" "$ws"

  [[ -d "$ws" ]] || { err "workspace is not a directory: $ws"; exit $EXIT_VALIDATION; }

  # Safe pattern: remote name must not contain spaces or shell-special characters
  if ! [[ "$remote" =~ ^[a-zA-Z0-9_.-]+$ ]]; then
    err "remote name contains unsafe characters: $remote"; exit $EXIT_VALIDATION
  fi

  # Safe pattern: branch name (allows / for namespaced branches)
  if ! [[ "$branch" =~ ^[a-zA-Z0-9/_.-]+$ ]]; then
    err "branch name contains unsafe characters: $branch"; exit $EXIT_VALIDATION
  fi

  git -C "$ws" rev-parse --git-dir >/dev/null 2>&1 \
    || { err "workspace is not a git repository: $ws"; exit $EXIT_VALIDATION; }

  # Confirm the local branch matches the declared branch
  local current_branch
  current_branch="$(git -C "$ws" branch --show-current 2>/dev/null || true)"
  if [[ "$current_branch" != "$branch" ]]; then
    err "local branch ($current_branch) does not match declared --branch ($branch)"
    err "refusing to push an unexpected branch."
    exit $EXIT_VALIDATION
  fi

  # Non-force push only: git push <remote> <branch>
  git -C "$ws" push "$remote" "$branch"
}

cmd_package_pr() {
  parse_opts "$@"
  require_opt workspace
  require_opt base
  require_opt head
  require_opt title
  require_opt body-file
  local ws="${OPT[workspace]}" base="${OPT[base]}" head_branch="${OPT[head]}"
  local title="${OPT[title]}" body_file="${OPT[body-file]}"

  warn_if_sensitive_path "workspace" "$ws"

  [[ -d "$ws" ]] || { err "workspace is not a directory: $ws"; exit $EXIT_VALIDATION; }

  # Safe branch name patterns
  if ! [[ "$base" =~ ^[a-zA-Z0-9/_.-]+$ ]]; then
    err "base branch name contains unsafe characters: $base"; exit $EXIT_VALIDATION
  fi
  if ! [[ "$head_branch" =~ ^[a-zA-Z0-9/_.-]+$ ]]; then
    err "head branch name contains unsafe characters: $head_branch"; exit $EXIT_VALIDATION
  fi

  # Title: non-empty, single-line, max 256 chars
  [[ -n "$title" ]] || { err "title must not be empty"; exit $EXIT_VALIDATION; }
  if [[ "$title" == *$'\n'* ]]; then
    err "title must be a single line (no newlines)"; exit $EXIT_VALIDATION
  fi
  if [[ ${#title} -gt 256 ]]; then
    err "title exceeds 256 characters (${#title} chars)"; exit $EXIT_VALIDATION
  fi

  [[ -f "$body_file" ]] || { err "body-file not found: $body_file"; exit $EXIT_VALIDATION; }

  # Open DRAFT PR only. No --ready, no --approve, no --merge.
  gh pr create \
    --draft \
    --base "$base" \
    --head "$head_branch" \
    --title "$title" \
    --body-file "$body_file"
}

# ---- dispatch --------------------------------------------------------------

main() {
  local sub=${1:-help}
  [[ $# -gt 0 ]] && shift || true
  case "$sub" in
    help|-h|--help)        usage ;;
    validate-input)        cmd_validate_input "$@" ;;
    print-preview)         cmd_print_preview "$@" ;;
    find-artifacts)        cmd_find_artifacts "$@" ;;
    print-approval)        cmd_print_approval "$@" ;;
    print-apply-bundle)    cmd_print_apply_bundle "$@" ;;
    print-apply)           cmd_print_apply "$@" ;;
    verify-final)          cmd_verify_final "$@" ;;
    audit-workspace)       cmd_audit_workspace "$@" ;;
    print-tier3-evidence)  cmd_print_tier3_evidence "$@" ;;
    package-plan)          cmd_package_plan "$@" ;;
    package-commit)        cmd_package_commit "$@" ;;
    package-push)          cmd_package_push "$@" ;;
    package-pr)            cmd_package_pr "$@" ;;
    *)
      err "unknown subcommand: $sub"
      info ""
      usage
      exit $EXIT_USAGE
      ;;
  esac
}

main "$@"
