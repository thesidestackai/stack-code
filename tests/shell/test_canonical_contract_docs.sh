#!/usr/bin/env bash
# Source-of-truth regression tests for the canonical claw install contract.
#
# Almost entirely textual: reads tracked files, writes nothing outside a
# throwaway temp directory, and never touches $HOME or the network. Locks the
# parts of the contract that live in comments and documentation, which are
# exactly the parts that drifted before:
#
#   * the canonical path is $HOME/.local/bin/claw;
#   * no tracked surface describes it as a symlink or tells an operator to
#     point it at a Cargo target directory;
#   * install.sh is described as build-only;
#   * the refresh and status commands are documented.
#
# The one exception is the last case. The build script's Git-invalidation
# contract is a BEHAVIOUR, and a purely textual guard already failed to catch
# a real staleness bug in it, so that case builds a throwaway Cargo/Git
# fixture around the tracked build.rs and observes what Cargo actually does.
# It has no dependencies, builds --offline, and lives entirely under mktemp.

# Needles below are single-quoted on purpose: they are literal source text to
# match, not expressions to expand.
# shellcheck disable=SC2016

set -euo pipefail

TEST_FILE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${TEST_FILE_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

PASS_COUNT=0
FAIL_COUNT=0

pass_case() { PASS_COUNT=$((PASS_COUNT + 1)); printf 'PASS: %s\n' "$1"; }

fail_case() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '\n=== FAIL: %s ===\n' "$1" >&2
  printf '  reason: %s\n' "$2" >&2
  if [[ -n "${3:-}" ]]; then
    printf '  evidence:\n' >&2
    printf '%s\n' "$3" | sed 's/^/    /' >&2
  fi
}

# hash_of_file <path> — content hash used to prove the fixture isolated the
# ref transition. Any stable digest will do; this never leaves the fixture.
hash_of_file() { cksum -- "$1" | awk '{print $1 "-" $2}'; }

# assert_contains <name> <file> <literal>
assert_contains() {
  local name="$1" file="$2" needle="$3"
  if [[ ! -f "${file}" ]]; then
    fail_case "${name}" "missing file: ${file}"
    return 1
  fi
  if ! grep -Fq -- "${needle}" "${file}"; then
    fail_case "${name}" "${file} does not contain: ${needle}"
    return 1
  fi
}

# assert_absent <name> <file> <literal>
assert_absent() {
  local name="$1" file="$2" needle="$3"
  if grep -Fq -- "${needle}" "${file}"; then
    fail_case "${name}" "${file} still contains: ${needle}" \
      "$(grep -Fn -- "${needle}" "${file}")"
    return 1
  fi
}

# ---------- canonical path is unchanged ----------
name="canonical_path_remains_home_local_bin_claw"
if assert_contains "${name}" scripts/a2-ide-harness.sh 'DEFAULT_CLAW="${HOME}/.local/bin/claw"' \
   && assert_contains "${name}" scripts/claw-canonical-status '${HOME:-}/.local/bin/claw' \
   && assert_contains "${name}" scripts/claw-canonical-refresh '${HOME}/.local/bin/claw' \
   && assert_contains "${name}" scripts/claw-sidestack-local '${HOME}/.local/bin/claw'; then
  pass_case "${name}"
fi

# ---------- harness no longer claims the canonical entrypoint is a symlink ----------
name="harness_no_longer_calls_canonical_claw_a_symlink"
if assert_absent "${name}" scripts/a2-ide-harness.sh 'This symlink is maintained' \
   && assert_contains "${name}" scripts/a2-ide-harness.sh 'This is a REGULAR FILE' \
   && assert_contains "${name}" scripts/a2-ide-harness.sh 'scripts/claw-canonical-refresh'; then
  pass_case "${name}"
fi

# ---------- the A2_CLAW override contract is preserved ----------
name="harness_a2_claw_override_preserved"
if assert_contains "${name}" scripts/a2-ide-harness.sh 'A2_CLAW="${A2_CLAW:-$DEFAULT_CLAW}"' \
   && assert_contains "${name}" scripts/a2-ide-harness.sh 'A2_CLAW=/absolute/path/to/claw'; then
  pass_case "${name}"
fi

# ---------- no tracked surface links the canonical path at a Cargo target ----------
#
# Deliberately narrow: a line must mention the canonical path AND a link or a
# Cargo target artifact before it counts. Generic references to build
# artifacts are legitimate and are not flagged.
name="no_tracked_instruction_links_canonical_path_to_a_cargo_target"
offenders=""
while IFS= read -r file; do
  [[ -f "${file}" ]] || continue
  while IFS= read -r line; do
    if [[ "${line}" == *".local/bin/claw"* ]] \
       && { [[ "${line}" == *"ln -s"* ]] \
         || [[ "${line}" == *"target/debug/claw"* ]] \
         || [[ "${line}" == *"target/release/claw"* ]]; }; then
      # A prohibition that names both is documentation, not an instruction.
      if [[ "${line}" == *"not"* || "${line}" == *"Do not"* || "${line}" == *"never"* \
            || "${line}" == *"Never"* || "${line}" == *"NOT"* ]]; then
        continue
      fi
      offenders+="${file}: ${line}"$'\n'
    fi
  done < "${file}"
done < <(git ls-files -- 'README.md' 'USAGE.md' 'install.sh' 'docs/*.md' 'scripts/*' 'rust/README.md')
if [[ -n "${offenders}" ]]; then
  fail_case "${name}" "a tracked surface points the canonical claw path at a Cargo target" "${offenders}"
else
  pass_case "${name}"
fi

# ---------- install.sh is described as build-only ----------
name="install_sh_is_documented_as_build_only"
if assert_contains "${name}" install.sh 'BUILD-ONLY' \
   && assert_contains "${name}" install.sh 'It does not copy anything into' \
   && assert_contains "${name}" install.sh 'This was a build only.' \
   && assert_contains "${name}" USAGE.md 'build helper, not a host installer'; then
  pass_case "${name}"
fi

# ---------- the refresh command is documented ----------
name="canonical_refresh_command_is_documented"
if assert_contains "${name}" README.md 'scripts/claw-canonical-refresh' \
   && assert_contains "${name}" USAGE.md 'scripts/claw-canonical-refresh' \
   && assert_contains "${name}" install.sh 'scripts/claw-canonical-refresh'; then
  pass_case "${name}"
fi

# ---------- the status command is documented ----------
name="canonical_status_command_is_documented"
if assert_contains "${name}" README.md 'scripts/claw-canonical-status' \
   && assert_contains "${name}" USAGE.md 'scripts/claw-canonical-status' \
   && assert_contains "${name}" install.sh 'scripts/claw-canonical-status'; then
  pass_case "${name}"
fi

# ---------- the regular-file contract and the Cargo-target warning are stated ----------
name="regular_file_contract_and_cargo_target_warning_are_documented"
if assert_contains "${name}" README.md 'never a symlink' \
   && assert_contains "${name}" README.md 'a Cargo build cache must never be the live operator binary' \
   && assert_contains "${name}" USAGE.md 'regular file' \
   && assert_contains "${name}" USAGE.md 'A Cargo build cache must never be the live operator binary'; then
  pass_case "${name}"
fi

# ---------- cargo install / ~/.cargo/bin is classified, not canonical ----------
name="cargo_install_is_classified_as_non_canonical"
if assert_contains "${name}" README.md 'SideStackAI canonical operator entrypoint' \
   && assert_contains "${name}" README.md 'generic upstream convenience' \
   && assert_contains "${name}" USAGE.md "not SideStack's canonical entrypoint"; then
  pass_case "${name}"
fi

# ---------- the wrapper no longer resolves claw from PATH ----------
name="wrapper_does_not_resolve_claw_from_path"
if assert_absent "${name}" scripts/claw-sidestack-local 'command -v claw' \
   && assert_absent "${name}" scripts/claw-sidestack-local 'exec claw "$@"' \
   && assert_contains "${name}" scripts/claw-sidestack-local 'exec "${CANONICAL_CLAW}" "$@"'; then
  pass_case "${name}"
fi

# ---------- CI actually runs the canonical safety suites ----------
#
# The scripts and tests that enforce the canonical install contract are
# worthless if CI never executes them. These assertions are deliberately
# narrow: exact command lines and exact path-filter entries, nothing about
# unrelated workflow formatting.
CI_WORKFLOW=".github/workflows/rust-ci.yml"

name="ci_triggers_on_the_new_production_scripts"
# The two production scripts must appear in BOTH the push and pull_request
# path filters, so a change to either one triggers CI on its own.
refresh_filters="$(grep -c '^      - scripts/claw-canonical-refresh$' "${CI_WORKFLOW}" || true)"
status_filters="$(grep -c '^      - scripts/claw-canonical-status$' "${CI_WORKFLOW}" || true)"
if [[ "${refresh_filters}" -ne 2 ]]; then
  fail_case "${name}" \
    "scripts/claw-canonical-refresh appears in ${refresh_filters} path filters, expected 2 (push + pull_request)"
elif [[ "${status_filters}" -ne 2 ]]; then
  fail_case "${name}" \
    "scripts/claw-canonical-status appears in ${status_filters} path filters, expected 2 (push + pull_request)"
else
  pass_case "${name}"
fi

name="ci_syntax_checks_the_canonical_contract_shell"
syntax_ok=1
for needle in \
  'bash -n scripts/claw-canonical-refresh' \
  'bash -n scripts/claw-canonical-status' \
  'bash -n tests/shell/test_claw_canonical_refresh.sh' \
  'bash -n tests/shell/test_claw_canonical_status.sh' \
  'bash -n tests/shell/test_canonical_contract_docs.sh'; do
  if ! assert_contains "${name}" "${CI_WORKFLOW}" "${needle}"; then
    syntax_ok=0
    break
  fi
done
[[ "${syntax_ok}" -eq 1 ]] && pass_case "${name}"

name="ci_shellchecks_the_canonical_contract_shell"
shellcheck_ok=1
for needle in \
  'shellcheck scripts/claw-canonical-refresh' \
  'shellcheck scripts/claw-canonical-status' \
  'shellcheck tests/shell/test_claw_canonical_refresh.sh' \
  'shellcheck tests/shell/test_claw_canonical_status.sh' \
  'shellcheck tests/shell/test_canonical_contract_docs.sh'; do
  if ! assert_contains "${name}" "${CI_WORKFLOW}" "${needle}"; then
    shellcheck_ok=0
    break
  fi
done
[[ "${shellcheck_ok}" -eq 1 ]] && pass_case "${name}"

name="ci_executes_the_canonical_contract_suites"
run_ok=1
for needle in \
  'bash tests/shell/test_claw_canonical_refresh.sh' \
  'bash tests/shell/test_claw_canonical_status.sh' \
  'bash tests/shell/test_canonical_contract_docs.sh'; do
  if ! assert_contains "${name}" "${CI_WORKFLOW}" "${needle}"; then
    run_ok=0
    break
  fi
done
[[ "${run_ok}" -eq 1 ]] && pass_case "${name}"

# the pre-existing suites must not have been displaced by the additions
name="ci_retains_the_existing_shell_suites"
if assert_contains "${name}" "${CI_WORKFLOW}" 'bash tests/shell/test_claw_sidestack_local.sh' \
   && assert_contains "${name}" "${CI_WORKFLOW}" 'bash tests/shell/test_a2_tier3_write_orchestrator.sh' \
   && assert_contains "${name}" "${CI_WORKFLOW}" 'bash tests/shell/test_a2_ide_harness_plan_path.sh'; then
  pass_case "${name}"
fi

# ---------- the full-SHA provenance contract ----------
#
# Claw's build-time provenance must be the FULL commit SHA. `rev-parse --short`
# here is what made the canonical freshness check unprovable.
name="build_rs_embeds_the_full_commit_sha"
if assert_contains "${name}" rust/crates/rusty-claude-cli/build.rs '.args(["rev-parse", "HEAD"])' \
   && assert_absent "${name}" rust/crates/rusty-claude-cli/build.rs '"--short"'; then
  pass_case "${name}"
fi

# ---------- the provenance watchers name real Git metadata ----------
#
# Cargo resolves relative `rerun-if-changed` entries against the package root,
# so the old `.git/HEAD` / `.git/refs` entries pointed at
# `rust/crates/rusty-claude-cli/.git/...`, which never exists -- and in a
# linked worktree `.git` is a pointer file, so even the repo-relative form is
# absent. Watchers must be resolved through Git itself, covering a branch
# checkout, a detached HEAD, and a packed ref.
name="build_rs_watches_resolved_git_metadata_paths"
if assert_absent "${name}" rust/crates/rusty-claude-cli/build.rs 'cargo:rerun-if-changed=.git/HEAD' \
   && assert_absent "${name}" rust/crates/rusty-claude-cli/build.rs 'cargo:rerun-if-changed=.git/refs' \
   && assert_contains "${name}" rust/crates/rusty-claude-cli/build.rs 'git_output(&["rev-parse", "--git-path", "HEAD"])' \
   && assert_contains "${name}" rust/crates/rusty-claude-cli/build.rs 'git_output(&["symbolic-ref", "-q", "HEAD"])' \
   && assert_contains "${name}" rust/crates/rusty-claude-cli/build.rs 'git_output(&["rev-parse", "--git-path", "packed-refs"])' \
   && assert_contains "${name}" rust/crates/rusty-claude-cli/build.rs 'fn watch_if_present'; then
  pass_case "${name}"
fi

name="refresh_and_status_require_a_full_forty_character_sha"
if assert_contains "${name}" scripts/claw-canonical-refresh '^[0-9a-f]{40}$' \
   && assert_contains "${name}" scripts/claw-canonical-status '^[0-9a-f]{40}$' \
   && assert_absent "${name}" scripts/claw-canonical-refresh '^[0-9a-f]{7,40}$' \
   && assert_absent "${name}" scripts/claw-canonical-status '^[0-9a-f]{7,40}$'; then
  pass_case "${name}"
fi

# ---------- the backup precedes every Cargo command ----------
name="backup_precedes_cargo_metadata_in_the_refresh_script"
backup_line="$(grep -n 'cp -Pp -- "${DEST}" "${BACKUP_FILE}"' scripts/claw-canonical-refresh | head -1 | cut -d: -f1 || true)"
metadata_line="$(grep -n 'cargo metadata --format-version 1 --locked' scripts/claw-canonical-refresh | head -1 | cut -d: -f1 || true)"
build_line="$(grep -n 'cargo build --locked' scripts/claw-canonical-refresh | head -1 | cut -d: -f1 || true)"
if [[ -z "${backup_line}" || -z "${metadata_line}" || -z "${build_line}" ]]; then
  fail_case "${name}" "could not locate the backup, metadata, and build steps" \
    "backup=${backup_line} metadata=${metadata_line} build=${build_line}"
elif [[ "${backup_line}" -ge "${metadata_line}" ]]; then
  fail_case "${name}" "the backup copy does not precede cargo metadata" \
    "backup line ${backup_line}, cargo metadata line ${metadata_line}"
elif [[ "${metadata_line}" -ge "${build_line}" ]]; then
  fail_case "${name}" "cargo metadata does not precede cargo build" \
    "metadata line ${metadata_line}, build line ${build_line}"
else
  pass_case "${name}"
fi

name="cargo_metadata_is_locked_offline_and_no_deps"
if assert_contains "${name}" scripts/claw-canonical-refresh \
     'cargo metadata --format-version 1 --locked --offline --no-deps'; then
  pass_case "${name}"
fi

# ---------- the backup copy never dereferences ----------
name="backup_copy_is_non_dereferencing_and_topology_verified"
if assert_contains "${name}" scripts/claw-canonical-refresh 'cp -Pp -- "${DEST}" "${BACKUP_FILE}"' \
   && assert_contains "${name}" scripts/claw-canonical-refresh 'backup copy is a symlink' \
   && assert_contains "${name}" scripts/claw-canonical-refresh 'backup copy is not a regular file'; then
  pass_case "${name}"
fi

# ---------- the residual same-user race is stated, not hidden ----------
#
# The shell implementation narrows the topology race; it cannot close it. That
# limit must stay written down rather than being quietly implied by rechecks.
name="residual_same_user_race_is_documented"
if assert_contains "${name}" scripts/claw-canonical-refresh 'Residual race' \
   && assert_contains "${name}" scripts/claw-canonical-status 'Residual race' \
   && assert_contains "${name}" scripts/claw-sidestack-local 'Residual race' \
   && assert_contains "${name}" scripts/claw-canonical-refresh 'SAME-USER' \
   && assert_contains "${name}" scripts/claw-canonical-status 'SAME-USER' \
   && assert_contains "${name}" scripts/claw-sidestack-local 'SAME-USER'; then
  pass_case "${name}"
fi

# ---------- the build script actually invalidates across a packed ref ----------
#
# Regression guard for the staleness bug the textual case above could not see.
# When the checked-out branch exists only in `packed-refs`, its loose ref file
# is absent, so a watcher that only names files that exist drops it. The next
# commit writes the loose ref back WITHOUT touching `HEAD` or `packed-refs` --
# and a warm Cargo target happily keeps serving the previous commit's GIT_SHA.
#
# Two things have to hold at once, which is why this is behavioural:
#   * the packed -> loose transition must rerun the build script;
#   * an idle rebuild must NOT, because "watch the missing file" would fix the
#     first at the cost of recompiling forever.
name="build_rs_invalidates_git_sha_across_a_packed_ref_transition"
if ! command -v cargo >/dev/null 2>&1; then
  fail_case "${name}" "cargo is required to verify the build-script invalidation contract"
else
  fixture="$(mktemp -d -t canonical-build-rs-fixture.XXXXXX)"
  trap 'rm -rf "${fixture}"' EXIT INT TERM
  mkdir -p "${fixture}/src"
  cp -- rust/crates/rusty-claude-cli/build.rs "${fixture}/build.rs"
  cat > "${fixture}/Cargo.toml" <<'FIXTURE_TOML'
[package]
name = "canonical-provenance-fixture"
version = "0.0.0"
edition = "2021"

[[bin]]
name = "canonical-provenance-fixture"
path = "src/main.rs"
FIXTURE_TOML
  cat > "${fixture}/src/main.rs" <<'FIXTURE_MAIN'
fn main() {
    println!("{}", env!("GIT_SHA"));
}
FIXTURE_MAIN
  printf '/target\n' > "${fixture}/.gitignore"

  # A nested branch name is deliberate: pack-refs prunes the intermediate
  # refs/heads/<dir>/ too, so the watcher has to cope with a ref whose parent
  # directory does not exist either.
  fixture_branch="fixture/packed-ref-branch"
  git -C "${fixture}" init -q -b "${fixture_branch}"
  git -C "${fixture}" config user.name fixture
  git -C "${fixture}" config user.email fixture@example.invalid
  git -C "${fixture}" config commit.gpgsign false
  git -C "${fixture}" add Cargo.toml src/main.rs build.rs .gitignore
  git -C "${fixture}" commit -q -m "commit A"
  sha_a="$(git -C "${fixture}" rev-parse HEAD)"

  git -C "${fixture}" pack-refs --all --prune
  head_before="$(cat "${fixture}/.git/HEAD")"
  packed_before="$(hash_of_file "${fixture}/.git/packed-refs")"

  fixture_build() {
    ( cd "${fixture}" \
      && CARGO_TARGET_DIR="${fixture}/target" cargo build --offline -q 2>&1 )
  }
  fixture_sha() { "${fixture}/target/debug/canonical-provenance-fixture"; }
  # Cargo prints "Fresh <pkg>" under -v only when it skips the unit entirely.
  # Colour is pinned off for this one command on purpose: CI exports
  # CARGO_TERM_COLOR=always, which wraps the status word so the line arrives as
  # "<ESC>[1m<ESC>[92m       Fresh<ESC>[0m canonical-provenance-fixture", and the
  # literal match below would then report a phantom recompile on a fixture Cargo
  # actually skipped. Only this helper parses Cargo's human output, so only this
  # helper needs the override.
  fixture_is_fresh() {
    ( cd "${fixture}" \
      && CARGO_TARGET_DIR="${fixture}/target" CARGO_TERM_COLOR=never \
        cargo build --offline -v 2>&1 ) \
      | grep -Fq 'Fresh canonical-provenance-fixture'
  }

  build_err=""
  if ! build_err="$(fixture_build)"; then
    fail_case "${name}" "the fixture did not build at commit A" "${build_err}"
  elif [[ -e "${fixture}/.git/refs/heads/${fixture_branch}" ]]; then
    fail_case "${name}" "fixture setup failed: the branch ref was not packed away"
  elif [[ "$(fixture_sha)" != "${sha_a}" ]]; then
    fail_case "${name}" "the packed-ref build did not embed commit A" \
      "expected ${sha_a}, got $(fixture_sha)"
  elif ! fixture_is_fresh; then
    # Emitting the nonexistent loose ref path would land here: Cargo treats an
    # absent rerun-if-changed target as unconditionally stale.
    fail_case "${name}" "an idle rebuild recompiled; the packed-ref watcher is unconditionally stale"
  else
    printf 'unrelated validation content\n' > "${fixture}/VALIDATION.txt"
    git -C "${fixture}" add VALIDATION.txt
    git -C "${fixture}" commit -q -m "commit B"
    sha_b="$(git -C "${fixture}" rev-parse HEAD)"
    packed_after="$(hash_of_file "${fixture}/.git/packed-refs")"

    if [[ "${sha_b}" == "${sha_a}" ]]; then
      fail_case "${name}" "fixture setup failed: commit B did not advance HEAD"
    elif [[ "$(cat "${fixture}/.git/HEAD")" != "${head_before}" ]]; then
      fail_case "${name}" "fixture setup failed: HEAD changed, so this no longer isolates the ref"
    elif [[ "${packed_after}" != "${packed_before}" ]]; then
      fail_case "${name}" "fixture setup failed: packed-refs changed, so this no longer isolates the ref"
    elif [[ ! -e "${fixture}/.git/refs/heads/${fixture_branch}" ]]; then
      fail_case "${name}" "fixture setup failed: commit B did not create the loose ref"
    elif ! build_err="$(fixture_build)"; then
      fail_case "${name}" "the fixture did not rebuild at commit B" "${build_err}"
    elif [[ "$(fixture_sha)" == "${sha_a}" ]]; then
      fail_case "${name}" \
        "GIT_SHA is stale: the packed -> loose ref transition did not rerun the build script" \
        "still reporting commit A ${sha_a}; HEAD is now ${sha_b}"
    elif [[ "$(fixture_sha)" != "${sha_b}" ]]; then
      fail_case "${name}" "the rebuild embedded neither commit" \
        "expected ${sha_b}, got $(fixture_sha)"
    else
      pass_case "${name}"
    fi
  fi
  rm -rf "${fixture}"
  trap - EXIT INT TERM
fi

# ---------- summary ----------
if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '\nFAIL: %d cases failed, %d passed\n' "${FAIL_COUNT}" "${PASS_COUNT}" >&2
  exit 1
fi
printf '\nOK: %d cases passed\n' "${PASS_COUNT}"
