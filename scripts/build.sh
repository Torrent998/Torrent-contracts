#!/usr/bin/env bash
# Deployable build. Requires stellar-cli >= 25.2.0 (soroban-sdk 29 spec shaking). NOT run in CI sandbox.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v stellar >/dev/null || { echo "stellar-cli not found: https://developers.stellar.org/docs/tools/cli"; exit 1; }
stellar --version
stellar contract build --package torrent-escrow
sha256sum target/wasm32v1-none/release/torrent_escrow.wasm
