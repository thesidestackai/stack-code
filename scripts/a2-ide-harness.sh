#!/usr/bin/env bash
# a2-ide-harness.sh — IDE-adjacent A2-L2b harness, v0 (print/validate ONLY).
#
# Purpose: give a non-terminal-first operator a safe, visual way to drive the
# proven A2-L2b chain from VS Code / Cursor / Claude Code / Codex-local without
# weakening any safety gate. This script NEVER runs an A2 command. It only:
#   - validates paths read-only
#   - locates .claw artifacts read-only
#   - shows hashes read-only
#   - PRINTS the exact command the operator must run manually
#
# Source of truth (merged on main):
#   docs/a2-l4-ide-harness-workflow-scope.md
#   handoffs/a2_ide_harness_workflow_implementation_prompt_DRAFT_2026-06-07.md
#
# SAFETY (hard invariants this script preserves):
#   - Preview does NOT write target.
#   - Approval does NOT write target; it requires a REAL interactive terminal.
#   - apply-bundle is the GENERATOR; it writes NO target.
#   - `claw plan apply` is the EXECUTOR; it is the ONLY command that writes the target.
#   - No auto-approval, no hidden apply, no batch/--yes/fake-TTY.
#   - This script calls NO model / NO broker / NO runtime; it never executes `claw`.
#
# This v0 is print/validate only by design. It has no exec mode on purpose.

set -euo pipefail

# ---- constants -------------------------------------------------------------

# Default built claw binary (override with A2_CLAW=/path/to/claw). The path may
# contain spaces; always quote it when printing the command for the operator.
DEFAULT_CLAW="/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw"
A2_CLAW="${A2_CLAW:-$DEFAULT_CLAW}"

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

# Read-only sha256 of a file, or a placeholder if absent/unreadable.
sha_or_note() {
  local p=$1
  if [[ -f "$p" ]] && command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$p" | awk '{print $1}'
  else
    printf '(unavailable — file missing or sha256sum not found)'
  fi
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
  find-artifacts    --workspace <path>
  print-approval    --workspace <path> --preview-bundle <path> --approval-output <path>
  print-apply-bundle --preview-generator-result <path> --approval-result <path>
  print-apply       --apply-bundle <path>
  verify-final      --workspace <path> --target <path> --after-sha <sha>

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
  local ws=${OPT[workspace]} plan=${OPT[plan]}
  local rc=$EXIT_OK

  rule; info "A2 validate-input (read-only)"; rule
  info "workspace : $ws"
  info "plan      : $plan"

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

# ---- dispatch --------------------------------------------------------------

main() {
  local sub=${1:-help}
  [[ $# -gt 0 ]] && shift || true
  case "$sub" in
    help|-h|--help)      usage ;;
    validate-input)      cmd_validate_input "$@" ;;
    print-preview)       cmd_print_preview "$@" ;;
    find-artifacts)      cmd_find_artifacts "$@" ;;
    print-approval)      cmd_print_approval "$@" ;;
    print-apply-bundle)  cmd_print_apply_bundle "$@" ;;
    print-apply)         cmd_print_apply "$@" ;;
    verify-final)        cmd_verify_final "$@" ;;
    *)
      err "unknown subcommand: $sub"
      info ""
      usage
      exit $EXIT_USAGE
      ;;
  esac
}

main "$@"
