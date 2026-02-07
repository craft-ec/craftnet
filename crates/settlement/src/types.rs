//! Settlement types for on-chain operations
//!
//! New model: Subscription + ZK-proven epoch settlement
//!
//! Receipts stay local on the relay. Relays generate ZK proofs per pool,
//! gossip proven summaries, and an aggregator posts distributions on-chain.

use tunnelcraft_core::{PublicKey, SubscriptionTier};

/// Grace period after subscription expires before claims open (1 day)
pub const GRACE_PERIOD_SECS: u64 = 86_400;

/// Subscription epoch duration (30 days)
pub const EPOCH_DURATION_SECS: u64 = 30 * 24 * 3600;

/// Epoch phase for a subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochPhase {
    /// Subscription is active — relays earning receipts
    Active,
    /// Subscription expired, grace period for final proofs (1 day)
    Grace,
    /// Grace period ended — distribution can be posted, claims open
    Claimable,
    /// Pool fully claimed or expired beyond recovery
    Closed,
}

/// Subscribe instruction data
#[derive(Debug, Clone)]
pub struct Subscribe {
    /// User's public key
    pub user_pubkey: PublicKey,
    /// Subscription tier
    pub tier: SubscriptionTier,
    /// Payment amount in lamports (USDC in production)
    pub payment_amount: u64,
}

/// Post a Merkle distribution root for a user's pool.
///
/// Called by the aggregator after the grace period ends. Sets the
/// distribution root that relays use to claim their share.
#[derive(Debug, Clone)]
pub struct PostDistribution {
    /// User whose pool this distribution covers
    pub user_pubkey: PublicKey,
    /// Merkle root of (relay, receipt_count) distribution
    pub distribution_root: [u8; 32],
    /// Total receipts across all relays for this pool
    pub total_receipts: u64,
}

/// Claim rewards from a user's pool using a Merkle proof
///
/// After distribution is posted, each relay claims its share:
/// payout = (relay_count / total_receipts) * pool_balance
#[derive(Debug, Clone)]
pub struct ClaimRewards {
    /// User pool to claim from
    pub user_pubkey: PublicKey,
    /// Node claiming rewards
    pub node_pubkey: PublicKey,
    /// Number of receipts this relay has (proven by Merkle proof)
    pub relay_count: u64,
    /// Merkle proof that (node_pubkey, relay_count) is in distribution_root
    pub merkle_proof: Vec<[u8; 32]>,
}

/// Withdraw accumulated rewards to wallet
#[derive(Debug, Clone)]
pub struct Withdraw {
    /// Amount to withdraw (0 = all available)
    pub amount: u64,
}

/// On-chain subscription state for a user
#[derive(Debug, Clone)]
pub struct SubscriptionState {
    /// User's public key
    pub user_pubkey: PublicKey,
    /// Active subscription tier
    pub tier: SubscriptionTier,
    /// When the subscription was created (unix seconds)
    pub created_at: u64,
    /// Subscription expiry timestamp (unix seconds)
    pub expires_at: u64,
    /// Pool balance (payment minus claimed rewards)
    pub pool_balance: u64,
    /// Original pool balance at distribution time (for proportional claim calculation)
    pub original_pool_balance: u64,
    /// Total receipts across all relays (set by post_distribution)
    pub total_receipts: u64,
    /// Merkle root of the distribution (set by post_distribution)
    pub distribution_root: Option<[u8; 32]>,
}

impl SubscriptionState {
    /// Determine the current epoch phase
    pub fn phase(&self, now: u64) -> EpochPhase {
        if now < self.expires_at {
            EpochPhase::Active
        } else if now < self.expires_at + GRACE_PERIOD_SECS {
            EpochPhase::Grace
        } else if self.pool_balance > 0 {
            EpochPhase::Claimable
        } else {
            EpochPhase::Closed
        }
    }
}

/// Node's on-chain account tracking rewards
#[derive(Debug, Clone)]
pub struct NodeAccount {
    /// Node's public key
    pub node_pubkey: PublicKey,
    /// Unclaimed reward balance (lamports)
    pub unclaimed_rewards: u64,
    /// Last withdrawal epoch
    pub last_withdrawal_epoch: u64,
}

/// Transaction signature (Solana format)
pub type TransactionSignature = [u8; 64];

/// On-chain account address
pub type AccountAddress = [u8; 32];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_creation() {
        let sub = Subscribe {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Standard,
            payment_amount: 15_000_000,
        };

        assert_eq!(sub.user_pubkey, [1u8; 32]);
        assert_eq!(sub.tier, SubscriptionTier::Standard);
        assert_eq!(sub.payment_amount, 15_000_000);
    }

    #[test]
    fn test_post_distribution_creation() {
        let dist = PostDistribution {
            user_pubkey: [1u8; 32],
            distribution_root: [0xAA; 32],
            total_receipts: 1000,
        };

        assert_eq!(dist.user_pubkey, [1u8; 32]);
        assert_eq!(dist.distribution_root, [0xAA; 32]);
        assert_eq!(dist.total_receipts, 1000);
    }

    #[test]
    fn test_claim_rewards_creation() {
        let claim = ClaimRewards {
            user_pubkey: [1u8; 32],
            node_pubkey: [2u8; 32],
            relay_count: 500,
            merkle_proof: vec![[0xBB; 32], [0xCC; 32]],
        };

        assert_eq!(claim.user_pubkey, [1u8; 32]);
        assert_eq!(claim.node_pubkey, [2u8; 32]);
        assert_eq!(claim.relay_count, 500);
        assert_eq!(claim.merkle_proof.len(), 2);
    }

    #[test]
    fn test_withdraw_all() {
        let withdraw = Withdraw { amount: 0 };
        assert_eq!(withdraw.amount, 0);
    }

    #[test]
    fn test_withdraw_partial() {
        let withdraw = Withdraw { amount: 500 };
        assert_eq!(withdraw.amount, 500);
    }

    #[test]
    fn test_subscription_state_creation() {
        let state = SubscriptionState {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Premium,
            created_at: 1700000000,
            expires_at: 1700000000 + EPOCH_DURATION_SECS,
            pool_balance: 40_000_000,
            original_pool_balance: 40_000_000,
            total_receipts: 0,
            distribution_root: None,
        };

        assert_eq!(state.tier, SubscriptionTier::Premium);
        assert_eq!(state.pool_balance, 40_000_000);
        assert!(state.distribution_root.is_none());
    }

    #[test]
    fn test_epoch_phase_active() {
        let now = 1700000000;
        let state = SubscriptionState {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Standard,
            created_at: now,
            expires_at: now + EPOCH_DURATION_SECS,
            pool_balance: 1_000_000,
            original_pool_balance: 1_000_000,
            total_receipts: 0,
            distribution_root: None,
        };

        assert_eq!(state.phase(now + 100), EpochPhase::Active);
    }

    #[test]
    fn test_epoch_phase_grace() {
        let now = 1700000000;
        let expires_at = now + EPOCH_DURATION_SECS;
        let state = SubscriptionState {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Standard,
            created_at: now,
            expires_at,
            pool_balance: 1_000_000,
            original_pool_balance: 1_000_000,
            total_receipts: 0,
            distribution_root: None,
        };

        // Just after expiry — should be Grace
        assert_eq!(state.phase(expires_at + 1), EpochPhase::Grace);
        // Just before grace ends
        assert_eq!(state.phase(expires_at + GRACE_PERIOD_SECS - 1), EpochPhase::Grace);
    }

    #[test]
    fn test_epoch_phase_claimable() {
        let now = 1700000000;
        let expires_at = now + EPOCH_DURATION_SECS;
        let state = SubscriptionState {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Standard,
            created_at: now,
            expires_at,
            pool_balance: 1_000_000,
            original_pool_balance: 1_000_000,
            total_receipts: 0,
            distribution_root: None,
        };

        // After grace period, with balance remaining
        assert_eq!(state.phase(expires_at + GRACE_PERIOD_SECS + 1), EpochPhase::Claimable);
    }

    #[test]
    fn test_epoch_phase_closed() {
        let now = 1700000000;
        let expires_at = now + EPOCH_DURATION_SECS;
        let state = SubscriptionState {
            user_pubkey: [1u8; 32],
            tier: SubscriptionTier::Standard,
            created_at: now,
            expires_at,
            pool_balance: 0, // Fully drained
            original_pool_balance: 1_000_000,
            total_receipts: 100,
            distribution_root: Some([0xAA; 32]),
        };

        // After grace, pool drained → Closed
        assert_eq!(state.phase(expires_at + GRACE_PERIOD_SECS + 1), EpochPhase::Closed);
    }

    #[test]
    fn test_node_account_creation() {
        let node = NodeAccount {
            node_pubkey: [1u8; 32],
            unclaimed_rewards: 1_000_000,
            last_withdrawal_epoch: 5,
        };

        assert_eq!(node.unclaimed_rewards, 1_000_000);
    }

    #[test]
    fn test_node_account_zero_state() {
        let node = NodeAccount {
            node_pubkey: [0u8; 32],
            unclaimed_rewards: 0,
            last_withdrawal_epoch: 0,
        };

        assert_eq!(node.unclaimed_rewards, 0);
    }

    #[test]
    fn test_grace_period_constant() {
        assert_eq!(GRACE_PERIOD_SECS, 86_400); // 1 day
    }

    #[test]
    fn test_epoch_duration_constant() {
        assert_eq!(EPOCH_DURATION_SECS, 30 * 24 * 3600); // 30 days
    }
}
