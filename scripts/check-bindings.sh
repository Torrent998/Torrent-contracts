#!/usr/bin/env bash
# Fails if bindings/*.json drift from what the contract actually emits/exposes.
# The golden stream is regenerated from the real contract; any diff means docs/ABI are stale.
set -euo pipefail
cd "$(dirname "$0")/.."
UPDATE_GOLDEN=1 cargo test -p torrent-escrow --test events stream_golden_matches_real_emitted_events -- --exact >/dev/null
git diff --exit-code -- bindings || { echo "bindings are stale: commit the regenerated files (and mirror them into torrent-indexer/torrent-api fixtures)"; exit 1; }
cargo test -p torrent-escrow --test abi_surface --test events
echo "bindings in sync"
