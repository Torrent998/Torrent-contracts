//! Contract governance: two-step admin rotation, timelocked upgrades with explicit admin auth,
//! and the irreversible upgrade lock. The upgrade path installs the REAL release wasm of this
//! crate (built by `scripts/check.sh` before the tests run) and proves state survives.
mod common;
use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, BytesN, IntoVal};
use torrent_escrow::{Error, UPGRADE_DELAY_SECS};

fn err<T: core::fmt::Debug, E: core::fmt::Debug>(
    r: Result<Result<T, E>, Result<Error, soroban_sdk::InvokeError>>,
) -> Error {
    match r {
        Err(Ok(e)) => e,
        other => panic!("expected contract error, got {other:?}"),
    }
}

pub fn release_wasm() -> Vec<u8> {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/wasm32v1-none/release/torrent_escrow.wasm"
    );
    std::fs::read(p).unwrap_or_else(|_| {
        panic!("{p} missing: run ./scripts/check.sh (it builds the wasm before the tests)")
    })
}

#[test]
fn constructor_sets_admin_and_defaults() {
    let f = setup();
    let g = f.escrow.get_governance();
    assert_eq!(g.admin, f.admin);
    assert_eq!(g.pending_admin, None);
    assert_eq!(g.pending_upgrade, None);
    assert_eq!(g.upgrade_executable_after, 0);
    assert!(!g.upgrades_locked);
    assert_eq!(g.upgrade_delay_secs, 7 * DAY);
}

#[test]
fn admin_rotation_is_two_step() {
    let f = setup();
    let next = Address::generate(&f.env);
    assert_eq!(err(f.escrow.try_accept_admin()), Error::NoPendingAdmin);
    assert_eq!(err(f.escrow.try_propose_admin(&f.admin)), Error::SameOwner);
    f.escrow.propose_admin(&next);
    assert_eq!(f.escrow.get_governance().pending_admin, Some(next.clone()));
    assert_eq!(f.escrow.get_governance().admin, f.admin);
    f.escrow.accept_admin();
    let g = f.escrow.get_governance();
    assert_eq!((g.admin, g.pending_admin), (next, None));
}

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
fn admin_calls_need_the_admin_signature() {
    let f = setup();
    let h = hash(&f.env, 3);
    let next = Address::generate(&f.env);
    // the pool controller (or anyone else) is NOT the admin
    let c = &f.controller;
    assert!(only!(f, c, "propose_upgrade", (h.clone(),))
        .try_propose_upgrade(&h)
        .is_err());
    assert!(only!(f, c, "propose_admin", (next.clone(),))
        .try_propose_admin(&next)
        .is_err());
    assert!(only!(f, c, "lock_upgrades", ())
        .try_lock_upgrades()
        .is_err());
    // the real admin can
    only!(f, &f.admin, "propose_upgrade", (h.clone(),)).propose_upgrade(&h);
    assert!(only!(f, c, "cancel_upgrade", ())
        .try_cancel_upgrade()
        .is_err());
    f.advance(UPGRADE_DELAY_SECS);
    assert!(only!(f, c, "execute_upgrade", ())
        .try_execute_upgrade()
        .is_err());
    only!(f, &f.admin, "cancel_upgrade", ()).cancel_upgrade();
    // Only the proposed admin can accept, not the current one.
    only!(f, &f.admin, "propose_admin", (next.clone(),)).propose_admin(&next);
    assert!(only!(f, &f.admin, "accept_admin", ())
        .try_accept_admin()
        .is_err());
    assert_eq!(f.escrow.get_governance().admin, f.admin);
    only!(f, &next, "accept_admin", ()).accept_admin();
    assert_eq!(f.escrow.get_governance().admin, next);
}

#[test]
fn upgrade_is_timelocked_and_preserves_state() {
    let f = setup();
    f.fund(1_000);
    let r = f.issue(&f.alice, 400, 30);
    let wasm_hash: BytesN<32> = f
        .env
        .deployer()
        .upload_contract_wasm(release_wasm().as_slice());
    assert_eq!(err(f.escrow.try_execute_upgrade()), Error::NoPendingUpgrade);
    assert_eq!(err(f.escrow.try_cancel_upgrade()), Error::NoPendingUpgrade);
    let after = f.escrow.propose_upgrade(&wasm_hash);
    assert_eq!(after, START + UPGRADE_DELAY_SECS);
    assert_eq!(
        err(f.escrow.try_propose_upgrade(&wasm_hash)),
        Error::UpgradePending
    );
    assert_eq!(
        f.escrow.get_governance().pending_upgrade,
        Some(wasm_hash.clone())
    );
    f.advance(UPGRADE_DELAY_SECS - 1);
    assert_eq!(err(f.escrow.try_execute_upgrade()), Error::UpgradeNotReady);
    f.advance(1);
    f.escrow.execute_upgrade();
    // Now running the real wasm: storage, ids and invariants carried over.
    assert_eq!(f.escrow.abi_version(), 2);
    let g = f.escrow.get_governance();
    assert_eq!((g.pending_upgrade, g.upgrade_executable_after), (None, 0));
    assert_eq!(f.escrow.get_right(&r).owner, f.alice);
    let t = f.escrow.get_totals(&f.pool_id);
    assert_eq!((t.funded, t.reserved), (1_000, 400));
    // and the upgraded code still enforces backing
    assert_eq!(
        err(f.escrow.try_issue_right(
            &f.pool_id,
            &f.bob,
            &601,
            &(f.env.ledger().timestamp() + DAY),
            &f.policy
        )),
        Error::InsufficientBacking
    );
    f.advance(30 * DAY);
    f.escrow.claim(&r, &f.alice);
    assert_eq!(f.token.balance(&f.escrow_addr), 600);
}

#[test]
fn cancelled_upgrade_cannot_execute() {
    let f = setup();
    let h = hash(&f.env, 4);
    f.escrow.propose_upgrade(&h);
    f.escrow.cancel_upgrade();
    f.advance(UPGRADE_DELAY_SECS);
    assert_eq!(err(f.escrow.try_execute_upgrade()), Error::NoPendingUpgrade);
    // a new proposal restarts the clock
    f.escrow.propose_upgrade(&h);
    assert_eq!(err(f.escrow.try_execute_upgrade()), Error::UpgradeNotReady);
}

#[test]
fn locking_upgrades_is_permanent_and_cancels_pending() {
    let f = setup();
    let h = hash(&f.env, 5);
    f.escrow.propose_upgrade(&h);
    f.escrow.lock_upgrades();
    let g = f.escrow.get_governance();
    assert!(g.upgrades_locked);
    assert_eq!(g.pending_upgrade, None);
    assert_eq!(err(f.escrow.try_propose_upgrade(&h)), Error::UpgradesLocked);
    assert_eq!(err(f.escrow.try_execute_upgrade()), Error::UpgradesLocked);
    assert_eq!(err(f.escrow.try_lock_upgrades()), Error::UpgradesLocked);
    // admin rotation still works, but never re-enables upgrades
    let next = Address::generate(&f.env);
    f.escrow.propose_admin(&next);
    f.escrow.accept_admin();
    assert!(f.escrow.get_governance().upgrades_locked);
}

#[test]
fn admin_has_no_power_over_pools() {
    let f = setup();
    f.fund(100);
    let a = &f.admin;
    let (p, m) = (f.pool_id, START + DAY);
    assert!(only!(f, a, "pause_pool", (p,)).try_pause_pool(&p).is_err());
    assert!(only!(f, a, "close_pool", (p,)).try_close_pool(&p).is_err());
    assert!(only!(f, a, "withdraw_unreserved", (p, a, 1i128))
        .try_withdraw_unreserved(&p, a, &1)
        .is_err());
    assert!(
        only!(f, a, "issue_right", (p, &f.alice, 1i128, m, &f.policy))
            .try_issue_right(&p, &f.alice, &1, &m, &f.policy)
            .is_err()
    );
    assert_eq!(f.token.balance(&f.escrow_addr), 100);
}
