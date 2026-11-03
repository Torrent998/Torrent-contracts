#!/usr/bin/env bash
# Release wasm with plain cargo (what CI publishes as an artifact).
# soroban-sdk 29 refuses to build for wasm unless the build system declares spec-shaking support;
# plain cargo does not shake the contract spec, so the spec section keeps unused entries. The code
# is identical and runs in the Soroban host (tests/governance.rs upgrades to this exact file).
# `stellar contract build` (scripts/build.sh) produces the canonical, spec-shaken artifact.
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32v1-none >/dev/null 2>&1 || true
SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo build --release --target wasm32v1-none -p torrent-escrow
WASM=target/wasm32v1-none/release/torrent_escrow.wasm
echo "$(sha256sum "$WASM" | cut -d' ' -f1)  $WASM ($(wc -c <"$WASM") bytes)"
