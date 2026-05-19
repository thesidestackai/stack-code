#!/usr/bin/env bash
# Offline test suite for scripts/claw-sidestack-smoke.
#
# No real broker. No real `curl`. No real `claw`. Each case stages an
# isolated REPO_ROOT with the smoke helper copied in, a fake wrapper at
# scripts/claw-sidestack-local, and a controlled env file. A fake `curl`
# shim on PATH records argv and exits per FAKE_CURL_EXIT.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
REAL_SMOKE="${REPO_ROOT}/scripts/claw-sidestack-smoke"

if [[ ! -x "${REAL_SMOKE}" ]]; then
  printf 'test setup: smoke helper missing or not executable at %s\n' "${REAL_SMOKE}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t claw-sidestack-smoke.XXXXXX)"
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

# stage_layout <case_name> [env_file_content]
#
# Builds an isolated REPO_ROOT under ${WORK_DIR}/<case_name>/root containing
# the smoke helper, a fake claw-sidestack-local wrapper, and a controllable
# env file. Returns the case_dir path on stdout.
stage_layout() {
  local case_name="$1"
  local env_content="${2-}"
  local case_dir="${WORK_DIR}/${case_name}"
  local root="${case_dir}/root"
  mkdir -p "${root}/scripts" "${root}/examples" "${case_dir}/fakebin"

  cp "${REAL_SMOKE}" "${root}/scripts/claw-sidestack-smoke"
  chmod 0755 "${root}/scripts/claw-sidestack-smoke"

  cat > "${root}/scripts/claw-sidestack-local" <<'WRAP'
#!/usr/bin/env bash
log="${FAKE_WRAPPER_LOG:?FAKE_WRAPPER_LOG must be set}"
{
  printf 'FAKE_WRAPPER_CALLED=1\n'
  printf 'ARGC=%d\n' "$#"
  for arg in "$@"; do
    printf 'ARG=%s\n' "${arg}"
  done
} >> "${log}"
WRAP
  chmod 0755 "${root}/scripts/claw-sidestack-local"

  if [[ -z "${env_content}" ]]; then
    cat > "${root}/examples/sidestack-local.env" <<'ENV'
export OPENAI_BASE_URL="http://127.0.0.1:11435/v1"
export OPENAI_API_KEY="local"
ENV
  else
    printf '%s\n' "${env_content}" > "${root}/examples/sidestack-local.env"
  fi

  cat > "${case_dir}/fakebin/curl" <<'CURL'
#!/usr/bin/env bash
log="${FAKE_CURL_LOG:?FAKE_CURL_LOG must be set}"
{
  printf 'FAKE_CURL_CALLED=1\n'
  for arg in "$@"; do
    printf 'ARG=%s\n' "${arg}"
  done
} >> "${log}"
exit "${FAKE_CURL_EXIT:-0}"
CURL
  chmod 0755 "${case_dir}/fakebin/curl"

  printf '%s\n' "${case_dir}"
}

# run_case <name> <case_dir> <expected_exit> <fake_curl_exit> [smoke argv...]
run_case() {
  local name="$1"
  local case_dir="$2"
  local expected_exit="$3"
  local curl_exit="$4"
  shift 4
  local smoke="${case_dir}/root/scripts/claw-sidestack-smoke"
  local fakebin="${case_dir}/fakebin"
  local curl_log="${case_dir}/fake_curl.log"
  local wrapper_log="${case_dir}/fake_wrapper.log"
  : > "${curl_log}"
  : > "${wrapper_log}"

  set +e
  env -i \
    HOME="${HOME}" \
    PATH="${fakebin}:/usr/bin:/bin" \
    FAKE_CURL_LOG="${curl_log}" \
    FAKE_CURL_EXIT="${curl_exit}" \
    FAKE_WRAPPER_LOG="${wrapper_log}" \
    bash "${smoke}" "$@" \
    >"${case_dir}/stdout" 2>"${case_dir}/stderr"
  local actual_exit=$?
  set -e

  if [[ "${actual_exit}" -ne "${expected_exit}" ]]; then
    fail_case "${name}" "expected exit ${expected_exit}, got ${actual_exit}" "${case_dir}"
    return 1
  fi
  return 0
}

fail_case() {
  local name="$1" reason="$2" case_dir="$3"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: %s\n' "${reason}" >&2
  printf '  --- stdout ---\n' >&2
  sed 's/^/    /' "${case_dir}/stdout" >&2 || true
  printf '  --- stderr ---\n' >&2
  sed 's/^/    /' "${case_dir}/stderr" >&2 || true
  printf '  --- fake curl log ---\n' >&2
  sed 's/^/    /' "${case_dir}/fake_curl.log" >&2 || true
  printf '  --- fake wrapper log ---\n' >&2
  sed 's/^/    /' "${case_dir}/fake_wrapper.log" >&2 || true
}

pass_case() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS: %s\n' "$1"
}

# ---------- case 1: no_args_prints_usage_no_calls ----------
c="no_args_prints_usage_no_calls"
cd_=$(stage_layout "${c}")
if run_case "${c}" "${cd_}" 0 0; then
  if ! grep -Fq 'Usage:' "${cd_}/stdout"; then
    fail_case "${c}" "stdout missing 'Usage:'" "${cd_}"
  elif [[ -s "${cd_}/fake_curl.log" ]]; then
    fail_case "${c}" "fake curl was invoked" "${cd_}"
  elif [[ -s "${cd_}/fake_wrapper.log" ]]; then
    fail_case "${c}" "fake wrapper was invoked" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- case 2: dry_run_no_calls ----------
c="dry_run_no_calls"
cd_=$(stage_layout "${c}")
if run_case "${c}" "${cd_}" 0 0 --dry-run; then
  if ! grep -Fq 'dry-run' "${cd_}/stdout"; then
    fail_case "${c}" "stdout missing 'dry-run' marker" "${cd_}"
  elif [[ -s "${cd_}/fake_curl.log" ]]; then
    fail_case "${c}" "fake curl was invoked" "${cd_}"
  elif [[ -s "${cd_}/fake_wrapper.log" ]]; then
    fail_case "${c}" "fake wrapper was invoked" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- case 3: malformed_mode_nonzero ----------
c="malformed_mode_nonzero"
cd_=$(stage_layout "${c}")
if run_case "${c}" "${cd_}" 64 0 --bogus; then
  if [[ -s "${cd_}/fake_curl.log" ]]; then
    fail_case "${c}" "fake curl was invoked on malformed arg" "${cd_}"
  elif [[ -s "${cd_}/fake_wrapper.log" ]]; then
    fail_case "${c}" "fake wrapper was invoked on malformed arg" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- case 4: live_curl_fail_blocks_wrapper ----------
c="live_curl_fail_blocks_wrapper"
cd_=$(stage_layout "${c}")
if run_case "${c}" "${cd_}" 8 7 --live; then
  if ! grep -Fq 'FAKE_CURL_CALLED=1' "${cd_}/fake_curl.log"; then
    fail_case "${c}" "fake curl was not invoked" "${cd_}"
  elif [[ -s "${cd_}/fake_wrapper.log" ]]; then
    fail_case "${c}" "fake wrapper was invoked despite curl failure" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- case 5: live_curl_ok_invokes_wrapper_once ----------
c="live_curl_ok_invokes_wrapper_once"
cd_=$(stage_layout "${c}")
if run_case "${c}" "${cd_}" 0 0 --live; then
  curl_calls=$(grep -c 'FAKE_CURL_CALLED=1' "${cd_}/fake_curl.log" || true)
  wrap_calls=$(grep -c 'FAKE_WRAPPER_CALLED=1' "${cd_}/fake_wrapper.log" || true)
  if [[ "${curl_calls}" != "1" ]]; then
    fail_case "${c}" "expected exactly 1 curl call, got ${curl_calls}" "${cd_}"
  elif [[ "${wrap_calls}" != "1" ]]; then
    fail_case "${c}" "expected exactly 1 wrapper call, got ${wrap_calls}" "${cd_}"
  elif ! grep -Fq 'ARG=--model' "${cd_}/fake_wrapper.log" \
      || ! grep -Fq 'ARG=fast' "${cd_}/fake_wrapper.log" \
      || ! grep -Fq 'ARG=prompt' "${cd_}/fake_wrapper.log" \
      || ! grep -Fq 'ARG=reply with the word ready' "${cd_}/fake_wrapper.log"; then
    fail_case "${c}" "fake wrapper argv did not match expected smoke prompt" "${cd_}"
  elif ! grep -Fq 'ARGC=4' "${cd_}/fake_wrapper.log"; then
    fail_case "${c}" "fake wrapper ARGC mismatch (expected 4)" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- case 6: live_refuses_11434_env ----------
c="live_refuses_11434_env"
cd_=$(stage_layout "${c}" 'export OPENAI_BASE_URL="http://127.0.0.1:11434/v1"
export OPENAI_API_KEY="local"')
if run_case "${c}" "${cd_}" 7 0 --live; then
  if ! grep -Fq 'LAW 1' "${cd_}/stderr"; then
    fail_case "${c}" "stderr missing 'LAW 1' refusal marker" "${cd_}"
  elif [[ -s "${cd_}/fake_curl.log" ]]; then
    fail_case "${c}" "fake curl was invoked despite :11434 env" "${cd_}"
  elif [[ -s "${cd_}/fake_wrapper.log" ]]; then
    fail_case "${c}" "fake wrapper was invoked despite :11434 env" "${cd_}"
  else
    pass_case "${c}"
  fi
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
