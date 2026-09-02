#!/usr/bin/env bash
# Offline test suite for scripts/claw-sidestack-local.
#
# No real `claw` invocation. No real $HOME. No network. No cargo. Each case
# stages an isolated REPO_ROOT (a throwaway git repo) under a temp dir, copies
# the wrapper and the canonical status helper into it, substitutes a controlled
# `examples/sidestack-local.env`, and installs a fake canonical `claw` into a
# throwaway HOME. Decoy `claw` binaries are planted on PATH and in
# ~/.cargo/bin so that any silent PATH fallback fails the suite loudly.
#
# The N6 broker readiness gate is exercised through a fake `curl` planted
# first on PATH, so the real wrapper logic runs against a controlled HTTP
# response and the real broker is never contacted. Every case gets one, so a
# case that reaches the network at all would fail loudly rather than silently
# querying :11435.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
REAL_WRAPPER="${REPO_ROOT}/scripts/claw-sidestack-local"
REAL_STATUS="${REPO_ROOT}/scripts/claw-canonical-status"
REAL_ENV_FILE="${REPO_ROOT}/examples/sidestack-local.env"

if [[ ! -x "${REAL_WRAPPER}" ]]; then
  printf 'test setup: wrapper missing or not executable at %s\n' "${REAL_WRAPPER}" >&2
  exit 2
fi
if [[ ! -x "${REAL_STATUS}" ]]; then
  printf 'test setup: canonical status helper missing or not executable at %s\n' "${REAL_STATUS}" >&2
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

# write_fake_claw <path> <identity> <reported_sha> [capabilities]
#
# A stand-in for a claw executable. It answers `--version` with a Git SHA
# banner in the real binary's format, and otherwise records that this specific
# instance was executed, so the suite can tell canonical from decoy.
#
# [capabilities] is the literal text of the banner's `Capabilities` line. The
# default is the token a build implementing the in-process N6 enforcement
# contract advertises, which is what every pre-existing case wants: they are
# testing the startup gate, not the capability floor. The literal "__NONE__"
# omits the line entirely — a binary predating the contract.
write_fake_claw() {
  local path="$1" identity="$2" sha="$3"
  local capabilities="${4-sidestack-n6-enforce-v1}"
  local capability_line=""
  if [[ "${capabilities}" != "__NONE__" ]]; then
    capability_line="  printf '  Capabilities     %s\n' '${capabilities}'"
  fi
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n  Version          0.1.0\n  Git SHA          %s\n' '${sha}'
${capability_line}
  exit 0
fi
log="\${FAKE_CLAW_LOG:?FAKE_CLAW_LOG must be set}"
{
  printf 'FAKE_CLAW_CALLED=1\n'
  printf 'IDENTITY=%s\n' '${identity}'
  printf 'OPENAI_BASE_URL=%s\n' "\${OPENAI_BASE_URL:-}"
  printf 'RUSTY_CLAUDE_LLM_CALLER=%s\n' "\${RUSTY_CLAUDE_LLM_CALLER:-}"
  printf 'RUSTY_CLAUDE_TASK_TYPE=%s\n' "\${RUSTY_CLAUDE_TASK_TYPE:-}"
  printf 'CLAW_SIDESTACK_N6_ENFORCE=%s\n' "\${CLAW_SIDESTACK_N6_ENFORCE:-}"
  printf 'ARGC=%d\n' "\$#"
  for arg in "\$@"; do
    printf 'ARG=%s\n' "\${arg}"
  done
} >> "\${log}"
FAKE
  chmod 0755 -- "${path}"
}

# The curl version the wrapper's capability floor accepts by default. Real
# curl on this lane is 8.5.0; fixtures mirror that unless a case overrides it.
FAKE_CURL_DEFAULT_VERSION='curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0 OpenSSL/3.0.13 zlib/1.3'

# write_fake_readiness_client <case_dir> <curl_exit> <http_code> <body>
#                             [version_line] [version_exit] [path]
#
# A stand-in for `curl`, planted first on PATH. It appends every argument it
# was given to <case_dir>/readiness.log so the suite can assert WHICH URL was
# queried (and that no credential was passed), then emits <body> followed by a
# newline and <http_code>, matching the `--write-out` contract the wrapper
# relies on. A non-zero <curl_exit> simulates a transport failure or timeout
# and suppresses all output.
#
# `--version` is answered SEPARATELY, from <version_line>, and is recorded in
# <case_dir>/curlversion.log rather than readiness.log. Keeping the two logs
# apart is load-bearing: the capability probe is not a readiness request, so
# `assert_readiness_not_called` must stay true for a case that refuses at the
# version floor. Both arms record SELF=$0, so a case can prove the executable
# that answered `--version` is the same one that performed the GET.
#
# <version_exit> simulates `curl --version` itself failing. <path> overrides
# where the fake is written, for cases that plant more than one curl.
write_fake_readiness_client() {
  local case_dir="$1" curl_exit="$2" http_code="$3" body="$4"
  local version_line="${5-${FAKE_CURL_DEFAULT_VERSION}}"
  local version_exit="${6-0}"
  local path="${7-${case_dir}/decoybin/curl}"
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  {
    printf 'CURL_VERSION_PROBE=1\n'
    printf 'SELF=%s\n' "\$0"
  } >> '${case_dir}/curlversion.log'
  printf '%s\n' '${version_line}'
  exit ${version_exit}
fi
{
  printf 'READINESS_CLIENT_CALLED=1\n'
  printf 'SELF=%s\n' "\$0"
  for arg in "\$@"; do
    printf 'ARG=%s\n' "\${arg}"
  done
} >> '${case_dir}/readiness.log'
if [[ '${curl_exit}' != '0' ]]; then
  exit ${curl_exit}
fi
printf '%s' '${body}'
printf '\n%s' '${http_code}'
FAKE
  chmod 0755 -- "${path}"
}

# assert_curl_floor_refusal <name> <case_dir>
#
# The shape every rejected-version case must have: the capability probe ran,
# no readiness request was issued, and canonical never executed.
assert_curl_floor_refusal() {
  local name="$1" case_dir="$2"
  local version_log="${case_dir}/curlversion.log"
  local readiness_log="${case_dir}/readiness.log"
  local claw_log="${case_dir}/fake_claw.log"
  if [[ ! -s "${version_log}" ]]; then
    fail_case "${name}" "the curl capability probe never ran" /dev/null /dev/null "${version_log}"
    return 1
  fi
  if [[ -s "${readiness_log}" ]]; then
    fail_case "${name}" "a readiness request was issued despite an unproven curl" \
      /dev/null /dev/null "${readiness_log}"
    return 1
  fi
  if grep -Fq 'FAKE_CLAW_CALLED=1' "${claw_log}"; then
    fail_case "${name}" "canonical claw executed despite an unproven curl" \
      /dev/null /dev/null "${claw_log}"
    return 1
  fi
}

# assert_same_curl_command_path <name> <case_dir>
#
# The command PATH that answered `--version` must be the same command path
# that performed the GET. A wrapper that probed one curl and then re-resolved
# another from PATH would record two different SELF values here.
#
# This proves path selection only. SELF is the path each fixture was invoked
# as, so identical values prove the wrapper did not re-resolve PATH — NOT that
# the bytes behind that path were unchanged. Executable identity is explicitly
# outside the wrapper's threat model; see the TRUST_BOUNDARY_RESIDUAL cases
# below, which demonstrate that the bytes CAN change behind a stable path.
assert_same_curl_command_path() {
  local name="$1" case_dir="$2"
  local probed used
  probed="$(grep -m1 '^SELF=' "${case_dir}/curlversion.log" | cut -d= -f2- || true)"
  used="$(grep -m1 '^SELF=' "${case_dir}/readiness.log" | cut -d= -f2- || true)"
  if [[ -z "${probed}" || -z "${used}" ]]; then
    fail_case "${name}" "missing SELF record (probed='${probed}' used='${used}')" \
      /dev/null "${case_dir}/curlversion.log" "${case_dir}/readiness.log"
    return 1
  fi
  if [[ "${probed}" != "${used}" ]]; then
    fail_case "${name}" "curl command-path binding broken: probed '${probed}' but used '${used}'" \
      /dev/null "${case_dir}/curlversion.log" "${case_dir}/readiness.log"
    return 1
  fi
}

# The default readiness answer: safe to start a qwen3:14b session.
READY_QWEN3_14B='{"ready": true, "reason_code": "READY_AFTER_SAFE_EVICTION", "requested_model": "qwen3:14b", "requires_hyperliquid_pause": false}'

# refusal_body <reason_code> [model]
refusal_body() {
  printf '{"ready": false, "reason_code": "%s", "requested_model": "%s", "requires_hyperliquid_pause": true}' \
    "$1" "${2:-qwen3:14b}"
}

assert_readiness_not_called() {
  local name="$1" readiness_log="$2"
  if [[ -s "${readiness_log}" ]]; then
    fail_case "${name}" "the readiness endpoint was queried when it must not have been" \
      /dev/null /dev/null "${readiness_log}"
    return 1
  fi
}

assert_readiness_called_for() {
  local name="$1" readiness_log="$2" encoded_model="$3"
  if ! grep -Fq "requested_model=${encoded_model}" "${readiness_log}"; then
    fail_case "${name}" "readiness was not queried for ${encoded_model}" \
      /dev/null /dev/null "${readiness_log}"
    return 1
  fi
}

# LAW 1 for the readiness call itself: the query may only ever address the
# allowlisted broker origin, never the raw Ollama port and never a remote host.
assert_readiness_url_is_broker_only() {
  local name="$1" readiness_log="$2"
  local url_lines line url
  url_lines="$(grep '^ARG=http' "${readiness_log}" || true)"
  if [[ -z "${url_lines}" ]]; then
    fail_case "${name}" "no readiness URL was recorded" /dev/null /dev/null "${readiness_log}"
    return 1
  fi
  while IFS= read -r line; do
    url="${line#ARG=}"
    if [[ ! "${url}" =~ ^http://(127\.0\.0\.1|localhost):11435/status/n6_planner_ready\? ]]; then
      fail_case "${name}" "readiness URL is not an allowlisted broker URL: ${url}" \
        /dev/null /dev/null "${readiness_log}"
      return 1
    fi
  done <<<"${url_lines}"
  if grep -Fq '11434' "${readiness_log}"; then
    fail_case "${name}" "the readiness client saw the raw Ollama port" \
      /dev/null /dev/null "${readiness_log}"
    return 1
  fi
}

# stage_layout <case_name> [env_file_content]
#
# Builds an isolated REPO_ROOT under ${WORK_DIR}/<case_name>/root with the
# wrapper and canonical status helper copied into scripts/ and a controllable
# env file under examples/. The root is a real git repo whose
# refs/remotes/origin/main equals HEAD, so the freshness comparison has a base.
# If env_file_content is omitted, the real env file is copied verbatim. If it
# is the literal "__MISSING__", no env file is created.
#
# Also plants two decoys that must never be executed: one `claw` on PATH and
# one at <fixture HOME>/.cargo/bin/claw.
stage_layout() {
  local case_name="$1"
  local content="${2-__COPY_REAL__}"
  local case_dir="${WORK_DIR}/${case_name}"
  local root="${case_dir}/root"
  mkdir -p "${root}/scripts" "${root}/examples" "${case_dir}/decoybin" "${case_dir}/home" \
    "${case_dir}/cwd"

  cp "${REAL_WRAPPER}" "${root}/scripts/claw-sidestack-local"
  cp "${REAL_STATUS}" "${root}/scripts/claw-canonical-status"
  chmod 0755 "${root}/scripts/claw-sidestack-local" "${root}/scripts/claw-canonical-status"

  if [[ "${content}" == "__MISSING__" ]]; then
    :
  elif [[ "${content}" == "__COPY_REAL__" ]]; then
    cp "${REAL_ENV_FILE}" "${root}/examples/sidestack-local.env"
  else
    printf '%s\n' "${content}" > "${root}/examples/sidestack-local.env"
  fi

  git -C "${root}" init -q
  git -C "${root}" add -A
  git -C "${root}" -c user.name=fixture -c user.email=fixture@example.invalid \
    commit -q -m "fixture"
  git -C "${root}" update-ref refs/remotes/origin/main "$(git -C "${root}" rev-parse HEAD)"

  # decoys — a PATH `claw` and a ~/.cargo/bin/claw, both of which the wrapper
  # must refuse to select under every condition.
  write_fake_claw "${case_dir}/decoybin/claw" "DECOY_PATH" "dec0y00"
  write_fake_claw "${case_dir}/home/.cargo/bin/claw" "DECOY_CARGO_BIN" "dec0y01"

  # Every case gets a readiness client so no case can silently reach the real
  # broker. Cases that need a different answer overwrite it.
  write_fake_readiness_client "${case_dir}" 0 200 "${READY_QWEN3_14B}"

  printf '%s\n' "${case_dir}"
}

# fixture_head_short <case_dir>
fixture_head_short() { git -C "$1/root" rev-parse --short HEAD; }

# fixture_head_full <case_dir> — the canonical contract compares FULL 40-char
# SHAs, so a "current" fixture must report the whole thing.
fixture_head_full() { git -C "$1/root" rev-parse HEAD; }

# install_canonical <case_dir> <mode> [capabilities]
#   current | stale | stale_prefix | symlink | nonexec | missing
#
# [capabilities] is forwarded to write_fake_claw. Omitted, the canonical fixture
# advertises the N6 enforcement capability, so cases exercising the startup gate
# are unaffected by the capability floor. Pass "__NONE__" for a binary that
# predates the contract.
install_canonical() {
  local case_dir="$1" mode="$2" capabilities="${3-sidestack-n6-enforce-v1}"
  local canonical="${case_dir}/home/.local/bin/claw"
  mkdir -p "${case_dir}/home/.local/bin"
  case "${mode}" in
    current)
      write_fake_claw "${canonical}" "CANONICAL" "$(fixture_head_full "${case_dir}")" \
        "${capabilities}"
      ;;
    stale)
      write_fake_claw "${canonical}" "CANONICAL_STALE" \
        "5ta1e005ta1e005ta1e005ta1e005ta1e005ta1e" "${capabilities}"
      ;;
    stale_prefix)
      # A legacy build whose banner is a 7-character PREFIX of the real HEAD.
      # Under the repaired exact-match contract this is unproven, not current.
      write_fake_claw "${canonical}" "CANONICAL_STALE_PREFIX" \
        "$(fixture_head_short "${case_dir}")" "${capabilities}"
      ;;
    symlink)
      write_fake_claw "${case_dir}/fake-cargo-target/release/claw" "SYMLINK_TARGET" \
        "$(fixture_head_full "${case_dir}")" "${capabilities}"
      ln -s "${case_dir}/fake-cargo-target/release/claw" "${canonical}"
      ;;
    nonexec)
      write_fake_claw "${canonical}" "CANONICAL" "$(fixture_head_full "${case_dir}")" \
        "${capabilities}"
      chmod 0644 "${canonical}"
      ;;
    missing)
      :
      ;;
    *)
      printf 'test setup: unknown canonical mode %s\n' "${mode}" >&2
      exit 2
      ;;
  esac
}

run_case() {
  local name="$1"
  shift
  local case_dir="$1"
  shift
  local expected_exit="$1"
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
  local log_file="${case_dir}/fake_claw.log"
  local stdout_file="${case_dir}/wrapper.stdout"
  local stderr_file="${case_dir}/wrapper.stderr"
  : > "${log_file}"
  : > "${case_dir}/readiness.log"
  : > "${case_dir}/curlversion.log"

  set +e
  (
    # RUN_CASE_CWD lets a case choose the directory the wrapper is invoked
    # FROM, which the `plan run` default-wrapper proof depends on: the runner
    # resolves its default `scripts/claw-sidestack-local` relative to that
    # directory. Unset (the default) keeps every pre-existing case on the
    # neutral `${case_dir}/cwd`, which holds no `scripts/` tree.
    cd "${RUN_CASE_CWD:-${case_dir}/cwd}" || exit 127
    env -i \
      HOME="${case_dir}/home" \
      PATH="${case_dir}/decoybin:${case_dir}/home/.cargo/bin:/usr/bin:/bin" \
      FAKE_CLAW_LOG="${log_file}" \
      "${extra_env[@]}" \
      bash "${wrapper}" "$@"
  ) >"${stdout_file}" 2>"${stderr_file}"
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

# No decoy may ever run, in any case, for any reason.
assert_no_decoy_executed() {
  local name="$1"
  local log_file="$2"
  if grep -Eq 'IDENTITY=DECOY_(PATH|CARGO_BIN)' "${log_file}"; then
    fail_case "${name}" "a decoy claw (PATH or ~/.cargo/bin) was executed" \
      /dev/null /dev/null "${log_file}"
    return 1
  fi
}

# ---------- case 1: happy_path_clean_shell ----------
case1_name="happy_path_clean_shell"
case1_dir="$(stage_layout "${case1_name}")"
install_canonical "${case1_dir}" current
if run_case "${case1_name}" "${case1_dir}" 0 -- --model fast prompt "say hi" >/dev/null; then
  log="${case1_dir}/fake_claw.log"
  if assert_log_contains "${case1_name}" "${log}" 'FAKE_CLAW_CALLED=1' \
     && assert_log_contains "${case1_name}" "${log}" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${case1_name}" "${log}" \
     && assert_log_contains "${case1_name}" "${log}" 'OPENAI_BASE_URL=http://127.0.0.1:11435/v1' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=prompt' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=say hi' \
     && assert_stderr_contains "${case1_name}" "${case1_dir}/wrapper.stderr" \
          "canonical claw: ${case1_dir}/home/.local/bin/claw" \
     && assert_readiness_called_for "${case1_name}" "${case1_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_url_is_broker_only "${case1_name}" "${case1_dir}/readiness.log"; then
    pass_case "${case1_name}"
  fi
fi

# ---------- case 2: preexisting_bad_openai_base_is_overridden_by_profile ----------
case2_name="preexisting_bad_openai_base_is_overridden_by_profile"
case2_dir="$(stage_layout "${case2_name}")"
install_canonical "${case2_dir}" current
if run_case "${case2_name}" "${case2_dir}" 0 \
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
install_canonical "${case3_dir}" current
if run_case "${case3_name}" "${case3_dir}" 3 -- prompt "should refuse" >/dev/null; then
  stderr="${case3_dir}/wrapper.stderr"
  log="${case3_dir}/fake_claw.log"
  if assert_readiness_not_called "${case3_name}" "${case3_dir}/readiness.log" \
     && assert_stderr_contains "${case3_name}" "${stderr}" 'LAW 1' \
     && assert_fake_not_called "${case3_name}" "${log}"; then
    pass_case "${case3_name}"
  fi
fi

# ---------- case 4: allowlist_refuses_cloud_url ----------
case4_name="allowlist_refuses_cloud_url"
case4_dir="$(stage_layout "${case4_name}" 'export OPENAI_BASE_URL="https://api.openai.com/v1"
export OPENAI_API_KEY="local"')"
install_canonical "${case4_dir}" current
if run_case "${case4_name}" "${case4_dir}" 3 -- prompt "should refuse" >/dev/null; then
  stderr="${case4_dir}/wrapper.stderr"
  log="${case4_dir}/fake_claw.log"
  if assert_readiness_not_called "${case4_name}" "${case4_dir}/readiness.log" \
     && assert_stderr_contains "${case4_name}" "${stderr}" 'LAW 1' \
     && assert_fake_not_called "${case4_name}" "${log}"; then
    pass_case "${case4_name}"
  fi
fi

# ---------- case 5: missing_env_file ----------
case5_name="missing_env_file"
case5_dir="$(stage_layout "${case5_name}" "__MISSING__")"
install_canonical "${case5_dir}" current
if run_case "${case5_name}" "${case5_dir}" 2 -- prompt "should refuse" >/dev/null; then
  stderr="${case5_dir}/wrapper.stderr"
  log="${case5_dir}/fake_claw.log"
  if assert_readiness_not_called "${case5_name}" "${case5_dir}/readiness.log" \
     && assert_stderr_contains "${case5_name}" "${stderr}" 'env file not found' \
     && assert_fake_not_called "${case5_name}" "${log}"; then
    pass_case "${case5_name}"
  fi
fi

# ---------- case 6: canonical_missing_refuses_without_path_fallback ----------
case6_name="canonical_missing_refuses_without_path_fallback"
case6_dir="$(stage_layout "${case6_name}")"
install_canonical "${case6_dir}" missing
if run_case "${case6_name}" "${case6_dir}" 4 -- prompt "should fail" >/dev/null; then
  stderr="${case6_dir}/wrapper.stderr"
  log="${case6_dir}/fake_claw.log"
  if assert_readiness_not_called "${case6_name}" "${case6_dir}/readiness.log" \
     && assert_stderr_contains "${case6_name}" "${stderr}" 'canonical claw is missing' \
     && assert_stderr_contains "${case6_name}" "${stderr}" 'does NOT fall back to PATH or to ~/.cargo/bin/claw' \
     && assert_fake_not_called "${case6_name}" "${log}" \
     && assert_no_decoy_executed "${case6_name}" "${log}"; then
    pass_case "${case6_name}"
  fi
fi

# ---------- case 7: arg_passthrough ----------
case7_name="arg_passthrough"
case7_dir="$(stage_layout "${case7_name}")"
install_canonical "${case7_dir}" current
if run_case "${case7_name}" "${case7_dir}" 0 -- --model fast prompt "say x" >/dev/null; then
  log="${case7_dir}/fake_claw.log"
  if assert_log_contains "${case7_name}" "${log}" 'ARGC=4' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=--model' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=fast' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=prompt' \
     && assert_log_contains "${case7_name}" "${log}" 'ARG=say x'; then
    pass_case "${case7_name}"
  fi
fi

# ---------- case 8: canonical_symlink_refuses ----------
case8_name="canonical_symlink_refuses"
case8_dir="$(stage_layout "${case8_name}")"
install_canonical "${case8_dir}" symlink
if run_case "${case8_name}" "${case8_dir}" 5 -- prompt "should refuse" >/dev/null; then
  stderr="${case8_dir}/wrapper.stderr"
  log="${case8_dir}/fake_claw.log"
  if assert_stderr_contains "${case8_name}" "${stderr}" 'invalid topology' \
     && assert_stderr_contains "${case8_name}" "${stderr}" 'never a symlink to a Cargo target' \
     && assert_fake_not_called "${case8_name}" "${log}"; then
    if [[ ! -L "${case8_dir}/home/.local/bin/claw" ]]; then
      fail_case "${case8_name}" "the wrapper altered the canonical symlink" \
        /dev/null "${stderr}" "${log}"
    else
      pass_case "${case8_name}"
    fi
  fi
fi

# ---------- case 9: canonical_stale_refuses ----------
case9_name="canonical_stale_refuses"
case9_dir="$(stage_layout "${case9_name}")"
install_canonical "${case9_dir}" stale
if run_case "${case9_name}" "${case9_dir}" 6 -- prompt "should refuse" >/dev/null; then
  stderr="${case9_dir}/wrapper.stderr"
  log="${case9_dir}/fake_claw.log"
  if assert_readiness_not_called "${case9_name}" "${case9_dir}/readiness.log" \
     && assert_stderr_contains "${case9_name}" "${stderr}" 'refusing to run a STALE canonical claw' \
     && assert_stderr_contains "${case9_name}" "${stderr}" 'claw-canonical-refresh' \
     && assert_fake_not_called "${case9_name}" "${log}" \
     && assert_no_decoy_executed "${case9_name}" "${log}"; then
    pass_case "${case9_name}"
  fi
fi

# ---------- case 10: canonical_stale_explicit_override_runs_with_warning ----------
case10_name="canonical_stale_explicit_override_runs_with_warning"
case10_dir="$(stage_layout "${case10_name}")"
install_canonical "${case10_dir}" stale
if run_case "${case10_name}" "${case10_dir}" 0 \
     "CLAW_SIDESTACK_ALLOW_STALE=1" -- --model fast prompt "explicitly allowed" >/dev/null; then
  stderr="${case10_dir}/wrapper.stderr"
  log="${case10_dir}/fake_claw.log"
  if assert_stderr_contains "${case10_name}" "${stderr}" 'WARNING — canonical claw is STALE' \
     && assert_log_contains "${case10_name}" "${log}" 'IDENTITY=CANONICAL_STALE' \
     && assert_no_decoy_executed "${case10_name}" "${log}"; then
    pass_case "${case10_name}"
  fi
fi

# ---------- case 11: unknown_base_warns_and_runs ----------
case11_name="unknown_base_warns_and_runs"
case11_dir="$(stage_layout "${case11_name}")"
install_canonical "${case11_dir}" current
git -C "${case11_dir}/root" update-ref -d refs/remotes/origin/main
if run_case "${case11_name}" "${case11_dir}" 0 -- --model fast prompt "unknown base" >/dev/null; then
  stderr="${case11_dir}/wrapper.stderr"
  log="${case11_dir}/fake_claw.log"
  if assert_stderr_contains "${case11_name}" "${stderr}" 'freshness could not be determined' \
     && assert_log_contains "${case11_name}" "${log}" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${case11_name}" "${log}"; then
    pass_case "${case11_name}"
  fi
fi

# ---------- case 12: law1_refusal_precedes_canonical_checks ----------
# A :11434 profile must be refused before the wrapper ever looks at canonical
# topology or freshness — even when the canonical executable is missing.
case12_name="law1_refusal_precedes_canonical_checks"
case12_dir="$(stage_layout "${case12_name}" 'export OPENAI_BASE_URL="http://127.0.0.1:11434/v1"
export OPENAI_API_KEY="local"')"
install_canonical "${case12_dir}" missing
if run_case "${case12_name}" "${case12_dir}" 3 -- prompt "should refuse" >/dev/null; then
  stderr="${case12_dir}/wrapper.stderr"
  log="${case12_dir}/fake_claw.log"
  if assert_readiness_not_called "${case12_name}" "${case12_dir}/readiness.log" \
     && assert_stderr_contains "${case12_name}" "${stderr}" 'LAW 1' \
     && assert_fake_not_called "${case12_name}" "${log}"; then
    if grep -Fq 'canonical claw is missing' "${stderr}"; then
      fail_case "${case12_name}" "canonical checks ran before the LAW 1 refusal" \
        /dev/null "${stderr}" "${log}"
    else
      pass_case "${case12_name}"
    fi
  fi
fi

# ---------- case 13: non_executable_canonical_refuses ----------
case13_name="non_executable_canonical_refuses"
case13_dir="$(stage_layout "${case13_name}")"
install_canonical "${case13_dir}" nonexec
if run_case "${case13_name}" "${case13_dir}" 5 -- prompt "should refuse" >/dev/null; then
  stderr="${case13_dir}/wrapper.stderr"
  log="${case13_dir}/fake_claw.log"
  if assert_readiness_not_called "${case13_name}" "${case13_dir}/readiness.log" \
     && assert_stderr_contains "${case13_name}" "${stderr}" 'invalid topology' \
     && assert_fake_not_called "${case13_name}" "${log}" \
     && assert_no_decoy_executed "${case13_name}" "${log}"; then
    pass_case "${case13_name}"
  fi
fi

# ---------- case 14: status_helper_missing_refuses ----------
case14_name="status_helper_missing_refuses"
case14_dir="$(stage_layout "${case14_name}")"
install_canonical "${case14_dir}" current
rm -f "${case14_dir}/root/scripts/claw-canonical-status"
if run_case "${case14_name}" "${case14_dir}" 7 -- prompt "should refuse" >/dev/null; then
  stderr="${case14_dir}/wrapper.stderr"
  log="${case14_dir}/fake_claw.log"
  if assert_readiness_not_called "${case14_name}" "${case14_dir}/readiness.log" \
     && assert_stderr_contains "${case14_name}" "${stderr}" 'canonical status helper missing' \
     && assert_fake_not_called "${case14_name}" "${log}" \
     && assert_no_decoy_executed "${case14_name}" "${log}"; then
    pass_case "${case14_name}"
  fi
fi

# ---------- case 15: a short-SHA canonical is unproven, not current ----------
#
# The binary reports a 7-character prefix of the fixture HEAD. Before the
# exact-match repair, prefix comparison called that CURRENT and ran it.
case15_name="canonical_reporting_a_short_sha_prefix_refuses_as_stale"
case15_dir="$(stage_layout "${case15_name}")"
install_canonical "${case15_dir}" stale_prefix
if run_case "${case15_name}" "${case15_dir}" 6 -- prompt "should refuse" >/dev/null; then
  stderr="${case15_dir}/wrapper.stderr"
  log="${case15_dir}/fake_claw.log"
  if assert_readiness_not_called "${case15_name}" "${case15_dir}/readiness.log" \
     && assert_stderr_contains "${case15_name}" "${stderr}" 'refusing to run a STALE canonical claw' \
     && assert_fake_not_called "${case15_name}" "${log}" \
     && assert_no_decoy_executed "${case15_name}" "${log}"; then
    pass_case "${case15_name}"
  fi
fi

# ---------- case 16: the wrapper revalidates topology after the status check ----
#
# The status helper is replaced with a stub that reports CURRENT *and* turns
# the canonical path into a symlink on its way out. The wrapper's own
# revalidation immediately before `exec` is the only thing left that can catch
# that, so this case fails loudly if that second check is ever removed.
case16_name="wrapper_rechecks_topology_after_the_status_helper_returns"
case16_dir="$(stage_layout "${case16_name}")"
install_canonical "${case16_dir}" current
canonical16="${case16_dir}/home/.local/bin/claw"
write_fake_claw "${case16_dir}/swapped-target/claw" "SWAPPED_TARGET" "0000000"
cat > "${case16_dir}/root/scripts/claw-canonical-status" <<STUB
#!/usr/bin/env bash
# Reports CURRENT *and* a supported capability — a maximally permissive status
# answer — then races the canonical path into a symlink. Every earlier gate is
# deliberately satisfied so the final topology revalidation is the only thing
# left that can catch the swap, which is exactly what this case measures.
printf 'canonical topology:  regular-executable\n'
printf 'n6 capability:       supported\n'
printf 'state:               CURRENT\n'
rm -f -- '${canonical16}'
ln -s '${case16_dir}/swapped-target/claw' '${canonical16}'
exit 0
STUB
chmod 0755 "${case16_dir}/root/scripts/claw-canonical-status"
if run_case "${case16_name}" "${case16_dir}" 5 -- --model fast prompt "should refuse" >/dev/null; then
  stderr="${case16_dir}/wrapper.stderr"
  log="${case16_dir}/fake_claw.log"
  if assert_stderr_contains "${case16_name}" "${stderr}" 'canonical claw is a symlink' \
     && assert_stderr_contains "${case16_name}" "${stderr}" 'never a symlink to a Cargo target' \
     && assert_fake_not_called "${case16_name}" "${log}" \
     && assert_no_decoy_executed "${case16_name}" "${log}"; then
    if grep -Fq 'IDENTITY=SWAPPED_TARGET' "${log}"; then
      fail_case "${case16_name}" "the swapped symlink target was executed" \
        /dev/null "${stderr}" "${log}"
    else
      pass_case "${case16_name}"
    fi
  fi
fi

# ---------- case 17: the canonical disappearing after the status check ----------
case17_name="wrapper_refuses_when_canonical_disappears_after_the_status_check"
case17_dir="$(stage_layout "${case17_name}")"
install_canonical "${case17_dir}" current
canonical17="${case17_dir}/home/.local/bin/claw"
cat > "${case17_dir}/root/scripts/claw-canonical-status" <<STUB
#!/usr/bin/env bash
# CURRENT and capable, so nothing before the final existence recheck can refuse.
printf 'n6 capability:       supported\n'
printf 'state:               CURRENT\n'
rm -f -- '${canonical17}'
exit 0
STUB
chmod 0755 "${case17_dir}/root/scripts/claw-canonical-status"
if run_case "${case17_name}" "${case17_dir}" 4 -- --model fast prompt "should refuse" >/dev/null; then
  stderr="${case17_dir}/wrapper.stderr"
  log="${case17_dir}/fake_claw.log"
  if assert_stderr_contains "${case17_name}" "${stderr}" 'canonical claw is missing' \
     && assert_stderr_contains "${case17_name}" "${stderr}" 'does NOT fall back to PATH or to ~/.cargo/bin/claw' \
     && assert_fake_not_called "${case17_name}" "${log}" \
     && assert_no_decoy_executed "${case17_name}" "${log}"; then
    pass_case "${case17_name}"
  fi
fi


# ===========================================================================
# N6 broker readiness gate
#
# The gate runs after the canonical freshness checks and before the final
# topology revalidation, so cases 3-6/9/12-15 above already prove it is never
# consulted when an earlier rule refuses, and cases 16/17 prove the final
# topology recheck still fires after readiness has said yes.
# ===========================================================================

# ---------- case 18: --model deep is queried as its exact upstream model ----
case18_name="readiness_queries_the_exact_deep_alias_model"
case18_dir="$(stage_layout "${case18_name}")"
install_canonical "${case18_dir}" current
write_fake_readiness_client "${case18_dir}" 0 200 \
  '{"ready": true, "reason_code": "READY_SAME_MODEL", "requested_model": "qwen3.5:27b"}'
if run_case "${case18_name}" "${case18_dir}" 0 -- --model deep prompt "hi" >/dev/null; then
  if assert_readiness_called_for "${case18_name}" "${case18_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_readiness_url_is_broker_only "${case18_name}" "${case18_dir}/readiness.log" \
     && assert_log_contains "${case18_name}" "${case18_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${case18_name}"
  fi
fi

# ---------- case 19: explicit model, routing prefix stripped for the wire ----
case19_name="readiness_queries_explicit_model_with_routing_prefix_stripped"
case19_dir="$(stage_layout "${case19_name}")"
install_canonical "${case19_dir}" current
if run_case "${case19_name}" "${case19_dir}" 0 -- --model openai/qwen3:14b prompt "hi" >/dev/null; then
  if assert_readiness_called_for "${case19_name}" "${case19_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_url_is_broker_only "${case19_name}" "${case19_dir}/readiness.log"; then
    if grep -Fq 'requested_model=openai' "${case19_dir}/readiness.log"; then
      fail_case "${case19_name}" "the routing prefix was not stripped before the readiness query" \
        /dev/null /dev/null "${case19_dir}/readiness.log"
    else
      pass_case "${case19_name}"
    fi
  fi
fi

# ---------- case 20: the --model=VALUE form resolves identically ------------
case20_name="readiness_supports_the_model_equals_form"
case20_dir="$(stage_layout "${case20_name}")"
install_canonical "${case20_dir}" current
if run_case "${case20_name}" "${case20_dir}" 0 -- --model=fast prompt "hi" >/dev/null; then
  if assert_readiness_called_for "${case20_name}" "${case20_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_log_contains "${case20_name}" "${case20_dir}/fake_claw.log" 'ARG=--model=fast'; then
    pass_case "${case20_name}"
  fi
fi

# ---------- cases 21-24: every refusal reason_code refuses ------------------
# ready=false means REFUSE, never wait: the wrapper exits 8 immediately, the
# canonical claw is never executed, and the reason_code is surfaced verbatim.
refusal_index=21
for reason in HYPERLIQUID_LANE_RECENTLY_ACTIVE HYPERLIQUID_HOLDER_PROTECTED BROKER_BUSY INSUFFICIENT_VRAM; do
  refusal_name="readiness_refuses_${reason}"
  refusal_dir="$(stage_layout "case${refusal_index}_${reason}")"
  install_canonical "${refusal_dir}" current
  write_fake_readiness_client "${refusal_dir}" 0 200 "$(refusal_body "${reason}")"
  if run_case "${refusal_name}" "${refusal_dir}" 8 -- --model fast prompt "should refuse" >/dev/null; then
    if assert_stderr_contains "${refusal_name}" "${refusal_dir}/wrapper.stderr" \
         'local coding readiness refused' \
       && assert_stderr_contains "${refusal_name}" "${refusal_dir}/wrapper.stderr" \
            'requested model: qwen3:14b' \
       && assert_stderr_contains "${refusal_name}" "${refusal_dir}/wrapper.stderr" \
            "reason_code: ${reason}" \
       && assert_fake_not_called "${refusal_name}" "${refusal_dir}/fake_claw.log" \
       && assert_no_decoy_executed "${refusal_name}" "${refusal_dir}/fake_claw.log"; then
      pass_case "${refusal_name}"
    fi
  fi
  refusal_index=$((refusal_index + 1))
done

# ---------- cases 25-30: every unusable answer fails closed -----------------
# Each entry is <label>|<curl_exit>|<http_code>|<body>. In all six the wrapper
# must exit 9 and must NOT execute the canonical claw.
fail_closed_index=25
while IFS='|' read -r fc_label fc_exit fc_code fc_body; do
  [[ -n "${fc_label}" ]] || continue
  fc_name="readiness_fails_closed_on_${fc_label}"
  fc_dir="$(stage_layout "case${fail_closed_index}_${fc_label}")"
  install_canonical "${fc_dir}" current
  write_fake_readiness_client "${fc_dir}" "${fc_exit}" "${fc_code}" "${fc_body}"
  if run_case "${fc_name}" "${fc_dir}" 9 -- --model fast prompt "should refuse" >/dev/null; then
    if assert_stderr_contains "${fc_name}" "${fc_dir}/wrapper.stderr" \
         'local coding readiness could not be established' \
       && assert_fake_not_called "${fc_name}" "${fc_dir}/fake_claw.log" \
       && assert_no_decoy_executed "${fc_name}" "${fc_dir}/fake_claw.log"; then
      pass_case "${fc_name}"
    fi
  fi
  fail_closed_index=$((fail_closed_index + 1))
done <<'FAILCLOSED'
connection_failure|7|000|
timeout|28|000|
non_2xx|0|503|{"detail": "unavailable"}
malformed_json|0|200|{"ready": true, "reason_code":
missing_ready_field|0|200|{"reason_code": "READY_SAME_MODEL", "requested_model": "qwen3:14b"}
answer_for_a_different_model|0|200|{"ready": true, "reason_code": "READY_SAME_MODEL", "requested_model": "devstral-small-2:latest"}
FAILCLOSED

# ---------- case 31: a duplicated `ready` key is ambiguous, so it refuses ----
case31_name="readiness_fails_closed_on_duplicate_ready_keys"
case31_dir="$(stage_layout "${case31_name}")"
install_canonical "${case31_dir}" current
write_fake_readiness_client "${case31_dir}" 0 200 \
  '{"ready": false, "ready": true, "reason_code": "READY_SAME_MODEL", "requested_model": "qwen3:14b"}'
if run_case "${case31_name}" "${case31_dir}" 9 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_stderr_contains "${case31_name}" "${case31_dir}/wrapper.stderr" \
       'local coding readiness could not be established' \
     && assert_fake_not_called "${case31_name}" "${case31_dir}/fake_claw.log"; then
    pass_case "${case31_name}"
  fi
fi

# ---------- case 32: a non-boolean `ready` is not an answer -----------------
case32_name="readiness_fails_closed_on_non_boolean_ready"
case32_dir="$(stage_layout "${case32_name}")"
install_canonical "${case32_dir}" current
write_fake_readiness_client "${case32_dir}" 0 200 \
  '{"ready": "true", "reason_code": "READY_SAME_MODEL", "requested_model": "qwen3:14b"}'
if run_case "${case32_name}" "${case32_dir}" 9 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_fake_not_called "${case32_name}" "${case32_dir}/fake_claw.log"; then
    pass_case "${case32_name}"
  fi
fi

# ---------- case 33: an inference invocation with no --model refuses --------
# `claw` would fall back to the compiled-in DEFAULT_MODEL, which is a cloud
# model, so there is no local upstream model to ask the broker about.
case33_name="inference_without_an_explicit_model_refuses_with_a_hint"
case33_dir="$(stage_layout "${case33_name}")"
install_canonical "${case33_dir}" current
if run_case "${case33_name}" "${case33_dir}" 9 -- prompt "no model given" >/dev/null; then
  if assert_stderr_contains "${case33_name}" "${case33_dir}/wrapper.stderr" \
       'readiness requires an explicit --model for this invocation' \
     && assert_stderr_contains "${case33_name}" "${case33_dir}/wrapper.stderr" 'Example: --model fast' \
     && assert_readiness_not_called "${case33_name}" "${case33_dir}/readiness.log" \
     && assert_fake_not_called "${case33_name}" "${case33_dir}/fake_claw.log" \
     && assert_no_decoy_executed "${case33_name}" "${case33_dir}/fake_claw.log"; then
    pass_case "${case33_name}"
  fi
fi

# ---------- case 34: the bare interactive REPL refuses the same way ---------
case34_name="interactive_repl_without_a_model_refuses"
case34_dir="$(stage_layout "${case34_name}")"
install_canonical "${case34_dir}" current
if run_case "${case34_name}" "${case34_dir}" 9 -- >/dev/null; then
  if assert_stderr_contains "${case34_name}" "${case34_dir}/wrapper.stderr" \
       'readiness requires an explicit --model for this invocation' \
     && assert_readiness_not_called "${case34_name}" "${case34_dir}/readiness.log" \
     && assert_fake_not_called "${case34_name}" "${case34_dir}/fake_claw.log"; then
    pass_case "${case34_name}"
  fi
fi

# ---------- case 35: a bare name with no env alias is not resolvable --------
# `sonnet` would go through claw's built-in cloud alias table or a repo config
# alias; neither is determinable here, so the wrapper refuses instead of
# guessing which upstream model the broker would be asked for.
case35_name="bare_model_name_without_an_env_alias_refuses"
case35_dir="$(stage_layout "${case35_name}")"
install_canonical "${case35_dir}" current
if run_case "${case35_name}" "${case35_dir}" 9 -- --model sonnet prompt "x" >/dev/null; then
  if assert_stderr_contains "${case35_name}" "${case35_dir}/wrapper.stderr" \
       'could not resolve the requested model' \
     && assert_stderr_contains "${case35_name}" "${case35_dir}/wrapper.stderr" \
          'RUSTY_CLAUDE_MODEL_ALIAS__SONNET' \
     && assert_readiness_not_called "${case35_name}" "${case35_dir}/readiness.log" \
     && assert_fake_not_called "${case35_name}" "${case35_dir}/fake_claw.log"; then
    pass_case "${case35_name}"
  fi
fi

# ---------- case 36: a claw config alias makes the model undeterminable -----
# Config aliases can rename ANY model string, and claw consults them whenever
# the env-alias lookup misses, so their presence is a fail-closed condition.
case36_name="config_alias_for_the_requested_model_refuses"
case36_dir="$(stage_layout "${case36_name}")"
install_canonical "${case36_dir}" current
mkdir -p "${case36_dir}/cwd/.claw"
printf '%s\n' '{"aliases": {"qwen3:14b": "devstral-small-2:latest"}}' \
  > "${case36_dir}/cwd/.claw/settings.json"
if run_case "${case36_name}" "${case36_dir}" 9 -- --model qwen3:14b prompt "x" >/dev/null; then
  if assert_stderr_contains "${case36_name}" "${case36_dir}/wrapper.stderr" \
       'redefined by a claw config alias' \
     && assert_readiness_not_called "${case36_name}" "${case36_dir}/readiness.log" \
     && assert_fake_not_called "${case36_name}" "${case36_dir}/fake_claw.log"; then
    pass_case "${case36_name}"
  fi
fi

# ---------- cases 37+: proven non-inference commands bypass the gate --------
# Each of these dispatches to a local printer/reporter in the CLI and never
# constructs a LiveCli, so there is no session for the broker to gate. `plan`
# is included on a narrower proof: its dispatcher makes no provider call, and
# every inference step it runs is spawned back through THIS wrapper as
# `--model fast … prompt …`, where it IS gated.
noninference_index=37
for noninference_cmd in --help --version version status sandbox doctor acp state init config diff export system-prompt dump-manifests bootstrap-plan agents mcp skills plugins plan; do
  ni_name="non_inference_bypasses_readiness_${noninference_cmd//-/_}"
  ni_dir="$(stage_layout "case${noninference_index}_${noninference_cmd#--}")"
  install_canonical "${ni_dir}" current
  if run_case "${ni_name}" "${ni_dir}" 0 -- "${noninference_cmd}" >/dev/null; then
    if assert_readiness_not_called "${ni_name}" "${ni_dir}/readiness.log" \
       && assert_no_decoy_executed "${ni_name}" "${ni_dir}/fake_claw.log"; then
      pass_case "${ni_name}"
    fi
  fi
  noninference_index=$((noninference_index + 1))
done

# ---------- case 58: `<subcommand> --help` still bypasses ------------------
case58_name="subcommand_help_bypasses_readiness"
case58_dir="$(stage_layout "${case58_name}")"
install_canonical "${case58_dir}" current
if run_case "${case58_name}" "${case58_dir}" 0 -- status --help >/dev/null; then
  if assert_readiness_not_called "${case58_name}" "${case58_dir}/readiness.log"; then
    pass_case "${case58_name}"
  fi
fi

# ---------- case 59: a bare prompt with --help in it is still inference -----
# `claw explain this --help` is a shorthand prompt, not a help request, so it
# must NOT be waved through by a naive "--help appears in argv" check.
case59_name="shorthand_prompt_containing_help_is_still_gated"
case59_dir="$(stage_layout "${case59_name}")"
install_canonical "${case59_dir}" current
if run_case "${case59_name}" "${case59_dir}" 9 -- explain this --help >/dev/null; then
  if assert_stderr_contains "${case59_name}" "${case59_dir}/wrapper.stderr" \
       'readiness requires an explicit --model for this invocation' \
     && assert_fake_not_called "${case59_name}" "${case59_dir}/fake_claw.log"; then
    pass_case "${case59_name}"
  fi
fi

# ---------- case 60: the readiness query carries no credential -------------
# The endpoint is unauthenticated. The API key must never reach the readiness
# client's argument list, and must never be echoed to stderr.
case60_name="readiness_query_never_carries_the_api_key"
case60_dir="$(stage_layout "${case60_name}" 'export OPENAI_BASE_URL="http://localhost:11435/v1"
export OPENAI_API_KEY="sk-n6-sentinel-must-not-leak"
export RUSTY_CLAUDE_MODEL_ALIAS__FAST="qwen3:14b"')"
install_canonical "${case60_dir}" current
if run_case "${case60_name}" "${case60_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  if assert_readiness_called_for "${case60_name}" "${case60_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_url_is_broker_only "${case60_name}" "${case60_dir}/readiness.log"; then
    if grep -Fq 'sk-n6-sentinel-must-not-leak' "${case60_dir}/readiness.log"; then
      fail_case "${case60_name}" "the API key reached the readiness client's arguments" \
        /dev/null /dev/null "${case60_dir}/readiness.log"
    elif grep -Fq 'sk-n6-sentinel-must-not-leak' "${case60_dir}/wrapper.stderr"; then
      fail_case "${case60_name}" "the API key was printed to stderr" \
        /dev/null "${case60_dir}/wrapper.stderr" /dev/null
    else
      pass_case "${case60_name}"
    fi
  fi
fi

# ---------- case 61: the readiness origin follows the validated base URL ----
# The origin is derived from the already-LAW-1-validated OPENAI_BASE_URL, so a
# `localhost` profile must produce a `localhost` readiness URL — never a second
# hard-coded host, and never :11434.
case61_name="readiness_origin_is_derived_from_the_validated_base_url"
case61_dir="$(stage_layout "${case61_name}" 'export OPENAI_BASE_URL="http://localhost:11435/v1"
export OPENAI_API_KEY="local"
export RUSTY_CLAUDE_MODEL_ALIAS__FAST="qwen3:14b"')"
install_canonical "${case61_dir}" current
if run_case "${case61_name}" "${case61_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  if assert_readiness_url_is_broker_only "${case61_name}" "${case61_dir}/readiness.log"; then
    if ! grep -Fq 'ARG=http://localhost:11435/status/n6_planner_ready?' "${case61_dir}/readiness.log"; then
      fail_case "${case61_name}" "the readiness URL did not follow the profile's localhost origin" \
        /dev/null /dev/null "${case61_dir}/readiness.log"
    else
      pass_case "${case61_name}"
    fi
  fi
fi

# ===========================================================================
# Repair coverage: tail-aware classification and model-binding proof.
#
# The first candidate classified on the LEADING VERB alone. That is wrong for
# every subcommand whose dispatch depends on its tail, and it silently waved
# through real `CliAction::Prompt` invocations. Each case below pins one shape
# against the actual Rust dispatch it mirrors.
# ===========================================================================

# ---------- skills: tail decides, not the verb -----------------------------
# `classify_skills_slash_command` (rust/crates/commands/src/lib.rs) keeps the
# invocation local ONLY for the whole-string forms below. Everything else is
# SkillSlashDispatch::Invoke -> CliAction::Prompt.
skills_local_index=62
while IFS='|' read -r skills_args skills_label; do
  [[ -n "${skills_label}" ]] || continue
  sl_name="skills_local_form_${skills_label}"
  sl_dir="$(stage_layout "case${skills_local_index}_${skills_label}")"
  install_canonical "${sl_dir}" current
  # shellcheck disable=SC2086  # deliberate word splitting: these are argv words
  if run_case "${sl_name}" "${sl_dir}" 0 -- skills ${skills_args} >/dev/null; then
    if assert_readiness_not_called "${sl_name}" "${sl_dir}/readiness.log" \
       && assert_no_decoy_executed "${sl_name}" "${sl_dir}/fake_claw.log"; then
      pass_case "${sl_name}"
    fi
  fi
  skills_local_index=$((skills_local_index + 1))
done <<'SKILLS_LOCAL'
|bare
list|list
help|help
-h|dash_h
--help|dash_dash_help
install|install
install ./some/path|install_target
SKILLS_LOCAL

# Inference-capable skills forms: each must query readiness for the EXACT
# resolved model before the canonical claw is allowed to run.
skills_gated_index=69
while IFS='|' read -r skills_args skills_label; do
  [[ -n "${skills_label}" ]] || continue
  sg_name="skills_inference_form_is_gated_${skills_label}"
  sg_dir="$(stage_layout "case${skills_gated_index}_${skills_label}")"
  install_canonical "${sg_dir}" current
  # shellcheck disable=SC2086  # deliberate word splitting: these are argv words
  if run_case "${sg_name}" "${sg_dir}" 0 -- --model fast skills ${skills_args} >/dev/null; then
    if assert_readiness_called_for "${sg_name}" "${sg_dir}/readiness.log" 'qwen3%3A14b' \
       && assert_readiness_url_is_broker_only "${sg_name}" "${sg_dir}/readiness.log" \
       && assert_no_decoy_executed "${sg_name}" "${sg_dir}/fake_claw.log"; then
      pass_case "${sg_name}"
    fi
  fi
  skills_gated_index=$((skills_gated_index + 1))
done <<'SKILLS_GATED'
help overview|help_overview
list extra|list_extra
some-skill|bare_skill
some-skill arg1 arg2|skill_with_args
installer|installer_is_not_the_install_subcommand
SKILLS_GATED

# ---------- the blocking regression, stated as a refusal -------------------
# THE case the first candidate failed: with the broker refusing, the original
# wrapper still executed canonical claw for `skills help overview` because it
# never asked. The repaired wrapper must refuse with exit 8 and never exec.
case74_name="skills_help_overview_is_refused_when_the_broker_says_not_ready"
case74_dir="$(stage_layout "${case74_name}")"
install_canonical "${case74_dir}" current
write_fake_readiness_client "${case74_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${case74_name}" "${case74_dir}" 8 -- --model fast skills help overview >/dev/null; then
  if assert_stderr_contains "${case74_name}" "${case74_dir}/wrapper.stderr" \
       'local coding readiness refused' \
     && assert_stderr_contains "${case74_name}" "${case74_dir}/wrapper.stderr" \
          'reason_code: HYPERLIQUID_ACTIVE' \
     && assert_fake_not_called "${case74_name}" "${case74_dir}/fake_claw.log"; then
    pass_case "${case74_name}"
  fi
fi

# ---------- `-p` consumes the rest of argv as a PROMPT ---------------------
# `parse_args_with_terminal` returns CliAction::Prompt for `-p`, joining every
# remaining token into the prompt text. `-p status` is a prompt that reads
# "status", NOT the local status report, so a positional scan must never see
# "status" here and bypass.
case75_name="dash_p_with_a_local_looking_word_is_still_inference"
case75_dir="$(stage_layout "${case75_name}")"
install_canonical "${case75_dir}" current
if run_case "${case75_name}" "${case75_dir}" 0 -- --model fast -p status >/dev/null; then
  if assert_readiness_called_for "${case75_name}" "${case75_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_no_decoy_executed "${case75_name}" "${case75_dir}/fake_claw.log"; then
    pass_case "${case75_name}"
  fi
fi

case76_name="dash_p_without_a_model_refuses_before_exec"
case76_dir="$(stage_layout "${case76_name}")"
install_canonical "${case76_dir}" current
if run_case "${case76_name}" "${case76_dir}" 9 -- -p doctor >/dev/null; then
  if assert_stderr_contains "${case76_name}" "${case76_dir}/wrapper.stderr" \
       'readiness requires an explicit --model for this invocation' \
     && assert_fake_not_called "${case76_name}" "${case76_dir}/fake_claw.log"; then
    pass_case "${case76_name}"
  fi
fi

# `--model` is a value-taking flag, so a `-p` sitting in its VALUE slot is a
# model string, not the prompt flag: it must NOT force the inference verdict.
# The real positional here is `status`, a proven-local report, so this is a
# local bypass. (The CLI itself then rejects `-p` as a model string; either
# way no provider is reached.)
case77_name="dash_p_as_a_flag_value_is_not_treated_as_the_prompt_flag"
case77_dir="$(stage_layout "${case77_name}")"
install_canonical "${case77_dir}" current
if run_case "${case77_name}" "${case77_dir}" 0 -- --model -p status >/dev/null; then
  if assert_readiness_not_called "${case77_name}" "${case77_dir}/readiness.log" \
     && assert_no_decoy_executed "${case77_name}" "${case77_dir}/fake_claw.log"; then
    pass_case "${case77_name}"
  fi
fi

# ---------- `--resume` continues a real session ----------------------------
# `parse_resume_args` yields ResumeSession / ResumeRepl. A session reference
# that happens to read like a local verb must not be scanned as a subcommand.
case78_name="resume_with_a_local_looking_reference_is_still_inference"
case78_dir="$(stage_layout "${case78_name}")"
install_canonical "${case78_dir}" current
if run_case "${case78_name}" "${case78_dir}" 0 -- --model fast --resume status >/dev/null; then
  if assert_readiness_called_for "${case78_name}" "${case78_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_no_decoy_executed "${case78_name}" "${case78_dir}/fake_claw.log"; then
    pass_case "${case78_name}"
  fi
fi

# ---------- `task run` fails closed ----------------------------------------
# `CliAction::TaskRun` carries ONLY the spec path. The global `--model` is
# never propagated into it. The bridge reads `spec.model` (default
# "fast-default") and the EFFECTIVE upstream model is whatever the broker
# resolves that to -- readable only from the broker's response, after
# inference. So the exact model cannot be proven here, and gating on the
# global --model would query the WRONG model.
case79_name="task_run_refuses_before_exec_instead_of_guessing_the_model"
case79_dir="$(stage_layout "${case79_name}")"
install_canonical "${case79_dir}" current
if run_case "${case79_name}" "${case79_dir}" 9 -- task run ./task.json >/dev/null; then
  if assert_stderr_contains "${case79_name}" "${case79_dir}/wrapper.stderr" \
       'takes its model from the task spec' \
     && assert_readiness_not_called "${case79_name}" "${case79_dir}/readiness.log" \
     && assert_fake_not_called "${case79_name}" "${case79_dir}/fake_claw.log"; then
    pass_case "${case79_name}"
  fi
fi

# The decisive one: a global --model must NOT satisfy the task-run gate, and
# must NOT cause a readiness query for a model the task will never use.
case80_name="task_run_with_a_global_model_still_refuses_and_queries_nothing"
case80_dir="$(stage_layout "${case80_name}")"
install_canonical "${case80_dir}" current
if run_case "${case80_name}" "${case80_dir}" 9 -- --model fast task run ./task.json >/dev/null; then
  if assert_readiness_not_called "${case80_name}" "${case80_dir}/readiness.log" \
     && assert_fake_not_called "${case80_name}" "${case80_dir}/fake_claw.log"; then
    pass_case "${case80_name}"
  fi
fi

# ---------- `plan run` child-step gating depends on the wrapper ------------
# `build_claw_command` spawns every model-bearing step as
# `<wrapper> --model fast ... prompt <text>`, so the steps are gated by THIS
# wrapper -- but only while the runner uses its default wrapper.
case81_name="plan_run_without_a_wrapper_override_bypasses_and_gates_its_children"
case81_dir="$(stage_layout "${case81_name}")"
install_canonical "${case81_dir}" current
if run_case "${case81_name}" "${case81_dir}" 0 -- plan run ./plan.yaml >/dev/null; then
  if assert_readiness_not_called "${case81_name}" "${case81_dir}/readiness.log" \
     && assert_no_decoy_executed "${case81_name}" "${case81_dir}/fake_claw.log"; then
    pass_case "${case81_name}"
  fi
fi

case82_name="plan_run_with_a_foreign_wrapper_override_refuses_before_exec"
case82_dir="$(stage_layout "${case82_name}")"
install_canonical "${case82_dir}" current
if run_case "${case82_name}" "${case82_dir}" 9 -- \
     plan run ./plan.yaml --wrapper /usr/bin/claw >/dev/null; then
  if assert_stderr_contains "${case82_name}" "${case82_dir}/wrapper.stderr" \
       'not proven to pass the readiness gate' \
     && assert_fake_not_called "${case82_name}" "${case82_dir}/fake_claw.log"; then
    pass_case "${case82_name}"
  fi
fi

case83_name="plan_run_with_the_equals_form_wrapper_override_also_refuses"
case83_dir="$(stage_layout "${case83_name}")"
install_canonical "${case83_dir}" current
if run_case "${case83_name}" "${case83_dir}" 9 -- \
     plan run ./plan.yaml --wrapper=/usr/bin/claw >/dev/null; then
  if assert_fake_not_called "${case83_name}" "${case83_dir}/fake_claw.log"; then
    pass_case "${case83_name}"
  fi
fi

# Pointing --wrapper at THIS wrapper preserves the child-gating proof, so it
# is allowed rather than refused.
case84_name="plan_run_pointing_the_wrapper_override_at_this_wrapper_is_allowed"
case84_dir="$(stage_layout "${case84_name}")"
install_canonical "${case84_dir}" current
if run_case "${case84_name}" "${case84_dir}" 0 -- \
     plan run ./plan.yaml --wrapper "${case84_dir}/root/scripts/claw-sidestack-local" >/dev/null; then
  if assert_readiness_not_called "${case84_name}" "${case84_dir}/readiness.log" \
     && assert_no_decoy_executed "${case84_name}" "${case84_dir}/fake_claw.log"; then
    pass_case "${case84_name}"
  fi
fi

# A `--wrapper` on a NON-run plan subcommand is not a child-spawning path.
case85_name="plan_status_is_local_even_with_a_wrapper_looking_argument"
case85_dir="$(stage_layout "${case85_name}")"
install_canonical "${case85_dir}" current
if run_case "${case85_name}" "${case85_dir}" 0 -- plan status . >/dev/null; then
  if assert_readiness_not_called "${case85_name}" "${case85_dir}/readiness.log"; then
    pass_case "${case85_name}"
  fi
fi

# ---------- help-shaped prompts stay gated ---------------------------------
case86_name="prompt_whose_text_is_skills_dash_dash_help_is_still_gated"
case86_dir="$(stage_layout "${case86_name}")"
install_canonical "${case86_dir}" current
if run_case "${case86_name}" "${case86_dir}" 0 -- --model fast prompt "skills --help" >/dev/null; then
  if assert_readiness_called_for "${case86_name}" "${case86_dir}/readiness.log" 'qwen3%3A14b'; then
    pass_case "${case86_name}"
  fi
fi

# A single shorthand argument that merely CONTAINS "skills" is a prompt.
case87_name="shorthand_prompt_containing_the_word_skills_is_gated"
case87_dir="$(stage_layout "${case87_name}")"
install_canonical "${case87_dir}" current
if run_case "${case87_name}" "${case87_dir}" 0 -- --model fast "skills --help" >/dev/null; then
  if assert_readiness_called_for "${case87_name}" "${case87_dir}/readiness.log" 'qwen3%3A14b'; then
    pass_case "${case87_name}"
  fi
fi

# ---------- global flags on either side of the subcommand ------------------
# The CLI consumes its global flags wherever they appear, so a flag AFTER the
# subcommand must still be stripped before the positional scan -- otherwise a
# flag value could be mistaken for the subcommand.
case88_name="global_model_flag_after_the_subcommand_is_still_resolved"
case88_dir="$(stage_layout "${case88_name}")"
install_canonical "${case88_dir}" current
if run_case "${case88_name}" "${case88_dir}" 0 -- prompt "hi" --model fast >/dev/null; then
  if assert_readiness_called_for "${case88_name}" "${case88_dir}/readiness.log" 'qwen3%3A14b'; then
    pass_case "${case88_name}"
  fi
fi

# A flag VALUE that reads like a local subcommand must never be scanned as one.
case89_name="a_flag_value_that_looks_like_a_local_subcommand_is_not_a_subcommand"
case89_dir="$(stage_layout "${case89_name}")"
install_canonical "${case89_dir}" current
if run_case "${case89_name}" "${case89_dir}" 0 -- \
     --model fast --allowed-tools doctor prompt "hi" >/dev/null; then
  if assert_readiness_called_for "${case89_name}" "${case89_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_no_decoy_executed "${case89_name}" "${case89_dir}/fake_claw.log"; then
    pass_case "${case89_name}"
  fi
fi

# ---------- the readiness GET stays bounded and unhijackable ---------------
# Verified against real curl 8.5.0 in this lane: `--max-filesize 65536` aborts
# with exit 63 for both a Content-Length body and a chunked one, which the
# wrapper turns into a transport failure and therefore a fail-closed exit 9.
# `--disable` (first argument, where curl requires it) suppresses ~/.curlrc,
# and `--noproxy '*'` was shown to be load-bearing: without it curl sends this
# GET to a proxy named by HTTP_PROXY/ALL_PROXY.
case90_name="readiness_get_is_bounded_direct_and_ignores_curlrc"
case90_dir="$(stage_layout "${case90_name}")"
install_canonical "${case90_dir}" current
if run_case "${case90_name}" "${case90_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  c90_log="${case90_dir}/readiness.log"
  if ! grep -Fq 'ARG=--max-filesize' "${c90_log}"; then
    fail_case "${case90_name}" "the readiness GET is not size-bounded" /dev/null /dev/null "${c90_log}"
  elif [[ "$(grep -m1 -n 'ARG=' "${c90_log}" | cut -d: -f2-)" != "ARG=--disable" ]]; then
    fail_case "${case90_name}" "--disable is not curl's first argument, so ~/.curlrc is still read" \
      /dev/null /dev/null "${c90_log}"
  elif ! grep -Fq 'ARG=--noproxy' "${c90_log}"; then
    fail_case "${case90_name}" "the readiness GET can be redirected by a proxy env var" \
      /dev/null /dev/null "${c90_log}"
  elif grep -Eq 'ARG=(--location|-L|--netrc|-n|--netrc-file|-X|--request|-d|--data.*)$' "${c90_log}"; then
    fail_case "${case90_name}" "the readiness GET follows redirects, reads .netrc, or is not a plain GET" \
      /dev/null /dev/null "${c90_log}"
  else
    pass_case "${case90_name}"
  fi
fi

# ---------- cases 91-96: the curl capability floor ACCEPTS >= 8.4.0 --------
# `--max-filesize` is only guaranteed to abort an in-progress transfer of
# unknown length from curl 8.4.0 onward, so the wrapper requires that version
# before it will issue the readiness GET. Every version at or above the floor
# must still work exactly as before.
#
# 8.10.0 and 9.0.0 are the lexicographic traps: compared as STRINGS, "8.10.0"
# and "8.4.0" order the wrong way round, so a string compare would reject a
# curl that satisfies the floor. They must be accepted.
curl_floor_accept_case() {
  local label="$1" version_line="$2"
  local name="curl_floor_accepts_${label}"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  write_fake_readiness_client "${dir}" 0 200 "${READY_QWEN3_14B}" "${version_line}"
  if run_case "${name}" "${dir}" 0 -- --model fast prompt "hi" >/dev/null; then
    if assert_readiness_called_for "${name}" "${dir}/readiness.log" 'qwen3%3A14b' \
       && assert_log_contains "${name}" "${dir}/fake_claw.log" "IDENTITY=CANONICAL" \
       && assert_no_decoy_executed "${name}" "${dir}/fake_claw.log"; then
      pass_case "${name}"
    fi
  fi
}

curl_floor_accept_case "8_4_0"  'curl 8.4.0 (x86_64-pc-linux-gnu) libcurl/8.4.0 OpenSSL/3.0.13'
curl_floor_accept_case "8_4_1"  'curl 8.4.1 (x86_64-pc-linux-gnu) libcurl/8.4.1 OpenSSL/3.0.13'
curl_floor_accept_case "8_5_0"  'curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0 OpenSSL/3.0.13'
curl_floor_accept_case "8_10_0" 'curl 8.10.0 (x86_64-pc-linux-gnu) libcurl/8.10.0 OpenSSL/3.0.13'
curl_floor_accept_case "8_10_1" 'curl 8.10.1 (x86_64-pc-linux-gnu) libcurl/8.10.1 OpenSSL/3.0.13'
curl_floor_accept_case "9_0_0"  'curl 9.0.0 (x86_64-pc-linux-gnu) libcurl/9.0.0 OpenSSL/3.0.13'

# ---------- cases 97-103: the curl capability floor REFUSES otherwise ------
# Every rejected shape must fail closed identically: exit 9, ZERO readiness
# requests, and no canonical exec. On an old curl the contract is "refuse
# before transfer", never "transfer with an unreliable bound".
curl_floor_reject_case() {
  local label="$1" version_line="$2" version_exit="${3-0}"
  local name="curl_floor_refuses_${label}"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  write_fake_readiness_client "${dir}" 0 200 "${READY_QWEN3_14B}" "${version_line}" "${version_exit}"
  if run_case "${name}" "${dir}" 9 -- --model fast prompt "hi" >/dev/null; then
    if assert_curl_floor_refusal "${name}" "${dir}"; then
      if ! grep -Fq 'curl 8.4.0 or newer is required' "${dir}/wrapper.stderr"; then
        fail_case "${name}" "the refusal does not name the required curl version" \
          /dev/null "${dir}/wrapper.stderr" /dev/null
      else
        pass_case "${name}"
      fi
    fi
  fi
}

curl_floor_reject_case "8_3_999" 'curl 8.3.999 (x86_64-pc-linux-gnu) libcurl/8.3.999'
curl_floor_reject_case "8_3_0"   'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0'
curl_floor_reject_case "7_88_1"  'curl 7.88.1 (x86_64-pc-linux-gnu) libcurl/7.88.1'
curl_floor_reject_case "7_68_0"  'curl 7.68.0 (x86_64-pc-linux-gnu) libcurl/7.68.0'
curl_floor_reject_case "malformed_banner" 'curl banana (x86_64-pc-linux-gnu) libcurl/banana'
curl_floor_reject_case "empty_banner" ''
curl_floor_reject_case "version_command_failure" 'curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0' 1

# ---------- cases 104-106: partial and decorated versions are UNKNOWN ------
# A two-component (`8.4`) or suffixed (`8.6.0-DEV`) banner cannot be ordered
# against the floor without guessing, so it is treated as unknown and refuses.
# Unknown == fail closed; the bound is never weakened on a maybe.
curl_floor_reject_case "partial_version" 'curl 8.4 (x86_64-pc-linux-gnu) libcurl/8.4'
curl_floor_reject_case "dev_suffix" 'curl 8.6.0-DEV (x86_64-pc-linux-gnu) libcurl/8.6.0-DEV'
curl_floor_reject_case "not_curl_banner" 'wget 1.21.4 (linux-gnu)'

# ---------- case 107: the 8.3.999 / 8.4.0 boundary is exact ----------------
# The discriminating proof: the highest rejected version and the lowest
# accepted version differ by the smallest step the floor recognises, and they
# land on opposite sides of it.
case107_name="curl_floor_boundary_8_3_999_rejects_and_8_4_0_accepts"
c107_low_dir="$(stage_layout "${case107_name}_low")"
install_canonical "${c107_low_dir}" current
write_fake_readiness_client "${c107_low_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 8.3.999 (x86_64-pc-linux-gnu) libcurl/8.3.999'
c107_high_dir="$(stage_layout "${case107_name}_high")"
install_canonical "${c107_high_dir}" current
write_fake_readiness_client "${c107_high_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 8.4.0 (x86_64-pc-linux-gnu) libcurl/8.4.0'
if run_case "${case107_name}_low" "${c107_low_dir}" 9 -- --model fast prompt "hi" >/dev/null \
   && run_case "${case107_name}_high" "${c107_high_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  if assert_curl_floor_refusal "${case107_name}" "${c107_low_dir}" \
     && assert_readiness_called_for "${case107_name}" "${c107_high_dir}/readiness.log" 'qwen3%3A14b'; then
    pass_case "${case107_name}"
  fi
fi

# ---------- case 108: an old curl is refused BEFORE any transfer -----------
# The portability contract, stated positively: even when the endpoint would
# have answered with an oversized unknown-length body, a pre-8.4 curl never
# reaches the request at all. The unreliable-bound path is not merely
# mitigated on old curl — it is unreachable.
case108_name="pre_84_curl_never_reaches_an_oversized_unknown_length_body"
case108_dir="$(stage_layout "${case108_name}")"
install_canonical "${case108_dir}" current
# A body far past the 64 KiB bound, served by a curl that predates enforcement.
write_fake_readiness_client "${case108_dir}" 0 200 "$(head -c 200000 /dev/zero | tr '\0' 'x')" \
  'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0'
if run_case "${case108_name}" "${case108_dir}" 9 -- --model fast prompt "hi" >/dev/null; then
  if assert_curl_floor_refusal "${case108_name}" "${case108_dir}"; then
    pass_case "${case108_name}"
  fi
fi

# ---------- case 109: one resolved command path is probed and used ---------
# The capability check is worthless if the wrapper checks one command and then
# re-resolves another from PATH for the GET. A second, PRE-8.4 curl is planted
# later on PATH: it must never be consulted, and the SELF recorded by the
# version probe must equal the SELF recorded by the request. This is a claim
# about PATH selection only, not about executable identity.
case109_name="curl_command_path_binding_probe_and_get_use_one_path"
case109_dir="$(stage_layout "${case109_name}")"
install_canonical "${case109_dir}" current
# A (first on PATH): satisfies the floor and serves the readiness answer.
write_fake_readiness_client "${case109_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0'
# B (later on PATH): reports a pre-8.4 version and records ANY invocation.
mkdir -p "${case109_dir}/decoybin2"
cat > "${case109_dir}/decoybin2/curl" <<CURLB
#!/usr/bin/env bash
printf 'SECOND_CURL_INVOKED=1 args=%s\n' "\$*" >> '${case109_dir}/second_curl.log'
if [[ "\${1:-}" == "--version" ]]; then
  printf 'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0\n'
  exit 0
fi
printf '%s' '${READY_QWEN3_14B}'
printf '\n%s' '200'
CURLB
chmod 0755 "${case109_dir}/decoybin2/curl"
: > "${case109_dir}/second_curl.log"
if run_case "${case109_name}" "${case109_dir}" 0 \
     "PATH=${case109_dir}/decoybin:${case109_dir}/decoybin2:${case109_dir}/home/.cargo/bin:/usr/bin:/bin" \
     -- --model fast prompt "hi" >/dev/null; then
  if assert_same_curl_command_path "${case109_name}" "${case109_dir}" \
     && assert_readiness_called_for "${case109_name}" "${case109_dir}/readiness.log" 'qwen3%3A14b'; then
    if [[ -s "${case109_dir}/second_curl.log" ]]; then
      fail_case "${case109_name}" "a second curl on PATH was invoked" \
        /dev/null /dev/null "${case109_dir}/second_curl.log"
    else
      pass_case "${case109_name}"
    fi
  fi
fi

# ---------- case 110: a failing floor is not escaped by a newer curl -------
# The mirror image of case 109. The FIRST curl on PATH is pre-8.4 and a newer
# one sits behind it. The wrapper must refuse on the command path it resolved,
# never walk PATH looking for a curl that passes.
case110_name="curl_command_path_binding_refuses_on_resolved_path_not_a_newer_one"
case110_dir="$(stage_layout "${case110_name}")"
install_canonical "${case110_dir}" current
write_fake_readiness_client "${case110_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0'
mkdir -p "${case110_dir}/decoybin2"
cat > "${case110_dir}/decoybin2/curl" <<CURLC
#!/usr/bin/env bash
printf 'SECOND_CURL_INVOKED=1 args=%s\n' "\$*" >> '${case110_dir}/second_curl.log'
if [[ "\${1:-}" == "--version" ]]; then
  printf 'curl 9.9.9 (x86_64-pc-linux-gnu) libcurl/9.9.9\n'
  exit 0
fi
printf '%s' '${READY_QWEN3_14B}'
printf '\n%s' '200'
CURLC
chmod 0755 "${case110_dir}/decoybin2/curl"
: > "${case110_dir}/second_curl.log"
if run_case "${case110_name}" "${case110_dir}" 9 \
     "PATH=${case110_dir}/decoybin:${case110_dir}/decoybin2:${case110_dir}/home/.cargo/bin:/usr/bin:/bin" \
     -- --model fast prompt "hi" >/dev/null; then
  if assert_curl_floor_refusal "${case110_name}" "${case110_dir}"; then
    if [[ -s "${case110_dir}/second_curl.log" ]]; then
      fail_case "${case110_name}" "the wrapper walked PATH for a curl that passes the floor" \
        /dev/null /dev/null "${case110_dir}/second_curl.log"
    else
      pass_case "${case110_name}"
    fi
  fi
fi

# ---------- case 111: the capability probe carries no credential -----------
# `curl --version` takes no arguments beyond the flag, and the API key must
# not reach the probe's argument list or the refusal text.
case111_name="curl_capability_probe_never_carries_the_api_key"
case111_dir="$(stage_layout "${case111_name}" 'export OPENAI_BASE_URL="http://127.0.0.1:11435/v1"
export OPENAI_API_KEY="sk-n6-floor-sentinel-must-not-leak"
export RUSTY_CLAUDE_MODEL_ALIAS__FAST="qwen3:14b"')"
install_canonical "${case111_dir}" current
write_fake_readiness_client "${case111_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0'
if run_case "${case111_name}" "${case111_dir}" 9 -- --model fast prompt "hi" >/dev/null; then
  if grep -Fq 'sk-n6-floor-sentinel-must-not-leak' "${case111_dir}/curlversion.log"; then
    fail_case "${case111_name}" "the API key reached the capability probe's arguments" \
      /dev/null /dev/null "${case111_dir}/curlversion.log"
  elif grep -Fq 'sk-n6-floor-sentinel-must-not-leak' "${case111_dir}/wrapper.stderr"; then
    fail_case "${case111_name}" "the API key was printed in the floor refusal" \
      /dev/null "${case111_dir}/wrapper.stderr" /dev/null
  else
    pass_case "${case111_name}"
  fi
fi

# ---------- case 112: a hostile banner cannot flood or escape the terminal -
# The refusal echoes a version string derived from curl's own output, so it is
# bounded and stripped to printable ASCII before it reaches stderr.
case112_name="curl_floor_refusal_bounds_and_sanitizes_a_hostile_banner"
case112_dir="$(stage_layout "${case112_name}")"
install_canonical "${case112_dir}" current
c112_flood="curl $(head -c 4000 /dev/zero | tr '\0' 'A')"
write_fake_readiness_client "${case112_dir}" 0 200 "${READY_QWEN3_14B}" "${c112_flood}"
if run_case "${case112_name}" "${case112_dir}" 9 -- --model fast prompt "hi" >/dev/null; then
  if assert_curl_floor_refusal "${case112_name}" "${case112_dir}"; then
    c112_longest="$(awk '{ if (length > max) max = length } END { print max + 0 }' \
      "${case112_dir}/wrapper.stderr")"
    if (( c112_longest > 400 )); then
      fail_case "${case112_name}" "the refusal echoed an unbounded banner (${c112_longest} chars)" \
        /dev/null "${case112_dir}/wrapper.stderr" /dev/null
    else
      pass_case "${case112_name}"
    fi
  fi
fi

# ---------- cases 113-118: non-inference commands gain NO curl requirement -
# The floor is a property of the readiness GET, and only the inference path
# performs one. A locally dispatched invocation must still run under a curl
# that could never satisfy the floor, and must not probe curl at all.
# assert_canonical_ran <name> <case_dir> <first_arg>
#
# `write_fake_claw` answers `--version` from a banner arm that deliberately
# does NOT append to the exec log, so for that one invocation the proof that
# canonical ran is the banner on stdout. Every other form logs its identity.
assert_canonical_ran() {
  local name="$1" dir="$2" first_arg="$3"
  if [[ "${first_arg}" == "--version" ]]; then
    if ! grep -Fq 'Claw Code' "${dir}/wrapper.stdout"; then
      fail_case "${name}" "canonical did not emit its version banner" \
        "${dir}/wrapper.stdout" "${dir}/wrapper.stderr" "${dir}/fake_claw.log"
      return 1
    fi
    return 0
  fi
  assert_log_contains "${name}" "${dir}/fake_claw.log" "IDENTITY=CANONICAL"
}

curl_floor_local_case() {
  local label="$1"
  shift
  local name="curl_floor_does_not_apply_to_local_${label}"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  write_fake_readiness_client "${dir}" 0 200 "${READY_QWEN3_14B}" \
    'curl 7.68.0 (x86_64-pc-linux-gnu) libcurl/7.68.0'
  if run_case "${name}" "${dir}" 0 -- "$@" >/dev/null; then
    if assert_canonical_ran "${name}" "${dir}" "$1" \
       && assert_readiness_not_called "${name}" "${dir}/readiness.log" \
       && assert_no_decoy_executed "${name}" "${dir}/fake_claw.log"; then
      if [[ -s "${dir}/curlversion.log" ]]; then
        fail_case "${name}" "a local invocation probed curl for its version" \
          /dev/null /dev/null "${dir}/curlversion.log"
      else
        pass_case "${name}"
      fi
    fi
  fi
}

curl_floor_local_case "help_flag" --help
curl_floor_local_case "version_flag" --version
curl_floor_local_case "version_subcommand" version
curl_floor_local_case "status_subcommand" status
curl_floor_local_case "doctor_subcommand" doctor
curl_floor_local_case "plan_run" plan run ./plan.yaml
# An effective dry run is local for the SAME reason: it cannot spawn a
# model-bearing child, so it performs no readiness GET and therefore needs no
# curl at all -- not even the version probe -- even though its `--wrapper`
# names a foreign executable the dry-run branch will never use.
curl_floor_local_case "plan_run_dry_run_with_a_foreign_wrapper" \
  plan run ./plan.yaml --dry-run --wrapper /usr/bin/claw

# ---------- cases 119-121: local commands run with NO curl on PATH ---------
# Stronger than the version floor: a local invocation must succeed on a host
# where curl is absent entirely. PATH is rebuilt from the tools the wrapper
# and the status helper actually need, with curl deliberately excluded.
make_curlless_bin() {
  local dir="$1" tool src
  mkdir -p -- "${dir}"
  for tool in bash sh git stat python3 sed tr grep sort cut dirname basename awk \
              head cat env id uname mkdir chmod ln rm ls date expr; do
    src="$(command -v "${tool}" 2>/dev/null || true)"
    [[ -n "${src}" ]] && ln -sf "${src}" "${dir}/${tool}"
  done
  if [[ -n "$(command -v curl 2>/dev/null || true)" && -e "${dir}/curl" ]]; then
    fail_case "make_curlless_bin" "curl leaked into the curl-free PATH" /dev/null /dev/null /dev/null
  fi
}

curl_absent_local_case() {
  local label="$1"
  shift
  local name="local_dispatch_survives_with_no_curl_on_path_${label}"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  make_curlless_bin "${dir}/nocurlbin"
  # The decoy claw stays reachable so a PATH fallback would still be caught.
  if run_case "${name}" "${dir}" 0 \
       "PATH=${dir}/nocurlbin:${dir}/home/.cargo/bin" \
       -- "$@" >/dev/null; then
    if assert_canonical_ran "${name}" "${dir}" "$1" \
       && assert_readiness_not_called "${name}" "${dir}/readiness.log" \
       && assert_no_decoy_executed "${name}" "${dir}/fake_claw.log"; then
      pass_case "${name}"
    fi
  fi
}

curl_absent_local_case "help_flag" --help
curl_absent_local_case "status_subcommand" status
curl_absent_local_case "doctor_subcommand" doctor

# ---------- case 122: inference with NO curl still refuses ----------------
# The pre-existing missing-curl refusal must survive the new floor: absent
# curl is still exit 9, and now there is no version probe to reach either.
case122_name="inference_with_no_curl_on_path_still_refuses"
case122_dir="$(stage_layout "${case122_name}")"
install_canonical "${case122_dir}" current
make_curlless_bin "${case122_dir}/nocurlbin"
if run_case "${case122_name}" "${case122_dir}" 9 \
     "PATH=${case122_dir}/nocurlbin:${case122_dir}/home/.cargo/bin" \
     -- --model fast prompt "hi" >/dev/null; then
  if assert_readiness_not_called "${case122_name}" "${case122_dir}/readiness.log" \
     && assert_no_decoy_executed "${case122_name}" "${case122_dir}/fake_claw.log"; then
    if grep -Fq 'FAKE_CLAW_CALLED=1' "${case122_dir}/fake_claw.log"; then
      fail_case "${case122_name}" "canonical executed without a curl to prove readiness" \
        /dev/null /dev/null "${case122_dir}/fake_claw.log"
    else
      pass_case "${case122_name}"
    fi
  fi
fi

# ---------- case 123: USAGE.md states the curl trust contract truthfully ----
# The wrapper's curl handling is a THREE-layer claim and the docs must keep the
# layers distinct: a compatibility guarantee (>= 8.4.0), a path-selection
# guarantee (resolved once, reused), and a trust ASSUMPTION (the local curl is
# trusted; its executable identity is not pinned). This case fails if the docs
# drift back toward claiming executable identity.
#
# Emphasis markers are stripped before matching so the assertions stay semantic
# rather than coupled to Markdown formatting.
case123_name="usage_docs_state_curl_trusted_dependency_contract"
case123_norm="${WORK_DIR}/usage-normalized.txt"
tr -d '*_`' < "${REPO_ROOT}/USAGE.md" > "${case123_norm}"
case123_ok=1
case123_missing=""
while IFS= read -r phrase; do
  [[ -n "${phrase}" ]] || continue
  if ! grep -iqF -- "${phrase}" "${case123_norm}"; then
    case123_ok=0
    case123_missing="${case123_missing}${case123_missing:+; }${phrase}"
  fi
done <<'PHRASES'
curl 8.4.0
trusted dependency
resolved command path
not executable pinning
same-user
outside its threat model
PHRASES
# The overclaims the independent review rejected must not reappear.
case123_bad=""
while IFS= read -r phrase; do
  [[ -n "${phrase}" ]] || continue
  if grep -iqF -- "${phrase}" "${case123_norm}"; then
    case123_ok=0
    case123_bad="${case123_bad}${case123_bad:+; }${phrase}"
  fi
done <<'BADPHRASES'
same curl executable
binary identity
pinned executable
identity proof
BADPHRASES
if (( case123_ok == 1 )); then
  pass_case "${case123_name}"
else
  fail_case "${case123_name}" \
    "USAGE.md curl trust contract drifted (missing: ${case123_missing:-none}) (overclaims present: ${case123_bad:-none})" \
    /dev/null /dev/null /dev/null
fi

# ---------- case 124: the wrapper source documents the same boundary -------
# Source comments are the other half of the contract. They must describe the
# trusted-dependency assumption and must not re-assert that the probed bytes
# are necessarily the bytes that serve the request.
case124_name="wrapper_source_documents_curl_trust_boundary"
case124_ok=1
case124_missing=""
while IFS= read -r phrase; do
  [[ -n "${phrase}" ]] || continue
  if ! grep -iqF -- "${phrase}" "${REAL_WRAPPER}"; then
    case124_ok=0
    case124_missing="${case124_missing}${case124_missing:+; }${phrase}"
  fi
done <<'PHRASES'
Trusted local dependencies
threat model
does not pin
retarget
PHRASES
case124_bad=""
while IFS= read -r phrase; do
  [[ -n "${phrase}" ]] || continue
  if grep -iqF -- "${phrase}" "${REAL_WRAPPER}"; then
    case124_ok=0
    case124_bad="${case124_bad}${case124_bad:+; }${phrase}"
  fi
done <<'BADPHRASES'
EXACT executable
necessarily the binary
same binary
binary identity
BADPHRASES
if (( case124_ok == 1 )); then
  pass_case "${case124_name}"
else
  fail_case "${case124_name}" \
    "wrapper trust-boundary comments drifted (missing: ${case124_missing:-none}) (overclaims present: ${case124_bad:-none})" \
    /dev/null /dev/null /dev/null
fi

# ---------- case 125: RESIDUAL — symlink retarget between probe and GET ----
# TRUST_BOUNDARY_RESIDUAL_CONFIRMED, not a promised refusal.
#
# The resolved curl path is a SYMLINK. The program it points at answers
# `--version` with a compliant 8.5.0 banner and, as a side effect, retargets
# the symlink at a different program, which then serves the readiness GET.
#
# The wrapper does NOT refuse this, and is not claimed to: resolving the
# command path once binds PATH SELECTION, never executable identity. This case
# asserts the residual is exactly as documented. If it ever stops holding, the
# wrapper gained a guarantee it does not currently claim and USAGE.md plus the
# source comments must be updated to match — that is a docs bug, not a
# security regression.
case125_name="TRUST_BOUNDARY_RESIDUAL_symlink_retarget_between_probe_and_get"
case125_dir="$(stage_layout "${case125_name}")"
install_canonical "${case125_dir}" current
: > "${case125_dir}/symlink.log"
cat > "${case125_dir}/decoybin/good-curl" <<GOODCURL
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'GOOD_VERSION_PROBE=1\n' >> '${case125_dir}/symlink.log'
  ln -sfn '${case125_dir}/decoybin/bad-curl' '${case125_dir}/decoybin/curl'
  printf 'curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0\n'
  exit 0
fi
printf 'GOOD_GET=1\n' >> '${case125_dir}/symlink.log'
printf '%s' '{"ready": true, "reason_code": "READY_GOOD", "requested_model": "qwen3:14b"}'
printf '\n200'
GOODCURL
cat > "${case125_dir}/decoybin/bad-curl" <<BADCURL
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'BAD_VERSION_PROBE=1\n' >> '${case125_dir}/symlink.log'
  printf 'curl 8.3.0 (x86_64-pc-linux-gnu) libcurl/8.3.0\n'
  exit 0
fi
printf 'BAD_GET=1\n' >> '${case125_dir}/symlink.log'
printf '%s' '{"ready": true, "reason_code": "READY_FROM_RETARGETED_PATH", "requested_model": "qwen3:14b"}'
printf '\n200'
BADCURL
chmod 0755 "${case125_dir}/decoybin/good-curl" "${case125_dir}/decoybin/bad-curl"
ln -sfn "${case125_dir}/decoybin/good-curl" "${case125_dir}/decoybin/curl"
if run_case "${case125_name}" "${case125_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  # The probe ran the original target; the GET was served by the retargeted one.
  if grep -Fq 'GOOD_VERSION_PROBE=1' "${case125_dir}/symlink.log" \
     && grep -Fq 'BAD_GET=1' "${case125_dir}/symlink.log" \
     && grep -Fq 'READY_FROM_RETARGETED_PATH' "${case125_dir}/wrapper.stderr"; then
    printf '  TRUST_BOUNDARY_RESIDUAL_CONFIRMED: %s\n' "${case125_name}"
    pass_case "${case125_name}"
  else
    fail_case "${case125_name}" \
      "documented symlink-retarget residual did not reproduce; docs and source comments must be re-checked against actual behaviour" \
      /dev/null "${case125_dir}/wrapper.stderr" "${case125_dir}/symlink.log"
  fi
fi

# ---------- case 126: RESIDUAL — a stateful, lying curl executable ---------
# TRUST_BOUNDARY_RESIDUAL_CONFIRMED, not a promised refusal.
#
# ONE executable reports a compliant `curl 8.5.0` for `--version`, then ignores
# `--max-filesize` on the GET and returns a body far past the 64 KiB bound.
# No path changes and no second curl is involved, so nothing about path
# resolution could detect it: the version banner is a CLAIM made by the program
# under test, and the size bound is enforced by curl itself, not by the
# wrapper.
#
# The wrapper trusts the local curl and therefore accepts this. Same contract
# as case 125: if this stops holding, the docs must be updated to match.
case126_name="TRUST_BOUNDARY_RESIDUAL_stateful_lying_curl_executable"
case126_dir="$(stage_layout "${case126_name}")"
install_canonical "${case126_dir}" current
: > "${case126_dir}/lying.log"
cat > "${case126_dir}/decoybin/curl" <<LYINGCURL
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'LYING_VERSION_PROBE=1\n' >> '${case126_dir}/lying.log'
  printf 'curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0\n'
  exit 0
fi
printf 'LYING_GET_IGNORED_MAX_FILESIZE=1\n' >> '${case126_dir}/lying.log'
python3 - <<'PYBODY'
import json
print(json.dumps({
    "ready": True,
    "reason_code": "READY_LYING_EXECUTABLE",
    "requested_model": "qwen3:14b",
    "padding": "x" * 200000,
}), end="")
PYBODY
printf '\n200'
LYINGCURL
chmod 0755 "${case126_dir}/decoybin/curl"
if run_case "${case126_name}" "${case126_dir}" 0 -- --model fast prompt "hi" >/dev/null; then
  if grep -Fq 'LYING_VERSION_PROBE=1' "${case126_dir}/lying.log" \
     && grep -Fq 'LYING_GET_IGNORED_MAX_FILESIZE=1' "${case126_dir}/lying.log" \
     && grep -Fq 'READY_LYING_EXECUTABLE' "${case126_dir}/wrapper.stderr"; then
    printf '  TRUST_BOUNDARY_RESIDUAL_CONFIRMED: %s\n' "${case126_name}"
    pass_case "${case126_name}"
  else
    fail_case "${case126_name}" \
      "documented lying-executable residual did not reproduce; docs and source comments must be re-checked against actual behaviour" \
      /dev/null "${case126_dir}/wrapper.stderr" "${case126_dir}/lying.log"
  fi
fi

# ============================================================================
# PR #180 review findings: argument-order semantics of parse_args_with_terminal
# ============================================================================
#
# Four parser-order defects were reported against this wrapper. Each is a
# mismatch between the wrapper's classifier and the ACTUAL Rust dispatch in
# rust/crates/rusty-claude-cli/src/main.rs, and three of the four could let an
# inference-capable invocation reach the canonical client without a readiness
# proof, or with a proof for the WRONG model. The cases below pin the real
# parser's behaviour, not the wrapper's convenience.

assert_readiness_not_called_for() {
  local name="$1" readiness_log="$2" encoded_model="$3"
  if grep -Fq "requested_model=${encoded_model}" "${readiness_log}"; then
    fail_case "${name}" "readiness was queried for ${encoded_model}, which is NOT the model this invocation dispatches on" \
      /dev/null /dev/null "${readiness_log}"
    return 1
  fi
}

# ---------- P1: a leading help/version flag must not outrank `-p` -----------
# `parse_args_with_terminal` returns CliAction::Prompt from INSIDE its argument
# loop the moment it reaches `-p`. `wants_help` / `wants_version` are only
# consulted AFTER that loop, so they never run for such an invocation:
# `--help --model fast -p status` is a prompt whose text reads "status".
# Classifying it local on the leading flag skipped the readiness request
# entirely and let canonical execute against a broker that would have refused.
for pr180_lead_flag in --help -h --version -V; do
  pr180_a_name="pr180_leading_${pr180_lead_flag#-}_flag_does_not_outrank_dash_p"
  pr180_a_name="${pr180_a_name//-/_}"
  pr180_a_dir="$(stage_layout "${pr180_a_name}")"
  install_canonical "${pr180_a_dir}" current
  write_fake_readiness_client "${pr180_a_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
  if run_case "${pr180_a_name}" "${pr180_a_dir}" 8 -- \
       "${pr180_lead_flag}" --model fast -p status >/dev/null; then
    if assert_readiness_called_for "${pr180_a_name}" "${pr180_a_dir}/readiness.log" 'qwen3%3A14b' \
       && assert_stderr_contains "${pr180_a_name}" "${pr180_a_dir}/wrapper.stderr" \
            'local coding readiness refused' \
       && assert_fake_not_called "${pr180_a_name}" "${pr180_a_dir}/fake_claw.log" \
       && assert_no_decoy_executed "${pr180_a_name}" "${pr180_a_dir}/fake_claw.log"; then
      pass_case "${pr180_a_name}"
    fi
  fi
done

# The same shape with a READY broker still runs -- the gate must admit it, not
# merely refuse everything -- and canonical must execute exactly once.
pr180_a_ready_name="pr180_leading_help_flag_before_dash_p_runs_once_when_ready"
pr180_a_ready_dir="$(stage_layout "${pr180_a_ready_name}")"
install_canonical "${pr180_a_ready_dir}" current
if run_case "${pr180_a_ready_name}" "${pr180_a_ready_dir}" 0 -- \
     --help --model fast -p status >/dev/null; then
  pr180_a_ready_execs="$(grep -c 'FAKE_CLAW_CALLED=1' "${pr180_a_ready_dir}/fake_claw.log" || true)"
  if assert_readiness_called_for "${pr180_a_ready_name}" "${pr180_a_ready_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_log_contains "${pr180_a_ready_name}" "${pr180_a_ready_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_a_ready_name}" "${pr180_a_ready_dir}/fake_claw.log"; then
    if [[ "${pr180_a_ready_execs}" == "1" ]]; then
      pass_case "${pr180_a_ready_name}"
    else
      fail_case "${pr180_a_ready_name}" "canonical executed ${pr180_a_ready_execs} times, expected exactly 1" \
        /dev/null /dev/null "${pr180_a_ready_dir}/fake_claw.log"
    fi
  fi
fi

# ---------- P1: `-p` consumes the tail for MODEL scanning too ---------------
# The readiness model must equal the model inference will actually use. `-p`
# joins every later token into the prompt TEXT, so a `--model` after it is
# prompt data, never model metadata.
pr180_b1_name="pr180_model_flag_after_the_dash_p_terminal_is_prompt_text"
pr180_b1_dir="$(stage_layout "${pr180_b1_name}")"
install_canonical "${pr180_b1_dir}" current
if run_case "${pr180_b1_name}" "${pr180_b1_dir}" 0 -- \
     --model fast -p hello --model deep >/dev/null; then
  if assert_readiness_called_for "${pr180_b1_name}" "${pr180_b1_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_not_called_for "${pr180_b1_name}" "${pr180_b1_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_no_decoy_executed "${pr180_b1_name}" "${pr180_b1_dir}/fake_claw.log"; then
    pass_case "${pr180_b1_name}"
  fi
fi

# The decisive direction: repeated `--model` BEFORE the terminal still follows
# last-assignment-wins, and a `--model` after it must not pull the verdict back
# to the earlier alias. Approving `fast` and then loading `deep` is exactly the
# wrong-model admission this gate exists to prevent.
pr180_b2_name="pr180_last_model_before_the_terminal_wins_over_the_tail"
pr180_b2_dir="$(stage_layout "${pr180_b2_name}")"
install_canonical "${pr180_b2_dir}" current
write_fake_readiness_client "${pr180_b2_dir}" 0 200 \
  '{"ready": true, "reason_code": "READY_AFTER_SAFE_EVICTION", "requested_model": "qwen3.5:27b", "requires_hyperliquid_pause": false}'
if run_case "${pr180_b2_name}" "${pr180_b2_dir}" 0 -- \
     --model fast --model deep -p hello --model fast >/dev/null; then
  if assert_readiness_called_for "${pr180_b2_name}" "${pr180_b2_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_readiness_not_called_for "${pr180_b2_name}" "${pr180_b2_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_no_decoy_executed "${pr180_b2_name}" "${pr180_b2_dir}/fake_claw.log"; then
    pass_case "${pr180_b2_name}"
  fi
fi

# The `--model=VALUE` form obeys the same terminal.
pr180_b3_name="pr180_model_equals_form_respects_the_dash_p_terminal"
pr180_b3_dir="$(stage_layout "${pr180_b3_name}")"
install_canonical "${pr180_b3_dir}" current
write_fake_readiness_client "${pr180_b3_dir}" 0 200 \
  '{"ready": true, "reason_code": "READY_AFTER_SAFE_EVICTION", "requested_model": "qwen3.5:27b", "requires_hyperliquid_pause": false}'
if run_case "${pr180_b3_name}" "${pr180_b3_dir}" 0 -- \
     --model=deep -p hello --model=fast >/dev/null; then
  if assert_readiness_called_for "${pr180_b3_name}" "${pr180_b3_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_readiness_not_called_for "${pr180_b3_name}" "${pr180_b3_dir}/readiness.log" 'qwen3%3A14b'; then
    pass_case "${pr180_b3_name}"
  fi
fi

# Tokens in the prompt tail are not top-level options of ANY kind: a `--model`,
# a local verb and a `--wrapper` all sit in the text here.
pr180_b4_name="pr180_prompt_tail_tokens_are_never_top_level_options"
pr180_b4_dir="$(stage_layout "${pr180_b4_name}")"
install_canonical "${pr180_b4_dir}" current
if run_case "${pr180_b4_name}" "${pr180_b4_dir}" 0 -- \
     --model fast -p run --model deep --wrapper /usr/bin/claw >/dev/null; then
  if assert_readiness_called_for "${pr180_b4_name}" "${pr180_b4_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_not_called_for "${pr180_b4_name}" "${pr180_b4_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_log_contains "${pr180_b4_name}" "${pr180_b4_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_b4_name}"
  fi
fi

# ---------- P1: repeated plan `--wrapper` is LAST-assignment-wins ----------
# `parse_plan_subcommand_args` reassigns `wrapper` on every occurrence and
# never breaks, so the last one is what `build_claw_command` spawns the plan's
# model-bearing steps through. A first-match reader called
# `--wrapper <self> --wrapper /usr/bin/claw` safely self-wrapped while the
# runner really used /usr/bin/claw -- a foreign-wrapper escape.

# pr180_plan_wrapper_case <label> <expected_exit> <argv...>
# The literal string __SELF__ in argv is replaced with this case's own wrapper.
pr180_plan_wrapper_case() {
  local label="$1" expected_exit="$2"
  shift 2
  local name="pr180_plan_wrapper_${label}"
  local dir arg
  local -a argv=()
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  for arg in "$@"; do
    if [[ "${arg}" == "__SELF__" ]]; then
      argv+=( "${dir}/root/scripts/claw-sidestack-local" )
    elif [[ "${arg}" == "--wrapper=__SELF__" ]]; then
      argv+=( "--wrapper=${dir}/root/scripts/claw-sidestack-local" )
    else
      argv+=( "${arg}" )
    fi
  done
  if run_case "${name}" "${dir}" "${expected_exit}" -- "${argv[@]}" >/dev/null; then
    if assert_readiness_not_called "${name}" "${dir}/readiness.log" \
       && assert_no_decoy_executed "${name}" "${dir}/fake_claw.log"; then
      if [[ "${expected_exit}" == "9" ]]; then
        if assert_stderr_contains "${name}" "${dir}/wrapper.stderr" \
             'not proven to pass the readiness gate' \
           && assert_fake_not_called "${name}" "${dir}/fake_claw.log"; then
          pass_case "${name}"
        fi
      else
        if assert_log_contains "${name}" "${dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
          pass_case "${name}"
        fi
      fi
    fi
  fi
}

# self then foreign: the runner uses the FOREIGN wrapper -> must refuse.
pr180_plan_wrapper_case self_then_foreign_refuses 9 \
  plan run ./plan.yaml --wrapper __SELF__ --wrapper /usr/bin/claw
# foreign then self: the runner really does use THIS wrapper -> refusing would
# be a false refusal.
pr180_plan_wrapper_case foreign_then_self_is_allowed 0 \
  plan run ./plan.yaml --wrapper /usr/bin/claw --wrapper __SELF__
pr180_plan_wrapper_case self_then_self_is_allowed 0 \
  plan run ./plan.yaml --wrapper __SELF__ --wrapper __SELF__
pr180_plan_wrapper_case foreign_then_foreign_refuses 9 \
  plan run ./plan.yaml --wrapper /usr/bin/claw --wrapper /bin/claw
# Mixed `--wrapper VALUE` / `--wrapper=VALUE` syntax obeys the same rule.
pr180_plan_wrapper_case equals_self_then_space_foreign_refuses 9 \
  plan run ./plan.yaml --wrapper=__SELF__ --wrapper /usr/bin/claw
pr180_plan_wrapper_case space_foreign_then_equals_self_is_allowed 0 \
  plan run ./plan.yaml --wrapper /usr/bin/claw --wrapper=__SELF__
# A trailing valueless `--wrapper` is the LAST assignment. The CLI errors on
# it, and the wrapper must fail closed rather than fall back to the earlier
# self-wrapper value.
pr180_plan_wrapper_case self_then_valueless_fails_closed 9 \
  plan run ./plan.yaml --wrapper __SELF__ --wrapper

# The refusal must name the EFFECTIVE wrapper, so the operator is told which
# executable the runner would actually have used.
pr180_c_msg_name="pr180_plan_wrapper_refusal_names_the_effective_wrapper"
pr180_c_msg_dir="$(stage_layout "${pr180_c_msg_name}")"
install_canonical "${pr180_c_msg_dir}" current
if run_case "${pr180_c_msg_name}" "${pr180_c_msg_dir}" 9 -- \
     plan run ./plan.yaml \
     --wrapper "${pr180_c_msg_dir}/root/scripts/claw-sidestack-local" \
     --wrapper /usr/bin/claw >/dev/null; then
  if assert_stderr_contains "${pr180_c_msg_name}" "${pr180_c_msg_dir}/wrapper.stderr" \
       'claw plan run --wrapper /usr/bin/claw' \
     && assert_fake_not_called "${pr180_c_msg_name}" "${pr180_c_msg_dir}/fake_claw.log"; then
    pass_case "${pr180_c_msg_name}"
  fi
fi

# Plan-tail flags consume their value token, so a `--wrapper` sitting in
# another flag's VALUE slot is that value, not an override. Here the CLI binds
# fast_model="--wrapper" and the plan file "/usr/bin/claw", leaving `wrapper`
# unset -- so the runner uses its DEFAULT (this wrapper) and the children stay
# gated. Refusing would be a false refusal built on a naive substring scan.
pr180_c_slot_name="pr180_plan_wrapper_in_another_flags_value_slot_is_not_an_override"
pr180_c_slot_dir="$(stage_layout "${pr180_c_slot_name}")"
install_canonical "${pr180_c_slot_dir}" current
if run_case "${pr180_c_slot_name}" "${pr180_c_slot_dir}" 0 -- \
     plan run --fast-model --wrapper /usr/bin/claw >/dev/null; then
  if assert_readiness_not_called "${pr180_c_slot_name}" "${pr180_c_slot_dir}/readiness.log" \
     && assert_log_contains "${pr180_c_slot_name}" "${pr180_c_slot_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_c_slot_name}" "${pr180_c_slot_dir}/fake_claw.log"; then
    pass_case "${pr180_c_slot_name}"
  fi
fi

# ---------- P2: the bare `help` subcommand is local ------------------------
# `parse_single_word_command_alias` maps the single word `help` to
# CliAction::Help, a local printer that cannot issue a provider request. It
# must not demand `--model` or a readiness proof.
pr180_d1_name="pr180_bare_help_subcommand_is_local"
pr180_d1_dir="$(stage_layout "${pr180_d1_name}")"
install_canonical "${pr180_d1_dir}" current
if run_case "${pr180_d1_name}" "${pr180_d1_dir}" 0 -- help >/dev/null; then
  if assert_readiness_not_called "${pr180_d1_name}" "${pr180_d1_dir}/readiness.log" \
     && assert_log_contains "${pr180_d1_name}" "${pr180_d1_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_d1_name}" "${pr180_d1_dir}/fake_claw.log"; then
    if [[ -s "${pr180_d1_dir}/curlversion.log" ]]; then
      fail_case "${pr180_d1_name}" "a local help invocation probed curl for its version" \
        /dev/null /dev/null "${pr180_d1_dir}/curlversion.log"
    else
      pass_case "${pr180_d1_name}"
    fi
  fi
fi

# `help` is local only as the WHOLE positional vector. `help --help` and
# `help -h` have no local-help topic and are declined by the diagnostic-verb
# alias, so both fall through to the prompt catch-all and stay gated.
for pr180_help_tail in --help -h; do
  pr180_d2_name="pr180_help_with_a_${pr180_help_tail#-}_tail_is_still_inference"
  pr180_d2_name="${pr180_d2_name//-/_}"
  pr180_d2_dir="$(stage_layout "${pr180_d2_name}")"
  install_canonical "${pr180_d2_dir}" current
  if run_case "${pr180_d2_name}" "${pr180_d2_dir}" 9 -- help "${pr180_help_tail}" >/dev/null; then
    if assert_stderr_contains "${pr180_d2_name}" "${pr180_d2_dir}/wrapper.stderr" \
         'readiness requires an explicit --model for this invocation' \
       && assert_fake_not_called "${pr180_d2_name}" "${pr180_d2_dir}/fake_claw.log"; then
      pass_case "${pr180_d2_name}"
    fi
  fi
done

# `-p help` is a PROMPT whose text reads "help", and `prompt help` is the
# prompt subcommand. Both must still be gated on a live readiness answer.
pr180_d3_name="pr180_dash_p_help_is_gated_not_local_help"
pr180_d3_dir="$(stage_layout "${pr180_d3_name}")"
install_canonical "${pr180_d3_dir}" current
write_fake_readiness_client "${pr180_d3_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_d3_name}" "${pr180_d3_dir}" 8 -- --model fast -p help >/dev/null; then
  if assert_readiness_called_for "${pr180_d3_name}" "${pr180_d3_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_d3_name}" "${pr180_d3_dir}/fake_claw.log"; then
    pass_case "${pr180_d3_name}"
  fi
fi

pr180_d4_name="pr180_prompt_help_subcommand_is_gated"
pr180_d4_dir="$(stage_layout "${pr180_d4_name}")"
install_canonical "${pr180_d4_dir}" current
write_fake_readiness_client "${pr180_d4_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_d4_name}" "${pr180_d4_dir}" 8 -- --model fast prompt help >/dev/null; then
  if assert_readiness_called_for "${pr180_d4_name}" "${pr180_d4_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_d4_name}" "${pr180_d4_dir}/fake_claw.log"; then
    pass_case "${pr180_d4_name}"
  fi
fi

pr180_d5_name="pr180_shorthand_prompt_whose_text_contains_help_is_gated"
pr180_d5_dir="$(stage_layout "${pr180_d5_name}")"
install_canonical "${pr180_d5_dir}" current
write_fake_readiness_client "${pr180_d5_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_d5_name}" "${pr180_d5_dir}" 8 -- --model fast "explain help to me" >/dev/null; then
  if assert_readiness_called_for "${pr180_d5_name}" "${pr180_d5_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_d5_name}" "${pr180_d5_dir}/fake_claw.log"; then
    pass_case "${pr180_d5_name}"
  fi
fi

# ---------- adjacent parser-order audit ------------------------------------
# `wants_help` / `wants_version` are set wherever the CLI's own guards say so,
# not only on the FIRST token. `--model fast --help` is CliAction::Help: the
# flag arrives while the positional vector is still empty.
pr180_e1_name="pr180_help_flag_after_a_global_flag_is_still_local_help"
pr180_e1_dir="$(stage_layout "${pr180_e1_name}")"
install_canonical "${pr180_e1_dir}" current
if run_case "${pr180_e1_name}" "${pr180_e1_dir}" 0 -- --model fast --help >/dev/null; then
  if assert_readiness_not_called "${pr180_e1_name}" "${pr180_e1_dir}/readiness.log" \
     && assert_log_contains "${pr180_e1_name}" "${pr180_e1_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_e1_name}"
  fi
fi

# The CLI intercepts `--help` after exactly four API-forwarding subcommands
# (prompt, commit, pr, issue) and shows top-level help instead of sending the
# flag to a provider.
pr180_e2_name="pr180_help_flag_after_the_prompt_subcommand_is_local_help"
pr180_e2_dir="$(stage_layout "${pr180_e2_name}")"
install_canonical "${pr180_e2_dir}" current
if run_case "${pr180_e2_name}" "${pr180_e2_dir}" 0 -- prompt hi --help >/dev/null; then
  if assert_readiness_not_called "${pr180_e2_name}" "${pr180_e2_dir}/readiness.log" \
     && assert_log_contains "${pr180_e2_name}" "${pr180_e2_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_e2_name}"
  fi
fi

# ...but a `--help` after ANY other positional is prompt text, not a help
# request. This is the boundary case for the rule above and must stay gated.
pr180_e3_name="pr180_help_flag_after_an_ordinary_positional_stays_gated"
pr180_e3_dir="$(stage_layout "${pr180_e3_name}")"
install_canonical "${pr180_e3_dir}" current
write_fake_readiness_client "${pr180_e3_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_e3_name}" "${pr180_e3_dir}" 8 -- --model fast explain this --help >/dev/null; then
  if assert_readiness_called_for "${pr180_e3_name}" "${pr180_e3_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_e3_name}" "${pr180_e3_dir}/fake_claw.log"; then
    pass_case "${pr180_e3_name}"
  fi
fi

# `--resume` pushes a positional, so a `--help` AFTER it is a resumed-session
# argument, not top-level help: the invocation continues a real session and
# stays gated.
pr180_e4_name="pr180_help_flag_after_resume_is_not_local_help"
pr180_e4_dir="$(stage_layout "${pr180_e4_name}")"
install_canonical "${pr180_e4_dir}" current
write_fake_readiness_client "${pr180_e4_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_e4_name}" "${pr180_e4_dir}" 8 -- --model fast --resume --help >/dev/null; then
  if assert_readiness_called_for "${pr180_e4_name}" "${pr180_e4_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_e4_name}" "${pr180_e4_dir}/fake_claw.log"; then
    pass_case "${pr180_e4_name}"
  fi
fi

# =====================================================================
# PR #180 fresh Codex review (review 5052994850) — three P1 findings.
# =====================================================================

# ---------- P1-A: a value slot must never become a model flag --------------
#
# Every value-taking GLOBAL flag consumes the token after it, so a `--model`
# standing in that slot is that flag's VALUE and never reaches the CLI's
# `--model` arm. `--model fast --base-commit --model deep -p hello` dispatches
# on `fast`; a scanner walking one token at a time reads `deep` and gates
# readiness on a model the invocation never loads.
#
# One case per value-taking global arm of `parse_args_with_terminal`, because
# `--base-commit` is only the example the review happened to pick.
pr180_value_slot_case() {
  local flag="$1"
  local name="pr180_model_in_the_${flag//-/_}_value_slot_is_not_a_model_flag"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current
  # The fixture answers only for qwen3:14b. A wrapper that gated on `deep`
  # would query qwen3.5:27b, fail the requested_model echo check, and exit 9
  # instead of 8 — so this case discriminates on the exit code as well as on
  # the recorded URL.
  write_fake_readiness_client "${dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
  if run_case "${name}" "${dir}" 8 -- \
       --model fast "${flag}" --model deep -p hello >/dev/null; then
    if assert_readiness_called_for "${name}" "${dir}/readiness.log" 'qwen3%3A14b' \
       && assert_fake_not_called "${name}" "${dir}/fake_claw.log"; then
      pass_case "${name}"
    fi
  fi
}

pr180_value_slot_case --base-commit
pr180_value_slot_case --permission-mode
pr180_value_slot_case --output-format
pr180_value_slot_case --reasoning-effort
pr180_value_slot_case --allowedTools
pr180_value_slot_case --allowed-tools

# The converse must still hold: once the value slot is FILLED, a later
# `--model` is a real option and the LAST one wins. Without this case a fix
# that simply ignored every `--model` after the first would also pass.
pr180_a7_name="pr180_a_model_flag_after_a_filled_value_slot_still_wins"
pr180_a7_dir="$(stage_layout "${pr180_a7_name}")"
install_canonical "${pr180_a7_dir}" current
write_fake_readiness_client "${pr180_a7_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE qwen3.5:27b)"
if run_case "${pr180_a7_name}" "${pr180_a7_dir}" 8 -- \
     --model fast --base-commit HEAD~1 --model deep -p hello >/dev/null; then
  if assert_readiness_called_for "${pr180_a7_name}" "${pr180_a7_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_fake_not_called "${pr180_a7_name}" "${pr180_a7_dir}/fake_claw.log"; then
    pass_case "${pr180_a7_name}"
  fi
fi

# `--manifests-dir` belongs to the `dump-manifests` SUBCOMMAND parser, never to
# the global loop, so the global loop pushes it onto the positional vector like
# any other token. Treating it as value-taking swallows the token after it —
# and when that token is the `-p` terminal, the invocation is classified on the
# `dump-manifests` positional instead of as the prompt the CLI actually
# dispatches. That is a readiness BYPASS, so it is pinned here.
pr180_a8_name="pr180_manifests_dir_does_not_swallow_the_prompt_terminal"
pr180_a8_dir="$(stage_layout "${pr180_a8_name}")"
install_canonical "${pr180_a8_dir}" current
write_fake_readiness_client "${pr180_a8_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_a8_name}" "${pr180_a8_dir}" 8 -- \
     --model fast dump-manifests --manifests-dir -p foo >/dev/null; then
  if assert_readiness_called_for "${pr180_a8_name}" "${pr180_a8_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_a8_name}" "${pr180_a8_dir}/fake_claw.log"; then
    pass_case "${pr180_a8_name}"
  fi
fi

# ...and it must not swallow a real `--model` either: with `--manifests-dir`
# treated as value-taking, this invocation would gate on `fast` while the CLI
# dispatches `deep`.
pr180_a9_name="pr180_manifests_dir_does_not_swallow_a_later_model_flag"
pr180_a9_dir="$(stage_layout "${pr180_a9_name}")"
install_canonical "${pr180_a9_dir}" current
write_fake_readiness_client "${pr180_a9_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE qwen3.5:27b)"
if run_case "${pr180_a9_name}" "${pr180_a9_dir}" 8 -- \
     --model fast status --manifests-dir --model deep -p hi >/dev/null; then
  if assert_readiness_called_for "${pr180_a9_name}" "${pr180_a9_dir}/readiness.log" 'qwen3.5%3A27b' \
     && assert_fake_not_called "${pr180_a9_name}" "${pr180_a9_dir}/fake_claw.log"; then
    pass_case "${pr180_a9_name}"
  fi
fi

# ---------- P1-B: the env alias is not the last resolution hop -------------
#
# The `-p` arm calls `resolve_model_alias_with_config` a SECOND time on the
# already-resolved string. That pass misses the env table but still consults
# the config aliases, which can rename any string. Preflighting the
# intermediate value would admit one model and load another.
pr180_b1_name="pr180_config_alias_on_the_env_alias_result_fails_closed"
pr180_b1_dir="$(stage_layout "${pr180_b1_name}")"
install_canonical "${pr180_b1_dir}" current
printf '{"aliases": {"qwen3:14b": "openai/other"}}\n' > "${pr180_b1_dir}/cwd/.claw.json"
if run_case "${pr180_b1_name}" "${pr180_b1_dir}" 9 -- --model fast -p hello >/dev/null; then
  if assert_stderr_contains "${pr180_b1_name}" "${pr180_b1_dir}/wrapper.stderr" \
       'which a claw config alias redefines again' \
     && assert_readiness_not_called "${pr180_b1_name}" "${pr180_b1_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_b1_name}" "${pr180_b1_dir}/fake_claw.log"; then
    pass_case "${pr180_b1_name}"
  fi
fi

# The refusal is specific to a config alias standing on the RESOLVED value. An
# unrelated alias in the same config must not refuse anything.
pr180_b2_name="pr180_an_unrelated_config_alias_does_not_refuse_the_env_alias"
pr180_b2_dir="$(stage_layout "${pr180_b2_name}")"
install_canonical "${pr180_b2_dir}" current
printf '{"aliases": {"something-else": "openai/other"}}\n' > "${pr180_b2_dir}/cwd/.claw.json"
write_fake_readiness_client "${pr180_b2_dir}" 0 200 "$(refusal_body HYPERLIQUID_ACTIVE)"
if run_case "${pr180_b2_name}" "${pr180_b2_dir}" 8 -- --model fast -p hello >/dev/null; then
  if assert_readiness_called_for "${pr180_b2_name}" "${pr180_b2_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_fake_not_called "${pr180_b2_name}" "${pr180_b2_dir}/fake_claw.log"; then
    pass_case "${pr180_b2_name}"
  fi
fi

# The same second hop applies to the `--model=VALUE` form.
pr180_b3_name="pr180_config_alias_on_the_env_alias_result_fails_closed_equals_form"
pr180_b3_dir="$(stage_layout "${pr180_b3_name}")"
install_canonical "${pr180_b3_dir}" current
printf '{"aliases": {"qwen3:14b": "openai/other"}}\n' > "${pr180_b3_dir}/cwd/.claw.json"
if run_case "${pr180_b3_name}" "${pr180_b3_dir}" 9 -- --model=fast -p hello >/dev/null; then
  if assert_readiness_not_called "${pr180_b3_name}" "${pr180_b3_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_b3_name}" "${pr180_b3_dir}/fake_claw.log"; then
    pass_case "${pr180_b3_name}"
  fi
fi

# ---------- P1-C: bind the DEFAULT plan wrapper to this executable ---------
#
# `plan run` without `--wrapper` uses the relative default
# `scripts/claw-sidestack-local`. Nothing binds that to the tree this wrapper
# lives in: invoked by absolute path from another tree, the runner hands every
# model-bearing step to THAT tree's wrapper.
pr180_c1_name="pr180_plan_run_default_wrapper_in_a_foreign_cwd_refuses"
pr180_c1_dir="$(stage_layout "${pr180_c1_name}")"
install_canonical "${pr180_c1_dir}" current
mkdir -p "${pr180_c1_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_c1_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_c1_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_c1_dir}/foreign" \
   run_case "${pr180_c1_name}" "${pr180_c1_dir}" 9 -- plan run ./plan.yaml >/dev/null; then
  if assert_stderr_contains "${pr180_c1_name}" "${pr180_c1_dir}/wrapper.stderr" \
       'not proven to pass the readiness gate' \
     && assert_readiness_not_called "${pr180_c1_name}" "${pr180_c1_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_c1_name}" "${pr180_c1_dir}/fake_claw.log"; then
    pass_case "${pr180_c1_name}"
  fi
fi

# Positive control: run the plan from the tree this wrapper lives in and the
# runner default resolves back to this very file, so the child-gating proof
# holds and the invocation stays local.
pr180_c2_name="pr180_plan_run_default_wrapper_in_its_own_tree_is_local"
pr180_c2_dir="$(stage_layout "${pr180_c2_name}")"
install_canonical "${pr180_c2_dir}" current
if RUN_CASE_CWD="${pr180_c2_dir}/root" \
   run_case "${pr180_c2_name}" "${pr180_c2_dir}" 0 -- plan run ./plan.yaml >/dev/null; then
  if assert_readiness_not_called "${pr180_c2_name}" "${pr180_c2_dir}/readiness.log" \
     && assert_log_contains "${pr180_c2_name}" "${pr180_c2_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_c2_name}" "${pr180_c2_dir}/fake_claw.log"; then
    pass_case "${pr180_c2_name}"
  fi
fi

# `--workspace-root` moves the directory the child resolves the relative
# program against, so it moves the executable that actually runs even when the
# CWD copy is this wrapper.
pr180_c3_name="pr180_plan_run_workspace_root_moving_the_default_wrapper_refuses"
pr180_c3_dir="$(stage_layout "${pr180_c3_name}")"
install_canonical "${pr180_c3_dir}" current
mkdir -p "${pr180_c3_dir}/elsewhere/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_c3_dir}/elsewhere/scripts/claw-sidestack-local"
chmod 0755 "${pr180_c3_dir}/elsewhere/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_c3_dir}/root" \
   run_case "${pr180_c3_name}" "${pr180_c3_dir}" 9 -- \
     plan run ./plan.yaml --workspace-root "${pr180_c3_dir}/elsewhere" >/dev/null; then
  if assert_readiness_not_called "${pr180_c3_name}" "${pr180_c3_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_c3_name}" "${pr180_c3_dir}/fake_claw.log"; then
    pass_case "${pr180_c3_name}"
  fi
fi

# ...including the `=` form, and LAST assignment wins.
pr180_c4_name="pr180_plan_run_workspace_root_equals_form_last_wins"
pr180_c4_dir="$(stage_layout "${pr180_c4_name}")"
install_canonical "${pr180_c4_dir}" current
mkdir -p "${pr180_c4_dir}/elsewhere/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_c4_dir}/elsewhere/scripts/claw-sidestack-local"
chmod 0755 "${pr180_c4_dir}/elsewhere/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_c4_dir}/root" \
   run_case "${pr180_c4_name}" "${pr180_c4_dir}" 9 -- \
     plan run ./plan.yaml "--workspace-root=${pr180_c4_dir}/root" \
     "--workspace-root=${pr180_c4_dir}/elsewhere" >/dev/null; then
  if assert_readiness_not_called "${pr180_c4_name}" "${pr180_c4_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_c4_name}" "${pr180_c4_dir}/fake_claw.log"; then
    pass_case "${pr180_c4_name}"
  fi
fi

# ...and the converse: a `--workspace-root` pointing BACK at this wrapper's own
# tree keeps the proof intact and must not be refused.
pr180_c5_name="pr180_plan_run_workspace_root_pointing_at_this_tree_is_local"
pr180_c5_dir="$(stage_layout "${pr180_c5_name}")"
install_canonical "${pr180_c5_dir}" current
if RUN_CASE_CWD="${pr180_c5_dir}/root" \
   run_case "${pr180_c5_name}" "${pr180_c5_dir}" 0 -- \
     plan run ./plan.yaml --workspace-root "${pr180_c5_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_c5_name}" "${pr180_c5_dir}/readiness.log" \
     && assert_log_contains "${pr180_c5_name}" "${pr180_c5_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_c5_name}"
  fi
fi

# A `--workspace-root` occupying another plan flag's VALUE slot is that value,
# not an override — the same tokenization rule the plan tail already applies to
# `--wrapper`.
pr180_c6_name="pr180_plan_run_workspace_root_in_a_value_slot_is_not_an_override"
pr180_c6_dir="$(stage_layout "${pr180_c6_name}")"
install_canonical "${pr180_c6_dir}" current
if RUN_CASE_CWD="${pr180_c6_dir}/root" \
   run_case "${pr180_c6_name}" "${pr180_c6_dir}" 0 -- \
     plan run ./plan.yaml --fast-model --workspace-root >/dev/null; then
  if assert_readiness_not_called "${pr180_c6_name}" "${pr180_c6_dir}/readiness.log" \
     && assert_log_contains "${pr180_c6_name}" "${pr180_c6_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_c6_name}"
  fi
fi

# When the runner default does not EXIST under the plan process CWD, the CLI's
# own `wrapper_path.exists()` precheck reports substrate-unavailable and spawns
# nothing at all, so there is no model-bearing child to gate. That stays local
# — the refusal above is about a default that resolves to a DIFFERENT
# executable, not about the flag being absent.
pr180_c7_name="pr180_plan_run_with_no_default_wrapper_under_cwd_stays_local"
pr180_c7_dir="$(stage_layout "${pr180_c7_name}")"
install_canonical "${pr180_c7_dir}" current
if run_case "${pr180_c7_name}" "${pr180_c7_dir}" 0 -- plan run ./plan.yaml >/dev/null; then
  if assert_readiness_not_called "${pr180_c7_name}" "${pr180_c7_dir}/readiness.log" \
     && assert_log_contains "${pr180_c7_name}" "${pr180_c7_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_c7_name}"
  fi
fi

# An explicit `--wrapper` pointing at this wrapper still wins even when the
# CWD default would resolve somewhere else: the override is what the runner
# uses, so the default is irrelevant.
pr180_c8_name="pr180_explicit_wrapper_override_wins_over_a_foreign_cwd_default"
pr180_c8_dir="$(stage_layout "${pr180_c8_name}")"
install_canonical "${pr180_c8_dir}" current
mkdir -p "${pr180_c8_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_c8_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_c8_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_c8_dir}/foreign" \
   run_case "${pr180_c8_name}" "${pr180_c8_dir}" 0 -- \
     plan run ./plan.yaml --wrapper "${pr180_c8_dir}/root/scripts/claw-sidestack-local" >/dev/null; then
  if assert_readiness_not_called "${pr180_c8_name}" "${pr180_c8_dir}/readiness.log" \
     && assert_log_contains "${pr180_c8_name}" "${pr180_c8_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_c8_name}"
  fi
fi

# A non-`run` plan subcommand spawns no children, so the default-wrapper proof
# does not apply to it even from a foreign tree.
pr180_c9_name="pr180_plan_status_from_a_foreign_cwd_is_still_local"
pr180_c9_dir="$(stage_layout "${pr180_c9_name}")"
install_canonical "${pr180_c9_dir}" current
mkdir -p "${pr180_c9_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_c9_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_c9_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_c9_dir}/foreign" \
   run_case "${pr180_c9_name}" "${pr180_c9_dir}" 0 -- plan status . >/dev/null; then
  if assert_readiness_not_called "${pr180_c9_name}" "${pr180_c9_dir}/readiness.log" \
     && assert_log_contains "${pr180_c9_name}" "${pr180_c9_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_c9_name}"
  fi
fi

# ---------- P1-D: preview mode must prove the wrapper it will spawn --------
#
# `--workspace-write-preview` takes a DIFFERENT branch of
# `run_plan_subcommand` than a normal `plan run`, and that branch has no
# `wrapper_path.exists()` precheck. `run_plan_with_write_preview` still spawns
# every read-only step BEFORE the lone workspace-write step, through
# `<workspace-root>/scripts/claw-sidestack-local`. So "the CWD default is
# missing" -- which really does stop a NORMAL run before it spawns -- proves
# nothing at all here, and the workspace-root wrapper must be judged on its own.

# The exact reported shape: invoked by absolute path from a CWD with no
# `scripts/`, previewing into a foreign tree that HAS its own wrapper.
pr180_d1_name="pr180_preview_with_no_cwd_default_and_a_foreign_workspace_root_refuses"
pr180_d1_dir="$(stage_layout "${pr180_d1_name}")"
install_canonical "${pr180_d1_dir}" current
mkdir -p "${pr180_d1_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_d1_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_d1_dir}/foreign/scripts/claw-sidestack-local"
if run_case "${pr180_d1_name}" "${pr180_d1_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --workspace-root "${pr180_d1_dir}/foreign" >/dev/null; then
  if assert_stderr_contains "${pr180_d1_name}" "${pr180_d1_dir}/wrapper.stderr" \
       'not proven to pass the readiness gate' \
     && assert_readiness_not_called "${pr180_d1_name}" "${pr180_d1_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_d1_name}" "${pr180_d1_dir}/fake_claw.log"; then
    pass_case "${pr180_d1_name}"
  fi
fi

# Positive control: same missing-CWD-default shape, but the workspace root IS
# this wrapper's tree, so the executable preview would spawn is this wrapper.
pr180_d2_name="pr180_preview_with_no_cwd_default_but_a_self_workspace_root_is_local"
pr180_d2_dir="$(stage_layout "${pr180_d2_name}")"
install_canonical "${pr180_d2_dir}" current
if run_case "${pr180_d2_name}" "${pr180_d2_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --workspace-root "${pr180_d2_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_d2_name}" "${pr180_d2_dir}/readiness.log" \
     && assert_log_contains "${pr180_d2_name}" "${pr180_d2_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_d2_name}" "${pr180_d2_dir}/fake_claw.log"; then
    pass_case "${pr180_d2_name}"
  fi
fi

# A protected CWD default does not launder a foreign workspace root either:
# the spawn base is the workspace root, not the CWD.
pr180_d3_name="pr180_preview_from_this_tree_into_a_foreign_workspace_root_refuses"
pr180_d3_dir="$(stage_layout "${pr180_d3_name}")"
install_canonical "${pr180_d3_dir}" current
mkdir -p "${pr180_d3_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_d3_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_d3_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_d3_dir}/root" \
   run_case "${pr180_d3_name}" "${pr180_d3_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --workspace-root "${pr180_d3_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_d3_name}" "${pr180_d3_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_d3_name}" "${pr180_d3_dir}/fake_claw.log"; then
    pass_case "${pr180_d3_name}"
  fi
fi

# An explicit foreign `--wrapper` is refused in preview mode for the same
# reason it is refused in a normal run: it is the executable that spawns.
pr180_d4_name="pr180_preview_with_an_explicit_foreign_wrapper_refuses"
pr180_d4_dir="$(stage_layout "${pr180_d4_name}")"
install_canonical "${pr180_d4_dir}" current
if RUN_CASE_CWD="${pr180_d4_dir}/root" \
   run_case "${pr180_d4_name}" "${pr180_d4_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview --wrapper /usr/bin/claw \
     --workspace-root "${pr180_d4_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_d4_name}" "${pr180_d4_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_d4_name}" "${pr180_d4_dir}/fake_claw.log"; then
    pass_case "${pr180_d4_name}"
  fi
fi

# ...and the documented escape hatch: an explicit `--wrapper` pointing at this
# wrapper is ABSOLUTE, so the child's chdir to a foreign workspace root cannot
# move it. Previewing into another tree stays available.
pr180_d5_name="pr180_preview_into_a_foreign_root_with_an_explicit_self_wrapper_is_local"
pr180_d5_dir="$(stage_layout "${pr180_d5_name}")"
install_canonical "${pr180_d5_dir}" current
mkdir -p "${pr180_d5_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_d5_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_d5_dir}/foreign/scripts/claw-sidestack-local"
if run_case "${pr180_d5_name}" "${pr180_d5_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper "${pr180_d5_dir}/root/scripts/claw-sidestack-local" \
     --workspace-root "${pr180_d5_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_d5_name}" "${pr180_d5_dir}/readiness.log" \
     && assert_log_contains "${pr180_d5_name}" "${pr180_d5_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_d5_name}"
  fi
fi

# The effective child wrapper does not resolve at all (the workspace root has
# no `scripts/` tree). Preview has no precheck that would stop this, so an
# unprovable spawn target fails CLOSED rather than being waved through.
pr180_d6_name="pr180_preview_with_an_unresolvable_workspace_root_wrapper_refuses"
pr180_d6_dir="$(stage_layout "${pr180_d6_name}")"
install_canonical "${pr180_d6_dir}" current
mkdir -p "${pr180_d6_dir}/bare"
if run_case "${pr180_d6_name}" "${pr180_d6_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --workspace-root "${pr180_d6_dir}/bare" >/dev/null; then
  if assert_readiness_not_called "${pr180_d6_name}" "${pr180_d6_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_d6_name}" "${pr180_d6_dir}/fake_claw.log"; then
    pass_case "${pr180_d6_name}"
  fi
fi

# ---------- P2-E: dry run cannot spawn a wrapper, so it must not be gated ---
#
# The dry-run arm of `run_plan_subcommand` builds its report from
# `validate_plan` + `preflight::precheck` only. It never binds a wrapper path,
# never probes the substrate and never spawns a subprocess -- so a foreign
# `--wrapper` names an executable that CANNOT run, and refusing on it blocks a
# purely local plan-validation workflow for no safety gain.

pr180_e1_name="pr180_dry_run_with_an_explicit_foreign_wrapper_is_local"
pr180_e1_dir="$(stage_layout "${pr180_e1_name}")"
install_canonical "${pr180_e1_dir}" current
if RUN_CASE_CWD="${pr180_e1_dir}/root" \
   run_case "${pr180_e1_name}" "${pr180_e1_dir}" 0 -- \
     plan run ./plan.yaml --dry-run --wrapper /usr/bin/claw >/dev/null; then
  if assert_readiness_not_called "${pr180_e1_name}" "${pr180_e1_dir}/readiness.log" \
     && assert_log_contains "${pr180_e1_name}" "${pr180_e1_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_e1_name}" "${pr180_e1_dir}/fake_claw.log"; then
    pass_case "${pr180_e1_name}"
  fi
fi

# Same, with the wrapper OMITTED from a CWD whose default would be foreign.
pr180_e2_name="pr180_dry_run_with_a_foreign_cwd_default_wrapper_is_local"
pr180_e2_dir="$(stage_layout "${pr180_e2_name}")"
install_canonical "${pr180_e2_dir}" current
mkdir -p "${pr180_e2_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_e2_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_e2_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_e2_dir}/foreign" \
   run_case "${pr180_e2_name}" "${pr180_e2_dir}" 0 -- \
     plan run ./plan.yaml --dry-run >/dev/null; then
  if assert_readiness_not_called "${pr180_e2_name}" "${pr180_e2_dir}/readiness.log" \
     && assert_log_contains "${pr180_e2_name}" "${pr180_e2_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_e2_name}" "${pr180_e2_dir}/fake_claw.log"; then
    pass_case "${pr180_e2_name}"
  fi
fi

# `--workspace-root` without `--workspace-write-preview` is a CLI parse error,
# so it cannot move a spawn under `--dry-run` either.
pr180_e3_name="pr180_dry_run_with_a_foreign_workspace_root_is_local"
pr180_e3_dir="$(stage_layout "${pr180_e3_name}")"
install_canonical "${pr180_e3_dir}" current
mkdir -p "${pr180_e3_dir}/elsewhere/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_e3_dir}/elsewhere/scripts/claw-sidestack-local"
chmod 0755 "${pr180_e3_dir}/elsewhere/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_e3_dir}/root" \
   run_case "${pr180_e3_name}" "${pr180_e3_dir}" 0 -- \
     plan run ./plan.yaml --dry-run --workspace-root "${pr180_e3_dir}/elsewhere" >/dev/null; then
  if assert_readiness_not_called "${pr180_e3_name}" "${pr180_e3_dir}/readiness.log" \
     && assert_log_contains "${pr180_e3_name}" "${pr180_e3_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_e3_name}"
  fi
fi

# A plain protected dry run keeps its existing local behavior.
pr180_e4_name="pr180_dry_run_from_this_tree_is_unchanged_local"
pr180_e4_dir="$(stage_layout "${pr180_e4_name}")"
install_canonical "${pr180_e4_dir}" current
if RUN_CASE_CWD="${pr180_e4_dir}/root" \
   run_case "${pr180_e4_name}" "${pr180_e4_dir}" 0 -- \
     plan run ./plan.yaml --dry-run >/dev/null; then
  if assert_readiness_not_called "${pr180_e4_name}" "${pr180_e4_dir}/readiness.log" \
     && assert_log_contains "${pr180_e4_name}" "${pr180_e4_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_e4_name}"
  fi
fi

# ---------- P1/P2-F: mixed and value-slot mode flags -----------------------
#
# `--dry-run` and `--workspace-write-preview` together are refused by
# `parse_plan_subcommand_args` BEFORE dispatch, and again by the preview branch
# itself. Both flags are order-independent booleans tested after the parse
# loop, so neither order can produce a spawn.

pr180_f1_name="pr180_dry_run_then_preview_cannot_spawn_and_is_local"
pr180_f1_dir="$(stage_layout "${pr180_f1_name}")"
install_canonical "${pr180_f1_dir}" current
mkdir -p "${pr180_f1_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_f1_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_f1_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_f1_dir}/root" \
   run_case "${pr180_f1_name}" "${pr180_f1_dir}" 0 -- \
     plan run ./plan.yaml --dry-run --workspace-write-preview \
     --workspace-root "${pr180_f1_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_f1_name}" "${pr180_f1_dir}/readiness.log" \
     && assert_log_contains "${pr180_f1_name}" "${pr180_f1_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_f1_name}"
  fi
fi

# ...and the reverse order is the same invocation.
pr180_f2_name="pr180_preview_then_dry_run_cannot_spawn_and_is_local"
pr180_f2_dir="$(stage_layout "${pr180_f2_name}")"
install_canonical "${pr180_f2_dir}" current
mkdir -p "${pr180_f2_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_f2_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_f2_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_f2_dir}/root" \
   run_case "${pr180_f2_name}" "${pr180_f2_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview --dry-run \
     --workspace-root "${pr180_f2_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_f2_name}" "${pr180_f2_dir}/readiness.log" \
     && assert_log_contains "${pr180_f2_name}" "${pr180_f2_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_f2_name}"
  fi
fi

# THE anti-shortcut guard. A `--dry-run` sitting in another plan flag's VALUE
# slot is that flag's value: the CLI parses `--fast-model --dry-run` as
# fast_model="--dry-run" and leaves dry_run FALSE, so this is a LIVE normal run
# and the foreign CWD default must still refuse. A classifier that merely tests
# whether argv contains `--dry-run` hands this invocation a bypass.
pr180_f3_name="pr180_dry_run_in_a_value_slot_is_not_a_dry_run"
pr180_f3_dir="$(stage_layout "${pr180_f3_name}")"
install_canonical "${pr180_f3_dir}" current
mkdir -p "${pr180_f3_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_f3_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_f3_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_f3_dir}/foreign" \
   run_case "${pr180_f3_name}" "${pr180_f3_dir}" 9 -- \
     plan run ./plan.yaml --fast-model --dry-run >/dev/null; then
  if assert_stderr_contains "${pr180_f3_name}" "${pr180_f3_dir}/wrapper.stderr" \
       'not proven to pass the readiness gate' \
     && assert_readiness_not_called "${pr180_f3_name}" "${pr180_f3_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_f3_name}" "${pr180_f3_dir}/fake_claw.log"; then
    pass_case "${pr180_f3_name}"
  fi
fi

# The same value-slot rule under preview: `--fast-model --dry-run` leaves this
# a LIVE preview, whose foreign workspace-root wrapper must still be proven.
pr180_f4_name="pr180_value_slot_dry_run_under_preview_still_gates_the_spawn_wrapper"
pr180_f4_dir="$(stage_layout "${pr180_f4_name}")"
install_canonical "${pr180_f4_dir}" current
mkdir -p "${pr180_f4_dir}/foreign/scripts"
printf '#!/usr/bin/env bash\nexit 0\n' > "${pr180_f4_dir}/foreign/scripts/claw-sidestack-local"
chmod 0755 "${pr180_f4_dir}/foreign/scripts/claw-sidestack-local"
if RUN_CASE_CWD="${pr180_f4_dir}/root" \
   run_case "${pr180_f4_name}" "${pr180_f4_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview --fast-model --dry-run \
     --workspace-root "${pr180_f4_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_f4_name}" "${pr180_f4_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_f4_name}" "${pr180_f4_dir}/fake_claw.log"; then
    pass_case "${pr180_f4_name}"
  fi
fi

# ---------- P1-R: an explicit RELATIVE --wrapper is resolved by the CHILD ----
#
# `build_claw_command` stores the operator's `--wrapper` value VERBATIM as
# `ClawCommand.program` and sets `cwd` to the effective workspace root
# (`runner.rs:130-162`), and `execute_with_timeout` spawns it as
# `Command::new(&cmd.program).current_dir(&cmd.cwd)` (`runner.rs:433-435`).
# `current_dir` is applied BEFORE the exec, so which executable actually runs
# depends on the PATH CLASS of the value:
#
#   absolute            the child's chdir cannot move it -- it is the literal
#                       program, and remains the documented escape hatch.
#   relative WITH slash execvp resolves it AFTER the chdir, so the real
#                       program is `<execution-base>/<value>`. Proving the
#                       caller's CWD holds an identically named path proves
#                       NOTHING about it.
#   bare name NO slash  execvp searches PATH, never the CWD. Nothing in the
#                       wrapper's contract can prove which PATH entry the
#                       child would pick, and this wrapper does not fall back
#                       to PATH, so it fails CLOSED.
#
# The execution base differs by mode: preview relocates via `--workspace-root`,
# while a normal run cannot (`--workspace-root` without
# `--workspace-write-preview` is a parse error at `main.rs:4661-4668`), so its
# base stays the CWD.

# assert_foreign_child_not_called <name> <log_file>
assert_foreign_child_not_called() {
  local name="$1"
  local log_file="$2"
  if [[ -s "${log_file}" ]]; then
    fail_case "${name}" "the foreign workspace-root wrapper was executed" \
      /dev/null /dev/null "${log_file}"
    return 1
  fi
}

# write_logging_foreign_wrapper <path> <log_file>
write_logging_foreign_wrapper() {
  local path="$1" log_file="$2"
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<EOF
#!/usr/bin/env bash
printf 'FOREIGN_CHILD_CALLED=1\n' >> '${log_file}'
exit 0
EOF
  chmod 0755 "${path}"
}

# --- A: the exact residual P1 ----------------------------------------------
# Protected CWD holds the real wrapper, so a CWD-based identity check reads
# "this is me". But preview spawns with `current_dir(/foreign)`, so the
# executable that really runs is `/foreign/scripts/claw-sidestack-local`.
pr180_r1_name="pr180_preview_with_relative_explicit_self_wrapper_and_foreign_root_refuses"
pr180_r1_dir="$(stage_layout "${pr180_r1_name}")"
install_canonical "${pr180_r1_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r1_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r1_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r1_dir}/root" \
   run_case "${pr180_r1_name}" "${pr180_r1_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper scripts/claw-sidestack-local \
     --workspace-root "${pr180_r1_dir}/foreign" >/dev/null; then
  if assert_stderr_contains "${pr180_r1_name}" "${pr180_r1_dir}/wrapper.stderr" \
       'not proven to pass the readiness gate' \
     && assert_readiness_not_called "${pr180_r1_name}" "${pr180_r1_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r1_name}" "${pr180_r1_dir}/foreign_child.log" \
     && assert_fake_not_called "${pr180_r1_name}" "${pr180_r1_dir}/fake_claw.log"; then
    pass_case "${pr180_r1_name}"
  fi
fi

# --- B: relative preview converse -------------------------------------------
# Mirror image: the CWD has no `scripts/` tree at all, but the EFFECTIVE
# workspace root is this wrapper's tree, so the relative value resolves to
# this wrapper and the run is permitted. Resolving against the caller's CWD
# would wrongly refuse this.
pr180_r2_name="pr180_preview_relative_explicit_wrapper_resolving_to_self_under_the_root_is_local"
pr180_r2_dir="$(stage_layout "${pr180_r2_name}")"
install_canonical "${pr180_r2_dir}" current
mkdir -p "${pr180_r2_dir}/bare"
if RUN_CASE_CWD="${pr180_r2_dir}/bare" \
   run_case "${pr180_r2_name}" "${pr180_r2_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper scripts/claw-sidestack-local \
     --workspace-root "${pr180_r2_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_r2_name}" "${pr180_r2_dir}/readiness.log" \
     && assert_log_contains "${pr180_r2_name}" "${pr180_r2_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_r2_name}" "${pr180_r2_dir}/fake_claw.log"; then
    pass_case "${pr180_r2_name}"
  fi
fi

# --- C: the ABSOLUTE escape hatch still works -------------------------------
# Same foreign root as case A -- and the foreign tree really does hold its own
# `scripts/claw-sidestack-local` -- but an ABSOLUTE program path ignores the
# child's chdir, so this stays the documented remedy.
pr180_r3_name="pr180_preview_absolute_self_wrapper_into_a_foreign_root_holding_a_decoy_is_local"
pr180_r3_dir="$(stage_layout "${pr180_r3_name}")"
install_canonical "${pr180_r3_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r3_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r3_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r3_dir}/root" \
   run_case "${pr180_r3_name}" "${pr180_r3_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper "${pr180_r3_dir}/root/scripts/claw-sidestack-local" \
     --workspace-root "${pr180_r3_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_r3_name}" "${pr180_r3_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r3_name}" "${pr180_r3_dir}/foreign_child.log" \
     && assert_log_contains "${pr180_r3_name}" "${pr180_r3_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_r3_name}"
  fi
fi

# --- D: an ABSOLUTE foreign wrapper is still refused -------------------------
pr180_r4_name="pr180_preview_absolute_foreign_wrapper_refuses"
pr180_r4_dir="$(stage_layout "${pr180_r4_name}")"
install_canonical "${pr180_r4_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r4_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r4_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r4_dir}/root" \
   run_case "${pr180_r4_name}" "${pr180_r4_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper "${pr180_r4_dir}/foreign/scripts/claw-sidestack-local" \
     --workspace-root "${pr180_r4_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_r4_name}" "${pr180_r4_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r4_name}" "${pr180_r4_dir}/foreign_child.log" \
     && assert_fake_not_called "${pr180_r4_name}" "${pr180_r4_dir}/fake_claw.log"; then
    pass_case "${pr180_r4_name}"
  fi
fi

# --- E: last-wins, self-ABSOLUTE then relative-foreign ----------------------
# The EFFECTIVE value is the trailing relative one, and it is judged against
# the preview execution base. A leading safe absolute cannot launder it.
pr180_r5_name="pr180_preview_last_wins_absolute_self_then_relative_foreign_refuses"
pr180_r5_dir="$(stage_layout "${pr180_r5_name}")"
install_canonical "${pr180_r5_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r5_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r5_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r5_dir}/root" \
   run_case "${pr180_r5_name}" "${pr180_r5_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper "${pr180_r5_dir}/root/scripts/claw-sidestack-local" \
     --wrapper scripts/claw-sidestack-local \
     --workspace-root "${pr180_r5_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_r5_name}" "${pr180_r5_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r5_name}" "${pr180_r5_dir}/foreign_child.log" \
     && assert_fake_not_called "${pr180_r5_name}" "${pr180_r5_dir}/fake_claw.log"; then
    pass_case "${pr180_r5_name}"
  fi
fi

# --- F: last-wins, relative-foreign then self-ABSOLUTE ----------------------
# Converse of E: the trailing absolute self value wins, so this is permitted
# and must not be refused on the strength of the earlier relative token.
pr180_r6_name="pr180_preview_last_wins_relative_foreign_then_absolute_self_is_local"
pr180_r6_dir="$(stage_layout "${pr180_r6_name}")"
install_canonical "${pr180_r6_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r6_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r6_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r6_dir}/root" \
   run_case "${pr180_r6_name}" "${pr180_r6_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper scripts/claw-sidestack-local \
     --wrapper "${pr180_r6_dir}/root/scripts/claw-sidestack-local" \
     --workspace-root "${pr180_r6_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_r6_name}" "${pr180_r6_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r6_name}" "${pr180_r6_dir}/foreign_child.log" \
     && assert_log_contains "${pr180_r6_name}" "${pr180_r6_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_r6_name}"
  fi
fi

# --- G: dry run keeps bypassing wrapper identity entirely -------------------
# No child wrapper can exist under `--dry-run`, so a foreign-looking relative
# value is not a reason to refuse. This is the P2 fix; it must not regress.
pr180_r7_name="pr180_dry_run_with_a_relative_foreign_wrapper_is_local"
pr180_r7_dir="$(stage_layout "${pr180_r7_name}")"
install_canonical "${pr180_r7_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r7_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r7_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r7_dir}/foreign" \
   run_case "${pr180_r7_name}" "${pr180_r7_dir}" 0 -- \
     plan run ./plan.yaml --dry-run \
     --wrapper scripts/claw-sidestack-local >/dev/null; then
  if assert_readiness_not_called "${pr180_r7_name}" "${pr180_r7_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r7_name}" "${pr180_r7_dir}/foreign_child.log" \
     && assert_log_contains "${pr180_r7_name}" "${pr180_r7_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_r7_name}"
  fi
fi

# --- equals form obeys the same base ----------------------------------------
pr180_r8_name="pr180_preview_equals_form_relative_self_wrapper_and_foreign_root_refuses"
pr180_r8_dir="$(stage_layout "${pr180_r8_name}")"
install_canonical "${pr180_r8_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r8_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r8_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r8_dir}/root" \
   run_case "${pr180_r8_name}" "${pr180_r8_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper=scripts/claw-sidestack-local \
     --workspace-root "${pr180_r8_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_r8_name}" "${pr180_r8_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r8_name}" "${pr180_r8_dir}/foreign_child.log" \
     && assert_fake_not_called "${pr180_r8_name}" "${pr180_r8_dir}/fake_claw.log"; then
    pass_case "${pr180_r8_name}"
  fi
fi

# --- a `..` relative value is resolved from the same base -------------------
# `foreign/../root/scripts/claw-sidestack-local` IS this wrapper, so the run is
# permitted -- the base is what matters, not the shape of the value.
pr180_r9_name="pr180_preview_dotdot_relative_wrapper_resolving_to_self_is_local"
pr180_r9_dir="$(stage_layout "${pr180_r9_name}")"
install_canonical "${pr180_r9_dir}" current
mkdir -p "${pr180_r9_dir}/foreign"
if RUN_CASE_CWD="${pr180_r9_dir}/root" \
   run_case "${pr180_r9_name}" "${pr180_r9_dir}" 0 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper ../root/scripts/claw-sidestack-local \
     --workspace-root "${pr180_r9_dir}/foreign" >/dev/null; then
  if assert_readiness_not_called "${pr180_r9_name}" "${pr180_r9_dir}/readiness.log" \
     && assert_log_contains "${pr180_r9_name}" "${pr180_r9_dir}/fake_claw.log" 'IDENTITY=CANONICAL'; then
    pass_case "${pr180_r9_name}"
  fi
fi

# --- a BARE name is PATH-searched and therefore unprovable ------------------
# execvp never consults the CWD for a slash-less program, and this wrapper has
# no PATH fallback to lean on, so the only safe answer is to refuse -- even
# when the workspace root IS this wrapper's own tree.
pr180_r10_name="pr180_preview_bare_name_wrapper_is_unprovable_and_refuses"
pr180_r10_dir="$(stage_layout "${pr180_r10_name}")"
install_canonical "${pr180_r10_dir}" current
if RUN_CASE_CWD="${pr180_r10_dir}/root" \
   run_case "${pr180_r10_name}" "${pr180_r10_dir}" 9 -- \
     plan run ./plan.yaml --workspace-write-preview \
     --wrapper claw-sidestack-local \
     --workspace-root "${pr180_r10_dir}/root" >/dev/null; then
  if assert_readiness_not_called "${pr180_r10_name}" "${pr180_r10_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_r10_name}" "${pr180_r10_dir}/fake_claw.log"; then
    pass_case "${pr180_r10_name}"
  fi
fi

# --- NORMAL mode keeps its own base -----------------------------------------
# A normal run cannot be relocated by `--workspace-root`, so its relative
# explicit wrapper is judged against the CWD. Running from this tree with the
# runner's own relative default is permitted.
pr180_r11_name="pr180_normal_relative_explicit_self_wrapper_from_this_tree_is_local"
pr180_r11_dir="$(stage_layout "${pr180_r11_name}")"
install_canonical "${pr180_r11_dir}" current
if RUN_CASE_CWD="${pr180_r11_dir}/root" \
   run_case "${pr180_r11_name}" "${pr180_r11_dir}" 0 -- \
     plan run ./plan.yaml --wrapper scripts/claw-sidestack-local >/dev/null; then
  if assert_readiness_not_called "${pr180_r11_name}" "${pr180_r11_dir}/readiness.log" \
     && assert_log_contains "${pr180_r11_name}" "${pr180_r11_dir}/fake_claw.log" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${pr180_r11_name}" "${pr180_r11_dir}/fake_claw.log"; then
    pass_case "${pr180_r11_name}"
  fi
fi

# ...and the same shape from a FOREIGN tree is refused, because the CWD-based
# base is exactly what a normal run would spawn.
pr180_r12_name="pr180_normal_relative_explicit_wrapper_from_a_foreign_tree_refuses"
pr180_r12_dir="$(stage_layout "${pr180_r12_name}")"
install_canonical "${pr180_r12_dir}" current
write_logging_foreign_wrapper \
  "${pr180_r12_dir}/foreign/scripts/claw-sidestack-local" \
  "${pr180_r12_dir}/foreign_child.log"
if RUN_CASE_CWD="${pr180_r12_dir}/foreign" \
   run_case "${pr180_r12_name}" "${pr180_r12_dir}" 9 -- \
     plan run ./plan.yaml --wrapper scripts/claw-sidestack-local >/dev/null; then
  if assert_readiness_not_called "${pr180_r12_name}" "${pr180_r12_dir}/readiness.log" \
     && assert_foreign_child_not_called "${pr180_r12_name}" "${pr180_r12_dir}/foreign_child.log" \
     && assert_fake_not_called "${pr180_r12_name}" "${pr180_r12_dir}/fake_claw.log"; then
    pass_case "${pr180_r12_name}"
  fi
fi

# ---------- cases pr180_r13-r17: the ACP compatibility flags stay local -----
# `parse_args_with_terminal` rewrites BOTH `--acp` and `-acp` into the bare
# positional `acp`, which dispatches `print_acp_status` -- a local printer that
# constructs no LiveCli and issues no provider request. `claw acp` is already
# covered by the non-inference loop above; these cases pin the two dash forms
# to the same verdict, and pin the two terminals that must still outrank them.

# ---------- case pr180_r13: `--acp` is local -------------------------------
pr180_r13_name="pr180_acp_long_compatibility_flag_is_local"
pr180_r13_dir="$(stage_layout "${pr180_r13_name}")"
install_canonical "${pr180_r13_dir}" current
if run_case "${pr180_r13_name}" "${pr180_r13_dir}" 0 -- --acp >/dev/null; then
  if assert_canonical_ran "${pr180_r13_name}" "${pr180_r13_dir}" --acp \
     && assert_readiness_not_called "${pr180_r13_name}" "${pr180_r13_dir}/readiness.log" \
     && assert_no_decoy_executed "${pr180_r13_name}" "${pr180_r13_dir}/fake_claw.log"; then
    pass_case "${pr180_r13_name}"
  fi
fi

# ---------- case pr180_r14: `-acp` is local --------------------------------
# The single-dash spelling is a real CLI arm, not a typo the parser tolerates,
# so it must not be swept up by the leading-unknown-dash drop either.
pr180_r14_name="pr180_acp_short_compatibility_flag_is_local"
pr180_r14_dir="$(stage_layout "${pr180_r14_name}")"
install_canonical "${pr180_r14_dir}" current
if run_case "${pr180_r14_name}" "${pr180_r14_dir}" 0 -- -acp >/dev/null; then
  if assert_canonical_ran "${pr180_r14_name}" "${pr180_r14_dir}" -acp \
     && assert_readiness_not_called "${pr180_r14_name}" "${pr180_r14_dir}/readiness.log" \
     && assert_no_decoy_executed "${pr180_r14_name}" "${pr180_r14_dir}/fake_claw.log"; then
    pass_case "${pr180_r14_name}"
  fi
fi

# ---------- case pr180_r15: the ACP tail rides along with the rewrite ------
# The rewrite must PUSH a positional, not swallow the flag: `--acp serve` has
# to reconstruct as `acp serve`, the same two-token vector `acp serve` builds,
# so the tail keeps deciding what the invocation is. A rewrite that dropped
# the tail would classify on `acp` alone and call every `--acp …` shape local
# no matter what followed.
pr180_r15_name="pr180_acp_compatibility_flag_keeps_its_tail"
pr180_r15_dir="$(stage_layout "${pr180_r15_name}")"
install_canonical "${pr180_r15_dir}" current
if run_case "${pr180_r15_name}" "${pr180_r15_dir}" 0 -- --acp serve >/dev/null; then
  if assert_canonical_ran "${pr180_r15_name}" "${pr180_r15_dir}" --acp \
     && assert_readiness_not_called "${pr180_r15_name}" "${pr180_r15_dir}/readiness.log" \
     && assert_no_decoy_executed "${pr180_r15_name}" "${pr180_r15_dir}/fake_claw.log"; then
    pass_case "${pr180_r15_name}"
  fi
fi

# ---------- case pr180_r16: `-p` still outranks the ACP flag ---------------
# `-p` RETURNS `CliAction::Prompt` from inside the CLI's argument loop, so a
# leading `--acp` never gets to make the invocation local. The classifier
# settles the `-p` terminal before it reads the reconstructed vector at all;
# this pins that ordering so the ACP rewrite can never become an inference
# bypass. Readiness is queried, refuses, and the canonical claw never runs.
pr180_r16_name="pr180_acp_compatibility_flag_does_not_mask_a_prompt_terminal"
pr180_r16_dir="$(stage_layout "${pr180_r16_name}")"
install_canonical "${pr180_r16_dir}" current
write_fake_readiness_client "${pr180_r16_dir}" 0 200 "$(refusal_body BROKER_BUSY)"
if run_case "${pr180_r16_name}" "${pr180_r16_dir}" 8 -- \
     --acp --model fast -p hello >/dev/null; then
  if assert_readiness_called_for "${pr180_r16_name}" "${pr180_r16_dir}/readiness.log" \
       'qwen3%3A14b' \
     && assert_stderr_contains "${pr180_r16_name}" "${pr180_r16_dir}/wrapper.stderr" \
          'local coding readiness refused' \
     && assert_fake_not_called "${pr180_r16_name}" "${pr180_r16_dir}/fake_claw.log" \
     && assert_no_decoy_executed "${pr180_r16_name}" "${pr180_r16_dir}/fake_claw.log"; then
    pass_case "${pr180_r16_name}"
  fi
fi

# ---------- case pr180_r17: `--resume` still outranks the ACP flag ---------
# `--acp --resume` is a CLI USAGE ERROR, not a resumed session: `--acp` fills
# the positional vector first, so the `--resume` arm's `rest.is_empty()` guard
# fails and `--resume` is pushed as a plain positional, leaving
# `parse_acp_args(["--resume"])` to reject the pair. This wrapper does not
# model that distinction -- `n6_forces_inference` settles `--resume` before the
# vector is read -- so the invocation stays GATED. That is the deliberate
# fail-closed direction, and it is pinned here: the ACP rewrite must not
# relax it into a local dispatch.
pr180_r17_name="pr180_acp_compatibility_flag_does_not_mask_resume"
pr180_r17_dir="$(stage_layout "${pr180_r17_name}")"
install_canonical "${pr180_r17_dir}" current
if run_case "${pr180_r17_name}" "${pr180_r17_dir}" 9 -- --acp --resume >/dev/null; then
  if assert_stderr_contains "${pr180_r17_name}" "${pr180_r17_dir}/wrapper.stderr" \
       'readiness requires an explicit --model for this invocation' \
     && assert_readiness_not_called "${pr180_r17_name}" "${pr180_r17_dir}/readiness.log" \
     && assert_fake_not_called "${pr180_r17_name}" "${pr180_r17_dir}/fake_claw.log" \
     && assert_no_decoy_executed "${pr180_r17_name}" "${pr180_r17_dir}/fake_claw.log"; then
    pass_case "${pr180_r17_name}"
  fi
fi

# ---------- P1: in-process enforcement marker -------------------------------
#
# The wrapper can only gate what it sees before `exec`. These cases prove it
# hands the canonical runtime the marker that turns on the in-process guard, so
# a post-exec `/model` switch, an Agent fallback, or a provider retry is still
# admission-gated. The Rust side of the contract is covered by
# `rust/crates/api/tests/n6_admission_integration.rs`.

# ---------- case pr180_p1_1: marker reaches canonical on an inference run ----
pr180_p1_1_name="pr180_p1_marker_is_exported_on_an_inference_dispatch"
pr180_p1_1_dir="$(stage_layout "${pr180_p1_1_name}")"
install_canonical "${pr180_p1_1_dir}" current
if run_case "${pr180_p1_1_name}" "${pr180_p1_1_dir}" 0 -- --model fast prompt "say hi" >/dev/null; then
  if assert_log_contains "${pr180_p1_1_name}" "${pr180_p1_1_dir}/fake_claw.log" \
       'CLAW_SIDESTACK_N6_ENFORCE=1' \
     && assert_readiness_called_for "${pr180_p1_1_name}" "${pr180_p1_1_dir}/readiness.log" \
          'qwen3%3A14b' \
     && assert_no_decoy_executed "${pr180_p1_1_name}" "${pr180_p1_1_dir}/fake_claw.log"; then
    pass_case "${pr180_p1_1_name}"
  fi
fi

# ---------- case pr180_p1_2: marker reaches canonical on a local dispatch ----
# `--acp` never queries readiness, but the process it starts can still change
# models later, so it must carry the marker too.
pr180_p1_2_name="pr180_p1_marker_is_exported_on_a_local_dispatch"
pr180_p1_2_dir="$(stage_layout "${pr180_p1_2_name}")"
install_canonical "${pr180_p1_2_dir}" current
if run_case "${pr180_p1_2_name}" "${pr180_p1_2_dir}" 0 -- --acp >/dev/null; then
  if assert_log_contains "${pr180_p1_2_name}" "${pr180_p1_2_dir}/fake_claw.log" \
       'CLAW_SIDESTACK_N6_ENFORCE=1' \
     && assert_readiness_not_called "${pr180_p1_2_name}" "${pr180_p1_2_dir}/readiness.log" \
     && assert_no_decoy_executed "${pr180_p1_2_name}" "${pr180_p1_2_dir}/fake_claw.log"; then
    pass_case "${pr180_p1_2_name}"
  fi
fi

# ---------- case pr180_p1_3: a refused run starts nothing to mark ------------
# When readiness refuses, the wrapper exits 8 and never execs, so no marked
# process exists at all.
pr180_p1_3_name="pr180_p1_marker_never_reaches_a_refused_dispatch"
pr180_p1_3_dir="$(stage_layout "${pr180_p1_3_name}")"
install_canonical "${pr180_p1_3_dir}" current
write_fake_readiness_client "${pr180_p1_3_dir}" 0 200 "$(refusal_body VRAM_CONTENDED qwen3:14b)"
if run_case "${pr180_p1_3_name}" "${pr180_p1_3_dir}" 8 -- --model fast prompt "say hi" >/dev/null; then
  if assert_fake_not_called "${pr180_p1_3_name}" "${pr180_p1_3_dir}/fake_claw.log" \
     && assert_no_decoy_executed "${pr180_p1_3_name}" "${pr180_p1_3_dir}/fake_claw.log"; then
    pass_case "${pr180_p1_3_name}"
  fi
fi

# ---------- case pr180_p1_4: the marker is a bare literal, not a secret ------
# Enforcement state must be set by this script and nothing else, and it must
# never carry a value derived from the environment or a credential.
pr180_p1_4_name="pr180_p1_marker_is_a_bare_literal_set_only_before_exec"
pr180_p1_4_marker_lines="$(grep -c 'CLAW_SIDESTACK_N6_ENFORCE' "${REAL_WRAPPER}" || true)"
pr180_p1_4_literal="$(grep -c '^export CLAW_SIDESTACK_N6_ENFORCE=1$' "${REAL_WRAPPER}" || true)"
# The line after the marker must be the exec, so nothing can run between the
# two and no later line can unset or overwrite it.
pr180_p1_4_next="$( { grep -A2 '^export CLAW_SIDESTACK_N6_ENFORCE=1$' "${REAL_WRAPPER}" || true; } | tail -1)"
# shellcheck disable=SC2016  # deliberate literal: this is the exact source line
# the wrapper must carry, so it must NOT expand here.
pr180_p1_4_expected_exec='exec "${CANONICAL_CLAW}" "$@"'
if [[ "${pr180_p1_4_literal}" == "1" ]] \
   && [[ "${pr180_p1_4_marker_lines}" == "1" ]] \
   && [[ "${pr180_p1_4_next}" == "${pr180_p1_4_expected_exec}" ]]; then
  pass_case "${pr180_p1_4_name}"
else
  fail_case "${pr180_p1_4_name}" \
    'the marker must appear exactly once, as a bare literal, immediately before exec' \
    /dev/null /dev/null "${REAL_WRAPPER}"
fi

# ===========================================================================
# PR180 P1: the canonical capability floor
#
# Exporting the enforcement marker is a REQUEST. A canonical binary built
# before the in-process contract existed accepts it and ignores it, and every
# model chosen after exec — a REPL /model switch, an Agent fallback, a provider
# retry — then runs through an ungated API while the startup gate reports
# success.
#
# Freshness cannot stand in for that proof. CURRENT means "this build's Git SHA
# equals the locally known origin/main": a worktree legitimately AHEAD of
# origin/main is never CURRENT yet may implement the contract, and an install
# that matches origin/main exactly may well not. `CLAW_SIDESTACK_ALLOW_STALE=1`
# and UNKNOWN_BASE admit binaries of unknown vintage outright.
#
# So these cases hold freshness and capability apart in both directions, and
# require the refusal to land BEFORE any readiness request or exec.
# ===========================================================================

# assert_capability_refusal <name> <case_dir>
#
# The shape every capability refusal must have: nothing was asked of the
# broker, the curl probe never even ran (the refusal precedes it), canonical
# never executed, and no decoy was reached instead.
assert_capability_refusal() {
  local name="$1" case_dir="$2"
  local log="${case_dir}/fake_claw.log"
  if [[ -s "${case_dir}/readiness.log" ]]; then
    fail_case "${name}" "a readiness request was issued for a binary that cannot enforce" \
      /dev/null /dev/null "${case_dir}/readiness.log"
    return 1
  fi
  if [[ -s "${case_dir}/curlversion.log" ]]; then
    fail_case "${name}" "the curl probe ran, so the capability refusal did not precede it" \
      /dev/null /dev/null "${case_dir}/curlversion.log"
    return 1
  fi
  assert_fake_not_called "${name}" "${log}" || return 1
  assert_no_decoy_executed "${name}" "${log}" || return 1
  assert_stderr_contains "${name}" "${case_dir}/wrapper.stderr" \
    'does not prove it implements the in-process N6 enforcement contract' || return 1
}

# stage_status_capability_report <case_dir> <shape>
#
# Replaces the status helper with a stub that reports CURRENT and a capability
# field of the given shape. Models what the wrapper must do with a report it
# cannot read cleanly — including one produced by a status helper that predates
# this contract entirely.
stage_status_capability_report() {
  local case_dir="$1" shape="$2"
  local stub="${case_dir}/root/scripts/claw-canonical-status"
  {
    printf '#!/usr/bin/env bash\n'
    printf "printf 'canonical topology:  regular-executable\\\\n'\n"
    case "${shape}" in
      absent) : ;;
      ambiguous)
        printf "printf 'n6 capability:       supported\\\\n'\n"
        printf "printf 'n6 capability:       unsupported\\\\n'\n"
        ;;
      malformed) printf "printf 'n6 capability:       probably\\\\n'\n" ;;
      blank) printf "printf 'n6 capability:       \\\\n'\n" ;;
      *) printf 'test setup: unknown capability report shape %s\n' "${shape}" >&2; exit 2 ;;
    esac
    printf "printf 'state:               CURRENT\\\\n'\n"
    printf 'exit 0\n'
  } > "${stub}"
  chmod 0755 "${stub}"
}

# ---------- capability 1: CURRENT is not capability ----------
#
# THE case. The install matches origin/main exactly — freshness is perfect —
# and the binary still cannot enforce anything. Against the pre-repair wrapper
# this invocation queried the broker and exec'd.
cap1_name="capability_current_but_unsupported_binary_refuses_inference"
cap1_dir="$(stage_layout "${cap1_name}")"
install_canonical "${cap1_dir}" current "__NONE__"
if run_case "${cap1_name}" "${cap1_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap1_name}" "${cap1_dir}" \
     && assert_stderr_contains "${cap1_name}" "${cap1_dir}/wrapper.stderr" \
          'does not advertise the enforcement capability'; then
    pass_case "${cap1_name}"
  fi
fi

# ---------- capability 2: CURRENT and capable proceeds unchanged ----------
cap2_name="capability_current_and_supported_binary_runs_inference"
cap2_dir="$(stage_layout "${cap2_name}")"
install_canonical "${cap2_dir}" current
if run_case "${cap2_name}" "${cap2_dir}" 0 -- --model fast prompt "should run" >/dev/null; then
  log="${cap2_dir}/fake_claw.log"
  if assert_readiness_called_for "${cap2_name}" "${cap2_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_readiness_url_is_broker_only "${cap2_name}" "${cap2_dir}/readiness.log" \
     && assert_log_contains "${cap2_name}" "${log}" 'IDENTITY=CANONICAL' \
     && assert_log_contains "${cap2_name}" "${log}" 'CLAW_SIDESTACK_N6_ENFORCE=1' \
     && assert_no_decoy_executed "${cap2_name}" "${log}"; then
    pass_case "${cap2_name}"
  fi
fi

# ---------- capability 3: the stale override does not buy capability --------
#
# CLAW_SIDESTACK_ALLOW_STALE=1 is an explicit operator decision about
# FRESHNESS. It says nothing about whether the binary can enforce, so it must
# not admit one that cannot.
cap3_name="capability_stale_override_with_unsupported_binary_refuses"
cap3_dir="$(stage_layout "${cap3_name}")"
install_canonical "${cap3_dir}" stale "__NONE__"
if run_case "${cap3_name}" "${cap3_dir}" 10 CLAW_SIDESTACK_ALLOW_STALE=1 \
     -- --model fast prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap3_name}" "${cap3_dir}" \
     && assert_stderr_contains "${cap3_name}" "${cap3_dir}/wrapper.stderr" \
          'this is NOT the staleness check'; then
    pass_case "${cap3_name}"
  fi
fi

# ---------- capability 4: the stale override still works when capable -------
#
# The operator's deliberate stale override is preserved: a stale build that DOES
# implement the contract runs exactly as it did before this repair.
cap4_name="capability_stale_override_with_supported_binary_still_runs"
cap4_dir="$(stage_layout "${cap4_name}")"
install_canonical "${cap4_dir}" stale
if run_case "${cap4_name}" "${cap4_dir}" 0 CLAW_SIDESTACK_ALLOW_STALE=1 \
     -- --model fast prompt "should run" >/dev/null; then
  log="${cap4_dir}/fake_claw.log"
  if assert_stderr_contains "${cap4_name}" "${cap4_dir}/wrapper.stderr" \
          'CLAW_SIDESTACK_ALLOW_STALE=1 was set' \
     && assert_readiness_called_for "${cap4_name}" "${cap4_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_log_contains "${cap4_name}" "${log}" 'IDENTITY=CANONICAL_STALE' \
     && assert_log_contains "${cap4_name}" "${log}" 'CLAW_SIDESTACK_N6_ENFORCE=1' \
     && assert_no_decoy_executed "${cap4_name}" "${log}"; then
    pass_case "${cap4_name}"
  fi
fi

# ---------- capability 5: UNKNOWN_BASE does not buy capability --------------
#
# With no locally known origin/main there is no freshness answer at all, which
# is precisely when inferring capability from freshness would be worst.
cap5_name="capability_unknown_base_with_unsupported_binary_refuses"
cap5_dir="$(stage_layout "${cap5_name}")"
install_canonical "${cap5_dir}" current "__NONE__"
git -C "${cap5_dir}/root" update-ref -d refs/remotes/origin/main
if run_case "${cap5_name}" "${cap5_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap5_name}" "${cap5_dir}"; then
    pass_case "${cap5_name}"
  fi
fi

# ---------- capability 6: UNKNOWN_BASE still runs when capable --------------
cap6_name="capability_unknown_base_with_supported_binary_still_runs"
cap6_dir="$(stage_layout "${cap6_name}")"
install_canonical "${cap6_dir}" current
git -C "${cap6_dir}/root" update-ref -d refs/remotes/origin/main
if run_case "${cap6_name}" "${cap6_dir}" 0 -- --model fast prompt "should run" >/dev/null; then
  log="${cap6_dir}/fake_claw.log"
  if assert_stderr_contains "${cap6_name}" "${cap6_dir}/wrapper.stderr" \
          'freshness could not be determined' \
     && assert_readiness_called_for "${cap6_name}" "${cap6_dir}/readiness.log" 'qwen3%3A14b' \
     && assert_log_contains "${cap6_name}" "${log}" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${cap6_name}" "${log}"; then
    pass_case "${cap6_name}"
  fi
fi

# ---------- capability 7: near-miss tokens are not the capability -----------
#
# A substring or prefix matcher would accept every one of these. `-v1` is a
# version, and a token that merely contains the contract name is not it.
for cap_near_miss in \
  "sidestack-n6-enforce-v10" \
  "not-sidestack-n6-enforce-v1" \
  "sidestack-n6-enforce" ; do
  cap7_name="capability_near_miss_token_refuses__${cap_near_miss}"
  cap7_dir="$(stage_layout "${cap7_name}")"
  install_canonical "${cap7_dir}" current "${cap_near_miss}"
  if run_case "${cap7_name}" "${cap7_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
    if assert_capability_refusal "${cap7_name}" "${cap7_dir}"; then
      pass_case "${cap7_name}"
    fi
  fi
done

# ---------- capability 8: an unreadable report is not a permissive one ------
#
# A status helper predating this contract emits no capability field; a report
# carrying two fields, a value this wrapper does not define, or a blank one
# cannot be resolved. None of them may be read as permission.
for cap_shape in absent ambiguous malformed blank; do
  cap8_name="capability_unreadable_status_report_refuses__${cap_shape}"
  cap8_dir="$(stage_layout "${cap8_name}")"
  install_canonical "${cap8_dir}" current
  stage_status_capability_report "${cap8_dir}" "${cap_shape}"
  if run_case "${cap8_name}" "${cap8_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
    if assert_capability_refusal "${cap8_name}" "${cap8_dir}"; then
      pass_case "${cap8_name}"
    fi
  fi
done

# ---------- capability 9: the refusal precedes the curl floor ---------------
#
# Ordering proof. The curl version floor would itself refuse this invocation
# (exit 9), so seeing exit 10 with an empty curl-probe log proves the
# capability check ran first — a binary that cannot enforce does not get to
# consume any preflight work on its way to running ungated.
cap9_name="capability_refusal_precedes_the_curl_version_probe"
cap9_dir="$(stage_layout "${cap9_name}")"
install_canonical "${cap9_dir}" current "__NONE__"
write_fake_readiness_client "${cap9_dir}" 0 200 "${READY_QWEN3_14B}" \
  'curl 7.81.0 (x86_64-pc-linux-gnu) libcurl/7.81.0'
if run_case "${cap9_name}" "${cap9_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap9_name}" "${cap9_dir}"; then
    pass_case "${cap9_name}"
  fi
fi

# ---------- capability 10: the refusal precedes model resolution ------------
#
# An inference-capable invocation with no --model would refuse with exit 9. The
# capability floor must come first, so the operator is told the real problem.
cap10_name="capability_refusal_precedes_the_model_resolution_refusal"
cap10_dir="$(stage_layout "${cap10_name}")"
install_canonical "${cap10_dir}" current "__NONE__"
if run_case "${cap10_name}" "${cap10_dir}" 10 -- prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap10_name}" "${cap10_dir}"; then
    pass_case "${cap10_name}"
  fi
fi

# ---------- capability 11: staleness still outranks it without an override --
#
# Precedence is unchanged for the existing rules: a STALE binary with no
# override refuses as STALE (exit 6), whether or not it is capable.
cap11_name="capability_does_not_displace_the_stale_refusal"
cap11_dir="$(stage_layout "${cap11_name}")"
install_canonical "${cap11_dir}" stale
if run_case "${cap11_name}" "${cap11_dir}" 6 -- --model fast prompt "should refuse" >/dev/null; then
  log="${cap11_dir}/fake_claw.log"
  if assert_readiness_not_called "${cap11_name}" "${cap11_dir}/readiness.log" \
     && assert_stderr_contains "${cap11_name}" "${cap11_dir}/wrapper.stderr" \
          'refusing to run a STALE canonical claw' \
     && assert_fake_not_called "${cap11_name}" "${log}"; then
    pass_case "${cap11_name}"
  fi
fi

# ---------- capability 12: local-only commands are NOT gated ----------------
#
# The capability requirement protects model-bearing invocations. A command
# proven unable to reach a provider has nothing for the in-process guard to
# protect, so an older canonical binary must still print its own help, report
# its own status, and speak ACP — under the existing freshness rules, unchanged.
capability_local_case() {
  local label="$1"
  shift
  local name="capability_does_not_gate_local_${label}"
  local dir
  dir="$(stage_layout "${name}")"
  install_canonical "${dir}" current "__NONE__"
  if run_case "${name}" "${dir}" 0 -- "$@" >/dev/null; then
    if assert_readiness_not_called "${name}" "${dir}/readiness.log" \
       && assert_canonical_ran "${name}" "${dir}" "$1" \
       && assert_no_decoy_executed "${name}" "${dir}/fake_claw.log"; then
      pass_case "${name}"
    fi
  fi
}

capability_local_case help_flag --help
capability_local_case version_flag --version
capability_local_case version_subcommand version
capability_local_case status_subcommand status
capability_local_case doctor_subcommand doctor
capability_local_case acp_subcommand acp
capability_local_case acp_long_flag --acp
capability_local_case acp_short_flag -acp

# ---------- capability 13: the wrapper reads only the fenced status report --
#
# The capability claim must come from the status helper — the one call that
# fences the version banner between two topology probes and a file-identity
# comparison. A second, unfenced `"${CANONICAL_CLAW}" --version` in the wrapper
# would be a weaker read of the same question, and would show up as the
# canonical binary being invoked before the exec.
cap13_name="capability_is_not_read_by_a_second_unfenced_version_call"
cap13_dir="$(stage_layout "${cap13_name}")"
install_canonical "${cap13_dir}" current "__NONE__"
if run_case "${cap13_name}" "${cap13_dir}" 10 -- --model fast prompt "should refuse" >/dev/null; then
  if assert_capability_refusal "${cap13_name}" "${cap13_dir}"; then
    pass_case "${cap13_name}"
  fi
fi

# ---------- capability 14: the source carries no weaker duplicate probe -----
#
# Belt and braces for the above: the wrapper must not contain its own
# `--version` invocation of the canonical binary at all.
cap14_name="wrapper_source_has_no_direct_canonical_version_probe"
# Comment lines are excluded deliberately: the header documents WHY this probe
# is absent, and naming the thing you must not do is not doing it. Any
# executable occurrence still fails here.
# shellcheck disable=SC2016  # deliberate literal: this is the exact source
# text being searched for, so it must NOT expand here.
cap14_probes="$( { grep -v '^[[:space:]]*#' "${REAL_WRAPPER}" \
  | grep -c '\${CANONICAL_CLAW}" --version'; } || true)"
if [[ "${cap14_probes}" == "0" ]]; then
  pass_case "${cap14_name}"
else
  fail_case "${cap14_name}" \
    'the wrapper probes the canonical binary directly, bypassing the identity-protected status call' \
    /dev/null /dev/null "${REAL_WRAPPER}"
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
