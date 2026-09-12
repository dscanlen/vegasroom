#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  echo "Usage: $0 [latest|VERSION]"
  echo "Pin Pi, run checks, reinstall vr, rebuild images, update global Pi packages, and run doctor."
  echo "Run on the Docker host with Pi sessions closed. Defaults to the npm latest release."
  echo "Explicit package pins and local extension files are not upgraded. See README.md."
}

if [[ $# -gt 1 ]]; then
  usage >&2
  exit 1
fi
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repo_root}"
stage="prerequisite checks"
trap 'status=$?; printf "Update stopped during %s (exit %s). Completed changes were not rolled back; fix the error and rerun this script.\n" "$stage" "$status" >&2; exit "$status"' ERR

for tool in npm perl cargo docker; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "Missing required command: ${tool}. Run this script on the Docker host." >&2
    exit 1
  fi
done
docker compose version

stage="version pin"
bash scripts/update-pi-harness-version.sh "${1:-latest}"
version="$(grep '^ARG PI_CODING_AGENT_VERSION=' harness/pi/Dockerfile)"
version="${version#*=}"

stage="repository checks"
bash scripts/check.sh

# vr embeds the Dockerfile and Compose file at compile time. Never use a stale
# vr from PATH, even if another installation appears earlier on PATH.
stage="vr installation"
install_root="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-${HOME}/.cargo}}"
if [[ "${install_root}" != /* ]]; then
  install_root="${repo_root}/${install_root}"
fi
cargo install --locked --force --path "${repo_root}" --root "${install_root}"
vr="${install_root}/bin/vr"

stage="harness and environment image rebuild"
"${vr}" init --build

stage="persistent Pi and extension package update"
"${vr}" shell -- sh -eu -c '
  version="$1"
  # Avoid changing project-local packages in the default mounted workspace.
  cd "$(mktemp -d)"
  unset PI_OFFLINE
  export PI_SKIP_VERSION_CHECK=1
  export GIT_TERMINAL_PROMPT=0
  export GIT_SSH_COMMAND="ssh -o BatchMode=yes -o ConnectTimeout=10"

  check_version() {
    actual="$("$1" --version)"
    if [ "$actual" != "$version" ]; then
      printf "Pi version mismatch for %s: expected %s, got %s\n" "$1" "$version" "$actual" >&2
      exit 1
    fi
  }

  check_version /usr/local/bin/pi
  # The bind-mounted prefix precedes the baked install on PATH. Synchronize it
  # to the exact pin, including when downgrading; do not use pi update --all.
  npm install -g --ignore-scripts --prefix /home/agent/.npm-global "@earendil-works/pi-coding-agent@$version"
  hash -r
  check_version pi
  pi update --extensions
  check_version pi
  pi list
' vr-update "${version}"

stage="runtime checks"
"${vr}" doctor
printf '\nUpdated Pi to %s; repository and runtime checks passed. Restart Pi sessions to load updated packages.\n' "${version}"
