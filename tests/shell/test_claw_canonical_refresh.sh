#!/usr/bin/env bash
# Offline test suite for scripts/claw-canonical-refresh.
#
# No real $HOME. No real cargo. No real `claw`. No network. Every case builds
# an isolated fixture: a throwaway git repo holding a copy of the refresh
# script and a stub rust/ workspace, a throwaway HOME acting as the canonical
# destination, and a fake `cargo` on PATH that records its argv/env and
# fabricates a "built" binary. The real ~/.local/bin/claw and the real
# ~/.cargo/bin/claw are never read, written, or executed.
#
# The last cases additionally shadow PATH with a stand-in BSD userland, so the
# tracked script -- not a copy of its logic -- is proved to run on a host that
# has no GNU `realpath -m`, no GNU `stat -c`, and no `sha256sum` at all.

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
REAL_REFRESH="${REPO_ROOT}/scripts/claw-canonical-refresh"

if [[ ! -x "${REAL_REFRESH}" ]]; then
  printf 'test setup: refresh script missing or not executable at %s\n' "${REAL_REFRESH}" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t claw-canonical-refresh.XXXXXX)"
cleanup() { rm -rf "${WORK_DIR}"; }
trap cleanup EXIT INT TERM

PASS_COUNT=0
FAIL_COUNT=0

pass_case() { PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS: %s\n' "$1"; }

fail_case() {
  local name="$1" reason="$2" case_dir="$3"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: %s\n' "${reason}" >&2
  printf '  --- stdout ---\n' >&2
  sed 's/^/    /' "${case_dir}/stdout" >&2 || true
  printf '  --- stderr ---\n' >&2
  sed 's/^/    /' "${case_dir}/stderr" >&2 || true
  printf '  --- cargo log ---\n' >&2
  sed 's/^/    /' "${case_dir}/cargo.log" >&2 || true
}

# stage <case_dir>
#
# Builds: <case_dir>/repo    isolated git repo (refresh script + stub rust/)
#         <case_dir>/home    isolated HOME
#         <case_dir>/fakebin fake cargo
# Leaves the repo committed with refs/remotes/origin/main == HEAD.
stage() {
  local case_dir="$1"
  local root="${case_dir}/repo"
  mkdir -p "${root}/scripts" "${root}/rust" "${case_dir}/home" "${case_dir}/fakebin"

  cp "${REAL_REFRESH}" "${root}/scripts/claw-canonical-refresh"
  chmod 0755 "${root}/scripts/claw-canonical-refresh"
  cat > "${root}/rust/Cargo.toml" <<'STUB'
# fixture stub — the fake cargo never parses this
[workspace]
members = []
STUB
  cat > "${root}/rust/Cargo.lock" <<'LOCK'
# fixture stub lockfile
version = 4
LOCK

  git -C "${root}" init -q
  git -C "${root}" add -A
  git -C "${root}" -c user.name=fixture -c user.email=fixture@example.invalid \
    commit -q -m "fixture"
  git -C "${root}" update-ref refs/remotes/origin/main "$(git -C "${root}" rev-parse HEAD)"

  # Fake cargo. `metadata` echoes the target directory it was told to use
  # (or FAKE_CARGO_METADATA_TARGET, to simulate a config redirect winning).
  # `build` records argv + the CARGO_TARGET_DIR it saw, records whether a
  # backup already existed at build time, and fabricates the artifact.
  cat > "${case_dir}/fakebin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
log="${FAKE_CARGO_LOG:?FAKE_CARGO_LOG must be set}"
sub="${1:-}"
if [[ "${sub}" == "metadata" ]]; then
  {
    printf 'METADATA_ARGV=%s\n' "$*"
    if compgen -G "${FAKE_BACKUP_DIR:-/nonexistent}/claw.*" >/dev/null 2>&1; then
      printf 'METADATA_SAW_BACKUP=yes\n'
    else
      printf 'METADATA_SAW_BACKUP=no\n'
    fi
  } >> "${log}"
  if [[ -n "${FAKE_CARGO_METADATA_MUTATE:-}" ]]; then
    printf 'mutated-by-cargo-metadata\n' >> "${FAKE_CARGO_METADATA_MUTATE}"
  fi
  printf '{"target_directory":"%s"}\n' \
    "${FAKE_CARGO_METADATA_TARGET:-${CARGO_TARGET_DIR:-}}"
  exit 0
fi
{
  printf 'CARGO_ARGV=%s\n' "$*"
  printf 'CARGO_TARGET_DIR_ENV=%s\n' "${CARGO_TARGET_DIR:-}"
  printf 'CARGO_CWD=%s\n' "$PWD"
  if compgen -G "${FAKE_BACKUP_DIR:-/nonexistent}/claw.*" >/dev/null 2>&1; then
    printf 'BUILD_SAW_BACKUP=yes\n'
  else
    printf 'BUILD_SAW_BACKUP=no\n'
  fi
} >> "${log}"
target_dir=""
prev=""
for arg in "$@"; do
  if [[ "${prev}" == "--target-dir" ]]; then target_dir="${arg}"; fi
  prev="${arg}"
done
target_dir="${target_dir:-${CARGO_TARGET_DIR:-}}"
mkdir -p "${target_dir}/release"
cat > "${target_dir}/release/claw" <<INNER
#!/usr/bin/env bash
if [[ -n "\${FAKE_CLAW_FAIL_PATH:-}" && "\$0" == "\${FAKE_CLAW_FAIL_PATH}" ]]; then
  exit 3
fi
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n'
  printf '  Version          0.1.0\n'
  printf '  Git SHA          %s\n' "${FAKE_BUILT_SHA}"
  exit 0
fi
exit 1
INNER
chmod 0755 "${target_dir}/release/claw"
CARGO
  chmod 0755 "${case_dir}/fakebin/cargo"

  printf '%s\n' "${root}"
}

# write_fake_claw <path> <reported_sha>
write_fake_claw() {
  local path="$1" sha="$2"
  mkdir -p -- "$(dirname -- "${path}")"
  cat > "${path}" <<FAKE
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'Claw Code\n  Version          0.1.0\n  Git SHA          %s\n' '${sha}'
  exit 0
fi
exit 1
FAKE
  chmod 0755 -- "${path}"
}

# run_refresh <case_dir> <root> [extra env...]
run_refresh() {
  local case_dir="$1" root="$2"
  shift 2
  local home="${case_dir}/home"
  : > "${case_dir}/cargo.log"
  set +e
  env -i \
    HOME="${home}" \
    PATH="${case_dir}/fakebin:/usr/bin:/bin" \
    FAKE_CARGO_LOG="${case_dir}/cargo.log" \
    FAKE_BACKUP_DIR="${home}/.local/bin/.claw-canonical-backups" \
    FAKE_BUILT_SHA="${FAKE_BUILT_SHA:-}" \
    "$@" \
    bash "${root}/scripts/claw-canonical-refresh" \
    >"${case_dir}/stdout" 2>"${case_dir}/stderr"
  local rc=$?
  set -e
  printf '%s' "${rc}"
}

hash_of() { sha256sum -- "$1" | awk '{print $1}'; }

assert_no_build() {
  local name="$1" case_dir="$2"
  if grep -q 'CARGO_ARGV=' "${case_dir}/cargo.log"; then
    fail_case "${name}" "cargo build ran when it should not have" "${case_dir}"
    return 1
  fi
}

assert_no_backup_dir() {
  local name="$1" case_dir="$2"
  if [[ -d "${case_dir}/home/.local/bin/.claw-canonical-backups" ]]; then
    fail_case "${name}" "a backup directory was created when it should not have been" "${case_dir}"
    return 1
  fi
}

# The backup must be a real regular file, never a symlink: a copied link
# would mean a raced canonical path got hashed as though it were the binary.
assert_backup_is_regular_file() {
  local name="$1" case_dir="$2"
  local backup
  backup="$(compgen -G "${case_dir}/home/.local/bin/.claw-canonical-backups/claw.*" | head -1 || true)"
  if [[ -z "${backup}" ]]; then
    fail_case "${name}" "no backup was created" "${case_dir}"
    return 1
  fi
  if [[ -L "${backup}" || ! -f "${backup}" ]]; then
    fail_case "${name}" "the backup is not a regular non-symlink file: ${backup}" "${case_dir}"
    return 1
  fi
}

assert_no_leftover_candidate() {
  local name="$1" case_dir="$2"
  if compgen -G "${case_dir}/home/.local/bin/.claw-canonical-candidate.*" >/dev/null; then
    fail_case "${name}" "a candidate file was left behind" "${case_dir}"
    return 1
  fi
}

# stage_bsd_utils <dir> <flavour>
#
# Writes a stand-in for a non-GNU userland into <dir>, to be prepended to the
# fixture PATH. This is what a stock macOS looks like to this script:
#
#   bsd      BSD realpath (rejects -m), BSD stat (rejects -c, supports
#            -f '%Lp'), no sha256sum at all, and macOS's `shasum -a 256`.
#   minimal  BSD realpath, and none of stat / sha256sum / shasum / openssl,
#            so only the last-resort implementation is left.
#
# Anything not shadowed here still resolves through /usr/bin:/bin, so the
# script runs against real git, awk, cp, mv, and python3.
stage_bsd_utils() {
  local dir="$1" flavour="$2"
  mkdir -p "${dir}"

  cat > "${dir}/realpath" <<'BSD_REALPATH'
#!/usr/bin/env bash
# BSD realpath: no -m, and the path must already exist.
for arg in "$@"; do
  case "${arg}" in
    -m) printf 'realpath: illegal option -- m\n' >&2; exit 1 ;;
  esac
done
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --) shift; break ;;
    -q) shift ;;
    *) break ;;
  esac
done
for target in "$@"; do
  if [[ ! -e "${target}" ]]; then
    printf 'realpath: %s: No such file or directory\n' "${target}" >&2
    exit 1
  fi
  if [[ -d "${target}" ]]; then
    ( cd -- "${target}" >/dev/null && pwd -P )
  else
    parent="$( cd -- "$(dirname -- "${target}")" >/dev/null && pwd -P )"
    if [[ "${parent}" == "/" ]]; then
      printf '/%s\n' "$(basename -- "${target}")"
    else
      printf '%s/%s\n' "${parent}" "$(basename -- "${target}")"
    fi
  fi
done
BSD_REALPATH

  cat > "${dir}/sha256sum" <<'NO_TOOL'
#!/usr/bin/env bash
printf '%s: command not found\n' "$(basename -- "$0")" >&2
exit 127
NO_TOOL
  chmod 0755 "${dir}/sha256sum"

  if [[ "${flavour}" == "bsd" ]]; then
    cat > "${dir}/stat" <<'BSD_STAT'
#!/usr/bin/env bash
# BSD stat: no GNU -c; permission bits come from -f '%Lp'.
fmt=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    -c*) printf 'stat: illegal option -- c\n' >&2; exit 1 ;;
    -f) fmt="$2"; shift 2 ;;
    --) shift; break ;;
    -*) shift ;;
    *) break ;;
  esac
done
[[ "${fmt}" == "%Lp" ]] || { printf 'stat: unsupported format %s\n' "${fmt}" >&2; exit 1; }
for target in "$@"; do
  python3 -c 'import os, sys; print(format(os.stat(sys.argv[1]).st_mode & 0o7777, "o"))' "${target}" || exit 1
done
BSD_STAT
    cat > "${dir}/shasum" <<'MACOS_SHASUM'
#!/usr/bin/env bash
# macOS ships `shasum -a 256` in place of sha256sum.
algo=256
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    -a) algo="$2"; shift 2 ;;
    --) shift; break ;;
    -*) shift ;;
    *) break ;;
  esac
done
[[ "${algo}" == "256" ]] || { printf 'shasum: unsupported algorithm\n' >&2; exit 1; }
for target in "$@"; do
  digest="$(python3 -c 'import hashlib, sys
print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "${target}")" || exit 1
  printf '%s  %s\n' "${digest}" "${target}"
done
MACOS_SHASUM
  else
    cp -- "${dir}/sha256sum" "${dir}/stat"
    cp -- "${dir}/sha256sum" "${dir}/shasum"
    cp -- "${dir}/sha256sum" "${dir}/openssl"
  fi

  chmod 0755 "${dir}"/*
}

# assert_activated <name> <case_dir> <expected_sha256>
#
# The refresh reported success, so the canonical file must BE the built
# artifact and the reported digest must be a real SHA-256 of it.
assert_activated() {
  local name="$1" case_dir="$2" want="$3"
  local dest="${case_dir}/home/.local/bin/claw"
  if [[ ! "${want}" =~ ^[0-9a-f]{64}$ ]]; then
    fail_case "${name}" "no built artifact to compare against: cargo never produced one" \
      "${case_dir}"
    return 1
  fi
  if [[ -L "${dest}" || ! -f "${dest}" ]]; then
    fail_case "${name}" "canonical path is not a regular file after refresh" "${case_dir}"
    return 1
  fi
  if [[ "$(hash_of "${dest}")" != "${want}" ]]; then
    fail_case "${name}" "activated bytes do not match the built artifact" "${case_dir}"
    return 1
  fi
  if ! grep -Fq "new sha256:    ${want}" "${case_dir}/stdout"; then
    fail_case "${name}" "the report did not carry the true SHA-256 of the activated file" \
      "${case_dir}"
    return 1
  fi
}

# ---------- case 1: happy refresh over an existing regular file ----------
name="refresh_backs_up_before_cargo_metadata_and_activates_atomically"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
backup_dir="${case_dir}/home/.local/bin/.claw-canonical-backups"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0, got ${rc}" "${case_dir}"
elif ! grep -Fq 'METADATA_SAW_BACKUP=yes' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the backup did not exist when cargo metadata ran" "${case_dir}"
elif ! grep -Fq 'BUILD_SAW_BACKUP=yes' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the backup did not exist at build time" "${case_dir}"
elif ! grep -E -q '^METADATA_ARGV=.*--locked' "${case_dir}/cargo.log"; then
  fail_case "${name}" "cargo metadata was not invoked --locked" "${case_dir}"
elif ! grep -E -q '^METADATA_ARGV=.*--offline' "${case_dir}/cargo.log"; then
  fail_case "${name}" "cargo metadata was not invoked --offline" "${case_dir}"
elif ! grep -E -q '^METADATA_ARGV=.*--no-deps' "${case_dir}/cargo.log"; then
  fail_case "${name}" "cargo metadata was not invoked --no-deps" "${case_dir}"
elif ! grep -Fq -- '--locked' "${case_dir}/cargo.log" \
   || ! grep -Fq -- '--offline' "${case_dir}/cargo.log" \
   || ! grep -Fq -- '--release' "${case_dir}/cargo.log"; then
  fail_case "${name}" "cargo was not invoked --locked --offline --release" "${case_dir}"
elif ! grep -Fq "CARGO_TARGET_DIR_ENV=${root}/.canonical-refresh-target" "${case_dir}/cargo.log"; then
  fail_case "${name}" "CARGO_TARGET_DIR was not forced inside the worktree" "${case_dir}"
elif ! grep -Fq -- "--target-dir ${root}/.canonical-refresh-target" "${case_dir}/cargo.log"; then
  fail_case "${name}" "cargo build did not receive a worktree-local --target-dir" "${case_dir}"
elif [[ -L "${dest}" || ! -f "${dest}" || ! -x "${dest}" ]]; then
  fail_case "${name}" "destination is not a regular executable file" "${case_dir}"
elif [[ "$(stat -c '%a' "${dest}")" != "755" ]]; then
  fail_case "${name}" "destination mode is $(stat -c '%a' "${dest}"), expected 755" "${case_dir}"
elif [[ "$(hash_of "${dest}")" != "$(hash_of "${root}/.canonical-refresh-target/release/claw")" ]]; then
  fail_case "${name}" "destination bytes do not match the built artifact" "${case_dir}"
elif ! compgen -G "${backup_dir}/claw.*.0ddba11" >/dev/null; then
  fail_case "${name}" "no backup named for the previous git sha" "${case_dir}"
elif [[ "$(hash_of "$(compgen -G "${backup_dir}/claw.*.0ddba11" | head -1)")" != "${old_hash}" ]]; then
  fail_case "${name}" "backup bytes do not match the previous executable" "${case_dir}"
elif ! grep -Fq "old sha256:    ${old_hash}" "${case_dir}/stdout" \
   || ! grep -Fq "source HEAD:   ${head_sha}" "${case_dir}/stdout" \
   || ! grep -Fq "destination:   ${dest}" "${case_dir}/stdout" \
   || ! grep -Fq "new git sha:   ${head_sha}" "${case_dir}/stdout"; then
  fail_case "${name}" "the report is missing provenance fields" "${case_dir}"
elif ! assert_backup_is_regular_file "${name}" "${case_dir}"; then
  :
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then
  :
else
  pass_case "${name}"
fi

# ---------- case 2: source HEAD != origin/main ----------
name="stale_source_head_refuses_before_backup_or_build"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
# leave origin/main where it is and move HEAD ahead of it
git -C "${root}" -c user.name=fixture -c user.email=fixture@example.invalid \
  commit -q --allow-empty -m "ahead of origin/main"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "3" ]]; then
  fail_case "${name}" "expected exit 3 (source binding), got ${rc}" "${case_dir}"
elif ! grep -Fq 'does not equal the locally known origin/main' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal message did not name the origin/main mismatch" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif ! assert_no_backup_dir "${name}" "${case_dir}"; then :
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "destination was modified" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 3: missing origin/main ----------
name="absent_origin_main_refuses_with_actionable_message"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
git -C "${root}" update-ref -d refs/remotes/origin/main
write_fake_claw "${case_dir}/home/.local/bin/claw" "0ddba11"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "3" ]]; then
  fail_case "${name}" "expected exit 3, got ${rc}" "${case_dir}"
elif ! grep -Fq 'git fetch origin main' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not tell the operator how to recover" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 4: staged tracked change ----------
name="staged_tracked_change_refuses"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
printf 'staged\n' > "${root}/rust/Cargo.toml"
git -C "${root}" add rust/Cargo.toml
write_fake_claw "${case_dir}/home/.local/bin/claw" "0ddba11"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "4" ]]; then
  fail_case "${name}" "expected exit 4 (dirty source), got ${rc}" "${case_dir}"
elif ! grep -Fq 'staged changes' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the staged change" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif ! assert_no_backup_dir "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 5: unstaged tracked change ----------
name="unstaged_tracked_change_refuses"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
printf 'dirty\n' >> "${root}/rust/Cargo.toml"
write_fake_claw "${case_dir}/home/.local/bin/claw" "0ddba11"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "4" ]]; then
  fail_case "${name}" "expected exit 4 (dirty source), got ${rc}" "${case_dir}"
elif ! grep -Fq 'unstaged tracked changes' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the unstaged change" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 6: untracked file inside the build tree ----------
name="untracked_file_in_rust_tree_refuses"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
printf 'fn main() {}\n' > "${root}/rust/sneaky.rs"
write_fake_claw "${case_dir}/home/.local/bin/claw" "0ddba11"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "4" ]]; then
  fail_case "${name}" "expected exit 4 (dirty source), got ${rc}" "${case_dir}"
elif ! grep -Fq 'untracked files inside rust/' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the untracked build-tree file" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 7: repo-local cargo config cannot redirect the target ----------
name="repo_local_cargo_config_cannot_redirect_the_build_target"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
mkdir -p "${root}/rust/.cargo"
cat > "${root}/rust/.cargo/config.toml" <<'CFG'
[build]
target-dir = "/media/suki/18TB 2/build-artifacts/stack-code/rust/target"
CFG
git -C "${root}" add -A
git -C "${root}" -c user.name=fixture -c user.email=fixture@example.invalid \
  commit -q -m "fixture: local cargo config"
git -C "${root}" update-ref refs/remotes/origin/main "$(git -C "${root}" rev-parse HEAD)"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0, got ${rc}" "${case_dir}"
elif grep -Fq '/media/suki/18TB 2' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the forbidden external target directory reached cargo" "${case_dir}"
elif ! grep -Fq "CARGO_TARGET_DIR_ENV=${root}/.canonical-refresh-target" "${case_dir}/cargo.log"; then
  fail_case "${name}" "CARGO_TARGET_DIR did not override the repo-local config" "${case_dir}"
elif [[ ! -f "${root}/.canonical-refresh-target/release/claw" ]]; then
  fail_case "${name}" "the artifact did not land inside the worktree" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 8: resolved target escaping the worktree ----------
name="cargo_reported_target_escaping_the_worktree_refuses_after_a_safe_backup"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(run_refresh "${case_dir}" "${root}" \
  "FAKE_CARGO_METADATA_TARGET=/media/suki/18TB 2/build-artifacts/stack-code/rust/target")"
# The backup now precedes every Cargo command, so a metadata-stage refusal
# legitimately leaves a verified backup behind. What must NOT happen is a
# build, or any change to the canonical executable.
if [[ "${rc}" != "5" ]]; then
  fail_case "${name}" "expected exit 5 (build target refusal), got ${rc}" "${case_dir}"
elif ! grep -Fq 'escapes the approved source worktree' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the escaping target" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif ! grep -Fq 'METADATA_SAW_BACKUP=yes' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the backup did not precede the refused cargo metadata" "${case_dir}"
elif ! assert_backup_is_regular_file "${name}" "${case_dir}"; then :
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "destination was modified" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 9: symlinked canonical ----------
name="symlinked_canonical_refuses_without_building_or_following"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
cargo_artifact="${case_dir}/fake-cargo-target/release/claw"
write_fake_claw "${cargo_artifact}" "0ddba11"
artifact_hash="$(hash_of "${cargo_artifact}")"
mkdir -p "${case_dir}/home/.local/bin"
ln -s "${cargo_artifact}" "${case_dir}/home/.local/bin/claw"
rc="$(run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "6" ]]; then
  fail_case "${name}" "expected exit 6 (topology refusal), got ${rc}" "${case_dir}"
elif ! grep -Fq 'must be a regular file; do not point it at a Cargo target' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not explain the regular-file contract" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif ! assert_no_backup_dir "${name}" "${case_dir}"; then :
elif [[ ! -L "${case_dir}/home/.local/bin/claw" ]]; then
  fail_case "${name}" "the symlink was replaced instead of refused" "${case_dir}"
elif [[ "$(hash_of "${cargo_artifact}")" != "${artifact_hash}" ]]; then
  fail_case "${name}" "the symlink target was written through" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 10: first install ----------
name="first_install_without_existing_canonical"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0, got ${rc}" "${case_dir}"
elif [[ -L "${dest}" || ! -f "${dest}" || ! -x "${dest}" ]]; then
  fail_case "${name}" "first install did not produce a regular executable" "${case_dir}"
elif [[ "$(stat -c '%a' "${dest}")" != "755" ]]; then
  fail_case "${name}" "first install mode is $(stat -c '%a' "${dest}"), expected 755" "${case_dir}"
elif ! grep -Fq 'first install' "${case_dir}/stdout"; then
  fail_case "${name}" "the report did not identify a first install" "${case_dir}"
elif ! assert_no_backup_dir "${name}" "${case_dir}"; then :
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 11: bad provenance ----------
name="built_binary_reporting_the_wrong_sha_is_never_activated"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(FAKE_BUILT_SHA="0123456789abcdef0123456789abcdef01234567" run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "8" ]]; then
  fail_case "${name}" "expected exit 8 (provenance refusal), got ${rc}" "${case_dir}"
elif ! grep -Fq 'does not report the source Git SHA' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the provenance mismatch" "${case_dir}"
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "the canonical executable was replaced despite bad provenance" "${case_dir}"
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 12: post-activation failure rolls back ----------
name="post_activation_verification_failure_restores_previous_executable"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}" \
  "FAKE_CLAW_FAIL_PATH=${dest}")"
backup_dir="${case_dir}/home/.local/bin/.claw-canonical-backups"
if [[ "${rc}" != "10" ]]; then
  fail_case "${name}" "expected exit 10 (rolled back), got ${rc}" "${case_dir}"
elif ! grep -Fq 'post-activation verification failed' "${case_dir}/stderr"; then
  fail_case "${name}" "no post-activation failure was reported" "${case_dir}"
elif ! grep -Fq 'rolled back to the previous canonical executable' "${case_dir}/stderr"; then
  fail_case "${name}" "no rollback was reported" "${case_dir}"
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "the previous executable was not restored" "${case_dir}"
elif [[ -L "${dest}" || ! -x "${dest}" ]]; then
  fail_case "${name}" "the restored executable is not a regular executable file" "${case_dir}"
elif ! compgen -G "${backup_dir}/claw.*.0ddba11" >/dev/null; then
  fail_case "${name}" "the backup was not retained after rollback" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 13: ~/.cargo/bin is never a canonical target ----------
name="cargo_bin_destination_is_refused"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
rc="$(run_refresh "${case_dir}" "${root}" \
  "CLAW_CANONICAL_PATH=${case_dir}/home/.cargo/bin/claw")"
if [[ "${rc}" != "2" ]]; then
  fail_case "${name}" "expected exit 2 (usage refusal), got ${rc}" "${case_dir}"
elif ! grep -Fq 'refusing to install into ~/.cargo/bin' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name ~/.cargo/bin" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif [[ -e "${case_dir}/home/.cargo/bin/claw" ]]; then
  fail_case "${name}" "something was written into ~/.cargo/bin" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 14: the refresh is repeatable from the same source ----------
name="refresh_is_repeatable_and_never_clobbers_an_existing_backup"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
backup_dir="${case_dir}/home/.local/bin/.claw-canonical-backups"
rc1="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
first_backup_count="$(find "${backup_dir}" -type f | wc -l)"
first_hash="$(hash_of "${dest}")"
rc2="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
second_backup_count="$(find "${backup_dir}" -type f | wc -l)"
if [[ "${rc1}" != "0" ]]; then
  fail_case "${name}" "first refresh failed with ${rc1}" "${case_dir}"
elif [[ "${rc2}" != "0" ]]; then
  fail_case "${name}" "second refresh failed with ${rc2}" "${case_dir}"
elif [[ "$(hash_of "${dest}")" != "${first_hash}" ]]; then
  fail_case "${name}" "the repeated refresh changed the active bytes" "${case_dir}"
elif [[ "${second_backup_count}" -le "${first_backup_count}" ]]; then
  fail_case "${name}" "the second refresh clobbered the first backup instead of adding one" "${case_dir}"
elif [[ -L "${dest}" || ! -x "${dest}" ]]; then
  fail_case "${name}" "the destination is no longer a regular executable" "${case_dir}"
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- case 15: provenance length discrimination ----------
#
# The canonical installation contract requires the built candidate to report
# the source HEAD as a FULL 40-character lowercase Git SHA. A matching prefix
# is NOT provenance, and a near-length or uppercase value is not a Git SHA.
name="built_sha_must_be_exactly_the_full_forty_character_head"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
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
  sub_dir="${WORK_DIR}/${name}.sub${sub_index}"
  mkdir -p "${sub_dir}"
  sub_root="$(stage "${sub_dir}")"
  sub_head="$(git -C "${sub_root}" rev-parse HEAD)"
  sub_reported="${reported}"
  # rebuild the variant against THIS fixture's HEAD
  case "${sub_index}" in
    1) sub_reported="${sub_head:0:7}" ;;
    2) sub_reported="${sub_head:0:12}" ;;
    3) sub_reported="${sub_head:0:39}" ;;
    4) sub_reported="${sub_head}f" ;;
    5) sub_reported="$(printf '%s' "${sub_head}" | tr 'a-f' 'A-F')" ;;
  esac
  sub_dest="${sub_dir}/home/.local/bin/claw"
  write_fake_claw "${sub_dest}" "0ddba11"
  sub_old_hash="$(hash_of "${sub_dest}")"
  sub_rc="$(FAKE_BUILT_SHA="${sub_reported}" run_refresh "${sub_dir}" "${sub_root}")"
  if [[ "${sub_rc}" != "8" ]]; then
    sub_failures+="reported '${sub_reported}' -> exit ${sub_rc}, expected 8 (provenance refusal)"$'\n'
  elif [[ "$(hash_of "${sub_dest}")" != "${sub_old_hash}" ]]; then
    sub_failures+="reported '${sub_reported}' -> the canonical executable was replaced"$'\n'
  fi
done
# the exact full SHA is the only accepted form
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
if [[ -n "${sub_failures}" ]]; then
  printf '\n=== FAIL: %s ===\n' "${name}" >&2
  printf '  reason: unproven provenance was accepted\n' >&2
  printf '%s' "${sub_failures}" | sed 's/^/    /' >&2
  FAIL_COUNT=$((FAIL_COUNT + 1))
elif [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "the exact full 40-character SHA was rejected (exit ${rc})" "${case_dir}"
elif ! grep -Fq "new git sha:   ${head_sha}" "${case_dir}/stdout"; then
  fail_case "${name}" "the report did not carry the full 40-character SHA" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 16: legacy 7-character canonical is still refreshable ----------
#
# Transitional contract: the binary being REPLACED predates full-SHA
# provenance and reports only a legacy 7-character abbreviation. That must not
# block the refresh — only the newly built candidate has to satisfy the full
# 40-character rule — and the legacy binary must still be backed up safely.
name="legacy_seven_character_canonical_is_backed_up_and_replaced"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
legacy_sha="${head_sha:0:7}"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "${legacy_sha}"
legacy_hash="$(hash_of "${dest}")"
backup_dir="${case_dir}/home/.local/bin/.claw-canonical-backups"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}")"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 over a legacy 7-character binary, got ${rc}" "${case_dir}"
elif ! compgen -G "${backup_dir}/claw.*.${legacy_sha}" >/dev/null; then
  fail_case "${name}" "no backup named for the legacy 7-character sha" "${case_dir}"
elif [[ "$(hash_of "$(compgen -G "${backup_dir}/claw.*.${legacy_sha}" | head -1)")" != "${legacy_hash}" ]]; then
  fail_case "${name}" "the legacy backup bytes do not match the replaced executable" "${case_dir}"
elif ! assert_backup_is_regular_file "${name}" "${case_dir}"; then :
elif ! grep -Fq "old git sha:   ${legacy_sha}" "${case_dir}/stdout"; then
  fail_case "${name}" "the report did not name the legacy previous sha" "${case_dir}"
elif ! grep -Fq "new git sha:   ${head_sha}" "${case_dir}/stdout"; then
  fail_case "${name}" "the report did not name the full new sha" "${case_dir}"
elif [[ "$(hash_of "${dest}")" == "${legacy_hash}" ]]; then
  fail_case "${name}" "the legacy binary was not replaced" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 17: metadata mutating a tracked file stops before the build ----------
name="cargo_metadata_mutating_tracked_source_refuses_before_build"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}" \
  "FAKE_CARGO_METADATA_MUTATE=${root}/rust/Cargo.toml")"
if [[ "${rc}" != "4" ]]; then
  fail_case "${name}" "expected exit 4 (dirty source), got ${rc}" "${case_dir}"
elif ! grep -Fq 'cargo metadata introduced unstaged tracked changes' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the tracked mutation" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif ! grep -Fq 'METADATA_SAW_BACKUP=yes' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the backup did not precede the mutating cargo metadata" "${case_dir}"
elif ! assert_backup_is_regular_file "${name}" "${case_dir}"; then :
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "the canonical executable was modified" "${case_dir}"
elif ! grep -Fq 'mutated-by-cargo-metadata' "${root}/rust/Cargo.toml"; then
  fail_case "${name}" "the fixture mutation was silently restored instead of reported" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- case 18: metadata mutating the lockfile stops before the build ----------
name="cargo_metadata_mutating_the_lockfile_refuses_before_build"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
lock_before="$(hash_of "${root}/rust/Cargo.lock")"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
old_hash="$(hash_of "${dest}")"
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}" \
  "FAKE_CARGO_METADATA_MUTATE=${root}/rust/Cargo.lock")"
if [[ "${rc}" != "4" ]]; then
  fail_case "${name}" "expected exit 4 (dirty source), got ${rc}" "${case_dir}"
elif ! grep -Fq 'cargo metadata modified' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name the lockfile mutation" "${case_dir}"
elif ! grep -Fq 'Cargo.lock' "${case_dir}/stderr"; then
  fail_case "${name}" "refusal did not name Cargo.lock" "${case_dir}"
elif ! assert_no_build "${name}" "${case_dir}"; then :
elif [[ "$(hash_of "${root}/rust/Cargo.lock")" == "${lock_before}" ]]; then
  fail_case "${name}" "the fixture never actually mutated the lockfile" "${case_dir}"
elif [[ "$(hash_of "${dest}")" != "${old_hash}" ]]; then
  fail_case "${name}" "the canonical executable was modified" "${case_dir}"
else
  pass_case "${name}"
fi

# ---------- portability: a host with no GNU coreutils ----------
#
# install.sh supports Linux, macOS, and WSL and points the operator at this
# command, but the script used to reach for `realpath -m`, `sha256sum`, and
# `stat -c`. On a stock macOS the first of those aborts under `set -e` in
# section 1 -- before any backup, build, or activation -- so the whole
# canonical workflow was unreachable there.
#
# This runs the tracked script itself under a stand-in BSD userland. It is not
# a check that portable-looking text appears somewhere: the refresh has to
# actually complete, and the SHA-256 it reports has to be the real digest of
# the file it activated.
name="refresh_completes_without_gnu_realpath_stat_or_sha256sum"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
stage_bsd_utils "${case_dir}/bsdbin" bsd
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}" \
  "PATH=${case_dir}/bsdbin:${case_dir}/fakebin:/usr/bin:/bin")"
built_hash="$(hash_of "${root}/.canonical-refresh-target/release/claw" 2>/dev/null || true)"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 on a non-GNU host, got ${rc}" "${case_dir}"
elif grep -Eq 'illegal option|command not found' "${case_dir}/stderr"; then
  fail_case "${name}" "the refresh still reached for a GNU-only utility" "${case_dir}"
elif ! grep -Fq 'METADATA_SAW_BACKUP=yes' "${case_dir}/cargo.log"; then
  fail_case "${name}" "the backup no longer precedes cargo metadata" "${case_dir}"
elif ! assert_backup_is_regular_file "${name}" "${case_dir}"; then :
elif ! assert_activated "${name}" "${case_dir}" "${built_hash}"; then :
elif ! grep -Eq 'regular file, mode 0?755' "${case_dir}/stdout"; then
  fail_case "${name}" "the report did not resolve the canonical file mode" "${case_dir}"
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- portability: not even shasum or openssl ----------
#
# Guards against trading one hard dependency for another: no single hashing
# tool may be load-bearing, and the provenance digest must stay an exact
# SHA-256 whichever implementation ends up being used.
name="refresh_completes_without_sha256sum_shasum_or_openssl"
case_dir="${WORK_DIR}/${name}"; mkdir -p "${case_dir}"
root="$(stage "${case_dir}")"
head_sha="$(git -C "${root}" rev-parse HEAD)"
dest="${case_dir}/home/.local/bin/claw"
write_fake_claw "${dest}" "0ddba11"
stage_bsd_utils "${case_dir}/bsdbin" minimal
rc="$(FAKE_BUILT_SHA="${head_sha}" run_refresh "${case_dir}" "${root}" \
  "PATH=${case_dir}/bsdbin:${case_dir}/fakebin:/usr/bin:/bin")"
built_hash="$(hash_of "${root}/.canonical-refresh-target/release/claw" 2>/dev/null || true)"
if [[ "${rc}" != "0" ]]; then
  fail_case "${name}" "expected exit 0 with no sha256sum/shasum/openssl, got ${rc}" "${case_dir}"
elif grep -Eq 'illegal option|command not found' "${case_dir}/stderr"; then
  fail_case "${name}" "the refresh still reached for an unavailable utility" "${case_dir}"
elif ! assert_activated "${name}" "${case_dir}" "${built_hash}"; then :
elif ! assert_no_leftover_candidate "${name}" "${case_dir}"; then :
else
  pass_case "${name}"
fi

# ---------- portability: the GNU forms stay inside the helpers ----------
#
# The two cases above run on a host that has python3. This one keeps the
# non-portable forms from creeping back into the body of the script, where
# they would once again be reached before any fallback could apply.
name="gnu_only_utilities_stay_inside_the_portable_helpers"
helper_start="$(grep -n '^# portable utility helpers$' "${REAL_REFRESH}" | head -1 | cut -d: -f1 || true)"
helper_end="$(grep -n '^assert_dest_topology()' "${REAL_REFRESH}" | head -1 | cut -d: -f1 || true)"
if [[ -z "${helper_start}" || -z "${helper_end}" || "${helper_start}" -ge "${helper_end}" ]]; then
  fail_case "${name}" "could not locate the portable helper block" \
    "start=${helper_start} end=${helper_end}"
else
  outside="$(awk -v a="${helper_start}" -v b="${helper_end}" \
    'NR < a || NR >= b { print NR ": " $0 }' "${REAL_REFRESH}" \
    | grep -E 'realpath -m|sha256sum|stat -c' || true)"
  if [[ -n "${outside}" ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
    printf '\n=== FAIL: %s ===\n' "${name}" >&2
    printf '  reason: GNU-only utility used outside the portable helpers\n' >&2
    printf '%s\n' "${outside}" | sed 's/^/    /' >&2
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
