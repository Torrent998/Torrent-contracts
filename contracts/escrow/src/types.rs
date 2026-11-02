//! Public on-chain types (ABI v1). See `bindings/abi.v1.json`.
use soroban_sdk::{contracterror, contracttype, Address, BytesN};

/// Version of the public ABI / event schema implemented by this contract.
pub const ABI_VERSION: u32 = 1;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    PoolNotFound = 1,
    RightNotFound = 2,
    TransferNotFound = 3,
    NotAllowed = 4,
    NotMember = 5,
    AlreadyMember = 6,
    PolicyMismatch = 7,
    InvalidAmount = 8,
    PoolPaused = 9,
    /// Issuing would make `reserved > funded - claimed - withdrawn`.
    InsufficientBacking = 10,
    MaturityInPast = 11,
    NotMatured = 12,
    AlreadyClaimed = 13,
    RightNotActive = 14,
    SameOwner = 15,
    InvalidExpiry = 16,
    TransferPending = 17,
    TransferNotPending = 18,
    TransferExpired = 19,
    /// Right changed owner since the transfer was proposed.
    StaleTransfer = 20,
    NotFunded = 21,
    Overflow = 22,
    /// Token moved a different amount than requested (fee-on-transfer etc.). Fail closed.
    DepositMismatch = 23,
    MembershipFrozen = 24,
    AlreadyPaused = 25,
    NotPaused = 26,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PoolStatus {
    Active,
    Paused,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConfig {
    pub pool_id: u64,
    pub controller: Address,
    pub token: Address,
    pub membership_policy_hash: BytesN<32>,
    pub start_ledger: u32,
    pub status: PoolStatus,
}

/// All values are integer token base units.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct EscrowTotals {
    /// Cumulative amount deposited.
    pub funded: i128,
    /// Sum of amounts of rights issued and not yet claimed.
    pub reserved: i128,
    /// Cumulative amount paid out through claims.
    pub claimed: i128,
    /// Cumulative unreserved funds returned to the controller.
    pub withdrawn: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    pub pool_id: u64,
    pub address: Address,
    pub eligible: bool,
    pub joined_at: u64,
    pub signed_policy_hash: BytesN<32>,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RightState {
    Issued,
    Claimed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutRight {
    pub right_id: u64,
    pub pool_id: u64,
    pub amount: i128,
    pub owner: Address,
    pub maturity: u64,
    pub transfer_policy_hash: BytesN<32>,
    pub state: RightState,
    /// At most one open transfer per right.
    pub pending_transfer: Option<u64>,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TransferStatus {
    Pending,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferApproval {
    pub transfer_id: u64,
    pub right_id: u64,
    pub from: Address,
    pub to: Address,
    pub expiry: u64,
    pub status: TransferStatus,
}

#[contracttype]
#[derive(Clone)]
pub(crate) enum DataKey {
    PoolCount,
    RightCount,
    TransferCount,
    Pool(u64),
    Totals(u64),
    Receipts(u64),
    Allowed(u64, Address),
    Member(u64, Address),
    Right(u64),
    Transfer(u64),
}
