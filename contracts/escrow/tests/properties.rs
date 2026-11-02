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
            // conservation of synthetic supply
            let sum: i128 = actors.iter().map(|a| f.token.balance(a)).sum::<i128>()
                + f.token.balance(&f.escrow_addr);
            prop_assert_eq!(sum, total_supply);
        }
    }
}
