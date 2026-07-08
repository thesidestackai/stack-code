#!/usr/bin/env bash
# check-harness-exec-safety.sh — static safety grep for scripts/a2-ide-harness.sh.
#
# Asserts that N6A execution subcommands do not introduce forbidden operations.
# Run from the repository root. Exits non-zero on any violation.
#
# Source of truth: docs/stack-code-n6a-helper-exec-allowlist-design.md §33
set -euo pipefail

SCRIPT="$(dirname "$0")/a2-ide-harness.sh"

if [[ ! -f "$SCRIPT" ]]; then
  echo "ERROR: a2-ide-harness.sh not found at: $SCRIPT" >&2
  exit 1
fi

VIOLATIONS=0

# Check non-comment lines of the script for a forbidden pattern.
# Args: label pattern
check() {
  local label="$1" pattern="$2"
  if grep -v '^\s*#' "$SCRIPT" | grep -qE "$pattern" 2>/dev/null; then
    echo "FAIL [$label]: forbidden pattern found in $SCRIPT" >&2
    grep -v '^\s*#' "$SCRIPT" | grep -nE "$pattern" | head -5 >&2
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
}

# Law 1: raw :11434 must never appear as a non-comment literal.
check "NO-RAW-11434"     '\b11434\b'

# Non-force push only: git push must never use --force or variants.
check "NO-FORCE-PUSH"    'git\s+.*push\s+.*--(force|force-with-lease|force-if-includes)\b'

# No PR mark-ready, merge, or approve.
check "NO-PR-MERGE"      'gh\s+pr\s+(merge|ready)\b'
check "NO-PR-APPROVE"    'gh\s+pr\s+review\s+.*--approve\b'

# package-plan may call claw plan run only. claw plan apply/approve/apply-bundle
# must never appear as direct invocations (not inside info/echo strings — those
# use the shq() form `$(shq "$A2_CLAW")` which does not match this pattern).
check "NO-CLAW-APPLY-INVOKE"  '"[$](claw_bin|A2_CLAW)"\s+plan\s+(apply|approve|apply-bundle)\b'

# Exact-path staging only: git add . and git add -A are forbidden.
check "NO-GIT-ADD-ALL"   'git\s+(-C\s+\S+\s+)?add\s+(\.(\s|$)|-A\b|-a\b|--all\b)'

# No commit --amend.
check "NO-COMMIT-AMEND"  'git\s+.*commit\s+.*--amend\b'

if [[ "$VIOLATIONS" -gt 0 ]]; then
  echo "check-harness-exec-safety: $VIOLATIONS violation(s) found in $SCRIPT" >&2
  exit 1
fi

echo "check-harness-exec-safety: PASS ($SCRIPT)"
