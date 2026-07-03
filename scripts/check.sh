#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
