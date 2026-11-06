# ADR 0001 — Single escrow contract, multi-pool, fail-closed accounting

Status: accepted (M1); extended by [ADR 0002](./0002-abi-v2-lifecycle-and-governance.md) (ABI v2). Sources: `docs/STELLAR_PROTOCOL.md` (contract spec), the product notes in the git history, the product notes in the git history. Where the master index and a specific doc differ, the specific doc wins; `torrent-api` holds the canonical docs copy.

## Decisions
1. One contract (`torrent-escrow`) hosts many pools keyed by `pool_id: u64`. Rights and transfers use global monotonic `u64` ids (opaque to clients).
2. **Invariant** `reserved <= funded - claimed - withdrawn` is checked inside `save_totals`, the only writer of totals. There is no code path that changes totals without it, and tokens move only after the check (checks-effects-interactions).
3. Deposits verify the contract's token balance delta equals the requested amount; fee-on-transfer or rebasing tokens are rejected (`DepositMismatch`).
4. Membership: controller `allow_member` (only before the first deposit) then member `join_pool` with the exact policy hash. After funding the allow-list is frozen, so rules cannot change unilaterally.
5. Transfers are two-party: owner proposes (auth owner), recipient accepts (auth recipient). One open proposal per right; ids are single use; acceptance re-checks owner/expiry/eligibility. Claiming voids any open proposal.
6. `Nonce(address, seq)` from the spec is **not** implemented: Soroban's built-in authorization binds signatures to the invocation and ledger-bounded nonces, and our single-use transfer ids provide application-level replay protection. Revisit if off-chain signed approvals are introduced.
7. Pause blocks deposit/issue/transfer/withdraw and membership changes; **claims always work** (no confiscation).
8. `withdraw_unreserved` lets the controller recover only value not backing any issued right.
9. Every event carries `network_id` (sha256 of the passphrase). Schema in `bindings/events.v2.json` (v1 at the time of this ADR).

## Not done / known limits
- Not audited. Testnet/synthetic only. The Testnet deployment and smoke run are recorded in `deployments/` and in the README status table.
- Single token per pool; no amendment flow (upgrade policy: ADR 0002); no per-pool transfer-policy enforcement beyond storing `transfer_policy_hash` on the right (the hash is opaque data for the API/web).
