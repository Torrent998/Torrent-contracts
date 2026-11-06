<div align="center">

<img src="./brand/icon.svg" alt="Torrent logo" width="96" height="96" />

# Torrent-contracts

**Soroban escrow for savings circles: every payout right is fully funded before it exists.**

[![CI](https://github.com/Torrent998/Torrent-contracts/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Torrent998/Torrent-contracts/actions/workflows/ci.yml)
[![Docs](https://github.com/Torrent998/Torrent-contracts/actions/workflows/docs.yml/badge.svg?branch=main)](https://github.com/Torrent998/Torrent-contracts/actions/workflows/docs.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-42DEB4.svg)](./LICENSE)
[![Network: Stellar Testnet](https://img.shields.io/badge/network-Stellar%20Testnet-00191C.svg)](#status)
[![Status: prototype, unaudited](https://img.shields.io/badge/status-prototype%2C%20unaudited-A5F9DE.svg)](#security)
[![Rust](https://img.shields.io/badge/rust-soroban%20sdk%2029-orange.svg)](./Cargo.toml)

[Live app](https://torrent-web1.vercel.app) · [Docs](https://github.com/Torrent998/Torrent-web/tree/main/gitbook) · [Quick start](#quick-start) · [Contract on Testnet](https://stellar.expert/explorer/testnet/contract/CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG) · [Architecture](./docs/ARCHITECTURE.md) · [Contributing](./CONTRIBUTING.md)

</div>

---

## What this is

In a savings circle, members pay into a shared pot and take turns receiving it. Sometimes a member needs their payout early, or would take it later.
Torrent lets members trade the timing of a payout, but only for payouts that are already fully funded. This repository holds the Soroban contract that
enforces that rule. It is the only writer of financial truth in Torrent: it holds the money, issues payout rights against money that is really there, moves
rights between members with both signatures, and pays each right out exactly once.

The contract checks one invariant after every state change: `reserved <= funded - claimed - withdrawn`. A payout right never stands for a contribution that has not been paid.

## Status

Torrent is a **Testnet prototype with synthetic funds. It has not been audited.**

| Area | State |
|---|---|
| `torrent-escrow` ABI v2: pools, membership, deposit, issue, transfer with timing rules, claim, withdraw-unreserved, pause, close, controller and admin rotation, timelocked upgrades | implemented |
| Tests | 57 passing in the Soroban test host (unit, failure paths, per-signer authorization, fee-on-transfer rejection, replay, event golden, ABI surface, property tests, upgrade to the real release wasm) |
| Deployment | live on Stellar Testnet: [`CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG`](https://stellar.expert/explorer/testnet/contract/CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG), ABI v2, deployed in ledger 5553711 ([tx](https://stellar.expert/explorer/testnet/tx/653b9c6d026720e5c8f389e0347b21a7d369706f19baf9313f0a4947c4b2ac0f)) |
| Wasm | sha256 `9dd206c4b7e4f673dd6006da9c45efe3d8b0ad5e1155d8e3b957f855f330f264`, built with `stellar contract build` (CI builds with plain cargo and gets a different, unshaken artifact) |
| Smoke lifecycle on Testnet | create pool, join, deposit, issue, transfer, claim, leave, rotate controller, close, withdraw: [`deployments/testnet-smoke.json`](./deployments/testnet-smoke.json) records every transaction hash and the contract refusals (`InsufficientBacking`, `InvalidExpiry`, `TransferNotPending`, `NotMatured`, `MemberHasRights`, `AlreadyClaimed`, `PoolClosed`) with their error codes |
| Full-stack runs | API deposit and organizer flows confirmed through the indexer: [`testnet-api-e2e.json`](./deployments/testnet-api-e2e.json), [`testnet-api-organizer-e2e.json`](./deployments/testnet-api-organizer-e2e.json) |
| Constructor event for the admin | **not done**: the admin reads `null` in the indexer until the first admin rotation |
| Consent-based rule amendments | **not done** |
| Security audit, Mainnet, real funds | **no**, **no**, **no** |

The contract hash and transaction hashes above were checked on Horizon on 2026-11-06.

## How it fits together

Torrent is four repositories, each owning one boundary. The contract is the only source of truth for backing, ownership and payout. The indexer rebuilds that
truth from ledger events. The API organizes people and signing requests. The web app shows it and asks the member's wallet to sign.

```mermaid
flowchart LR
  U(["Member wallet"]) -.->|signs each call| W
  W["<b>Torrent-web</b><br/>Next.js app"] -->|HTTPS, OpenAPI v1| A["<b>Torrent-api</b><br/>circles, invitations, signing requests"]
  A -->|ledger read models| I["<b>Torrent-indexer</b><br/>ledger-derived state"]
  A -->|user-signed transactions| C["<b>Torrent-contracts</b><br/>Soroban escrow on Stellar"]
  C -->|contract events via RPC| I
  classDef here fill:#42DEB4,stroke:#00191C,color:#00191C
  class C here
```

| Repository | Stack | Owns |
|---|---|---|
| **[Torrent-contracts](https://github.com/Torrent998/Torrent-contracts)** (this repo) | Rust, Soroban SDK 29 | Escrow pools, fully backed payout rights, member-only transfers, one-time claims |
| [Torrent-indexer](https://github.com/Torrent998/Torrent-indexer) | TypeScript, Node 22, Stellar RPC | Event ingestion with a durable cursor and read models of pools, rights and transfers |
| [Torrent-api](https://github.com/Torrent998/Torrent-api) | TypeScript, NestJS, SQLite | Circles, invitations, wallet sign-in, the signing-request state machine |
| [Torrent-web](https://github.com/Torrent998/Torrent-web) | TypeScript, Next.js 16, Tailwind CSS 4 | Landing page and app, Freighter wallet adapter |

## The contract

`torrent-escrow` (ABI v2):

- **Pools and membership:** `create_pool` (bound to the circle's policy hash), `allow_member` / `remove_member` (controller, only before the first deposit),
  `join_pool` (member signs the policy hash), `leave_pool` (refused while the member owns an unclaimed right).
- **Escrow:** `deposit`, `withdraw_unreserved` (only what no right reserves), `pause_pool` / `unpause_pool`, `close_pool` (terminal: nothing new enters, claims and
  leftover withdrawals keep working).
- **Payout rights:** `issue_right`, only against unreserved escrow. One owner at a time.
- **Transfers:** `propose_transfer`, then `accept_transfer` (both parties authorize) or `cancel_transfer`. Only before maturity, and an offer cannot outlive the right's maturity.
- **Claims:** `claim` by the current owner, after maturity, exactly once.
- **Keys and upgrades:** two-step `propose_controller` / `accept_controller` per pool and `propose_admin` / `accept_admin` for the contract. The admin can only manage
  upgrades: `propose_upgrade` announces a wasm hash on-chain with a 7-day timelock, then `execute_upgrade`, `cancel_upgrade` or `lock_upgrades` (freezes the code forever).
  The admin has no power over any pool, right or token.
- **Getters:** `get_pool`, `get_totals`, `get_unreserved`, `get_right`, `get_transfer`, `get_member`, `is_member`, `get_pending_controller`, `get_governance`,
  `get_counts`, `abi_version`.

Fee-on-transfer tokens are rejected. Every mutating call uses Soroban authorization, its unauthorized paths are tested, and every state change emits an event.
The versioned public boundary is in [`bindings/`](./bindings/): [`abi.v2.json`](./bindings/abi.v2.json), [`events.v2.json`](./bindings/events.v2.json),
[`scenario.v2.json`](./bindings/scenario.v2.json) and [`stream.v2.json`](./bindings/stream.v2.json) (the real events of that scenario as base64 XDR, consumed by Torrent-indexer).

## Tech stack

| Layer | Tools |
|---|---|
| Contract | Rust (stable), `soroban-sdk` 29, target `wasm32v1-none` |
| Build and deploy | `stellar contract build`, stellar-cli 25.2 or newer, bash scripts in [`scripts/`](./scripts/) |
| Tests | Soroban test host, property tests over random call sequences, golden event stream |
| CI | GitHub Actions: fmt, clippy `-D warnings`, release wasm, all tests, bindings drift check |

## Quick start

Requires Rust stable and `rustup target add wasm32v1-none`.

```bash
git clone https://github.com/Torrent998/Torrent-contracts && cd Torrent-contracts
rustup target add wasm32v1-none
./scripts/check.sh            # fmt, clippy -D warnings, release wasm, all tests (including the upgrade to that wasm)
./scripts/check-bindings.sh   # regenerates bindings/ from the real contract and fails on any drift
```

The release wasm lands in `target/wasm32v1-none/release/torrent_escrow.wasm`. To deploy and smoke-test on Testnet (stellar-cli 25.2 or newer, `jq` and `curl`):

```bash
stellar keys generate --fund torrent-deployer --network testnet
STELLAR_SOURCE_ALIAS=torrent-deployer ./scripts/deploy-testnet.sh   # build, upload, deploy, verify abi_version = 2
./scripts/smoke-testnet.sh                                          # full lifecycle, writes deployments/testnet-smoke.json
```

Keys stay in your local stellar-cli keystore. Nothing secret is read from or written to this repository. See [deployments/README.md](./deployments/README.md) for the output files.

## Project structure

```text
contracts/escrow/     the torrent-escrow contract: src/ and tests/ (abi_surface, authz, escrow_flow, events, governance, lifecycle, properties)
bindings/             versioned ABI, event schema, scenario and recorded event stream (ABI v2)
deployments/          Testnet deployment record, smoke run and full-stack run evidence
scripts/              check.sh, check-bindings.sh, build.sh, deploy-testnet.sh, smoke-testnet.sh, check-docs.mjs
docs/                 architecture, contract specification, security model, deployment notes, ADRs
```

Design records: [ADR 0001](./docs/adr/0001-escrow-contract-design.md) (escrow design) and [ADR 0002](./docs/adr/0002-abi-v2-lifecycle-and-governance.md) (ABI v2 lifecycle, timing rules, upgrade policy).
Contract specification and invariants: [docs/STELLAR_PROTOCOL.md](./docs/STELLAR_PROTOCOL.md).

## Contributing

Contributions are welcome and Torrent takes part in the Stellar Wave program on Drips. Open, scoped tasks carry the
[`Stellar Wave`](https://github.com/Torrent998/Torrent-contracts/issues?q=is%3Aissue+is%3Aopen+label%3A%22Stellar+Wave%22) label and one complexity label
(`trivial`, `medium` or `high`). Read [CONTRIBUTING.md](./CONTRIBUTING.md) first: it covers the check command, commit style, how to claim an issue and what reviewers look for.
Changes to the ABI, event schema or fixtures must be versioned and described for the sibling repositories.

## Security

Unaudited prototype, Testnet only, synthetic funds only. Report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/Torrent998/Torrent-contracts/security/advisories/new) as described in [SECURITY.md](./SECURITY.md).
The threat model is in [docs/SECURITY.md](./docs/SECURITY.md).

## Community

Questions and design discussion: [GitHub Discussions](https://github.com/Torrent998/Torrent-contracts/discussions).
<!-- TODO(owner): add the Telegram or Discord invite link here -->

## Maintainers

| Maintainer | Role | Link |
|---|---|---|
| Cjay | Maintainer, all four Torrent repositories | [@Cjay-Cyber-2](https://github.com/Cjay-Cyber-2) |

Organization: [Torrent998](https://github.com/Torrent998).

## Contributors

<a href="https://github.com/Torrent998/Torrent-contracts/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=Torrent998/Torrent-contracts" alt="Contributors" />
</a>

## License

[MIT](./LICENSE)
