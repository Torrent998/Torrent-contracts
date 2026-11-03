<div align="center">

<img src="./brand/icon.svg" alt="Torrent logo" width="96" height="96" />

# Torrent-contracts

**Soroban escrow pools and fully backed payout rights for Torrent savings circles.**

[![CI](https://github.com/Torrent998/Torrent-contracts/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Torrent998/Torrent-contracts/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-42DEB4.svg)](./LICENSE)
[![Network: Stellar Testnet](https://img.shields.io/badge/network-Stellar%20Testnet-00191C.svg)](#status)
[![Status: prototype, unaudited](https://img.shields.io/badge/status-prototype%2C%20unaudited-A5F9DE.svg)](#status)

Part of **Torrent** — ownership of *when* money becomes available.

</div>

## This repository

`torrent-escrow`, a single Soroban contract that holds the circle's money and enforces every financial rule (ABI v2):

- **Pools and membership**: `create_pool` (bound to the circle's policy hash), `allow_member` / `remove_member`
  (controller, only before the first deposit), `join_pool` (member signs the policy hash), `leave_pool`
  (member's own signature, refused while they own an unclaimed right).
- **Escrow**: `deposit`, `withdraw_unreserved` (only what no right reserves), `pause_pool` / `unpause_pool`,
  `close_pool` (terminal: nothing new enters; claims and leftover withdrawals keep working).
- **Payout rights**: `issue_right` only against unreserved escrow; one owner at a time.
- **Transfers**: `propose_transfer` -> `accept_transfer` (both parties authorize) or `cancel_transfer`. Only
  before maturity, and an offer may not outlive the right's maturity.
- **Claims**: `claim` by the current owner, after maturity, exactly once.
- **Keys and upgrades**: two-step `propose_controller` / `accept_controller` per pool and `propose_admin` /
  `accept_admin` for the contract. The admin can only manage upgrades: `propose_upgrade` announces a wasm
  hash on-chain with a **7-day timelock**, then `execute_upgrade`, `cancel_upgrade` or `lock_upgrades`
  (freezes the code forever). The admin has no power over any pool, right or token.
- **Getters**: `get_pool`, `get_totals`, `get_unreserved`, `get_right`, `get_transfer`, `get_member`,
  `is_member`, `get_pending_controller`, `get_governance`, `get_counts`, `abi_version`.

The backing invariant `reserved <= funded - claimed - withdrawn` is asserted after every mutating call and checked
by property tests over random sequences of deposits, issues, transfers, claims, withdrawals, pauses, closes,
removals and departures. Fee-on-transfer tokens are rejected. Every mutating call uses Soroban authorization and
its unauthorized paths are tested. Every state change emits an event.

Public boundary, versioned (ABI v2): [`bindings/abi.v2.json`](./bindings/abi.v2.json) (functions, auth, errors,
upgrade policy), [`bindings/events.v2.json`](./bindings/events.v2.json) (event topics and data, consumed by
Torrent-indexer), [`bindings/scenario.v2.json`](./bindings/scenario.v2.json) and
[`bindings/stream.v2.json`](./bindings/stream.v2.json) (the real events emitted by that scenario, as base64 XDR).

## Quickstart

Requires Rust stable (1.85+).

```bash
git clone https://github.com/Torrent998/Torrent-contracts && cd Torrent-contracts
rustup target add wasm32v1-none
./scripts/check.sh            # fmt, clippy -D warnings, release wasm, all tests (incl. upgrade to that wasm)
./scripts/check-bindings.sh   # regenerates bindings/ from the real contract and fails on any drift
```

Nothing to shut down (no services). The release wasm lands in `target/wasm32v1-none/release/torrent_escrow.wasm`;
CI publishes it with its sha256 as the `torrent_escrow-wasm-<sha>` artifact. Testnet deployment and smoke steps are
in [deployments/README.md](./deployments/README.md).

## Status

| Area | State |
|---|---|
| `torrent-escrow` ABI v2: pools, membership (allow, join, remove before funding, leave), deposit, issue, transfer with timing rules, claim, withdraw-unreserved, pause, close, controller/admin rotation, timelocked upgrades and upgrade lock | implemented |
| Tests: unit and failure paths, per-signer authorization (controller, member, admin, successor), fee-on-transfer rejection, replay, event golden, ABI surface, property tests (backing invariant, ownership model, `open_rights` accounting, timing rules), upgrade to the real release wasm | 57 passing in the Soroban test host with a Stellar Asset Contract token |
| Bindings drift check (`scripts/check-bindings.sh`) | in CI |
| Release wasm (`cargo build --release --target wasm32v1-none`) | built in CI and published as an artifact with its sha256; executes in the Soroban host (the governance test upgrades to it). The contract spec is not shaken by plain cargo; `stellar contract build` (stellar-cli >= 25.2) gives the canonical artifact |
| Deployment scripts (`build.sh`, `deploy-testnet.sh`, `smoke-testnet.sh`) | complete, **never run** (no Testnet access from the authoring environment) |
| Testnet deployment / transaction hashes | **none yet** |
| Consent-based rule amendments | **not yet** |
| Security audit / Mainnet / real funds | **no** / **no** / **no** |

Design records: [ADR 0001](./docs/adr/0001-escrow-contract-design.md) (escrow design) and
[ADR 0002](./docs/adr/0002-abi-v2-lifecycle-and-governance.md) (ABI v2 lifecycle, timing rules, upgrade policy).

## What is Torrent?

In a savings circle, members contribute to a shared pot and take turns receiving it. Sometimes a member
needs their payout earlier, or would happily take it later. **Torrent lets members exchange the timing of a
payout, but only for payouts that are already fully funded.**

1. **Create a circle.** An organizer sets the rules; their hash binds the circle to an on-chain pool.
2. **Fund the pool.** Members deposit a supported Stellar asset into a Soroban escrow contract.
3. **Issue payout rights.** Each right has a unique id, owner, amount and maturity. The contract refuses to
   issue a right that is not covered by unreserved escrow.
4. **Transfer or claim.** The owner proposes, the recipient accepts, both sign. At maturity the current owner
   claims exactly once.

The contract enforces one invariant after every call: `reserved <= funded - claimed - withdrawn`.
A payout right never represents a future contribution that has not been paid.

## Architecture

Torrent is four independently tested repositories, each owning one boundary. The contract is the only source of truth for backing, ownership and payout; the indexer reconstructs that truth from the ledger; the API organizes people and signing requests; the web app shows it honestly.

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
| **[Torrent-contracts](https://github.com/Torrent998/Torrent-contracts)** (this repo) | Rust · Soroban SDK 29 | Escrow pools, fully backed payout rights, member-only transfers and one-time claims. The only writer of financial truth. |
| [Torrent-indexer](https://github.com/Torrent998/Torrent-indexer) | TypeScript · Node 22 · Stellar RPC | Ingests contract events with a durable cursor and rebuilds pools, rights and transfers from the ledger. |
| [Torrent-api](https://github.com/Torrent998/Torrent-api) | TypeScript · NestJS · SQLite | Circles, invitations, wallet sign-in and the signing-request state machine. Holds no keys and no balances. |
| [Torrent-web](https://github.com/Torrent998/Torrent-web) | TypeScript · Next.js 16 · Tailwind CSS 4 | Landing page and web app: circles, coverage, rights, transfers, claims and signing requests. |

## Project documents

| Document | What it covers |
|---|---|
| [docs/PRD.md](./docs/PRD.md) | Product purpose, users, MVP and non-goals |
| [docs/SRD.md](./docs/SRD.md) | Domain model, state machine, API surface |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | The four repositories and how data flows between them |
| [docs/STELLAR_PROTOCOL.md](./docs/STELLAR_PROTOCOL.md) | Contract specification and invariants |
| [docs/SECURITY.md](./docs/SECURITY.md) | Threat model and go-live restrictions |
| [docs/ROADMAP.md](./docs/ROADMAP.md) | Milestones and quality gates |
| [docs/BRAND.md](./docs/BRAND.md) | Approved logo and interface principles |
| [docs/adr/](./docs/adr/) | Architecture decision records for this repository |
| [AGENTS.md](./AGENTS.md) | Rules for AI coding agents working in this repository |

The shared product documents are mirrored in all four repositories; the canonical copy lives in
[Torrent-api](https://github.com/Torrent998/Torrent-api/tree/main/docs).

## Contributing and Drips Wave

Contributions are welcome. Start with [CONTRIBUTING.md](./CONTRIBUTING.md), then pick a scoped task from
[docs/CONTRIBUTOR_ISSUES.md](./docs/CONTRIBUTOR_ISSUES.md). Each task names its motivation, the files it touches,
acceptance tests and non-goals. Changes that cross a repository boundary go through the versioned fixtures
each repo publishes, so every PR stays reviewable on its own.

Torrent intends to apply to the [Drips Wave](https://docs.drips.network/wave/maintainers/participating-in-a-wave/)
program for Stellar projects. Participation is subject to Drips' review; nothing here implies approval or rewards.
Please follow the [Code of Conduct](./CODE_OF_CONDUCT.md).

## Security

Torrent is an **unaudited prototype for Stellar Testnet**. Do not use it with real funds.
Please report vulnerabilities privately as described in [SECURITY.md](./SECURITY.md); do not open public issues
for them. The threat model is in [docs/SECURITY.md](./docs/SECURITY.md).

## License

[MIT](./LICENSE). The Torrent logo in [`brand/`](./brand/) is the project's approved identity and is shared by all four
repositories; see [brand/README.md](./brand/README.md) for usage rules.
