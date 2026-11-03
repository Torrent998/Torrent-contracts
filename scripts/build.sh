#!/usr/bin/env bash
# Deployable build for Testnet. Prefers stellar-cli >= 25.2.0 (`stellar contract build`, which
# shakes the contract spec); falls back to scripts/build-wasm.sh with a warning.
set -euo pipefail
cd "$(dirname "$0")/.."
WASM=target/wasm32v1-none/release/torrent_escrow.wasm
if command -v stellar >/dev/null; then
  stellar --version
  stellar contract build --package torrent-escrow
else
  echo "warning: stellar-cli not found; building with cargo (spec not shaken). https://developers.stellar.org/docs/tools/cli" >&2
  ./scripts/build-wasm.sh >/dev/null
fi
sha256sum "$WASM"
