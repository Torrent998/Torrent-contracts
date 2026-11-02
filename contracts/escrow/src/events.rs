//! Contract events (schema v1). Every event carries `network_id` (sha256 of the network
//! passphrase) so consumers can fail closed on the wrong network. The emitting contract id is
//! part of the Soroban event envelope. No personal data is ever emitted.
use soroban_sdk::{contractevent, Address, BytesN};

#[contractevent]
pub struct PoolCreated {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
    pub controller: Address,
    pub token: Address,
    pub membership_policy_hash: BytesN<32>,
    pub start_ledger: u32,
}

#[contractevent]
pub struct MemberJoined {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
    pub member: Address,
}

#[contractevent]
pub struct Funded {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
    pub receipt_id: u64,
    pub from: Address,
    pub amount: i128,
    pub funded_total: i128,
}

#[contractevent]
pub struct RightIssued {
    #[topic]
    pub pool_id: u64,
    #[topic]
    pub right_id: u64,
    pub network_id: BytesN<32>,
    pub owner: Address,
    pub amount: i128,
    pub maturity: u64,
    pub reserved_total: i128,
}

#[contractevent]
pub struct TransferProposed {
    #[topic]
    pub right_id: u64,
    #[topic]
    pub transfer_id: u64,
    pub network_id: BytesN<32>,
    pub from: Address,
    pub to: Address,
    pub expiry: u64,
}

#[contractevent]
pub struct TransferCompleted {
    #[topic]
    pub right_id: u64,
    #[topic]
    pub transfer_id: u64,
    pub network_id: BytesN<32>,
    pub from: Address,
    pub to: Address,
}

#[contractevent]
pub struct TransferCancelled {
    #[topic]
    pub right_id: u64,
    #[topic]
    pub transfer_id: u64,
    pub network_id: BytesN<32>,
}

#[contractevent]
pub struct RightClaimed {
    #[topic]
    pub pool_id: u64,
    #[topic]
    pub right_id: u64,
    pub network_id: BytesN<32>,
    pub owner: Address,
    pub recipient: Address,
    pub amount: i128,
    pub claimed_total: i128,
}

#[contractevent]
pub struct UnreservedWithdrawn {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
    pub to: Address,
    pub amount: i128,
    pub withdrawn_total: i128,
}

#[contractevent]
pub struct PoolPaused {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
}

#[contractevent]
pub struct PoolUnpaused {
    #[topic]
    pub pool_id: u64,
    pub network_id: BytesN<32>,
}
