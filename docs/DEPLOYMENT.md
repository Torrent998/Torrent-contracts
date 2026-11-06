# Torrent â€” Environment, Deployment and Recovery

## Environment boundaries
- Local: deterministic Docker Compose services, Stellar local network/Testnet, synthetic users/fixtures.
- Staging: separate database, bucket, API credentials, signing identities, Testnet assets/contracts, observability.
- Production: requires security audit, real issuer/provider agreements and relevant jurisdictional legal/compliance review. Not included in this Testnet prototype.

## Suggested variables (non-secret examples only)
```dotenv
APP_ENV=development
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
HORIZON_URL=https://horizon-testnet.stellar.org
DATABASE_URL=postgresql://localhost:5432/torrent
REDIS_URL=redis://localhost:6379
API_BASE_URL=http://localhost:3001
CHAIN_START_LEDGER=0
LOG_LEVEL=info
```
- Credentials to sign on-chain go into a secure secret manager or hardware-backed user wallet, never into a public mobile/web bundle.
- Signed store badge links are configured after actual publication; no fabricated App Store or Google Play app URLs.

## CI & GitHub setup
- GitHub Actions per repo: formatting/lint, schema checks, unit tests, dependency scanning; chain/devices smoke tests where supported.
- Branch protection and PR review recommended.
- Release tag points to tested sibling versions in an integration lockfile.
- All paths use offline/synthetic test fixtures, not real user financial records.

## Failure recovery
- Rebuild indexers from ledger cursor; compare DB projection vs chain truth.
- Restore DB backups after verifying transaction-intent reconciliation.
- Never republish user transactions solely because a client request timed out.
- Test signing-key rotation, dependency outages, stale quotes, malicious user input, insufficient balances and duplicate delivery.
- External reference infrastructure may change; check upstream project versions and licenses at build time.

## Testnet deployment and smoke run (2026-11-04)

Testnet only, synthetic funds. Raw evidence: `deployments/testnet.json` and `deployments/testnet-smoke.json`.
No audit has been done. Nothing here is a Mainnet statement.

| Item | Value |
|---|---|
| Contract id | [`CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG`](https://stellar.expert/explorer/testnet/contract/CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG) |
| Wasm sha256 (= wasm hash) | `9dd206c4b7e4f673dd6006da9c45efe3d8b0ad5e1155d8e3b957f855f330f264` |
| ABI version read back from the contract | 2 |
| Deploy transaction | [653b9c6d02…](https://stellar.expert/explorer/testnet/tx/653b9c6d026720e5c8f389e0347b21a7d369706f19baf9313f0a4947c4b2ac0f) (ledger 5553711) |
| Admin (public address) | `GAE3F3TYRELLL34IM2N74QG3PXWZUYDMCKBTPDWPN7ZNHBVTXWNC7ZNT` |
| Source commit | `68c20f4fe041d8f00ef2112080235a9840841c55` |
| Tool | stellar 27.1.0 (8e402ea28202950b272fbabc34caad4d2f64fe87) |

**Reproducible build.** `stellar contract build` on this checkout gives the same sha256 as the deployed wasm
(`9dd206c4b7e4f673dd6006da9c45efe3d8b0ad5e1155d8e3b957f855f330f264`). The CI artifact is built with plain `cargo` (`scripts/build-wasm.sh`, spec not shaken) and has a
different sha256 (`6768a12e4127a9c1c9344a259378ebc99becb365dd29b12d0c564b324263d3f7` when built here). The deployed
artifact is the `stellar contract build` one. Moving CI to a pinned stellar-cli is listed in the product notes in the git history.

### Smoke run (`scripts/smoke-testnet.sh`, pool 8, token = native XLM Stellar Asset Contract `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`)

All steps passed. Every transaction hash below was checked as `successful` on Horizon. Refusals are contract errors
returned by simulation (no transaction), matched by error code from `contracts/escrow/src/types.rs`.

| Step | Result | Transaction |
|---|---|---|
| `allow_alice` | ok | [db13e59bc5…](https://stellar.expert/explorer/testnet/tx/db13e59bc5535c6a39331ddca1a13e1a893777ceb268e5db00a2f2bb7b0244f5) |
| `join_alice` | ok | [0e7eb5cfef…](https://stellar.expert/explorer/testnet/tx/0e7eb5cfef996d3ea935a9cffd69fe9db84558da7ec3c6b4f2138c92613daf62) |
| `allow_bob` | ok | [a4236747ba…](https://stellar.expert/explorer/testnet/tx/a4236747bafa4c2697cc69d16ed3c8c05d95a303b4facd410aa830a42e7e814b) |
| `join_bob` | ok | [ed503a60b0…](https://stellar.expert/explorer/testnet/tx/ed503a60b03e9d91f920b75c0f85724aec5a7fd3c4408201f7f1a96d4bb004b0) |
| `deposit` | ok | [5198d1687d…](https://stellar.expert/explorer/testnet/tx/5198d1687d4adf73c9b4a57fcfc94b975753392fe0f45bea7b0fbb89a7f43c6c) |
| `issue_unbacked` | refused: InsufficientBacking (#10) | n/a (simulation) |
| `offer_beyond_maturity` | refused: InvalidExpiry (#16) | n/a (simulation) |
| `accept_transfer` | ok | [cc55f49797…](https://stellar.expert/explorer/testnet/tx/cc55f49797c281273fdf1d788b2022799ceb93f376ff53d856141b500060a1ba) |
| `accept_replay` | refused: TransferNotPending (#18) | n/a (simulation) |
| `claim_early` | refused: NotMatured (#12) | n/a (simulation) |
| `leave_while_owning` | refused: MemberHasRights (#29) | n/a (simulation) |
| `claim` | ok | [d00b466507…](https://stellar.expert/explorer/testnet/tx/d00b4665072131b1eeb193bf0cdd819121de66a68bfb269586c05e961e8e15af) |
| `double_claim` | refused: AlreadyClaimed (#13) | n/a (simulation) |
| `leave_bob` | ok | [2298f0e919…](https://stellar.expert/explorer/testnet/tx/2298f0e919952163b1d749fa7191424bd74e83819a13fb6a8cdd13de6ed3d303) |
| `propose_controller` | ok | [76c38ba9c6…](https://stellar.expert/explorer/testnet/tx/76c38ba9c6af2cdf20230d25dc3d53764317de991739b1cc3cc285792de8337a) |
| `accept_controller` | ok | [e3807cc871…](https://stellar.expert/explorer/testnet/tx/e3807cc87198bf7f9ebd23ccf8efbbfa4fec6e10da24c20d108b86817a1739a3) |
| `close_pool` | ok | [f7b754dafa…](https://stellar.expert/explorer/testnet/tx/f7b754dafac53ac588d04f6630fd74c58b3fc8901bb66c672503d44e30b91d50) |
| `deposit_after_close` | refused: PoolClosed (#27) | n/a (simulation) |
| `withdraw_leftover` | ok | [e3240d5862…](https://stellar.expert/explorer/testnet/tx/e3240d58628cfe3960277b6863fa05987bf85ebe16abd1ff9fac7d4fa45b112f) |

Final on-chain totals for the pool: claimed 6000000, funded 10000000, reserved 0, withdrawn 4000000.

### Problems found and fixed in the scripts by the first real run
1. The refusal check grepped for error *names* but stellar-cli prints numeric codes (`Error(Contract, #10)`); names are now resolved through the contract's own enum.
2. Transient RPC transport errors (`client error (SendRequest)`) aborted runs; the invoke helper now retries transport errors only, never contract refusals.
3. The 120 s offer / 180 s maturity windows were shorter than a slow link needs to land a transaction (the contract correctly rejected the late offer with `InvalidExpiry`); the default is now 420 s (`SMOKE_MATURITY_SECS`).
4. Transaction hashes were taken from any 64-hex string in the CLI output and so recorded a wrong id for most steps; they now come from the explorer link (`/tx/<hash>`). The first run's evidence was discarded and the script rerun; only the rerun is recorded here.
Earlier failed runs left extra pools on Testnet; they are not part of this record.
