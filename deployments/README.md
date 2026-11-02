# Deployments

**No deployments exist.** M1 was proven inside the Soroban test host only (see README status).
When a Testnet deployment is made, record it in `testnet.json` as
`{ "network", "networkPassphrase", "contractId", "wasmHash", "deployTx", "deployLedger", "abiVersion", "gitSha" }`
so `torrent-indexer` can set its start ledger. Deploy with a deployable build:

```bash
stellar contract build            # stellar-cli >= 25.2.0 (spec shaking); not available in the authoring sandbox
stellar contract deploy --wasm target/wasm32v1-none/release/torrent_escrow.wasm \
  --source-account "$STELLAR_SOURCE_ALIAS" --network testnet
```
Never commit keys, seeds or `.env`.
