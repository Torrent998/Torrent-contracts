//! Property tests: random op sequences against the real contract. After EVERY operation
//! (successful or rejected) the global invariants must hold.
mod common;
use common::*;
use proptest::prelude::*;
use soroban_sdk::Address;
use torrent_escrow::RightState;

#[derive(Debug, Clone)]
enum Op {
    Deposit { who: usize, amount: i128 },
    Issue { to: usize, amount: i128, days: u64 },
    Propose { right: usize, to: usize, ttl: u64 },
    Accept { transfer: usize },
    Cancel { transfer: usize },
    Claim { right: usize, recipient: usize },
    Withdraw { amount: i128 },
    Advance { secs: u64 },
    Pause,
    Unpause,
    Close,
    Leave { who: usize },
    Remove { who: usize },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0usize..5, -10i128..2_000).prop_map(|(who, amount)| Op::Deposit { who, amount }),
        4 => (0usize..4, -10i128..1_500, 0u64..20).prop_map(|(to, amount, days)| Op::Issue { to, amount, days }),
        3 => (0usize..8, 0usize..4, 0u64..5 * 86_400).prop_map(|(right, to, ttl)| Op::Propose { right, to, ttl }),
        3 => (0usize..8).prop_map(|transfer| Op::Accept { transfer }),
        1 => (0usize..8).prop_map(|transfer| Op::Cancel { transfer }),
        4 => (0usize..8, 0usize..4).prop_map(|(right, recipient)| Op::Claim { right, recipient }),
        1 => (-5i128..800).prop_map(|amount| Op::Withdraw { amount }),
        3 => (0u64..8 * 86_400).prop_map(|secs| Op::Advance { secs }),
        1 => Just(Op::Pause),
        1 => Just(Op::Unpause),
        1 => Just(Op::Close),
        1 => (0usize..3).prop_map(|who| Op::Leave { who }),
        1 => (0usize..3).prop_map(|who| Op::Remove { who }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn invariants_hold_across_random_sequences(ops in prop::collection::vec(op(), 1..40)) {
        let f = setup();
        f.env.cost_estimate().budget().reset_unlimited();
        // index 0..3 => controller, alice, bob, carol, outsider
        let actors: Vec<Address> = vec![
            f.controller.clone(), f.alice.clone(), f.bob.clone(), f.carol.clone(), f.outsider.clone(),
        ];
        let members: Vec<Address> = actors[1..4].to_vec();
        let total_supply: i128 = actors.iter().map(|a| f.token.balance(a)).sum::<i128>()
            + f.token.balance(&f.escrow_addr);
        let mut n_rights = 0u64;
        let mut n_transfers = 0u64;
        let mut claimed_once = std::collections::HashSet::new();

        for o in ops {
            let now = f.env.ledger().timestamp();
            match o {
                Op::Deposit { who, amount } => {
                    let _ = f.escrow.try_deposit(&f.pool_id, &actors[who], &amount);
                }
                Op::Issue { to, amount, days } => {
                    let maturity = now + days * 86_400;
                    if let Ok(Ok(_)) = f.escrow.try_issue_right(
                        &f.pool_id, &members[to % 3], &amount, &maturity, &hash(&f.env, 1)) {
                        n_rights += 1;
                    }
                }
                Op::Propose { right, to, ttl } => {
                    if n_rights > 0 {
                        let id = (right as u64 % n_rights) + 1;
                        if let Ok(Ok(_)) = f.escrow.try_propose_transfer(&id, &members[to % 3], &(now + ttl)) {
                            n_transfers += 1;
                            // timing rule: only unmatured rights are offered, never beyond maturity
                            let r = f.escrow.get_right(&id);
                            prop_assert!(now < r.maturity && now + ttl <= r.maturity);
                        }
                    }
                }
                Op::Accept { transfer } => {
                    if n_transfers > 0 {
                        let id = (transfer as u64 % n_transfers) + 1;
                        let _ = f.escrow.try_accept_transfer(&id);
                    }
                }
                Op::Cancel { transfer } => {
                    if n_transfers > 0 {
                        let id = (transfer as u64 % n_transfers) + 1;
                        let _ = f.escrow.try_cancel_transfer(&id);
                    }
                }
                Op::Claim { right, recipient } => {
                    if n_rights > 0 {
                        let id = (right as u64 % n_rights) + 1;
                        let before = f.escrow.get_right(&id);
                        let res = f.escrow.try_claim(&id, &actors[recipient]);
                        let ok = matches!(res, Ok(Ok(_)));
                        // claim succeeds iff issued && matured (auth is mocked for the owner)
                        prop_assert_eq!(
                            ok,
                            before.state == RightState::Issued && now >= before.maturity
                        );
                        if ok {
                            prop_assert!(claimed_once.insert(id), "right {} paid out twice", id);
                        }
                    }
                }
                Op::Withdraw { amount } => {
                    let _ = f.escrow.try_withdraw_unreserved(&f.pool_id, &f.controller, &amount);
                }
                Op::Advance { secs } => f.advance(secs),
                Op::Pause => { let _ = f.escrow.try_pause_pool(&f.pool_id); }
                Op::Unpause => { let _ = f.escrow.try_unpause_pool(&f.pool_id); }
                Op::Close => { let _ = f.escrow.try_close_pool(&f.pool_id); }
                Op::Leave { who } => {
                    let m = &members[who];
                    let had_rights = f.escrow.try_get_member(&f.pool_id, m)
                        .ok().and_then(|r| r.ok()).map(|m| m.open_rights > 0);
                    let res = f.escrow.try_leave_pool(&f.pool_id, m);
                    // leaving succeeds only for a current member who owns no unclaimed right
                    if matches!(res, Ok(Ok(_))) {
                        prop_assert_eq!(had_rights, Some(false));
                    }
                }
                Op::Remove { who } => {
                    let res = f.escrow.try_remove_member(&f.pool_id, &members[who]);
                    if matches!(res, Ok(Ok(_))) {
                        prop_assert_eq!(f.escrow.get_totals(&f.pool_id).funded, 0, "removed after funding");
                    }
                }
            }

            // ---- invariants ----
            let t = f.escrow.get_totals(&f.pool_id);
            prop_assert!(t.reserved >= 0);
            prop_assert!(t.reserved <= t.funded - t.claimed - t.withdrawn, "overcollateralization");
            // contract's real token balance equals the book value (no unexpected outflow)
            prop_assert_eq!(f.token.balance(&f.escrow_addr), t.funded - t.claimed - t.withdrawn);
            prop_assert!(f.token.balance(&f.escrow_addr) >= t.reserved);
            // reserved == sum of unclaimed rights; claimed == sum of claimed rights
            let (mut open, mut paid) = (0i128, 0i128);
            for id in 1..=n_rights {
                let r = f.escrow.get_right(&id);
                match r.state {
                    RightState::Issued => open += r.amount,
                    RightState::Claimed => paid += r.amount,
                }
            }
            prop_assert_eq!(open, t.reserved);
            prop_assert_eq!(paid, t.claimed);
            // every member's open_rights counter equals the unclaimed rights they own, and
            // every unclaimed right is owned by a current member
            for m in &members {
                let owned = (1..=n_rights)
                    .map(|id| f.escrow.get_right(&id))
                    .filter(|r| r.state == RightState::Issued && &r.owner == m)
                    .count() as u32;
                let counter = f.escrow.try_get_member(&f.pool_id, m)
                    .ok().and_then(|r| r.ok()).map(|m| m.open_rights).unwrap_or(0);
                prop_assert_eq!(owned, counter, "open_rights drifted");
                if owned > 0 {
                    prop_assert!(f.escrow.is_member(&f.pool_id, m));
                }
            }
            // conservation of synthetic supply
            let sum: i128 = actors.iter().map(|a| f.token.balance(a)).sum::<i128>()
                + f.token.balance(&f.escrow_addr);
            prop_assert_eq!(sum, total_supply);
        }
    }
}

#[derive(Debug, Clone)]
enum OwnOp {
    Issue { to: usize, amount: i128 },
    Propose { right: usize, to: usize },
    Accept { transfer: usize },
    Cancel { transfer: usize },
    Claim { right: usize },
    Advance { secs: u64 },
}

fn own_op() -> impl Strategy<Value = OwnOp> {
    prop_oneof![
        3 => (0usize..3, 1i128..300).prop_map(|(to, amount)| OwnOp::Issue { to, amount }),
        4 => (0usize..6, 0usize..3).prop_map(|(right, to)| OwnOp::Propose { right, to }),
        4 => (0usize..10).prop_map(|transfer| OwnOp::Accept { transfer }),
        1 => (0usize..10).prop_map(|transfer| OwnOp::Cancel { transfer }),
        2 => (0usize..6).prop_map(|right| OwnOp::Claim { right }),
        2 => (0u64..4 * 86_400).prop_map(|secs| OwnOp::Advance { secs }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// Ownership model: a right's owner changes ONLY through a completed transfer to its named
    /// recipient, there is always exactly one owner who is a pool member, and terminal states
    /// (claimed right, completed/cancelled transfer) never revert.
    #[test]
    fn ownership_only_moves_through_completed_transfers(ops in prop::collection::vec(own_op(), 1..40)) {
        use torrent_escrow::TransferStatus;
        let f = setup();
        f.env.cost_estimate().budget().reset_unlimited();
        f.fund(100_000);
        let members: Vec<Address> = vec![f.alice.clone(), f.bob.clone(), f.carol.clone()];
        let mut owners: Vec<Address> = Vec::new();   // model: right id (1-based) -> owner
        let mut transfers: Vec<(u64, Address)> = Vec::new(); // (right id, recipient) per transfer id
        let mut done: std::collections::HashMap<u64, TransferStatus> = Default::default();

        for o in ops {
            let now = f.env.ledger().timestamp();
            match o {
                OwnOp::Issue { to, amount } => {
                    if let Ok(Ok(_)) = f.escrow.try_issue_right(&f.pool_id, &members[to], &amount, &(now + 4 * 86_400), &hash(&f.env, 1)) {
                        owners.push(members[to].clone());
                    }
                }
                OwnOp::Propose { right, to } if !owners.is_empty() => {
                    let id = (right % owners.len()) as u64 + 1;
                    if let Ok(Ok(_)) = f.escrow.try_propose_transfer(&id, &members[to], &(now + 86_400)) {
                        transfers.push((id, members[to].clone()));
                    }
                }
                OwnOp::Accept { transfer } if !transfers.is_empty() => {
                    let idx = transfer % transfers.len();
                    let (rid, to) = transfers[idx].clone();
                    if let Ok(Ok(_)) = f.escrow.try_accept_transfer(&(idx as u64 + 1)) {
                        owners[(rid - 1) as usize] = to; // the only way the model owner moves
                    }
                }
                OwnOp::Cancel { transfer } if !transfers.is_empty() => {
                    let _ = f.escrow.try_cancel_transfer(&((transfer % transfers.len()) as u64 + 1));
                }
                OwnOp::Claim { right } if !owners.is_empty() => {
                    let id = (right % owners.len()) as u64 + 1;
                    let _ = f.escrow.try_claim(&id, &owners[(id - 1) as usize]);
                }
                OwnOp::Advance { secs } => f.advance(secs),
                _ => {}
            }
            for (i, model_owner) in owners.iter().enumerate() {
                let r = f.escrow.get_right(&(i as u64 + 1));
                prop_assert_eq!(&r.owner, model_owner, "owner diverged from completed-transfer history");
                prop_assert!(f.escrow.is_member(&f.pool_id, &r.owner));
            }
            for (i, _) in transfers.iter().enumerate() {
                let t = f.escrow.get_transfer(&(i as u64 + 1));
                if let Some(prev) = done.get(&t.transfer_id) {
                    if *prev != TransferStatus::Pending { prop_assert_eq!(*prev, t.status, "terminal transfer status changed"); }
                }
                done.insert(t.transfer_id, t.status);
            }
            let t = f.escrow.get_totals(&f.pool_id);
            prop_assert!(t.reserved <= t.funded - t.claimed - t.withdrawn);
        }
    }
}
