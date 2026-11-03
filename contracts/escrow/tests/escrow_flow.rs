//! Functional + failure-path tests against the real contract inside the Soroban test host.
mod common;
use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use torrent_escrow::{Error, PoolStatus, RightState, TransferStatus};

fn err<T: core::fmt::Debug, E: core::fmt::Debug>(
    r: Result<Result<T, E>, Result<Error, soroban_sdk::InvokeError>>,
) -> Error {
    match r {
        Err(Ok(e)) => e,
        other => panic!("expected contract error, got {other:?}"),
    }
}

#[test]
fn happy_path_fund_issue_transfer_claim() {
    let f = setup();
    f.fund(1_000);
    assert_eq!(f.escrow.get_unreserved(&f.pool_id), 1_000);
    let r = f.issue(&f.alice, 600, 30);
    assert_eq!(f.escrow.get_unreserved(&f.pool_id), 400);

    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + 5 * DAY));
    f.escrow.accept_transfer(&t);
    assert_eq!(f.escrow.get_right(&r).owner, f.bob);
    assert_eq!(f.escrow.get_transfer(&t).status, TransferStatus::Completed);

    f.advance(30 * DAY);
    let before = f.token.balance(&f.carol);
    f.escrow.claim(&r, &f.carol);
    assert_eq!(f.token.balance(&f.carol) - before, 600);
    assert_eq!(f.escrow.get_right(&r).state, RightState::Claimed);
    let totals = f.escrow.get_totals(&f.pool_id);
    assert_eq!(
        (totals.funded, totals.reserved, totals.claimed),
        (1_000, 0, 600)
    );
    // contract holds exactly funded - claimed
    assert_eq!(f.token.balance(&f.escrow_addr), 400);
}

#[test]
fn cannot_issue_beyond_backing() {
    let f = setup();
    f.fund(500);
    f.issue(&f.alice, 300, 10);
    let maturity = START + DAY;
    let e = err(f
        .escrow
        .try_issue_right(&f.pool_id, &f.bob, &201, &maturity, &f.policy));
    assert_eq!(e, Error::InsufficientBacking);
    // exactly the remainder is fine
    f.issue(&f.bob, 200, 10);
    assert_eq!(f.escrow.get_unreserved(&f.pool_id), 0);
    // reserved did not change after the rejected call
    assert_eq!(f.escrow.get_totals(&f.pool_id).reserved, 500);
}

#[test]
fn cannot_issue_with_zero_funding() {
    let f = setup();
    let e = err(f
        .escrow
        .try_issue_right(&f.pool_id, &f.alice, &1, &(START + 1), &f.policy));
    assert_eq!(e, Error::InsufficientBacking);
}

#[test]
fn issue_validation() {
    let f = setup();
    f.fund(100);
    let m = START + DAY;
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&f.pool_id, &f.alice, &0, &m, &f.policy)),
        Error::InvalidAmount
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&f.pool_id, &f.alice, &-5, &m, &f.policy)),
        Error::InvalidAmount
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&f.pool_id, &f.alice, &1, &START, &f.policy)),
        Error::MaturityInPast
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&f.pool_id, &f.outsider, &1, &m, &f.policy)),
        Error::NotMember
    );
    assert_eq!(
        err(f.escrow.try_issue_right(&999, &f.alice, &1, &m, &f.policy)),
        Error::PoolNotFound
    );
}

#[test]
fn old_owner_cannot_claim_after_transfer() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.accept_transfer(&t);
    f.advance(2 * DAY);
    // Auth is mocked for everyone, so inspect *who* the contract demanded auth from.
    f.escrow.claim(&r, &f.alice);
    assert_eq!(
        signers(&f),
        vec![f.bob.clone()],
        "claim must require the CURRENT owner"
    );
}

#[test]
fn old_owner_real_auth_rejected() {
    // Same scenario with real (non-mocked) auth for alice only: the host must reject.
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.accept_transfer(&t);
    f.advance(2 * DAY);
    let res = f
        .escrow
        .mock_auths(&[MockAuth {
            address: &f.alice,
            invoke: &MockAuthInvoke {
                contract: &f.escrow_addr,
                fn_name: "claim",
                args: (r, &f.alice).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .try_claim(&r, &f.alice);
    assert!(res.is_err(), "alice no longer owns the right");
    assert_eq!(f.token.balance(&f.escrow_addr), 100);
}

#[test]
fn new_owner_cannot_claim_before_maturity() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 10);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.accept_transfer(&t);
    f.advance(10 * DAY - 1);
    assert_eq!(err(f.escrow.try_claim(&r, &f.bob)), Error::NotMatured);
    f.advance(1); // exactly at maturity
    f.escrow.claim(&r, &f.bob);
}

#[test]
fn double_claim_rejected() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    f.advance(DAY);
    f.escrow.claim(&r, &f.alice);
    assert_eq!(err(f.escrow.try_claim(&r, &f.alice)), Error::AlreadyClaimed);
    assert_eq!(f.token.balance(&f.escrow_addr), 0);
    assert_eq!(f.escrow.get_totals(&f.pool_id).claimed, 100);
}

#[test]
fn claim_unknown_right() {
    let f = setup();
    assert_eq!(err(f.escrow.try_claim(&42, &f.alice)), Error::RightNotFound);
}

fn signers(f: &Fixture) -> Vec<Address> {
    f.env.auths().into_iter().map(|(a, _)| a).collect()
}

#[test]
fn transfer_calls_demand_the_right_signers() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    assert_eq!(
        signers(&f),
        vec![f.alice.clone()],
        "proposal needs the current owner"
    );
    f.escrow.accept_transfer(&t);
    assert_eq!(
        signers(&f),
        vec![f.bob.clone()],
        "acceptance needs the recipient"
    );
}

#[test]
fn issuance_and_pause_require_controller_auth() {
    let f = setup();
    f.fund(10);
    f.issue(&f.alice, 1, 1);
    assert_eq!(signers(&f), vec![f.controller.clone()]);
    f.escrow.pause_pool(&f.pool_id);
    assert_eq!(signers(&f), vec![f.controller.clone()]);
}

#[test]
fn no_auth_means_no_state_change() {
    // Fresh env WITHOUT mock_all_auths: every mutating call must be refused by the host.
    let env = Env::default();
    let admin = Address::generate(&env);
    let id = env.register(torrent_escrow::TorrentEscrow, (admin,));
    let c = torrent_escrow::TorrentEscrowClient::new(&env, &id);
    let who = Address::generate(&env);
    let h = hash(&env, 1);
    assert!(c.try_create_pool(&who, &who, &h).is_err());
}

#[test]
fn transfer_rules() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 10);
    let exp = START + DAY;
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.alice, &exp)),
        Error::SameOwner
    );
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.outsider, &exp)),
        Error::NotMember
    );
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.bob, &START)),
        Error::InvalidExpiry
    );
    assert_eq!(
        err(f.escrow.try_propose_transfer(&77, &f.bob, &exp)),
        Error::RightNotFound
    );
    let t = f.escrow.propose_transfer(&r, &f.bob, &exp);
    // only one open proposal per right
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.carol, &exp)),
        Error::TransferPending
    );
    // replay: accepting twice fails, owner unchanged afterwards
    f.escrow.accept_transfer(&t);
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t)),
        Error::TransferNotPending
    );
    assert_eq!(f.escrow.get_right(&r).owner, f.bob);
}

#[test]
fn expired_transfer_cannot_be_accepted_and_can_be_replaced() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 10);
    let t1 = f.escrow.propose_transfer(&r, &f.bob, &(START + 100));
    f.advance(100);
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t1)),
        Error::TransferExpired
    );
    let t2 = f.escrow.propose_transfer(&r, &f.carol, &(START + 10_000));
    assert_eq!(f.escrow.get_transfer(&t1).status, TransferStatus::Cancelled);
    f.escrow.accept_transfer(&t2);
    assert_eq!(f.escrow.get_right(&r).owner, f.carol);
}

#[test]
fn cancelled_transfer_cannot_be_accepted() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 10);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.cancel_transfer(&t);
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t)),
        Error::TransferNotPending
    );
    assert_eq!(
        err(f.escrow.try_cancel_transfer(&t)),
        Error::TransferNotPending
    );
    assert_eq!(f.escrow.get_right(&r).owner, f.alice);
    assert_eq!(f.escrow.get_right(&r).pending_transfer, None);
}

#[test]
fn claiming_voids_pending_transfer() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.advance(DAY);
    f.escrow.claim(&r, &f.alice);
    assert_eq!(f.escrow.get_transfer(&t).status, TransferStatus::Cancelled);
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t)),
        Error::TransferNotPending
    );
}

#[test]
fn deposit_rules() {
    let f = setup();
    assert_eq!(
        err(f.escrow.try_deposit(&f.pool_id, &f.alice, &0)),
        Error::InvalidAmount
    );
    assert_eq!(
        err(f.escrow.try_deposit(&f.pool_id, &f.outsider, &10)),
        Error::NotMember
    );
    assert_eq!(
        err(f.escrow.try_deposit(&404, &f.alice, &10)),
        Error::PoolNotFound
    );
    // member can fund; receipts are sequential
    assert_eq!(f.escrow.deposit(&f.pool_id, &f.alice, &10), 1);
    assert_eq!(f.escrow.deposit(&f.pool_id, &f.controller, &10), 2);
    assert_eq!(f.token.balance(&f.escrow_addr), 20);
}

#[test]
fn deposit_beyond_balance_reverts_everything() {
    let f = setup();
    let res = f.escrow.try_deposit(&f.pool_id, &f.alice, &2_000_000);
    assert!(res.is_err());
    assert_eq!(f.escrow.get_totals(&f.pool_id).funded, 0);
    assert_eq!(f.token.balance(&f.alice), 1_000_000);
}

#[test]
fn membership_rules() {
    let f = setup();
    let id = f.pool_id;
    let stranger = Address::generate(&f.env);
    // not pre-approved
    assert_eq!(
        err(f.escrow.try_join_pool(&id, &stranger, &f.policy)),
        Error::NotAllowed
    );
    f.escrow.allow_member(&id, &stranger);
    // wrong policy hash
    assert_eq!(
        err(f.escrow.try_join_pool(&id, &stranger, &hash(&f.env, 8))),
        Error::PolicyMismatch
    );
    f.escrow.join_pool(&id, &stranger, &f.policy);
    assert_eq!(
        err(f.escrow.try_join_pool(&id, &stranger, &f.policy)),
        Error::AlreadyMember
    );
    assert!(f.escrow.is_member(&id, &stranger));
}

#[test]
fn membership_frozen_after_funding() {
    let f = setup();
    f.fund(1);
    let late = Address::generate(&f.env);
    assert_eq!(
        err(f.escrow.try_allow_member(&f.pool_id, &late)),
        Error::MembershipFrozen
    );
}

#[test]
fn withdraw_only_unreserved() {
    let f = setup();
    f.fund(1_000);
    f.issue(&f.alice, 700, 10);
    assert_eq!(
        err(f
            .escrow
            .try_withdraw_unreserved(&f.pool_id, &f.controller, &301)),
        Error::InsufficientBacking
    );
    assert_eq!(
        err(f
            .escrow
            .try_withdraw_unreserved(&f.pool_id, &f.controller, &0)),
        Error::InvalidAmount
    );
    let before = f.token.balance(&f.controller);
    f.escrow
        .withdraw_unreserved(&f.pool_id, &f.controller, &300);
    assert_eq!(f.token.balance(&f.controller) - before, 300);
    assert_eq!(f.token.balance(&f.escrow_addr), 700);
    // the remaining 700 are untouchable by anyone but the right owner
    assert_eq!(
        err(f
            .escrow
            .try_withdraw_unreserved(&f.pool_id, &f.controller, &1)),
        Error::InsufficientBacking
    );
    f.advance(10 * DAY);
    f.escrow.claim(&f.escrow.get_right(&1).right_id, &f.alice);
    assert_eq!(f.token.balance(&f.escrow_addr), 0);
}

#[test]
fn pause_blocks_risky_ops_but_never_claims() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    f.escrow.pause_pool(&f.pool_id);
    assert_eq!(f.escrow.get_pool(&f.pool_id).status, PoolStatus::Paused);
    assert_eq!(
        err(f.escrow.try_pause_pool(&f.pool_id)),
        Error::AlreadyPaused
    );
    assert_eq!(
        err(f.escrow.try_deposit(&f.pool_id, &f.alice, &1)),
        Error::PoolPaused
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&f.pool_id, &f.bob, &1, &(START + DAY), &f.policy)),
        Error::PoolPaused
    );
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.bob, &(START + DAY))),
        Error::PoolPaused
    );
    assert_eq!(
        err(f
            .escrow
            .try_withdraw_unreserved(&f.pool_id, &f.controller, &1)),
        Error::PoolPaused
    );
    f.advance(DAY);
    let before = f.token.balance(&f.alice);
    f.escrow.claim(&r, &f.alice); // funds are never trapped
    assert_eq!(f.token.balance(&f.alice) - before, 100);
    f.escrow.unpause_pool(&f.pool_id);
    assert_eq!(err(f.escrow.try_unpause_pool(&f.pool_id)), Error::NotPaused);
}

#[test]
fn pools_are_isolated() {
    let f = setup();
    let other = f
        .escrow
        .create_pool(&f.controller, &f.token.address, &f.policy);
    f.fund(100); // pool 1 only
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&other, &f.alice, &1, &(START + DAY), &f.policy)),
        Error::NotMember
    );
    f.escrow.allow_member(&other, &f.alice);
    f.escrow.join_pool(&other, &f.alice, &f.policy);
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&other, &f.alice, &1, &(START + DAY), &f.policy)),
        Error::InsufficientBacking
    );
}

#[test]
fn abi_version_is_2() {
    let f = setup();
    assert_eq!(f.escrow.abi_version(), 2);
}
