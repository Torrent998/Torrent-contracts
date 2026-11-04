# Torrent: checklist for the owner

Written 2026-11-04 after running the local continuation runbook on the owner's machine. Everything is **Stellar Testnet,
synthetic funds, no audit**. Items below are not ticked for you.

## Status

| Phase | State | Evidence |
|---|---|---|
| 0. Baseline checks (all 4 repos) | Done | `scripts/check.sh` + `check-bindings.sh` (contracts); `npm run check` (indexer, api, web) all green on this machine after Windows-only fixes |
| 1. Deploy contract to Testnet | Done | [`CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG`](https://stellar.expert/explorer/testnet/contract/CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG), deploy ledger 5553711, `deployments/testnet.json` |
| 1. Smoke lifecycle on Testnet | Done | `deployments/testnet-smoke.json`: 12 transactions (all checked `successful` on Horizon) and 7 expected contract refusals |
| 1. Wasm reproducibility | Done, with a caveat | `stellar contract build` gives the deployed sha256 `9dd206c4…f264`; the CI artifact (plain cargo) differs |
| 2. Indexer on the live contract | Done | status `SYNCED`, no faults; totals equal `get_totals` read from the chain; every event kind ingested |
| 2. API on the live contract | Done for deposit and organizer flows | `deployments/testnet-api-e2e.json`, `testnet-api-organizer-e2e.json` (6 transactions verified) |
| 2. Web build against Testnet env | Done | `npm run build` passes |
| 3. Real wallet (Freighter) | **Not done, needs you** | `Torrent-web/docs/FREIGHTER_WALKTHROUGH.md` |
| 4. Organizer signing requests in the API | Done (create pool, issue right, close pool) | `Torrent-api` CHANGELOG, 6 tests, live run |
| 4. Constructor event for the admin | **Not done** | needs a contract change, redeploy and bindings re-mirrored; indexer shows `admin: null` until a rotation |
| 4. Consent-based rule amendments | **Not done** | large contract change with an ADR and an ABI bump |
| 4. Playwright end-to-end suite with a mocked wallet | **Not done** | |
| Final CI status on `main` | See section 1 | check the Actions tab of each repo |

## 1. What is proven and what is not

**Proven (I ran it and saw the output):**
- Contract deployed on Testnet and its `abi_version` reads 2. Full lifecycle on-chain: create pool, allow/join, deposit,
  issue right, transfer (two signatures), claim, leave, controller rotation, close, withdraw leftover; plus refusals:
  unbacked issue (#10), offer beyond maturity (#16), accept replay (#18), early claim (#12), leave while owning a right (#29),
  double claim (#13), deposit after close (#27). Details and links: `docs/DEPLOYMENT.md`.
- Indexer follows the live contract and agrees with it (totals, members, status).
- API: wallet-signature login, circle creation, binding a circle to an on-chain pool by policy hash, deposit and organizer
  signing requests that were signed as real prepared Soroban transactions and confirmed only after the indexer saw their
  events. An unbacked issue_right is refused before signing.
- Escalating checks: every repo's full test suite passes locally.

**Not proven:**
- No Freighter run by anyone. The adapter `@stellar/freighter-api` 6.0.1 has never been exercised.
- The API's transfer (propose/approve/accept) and claim intents were not run live, only in-process tests.
- Joining a pool has no API request; no web buttons for create pool, issue right, close pool.
- Reconciliation drift detection was not forced live.
- **No security audit. Testnet only. Not production-ready.**

**CI:** I could not rely on a local run for this. Open each repo's Actions tab (`Torrent998/Torrent-contracts`, `-indexer`,
`-api`, `-web`) and confirm the latest run on `main` is green. If red, send the log text back (hand-back, section 10).

**Wasm caveat:** CI publishes a plain-cargo wasm whose sha256 (`6768a12e…d3f7` when built here) differs from the deployed
`stellar contract build` artifact (`9dd206c4…f264`). Moving CI to a pinned stellar-cli is a listed contributor task.

## 2. Releases (owner, GitHub UI)

Torrent has no mobile app, so no APK releases. If you want a tagged release of any repo: Releases -> Draft a new release ->
type a **lowercase** `vX.Y.Z` -> "Create new tag on publish" -> target `main` -> Publish. Only do this if the repo has a
release workflow you want to trigger.

## 3. Android signing

Not applicable (no mobile app).

## 4. Vercel (landing page and app)

1. vercel.com -> sign in with GitHub, allow the `Torrent998` org -> Add New -> Project -> import `Torrent998/Torrent-web`.
2. Root Directory: leave blank. Framework: Next.js.
3. Production environment variables: `NEXT_PUBLIC_API_BASE_URL` = the public URL of the hosted API (section 5),
   `NEXT_PUBLIC_STELLAR_NETWORK=testnet`, `NEXT_PUBLIC_STELLAR_RPC_URL=https://soroban-testnet.stellar.org`.
4. Deploy, open the live URL, click every link. Put the URL in the repo's Website field and the org profile.
   Docs: https://vercel.com/docs/monorepos

## 5. Backends (indexer and API)

Both are long-running Node services with SQLite files. Create one Render, Railway or Fly service each with a persistent disk.
- `Torrent-indexer`: `npm ci && npm run build`, start `npm start`. Env: `TORRENT_ESCROW_CONTRACT_ID=CBWF73FUJDGXSXOYUVYHP3XAFSPZWX65YGFSXDSSF5VRFLEWFFTQUIJG`,
  `CHAIN_START_LEDGER=5553711`, `STELLAR_RPC_URL=https://soroban-testnet.stellar.org`, `DATABASE_PATH` on the persistent disk, `PORT` as the host requires.
- `Torrent-api`: same build/start. Env: `TORRENT_ESCROW_CONTRACT_ID` (same), `INDEXER_BASE_URL` = the indexer's URL,
  `DATABASE_PATH` on the persistent disk, `STELLAR_RPC_URL`. Check `GET /v1/contract` on the API and `GET /v1/status` on the indexer.
- Then redeploy the landing page with the final API URL. Type secrets (if any) only into the hosting dashboard yourself;
  none of these services needs a secret key.

## 6. GitHub org and repo settings

- Org `Torrent998`: upload `brand/icon-512.png` (or the avatar file your brand folder names) as the avatar; create a public
  `.github` repo with `profile/README.md` copied from `brand/ORG_PROFILE.md`.
- Each repo: Settings -> Code security -> enable private vulnerability reporting, Dependabot alerts and secret scanning;
  add topics (`stellar`, `soroban`, `drips-wave`, `rotating-savings`); add the website URL; pin the main repos.

## 7. Drips Wave application

1. Read the current steps at https://www.drips.network/wave.
2. Apply with `Torrent-contracts` and `Torrent-web` (plus indexer/api if allowed).
3. Create real GitHub issues from each repo's `docs/CONTRIBUTOR_ISSUES.md` (scope, acceptance test, labels per Wave's docs).
   Item 1 of the contracts list (first Testnet deployment) is **done**: do not reopen it as work.
4. Complete any identity/KYC Drips asks for. Describe the project truthfully using section 1 (Testnet, no audit, wallet untested).

## 8. Store listings

Not applicable (no mobile app).

## 9. Before Mainnet (not part of this application)

Independent security audit, bug bounty, legal review, key-management plan, a real asset decision. Do not call the project
production-ready before this.

## 10. Hand-back

Do the Freighter walkthrough (`Torrent-web/docs/FREIGHTER_WALKTHROUGH.md`), then if anything in 1 to 9 fails, send the error
text (never keys or seed phrases) to a coding agent in this workspace and tell it to read `AGENTS.md`.
