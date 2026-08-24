#!/usr/bin/env bash
# Offline test suite for scripts/claw-canonical-status.
#
# No real `claw`. No real $HOME. No network. No cargo. Every case builds an
# isolated fixture: a throwaway git repo that holds a copy of the status
# script, a throwaway HOME, and a fake `claw` shell script standing in for the
# canonical executable. The real ~/.local/bin/claw is never read or written.
#
# Locks the published exit codes:
#   0 CURRENT | 10 STALE | 11 MISSING | 12 INVALID_TOPOLOGY | 13 UNKNOWN_BASE

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
REAL_STATUS="${REPO_ROOT}/scripts/claw-canonical-status"

if [[ ! -x "${REAL_STATUS}" ]]; then
  printf 'test setup: status script missing or not executable at %s\n' "${REAL_STATUS}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t claw-canonical-status.XXXXXX)"
cleanup() { rm -rf "${WORK_DIR}"; }
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

pass_case() { PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS: %s\n' "$1"; }

fail_case() {
  local name="$1" reason="$2" out="$3" errf="$4"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: %s\n' "${reason}" >&2
  printf '  --- stdout ---\n' >&2
  sed 's/^/    /' "${out}" >&2 || true
  printf '  --- stderr ---\n' >&2
  sed 's/^/    /' "${errf}" >&2 || true
}

# make_repo <case_dir> — isolated git repo holding a copy of the status script.
make_repo() {
  local case_dir="$1"
  local root="${case_dir}/repo"
  mkdir -p "${root}/scripts"
  cp "${REAL_STATUS}" "${root}/scripts/claw-canonical-status"
  chmod 0755 "${root}/scripts/claw-canonical-status"
  git -C "${root}" init -q
  git -C "${root}" add -A
  git -C "${root}" -c user.name=fixture -c user.email=fixture@example.invalid \
    commit -q -m "fixture"
  printf '%s\n' "${root}"
}

# set_origin_main <root> <sha>
set_origin_main() { git -C "$1" update-ref refs/remotes/origin/main "$2"; }

# write_fake_claw <path> <reported_sha>
write_fake_claw() {
  local path="$1" sha="$2"
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
printf 'FAKE_CLAW_ARGV=%s\n' "\$*" >> "\${FAKE_CLAW_LOG:-/dev/null}"
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n'
  printf '  Version          0.1.0\n'
  printf '  Git SHA          %s\n' '${sha}'
  printf '  Target           x86_64-unknown-linux-gnu\n'
  exit 0
fi
exit 1
FAKE
  chmod 0755 -- "${path}"
}

snapshot() { find "$1" -printf '%y %m %s %p\n' 2>/dev/null | sort; }

# run_status <case_dir> <root> <home> [extra env...]
run_status() {
  local case_dir="$1" root="$2" home="$3"
  shift 3
  local out="${case_dir}/stdout" errf="${case_dir}/stderr"
  set +e
  env -i \
    HOME="${home}" \
    PATH="/usr/bin:/bin" \
    FAKE_CLAW_LOG="${case_dir}/fake_claw.log" \
    "$@" \
    bash "${root}/scripts/claw-canonical-status" \
    >"${out}" 2>"${errf}"
  local rc=$?
  set -e
  printf '%s' "${rc}"
}

# ---------- case 1: current ----------
name="current_regular_executable_matching_origin_main"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "report did not declare CURRENT" "${out}" "${errf}"
elif ! grep -Fq "installed git sha:   ${head_sha}" "${out}"; then
  fail_case "${name}" "report did not include the installed git sha" "${out}" "${errf}"
elif ! grep -Fq "origin/main sha:     ${head_sha}" "${out}"; then
  fail_case "${name}" "report did not include the origin/main sha" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
elif grep -qv 'FAKE_CLAW_ARGV=--version' "${case_dir}/fake_claw.log"; then
  fail_case "${name}" "status invoked the canonical binary with something other than --version" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 2: stale ----------
name="stale_regular_executable_not_matching_origin_main"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               STALE" "${out}"; then
  fail_case "${name}" "report did not declare STALE" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 3: missing ----------
name="missing_canonical"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
mkdir -p "${home}/.local/bin"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "11" ]]; then
  fail_case "${name}" "expected exit 11 (MISSING), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               MISSING" "${out}"; then
  fail_case "${name}" "report did not declare MISSING" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 4: symlink to a fake cargo artifact ----------
name="symlink_canonical_is_invalid_topology_and_target_is_never_executed"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
cargo_artifact="${case_dir}/fake-cargo-target/release/claw"
marker="${case_dir}/cargo_artifact_was_executed"
mkdir -p "$(dirname "${cargo_artifact}")"
cat > "${cargo_artifact}" <<MARKER
#!/usr/bin/env bash
printf 'executed\n' > '${marker}'
printf 'Claw Code\n  Git SHA          ${head_sha}\n'
MARKER
chmod 0755 "${cargo_artifact}"
mkdir -p "${home}/.local/bin"
ln -s "${cargo_artifact}" "${home}/.local/bin/claw"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "12" ]]; then
  fail_case "${name}" "expected exit 12 (INVALID_TOPOLOGY), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               INVALID_TOPOLOGY" "${out}"; then
  fail_case "${name}" "report did not declare INVALID_TOPOLOGY" "${out}" "${errf}"
elif ! grep -Fq "canonical topology:  symlink" "${out}"; then
  fail_case "${name}" "report did not identify the symlink topology" "${out}" "${errf}"
elif [[ -e "${marker}" ]]; then
  fail_case "${name}" "the symlinked Cargo artifact was executed as canonical" "${out}" "${errf}"
elif [[ ! -L "${home}/.local/bin/claw" ]]; then
  fail_case "${name}" "status altered the symlink" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 5: unknown base ----------
name="unknown_base_when_origin_main_is_unavailable"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"   # no refs/remotes/origin/main created
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "13" ]]; then
  fail_case "${name}" "expected exit 13 (UNKNOWN_BASE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               UNKNOWN_BASE" "${out}"; then
  fail_case "${name}" "report did not declare UNKNOWN_BASE" "${out}" "${errf}"
elif ! grep -Fq "origin/main sha:     <unavailable>" "${out}"; then
  fail_case "${name}" "report did not mark origin/main unavailable" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 6: regular file that is not executable ----------
name="non_executable_regular_file_is_invalid_topology"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
chmod 0644 "${home}/.local/bin/claw"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "12" ]]; then
  fail_case "${name}" "expected exit 12 (INVALID_TOPOLOGY), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "canonical topology:  regular-file-not-executable" "${out}"; then
  fail_case "${name}" "report did not identify the non-executable topology" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 7: CLAW_CANONICAL_PATH override is honored ----------
name="canonical_path_override_is_honored"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
override="${case_dir}/elsewhere/claw"
write_fake_claw "${override}" "${head_sha}"
rc="$(run_status "${case_dir}" "${root}" "${home}" "CLAW_CANONICAL_PATH=${override}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) via override, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "canonical path:      ${override}" "${out}"; then
  fail_case "${name}" "report did not use the override path" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 8: --quiet is silent, unknown arg is a usage error ----------
name="quiet_is_silent_and_unknown_arg_is_usage_error"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
set +e
env -i HOME="${home}" PATH="/usr/bin:/bin" \
  bash "${root}/scripts/claw-canonical-status" --quiet \
  >"${case_dir}/quiet.stdout" 2>"${case_dir}/quiet.stderr"
quiet_rc=$?
env -i HOME="${home}" PATH="/usr/bin:/bin" \
  bash "${root}/scripts/claw-canonical-status" --bogus \
  >"${case_dir}/usage.stdout" 2>"${case_dir}/usage.stderr"
usage_rc=$?
set -e
out="${case_dir}/quiet.stdout"; errf="${case_dir}/quiet.stderr"
if [[ "${quiet_rc}" != "10" ]]; then
  fail_case "${name}" "expected --quiet to still exit 10, got ${quiet_rc}" "${out}" "${errf}"
elif [[ -s "${out}" ]]; then
  fail_case "${name}" "--quiet still printed a report" "${out}" "${errf}"
elif [[ "${usage_rc}" != "2" ]]; then
  fail_case "${name}" "expected exit 2 for an unknown argument, got ${usage_rc}" \
    "${case_dir}/usage.stdout" "${case_dir}/usage.stderr"
else
  pass_case "${name}"
fi

# ---------- case 9: provenance length discrimination ----------
#
# CURRENT requires the installed binary to report the FULL 40-character
# lowercase origin/main SHA. A matching 7- or 12-character prefix is a legacy
# build whose provenance is unproven, not a current one — the pre-repair
# prefix comparison would have called it CURRENT.
name="only_the_exact_full_sha_is_current_prefixes_are_stale"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
sub_failures=""
sub_index=0
for reported in \
  "${head_sha:0:7}" \
  "${head_sha:0:12}" \
  "${head_sha:0:39}" \
  "${head_sha}f" \
  "$(printf '%s' "${head_sha}" | tr 'a-f' 'A-F')" \
  "not-a-sha"; do
  sub_index=$((sub_index + 1))
  sub_home="${case_dir}/home-${sub_index}"
  write_fake_claw "${sub_home}/.local/bin/claw" "${reported}"
  sub_rc="$(run_status "${case_dir}" "${root}" "${sub_home}")"
  if [[ "${sub_rc}" != "10" ]]; then
    sub_failures+="reported '${reported}' -> exit ${sub_rc}, expected 10 (STALE)"$'\n'
  elif ! grep -Fq "state:               STALE" "${case_dir}/stdout"; then
    sub_failures+="reported '${reported}' -> report did not declare STALE"$'\n'
  fi
done
# and the exact full SHA is CURRENT
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ -n "${sub_failures}" ]]; then
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: an unproven provenance string was not treated as STALE\n' >&2
  printf '%s' "${sub_failures}" | sed 's/^/    /' >&2
  FAIL_COUNT=$((FAIL_COUNT + 1))
elif [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "the exact full SHA was not CURRENT (exit ${rc})" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 10: a malformed banner is noted, never silently current ----------
name="malformed_provenance_is_reported_as_unproven"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha:0:7}"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "did not report a full 40-character Git SHA" "${out}"; then
  fail_case "${name}" "report did not explain why the provenance is unusable" "${out}" "${errf}"
elif ! grep -Fq "state:               STALE" "${out}"; then
  fail_case "${name}" "report did not declare STALE" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 11: topology swapped underneath the version call ----------
#
# The canonical binary replaces itself with a symlink while `--version` runs.
# The post-execution recheck must catch that and return INVALID_TOPOLOGY
# rather than trusting the banner it just printed.
name="topology_change_during_version_call_is_invalid_topology"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
canonical="${home}/.local/bin/claw"
elsewhere="${case_dir}/elsewhere/claw"
write_fake_claw "${elsewhere}" "${head_sha}"
mkdir -p "$(dirname "${canonical}")"
cat > "${canonical}" <<SWAP
#!/usr/bin/env bash
# Print a perfectly good full-SHA banner, then swap ourselves for a symlink.
printf 'Claw Code\n  Version          0.1.0\n  Git SHA          %s\n' '${head_sha}'
rm -f -- '${canonical}'
ln -s '${elsewhere}' '${canonical}'
exit 0
SWAP
chmod 0755 "${canonical}"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "12" ]]; then
  fail_case "${name}" "expected exit 12 (INVALID_TOPOLOGY), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               INVALID_TOPOLOGY" "${out}"; then
  fail_case "${name}" "report did not declare INVALID_TOPOLOGY" "${out}" "${errf}"
elif ! grep -Fq "topology changed while the version banner was being read" "${out}"; then
  fail_case "${name}" "report did not explain the topology change" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 12: file identity swapped underneath the version call ----------
#
# The replacement is itself a regular executable, so topology alone cannot
# catch it: the device/inode comparison across the call must.
name="identity_change_during_version_call_is_not_current"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
canonical="${home}/.local/bin/claw"
mkdir -p "$(dirname "${canonical}")"
cat > "${canonical}" <<SWAP
#!/usr/bin/env bash
printf 'Claw Code\n  Version          0.1.0\n  Git SHA          %s\n' '${head_sha}'
# replace ourselves with a different regular executable
printf '#!/usr/bin/env bash\nexit 0\n' > '${canonical}.new'
chmod 0755 '${canonical}.new'
mv -f '${canonical}.new' '${canonical}'
exit 0
SWAP
chmod 0755 "${canonical}"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity changed while the version banner was being read" "${out}"; then
  fail_case "${name}" "report did not explain the identity change" "${out}" "${errf}"
elif grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "a swapped binary was reported CURRENT" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
