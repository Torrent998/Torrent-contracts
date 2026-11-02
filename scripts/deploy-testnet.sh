#!/usr/bin/env bash
# Deploys torrent-escrow to Stellar TESTNET ONLY and records deployments/testnet.json.
# STATUS: documented, never executed by the authors (no stellar-cli or network in the sandbox).
# Uses a stellar-cli key ALIAS from your local keystore. No secret is read from or written to this repo.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${STELLAR_SOURCE_ALIAS:?set STELLAR_SOURCE_ALIAS to a funded TESTNET identity alias (stellar keys generate --fund ...)}"
NETWORK=testnet
[ "${STELLAR_NETWORK:-testnet}" = "testnet" ] || { echo "Testnet only. Mainnet needs the go-live review in docs/PROJECT_MASTER.md"; exit 1; }
./scripts/build.sh
WASM=target/wasm32v1-none/release/torrent_escrow.wasm
WASM_HASH=$(stellar contract upload --wasm "$WASM" --source-account "$STELLAR_SOURCE_ALIAS" --network "$NETWORK")
CONTRACT_ID=$(stellar contract deploy --wasm-hash "$WASM_HASH" --source-account "$STELLAR_SOURCE_ALIAS" --network "$NETWORK")
echo "contract: $CONTRACT_ID"
# Deployment ledger: read it from the deploy transaction on a block explorer / `stellar` output and put it in
# deployments/testnet.json as deployLedger (torrent-indexer needs it as CHAIN_START_LEDGER).
cat > deployments/testnet.json <<JSON
{
  "network": "testnet",
  "networkPassphrase": "Test SDF Network ; September 2015",
  "contractId": "$CONTRACT_ID",
  "wasmHash": "$WASM_HASH",
  "abiVersion": 1,
  "gitSha": "$(git rev-parse HEAD)",
  "deployLedger": null,
  "deployTx": null
}
JSON
echo "Fill deployLedger/deployTx, then set TORRENT_ESCROW_CONTRACT_ID / CHAIN_START_LEDGER in torrent-indexer and torrent-api."
