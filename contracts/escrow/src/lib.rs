//! Torrent escrow and payout-rights contract (Soroban).
//!
//! Core invariant, enforced after every state change of every pool:
//! `reserved <= funded - claimed - withdrawn` (and `reserved >= 0`), i.e. every outstanding
//! payout right is backed by tokens actually held by this contract.
//!
//! Testnet/synthetic only. Not audited. See `docs/SECURITY.md` and `docs/adr/`.
#![no_std]

mod events;
mod types;

pub use events::*;
pub use types::*;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, ContractExecutable, Env};
use types::DataKey;

const DAY_IN_LEDGERS: u32 = 17_280;
const TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const TTL_EXTEND_TO: u32 = 60 * DAY_IN_LEDGERS;

#[contract]
pub struct TorrentEscrow;

// ---- storage helpers -------------------------------------------------------------------

fn load<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey) -> Option<V> {
    let v = env.storage().persistent().get(key);
    if v.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    v
}

fn save<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey, v: &V) {
    env.storage().persistent().set(key, v);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn next_id(env: &Env, key: DataKey) -> u64 {
    let n: u64 = env.storage().instance().get(&key).unwrap_or(0) + 1;
    env.storage().instance().set(&key, &n);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    n
}

fn pool(env: &Env, id: u64) -> Result<PoolConfig, Error> {
    load(env, &DataKey::Pool(id)).ok_or(Error::PoolNotFound)
}

fn totals(env: &Env, id: u64) -> EscrowTotals {
    load(env, &DataKey::Totals(id)).unwrap_or_default()
}

fn right(env: &Env, id: u64) -> Result<PayoutRight, Error> {
    load(env, &DataKey::Right(id)).ok_or(Error::RightNotFound)
}

fn transfer(env: &Env, id: u64) -> Result<TransferApproval, Error> {
    load(env, &DataKey::Transfer(id)).ok_or(Error::TransferNotFound)
}

fn active_pool(env: &Env, id: u64) -> Result<PoolConfig, Error> {
    let p = pool(env, id)?;
    match p.status {
        PoolStatus::Active => Ok(p),
        PoolStatus::Paused => Err(Error::PoolPaused),
        PoolStatus::Closed => Err(Error::PoolClosed),
    }
}

fn member(env: &Env, pool_id: u64, who: &Address) -> Option<Member> {
    load::<Member>(env, &DataKey::Member(pool_id, who.clone()))
}

fn is_eligible(env: &Env, pool_id: u64, who: &Address) -> bool {
    member(env, pool_id, who)
        .map(|m| m.eligible)
        .unwrap_or(false)
}

/// Adjusts a member's count of owned, unclaimed rights (+1 on receipt, -1 on hand-over/claim).
fn bump_open_rights(env: &Env, pool_id: u64, who: &Address, up: bool) -> Result<(), Error> {
    let mut m = member(env, pool_id, who).ok_or(Error::NotMember)?;
    m.open_rights = if up {
        m.open_rights.checked_add(1).ok_or(Error::Overflow)?
    } else {
        m.open_rights.checked_sub(1).ok_or(Error::Overflow)?
    };
    save(env, &DataKey::Member(pool_id, who.clone()), &m);
    Ok(())
}

fn instance_get<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Option<V> {
    env.storage().instance().get(key)
}

fn instance_set<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey, v: &V) {
    env.storage().instance().set(key, v);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn admin(env: &Env) -> Address {
    // Set by the constructor; a deployed instance always has one.
    instance_get(env, &DataKey::Admin).expect("admin set at construction")
}

fn upgrades_locked(env: &Env) -> bool {
    instance_get(env, &DataKey::UpgradesLocked).unwrap_or(false)
}

/// Marks an open proposal as cancelled and tells the indexer (v2: no silent state changes).
fn void_transfer(env: &Env, transfer_id: u64) -> Result<(), Error> {
    let mut tr = transfer(env, transfer_id)?;
    if tr.status != TransferStatus::Pending {
        return Ok(());
    }
    tr.status = TransferStatus::Cancelled;
    save(env, &DataKey::Transfer(transfer_id), &tr);
    TransferCancelled {
        right_id: tr.right_id,
        transfer_id,
        network_id: net(env),
    }
    .publish(env);
    Ok(())
}

fn add(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

fn sub(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

/// Tokens in escrow not yet paid out or withdrawn.
fn held(t: &EscrowTotals) -> Result<i128, Error> {
    sub(sub(t.funded, t.claimed)?, t.withdrawn)
}

/// The overcollateralization invariant. Called before persisting any totals change.
fn assert_backed(t: &EscrowTotals) -> Result<(), Error> {
    if t.reserved < 0 || t.reserved > held(t)? {
        return Err(Error::InsufficientBacking);
    }
    Ok(())
}

fn save_totals(env: &Env, id: u64, t: &EscrowTotals) -> Result<(), Error> {
    assert_backed(t)?;
    save(env, &DataKey::Totals(id), t);
    Ok(())
}

fn net(env: &Env) -> BytesN<32> {
    env.ledger().network_id()
}

fn now(env: &Env) -> u64 {
    env.ledger().timestamp()
}

#[contractimpl]
impl TorrentEscrow {
    /// Runs once at deployment. `admin` may only rotate itself and manage timelocked upgrades;
    /// it has no authority over any pool, right or token.
    pub fn __constructor(env: Env, admin: Address) {
        instance_set(&env, &DataKey::Admin, &admin);
    }

    /// ABI / event schema version.
    pub fn abi_version(_env: Env) -> u32 {
        ABI_VERSION
    }

    /// Create a pool. Membership policy is fixed forever by `membership_policy_hash`.
    pub fn create_pool(
        env: Env,
        controller: Address,
        token: Address,
        membership_policy_hash: BytesN<32>,
    ) -> Result<u64, Error> {
        controller.require_auth();
        let pool_id = next_id(&env, DataKey::PoolCount);
        let cfg = PoolConfig {
            pool_id,
            controller: controller.clone(),
            token: token.clone(),
            membership_policy_hash: membership_policy_hash.clone(),
            start_ledger: env.ledger().sequence(),
            status: PoolStatus::Active,
        };
        save(&env, &DataKey::Pool(pool_id), &cfg);
        save(&env, &DataKey::Totals(pool_id), &EscrowTotals::default());
        PoolCreated {
            pool_id,
            network_id: net(&env),
            controller,
            token,
            membership_policy_hash,
            start_ledger: cfg.start_ledger,
        }
        .publish(&env);
        Ok(pool_id)
    }

    /// Controller pre-approves an eligible member address. Only allowed before first funding:
    /// membership rules cannot be changed unilaterally once value is escrowed.
    pub fn allow_member(env: Env, pool_id: u64, member: Address) -> Result<(), Error> {
        let p = active_pool(&env, pool_id)?;
        p.controller.require_auth();
        if totals(&env, pool_id).funded > 0 {
            return Err(Error::MembershipFrozen);
        }
        save(&env, &DataKey::Allowed(pool_id, member), &true);
        Ok(())
    }

    /// Member consents to the pool's policy. Requires prior `allow_member` by the controller.
    pub fn join_pool(
        env: Env,
        pool_id: u64,
        member: Address,
        signed_policy_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let p = active_pool(&env, pool_id)?;
        member.require_auth();
        if signed_policy_hash != p.membership_policy_hash {
            return Err(Error::PolicyMismatch);
        }
        let allowed: Option<bool> = load(&env, &DataKey::Allowed(pool_id, member.clone()));
        if allowed != Some(true) {
            return Err(Error::NotAllowed);
        }
        if load::<Member>(&env, &DataKey::Member(pool_id, member.clone())).is_some() {
            return Err(Error::AlreadyMember);
        }
        let m = Member {
            pool_id,
            address: member.clone(),
            eligible: true,
            joined_at: now(&env),
            signed_policy_hash,
            open_rights: 0,
        };
        save(&env, &DataKey::Member(pool_id, member.clone()), &m);
        MemberJoined {
            pool_id,
            network_id: net(&env),
            member,
        }
        .publish(&env);
        Ok(())
    }

    /// Deposit pool tokens into escrow. `from` must be the controller or a joined member.
    /// Fails closed if the token moves a different amount than requested.
    pub fn deposit(env: Env, pool_id: u64, from: Address, amount: i128) -> Result<u64, Error> {
        let p = active_pool(&env, pool_id)?;
        from.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if from != p.controller && !is_eligible(&env, pool_id, &from) {
            return Err(Error::NotMember);
        }
        let client = token::Client::new(&env, &p.token);
        let me = env.current_contract_address();
        let before = client.balance(&me);
        client.transfer(&from, &me, &amount);
        if sub(client.balance(&me), before)? != amount {
            return Err(Error::DepositMismatch);
        }
        let mut t = totals(&env, pool_id);
        t.funded = add(t.funded, amount)?;
        save_totals(&env, pool_id, &t)?;
        let receipt_id: u64 = load(&env, &DataKey::Receipts(pool_id)).unwrap_or(0u64) + 1;
        save(&env, &DataKey::Receipts(pool_id), &receipt_id);
        Funded {
            pool_id,
            network_id: net(&env),
            receipt_id,
            from,
            amount,
            funded_total: t.funded,
        }
        .publish(&env);
        Ok(receipt_id)
    }

    /// Controller issues a right to an eligible member, reserving `amount` of funded value.
    /// Refused if reserved rights would exceed unallocated funded value.
    pub fn issue_right(
        env: Env,
        pool_id: u64,
        owner: Address,
        amount: i128,
        maturity: u64,
        policy_hash: BytesN<32>,
    ) -> Result<u64, Error> {
        let p = active_pool(&env, pool_id)?;
        p.controller.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if maturity <= now(&env) {
            return Err(Error::MaturityInPast);
        }
        if !is_eligible(&env, pool_id, &owner) {
            return Err(Error::NotMember);
        }
        let mut t = totals(&env, pool_id);
        t.reserved = add(t.reserved, amount)?;
        save_totals(&env, pool_id, &t)?; // enforces reserved <= held
        let right_id = next_id(&env, DataKey::RightCount);
        let r = PayoutRight {
            right_id,
            pool_id,
            amount,
            owner: owner.clone(),
            maturity,
            transfer_policy_hash: policy_hash,
            state: RightState::Issued,
            pending_transfer: None,
        };
        save(&env, &DataKey::Right(right_id), &r);
        bump_open_rights(&env, pool_id, &owner, true)?;
        RightIssued {
            pool_id,
            right_id,
            network_id: net(&env),
            owner,
            amount,
            maturity,
            reserved_total: t.reserved,
        }
        .publish(&env);
        Ok(right_id)
    }

    /// Current owner proposes handing the right to another eligible member.
    pub fn propose_transfer(
        env: Env,
        right_id: u64,
        new_owner: Address,
        expiry: u64,
    ) -> Result<u64, Error> {
        let mut r = right(&env, right_id)?;
        active_pool(&env, r.pool_id)?;
        r.owner.require_auth();
        if r.state != RightState::Issued {
            return Err(Error::RightNotActive);
        }
        if new_owner == r.owner {
            return Err(Error::SameOwner);
        }
        if !is_eligible(&env, r.pool_id, &new_owner) {
            return Err(Error::NotMember);
        }
        // Timing rules: a matured right is for claiming, not trading, and an offer may not
        // outlive the maturity of the right it offers.
        if now(&env) >= r.maturity {
            return Err(Error::RightMatured);
        }
        if expiry <= now(&env) || expiry > r.maturity {
            return Err(Error::InvalidExpiry);
        }
        if let Some(open) = r.pending_transfer {
            let old = transfer(&env, open)?;
            if old.status == TransferStatus::Pending && old.expiry > now(&env) {
                return Err(Error::TransferPending);
            }
            // An expired proposal is replaced; v2 emits its cancellation explicitly.
            void_transfer(&env, open)?;
        }
        let transfer_id = next_id(&env, DataKey::TransferCount);
        let tr = TransferApproval {
            transfer_id,
            right_id,
            from: r.owner.clone(),
            to: new_owner.clone(),
            expiry,
            status: TransferStatus::Pending,
        };
        save(&env, &DataKey::Transfer(transfer_id), &tr);
        r.pending_transfer = Some(transfer_id);
        save(&env, &DataKey::Right(right_id), &r);
        TransferProposed {
            right_id,
            transfer_id,
            network_id: net(&env),
            from: tr.from,
            to: new_owner,
            expiry,
        }
        .publish(&env);
        Ok(transfer_id)
    }

    /// Current owner withdraws a pending proposal.
    pub fn cancel_transfer(env: Env, transfer_id: u64) -> Result<(), Error> {
        let mut tr = transfer(&env, transfer_id)?;
        tr.from.require_auth();
        if tr.status != TransferStatus::Pending {
            return Err(Error::TransferNotPending);
        }
        tr.status = TransferStatus::Cancelled;
        save(&env, &DataKey::Transfer(transfer_id), &tr);
        let mut r = right(&env, tr.right_id)?;
        if r.pending_transfer == Some(transfer_id) {
            r.pending_transfer = None;
            save(&env, &DataKey::Right(tr.right_id), &r);
        }
        TransferCancelled {
            right_id: tr.right_id,
            transfer_id,
            network_id: net(&env),
        }
        .publish(&env);
        Ok(())
    }

    /// Recipient accepts. The transfer id is single-use (status flips to `Completed`), which
    /// prevents replay; the owner is re-checked to reject stale proposals.
    pub fn accept_transfer(env: Env, transfer_id: u64) -> Result<(), Error> {
        let mut tr = transfer(&env, transfer_id)?;
        let mut r = right(&env, tr.right_id)?;
        active_pool(&env, r.pool_id)?;
        tr.to.require_auth();
        if tr.status != TransferStatus::Pending {
            return Err(Error::TransferNotPending);
        }
        if tr.expiry <= now(&env) {
            return Err(Error::TransferExpired);
        }
        if r.state != RightState::Issued {
            return Err(Error::RightNotActive);
        }
        if now(&env) >= r.maturity {
            return Err(Error::RightMatured);
        }
        if r.owner != tr.from || r.pending_transfer != Some(transfer_id) {
            return Err(Error::StaleTransfer);
        }
        if !is_eligible(&env, r.pool_id, &tr.to) {
            return Err(Error::NotMember);
        }
        bump_open_rights(&env, r.pool_id, &tr.from, false)?;
        bump_open_rights(&env, r.pool_id, &tr.to, true)?;
        r.owner = tr.to.clone();
        r.pending_transfer = None;
        tr.status = TransferStatus::Completed;
        save(&env, &DataKey::Right(tr.right_id), &r);
        save(&env, &DataKey::Transfer(transfer_id), &tr);
        TransferCompleted {
            right_id: tr.right_id,
            transfer_id,
            network_id: net(&env),
            from: tr.from,
            to: tr.to,
        }
        .publish(&env);
        Ok(())
    }

    /// Owner claims a matured right exactly once. Works while the pool is paused: pausing never
    /// traps funds that back an issued right.
    pub fn claim(env: Env, right_id: u64, recipient: Address) -> Result<(), Error> {
        let mut r = right(&env, right_id)?;
        r.owner.require_auth();
        if r.state == RightState::Claimed {
            return Err(Error::AlreadyClaimed);
        }
        if now(&env) < r.maturity {
            return Err(Error::NotMatured);
        }
        let p = pool(&env, r.pool_id)?;
        // Effects before interaction.
        let mut t = totals(&env, r.pool_id);
        t.reserved = sub(t.reserved, r.amount)?;
        t.claimed = add(t.claimed, r.amount)?;
        save_totals(&env, r.pool_id, &t)?;
        if let Some(open) = r.pending_transfer {
            void_transfer(&env, open)?;
            r.pending_transfer = None;
        }
        r.state = RightState::Claimed;
        save(&env, &DataKey::Right(right_id), &r);
        bump_open_rights(&env, r.pool_id, &r.owner, false)?;
        token::Client::new(&env, &p.token).transfer(
            &env.current_contract_address(),
            &recipient,
            &r.amount,
        );
        RightClaimed {
            pool_id: r.pool_id,
            right_id,
            network_id: net(&env),
            owner: r.owner,
            recipient,
            amount: r.amount,
            claimed_total: t.claimed,
        }
        .publish(&env);
        Ok(())
    }

    /// Controller recovers only funds not reserved for any issued right. Allowed while the pool
    /// is active or closed (to settle leftovers), not while paused.
    pub fn withdraw_unreserved(
        env: Env,
        pool_id: u64,
        to: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let p = pool(&env, pool_id)?;
        if p.status == PoolStatus::Paused {
            return Err(Error::PoolPaused);
        }
        p.controller.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut t = totals(&env, pool_id);
        t.withdrawn = add(t.withdrawn, amount)?;
        save_totals(&env, pool_id, &t)?; // fails if it would eat into reserved value
        token::Client::new(&env, &p.token).transfer(&env.current_contract_address(), &to, &amount);
        UnreservedWithdrawn {
            pool_id,
            network_id: net(&env),
            to,
            amount,
            withdrawn_total: t.withdrawn,
        }
        .publish(&env);
        Ok(())
    }

    /// Stops deposits, issuance, withdrawals and transfers. Claims remain possible.
    pub fn pause_pool(env: Env, pool_id: u64) -> Result<(), Error> {
        let mut p = pool(&env, pool_id)?;
        p.controller.require_auth();
        match p.status {
            PoolStatus::Paused => return Err(Error::AlreadyPaused),
            PoolStatus::Closed => return Err(Error::PoolClosed),
            PoolStatus::Active => {}
        }
        p.status = PoolStatus::Paused;
        save(&env, &DataKey::Pool(pool_id), &p);
        PoolPaused {
            pool_id,
            network_id: net(&env),
        }
        .publish(&env);
        Ok(())
    }

    pub fn unpause_pool(env: Env, pool_id: u64) -> Result<(), Error> {
        let mut p = pool(&env, pool_id)?;
        p.controller.require_auth();
        if p.status != PoolStatus::Paused {
            return Err(Error::NotPaused);
        }
        p.status = PoolStatus::Active;
        save(&env, &DataKey::Pool(pool_id), &p);
        PoolUnpaused {
            pool_id,
            network_id: net(&env),
        }
        .publish(&env);
        Ok(())
    }

    /// Permanently closes a pool (from Active or Paused). Afterwards nothing new can enter it:
    /// no deposits, members, rights or transfers. Every issued right stays claimable at
    /// maturity and unreserved value stays withdrawable, so closing never confiscates.
    pub fn close_pool(env: Env, pool_id: u64) -> Result<(), Error> {
        let mut p = pool(&env, pool_id)?;
        p.controller.require_auth();
        if p.status == PoolStatus::Closed {
            return Err(Error::AlreadyClosed);
        }
        p.status = PoolStatus::Closed;
        save(&env, &DataKey::Pool(pool_id), &p);
        PoolClosed {
            pool_id,
            network_id: net(&env),
        }
        .publish(&env);
        Ok(())
    }

    /// Controller withdraws an allow-list entry or a joined member. Only before the first
    /// deposit: after funding, membership can change only by the member's own `leave_pool`.
    pub fn remove_member(env: Env, pool_id: u64, member: Address) -> Result<(), Error> {
        let p = active_pool(&env, pool_id)?;
        p.controller.require_auth();
        if totals(&env, pool_id).funded > 0 {
            return Err(Error::MembershipFrozen);
        }
        let allowed_key = DataKey::Allowed(pool_id, member.clone());
        let member_key = DataKey::Member(pool_id, member.clone());
        let store = env.storage().persistent();
        if !store.has(&allowed_key) && !store.has(&member_key) {
            return Err(Error::NotAllowed);
        }
        store.remove(&allowed_key);
        store.remove(&member_key);
        MemberRemoved {
            pool_id,
            network_id: net(&env),
            member,
        }
        .publish(&env);
        Ok(())
    }

    /// A member leaves on their own signature. Refused while they own any unclaimed right
    /// (hand it over or claim it first). Their past deposits stay in the pool's escrow.
    pub fn leave_pool(env: Env, pool_id: u64, member: Address) -> Result<(), Error> {
        pool(&env, pool_id)?;
        member.require_auth();
        let mut m = self::member(&env, pool_id, &member)
            .filter(|m| m.eligible)
            .ok_or(Error::NotMember)?;
        if m.open_rights > 0 {
            return Err(Error::MemberHasRights);
        }
        m.eligible = false;
        save(&env, &DataKey::Member(pool_id, member.clone()), &m);
        env.storage()
            .persistent()
            .remove(&DataKey::Allowed(pool_id, member.clone()));
        MemberLeft {
            pool_id,
            network_id: net(&env),
            member,
        }
        .publish(&env);
        Ok(())
    }

    /// Step 1 of controller rotation: the current controller names a successor.
    pub fn propose_controller(env: Env, pool_id: u64, proposed: Address) -> Result<(), Error> {
        let p = pool(&env, pool_id)?;
        p.controller.require_auth();
        if proposed == p.controller {
            return Err(Error::SameOwner);
        }
        save(&env, &DataKey::PendingController(pool_id), &proposed);
        ControllerProposed {
            pool_id,
            network_id: net(&env),
            controller: p.controller,
            proposed,
        }
        .publish(&env);
        Ok(())
    }

    /// Step 2: the named successor accepts with its own signature.
    pub fn accept_controller(env: Env, pool_id: u64) -> Result<(), Error> {
        let mut p = pool(&env, pool_id)?;
        let key = DataKey::PendingController(pool_id);
        let next: Address = load(&env, &key).ok_or(Error::NoPendingController)?;
        next.require_auth();
        env.storage().persistent().remove(&key);
        let previous = p.controller.clone();
        p.controller = next.clone();
        save(&env, &DataKey::Pool(pool_id), &p);
        ControllerChanged {
            pool_id,
            network_id: net(&env),
            previous,
            controller: next,
        }
        .publish(&env);
        Ok(())
    }

    // ---- contract governance (admin) ----------------------------------------------------

    /// Step 1 of admin rotation.
    pub fn propose_admin(env: Env, proposed: Address) -> Result<(), Error> {
        let a = admin(&env);
        a.require_auth();
        if proposed == a {
            return Err(Error::SameOwner);
        }
        instance_set(&env, &DataKey::PendingAdmin, &proposed);
        AdminProposed {
            network_id: net(&env),
            admin: a,
            proposed,
        }
        .publish(&env);
        Ok(())
    }

    /// Step 2: the proposed admin accepts.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let next: Address =
            instance_get(&env, &DataKey::PendingAdmin).ok_or(Error::NoPendingAdmin)?;
        next.require_auth();
        let previous = admin(&env);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        instance_set(&env, &DataKey::Admin, &next);
        AdminChanged {
            network_id: net(&env),
            previous,
            admin: next,
        }
        .publish(&env);
        Ok(())
    }

    /// Announces a new wasm hash. It cannot be installed before `UPGRADE_DELAY_SECS` have
    /// passed, so members and the indexer see every upgrade a week before it can happen.
    pub fn propose_upgrade(env: Env, wasm_hash: BytesN<32>) -> Result<u64, Error> {
        admin(&env).require_auth();
        if upgrades_locked(&env) {
            return Err(Error::UpgradesLocked);
        }
        if instance_get::<PendingUpgrade>(&env, &DataKey::PendingUpgrade).is_some() {
            return Err(Error::UpgradePending);
        }
        let executable_after = now(&env)
            .checked_add(UPGRADE_DELAY_SECS)
            .ok_or(Error::Overflow)?;
        let u = PendingUpgrade {
            wasm_hash: wasm_hash.clone(),
            proposed_at: now(&env),
            executable_after,
        };
        instance_set(&env, &DataKey::PendingUpgrade, &u);
        UpgradeProposed {
            network_id: net(&env),
            wasm_hash,
            executable_after,
        }
        .publish(&env);
        Ok(executable_after)
    }

    pub fn cancel_upgrade(env: Env) -> Result<(), Error> {
        admin(&env).require_auth();
        let u: PendingUpgrade =
            instance_get(&env, &DataKey::PendingUpgrade).ok_or(Error::NoPendingUpgrade)?;
        env.storage().instance().remove(&DataKey::PendingUpgrade);
        UpgradeCancelled {
            network_id: net(&env),
            wasm_hash: u.wasm_hash,
        }
        .publish(&env);
        Ok(())
    }

    /// Installs the proposed wasm once the timelock has elapsed. Storage (pools, rights,
    /// totals) is preserved by the host; the constructor does not run again.
    pub fn execute_upgrade(env: Env) -> Result<(), Error> {
        admin(&env).require_auth();
        if upgrades_locked(&env) {
            return Err(Error::UpgradesLocked);
        }
        let u: PendingUpgrade =
            instance_get(&env, &DataKey::PendingUpgrade).ok_or(Error::NoPendingUpgrade)?;
        if now(&env) < u.executable_after {
            return Err(Error::UpgradeNotReady);
        }
        env.storage().instance().remove(&DataKey::PendingUpgrade);
        ContractUpgraded {
            network_id: net(&env),
            wasm_hash: u.wasm_hash.clone(),
        }
        .publish(&env);
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(u.wasm_hash));
        Ok(())
    }

    /// Irreversibly freezes the code. Cancels any pending upgrade.
    pub fn lock_upgrades(env: Env) -> Result<(), Error> {
        admin(&env).require_auth();
        if upgrades_locked(&env) {
            return Err(Error::UpgradesLocked);
        }
        if let Some(u) = instance_get::<PendingUpgrade>(&env, &DataKey::PendingUpgrade) {
            env.storage().instance().remove(&DataKey::PendingUpgrade);
            UpgradeCancelled {
                network_id: net(&env),
                wasm_hash: u.wasm_hash,
            }
            .publish(&env);
        }
        instance_set(&env, &DataKey::UpgradesLocked, &true);
        UpgradesLocked {
            network_id: net(&env),
        }
        .publish(&env);
        Ok(())
    }

    // ---- views ---------------------------------------------------------------------------

    pub fn get_pool(env: Env, pool_id: u64) -> Result<PoolConfig, Error> {
        pool(&env, pool_id)
    }

    pub fn get_totals(env: Env, pool_id: u64) -> Result<EscrowTotals, Error> {
        pool(&env, pool_id)?;
        Ok(totals(&env, pool_id))
    }

    /// Funded value not reserved for any right (what the controller may still withdraw/issue).
    pub fn get_unreserved(env: Env, pool_id: u64) -> Result<i128, Error> {
        pool(&env, pool_id)?;
        let t = totals(&env, pool_id);
        sub(held(&t)?, t.reserved)
    }

    pub fn get_right(env: Env, right_id: u64) -> Result<PayoutRight, Error> {
        right(&env, right_id)
    }

    pub fn get_transfer(env: Env, transfer_id: u64) -> Result<TransferApproval, Error> {
        transfer(&env, transfer_id)
    }

    pub fn is_member(env: Env, pool_id: u64, who: Address) -> bool {
        is_eligible(&env, pool_id, &who)
    }

    /// Membership record (also for members who left: `eligible == false`).
    pub fn get_member(env: Env, pool_id: u64, who: Address) -> Result<Member, Error> {
        pool(&env, pool_id)?;
        member(&env, pool_id, &who).ok_or(Error::NotMember)
    }

    pub fn get_pending_controller(env: Env, pool_id: u64) -> Result<Option<Address>, Error> {
        pool(&env, pool_id)?;
        Ok(load(&env, &DataKey::PendingController(pool_id)))
    }

    pub fn get_governance(env: Env) -> Governance {
        let u: Option<PendingUpgrade> = instance_get(&env, &DataKey::PendingUpgrade);
        Governance {
            admin: admin(&env),
            pending_admin: instance_get(&env, &DataKey::PendingAdmin),
            upgrade_executable_after: u.as_ref().map(|u| u.executable_after).unwrap_or(0),
            pending_upgrade: u.map(|u| u.wasm_hash),
            upgrades_locked: upgrades_locked(&env),
            upgrade_delay_secs: UPGRADE_DELAY_SECS,
        }
    }

    /// Highest pool/right/transfer ids issued so far, for indexer reconciliation.
    pub fn get_counts(env: Env) -> Counts {
        let c = |k: DataKey| {
            env.storage()
                .instance()
                .get::<DataKey, u64>(&k)
                .unwrap_or(0)
        };
        Counts {
            pools: c(DataKey::PoolCount),
            rights: c(DataKey::RightCount),
            transfers: c(DataKey::TransferCount),
        }
    }
}
