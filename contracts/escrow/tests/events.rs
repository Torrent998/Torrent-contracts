//! Event schema v2 golden test. `bindings/events.v2.json` is the cross-repo contract consumed by
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
        "pool_closed",
        "member_removed",
        "member_left",
        "controller_proposed",
        "controller_changed",
        "admin_proposed",
        "admin_changed",
        "upgrade_proposed",
        "upgrade_cancelled",
        "contract_upgraded",
        "upgrades_locked",
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
    f.escrow.propose_transfer(&r2, &f.bob, &(START + 2 * DAY));
    f.escrow.cancel_transfer(&2);
    assert_eq!(names(&f), ["transfer_cancelled"]);
    // v2: an offer voided by a claim is announced before the claim
    let t3 = f.escrow.propose_transfer(&r2, &f.alice, &(START + 2 * DAY));
    assert_eq!(t3, 3);
    f.advance(DAY);
    f.escrow.claim(&r2, &f.carol);
    assert_eq!(names(&f), ["transfer_cancelled", "right_claimed"]);
    // v2: replacing an expired offer announces its cancellation first
    f.fund(10);
    assert_eq!(names(&f), ["funded"]);
    let r3 = f.issue(&f.alice, 1, 3);
    f.escrow
        .propose_transfer(&r3, &f.bob, &(f.env.ledger().timestamp() + DAY));
    f.advance(DAY);
    f.escrow
        .propose_transfer(&r3, &f.bob, &(f.env.ledger().timestamp() + DAY));
    assert_eq!(names(&f), ["transfer_cancelled", "transfer_proposed"]);
    f.escrow.leave_pool(&f.pool_id, &f.carol);
    assert_eq!(names(&f), ["member_left"]);
    f.escrow.propose_controller(&f.pool_id, &f.outsider);
    assert_eq!(names(&f), ["controller_proposed"]);
    f.escrow.accept_controller(&f.pool_id);
    assert_eq!(names(&f), ["controller_changed"]);
    f.escrow.close_pool(&f.pool_id);
    assert_eq!(names(&f), ["pool_closed"]);
    let p2 = f
        .escrow
        .create_pool(&f.controller, &f.token.address, &f.policy);
    f.escrow.allow_member(&p2, &f.alice);
    f.escrow.remove_member(&p2, &f.alice);
    assert_eq!(names(&f), ["member_removed"]);
    f.escrow.propose_admin(&f.outsider);
    assert_eq!(names(&f), ["admin_proposed"]);
    f.escrow.accept_admin();
    assert_eq!(names(&f), ["admin_changed"]);
    f.escrow.propose_upgrade(&hash(&f.env, 1));
    assert_eq!(names(&f), ["upgrade_proposed"]);
    f.escrow.cancel_upgrade();
    assert_eq!(names(&f), ["upgrade_cancelled"]);
    f.escrow.lock_upgrades();
    assert_eq!(names(&f), ["upgrades_locked"]);
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
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bindings/events.v2.json");
        let s = std::fs::read_to_string(p).expect("bindings/events.v2.json");
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
    // Mirrors bindings/scenario.v2.json step by step and checks the per-step event names.
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bindings/scenario.v2.json"
    ))
    .unwrap();
    assert!(raw.contains("\"abiVersion\": 2"));
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

/// Records the REAL emitted events of the synthetic scenario as `bindings/stream.v2.json`
/// (base64 XDR topics/value). `torrent-indexer` decodes this exact stream in its tests, so the
/// two repos are checked against the same bytes, not against each other's descriptions.
/// Regenerate with `UPDATE_GOLDEN=1 cargo test --test events stream`.
#[test]
fn stream_golden_matches_real_emitted_events() {
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::xdr::{Limits, WriteXdr};
    use soroban_sdk::{IntoVal, TryFromVal};
    let f = setup_empty();
    let mut entries: Vec<String> = Vec::new();
    let mut ledger = 100u32;
    let mut capture = |f: &Fixture, step: &str| {
        ledger += 1;
        let ts = f.env.ledger().timestamp();
        for (i, e) in f
            .env
            .events()
            .all()
            .filter_by_contract(&f.escrow_addr)
            .events()
            .iter()
            .enumerate()
        {
            let ContractEventBody::V0(b) = &e.body;
            let topics: Vec<String> = b
                .topics
                .iter()
                .map(|t| format!("\"{}\"", t.to_xdr_base64(Limits::none()).unwrap()))
                .collect();
            entries.push(format!(
                "    {{ \"step\": \"{step}\", \"ledger\": {ledger}, \"closeTime\": {ts}, \"eventIndex\": {i}, \"topics\": [{}], \"value\": \"{}\" }}",
                topics.join(", "),
                b.data.to_xdr_base64(Limits::none()).unwrap()
            ));
        }
        f.env.ledger().with_mut(|l| l.sequence_number = ledger + 1);
    };
    f.env.ledger().with_mut(|l| l.sequence_number = 101);
    f.escrow
        .create_pool(&f.controller, &f.token.address, &f.policy);
    capture(&f, "create_pool");
    for (n, who) in [("join_alice", &f.alice), ("join_bob", &f.bob)] {
        f.escrow.allow_member(&1, who);
        f.escrow.join_pool(&1, who, &f.policy);
        capture(&f, n);
    }
    f.escrow.deposit(&1, &f.controller, &1000);
    capture(&f, "deposit");
    let r = f
        .escrow
        .issue_right(&1, &f.alice, &600, &(START + 30 * DAY), &f.policy);
    capture(&f, "issue_right");
    let t = f.escrow.propose_transfer(&r, &f.bob, &(START + 5 * DAY));
    capture(&f, "propose_transfer");
    f.escrow.accept_transfer(&t);
    capture(&f, "accept_transfer");
    f.advance(30 * DAY);
    f.escrow.claim(&r, &f.bob);
    capture(&f, "claim");
    // Second right to exercise cancel + withdraw + pause events.
    let r2 = f
        .escrow
        .issue_right(&1, &f.alice, &100, &(START + 90 * DAY), &f.policy);
    capture(&f, "issue_right_2");
    let t2 = f.escrow.propose_transfer(&r2, &f.bob, &(START + 40 * DAY));
    capture(&f, "propose_transfer_2");
    f.escrow.cancel_transfer(&t2);
    capture(&f, "cancel_transfer_2");
    f.escrow.withdraw_unreserved(&1, &f.controller, &300);
    capture(&f, "withdraw_unreserved");
    f.escrow.pause_pool(&1);
    capture(&f, "pause");
    f.escrow.unpause_pool(&1);
    capture(&f, "unpause");

    // ---- v2 events ----
    let now = |f: &Fixture| f.env.ledger().timestamp();
    f.escrow.deposit(&1, &f.alice, &200);
    capture(&f, "deposit_2");
    let r3 = f
        .escrow
        .issue_right(&1, &f.alice, &50, &(now(&f) + 5 * DAY), &f.policy);
    capture(&f, "issue_right_3");
    f.escrow.propose_transfer(&r3, &f.bob, &(now(&f) + DAY));
    capture(&f, "propose_transfer_3");
    f.advance(2 * DAY);
    f.escrow.propose_transfer(&r3, &f.bob, &(now(&f) + DAY));
    capture(&f, "replace_expired_transfer_3");
    f.advance(3 * DAY);
    f.escrow.claim(&r3, &f.alice);
    capture(&f, "claim_3_voids_offer");
    f.escrow.leave_pool(&1, &f.bob);
    capture(&f, "leave_bob");
    f.escrow.propose_controller(&1, &f.outsider);
    capture(&f, "propose_controller");
    f.escrow.accept_controller(&1);
    capture(&f, "accept_controller");
    f.escrow.close_pool(&1);
    capture(&f, "close_pool");
    f.escrow
        .create_pool(&f.controller, &f.token.address, &f.policy);
    capture(&f, "create_pool_2");
    f.escrow.allow_member(&2, &f.carol);
    f.escrow.join_pool(&2, &f.carol, &f.policy);
    capture(&f, "join_carol_2");
    f.escrow.remove_member(&2, &f.carol);
    capture(&f, "remove_carol_2");
    f.escrow.propose_admin(&f.carol);
    capture(&f, "propose_admin");
    f.escrow.accept_admin();
    capture(&f, "accept_admin");
    // `contract_upgraded` is not recorded here: executing an upgrade needs real wasm bytes,
    // whose hash depends on the compiler. tests/governance.rs covers it with the real wasm.
    f.escrow.propose_upgrade(&hash(&f.env, 0xab));
    capture(&f, "propose_upgrade");
    f.escrow.cancel_upgrade();
    capture(&f, "cancel_upgrade");
    f.escrow.propose_upgrade(&hash(&f.env, 0xcd));
    capture(&f, "propose_upgrade_2");
    f.escrow.lock_upgrades();
    capture(&f, "lock_upgrades_cancels_pending");

    // Recorded view-function results at the end of the scenario (what an RPC simulate returns).
    let to_b64 = |v: soroban_sdk::Val| {
        soroban_sdk::xdr::ScVal::try_from_val(&f.env, &v)
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap()
    };
    let t_totals = to_b64(f.escrow.get_totals(&1).into_val(&f.env));
    let t_r1 = to_b64(f.escrow.get_right(&1).into_val(&f.env));
    let t_r2 = to_b64(f.escrow.get_right(&2).into_val(&f.env));
    let t_pool = to_b64(f.escrow.get_pool(&1).into_val(&f.env));
    let t_member = to_b64(f.escrow.get_member(&1, &f.alice).into_val(&f.env));
    let t_gov = to_b64(f.escrow.get_governance().into_val(&f.env));
    let t_counts = to_b64(f.escrow.get_counts().into_val(&f.env));
    let views = format!(
        "{{ \"get_totals(1)\": \"{t_totals}\", \"get_right(1)\": \"{t_r1}\", \"get_right(2)\": \"{t_r2}\", \"get_pool(1)\": \"{t_pool}\", \"get_member(1,alice)\": \"{t_member}\", \"get_governance()\": \"{t_gov}\", \"get_counts()\": \"{t_counts}\" }}"
    );
    let doc = format!(
        "{{\n  \"schema\": \"torrent.event-stream\",\n  \"abiVersion\": 2,\n  \"networkPassphrase\": \"{PASSPHRASE}\",\n  \"contractId\": \"{}\",\n  \"note\": \"Real events emitted by torrent-escrow in the Soroban test host with synthetic accounts. NOT from Testnet.\",\n  \"views\": {},\n  \"events\": [\n{}\n  ]\n}}\n",
        f.escrow_addr.to_string(),
        views,
        entries.join(",\n")
    );
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bindings/stream.v2.json");
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(path, &doc).unwrap();
    }
    let on_disk = std::fs::read_to_string(path).expect("run with UPDATE_GOLDEN=1 once");
    assert_eq!(
        on_disk, doc,
        "bindings/stream.v2.json is stale; regenerate with UPDATE_GOLDEN=1"
    );
}
