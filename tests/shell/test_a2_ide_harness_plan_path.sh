#!/usr/bin/env bash
# Offline test suite for the --plan workspace-relative resolution fix in
# scripts/a2-ide-harness.sh (cmd_validate_input, cmd_package_plan).
#
# No real `claw` invocation. No network. No broker. Each case stages an
# isolated workspace under a temp dir and runs the harness from an unrelated
# CWD to prove behavior is anchored to --workspace, not the process CWD. A
# fake `claw` shim on PATH records argv so package-plan cases can assert on
# the resolved plan path reaching claw unchanged.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
HARNESS="${REPO_ROOT}/scripts/a2-ide-harness.sh"

if [[ ! -f "${HARNESS}" ]]; then
  printf 'test setup: harness missing at %s\n' "${HARNESS}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t a2-ide-harness-plan-path.XXXXXX)"
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

# stage_layout <case_name> [ws_dirname] [plan_filename]
#
# Builds an isolated case dir with:
#   <case_dir>/ws/<ws_dirname>/examples/<plan_filename>  (valid readonly plan)
#   <case_dir>/ws/<ws_dirname>/scripts/claw-sidestack-local (stub wrapper)
#   <case_dir>/unrelated-cwd/                             (run from here)
#   <case_dir>/fakebin/claw                                (records argv)
# Echoes: "<case_dir>|<workspace_abs>|<plan_relative>"
stage_layout() {
  local case_name="$1"
  local ws_dirname="${2:-workspace}"
  local plan_filename="${3:-a2_l1a_valid_readonly_plan.yaml}"
  local case_dir="${WORK_DIR}/${case_name}"
  local ws="${case_dir}/ws/${ws_dirname}"

  mkdir -p "${ws}/examples" "${ws}/scripts" "${case_dir}/unrelated-cwd" "${case_dir}/fakebin"

  cat > "${ws}/examples/${plan_filename}" <<'PLAN'
version: 1
steps:
  - id: noop
    kind: noop
PLAN

  cat > "${ws}/scripts/claw-sidestack-local" <<'WRAP'
#!/usr/bin/env bash
exec "$@"
WRAP
  chmod 0755 "${ws}/scripts/claw-sidestack-local"

  cat > "${case_dir}/fakebin/claw" <<'FAKE'
#!/usr/bin/env bash
log="${FAKE_CLAW_LOG:?FAKE_CLAW_LOG must be set}"
{
  printf 'FAKE_CLAW_CALLED=1\n'
  printf 'ARGC=%d\n' "$#"
  for arg in "$@"; do
    printf 'ARG=%s\n' "${arg}"
  done
} >> "${log}"
FAKE
  chmod 0755 "${case_dir}/fakebin/claw"

  printf '%s|%s|examples/%s' "${case_dir}" "${ws}" "${plan_filename}"
}

fail_case() {
  local name="$1" reason="$2" stdout_file="$3" stderr_file="$4" log_file="${5:-/dev/null}"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: %s\n' "${reason}" >&2
  printf '  --- stdout ---\n' >&2
  sed 's/^/    /' "${stdout_file}" >&2 || true
  printf '  --- stderr ---\n' >&2
  sed 's/^/    /' "${stderr_file}" >&2 || true
  printf '  --- fake claw log ---\n' >&2
  sed 's/^/    /' "${log_file}" >&2 || true
}

pass_case() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS: %s\n' "$1"
}

assert_eq() {
  local name="$1" what="$2" expected="$3" actual="$4" stdout_file="$5" stderr_file="$6"
  if [[ "${actual}" != "${expected}" ]]; then
    fail_case "${name}" "${what}: expected [${expected}] got [${actual}]" "${stdout_file}" "${stderr_file}"
    return 1
  fi
}

assert_contains() {
  local name="$1" label="$2" file="$3" needle="$4" stdout_file="$5" stderr_file="$6"
  if ! grep -Fq -- "${needle}" "${file}"; then
    fail_case "${name}" "${label} missing expected text: ${needle}" "${stdout_file}" "${stderr_file}"
    return 1
  fi
}

assert_fake_not_called() {
  local name="$1" log_file="$2" stdout_file="$3" stderr_file="$4"
  if [[ -s "${log_file}" ]] && grep -Fq 'FAKE_CLAW_CALLED=1' "${log_file}"; then
    fail_case "${name}" "fake claw was unexpectedly invoked" "${stdout_file}" "${stderr_file}" "${log_file}"
    return 1
  fi
}

# =============================================================================
# validate-input
# =============================================================================

# case: relative plan succeeds from an unrelated CWD
name="validate_input_relative_plan_unrelated_cwd"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${plan_rel}" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stdout" "${out}" "validate-input: OK" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: absolute plan still succeeds
name="validate_input_absolute_plan"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${ws}/${plan_rel}" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stdout" "${out}" "validate-input: OK" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: missing relative plan fails with EXIT_VALIDATION (3)
name="validate_input_missing_relative_plan"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "examples/does-not-exist.yaml" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 3 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stderr" "${errf}" "plan.yaml not found" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: workspace path containing spaces works
name="validate_input_workspace_with_spaces"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}" "work space dir")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${plan_rel}" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: plan path containing spaces works
name="validate_input_plan_with_spaces"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}" "workspace" "plan file.yaml")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${plan_rel}" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: plan inspection reads the resolved file (after_file violation must be
# detected — this only happens if the resolved, not CWD-relative, file is read)
name="validate_input_reads_resolved_plan_content"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
printf 'version: 1\nsteps:\n  - id: noop\n    kind: noop\n    after_file: "/etc/should-be-rejected"\n' \
  > "${ws}/${plan_rel}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${plan_rel}" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 3 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stderr" "${errf}" "absolute after_file path is not allowed" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: no repository or target mutation occurs (read-only)
name="validate_input_no_mutation"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
before="$(find "${ws}" -type f | sort)"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && bash "${HARNESS}" validate-input --workspace "${ws}" --plan "${plan_rel}" >"${out}" 2>"${errf}" )
set -e
after="$(find "${ws}" -type f | sort)"
if [[ "${before}" == "${after}" ]]; then
  pass_case "${name}"
else
  fail_case "${name}" "workspace file listing changed after validate-input" "${out}" "${errf}"
fi

# =============================================================================
# package-plan
# =============================================================================

# case: relative plan succeeds from an unrelated CWD; fake claw invoked once
# with the resolved workspace-anchored plan path, wrapper unchanged, and the
# expected argv shape.
name="package_plan_relative_plan_unrelated_cwd"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "${plan_rel}" \
    --claw-binary "${case_dir}/fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "FAKE_CLAW_CALLED=1" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=${ws}/${plan_rel}" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=--wrapper" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=${ws}/scripts/claw-sidestack-local" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=plan" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=run" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=--workspace-root" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=--workspace-write-preview" "${out}" "${errf}" \
   && [[ "$(grep -c '^FAKE_CLAW_CALLED=1$' "${log}")" -eq 1 ]]; then
  pass_case "${name}"
else
  fail_case "${name}" "argv or invocation-count assertions failed" "${out}" "${errf}" "${log}"
fi

# case: absolute plan is passed unchanged
name="package_plan_absolute_plan_unchanged"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
abs_plan="${ws}/${plan_rel}"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "${abs_plan}" \
    --claw-binary "${case_dir}/fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=${abs_plan}" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: missing relative plan fails before fake Claw invocation
name="package_plan_missing_relative_plan"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "examples/does-not-exist.yaml" \
    --claw-binary "${case_dir}/fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 3 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stderr" "${errf}" "plan file not found" "${out}" "${errf}" \
   && assert_fake_not_called "${name}" "${log}" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: workspace path containing spaces works; wrapper argv still correct
name="package_plan_workspace_with_spaces"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}" "work space dir")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "${plan_rel}" \
    --claw-binary "${case_dir}/fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=${ws}/${plan_rel}" "${out}" "${errf}" \
   && assert_contains "${name}" "fake claw log" "${log}" "ARG=${ws}/scripts/claw-sidestack-local" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# case: plan filename itself (not just the workspace dirname) contains spaces;
# the resolved plan path must reach fake claw as one unsplit argv element.
name="package_plan_plan_filename_with_spaces"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}" "workspace" "a2 valid readonly plan.yaml")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "${plan_rel}" \
    --claw-binary "${case_dir}/fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
expected_argv="$(printf 'ARG=plan\nARG=run\nARG=%s/%s\nARG=--workspace-root\nARG=%s\nARG=--workspace-write-preview\nARG=--wrapper\nARG=%s/scripts/claw-sidestack-local' \
  "${ws}" "${plan_rel}" "${ws}" "${ws}")"
actual_argv="$(grep '^ARG=' "${log}" || true)"

if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && [[ "$(grep -c '^FAKE_CLAW_CALLED=1$' "${log}")" -eq 1 ]] \
   && assert_eq "${name}" "argv order (plan run first; resolved plan is one unsplit element)" \
        "${expected_argv}" "${actual_argv}" "${out}" "${errf}"; then
  pass_case "${name}"
else
  fail_case "${name}" "argv order, invocation-count, or exit-code assertions failed" "${out}" "${errf}" "${log}"
fi

# =============================================================================
# regression
# =============================================================================

# case: --claw-binary must remain absolute (relative claw-binary is refused)
name="regression_claw_binary_must_be_absolute"
IFS='|' read -r case_dir ws plan_rel <<<"$(stage_layout "${name}")"
log="${case_dir}/fake_claw.log"; : > "${log}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
( cd "${case_dir}/unrelated-cwd" && \
  PATH="${case_dir}/fakebin:${PATH}" FAKE_CLAW_LOG="${log}" \
  bash "${HARNESS}" package-plan --workspace "${ws}" --plan "${plan_rel}" \
    --claw-binary "fakebin/claw" >"${out}" 2>"${errf}" )
rc=$?
set -e
if assert_eq "${name}" "exit code" 3 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stderr" "${errf}" "claw-binary must be an absolute path" "${out}" "${errf}" \
   && assert_fake_not_called "${name}" "${log}" "${out}" "${errf}"; then
  pass_case "${name}"
fi

# =============================================================================
# canonical default entrypoint
# =============================================================================

# case: with A2_CLAW unset, the default resolves to $HOME/.local/bin/claw
# (never a target/debug/claw artifact). `help` never invokes claw, so this is
# read-only by construction.
name="help_default_claw_uses_home_local_bin"
case_dir="${WORK_DIR}/${name}"
fixture_home="${case_dir}/home with spaces"
mkdir -p "${fixture_home}"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
env -u A2_CLAW HOME="${fixture_home}" bash "${HARNESS}" help >"${out}" 2>"${errf}"
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stdout" "${out}" "current: ${fixture_home}/.local/bin/claw" "${out}" "${errf}"; then
  if grep -Fq '/target/debug/claw' "${out}"; then
    fail_case "${name}" "help output still references a target/debug/claw artifact" "${out}" "${errf}"
  else
    pass_case "${name}"
  fi
fi

# case: an explicit A2_CLAW override remains authoritative over the default
name="help_explicit_A2_CLAW_override_wins"
case_dir="${WORK_DIR}/${name}"
fixture_home="${case_dir}/home"
mkdir -p "${fixture_home}"
override_claw="${case_dir}/approved-claw"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
env A2_CLAW="${override_claw}" HOME="${fixture_home}" bash "${HARNESS}" help >"${out}" 2>"${errf}"
rc=$?
set -e
if assert_eq "${name}" "exit code" 0 "${rc}" "${out}" "${errf}" \
   && assert_contains "${name}" "stdout" "${out}" "current: ${override_claw}" "${out}" "${errf}"; then
  if grep -Fq "${fixture_home}/.local/bin/claw" "${out}"; then
    fail_case "${name}" "default was substituted despite an explicit A2_CLAW override" "${out}" "${errf}"
  else
    pass_case "${name}"
  fi
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
