#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
for script in scripts/*.sh; do
  bash -n "${script}"
done
bash scripts/test-update-harness.sh

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
