//! ABI v2 pool lifecycle: closing, member removal and leaving, controller rotation, transfer
//! timing rules, and the getters the indexer/API reconcile against.
mod common;
use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, IntoVal};
use torrent_escrow::{Error, PoolStatus, RightState, TransferStatus};

fn err<T: core::fmt::Debug, E: core::fmt::Debug>(
    r: Result<Result<T, E>, Result<Error, soroban_sdk::InvokeError>>,
) -> Error {
    match r {
        Err(Ok(e)) => e,
        other => panic!("expected contract error, got {other:?}"),
    }
}

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

// ---- close_pool ------------------------------------------------------------------------

#[test]
fn closing_stops_new_activity_but_never_claims_or_leftover_withdrawal() {
    let f = setup();
    f.fund(1_000);
    let r = f.issue(&f.alice, 600, 3);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.close_pool(&f.pool_id);
    assert_eq!(f.escrow.get_pool(&f.pool_id).status, PoolStatus::Closed);
    let p = f.pool_id;
    assert_eq!(
        err(f.escrow.try_deposit(&p, &f.controller, &1)),
        Error::PoolClosed
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&p, &f.alice, &1, &(START + DAY), &f.policy)),
        Error::PoolClosed
    );
    assert_eq!(
        err(f.escrow.try_allow_member(&p, &f.outsider)),
        Error::PoolClosed
    );
    assert_eq!(
        err(f.escrow.try_join_pool(&p, &f.outsider, &f.policy)),
        Error::PoolClosed
    );
    assert_eq!(err(f.escrow.try_accept_transfer(&t)), Error::PoolClosed);
    assert_eq!(
        err(f.escrow.try_propose_transfer(&r, &f.carol, &(START + DAY))),
        Error::PoolClosed
    );
    assert_eq!(err(f.escrow.try_pause_pool(&p)), Error::PoolClosed);
    assert_eq!(err(f.escrow.try_unpause_pool(&p)), Error::NotPaused);
    assert_eq!(err(f.escrow.try_close_pool(&p)), Error::AlreadyClosed);
    // The open proposal can still be withdrawn by its proposer.
    f.escrow.cancel_transfer(&t);
    // Leftover (unreserved) value can be settled; reserved value cannot.
    assert_eq!(
        err(f.escrow.try_withdraw_unreserved(&p, &f.controller, &401)),
        Error::InsufficientBacking
    );
    f.escrow.withdraw_unreserved(&p, &f.controller, &400);
    // The right is still paid in full at maturity.
    f.advance(3 * DAY);
    f.escrow.claim(&r, &f.alice);
    let tot = f.escrow.get_totals(&p);
    assert_eq!((tot.reserved, tot.claimed, tot.withdrawn), (0, 600, 400));
    assert_eq!(f.token.balance(&f.escrow_addr), 0);
}

#[test]
fn a_paused_pool_can_be_closed_and_stays_claimable() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 1);
    f.escrow.pause_pool(&f.pool_id);
    f.escrow.close_pool(&f.pool_id);
    f.advance(DAY);
    f.escrow.claim(&r, &f.alice);
    assert_eq!(f.escrow.get_right(&r).state, RightState::Claimed);
}

#[test]
fn only_the_controller_can_close() {
    let f = setup();
    let p = f.pool_id;
    assert!(only!(f, &f.alice, "close_pool", (p,))
        .try_close_pool(&p)
        .is_err());
    only!(f, &f.controller, "close_pool", (p,)).close_pool(&p);
}

// ---- remove_member ---------------------------------------------------------------------

#[test]
fn controller_can_remove_members_only_before_funding() {
    let f = setup();
    let p = f.pool_id;
    // allow-listed but not joined, then joined
    f.escrow.allow_member(&p, &f.outsider);
    f.escrow.remove_member(&p, &f.outsider);
    assert_eq!(
        err(f.escrow.try_join_pool(&p, &f.outsider, &f.policy)),
        Error::NotAllowed
    );
    f.escrow.remove_member(&p, &f.carol);
    assert!(!f.escrow.is_member(&p, &f.carol));
    assert_eq!(err(f.escrow.try_get_member(&p, &f.carol)), Error::NotMember);
    assert_eq!(
        err(f.escrow.try_remove_member(&p, &f.carol)),
        Error::NotAllowed
    );
    // can be re-admitted while still unfunded
    f.escrow.allow_member(&p, &f.carol);
    f.escrow.join_pool(&p, &f.carol, &f.policy);
    f.fund(10);
    assert_eq!(
        err(f.escrow.try_remove_member(&p, &f.bob)),
        Error::MembershipFrozen
    );
    assert!(f.escrow.is_member(&p, &f.bob));
}

#[test]
fn remove_member_needs_the_controller_and_an_active_pool() {
    let f = setup();
    let p = f.pool_id;
    assert!(only!(f, &f.alice, "remove_member", (p, &f.bob))
        .try_remove_member(&p, &f.bob)
        .is_err());
    f.escrow.pause_pool(&p);
    assert_eq!(
        err(f.escrow.try_remove_member(&p, &f.bob)),
        Error::PoolPaused
    );
}

// ---- leave_pool ------------------------------------------------------------------------

#[test]
fn a_member_with_unclaimed_rights_cannot_leave() {
    let f = setup();
    let p = f.pool_id;
    f.fund(500);
    let r = f.issue(&f.alice, 200, 2);
    assert_eq!(f.escrow.get_member(&p, &f.alice).open_rights, 1);
    assert_eq!(
        err(f.escrow.try_leave_pool(&p, &f.alice)),
        Error::MemberHasRights
    );
    // hand the right to bob: counters move with ownership
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.accept_transfer(&t);
    assert_eq!(f.escrow.get_member(&p, &f.alice).open_rights, 0);
    assert_eq!(f.escrow.get_member(&p, &f.bob).open_rights, 1);
    f.escrow.leave_pool(&p, &f.alice);
    let m = f.escrow.get_member(&p, &f.alice);
    assert!(!m.eligible);
    assert!(!f.escrow.is_member(&p, &f.alice));
    assert_eq!(err(f.escrow.try_leave_pool(&p, &f.alice)), Error::NotMember);
    // a former member cannot rejoin, receive rights or offers, or deposit
    assert_eq!(
        err(f.escrow.try_join_pool(&p, &f.alice, &f.policy)),
        Error::NotAllowed
    );
    assert_eq!(
        err(f
            .escrow
            .try_issue_right(&p, &f.alice, &1, &(START + DAY), &f.policy)),
        Error::NotMember
    );
    assert_eq!(
        err(f.escrow.try_deposit(&p, &f.alice, &1)),
        Error::NotMember
    );
    // bob claims, then may leave too; their deposits stay escrowed
    f.advance(2 * DAY);
    f.escrow.claim(&r, &f.bob);
    assert_eq!(f.escrow.get_member(&p, &f.bob).open_rights, 0);
    f.escrow.leave_pool(&p, &f.bob);
    assert_eq!(f.escrow.get_unreserved(&p), 300);
}

#[test]
fn leaving_needs_the_members_own_signature_and_works_in_a_closed_pool() {
    let f = setup();
    let p = f.pool_id;
    assert!(only!(f, &f.controller, "leave_pool", (p, &f.alice))
        .try_leave_pool(&p, &f.alice)
        .is_err());
    assert_eq!(
        err(f.escrow.try_leave_pool(&p, &f.outsider)),
        Error::NotMember
    );
    f.escrow.close_pool(&p);
    only!(f, &f.alice, "leave_pool", (p, &f.alice)).leave_pool(&p, &f.alice);
}

#[test]
fn a_departed_recipient_cannot_accept_a_pending_offer() {
    let f = setup();
    let p = f.pool_id;
    f.fund(100);
    let r = f.issue(&f.alice, 100, 2);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.leave_pool(&p, &f.bob);
    assert_eq!(err(f.escrow.try_accept_transfer(&t)), Error::NotMember);
    assert_eq!(f.escrow.get_right(&r).owner, f.alice);
}

// ---- controller rotation ---------------------------------------------------------------

#[test]
fn controller_rotation_is_two_step_and_moves_all_powers() {
    let f = setup();
    let p = f.pool_id;
    let next = Address::generate(&f.env);
    assert_eq!(
        err(f.escrow.try_accept_controller(&p)),
        Error::NoPendingController
    );
    assert_eq!(
        err(f.escrow.try_propose_controller(&p, &f.controller)),
        Error::SameOwner
    );
    assert!(only!(f, &f.alice, "propose_controller", (p, &next))
        .try_propose_controller(&p, &next)
        .is_err());
    only!(f, &f.controller, "propose_controller", (p, &next)).propose_controller(&p, &next);
    assert_eq!(f.escrow.get_pending_controller(&p), Some(next.clone()));
    // the old controller cannot accept on the successor's behalf
    assert!(only!(f, &f.controller, "accept_controller", (p,))
        .try_accept_controller(&p)
        .is_err());
    only!(f, &next, "accept_controller", (p,)).accept_controller(&p);
    assert_eq!(f.escrow.get_pool(&p).controller, next);
    assert_eq!(f.escrow.get_pending_controller(&p), None);
    // old controller lost its powers, new one has them
    assert!(only!(f, &f.controller, "pause_pool", (p,))
        .try_pause_pool(&p)
        .is_err());
    only!(f, &next, "pause_pool", (p,)).pause_pool(&p);
}

#[test]
fn a_new_proposal_replaces_the_pending_controller() {
    let f = setup();
    let p = f.pool_id;
    let (a, b) = (Address::generate(&f.env), Address::generate(&f.env));
    f.escrow.propose_controller(&p, &a);
    f.escrow.propose_controller(&p, &b);
    assert!(only!(f, &a, "accept_controller", (p,))
        .try_accept_controller(&p)
        .is_err());
    only!(f, &b, "accept_controller", (p,)).accept_controller(&p);
    assert_eq!(f.escrow.get_pool(&p).controller, b);
}

// ---- timing rules ----------------------------------------------------------------------

#[test]
fn offers_cannot_outlive_maturity_and_matured_rights_are_not_traded() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 2); // matures START + 2d
    assert_eq!(
        err(f
            .escrow
            .try_propose_transfer(&r, &f.bob, &(START + 2 * DAY + 1))),
        Error::InvalidExpiry
    );
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + 2 * DAY));
    f.advance(2 * DAY);
    // at maturity the offer has also expired; the right is for claiming now
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t)),
        Error::TransferExpired
    );
    assert_eq!(
        err(f
            .escrow
            .try_propose_transfer(&r, &f.bob, &(START + 3 * DAY))),
        Error::RightMatured
    );
    f.escrow.claim(&r, &f.alice);
}

#[test]
fn replacing_an_expired_offer_records_its_cancellation() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 5);
    let t1 = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.advance(DAY);
    let t2 = f.escrow.propose_transfer(&r, &f.carol, &(START + 2 * DAY));
    assert_eq!(f.escrow.get_transfer(&t1).status, TransferStatus::Cancelled);
    assert_eq!(f.escrow.get_right(&r).pending_transfer, Some(t2));
}

// ---- replay ----------------------------------------------------------------------------

#[test]
fn completed_transfers_and_claims_cannot_be_replayed() {
    let f = setup();
    f.fund(100);
    let r = f.issue(&f.alice, 100, 2);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    f.escrow.accept_transfer(&t);
    assert_eq!(
        err(f.escrow.try_accept_transfer(&t)),
        Error::TransferNotPending
    );
    assert_eq!(
        err(f.escrow.try_cancel_transfer(&t)),
        Error::TransferNotPending
    );
    f.advance(2 * DAY);
    f.escrow.claim(&r, &f.bob);
    assert_eq!(err(f.escrow.try_claim(&r, &f.bob)), Error::AlreadyClaimed);
    assert_eq!(f.escrow.get_member(&f.pool_id, &f.bob).open_rights, 0);
}

// ---- getters ---------------------------------------------------------------------------

#[test]
fn counts_and_member_records_track_ids() {
    let f = setup();
    let c = f.escrow.get_counts();
    assert_eq!((c.pools, c.rights, c.transfers), (1, 0, 0));
    f.fund(100);
    let r = f.issue(&f.alice, 10, 1);
    f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    let c = f.escrow.get_counts();
    assert_eq!((c.pools, c.rights, c.transfers), (1, 1, 1));
    let m = f.escrow.get_member(&f.pool_id, &f.alice);
    assert_eq!(
        (m.eligible, m.open_rights, m.signed_policy_hash),
        (true, 1, f.policy.clone())
    );
    assert_eq!(
        err(f.escrow.try_get_member(&99, &f.alice)),
        Error::PoolNotFound
    );
    assert_eq!(
        err(f.escrow.try_get_pending_controller(&99)),
        Error::PoolNotFound
    );
}
