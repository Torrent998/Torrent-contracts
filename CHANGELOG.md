# Changelog
## Unreleased
- **ABI v2** (breaking; v1 was never deployed): `close_pool`, `remove_member` (before funding), `leave_pool`
  (refused while owning unclaimed rights; `Member.open_rights`), two-step controller and admin rotation,
  admin-only upgrades with a 7-day on-chain timelock plus `cancel_upgrade` and `lock_upgrades`, a constructor
  taking the admin, transfer timing rules (`RightMatured`, offers may not outlive maturity), explicit
  `transfer_cancelled` events for replaced and claim-voided offers, and the getters `get_member`,
  `get_pending_controller`, `get_governance`, `get_counts`. Ten new error codes (27-36). ADR 0002.
- Bindings moved to `bindings/*.v2.json`; the golden stream records every v2 event except `contract_upgraded`.
- Tests: 57 (new lifecycle and governance suites, including an upgrade to the real release wasm; property tests
  extended with close/leave/remove and `open_rights` accounting).
- CI publishes the release wasm with its sha256. `deploy-testnet.sh` and `smoke-testnet.sh` are complete (not run).
- README overhaul, security policy, code of conduct, issue forms, Dependabot and repo-specific contributor tasks.
- M0: ABI v1, event schema v1, synthetic scenario, ADR 0001.
- M1: `torrent-escrow` Soroban contract with unit, failure-path, event-golden and property tests.
