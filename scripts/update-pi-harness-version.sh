#!/usr/bin/env bash
set -euo pipefail

package="@earendil-works/pi-coding-agent"
version="${1:-latest}"
dockerfile="harness/pi/Dockerfile"

if [[ "${version}" == "latest" ]]; then
  version="$(npm view "${package}" version)"
fi

if [[ ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.+][0-9A-Za-z.-]+)?$ ]]; then
  echo "Invalid ${package} version: ${version}" >&2
  exit 1
fi

if ! grep -q '^ARG PI_CODING_AGENT_VERSION=' "${dockerfile}"; then
  echo "Could not find PI_CODING_AGENT_VERSION ARG in ${dockerfile}" >&2
  exit 1
fi

perl -0pi -e "s/^ARG PI_CODING_AGENT_VERSION=.*$/ARG PI_CODING_AGENT_VERSION=${version}/m" "${dockerfile}"

echo "Pinned ${package} to ${version} in ${dockerfile}"
echo "Next: run ./scripts/check.sh, then rebuild with: vr init --build"
