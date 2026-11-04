#!/usr/bin/env bash
# Deploys torrent-escrow (ABI v2) to Stellar TESTNET ONLY and records deployments/testnet.json.
#
# STATUS: run for real on 2026-11-04 (see docs/DEPLOYMENT.md); deployments/testnet.json is its output.
#
# Requirements: stellar-cli >= 25.2.0, curl, jq, a funded Testnet identity in your local keystore.
# No secret is read from or written to this repository: stellar-cli signs with a key ALIAS.
#
#   stellar keys generate --fund torrent-deployer --network testnet
#   STELLAR_SOURCE_ALIAS=torrent-deployer ./scripts/deploy-testnet.sh
#
# Optional:
#   TORRENT_ADMIN_ADDRESS  G... address that may rotate the admin and propose/execute timelocked
#                          upgrades (default: the deployer). It has NO authority over pools.
#   STELLAR_RPC_URL        default https://soroban-testnet.stellar.org
set -euo pipefail
cd "$(dirname "$0")/.."

: "${STELLAR_SOURCE_ALIAS:?set STELLAR_SOURCE_ALIAS to a funded TESTNET identity alias (stellar keys generate --fund ...)}"
NETWORK=testnet
PASSPHRASE="Test SDF Network ; September 2015"
RPC="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
[ "${STELLAR_NETWORK:-testnet}" = "testnet" ] || { echo "Testnet only. Mainnet needs the go-live review in docs/SECURITY.md" >&2; exit 1; }
for bin in stellar curl jq git; do command -v "$bin" >/dev/null || { echo "missing: $bin" >&2; exit 1; }; done
[ -z "$(git status --porcelain -- contracts Cargo.toml Cargo.lock)" ] || { echo "contract sources have uncommitted changes; deploy a committed revision" >&2; exit 1; }

# The RPC must be on the network we think it is (fail closed on a wrong passphrase).
got=$(curl -fsS "$RPC" -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getNetwork"}' | jq -r .result.passphrase)
[ "$got" = "$PASSPHRASE" ] || { echo "RPC $RPC reports passphrase '$got', expected Testnet" >&2; exit 1; }

DEPLOYER=$(stellar keys address "$STELLAR_SOURCE_ALIAS")
ADMIN="${TORRENT_ADMIN_ADDRESS:-$DEPLOYER}"
LOG=$(mktemp); trap 'rm -f "$LOG"' EXIT

./scripts/build.sh
WASM=target/wasm32v1-none/release/torrent_escrow.wasm
WASM_SHA=$(sha256sum "$WASM" | cut -d' ' -f1)

echo "uploading wasm ($WASM_SHA)"
WASM_HASH=$(stellar contract upload --wasm "$WASM" --source-account "$STELLAR_SOURCE_ALIAS" --network "$NETWORK" 2>>"$LOG")

echo "deploying with admin $ADMIN"
CONTRACT_ID=$(stellar contract deploy --wasm-hash "$WASM_HASH" --source-account "$STELLAR_SOURCE_ALIAS" \
  --network "$NETWORK" -- --admin "$ADMIN" 2>"$LOG.deploy")
cat "$LOG.deploy" >&2
# stellar-cli logs the submitted transaction hash on stderr.
DEPLOY_TX=$(grep -oE '\b[0-9a-f]{64}\b' "$LOG.deploy" | tail -1 || true)
rm -f "$LOG.deploy"
[ -n "$DEPLOY_TX" ] || { echo "could not read the deploy transaction hash from stellar-cli output; fill deployTx by hand" >&2; }

DEPLOY_LEDGER=null
if [ -n "$DEPLOY_TX" ]; then
  DEPLOY_LEDGER=$(curl -fsS "$RPC" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":{\"hash\":\"$DEPLOY_TX\"}}" | jq '.result.ledger // null')
fi

ABI=$(stellar contract invoke --id "$CONTRACT_ID" --source-account "$STELLAR_SOURCE_ALIAS" --network "$NETWORK" -- abi_version)
[ "$ABI" = "2" ] || { echo "deployed contract reports abi_version=$ABI, expected 2" >&2; exit 1; }

jq -n \
  --arg contractId "$CONTRACT_ID" --arg wasmHash "$WASM_HASH" --arg wasmSha256 "$WASM_SHA" \
  --arg admin "$ADMIN" --arg gitSha "$(git rev-parse HEAD)" --arg deployTx "$DEPLOY_TX" \
  --argjson deployLedger "$DEPLOY_LEDGER" --arg cli "$(stellar --version | head -1)" \
  --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg pass "$PASSPHRASE" '{
    network: "testnet", networkPassphrase: $pass, contractId: $contractId, wasmHash: $wasmHash,
    wasmSha256: $wasmSha256, abiVersion: 2, admin: $admin, gitSha: $gitSha,
    deployTx: (if $deployTx == "" then null else $deployTx end), deployLedger: $deployLedger,
    stellarCli: $cli, deployedAt: $at
  }' > deployments/testnet.json
cat deployments/testnet.json
cat <<MSG

Next:
  1. Commit deployments/testnet.json (it holds no secrets).
  2. torrent-indexer: TORRENT_ESCROW_CONTRACT_ID=$CONTRACT_ID CHAIN_START_LEDGER=$DEPLOY_LEDGER
  3. torrent-api:     TORRENT_ESCROW_CONTRACT_ID=$CONTRACT_ID
  4. Run ./scripts/smoke-testnet.sh to exercise every entrypoint with synthetic XLM.
MSG
