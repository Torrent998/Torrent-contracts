#![allow(dead_code)]
//! Shared fixtures. All accounts are generated synthetic addresses; the token is a Stellar Asset
//! Contract registered inside the Soroban test host (no network, no real funds).
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env,
};
use torrent_escrow::{TorrentEscrow, TorrentEscrowClient};

pub const START: u64 = 1_000_000;
pub const PASSPHRASE: &str = "Test SDF Network ; September 2015";
pub const DAY: u64 = 86_400;

pub struct Fixture<'a> {
    pub env: Env,
    pub escrow: TorrentEscrowClient<'a>,
    pub escrow_addr: Address,
    pub token: TokenClient<'a>,
    pub token_admin: StellarAssetClient<'a>,
    pub controller: Address,
    pub alice: Address,
    pub bob: Address,
    pub carol: Address,
    pub outsider: Address,
    pub policy: BytesN<32>,
    pub pool_id: u64,
}

pub fn hash(env: &Env, b: u8) -> BytesN<32> {
    BytesN::from_array(env, &[b; 32])
}

pub fn set_time(env: &Env, ts: u64) {
    env.ledger().with_mut(|l| l.timestamp = ts);
}

/// Pool with controller + alice/bob/carol joined, nothing funded. Auth mocking is OFF; the
/// setup itself runs under `mock_all_auths` and the caller re-enables strictness if needed.
pub fn setup_empty<'a>() -> Fixture<'a> {
    build(false)
}

pub fn setup<'a>() -> Fixture<'a> {
    build(true)
}

fn build<'a>(with_pool: bool) -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, START);
    let nid = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, PASSPHRASE.as_bytes()));
    env.ledger().set_network_id(nid.to_array());
    let sac_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(sac_admin);
    let token = TokenClient::new(&env, &sac.address());
    let token_admin = StellarAssetClient::new(&env, &sac.address());
    let escrow_addr = env.register(TorrentEscrow, ());
    let escrow = TorrentEscrowClient::new(&env, &escrow_addr);
    let controller = Address::generate(&env);
    let [alice, bob, carol, outsider] = core::array::from_fn(|_| Address::generate(&env));
    let policy = hash(&env, 7);
    let mut pool_id = 0;
    if with_pool {
        pool_id = escrow.create_pool(&controller, &sac.address(), &policy);
        for m in [&alice, &bob, &carol] {
            escrow.allow_member(&pool_id, m);
            escrow.join_pool(&pool_id, m, &policy);
        }
    }
    for who in [&controller, &alice, &bob, &carol, &outsider] {
        token_admin.mint(who, &1_000_000);
    }
    Fixture {
        env,
        escrow,
        escrow_addr,
        token,
        token_admin,
        controller,
        alice,
        bob,
        carol,
        outsider,
        policy,
        pool_id,
    }
}

impl<'a> Fixture<'a> {
    pub fn fund(&self, amount: i128) {
        self.escrow
            .deposit(&self.pool_id, &self.controller, &amount);
    }

    /// Issue a right to `owner` maturing `days` from now.
    pub fn issue(&self, owner: &Address, amount: i128, days: u64) -> u64 {
        let maturity = self.env.ledger().timestamp() + days * DAY;
        self.escrow.issue_right(
            &self.pool_id,
            owner,
            &amount,
            &maturity,
            &hash(&self.env, 9),
        )
    }

    pub fn advance(&self, secs: u64) {
        set_time(&self.env, self.env.ledger().timestamp() + secs);
    }
}
