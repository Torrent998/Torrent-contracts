#!/usr/bin/env bash
# Local gate == CI gate. Runs fmt, clippy, the release wasm build, then all tests.
# The wasm is built first because tests/governance.rs installs it as a real upgrade target.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
./scripts/build-wasm.sh
cargo test --all
echo "ok"
