#!/usr/bin/env bash
# Local gate == CI gate. Runs fmt, clippy, tests and a wasm compile smoke check.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
# Compile smoke check only (bypasses the stellar-cli spec-shaking guard; NOT a deployable artifact).
rustup target add wasm32v1-none >/dev/null 2>&1 || true
SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo build --release --target wasm32v1-none -p torrent-escrow
echo "ok"
