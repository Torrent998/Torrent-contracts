//! Authorization and hostile-token tests. These do NOT use `mock_all_auths` for the call under
//! test: only the named signer is authorized, so any call that requires someone else must fail.
mod common;
use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, MuxedAddress};
use torrent_escrow::Error;

/// Authorize exactly `who` for `fn_name(args)` on the escrow, nothing else.
macro_rules! only {
    ($f:expr, $who:expr, $name:expr, $args:expr) => {
        $f.escrow.mock_auths(&[MockAuth {
            address: $who,
            invoke: &MockAuthInvoke {
                contract: &$f.escrow_addr,
                fn_name: $name,
                args: $args.into_val(&$f.env),
                sub_invokes: &[],
            },
        }])
    };
}

#[test]
fn only_the_controller_can_issue_pause_unpause_withdraw_allow() {
    let f = setup();
    f.fund(100);
    let m = START + DAY;
    // alice (a member, not the controller) is authorized for each call; each must fail
    assert!(only!(
        f,
        &f.alice,
        "issue_right",
        (f.pool_id, &f.alice, 1i128, m, &f.policy)
    )
    .try_issue_right(&f.pool_id, &f.alice, &1, &m, &f.policy)
    .is_err());
    assert!(only!(f, &f.alice, "pause_pool", (f.pool_id,))
        .try_pause_pool(&f.pool_id)
        .is_err());
    assert!(only!(
        f,
        &f.alice,
        "withdraw_unreserved",
        (f.pool_id, &f.alice, 1i128)
    )
    .try_withdraw_unreserved(&f.pool_id, &f.alice, &1)
    .is_err());
    assert!(only!(f, &f.alice, "allow_member", (f.pool_id, &f.outsider))
        .try_allow_member(&f.pool_id, &f.outsider)
        .is_err());
    // and the controller can
    only!(
        f,
        &f.controller,
        "issue_right",
        (f.pool_id, &f.alice, 1i128, m, &f.policy)
    )
    .issue_right(&f.pool_id, &f.alice, &1, &m, &f.policy);
    only!(f, &f.controller, "pause_pool", (f.pool_id,)).pause_pool(&f.pool_id);
}

#[test]
fn transfer_steps_need_the_right_party() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 10);
    let exp = START + DAY;
    // bob cannot propose alice's right
    assert!(only!(f, &f.bob, "propose_transfer", (r, &f.bob, exp))
        .try_propose_transfer(&r, &f.bob, &exp)
        .is_err());
    let t =
        only!(f, &f.alice, "propose_transfer", (r, &f.bob, exp)).propose_transfer(&r, &f.bob, &exp);
    // alice cannot accept on bob's behalf; carol cannot accept either
    assert!(only!(f, &f.alice, "accept_transfer", (t,))
        .try_accept_transfer(&t)
        .is_err());
    assert!(only!(f, &f.carol, "accept_transfer", (t,))
        .try_accept_transfer(&t)
        .is_err());
    // only the proposer can cancel
    assert!(only!(f, &f.bob, "cancel_transfer", (t,))
        .try_cancel_transfer(&t)
        .is_err());
    only!(f, &f.bob, "accept_transfer", (t,)).accept_transfer(&t);
    assert_eq!(f.escrow.get_right(&r).owner, f.bob);
}

#[test]
fn deposit_and_join_need_the_depositor_or_member_signature() {
    let f = setup();
    // controller authorizes, but `from` is alice => alice's auth is missing
    assert!(
        only!(f, &f.controller, "deposit", (f.pool_id, &f.alice, 10i128))
            .try_deposit(&f.pool_id, &f.alice, &10)
            .is_err()
    );
    let stranger = Address::generate(&f.env);
    f.escrow.allow_member(&f.pool_id, &stranger);
    // someone else cannot sign the member's consent to the policy
    assert!(only!(
        f,
        &f.controller,
        "join_pool",
        (f.pool_id, &stranger, &f.policy)
    )
    .try_join_pool(&f.pool_id, &stranger, &f.policy)
    .is_err());
}

// ---- hostile token: takes a fee on transfer ------------------------------------------------

#[contract]
pub struct FeeToken;
#[contractimpl]
impl FeeToken {
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }
    pub fn mint(env: Env, to: Address, amount: i128) {
        let b = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&to, &(b + amount));
    }
    /// Moves `amount` out of `from` but only credits `amount - 1` to `to` (a 1-unit fee).
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to = to.address();
        let fb = Self::balance(env.clone(), from.clone());
        env.storage().persistent().set(&from, &(fb - amount));
        let tb = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&to, &(tb + amount - 1));
    }
}

#[test]
fn fee_on_transfer_token_is_rejected_and_nothing_is_credited() {
    let f = setup_empty();
    let token = f.env.register(FeeToken, ());
    let tc = FeeTokenClient::new(&f.env, &token);
    let pool = f.escrow.create_pool(&f.controller, &token, &f.policy);
    tc.mint(&f.controller, &1000);
    let res = f.escrow.try_deposit(&pool, &f.controller, &100);
    assert_eq!(res.unwrap_err().unwrap(), Error::DepositMismatch);
    // whole call reverted: no funding recorded, tokens unchanged
    assert_eq!(f.escrow.get_totals(&pool).funded, 0);
    assert_eq!(tc.balance(&f.controller), 1000);
}
