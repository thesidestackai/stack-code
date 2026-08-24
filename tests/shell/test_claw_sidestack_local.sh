#!/usr/bin/env bash
# Offline test suite for scripts/claw-sidestack-local.
#
# No real `claw` invocation. No real $HOME. No network. No cargo. Each case
# stages an isolated REPO_ROOT (a throwaway git repo) under a temp dir, copies
# the wrapper and the canonical status helper into it, substitutes a controlled
# `examples/sidestack-local.env`, and installs a fake canonical `claw` into a
# throwaway HOME. Decoy `claw` binaries are planted on PATH and in
# ~/.cargo/bin so that any silent PATH fallback fails the suite loudly.

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

# write_fake_claw <path> <identity> <reported_sha>
#
# A stand-in for a claw executable. It answers `--version` with a Git SHA
# banner in the real binary's format, and otherwise records that this specific
# instance was executed, so the suite can tell canonical from decoy.
write_fake_claw() {
  local path="$1" identity="$2" sha="$3"
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n  Version          0.1.0\n  Git SHA          %s\n' '${sha}'
  exit 0
fi
log="\${FAKE_CLAW_LOG:?FAKE_CLAW_LOG must be set}"
{
  printf 'FAKE_CLAW_CALLED=1\n'
  printf 'IDENTITY=%s\n' '${identity}'
  printf 'OPENAI_BASE_URL=%s\n' "\${OPENAI_BASE_URL:-}"
  printf 'RUSTY_CLAUDE_LLM_CALLER=%s\n' "\${RUSTY_CLAUDE_LLM_CALLER:-}"
  printf 'RUSTY_CLAUDE_TASK_TYPE=%s\n' "\${RUSTY_CLAUDE_TASK_TYPE:-}"
  printf 'ARGC=%d\n' "\$#"
  for arg in "\$@"; do
    printf 'ARG=%s\n' "\${arg}"
  done
} >> "\${log}"
FAKE
  chmod 0755 -- "${path}"
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
  mkdir -p "${root}/scripts" "${root}/examples" "${case_dir}/decoybin" "${case_dir}/home"

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

  printf '%s\n' "${case_dir}"
}

# fixture_head_short <case_dir>
fixture_head_short() { git -C "$1/root" rev-parse --short HEAD; }

# fixture_head_full <case_dir> — the canonical contract compares FULL 40-char
# SHAs, so a "current" fixture must report the whole thing.
fixture_head_full() { git -C "$1/root" rev-parse HEAD; }

# install_canonical <case_dir> <mode>
#   current | stale | stale_prefix | symlink | nonexec | missing
install_canonical() {
  local case_dir="$1" mode="$2"
  local canonical="${case_dir}/home/.local/bin/claw"
  mkdir -p "${case_dir}/home/.local/bin"
  case "${mode}" in
    current)
      write_fake_claw "${canonical}" "CANONICAL" "$(fixture_head_full "${case_dir}")"
      ;;
    stale)
      write_fake_claw "${canonical}" "CANONICAL_STALE" \
        "5ta1e005ta1e005ta1e005ta1e005ta1e005ta1e"
      ;;
    stale_prefix)
      # A legacy build whose banner is a 7-character PREFIX of the real HEAD.
      # Under the repaired exact-match contract this is unproven, not current.
      write_fake_claw "${canonical}" "CANONICAL_STALE_PREFIX" \
        "$(fixture_head_short "${case_dir}")"
      ;;
    symlink)
      write_fake_claw "${case_dir}/fake-cargo-target/release/claw" "SYMLINK_TARGET" \
        "$(fixture_head_full "${case_dir}")"
      ln -s "${case_dir}/fake-cargo-target/release/claw" "${canonical}"
      ;;
    nonexec)
      write_fake_claw "${canonical}" "CANONICAL" "$(fixture_head_full "${case_dir}")"
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

  set +e
  env -i \
    HOME="${case_dir}/home" \
    PATH="${case_dir}/decoybin:${case_dir}/home/.cargo/bin:/usr/bin:/bin" \
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
if run_case "${case1_name}" "${case1_dir}" 0 -- prompt "say hi" >/dev/null; then
  log="${case1_dir}/fake_claw.log"
  if assert_log_contains "${case1_name}" "${log}" 'FAKE_CLAW_CALLED=1' \
     && assert_log_contains "${case1_name}" "${log}" 'IDENTITY=CANONICAL' \
     && assert_no_decoy_executed "${case1_name}" "${log}" \
     && assert_log_contains "${case1_name}" "${log}" 'OPENAI_BASE_URL=http://127.0.0.1:11435/v1' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=prompt' \
     && assert_log_contains "${case1_name}" "${log}" 'ARG=say hi' \
     && assert_stderr_contains "${case1_name}" "${case1_dir}/wrapper.stderr" \
          "canonical claw: ${case1_dir}/home/.local/bin/claw"; then
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
  if assert_stderr_contains "${case3_name}" "${stderr}" 'LAW 1' \
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
  if assert_stderr_contains "${case4_name}" "${stderr}" 'LAW 1' \
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
  if assert_stderr_contains "${case5_name}" "${stderr}" 'env file not found' \
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
  if assert_stderr_contains "${case6_name}" "${stderr}" 'canonical claw is missing' \
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
  if assert_stderr_contains "${case9_name}" "${stderr}" 'refusing to run a STALE canonical claw' \
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
     "CLAW_SIDESTACK_ALLOW_STALE=1" -- prompt "explicitly allowed" >/dev/null; then
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
if run_case "${case11_name}" "${case11_dir}" 0 -- prompt "unknown base" >/dev/null; then
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
  if assert_stderr_contains "${case12_name}" "${stderr}" 'LAW 1' \
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
  if assert_stderr_contains "${case13_name}" "${stderr}" 'invalid topology' \
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
  if assert_stderr_contains "${case14_name}" "${stderr}" 'canonical status helper missing' \
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
  if assert_stderr_contains "${case15_name}" "${stderr}" 'refusing to run a STALE canonical claw' \
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
# Reports CURRENT, then races the canonical path into a symlink.
printf 'canonical topology:  regular-executable\n'
printf 'state:               CURRENT\n'
rm -f -- '${canonical16}'
ln -s '${case16_dir}/swapped-target/claw' '${canonical16}'
exit 0
STUB
chmod 0755 "${case16_dir}/root/scripts/claw-canonical-status"
if run_case "${case16_name}" "${case16_dir}" 5 -- prompt "should refuse" >/dev/null; then
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
printf 'state:               CURRENT\n'
rm -f -- '${canonical17}'
exit 0
STUB
chmod 0755 "${case17_dir}/root/scripts/claw-canonical-status"
if run_case "${case17_name}" "${case17_dir}" 4 -- prompt "should refuse" >/dev/null; then
  stderr="${case17_dir}/wrapper.stderr"
  log="${case17_dir}/fake_claw.log"
  if assert_stderr_contains "${case17_name}" "${stderr}" 'canonical claw is missing' \
     && assert_stderr_contains "${case17_name}" "${stderr}" 'does NOT fall back to PATH or to ~/.cargo/bin/claw' \
     && assert_fake_not_called "${case17_name}" "${log}" \
     && assert_no_decoy_executed "${case17_name}" "${log}"; then
    pass_case "${case17_name}"
  fi
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
