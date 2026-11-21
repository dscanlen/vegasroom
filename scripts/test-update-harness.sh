#!/usr/bin/env bash
# Offline contract tests: no Docker daemon, registry, or real installation.
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
suite="$(mktemp -d)"
trap 'rm -rf -- "$suite"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

setup() {
  fixture="$(mktemp -d "${suite}/case.XXXXXX")"
  repo="${fixture}/repo with spaces"
  mkdir -p "${repo}/scripts" "${repo}/harness/pi" "${fixture}/bin"
  cp "${repo_root}"/scripts/{update-harness.sh,check.sh} "${repo}/scripts/"
  # check.sh includes this suite; do not recurse inside the fixture.
  printf '#!/usr/bin/env bash\ntrue\n' > "${repo}/scripts/test-update-harness.sh"
  cp "${repo_root}/harness/pi/Dockerfile" "${repo}/harness/pi/Dockerfile"
  cp "${repo}/harness/pi/Dockerfile" "${fixture}/original"
  export TEST_FIXTURE="${fixture}" TEST_FAIL="" TEST_VERSION="1.2.3"
  export CARGO_INSTALL_ROOT="${fixture}/install root"
  printf '0.1.0\n' > "${fixture}/active-version"
  : > "${fixture}/events"
  # Each mock records a stage and can fail before later stages run.
  printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' '
record() {
  echo "$1" >> "$TEST_FIXTURE/events"
  if [[ "$TEST_FAIL" == "$1" ]]; then exit 42; fi
}
case "$(basename "$0")" in
  docker)
    [[ "$*" == "compose version" ]]
    record preflight
    ;;
  cargo)
    record "cargo-$1"
    if [[ "$1" == install ]]; then
      [[ "$2 $3 $4" == "--locked --force --path" ]]
      [[ "$6" == --root ]]
      mkdir -p "$7/bin"
      cp "$TEST_FIXTURE/bin/vr" "$7/bin/vr"
    fi
    ;;
  npm)
    case "$1" in
      view)
        record resolve
        [[ "$2" == @earendil-works/pi-coding-agent@* && "$3" == version ]]
        printf "%s\n" "$TEST_VERSION"
        ;;
      install)
        record npm-install
        [[ "$2 $3 $4" == "-g --ignore-scripts --prefix" ]]
        [[ "$5" == "$TEST_FIXTURE/npm-global" ]]
        [[ "$6" == "@earendil-works/pi-coding-agent@$TEST_VERSION" ]]
        printf "%s\n" "$TEST_VERSION" > "$TEST_FIXTURE/active-version"
        ;;
      *) exit 99 ;;
    esac
    ;;
  vr)
    [[ "$0" == "$CARGO_INSTALL_ROOT/bin/vr" ]]
    case "$1" in
      init) [[ "$2" == --build ]]; record build ;;
      doctor) record doctor ;;
      shell)
        [[ "$2 $3 $4 $5" == "-- sh -eu -c" ]]
        record shell
        script="$6"
        script="${script//\/usr\/local\/bin\/pi/$TEST_FIXTURE/bin/baked-pi}"
        script="${script//\/home\/agent\/.npm-global/$TEST_FIXTURE/npm-global}"
        sh -eu -c "$script" "$7" "$8"
        ;;
      *) exit 99 ;;
    esac
    ;;
  baked-pi)
    [[ "$1" == --version ]]
    record baked-version
    if [[ "$TEST_FAIL" == baked-mismatch ]]; then echo 0.0.0; else echo "$TEST_VERSION"; fi
    ;;
  pi)
    case "$*" in
      --version)
        record active-version
        if [[ "$TEST_FAIL" == active-mismatch ]]; then
          echo 0.0.0
        else
          read -r version < "$TEST_FIXTURE/active-version"
          echo "$version"
        fi
        ;;
      "update --extensions")
        record extensions
        [[ -z "${PI_OFFLINE:-}" ]]
        [[ "$PWD" != *"repo with spaces"* ]]
        ;;
      list) record list ;;
      *) exit 99 ;;
    esac
    ;;
esac' > "${fixture}/bin/mock"
  chmod +x "${fixture}/bin/mock"
  for tool in cargo npm docker vr pi baked-pi; do
    cp "${fixture}/bin/mock" "${fixture}/bin/${tool}"
  done
}

run_update() {
  (cd "${suite}" && PATH="${fixture}/bin:${PATH}" TMPDIR="${fixture}" PI_OFFLINE=1 \
    bash "${repo}/scripts/update-harness.sh" "$@") > "${fixture}/output" 2>&1
}

assert_event() {
  grep -qx -- "$1" "${fixture}/events" || fail "missing stage $1"
}
assert_no_event() {
  if grep -qx -- "$1" "${fixture}/events"; then fail "unexpected stage $1"; fi
}
assert_pin_unchanged() {
  cmp -s "${fixture}/original" "${repo}/harness/pi/Dockerfile" || fail "pin changed on invalid input"
}

setup
run_update || fail "default update failed: $(<"${fixture}/output")"
expected=$'preflight\nresolve\ncargo-fmt\ncargo-clippy\ncargo-test\ncargo-install\nbuild\nshell\nbaked-version\nnpm-install\nactive-version\nextensions\nactive-version\nlist\ndoctor'
[[ "$(<"${fixture}/events")" == "${expected}" ]] || fail "incorrect update order"
grep -qx 'ARG PI_CODING_AGENT_VERSION=1.2.3' "${repo}/harness/pi/Dockerfile" || fail "pin not updated"
grep -q 'runtime checks passed' "${fixture}/output" || fail "missing success summary"

setup
TEST_VERSION=0.85.1-rc.2
printf '9.9.9\n' > "${fixture}/active-version"
run_update "${TEST_VERSION}" || fail "explicit prerelease update failed"
assert_event doctor

for input in bad '1.2' '1.2.3;touch nope' --unknown; do
  setup
  if run_update "${input}"; then fail "accepted invalid version ${input}"; fi
  assert_no_event resolve
  assert_pin_unchanged
done

for result in '' garbage $'1.2.3\n1.2.4' 9.9.9; do
  setup
  TEST_VERSION="${result}"
  if run_update 1.2.3; then fail "accepted unexpected registry result"; fi
  assert_no_event cargo-fmt
  assert_pin_unchanged
done

for failed_stage in preflight resolve cargo-fmt cargo-clippy cargo-test cargo-install build shell baked-version baked-mismatch npm-install active-mismatch extensions list doctor; do
  setup
  TEST_FAIL="${failed_stage}"
  if run_update; then
    fail "ignored failure in ${failed_stage}"
  else
    status=$?
  fi
  case "${failed_stage}" in
    baked-mismatch|active-mismatch) [[ "${status}" == 1 ]] || fail "lost mismatch status" ;;
    *) [[ "${status}" == 42 ]] || fail "lost failure status in ${failed_stage}" ;;
  esac
  if grep -q 'runtime checks passed' "${fixture}/output"; then fail "false success in ${failed_stage}"; fi
  if [[ "${failed_stage}" != doctor ]]; then assert_no_event doctor; fi
  case "${failed_stage}" in
    preflight|resolve) assert_pin_unchanged ;;
    cargo-*) assert_no_event build ;;
    build) assert_no_event shell ;;
    baked-mismatch|npm-install|active-mismatch) assert_no_event extensions ;;
  esac
done

setup
run_update --help || fail "help failed"
[[ ! -s "${fixture}/events" ]] || fail "help ran commands"
if run_update latest extra; then fail "accepted extra arguments"; fi
assert_pin_unchanged

setup
printf '\nARG PI_CODING_AGENT_VERSION=0.0.0\n' >> "${repo}/harness/pi/Dockerfile"
if run_update; then fail "accepted duplicate ARG"; fi
assert_no_event resolve

setup
printf 'FROM node:24\n' > "${repo}/harness/pi/Dockerfile"
if run_update; then fail "accepted missing ARG"; fi
assert_no_event resolve

setup
mkdir "${fixture}/missing-docker"
for tool in bash dirname npm perl cargo; do
  target="$(PATH="${fixture}/bin:${PATH}" command -v "${tool}")"
  ln -s "${target}" "${fixture}/missing-docker/${tool}"
done
if PATH="${fixture}/missing-docker" "${BASH}" "${repo}/scripts/update-harness.sh" > "${fixture}/output" 2>&1; then
  fail "ignored missing Docker"
fi
grep -q 'Missing required command: docker' "${fixture}/output" || fail "missing prerequisite diagnostic"
assert_pin_unchanged

echo "Harness update script tests passed."
