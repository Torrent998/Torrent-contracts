//! Event schema v1 golden test. `bindings/events.v1.json` is the cross-repo contract consumed by
//! torrent-indexer fixtures; this test fails if the contract and the file drift apart.
mod common;
use common::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use torrent_escrow::*;

fn names(f: &Fixture) -> Vec<String> {
    f.env
        .events()
        .all()
        .filter_by_contract(&f.escrow_addr)
        .events()
        .iter()
        .map(|e| {
            let ContractEventBody::V0(b) = &e.body;
            match &b.topics[0] {
                ScVal::Symbol(s) => s.to_utf8_string_lossy(),
                other => panic!("unexpected topic {other:?}"),
            }
        })
        .collect()
}

#[test]
fn golden_fixture_lists_exactly_the_events_this_crate_defines() {
    let golden = serde_like::load();
    assert_eq!(golden.abi_version, ABI_VERSION);
    let mut defined = vec![
        "pool_created",
        "member_joined",
        "funded",
        "right_issued",
        "transfer_proposed",
        "transfer_completed",
        "transfer_cancelled",
        "right_claimed",
        "unreserved_withdrawn",
        "pool_paused",
        "pool_unpaused",
    ];
    let mut fixture: Vec<&str> = golden.events.iter().map(|s| s.as_str()).collect();
    defined.sort();
    fixture.sort();
    assert_eq!(defined, fixture);
}

#[test]
fn each_call_emits_its_event() {
    let f = setup();
    f.fund(1_000);
    assert_eq!(names(&f), ["funded"]);
    let r = f.issue(&f.alice, 500, 1);
    assert_eq!(names(&f), ["right_issued"]);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + DAY));
    assert_eq!(names(&f), ["transfer_proposed"]);
    f.escrow.accept_transfer(&t);
    assert_eq!(names(&f), ["transfer_completed"]);
    f.advance(DAY);
    f.escrow.claim(&r, &f.bob);
    assert_eq!(names(&f), ["right_claimed"]);
    f.escrow
        .withdraw_unreserved(&f.pool_id, &f.controller, &499);
    assert_eq!(names(&f), ["unreserved_withdrawn"]);
    f.escrow.pause_pool(&f.pool_id);
    assert_eq!(names(&f), ["pool_paused"]);
    f.escrow.unpause_pool(&f.pool_id);
    assert_eq!(names(&f), ["pool_unpaused"]);
    let r2 = f.issue(&f.carol, 1, 1);
    f.escrow.propose_transfer(&r2, &f.bob, &(START + 10 * DAY));
    f.escrow.cancel_transfer(&2);
    assert_eq!(names(&f), ["transfer_cancelled"]);
}

#[test]
fn rejected_calls_emit_nothing() {
    let f = setup();
    assert!(f.escrow.try_deposit(&f.pool_id, &f.outsider, &5).is_err());
    assert!(names(&f).is_empty());
}

/// Tiny JSON reader so the test needs no extra dependency: pulls `abiVersion` and `events[].name`.
mod serde_like {
    pub struct Golden {
        pub abi_version: u32,
        pub events: Vec<String>,
    }
    pub fn load() -> Golden {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bindings/events.v1.json");
        let s = std::fs::read_to_string(p).expect("bindings/events.v1.json");
        let abi_version = s
            .split("\"abiVersion\":")
            .nth(1)
            .and_then(|r| r.trim_start().split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse().ok())
            .expect("abiVersion");
        let events = s
            .split("\"name\": \"")
            .skip(1)
            .map(|r| r.split('"').next().unwrap().to_string())
            .collect();
        Golden {
            abi_version,
            events,
        }
    }
}

#[test]
fn synthetic_scenario_fixture_replays_against_the_contract() {
    // Mirrors bindings/scenario.v1.json step by step and checks the per-step event names.
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bindings/scenario.v1.json"
    ))
    .unwrap();
    assert!(raw.contains("\"abiVersion\": 1"));
    let f = setup_empty();
    f.escrow
        .create_pool(&f.controller, &f.token.address, &f.policy);
    assert_eq!(names(&f), ["pool_created"]);
    for who in [&f.alice, &f.bob] {
        f.escrow.allow_member(&1, who);
        f.escrow.join_pool(&1, who, &f.policy);
        assert_eq!(names(&f), ["member_joined"]);
    }
    f.escrow.deposit(&1, &f.controller, &1000);
    assert_eq!(names(&f), ["funded"]);
    let r = f
        .escrow
        .issue_right(&1, &f.alice, &600, &(START + 30 * DAY), &f.policy);
    assert_eq!(names(&f), ["right_issued"]);
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + 5 * DAY));
    assert_eq!(names(&f), ["transfer_proposed"]);
    f.escrow.accept_transfer(&t);
    assert_eq!(names(&f), ["transfer_completed"]);
    f.advance(30 * DAY);
    f.escrow.claim(&r, &f.bob);
    assert_eq!(names(&f), ["right_claimed"]);
    let totals = f.escrow.get_totals(&1);
    assert_eq!(
        (
            totals.funded,
            totals.reserved,
            totals.claimed,
            totals.withdrawn
        ),
        (1000, 0, 600, 0)
    );
    assert_eq!(f.escrow.get_right(&r).owner, f.bob);
    for s in raw.split("\"emits\": [").skip(1) {
        let list = s.split(']').next().unwrap();
        for n in list.split('"').filter(|x| x.contains('_')) {
            assert!(
                serde_like::load().events.iter().any(|e| e == n),
                "unknown event {n}"
            );
        }
    }
}
