# Deployments

**No deployments exist.** Everything so far was proven inside the Soroban test host (see the README status).
No file in this folder other than this README is real until someone runs the scripts below and commits their output.

| File | Written by | Holds |
|---|---|---|
| `testnet.json` | `scripts/deploy-testnet.sh` | `network`, `networkPassphrase`, `contractId`, `wasmHash`, `wasmSha256`, `abiVersion` (2), `admin`, `gitSha`, `deployTx`, `deployLedger`, `stellarCli`, `deployedAt` |
| `testnet-smoke.json` | `scripts/smoke-testnet.sh` | every smoke step with its transaction hash or the error it was expected to be refused with |

## Steps (Testnet only, synthetic funds)

Requires stellar-cli >= 25.2.0, `jq` and `curl`. Keys stay in your local stellar-cli keystore; nothing secret is
written here.

```bash
stellar keys generate --fund torrent-deployer --network testnet
STELLAR_SOURCE_ALIAS=torrent-deployer ./scripts/deploy-testnet.sh   # build, upload, deploy (--admin), verify abi_version=2
./scripts/smoke-testnet.sh                                           # full lifecycle against the native-XLM SAC
```

`deploy-testnet.sh` refuses to run if the RPC does not report the Testnet passphrase or if the contract sources have
uncommitted changes. Set `TORRENT_ADMIN_ADDRESS` to make someone other than the deployer the upgrade admin; that
address can only rotate itself and propose (7-day timelock), cancel, execute or permanently lock upgrades.

Then configure the siblings: `TORRENT_ESCROW_CONTRACT_ID` and `CHAIN_START_LEDGER` (= `deployLedger`) in
torrent-indexer, `TORRENT_ESCROW_CONTRACT_ID` in torrent-api.

Never commit keys, seeds or `.env`.
