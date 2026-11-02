#!/usr/bin/env bash
# Manual smoke sequence against a deployed contract (TESTNET, synthetic funds). Documented, never executed by the authors.
# Requires: CONTRACT_ID, TOKEN_ID (a Stellar Asset Contract you control), and funded aliases controller/alice.
set -euo pipefail
: "${CONTRACT_ID:?}" "${TOKEN_ID:?}"
POLICY=$(printf '07%.0s' $(seq 1 32))
inv() { stellar contract invoke --id "$CONTRACT_ID" --network testnet --source-account "$1" -- "${@:2}"; }
CONTROLLER=$(stellar keys address controller); ALICE=$(stellar keys address alice)
POOL=$(inv controller create_pool --controller "$CONTROLLER" --token "$TOKEN_ID" --membership_policy_hash "$POLICY" | tr -d '"')
inv controller allow_member --pool_id "$POOL" --member "$ALICE"
inv alice join_pool --pool_id "$POOL" --member "$ALICE" --signed_policy_hash "$POLICY"
inv controller deposit --pool_id "$POOL" --from "$CONTROLLER" --amount 1000
inv controller issue_right --pool_id "$POOL" --owner "$ALICE" --amount 600 --maturity "$(( $(date +%s) + 120 ))" --policy_hash "$POLICY"
inv controller get_totals --pool_id "$POOL"
echo "Record the transaction hashes printed above in deployments/testnet.json (txs)."
