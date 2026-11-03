//! Public on-chain types (ABI v2). See `bindings/abi.v2.json`.
use soroban_sdk::{contracterror, contracttype, Address, BytesN};

/// Version of the public ABI / event schema implemented by this contract.
pub const ABI_VERSION: u32 = 2;

/// Minimum delay between `propose_upgrade` and `execute_upgrade` (7 days, ledger time). Gives
/// members a disclosed window to see the new wasm hash before it can take effect.
pub const UPGRADE_DELAY_SECS: u64 = 7 * 86_400;

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
    /// Pool was closed by its controller; only claims, cancellations, leaving and
    /// withdrawals of unreserved value remain possible.
    PoolClosed = 27,
    AlreadyClosed = 28,
    /// A member who still owns unclaimed rights cannot leave.
    MemberHasRights = 29,
    NoPendingController = 30,
    /// The right has matured: it can be claimed, no longer traded.
    RightMatured = 31,
    NoPendingAdmin = 32,
    UpgradePending = 33,
    NoPendingUpgrade = 34,
    /// The upgrade timelock (`UPGRADE_DELAY_SECS`) has not elapsed.
    UpgradeNotReady = 35,
    /// `lock_upgrades` was called; the code can never change again.
    UpgradesLocked = 36,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PoolStatus {
    Active,
    Paused,
    /// Terminal. No new funding, members, rights or transfers; claims always work.
    Closed,
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
    /// Number of unclaimed rights this member currently owns in the pool.
    pub open_rights: u32,
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

/// A proposed code upgrade, executable by the admin once `executable_after` has passed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub wasm_hash: BytesN<32>,
    pub proposed_at: u64,
    pub executable_after: u64,
}

/// Contract-level governance. The admin has no power over pools, rights or funds: it can only
/// rotate itself (two-step), propose/cancel/execute a timelocked upgrade, or lock upgrades forever.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Governance {
    pub admin: Address,
    pub pending_admin: Option<Address>,
    /// Wasm hash of the pending upgrade, if any.
    pub pending_upgrade: Option<BytesN<32>>,
    /// Earliest ledger time the pending upgrade can execute (0 when none is pending).
    pub upgrade_executable_after: u64,
    pub upgrades_locked: bool,
    pub upgrade_delay_secs: u64,
}

/// Monotonic id counters (the highest id handed out so far; 0 = none).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Counts {
    pub pools: u64,
    pub rights: u64,
    pub transfers: u64,
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
    PendingController(u64),
    Admin,
    PendingAdmin,
    PendingUpgrade,
    UpgradesLocked,
}
