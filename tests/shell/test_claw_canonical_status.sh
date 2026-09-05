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

# write_fake_claw <path> <reported_sha> [capabilities]
#
# [capabilities] is the literal text of the banner's `Capabilities` line. The
# default is the token a build implementing the in-process N6 enforcement
# contract advertises. The literal "__NONE__" omits the line entirely, which is
# what a binary predating that contract looks like.
write_fake_claw() {
  local path="$1" sha="$2" capabilities="${3-sidestack-n6-enforce-v1}"
  local capability_line=""
  if [[ "${capabilities}" != "__NONE__" ]]; then
    capability_line="  printf '  Capabilities     %s\n' '${capabilities}'"
  fi
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
printf 'FAKE_CLAW_ARGV=%s\n' "\$*" >> "\${FAKE_CLAW_LOG:-/dev/null}"
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n'
  printf '  Version          0.1.0\n'
  printf '  Git SHA          %s\n' '${sha}'
  printf '  Target           x86_64-unknown-linux-gnu\n'
${capability_line}
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

# ---------- portable file identity (Linux / macOS / WSL) ----------
#
# install.sh supports Linux, macOS, and WSL. Identity probing is what catches a
# regular executable being swapped for another regular executable underneath
# the single `--version` call, where topology alone sees nothing change. A
# GNU-only `stat -c` makes that guard vacuous on a stock macOS: it fails on
# both sides of the call, "" compares equal to "", and the swap is waved
# through as CURRENT.
#
# These cases run the tracked script itself under stand-in userlands. They are
# not a macOS substitute; they are a regression guard for the GNU assumption.

# stage_identity_utils <dir> <flavour>
#
# bsd     — BSD/macOS stat: rejects GNU `-c`, serves `-f` with %z for size.
#           python3 is shimmed to fail so only the BSD branch can succeed.
# broken  — every supported identity mechanism fails: no usable stat, no
#           python3. Identity is genuinely unobtainable.
stage_identity_utils() {
  local dir="$1" flavour="$2"
  mkdir -p "${dir}"

  if [[ "${flavour}" == "bsd" ]]; then
    cat > "${dir}/stat" <<'BSD_STAT'
#!/usr/bin/env bash
# Stand-in BSD/macOS stat: no GNU -c, and size is %z rather than %s. `--` is
# not honoured as an end-of-options marker, matching the BSD documentation.
set -uo pipefail
if [[ "${1:-}" == "-c" ]]; then
  printf 'stat: illegal option -- c\n' >&2
  exit 1
fi
if [[ "${1:-}" == "-f" ]]; then
  fmt="${2:-}"
  shift 2
  if [[ "${1:-}" == "--" ]]; then
    printf 'stat: --: No such file or directory\n' >&2
    exit 1
  fi
  fmt="${fmt//%z/%s}"
  fmt="${fmt//%Lp/%a}"
  /usr/bin/stat -c "${fmt}" -- "$@"
  exit $?
fi
printf 'stat: unsupported invocation\n' >&2
exit 1
BSD_STAT
  else
    cat > "${dir}/stat" <<'NO_STAT'
#!/usr/bin/env bash
printf 'stat: not available\n' >&2
exit 127
NO_STAT
  fi
  chmod 0755 "${dir}/stat"

  # No python3 fallback in either flavour: the bsd case must prove the BSD
  # branch itself works, and the broken case must leave nothing that works.
  cat > "${dir}/python3" <<'NO_PY'
#!/usr/bin/env bash
printf 'python3: not available\n' >&2
exit 127
NO_PY
  chmod 0755 "${dir}/python3"
}

# write_swapping_claw <canonical> <replacement> <sha>
#
# Prints a perfectly good full-SHA banner, then atomically replaces itself at
# the canonical pathname with a different *regular executable*. Topology is
# identical before and after; only device/inode/size differ.
write_swapping_claw() {
  local canonical="$1" replacement="$2" sha="$3"
  mkdir -p -- "$(dirname -- "${canonical}")" "$(dirname -- "${replacement}")"
  {
    printf '#!/usr/bin/env bash\n'
    printf '# replacement executable B\n'
    printf '# %s\n' "$(printf 'B%.0s' $(seq 1 200))"
    printf 'exit 0\n'
  } > "${replacement}"
  chmod 0755 -- "${replacement}"

  cat > "${canonical}" <<SWAP
#!/usr/bin/env bash
printf 'FAKE_CLAW_ARGV=%s\n' "\$*" >> "\${FAKE_CLAW_LOG:-/dev/null}"
printf 'Claw Code\n'
printf '  Version          0.1.0\n'
printf '  Git SHA          %s\n' '${sha}'
printf '  Target           x86_64-unknown-linux-gnu\n'
cp -p -- '${replacement}' '${canonical}.staged'
mv -f -- '${canonical}.staged' '${canonical}'
exit 0
SWAP
  chmod 0755 -- "${canonical}"
}

# ---------- case 13: BSD/macOS stat, nothing swapped, still CURRENT ----------
#
# Ordinary macOS-style execution must remain usable: a correct install is not
# allowed to become permanently STALE just because the host is not GNU.
name="bsd_stat_userland_unchanged_binary_is_current"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_identity_utils "${case_dir}/bsdbin" bsd
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/bsdbin:/usr/bin:/bin")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) on a BSD userland, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "an unchanged install was not CURRENT on a BSD userland" "${out}" "${errf}"
elif grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "BSD identity probing did not work; it fell back to unavailable" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 14: BSD/macOS stat, file swapped under --version ----------
#
# The regression this lane exists for. Both files are regular executables, so
# only the identity comparison can catch the swap — and on a GNU-only
# implementation that comparison is "" vs "", which passes.
name="bsd_stat_userland_swapped_binary_is_not_current"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
canonical="${home}/.local/bin/claw"
write_swapping_claw "${canonical}" "${case_dir}/elsewhere/claw_b" "${head_sha}"
stage_identity_utils "${case_dir}/bsdbin" bsd
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/bsdbin:/usr/bin:/bin")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ ! -f "${canonical}" || -L "${canonical}" || ! -x "${canonical}" ]]; then
  fail_case "${name}" "fixture invalid: the replacement is not a regular executable" "${out}" "${errf}"
elif grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "a binary swapped under --version was reported CURRENT on a BSD userland" "${out}" "${errf}"
elif [[ "${rc}" == "0" ]]; then
  fail_case "${name}" "expected a non-zero exit for a swapped binary, got 0" "${out}" "${errf}"
elif [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity changed while the version banner was being read" "${out}"; then
  fail_case "${name}" "report did not explain the identity change" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 15: identity probing unavailable fails closed ----------
#
# Two unreadable identities must never compare equal to each other. A host
# that cannot prove the file is unchanged must not certify it as CURRENT.
name="unavailable_identity_probing_fails_closed"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_identity_utils "${case_dir}/nobin" broken
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/nobin:/usr/bin:/bin")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "unprovable identity was reported CURRENT" "${out}" "${errf}"
elif [[ "${rc}" == "0" ]]; then
  fail_case "${name}" "expected a non-zero exit when identity is unprovable, got 0" "${out}" "${errf}"
elif [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "report did not explain that identity was unobtainable" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- malformed-but-successful identity probes ----------
#
# Exit status is not proof. A `stat` on PATH that exits 0 while printing a
# banner satisfies a status-only probe, and the backend it enshrines may then
# print something perfectly well-formed for the canonical file — so the swap
# guard ends up resting on a mechanism that never proved itself. Only the shape
# of the probe output can tell these backends apart from working ones.

# stage_malformed_identity_utils <dir> <flavour>
#
# gnu-only        — GNU `stat -c` exits 0 with a banner for the "/" probe but
#                   returns a well-formed identity for every other path.
#                   Nothing else is available.
# gnu-then-bsd    — GNU `stat -c` is malformed for every path; BSD `stat -f`
#                   works. A correct detector rejects gnu and selects bsd.
# gnu-bsd-then-py — both stat mechanisms are malformed for every path; python3
#                   is left real, so only the python3 backend can succeed.
#
# Every invocation is appended to $IDENTITY_PROBE_LOG so a case can show which
# mechanism was actually used, not merely which verdict came out.
stage_malformed_identity_utils() {
  local dir="$1" flavour="$2"
  mkdir -p "${dir}"
  local gnu_probe_only="0" bsd_valid="0" shim_python="1"
  case "${flavour}" in
    gnu-only) gnu_probe_only="1" ;;
    gnu-then-bsd) bsd_valid="1" ;;
    gnu-bsd-then-py) shim_python="0" ;;
    *)
      printf 'test setup: unknown malformed identity flavour: %s\n' "${flavour}" >&2
      exit 2
      ;;
  esac

  cat > "${dir}/stat" <<STAT_SHIM
#!/usr/bin/env bash
# Stand-in stat that exits 0 while printing something that is not an identity.
set -uo pipefail
printf 'stat %s\n' "\$*" >> "\${IDENTITY_PROBE_LOG:-/dev/null}"
if [[ "\${1:-}" == "-c" ]]; then
  fmt="\${2:-}"
  shift 2
  [[ "\${1:-}" == "--" ]] && shift
  if [[ "${gnu_probe_only}" == "1" && "\${1:-}" != "/" ]]; then
    /usr/bin/stat -c "\${fmt}" -- "\${1:-}"
    exit \$?
  fi
  printf 'not-an-identity\n'
  exit 0
fi
if [[ "\${1:-}" == "-f" ]]; then
  fmt="\${2:-}"
  shift 2
  if [[ "\${1:-}" == "--" ]]; then
    printf 'stat: --: No such file or directory\n' >&2
    exit 1
  fi
  if [[ "${bsd_valid}" == "1" ]]; then
    fmt="\${fmt//%z/%s}"
    /usr/bin/stat -c "\${fmt}" -- "\$@"
    exit \$?
  fi
  printf 'not-an-identity\n'
  exit 0
fi
printf 'stat: unsupported invocation\n' >&2
exit 1
STAT_SHIM
  chmod 0755 "${dir}/stat"

  if [[ "${shim_python}" == "1" ]]; then
    cat > "${dir}/python3" <<'NO_PY'
#!/usr/bin/env bash
printf 'python3: not available\n' >&2
exit 127
NO_PY
    chmod 0755 "${dir}/python3"
  fi
}

# ---------- case 16: a malformed successful probe is not a backend ----------
#
# The exact defect: the probe exits 0 with "not-an-identity", the same backend
# would answer the canonical path correctly, and no other mechanism exists.
# Trusting exit status alone yields CURRENT here. Nothing proved itself, so the
# only honest verdict is STALE.
name="malformed_successful_identity_probe_is_not_trusted"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_malformed_identity_utils "${case_dir}/badbin" gnu-only
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/badbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "a backend whose probe returned a non-identity was trusted, and its install reported CURRENT" "${out}" "${errf}"
elif [[ "${rc}" == "0" ]]; then
  fail_case "${name}" "expected a non-zero exit when no backend proved itself, got 0" "${out}" "${errf}"
elif [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "report did not explain that no identity mechanism was available" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 17: a malformed gnu probe falls through to bsd ----------
#
# Rejecting a malformed backend must not become a premature global failure. A
# host whose `stat -c` is unusable but whose `stat -f` works is an ordinary
# macOS-shaped host, and a correct install there is still CURRENT.
name="malformed_gnu_probe_falls_through_to_valid_bsd"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
canonical="${home}/.local/bin/claw"
write_fake_claw "${canonical}" "${head_sha}"
stage_malformed_identity_utils "${case_dir}/badbin" gnu-then-bsd
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/badbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
probe_log="${case_dir}/probe.log"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) once bsd was selected, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "an unchanged install was not CURRENT after falling through to bsd" "${out}" "${errf}"
elif grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "a malformed gnu probe was turned into a global identity failure" "${out}" "${errf}"
elif ! grep -Fq -- "-f %d:%i:%z ${canonical}" "${probe_log}"; then
  fail_case "${name}" "the canonical identity was not read through the bsd mechanism" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 18: malformed gnu and bsd fall through to python3 ----------
#
# Fallthrough is a chain, not a single step: two malformed stat mechanisms must
# still leave the python3 backend reachable.
name="malformed_stat_probes_fall_through_to_python3"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_malformed_identity_utils "${case_dir}/badbin" gnu-bsd-then-py
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/badbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
probe_log="${case_dir}/probe.log"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) once python3 was selected, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "an unchanged install was not CURRENT after falling through to python3" "${out}" "${errf}"
elif grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "malformed stat probes were turned into a global identity failure" "${out}" "${errf}"
elif ! grep -Fq -- "-c %d:%i:%s -- /" "${probe_log}"; then
  fail_case "${name}" "the gnu mechanism was never probed" "${out}" "${errf}"
elif ! grep -Fq -- "-f %d:%i:%z /" "${probe_log}"; then
  fail_case "${name}" "the bsd mechanism was never probed after gnu was rejected" "${out}" "${errf}"
elif grep -Fq -- "${home}" "${probe_log}"; then
  fail_case "${name}" "a rejected stat mechanism was still used to read the canonical identity" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- nonzero-status identity probes ----------
#
# The other half of "exit status is not proof": output shape is not proof
# either. A utility that exits nonzero can still print a perfectly well-formed
# device:inode:size line — a stub on PATH, a wrapper that emits a cached line
# before failing, a partially written buffer. Accepting it on shape alone
# enshrines a mechanism whose own report of itself is failure, and the swap
# guard then rests on a backend that never worked.
#
# These cases hold both probe-time selection and the per-file read to the same
# two-part contract: the command must exit 0 AND print an identity.

# stage_status_identity_utils <dir> <flavour>
#
# gnu-nonzero-only    — GNU `stat -c` prints a well-formed identity for the "/"
#                       probe but exits 9, and answers every other path
#                       correctly with exit 0. Nothing else is available. A
#                       status-blind detector trusts gnu here and certifies the
#                       install; nothing proved itself, so the honest verdict
#                       is STALE.
# gnu-nonzero-then-bsd — GNU `stat -c` prints a well-formed identity but exits
#                       9 for every path; BSD `stat -f` works. A correct
#                       detector rejects gnu and selects bsd.
# runtime-nonzero     — every backend probes cleanly, but the read of any path
#                       other than "/" prints a well-formed identity while
#                       exiting 7. Probe-time acceptance is not enough: the
#                       per-file read has to check its own status too.
#
# Every invocation is appended to $IDENTITY_PROBE_LOG.
stage_status_identity_utils() {
  local dir="$1" flavour="$2"
  mkdir -p "${dir}"
  local gnu_probe_rc="0" gnu_all_paths_rc="0" bsd_valid="0" runtime_rc="0"
  case "${flavour}" in
    gnu-nonzero-only) gnu_probe_rc="9" ;;
    gnu-nonzero-then-bsd) gnu_probe_rc="9"; gnu_all_paths_rc="9"; bsd_valid="1" ;;
    runtime-nonzero) runtime_rc="7" ;;
    *)
      printf 'test setup: unknown status identity flavour: %s\n' "${flavour}" >&2
      exit 2
      ;;
  esac

  cat > "${dir}/stat" <<STAT_SHIM
#!/usr/bin/env bash
# Stand-in stat whose exit status and output shape are controlled separately.
set -uo pipefail
printf 'stat %s\n' "\$*" >> "\${IDENTITY_PROBE_LOG:-/dev/null}"
if [[ "\${1:-}" == "-c" ]]; then
  fmt="\${2:-}"
  shift 2
  [[ "\${1:-}" == "--" ]] && shift
  path="\${1:-}"
  if [[ "\${path}" == "/" ]]; then
    printf '1:2:3\n'
    exit ${gnu_probe_rc}
  fi
  if [[ "${gnu_all_paths_rc}" != "0" ]]; then
    printf '1:2:3\n'
    exit ${gnu_all_paths_rc}
  fi
  if [[ "${runtime_rc}" != "0" ]]; then
    printf '4:5:6\n'
    exit ${runtime_rc}
  fi
  /usr/bin/stat -c "\${fmt}" -- "\${path}"
  exit \$?
fi
if [[ "\${1:-}" == "-f" ]]; then
  fmt="\${2:-}"
  shift 2
  if [[ "\${1:-}" == "--" ]]; then
    printf 'stat: --: No such file or directory\n' >&2
    exit 1
  fi
  path="\${1:-}"
  if [[ "${bsd_valid}" != "1" ]]; then
    printf 'stat: illegal option -- f\n' >&2
    exit 1
  fi
  fmt="\${fmt//%z/%s}"
  /usr/bin/stat -c "\${fmt}" -- "\${path}"
  exit \$?
fi
printf 'stat: unsupported invocation\n' >&2
exit 1
STAT_SHIM
  chmod 0755 "${dir}/stat"

  cat > "${dir}/python3" <<PY_SHIM
#!/usr/bin/env bash
# python3 fallback. Probes cleanly only where the flavour needs a working
# backend after the stat mechanisms; otherwise it is unavailable, so a case
# cannot pass through a mechanism it did not mean to exercise.
set -uo pipefail
path="\${@: -1}"
printf 'python3 %s\n' "\${path}" >> "\${IDENTITY_PROBE_LOG:-/dev/null}"
if [[ "${runtime_rc}" != "0" ]]; then
  if [[ "\${path}" == "/" ]]; then
    /usr/bin/stat -c '%d:%i:%s' -- "\${path}"
    exit \$?
  fi
  printf '4:5:6\n'
  exit ${runtime_rc}
fi
printf 'python3: not available\n' >&2
exit 127
PY_SHIM
  chmod 0755 "${dir}/python3"
}

# ---------- case 19: a nonzero probe is not a backend ----------
#
# The exact defect this lane exists for: the "/" probe prints "1:2:3" and exits
# 9, the same backend answers the canonical path correctly, and no other
# mechanism exists. Trusting output shape alone yields CURRENT here.
name="nonzero_valid_looking_identity_probe_is_not_trusted"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_status_identity_utils "${case_dir}/rcbin" gnu-nonzero-only
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/rcbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "a backend whose probe exited nonzero was trusted on the shape of its output, and its install reported CURRENT" "${out}" "${errf}"
elif [[ "${rc}" == "0" ]]; then
  fail_case "${name}" "expected a non-zero exit when no backend proved itself, got 0" "${out}" "${errf}"
elif [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "report did not explain that no identity mechanism was available" "${out}" "${errf}"
elif ! grep -Fq -- "-c %d:%i:%s -- /" "${case_dir}/probe.log"; then
  fail_case "${name}" "the gnu mechanism was never probed" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 20: a nonzero gnu probe falls through to bsd ----------
#
# Rejecting a failing backend must not become a premature global failure: the
# next mechanism in the chain still has to be tried and used.
name="nonzero_gnu_probe_falls_through_to_valid_bsd"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
canonical="${home}/.local/bin/claw"
write_fake_claw "${canonical}" "${head_sha}"
stage_status_identity_utils "${case_dir}/rcbin" gnu-nonzero-then-bsd
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/rcbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
probe_log="${case_dir}/probe.log"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) once bsd was selected, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "an unchanged install was not CURRENT after falling through to bsd" "${out}" "${errf}"
elif grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "a nonzero gnu probe was turned into a global identity failure" "${out}" "${errf}"
elif ! grep -Fq -- "-f %d:%i:%z ${canonical}" "${probe_log}"; then
  fail_case "${name}" "the canonical identity was not read through the bsd mechanism" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- case 21: a nonzero per-file identity read is not trusted ----------
#
# Probe-time acceptance does not license the reads that follow. A backend that
# probed cleanly and then fails on the canonical path — while still printing
# something well-formed — has proved nothing about that file, so the binary
# cannot be certified unchanged across the version banner.
name="nonzero_status_identity_read_is_not_trusted_at_runtime"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}"
stage_status_identity_utils "${case_dir}/rcbin" runtime-nonzero
before="$(snapshot "${home}")"
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/rcbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "an identity read that exited nonzero was accepted on the shape of its output, and the install reported CURRENT" "${out}" "${errf}"
elif [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "identity could not be established" "${out}"; then
  fail_case "${name}" "report did not explain that the canonical identity was unprovable" "${out}" "${errf}"
elif [[ "$(snapshot "${home}")" != "${before}" ]]; then
  fail_case "${name}" "status mutated the fixture HOME" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: a CURRENT build that advertises the contract ----------
#
# The capability field is reported ALONGSIDE the freshness verdict, not instead
# of it, and neither answer disturbs the other.
name="current_build_advertising_the_capability_reports_supported"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "sidestack-n6-enforce-v1"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT), got ${rc}" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       supported" "${out}"; then
  fail_case "${name}" "report did not declare the capability supported" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "the capability field disturbed the freshness verdict" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: CURRENT is NOT capability ----------
#
# THE case this field exists for. The install matches origin/main exactly, so
# freshness is perfect — and the binary still cannot enforce anything. If these
# two answers were ever collapsed, this row would silently read as safe.
name="current_build_without_the_capability_reports_unsupported"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "__NONE__"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT) — capability must not change the exit code, got ${rc}" "${out}" "${errf}"
elif ! grep -Fq "state:               CURRENT" "${out}"; then
  fail_case "${name}" "a missing capability changed the freshness verdict" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       unsupported" "${out}"; then
  fail_case "${name}" "a binary with no Capabilities line was not reported unsupported" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: a STALE build can still advertise it ----------
#
# Capability is orthogonal in BOTH directions: a stale install is not thereby
# incapable, which is why the wrapper's stale override has to consult this field
# rather than infer it.
name="stale_build_advertising_the_capability_reports_supported"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
set_origin_main "${root}" "$(git -C "${root}" rev-parse HEAD)"
write_fake_claw "${home}/.local/bin/claw" \
  "5ta1e005ta1e005ta1e005ta1e005ta1e005ta1e" "sidestack-n6-enforce-v1"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       supported" "${out}"; then
  fail_case "${name}" "a stale but capable binary was not reported supported" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: whole-token matching ----------
#
# A prefix/substring matcher would accept every one of these. `-v1` is a version,
# not a stem, and a token that merely CONTAINS the contract name is not it.
for capability_variant in \
  "sidestack-n6-enforce-v10" \
  "sidestack-n6-enforce-v2" \
  "not-sidestack-n6-enforce-v1" \
  "sidestack-n6-enforce" \
  "sidestack-n6-enforce-v1x" ; do
  name="near_miss_capability_token_is_not_accepted__${capability_variant}"
  case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
  root="$(make_repo "${case_dir}")"
  home="${case_dir}/home"
  head_sha="$(git -C "${root}" rev-parse HEAD)"
  set_origin_main "${root}" "${head_sha}"
  write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "${capability_variant}"
  rc="$(run_status "${case_dir}" "${root}" "${home}")"
  out="${case_dir}/stdout"; errf="${case_dir}/stderr"
  if ! grep -Fqx "n6 capability:       unsupported" "${out}"; then
    fail_case "${name}" "near-miss token '${capability_variant}' was accepted as the capability" "${out}" "${errf}"
  else
    pass_case "${name}"
  fi
done

# ---------- capability: the token may sit among others ----------
#
# The line is a SET of tokens, so a future build advertising more than one
# capability must still be recognised.
name="capability_is_found_among_other_tokens_on_the_line"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" \
  "some-other-cap-v3 sidestack-n6-enforce-v1 trailing-cap-v9"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if ! grep -Fqx "n6 capability:       supported" "${out}"; then
  fail_case "${name}" "the capability was not found among other tokens" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: fails closed when the binary is missing ----------
name="missing_canonical_reports_unsupported_capability"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
mkdir -p "${home}/.local/bin"
set_origin_main "${root}" "$(git -C "${root}" rev-parse HEAD)"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "11" ]]; then
  fail_case "${name}" "expected exit 11 (MISSING), got ${rc}" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       unsupported" "${out}"; then
  fail_case "${name}" "a missing binary did not report an unsupported capability" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: fails closed on invalid topology ----------
#
# A symlinked canonical path is never executed, so nothing can have advertised
# anything. The field must not be silently absent or optimistic.
name="symlinked_canonical_reports_unsupported_capability"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${case_dir}/target/release/claw" "${head_sha}" "sidestack-n6-enforce-v1"
mkdir -p "${home}/.local/bin"
ln -s "${case_dir}/target/release/claw" "${home}/.local/bin/claw"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "12" ]]; then
  fail_case "${name}" "expected exit 12 (INVALID_TOPOLOGY), got ${rc}" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       unsupported" "${out}"; then
  fail_case "${name}" "a symlinked canonical path did not report an unsupported capability" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: fails closed when identity cannot be proven ----------
#
# Same fence as the Git SHA. A banner that cannot be attributed to the file at
# the canonical path proves neither provenance NOR capability, so a build that
# really does advertise the token must still be reported unsupported here.
name="unprovable_identity_discards_an_advertised_capability"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "sidestack-n6-enforce-v1"
stage_status_identity_utils "${case_dir}/rcbin" runtime-nonzero
rc="$(run_status "${case_dir}" "${root}" "${home}" \
  "PATH=${case_dir}/rcbin:/usr/bin:/bin" \
  "IDENTITY_PROBE_LOG=${case_dir}/probe.log")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (STALE), got ${rc}" "${out}" "${errf}"
elif ! grep -Fqx "n6 capability:       unsupported" "${out}"; then
  fail_case "${name}" "an unprovable identity did not discard the advertised capability" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: exactly one field, always ----------
#
# The wrapper refuses an ambiguous report, so the helper must never emit two.
name="capability_field_is_reported_exactly_once"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "sidestack-n6-enforce-v1"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
field_count="$(grep -c '^n6 capability:' "${out}" || true)"
if [[ "${field_count}" != "1" ]]; then
  fail_case "${name}" "expected exactly one capability field, found ${field_count}" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: --quiet still says nothing ----------
name="quiet_mode_suppresses_the_capability_field_too"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "sidestack-n6-enforce-v1"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
set +e
env -i HOME="${home}" PATH="/usr/bin:/bin" \
  bash "${root}/scripts/claw-canonical-status" --quiet >"${out}" 2>"${errf}"
rc=$?
set -e
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 (CURRENT), got ${rc}" "${out}" "${errf}"
elif [[ -s "${out}" ]]; then
  fail_case "${name}" "--quiet printed a report" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- capability: the status helper still runs --version exactly once ----------
#
# Capability must be read out of the SAME fenced banner as the Git SHA. A second
# invocation would be an unfenced read, and would show up here.
name="capability_does_not_add_a_second_version_invocation"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(make_repo "${case_dir}")"
home="${case_dir}/home"
head_sha="$(git -C "${root}" rev-parse HEAD)"
set_origin_main "${root}" "${head_sha}"
write_fake_claw "${home}/.local/bin/claw" "${head_sha}" "sidestack-n6-enforce-v1"
rc="$(run_status "${case_dir}" "${root}" "${home}")"
out="${case_dir}/stdout"; errf="${case_dir}/stderr"
invocations="$(grep -c '^FAKE_CLAW_ARGV=' "${case_dir}/fake_claw.log" || true)"
if [[ "${invocations}" != "1" ]]; then
  fail_case "${name}" "expected exactly one canonical invocation, found ${invocations}" "${out}" "${errf}"
elif ! grep -Fqx 'FAKE_CLAW_ARGV=--version' "${case_dir}/fake_claw.log"; then
  fail_case "${name}" "the single invocation was not --version" "${out}" "${errf}"
else
  pass_case "${name}"
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
