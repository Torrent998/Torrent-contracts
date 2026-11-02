<img src="./brand/icon.svg" alt="Torrent logo" width="144" />

# torrent-contracts

> Torrent — Ownership of when money becomes available.

**Status: M0 + M1 complete in the Soroban test host; NOT deployed, NOT audited.** This repo is one of **exactly four meaningful independent codebases** in the Torrent Stellar-native project. It is not a placeholder created merely to increase Drips Wave submissions.

## Honest status
| Item | State |
|---|---|
| `torrent-escrow` contract (pools, membership, deposit, issue, transfer, claim, withdraw-unreserved, pause) | implemented |
| Tests: unit + failure paths, event golden, ABI surface, property test of the backing invariant | passing (`./scripts/check.sh`) |
| Wasm compile smoke check (`wasm32v1-none`) | passes, but is **not** a deployable build (needs `stellar contract build`, stellar-cli >= 25.2) |
| Testnet deployment / transaction hashes | **none** (no network or stellar-cli in the authoring sandbox) |
| Security audit, Mainnet, real funds | **no** / **no** / **no** |

All tests run against the real contract in the Soroban test host with a Stellar Asset Contract token and synthetic accounts. Nothing is mocked inside the contract.

## Public boundary (versioned)
`bindings/abi.v1.json` (functions, auth, error codes), `bindings/events.v1.json` (event topics/data, shared with `torrent-indexer`), `bindings/scenario.v1.json` (synthetic script). ABI version 1. Design record: `docs/adr/0001-escrow-contract-design.md`.

Core invariant: `reserved <= funded - claimed - withdrawn` after every call; rights can only be issued against escrowed tokens; old owners cannot claim after a transfer; nothing is claimable before maturity or twice.

## Local startup
```bash
rustup target add wasm32v1-none
./scripts/check.sh      # fmt, clippy -D warnings, tests, wasm smoke build
cargo test --all        # tests only
```
Nothing to shut down (no services). Deployment steps: `deployments/README.md`.

## Owns
Escrow of backed assets, immutable fully-funded payout rights, transfer restrictions, schedule enforcement, claims, authorized amendments

## Why it is separate
It has a distinct runtime/toolchain, external interface, tests, release cadence and maintainership boundary. Its siblings depend on its delivered interface, but implementation responsibilities must not be duplicated.

## Sibling repositories
- `torrent-web` — Savings-circle dashboard, governance and identity UX, fully-funded payout inventory, swap/transfer requests, signing and reporting
- `torrent-api` — Authenticated groups, enrollment, policy state, off-chain agreements, transaction intent orchestration, RBAC, compliance and notifications
- `torrent-indexer` — Soroban event ingestion, canonical ledger-derived read models, reconciliation, replay/backfill, settlement status and event API

## Architecture and development context
Start with:
- `AGENTS.md` — coding agent rules;
- `docs/PRD.md` — product purpose and MVP;
- `docs/SRD.md` — user/system requirements, APIs and state;
- `docs/ARCHITECTURE.md` — four-repository responsibilities, data and execution paths;
- `docs/TECHNICAL_BUILD.md` — recommended bootstrap, testing, phases;
- `docs/SECURITY.md` — security/trust boundaries;
- `docs/BRAND.md` — approved flat logo and flexible UI direction;
- `docs/ROADMAP.md` — milestone acceptance gates.

## Technical target
Rust/Soroban. The actual product implementation starts at milestone M0; no real-money transactions or claims exist in this scaffold.

## Minimal integration contract
Always test the dependency path stated in ARCHITECTURE.md. Every new public endpoint/ABI/package message must be versioned and covered by consumer fixtures. See `docs/SRD.md` and `docs/TECHNICAL_BUILD.md`.

## Drips Wave
Stellar is essential: Soroban escrow contract takes custody of already-funded supported Stellar assets. Approval/eligibility for Wave depends on substantive open-source implementation and organizer review, not repo count.


## Official project identity

The project uses the **approved flat SVG logo** in [`brand/icon.svg`](./brand/icon.svg), with an editable transparent vector mark in [`brand/mark.svg`](./brand/mark.svg). The [512 px GitHub organization avatar](./brand/org-avatar.png) is in the same directory. This logo is shared by all four project repositories. UI design remains intentionally flexible.
