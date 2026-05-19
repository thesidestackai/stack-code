#!/usr/bin/env bash
# Offline test suite for scripts/claw-sidestack-local.
#
# No real `claw` invocation. No network. No cargo. Each case stages an
# isolated REPO_ROOT under a temp dir, copies the wrapper into it, and
# substitutes a controlled `examples/sidestack-local.env`. A fake `claw`
# shim on PATH records argv + selected env so we can assert on it.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
REAL_WRAPPER="${REPO_ROOT}/scripts/claw-sidestack-local"
REAL_ENV_FILE="${REPO_ROOT}/examples/sidestack-local.env"

if [[ ! -x "${REAL_WRAPPER}" ]]; then
  printf 'test setup: wrapper missing or not executable at %s\n' "${REAL_WRAPPER}" >&2
  exit 2
fi
if [[ ! -f "${REAL_ENV_FILE}" ]]; then
  printf 'test setup: real env file missing at %s\n' "${REAL_ENV_FILE}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t claw-sidestack-local.XXXXXX)"
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

# stage_layout <case_name> [env_file_content]
#
# Builds an isolated REPO_ROOT under ${WORK_DIR}/<case_name>/root with the
# wrapper copied into scripts/ and a controllable env file under examples/.
# If env_file_content is omitted, the real env file is copied verbatim. If
# it is the literal "__MISSING__", no env file is created.
stage_layout() {
  local case_name="$1"
  local content="${2-__COPY_REAL__}"
  local case_dir="${WORK_DIR}/${case_name}"
  local root="${case_dir}/root"
  mkdir -p "${root}/scripts" "${root}/examples" "${case_dir}/fakebin"

  cp "${REAL_WRAPPER}" "${root}/scripts/claw-sidestack-local"
  chmod 0755 "${root}/scripts/claw-sidestack-local"

  if [[ "${content}" == "__MISSING__" ]]; then
    :
  elif [[ "${content}" == "__COPY_REAL__" ]]; then
    cp "${REAL_ENV_FILE}" "${root}/examples/sidestack-local.env"
  else
    printf '%s\n' "${content}" > "${root}/examples/sidestack-local.env"
  fi

  cat > "${case_dir}/fakebin/claw" <<'FAKE'
#!/usr/bin/env bash
log="${FAKE_CLAW_LOG:?FAKE_CLAW_LOG must be set}"
{
  printf 'FAKE_CLAW_CALLED=1\n'
  printf 'OPENAI_BASE_URL=%s\n' "${OPENAI_BASE_URL:-}"
  printf 'RUSTY_CLAUDE_LLM_CALLER=%s\n' "${RUSTY_CLAUDE_LLM_CALLER:-}"
  printf 'RUSTY_CLAUDE_TASK_TYPE=%s\n' "${RUSTY_CLAUDE_TASK_TYPE:-}"
  printf 'ARGC=%d\n' "$#"
  for arg in "$@"; do
    printf 'ARG=%s\n' "${arg}"
  done
} >> "${log}"
FAKE
  chmod 0755 "${case_dir}/fakebin/claw"

  printf '%s\n' "${case_dir}"
}

run_case() {
  local name="$1"
  shift
  local case_dir="$1"
  shift
  local expected_exit="$1"
  shift
  local include_fakebin="$1"  # "yes" or "no"
  shift
  # remaining args: extra env in NAME=VALUE form, then "--" then argv for wrapper
  local extra_env=()
  while (($#)); do
    if [[ "$1" == "--" ]]; then
      shift
      break
    fi
    extra_env+=("$1")
    shift
  done
  local wrapper="${case_dir}/root/scripts/claw-sidestack-local"
  local fakebin="${case_dir}/fakebin"
  local log_file="${case_dir}/fake_claw.log"
  local stdout_file="${case_dir}/wrapper.stdout"
  local stderr_file="${case_dir}/wrapper.stderr"
  : > "${log_file}"

  local path
  if [[ "${include_fakebin}" == "yes" ]]; then
    path="${fakebin}:/usr/bin:/bin"
  else
    path="/usr/bin:/bin"
  fi

  set +e
  env -i \
    HOME="${HOME}" \
    PATH="${path}" \
    FAKE_CLAW_LOG="${log_file}" \
    "${extra_env[@]}" \
    bash "${wrapper}" "$@" \
    >"${stdout_file}" 2>"${stderr_file}"
  local actual_exit=$?
  set -e

  if [[ "${actual_exit}" -ne "${expected_exit}" ]]; then
    fail_case "${name}" "expected exit ${expected_exit}, got ${actual_exit}" \
      "${stdout_file}" "${stderr_file}" "${log_file}"
    return 1
  fi

  printf '%s' "${case_dir}"
}

fail_case() {
  local name="$1"
  local reason="$2"
  local stdout_file="$3"
  local stderr_file="$4"
  local log_file="$5"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: %s\n' "${reason}" >&2
  printf '  --- wrapper stdout ---\n' >&2
  sed 's/^/    /' "${stdout_file}" >&2 || true
  printf '  --- wrapper stderr ---\n' >&2
  sed 's/^/    /' "${stderr_file}" >&2 || true
  printf '  --- fake claw log ---\n' >&2
  sed 's/^/    /' "${log_file}" >&2 || true
}

pass_case() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS: %s\n' "$1"
}

assert_log_contains() {
  local name="$1"
  local log_file="$2"
  local needle="$3"
  if ! grep -Fq "${needle}" "${log_file}"; then
    fail_case "${name}" "fake claw log missing expected line: ${needle}" \
      /dev/null /dev/null "${log_file}"
    return 1
  fi
}

assert_stderr_contains() {
  local name="$1"
  local stderr_file="$2"
  local needle="$3"
  if ! grep -Fq "${needle}" "${stderr_file}"; then
    fail_case "${name}" "wrapper stderr missing expected text: ${needle}" \
      /dev/null "${stderr_file}" /dev/null
    return 1
  fi
}

assert_fake_not_called() {
  local name="$1"
  local log_file="$2"
  if grep -Fq 'FAKE_CLAW_CALLED=1' "${log_file}"; then
    fail_case "${name}" "fake claw was unexpectedly invoked" \
      /dev/null /dev/null "${log_file}"
    return 1
  fi
}

# ---------- case 1: happy_path_clean_shell ----------
case1_name="happy_path_clean_shell"
case1_dir="$(stage_layout "${case1_name}")"
if run_case "${case1_name}" "${case1_dir}" 0 yes -- prompt "say hi" >/dev/null; then
  log="${case1_dir}/fake_claw.log"
  if assert_log_contains "${case1_name}" "${log}" 'FAKE_CLAW_CALLED=1' \
     && assert_log_contains "${case1_name}" "${log}" 'OPENAI_BASE_URL=http://127.0.0.1:11435/v1' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=prompt' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=say hi'; then
    pass_case "${case1_name}"
  fi
fi

# ---------- case 2: preexisting_bad_openai_base_is_overridden_by_profile ----------
case2_name="preexisting_bad_openai_base_is_overridden_by_profile"
case2_dir="$(stage_layout "${case2_name}")"
if run_case "${case2_name}" "${case2_dir}" 0 yes \
     "OPENAI_BASE_URL=http://127.0.0.1:11434/v1" \
     -- --model fast prompt "ping" >/dev/null; then
  log="${case2_dir}/fake_claw.log"
  if assert_log_contains "${case2_name}" "${log}" 'FAKE_CLAW_CALLED=1' \
     && assert_log_contains "${case2_name}" "${log}" 'OPENAI_BASE_URL=http://127.0.0.1:11435/v1'; then
    if grep -Fq 'OPENAI_BASE_URL=http://127.0.0.1:11434/v1' "${log}"; then
      fail_case "${case2_name}" "fake claw saw the pre-existing :11434 value; profile did not override it" \
        /dev/null /dev/null "${log}"
    else
      pass_case "${case2_name}"
    fi
  fi
fi

# ---------- case 3: law1_refuses_bad_effective_env_file ----------
case3_name="law1_refuses_bad_effective_env_file"
case3_dir="$(stage_layout "${case3_name}" 'export OPENAI_BASE_URL="http://127.0.0.1:11434/v1"
export OPENAI_API_KEY="local"')"
if run_case "${case3_name}" "${case3_dir}" 3 yes -- prompt "should refuse" >/dev/null; then
  stderr="${case3_dir}/wrapper.stderr"
  log="${case3_dir}/fake_claw.log"
  if assert_stderr_contains "${case3_name}" "${stderr}" 'LAW 1' \
     && assert_fake_not_called "${case3_name}" "${log}"; then
    pass_case "${case3_name}"
  fi
fi

# ---------- case 4: allowlist_refuses_cloud_url ----------
case4_name="allowlist_refuses_cloud_url"
case4_dir="$(stage_layout "${case4_name}" 'export OPENAI_BASE_URL="https://api.openai.com/v1"
export OPENAI_API_KEY="local"')"
if run_case "${case4_name}" "${case4_dir}" 3 yes -- prompt "should refuse" >/dev/null; then
  stderr="${case4_dir}/wrapper.stderr"
  log="${case4_dir}/fake_claw.log"
  if assert_stderr_contains "${case4_name}" "${stderr}" 'LAW 1' \
     && assert_fake_not_called "${case4_name}" "${log}"; then
    pass_case "${case4_name}"
  fi
fi

# ---------- case 5: missing_env_file ----------
case5_name="missing_env_file"
case5_dir="$(stage_layout "${case5_name}" "__MISSING__")"
if run_case "${case5_name}" "${case5_dir}" 2 yes -- prompt "should refuse" >/dev/null; then
  stderr="${case5_dir}/wrapper.stderr"
  log="${case5_dir}/fake_claw.log"
  if assert_stderr_contains "${case5_name}" "${stderr}" 'env file not found' \
     && assert_fake_not_called "${case5_name}" "${log}"; then
    pass_case "${case5_name}"
  fi
fi

# ---------- case 6: claw_not_on_path ----------
case6_name="claw_not_on_path"
case6_dir="$(stage_layout "${case6_name}")"
if run_case "${case6_name}" "${case6_dir}" 4 no -- prompt "should fail" >/dev/null; then
  stderr="${case6_dir}/wrapper.stderr"
  log="${case6_dir}/fake_claw.log"
  # shellcheck disable=SC2016  # literal backticks match wrapper's user-facing message
  if assert_stderr_contains "${case6_name}" "${stderr}" '`claw` not found on PATH' \
     && assert_fake_not_called "${case6_name}" "${log}"; then
    pass_case "${case6_name}"
  fi
fi

# ---------- case 7: arg_passthrough ----------
case7_name="arg_passthrough"
case7_dir="$(stage_layout "${case7_name}")"
if run_case "${case7_name}" "${case7_dir}" 0 yes -- --model fast prompt "say x" >/dev/null; then
  log="${case7_dir}/fake_claw.log"
  if assert_log_contains "${case7_name}" "${log}" 'ARGC=4' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=--model' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=fast' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=prompt' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=say x'; then
    pass_case "${case7_name}"
  fi
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
