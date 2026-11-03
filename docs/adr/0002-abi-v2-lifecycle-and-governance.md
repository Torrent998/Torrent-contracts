# ADR 0002 — ABI v2: pool lifecycle, timing rules, governance and upgrade policy

Status: accepted. Supersedes the "Not done" list of ADR 0001 where noted. v1 was never deployed,
so v2 replaces it outright (bindings moved to `bindings/*.v2.json`).

## Context
An audit of `torrent-escrow` v1 against `docs/STELLAR_PROTOCOL.md` and `docs/SRD.md` found these
gaps: no way to finish a pool, no way to correct membership before funding or to leave after it,
no rotation of the controller key, no upgrade policy, transfer offers that could outlive the right
they offer, and two silent transfer status changes the indexer had to infer.

## Decisions
1. **Close, not delete.** `close_pool` (controller; from Active or Paused) is terminal. Afterwards no
   deposits, allow-list entries, joins, issuance, proposals or acceptances. Claims, cancellations,
   leaving and `withdraw_unreserved` keep working, so closing never traps or confiscates funds.
   A pool is *settled* when it is closed and `funded - claimed - withdrawn == 0`; that is derived, not stored.
2. **Membership changes respect consent.** `remove_member` (controller) only before the first deposit,
   the same window as `allow_member`. After funding, only the member can change their membership:
   `leave_pool` (member signature), refused with `MemberHasRights` while they own an unclaimed right.
   `Member.open_rights` is maintained on issue, accept and claim; a property test checks it equals the
   number of unclaimed rights each member owns. Deposits of a departed member stay in escrow.
3. **Two-step key rotation.** `propose_controller`/`accept_controller` per pool and
   `propose_admin`/`accept_admin` for the contract. The successor must sign, so a typo cannot hand
   control to an address nobody holds.
4. **Upgrade policy.** The constructor takes an `admin`. The admin can only rotate itself and manage
   upgrades: `propose_upgrade(wasm_hash)` announces the hash on-chain (`upgrade_proposed` with
   `executable_after = now + 7 days`), `cancel_upgrade`, `execute_upgrade` (only after the timelock),
   and `lock_upgrades`, which freezes the code forever. The admin cannot pause, close, withdraw or issue
   in any pool (tested). The week lets members and the indexer see an upgrade coming and claim or leave.
   This is disclosure, not a guarantee: an unaudited prototype must not hold real funds.
5. **Timing rules.** A right is tradable only before maturity (`RightMatured`), and an offer's expiry
   must be `> now` and `<= maturity` (`InvalidExpiry`). At maturity the right is for claiming only.
6. **Everything is evented.** v2 emits `transfer_cancelled` when an expired offer is replaced and when a
   claim voids an open offer (before `transfer_proposed` / `right_claimed` in the same transaction),
   plus `pool_closed`, `member_removed`, `member_left`, `controller_proposed`, `controller_changed`,
   `admin_proposed`, `admin_changed`, `upgrade_proposed`, `upgrade_cancelled`, `contract_upgraded`,
   `upgrades_locked`. Contract-level events have no id topic.
7. **Getters for reconciliation.** `get_member`, `get_pending_controller`, `get_governance`, `get_counts`.
8. **Nonce / replay (unchanged from ADR 0001 §6).** No `Nonce(address, seq)` table: every mutating call
   uses Soroban authorization, whose signed payloads carry a host-checked nonce and expiration ledger,
   and the application objects are single use (a transfer id completes or cancels once, a right is
   claimed once). Tests cover accept/cancel/claim replays. Revisit only if off-chain signed approvals
   (signatures relayed outside a transaction) are introduced.

## Consequences
- `abi_version()` returns 2; the indexer and API reject v1-shaped data.
- The golden stream (`bindings/stream.v2.json`) records every event except `contract_upgraded`, which
  needs real wasm bytes whose hash depends on the compiler. `tests/governance.rs` covers it by
  upgrading to this crate's own release wasm and checking state, ids and invariants survive.
- `scripts/check.sh` builds the release wasm before the tests; CI publishes it as an artifact.

## Not done
- Consent-based rule amendments (every member signs) remain open; see `docs/CONTRIBUTOR_ISSUES.md`.
- No Testnet deployment; no audit.
