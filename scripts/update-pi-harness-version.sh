#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 1 ]]; then
  echo "Usage: $0 [latest|VERSION]" >&2
  exit 1
fi
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  echo "Usage: $0 [latest|VERSION]"
  echo "Only pin the Dockerfile. For the full update, use scripts/update-harness.sh."
  exit 0
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package="@earendil-works/pi-coding-agent"
requested="${1:-latest}"
dockerfile="${repo_root}/harness/pi/Dockerfile"
version_pattern='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'

if [[ "${requested}" != "latest" && ! "${requested}" =~ ${version_pattern} ]]; then
  echo "Invalid ${package} version: ${requested}" >&2
  exit 1
fi

if [[ "$(grep -c '^ARG PI_CODING_AGENT_VERSION=' "${dockerfile}")" != 1 ]]; then
  echo "Expected exactly one PI_CODING_AGENT_VERSION ARG in ${dockerfile}" >&2
  exit 1
fi

# Resolve latest once, and verify explicit versions exist before editing.
version="$(npm view "${package}@${requested}" version)"
if [[ ! "${version}" =~ ${version_pattern} ]] ||
   [[ "${requested}" != "latest" && "${version}" != "${requested}" ]]; then
  echo "Unexpected ${package} version from npm: ${version}" >&2
  exit 1
fi

perl -0pi -e "s/^ARG PI_CODING_AGENT_VERSION=.*$/ARG PI_CODING_AGENT_VERSION=${version}/m" "${dockerfile}"

echo "Pinned ${package} to ${version} in ${dockerfile}"
echo "Full update (checks, reinstall vr, rebuild, packages, doctor):"
echo "  ${repo_root}/scripts/update-harness.sh ${version}"
