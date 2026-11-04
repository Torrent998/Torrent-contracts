#!/usr/bin/env bash
# End-to-end smoke run against a deployed torrent-escrow on TESTNET with synthetic funds.
#
# STATUS: run for real on 2026-11-04 (see docs/DEPLOYMENT.md); first run exposed four script bugs, now fixed.
#
# Requirements: stellar-cli >= 25.2.0, jq, deployments/testnet.json from deploy-testnet.sh.
# The pool token is the Testnet native-XLM Stellar Asset Contract, so friendbot-funded identities
# can take part with no custom asset. Amounts are stroops (1 XLM = 10_000_000).
#
#   ./scripts/smoke-testnet.sh            # creates/funds identities torrent-{controller,alice,bob}
#
# What it proves on a real network: create_pool, allow/join, deposit, issue_right (backed),
# propose/accept transfer (two signatures), timing rules, claim at maturity, double-claim refused,
# leave_pool, controller rotation, close_pool, withdraw_unreserved of leftovers.
set -euo pipefail
cd "$(dirname "$0")/.."
for bin in stellar jq; do command -v "$bin" >/dev/null || { echo "missing: $bin" >&2; exit 1; }; done
[ -f deployments/testnet.json ] || { echo "run scripts/deploy-testnet.sh first" >&2; exit 1; }
CONTRACT_ID="${CONTRACT_ID:-$(jq -r .contractId deployments/testnet.json)}"
NETWORK=testnet
OUT=deployments/testnet-smoke.json
TOKEN_ID="${TOKEN_ID:-$(stellar contract id asset --asset native --network "$NETWORK")}"
POLICY=$(printf '07%.0s' $(seq 1 32))
STEPS='[]'

ident() { # alias -> address, generating and friendbot-funding it on first use
  stellar keys address "$1" 2>/dev/null || { stellar keys generate --fund "$1" --network "$NETWORK" >/dev/null; stellar keys address "$1"; }
}
inv() { # inv <signer-alias> <step-name> <fn> [args...]; prints the return value
  local who=$1 step=$2; shift 2
  local err; err=$(mktemp)
  local out attempt=1
  # Retry only transport-level failures (RPC unreachable / connection dropped); contract refusals are never retried.
  until out=$(stellar contract invoke --id "$CONTRACT_ID" --network "$NETWORK" --source-account "$who" -- "$@" 2>"$err"); do
    if [ "$attempt" -lt 4 ] && grep -qE 'client error \((Connect|SendRequest)\)|timed out|Too Many Requests' "$err"; then
      attempt=$((attempt + 1)); sleep 3; continue
    fi
    cat "$err" >&2; rm -f "$err"; echo "step $step FAILED" >&2; exit 1
  done
  # The hash comes from the explorer link stellar-cli prints (.../tx/<hash>); a bare 64-hex match also catches unrelated ids.
  local tx; tx=$(grep -oE '/tx/[0-9a-f]{64}' "$err" | tail -1 | cut -d/ -f3 || true); rm -f "$err"
  STEPS=$(jq -c --arg s "$step" --arg tx "$tx" --arg out "$out" '. + [{step: $s, tx: (if $tx == "" then null else $tx end), result: $out}]' <<<"$STEPS")
  printf '%s' "$out"
}
expect_fail() { # expect_fail <signer-alias> <step-name> <expected Error name or code> <fn> [args...]
  local who=$1 step=$2 want=$3; shift 3
  local log; log=$(stellar contract invoke --id "$CONTRACT_ID" --network "$NETWORK" --source-account "$who" --send=no -- "$@" 2>&1) \
    && { echo "step $step unexpectedly succeeded" >&2; exit 1; }
  # stellar-cli prints contract errors as Error(Contract, #N); resolve names through the contract's own enum.
  local code=""
  if [[ "$want" =~ ^[0-9]+$ ]]; then code=$want; else
    code=$(sed -nE "s/^[[:space:]]*${want}[[:space:]]*=[[:space:]]*([0-9]+),.*/\1/p" contracts/escrow/src/types.rs | head -1)
  fi
  grep -qE "$want|Error\(Contract, #${code:-__none__}\)" <<<"$log" || { echo "step $step failed for another reason (wanted $want, code '${code}'):" >&2; echo "$log" >&2; exit 1; }
  STEPS=$(jq -c --arg s "$step" --arg w "$want" --arg c "$code" '. + [{step: $s, refused: $w, errorCode: (if $c == "" then null else ($c|tonumber) end)}]' <<<"$STEPS")
}
now() { date +%s; }

CONTROLLER=$(ident torrent-controller); ALICE=$(ident torrent-alice); BOB=$(ident torrent-bob)
[ "$(inv torrent-controller abi_version abi_version)" = "2" ] || { echo "contract is not ABI v2" >&2; exit 1; }

POOL=$(inv torrent-controller create_pool create_pool --controller "$CONTROLLER" --token "$TOKEN_ID" --membership_policy_hash "$POLICY" | tr -d '"')
for m in alice bob; do
  addr=$(stellar keys address "torrent-$m")
  inv torrent-controller "allow_$m" allow_member --pool_id "$POOL" --member "$addr" >/dev/null
  inv "torrent-$m" "join_$m" join_pool --pool_id "$POOL" --member "$addr" --signed_policy_hash "$POLICY" >/dev/null
done
inv torrent-controller deposit deposit --pool_id "$POOL" --from "$CONTROLLER" --amount 10000000 >/dev/null
# Generous windows: on a slow link each Testnet call can take 20-40 s, and the offer must land before the right matures.
MATURITY=$(( $(now) + ${SMOKE_MATURITY_SECS:-420} ))
expect_fail torrent-controller issue_unbacked InsufficientBacking issue_right --pool_id "$POOL" --owner "$ALICE" --amount 10000001 --maturity "$MATURITY" --policy_hash "$POLICY"
RIGHT=$(inv torrent-controller issue_right issue_right --pool_id "$POOL" --owner "$ALICE" --amount 6000000 --maturity "$MATURITY" --policy_hash "$POLICY" | tr -d '"')
expect_fail torrent-alice offer_beyond_maturity InvalidExpiry propose_transfer --right_id "$RIGHT" --new_owner "$BOB" --expiry $(( MATURITY + 1 ))
T=$(inv torrent-alice propose_transfer propose_transfer --right_id "$RIGHT" --new_owner "$BOB" --expiry "$MATURITY" | tr -d '"')
inv torrent-bob accept_transfer accept_transfer --transfer_id "$T" >/dev/null
expect_fail torrent-bob accept_replay TransferNotPending accept_transfer --transfer_id "$T"
expect_fail torrent-bob claim_early NotMatured claim --right_id "$RIGHT" --recipient "$BOB"
expect_fail torrent-bob leave_while_owning MemberHasRights leave_pool --pool_id "$POOL" --member "$BOB"
echo "waiting for maturity ($(( MATURITY - $(now) ))s)"; while [ "$(now)" -lt $(( MATURITY + 10 )) ]; do sleep 5; done
inv torrent-bob claim claim --right_id "$RIGHT" --recipient "$BOB" >/dev/null
expect_fail torrent-bob double_claim AlreadyClaimed claim --right_id "$RIGHT" --recipient "$BOB"
inv torrent-bob leave_bob leave_pool --pool_id "$POOL" --member "$BOB" >/dev/null
inv torrent-controller propose_controller propose_controller --pool_id "$POOL" --proposed "$ALICE" >/dev/null
inv torrent-alice accept_controller accept_controller --pool_id "$POOL" >/dev/null
inv torrent-alice close_pool close_pool --pool_id "$POOL" >/dev/null
expect_fail torrent-alice deposit_after_close PoolClosed deposit --pool_id "$POOL" --from "$ALICE" --amount 1
inv torrent-alice withdraw_leftover withdraw_unreserved --pool_id "$POOL" --to "$CONTROLLER" --amount 4000000 >/dev/null
TOTALS=$(inv torrent-alice get_totals get_totals --pool_id "$POOL")
echo "final totals: $TOTALS"
jq -e '.reserved == "0" and .claimed == "6000000" and .withdrawn == "4000000"' <<<"$TOTALS" >/dev/null \
  || { echo "unexpected final totals" >&2; exit 1; }

jq -n --arg c "$CONTRACT_ID" --arg pool "$POOL" --arg token "$TOKEN_ID" --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson steps "$STEPS" '{network: "testnet", contractId: $c, token: $token, poolId: $pool, ranAt: $at, steps: $steps}' > "$OUT"
echo "smoke run passed; transaction hashes recorded in $OUT (commit it)"
