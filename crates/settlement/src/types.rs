//! Settlement types for on-chain operations

use tunnelcraft_core::{Id, PublicKey, ChainEntry, CreditProof};

/// Status of a request in the settlement system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnChainStatus {
    /// Not yet submitted
    Unknown,
    /// Request settled by exit - credit consumed, points awarded
    Complete,
    /// Timed out without settlement (future: credit refund)
    Expired,
}

/// Credit purchase instruction data
#[derive(Debug, Clone)]
pub struct PurchaseCredits {
    /// Hash of the credit secret (user keeps secret)
    pub credit_hash: Id,
    /// Amount of credits to purchase
    pub amount: u64,
}

/// Request settlement submitted by exit node
///
/// Submitted once per request after exit reconstructs and processes it.
/// Records work done for reconciliation with chain-signed credit proof.
#[derive(Debug, Clone)]
pub struct SettleRequest {
    /// Request identifier
    pub request_id: Id,
    /// User's public key
    pub user_pubkey: PublicKey,
    /// Chain-signed credit proof (proves user has credits for this epoch)
    pub credit_proof: CreditProof,
    /// Signature chains from all request shards (User → Relays → Exit)
    pub request_chains: Vec<Vec<ChainEntry>>,
}

/// Response shard settlement submitted by last relay
///
/// Submitted independently for each response shard that completes delivery.
/// Network-level TCP ACK proves delivery; encryption proves credit usage.
/// Awards points to all nodes in the response chain.
#[derive(Debug, Clone)]
pub struct SettleResponseShard {
    /// Request identifier (links to original request)
    pub request_id: Id,
    /// Shard identifier
    pub shard_id: Id,
    /// Signature chain for this shard (Exit → Relays → User)
    pub response_chain: Vec<ChainEntry>,
}

/// Claim work points from a completed request
#[derive(Debug, Clone)]
pub struct ClaimWork {
    /// Request identifier
    pub request_id: Id,
    /// Node's public key
    pub node_pubkey: PublicKey,
}

/// Withdraw accumulated rewards
#[derive(Debug, Clone)]
pub struct Withdraw {
    /// Epoch to withdraw from
    pub epoch: u64,
    /// Amount to withdraw (0 = all available)
    pub amount: u64,
}

/// On-chain request state
#[derive(Debug, Clone)]
pub struct RequestState {
    /// Request identifier
    pub request_id: Id,
    /// Current status
    pub status: OnChainStatus,
    /// User's public key (set in Phase 1)
    pub user_pubkey: Option<PublicKey>,
    /// Credit amount for this request
    pub credit_amount: u64,
    /// Timestamp of last update
    pub updated_at: u64,
    /// Total points to distribute
    pub total_points: u64,
}

/// Node's accumulated points
#[derive(Debug, Clone)]
pub struct NodePoints {
    /// Node's public key
    pub node_pubkey: PublicKey,
    /// Points earned in current epoch
    pub current_epoch_points: u64,
    /// Total points ever earned
    pub lifetime_points: u64,
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
    fn test_on_chain_status_values() {
        // All status values should be distinct
        assert_ne!(OnChainStatus::Unknown, OnChainStatus::Complete);
        assert_ne!(OnChainStatus::Complete, OnChainStatus::Expired);
        assert_ne!(OnChainStatus::Expired, OnChainStatus::Unknown);
    }

    #[test]
    fn test_purchase_credits_creation() {
        let purchase = PurchaseCredits {
            credit_hash: [1u8; 32],
            amount: 1000,
        };

        assert_eq!(purchase.credit_hash, [1u8; 32]);
        assert_eq!(purchase.amount, 1000);
    }

    #[test]
    fn test_purchase_credits_zero_amount() {
        let purchase = PurchaseCredits {
            credit_hash: [0u8; 32],
            amount: 0,
        };

        assert_eq!(purchase.amount, 0);
    }

    #[test]
    fn test_purchase_credits_max_amount() {
        let purchase = PurchaseCredits {
            credit_hash: [0u8; 32],
            amount: u64::MAX,
        };

        assert_eq!(purchase.amount, u64::MAX);
    }

    #[test]
    fn test_settle_request_empty_chains() {
        let credit_proof = CreditProof {
            user_pubkey: [3u8; 32],
            balance: 1000,
            epoch: 1,
            chain_signature: [0u8; 64],
        };

        let settlement = SettleRequest {
            request_id: [1u8; 32],
            user_pubkey: [3u8; 32],
            credit_proof,
            request_chains: vec![],
        };

        assert!(settlement.request_chains.is_empty());
    }

    #[test]
    fn test_settle_request_multiple_chains() {
        let chain1 = vec![ChainEntry::new([1u8; 32], [0u8; 64], 3)];
        let chain2 = vec![ChainEntry::new([2u8; 32], [0u8; 64], 3)];
        let chain3 = vec![ChainEntry::new([3u8; 32], [0u8; 64], 3)];

        let credit_proof = CreditProof {
            user_pubkey: [3u8; 32],
            balance: 1000,
            epoch: 1,
            chain_signature: [0u8; 64],
        };

        let settlement = SettleRequest {
            request_id: [1u8; 32],
            user_pubkey: [3u8; 32],
            credit_proof,
            request_chains: vec![chain1, chain2, chain3],
        };

        assert_eq!(settlement.request_chains.len(), 3);
    }

    #[test]
    fn test_settle_response_shard_creation() {
        let settlement = SettleResponseShard {
            request_id: [1u8; 32],
            shard_id: [3u8; 32],
            response_chain: vec![ChainEntry::new([2u8; 32], [0u8; 64], 3)],
        };

        assert_eq!(settlement.request_id, [1u8; 32]);
        assert_eq!(settlement.shard_id, [3u8; 32]);
        assert_eq!(settlement.response_chain.len(), 1);
    }

    #[test]
    fn test_claim_work_creation() {
        let claim = ClaimWork {
            request_id: [1u8; 32],
            node_pubkey: [2u8; 32],
        };

        assert_eq!(claim.request_id, [1u8; 32]);
        assert_eq!(claim.node_pubkey, [2u8; 32]);
    }

    #[test]
    fn test_withdraw_all() {
        let withdraw = Withdraw {
            epoch: 42,
            amount: 0,  // 0 = withdraw all
        };

        assert_eq!(withdraw.epoch, 42);
        assert_eq!(withdraw.amount, 0);
    }

    #[test]
    fn test_withdraw_partial() {
        let withdraw = Withdraw {
            epoch: 100,
            amount: 500,
        };

        assert_eq!(withdraw.epoch, 100);
        assert_eq!(withdraw.amount, 500);
    }

    #[test]
    fn test_request_state_creation() {
        let state = RequestState {
            request_id: [1u8; 32],
            status: OnChainStatus::Complete,
            user_pubkey: Some([2u8; 32]),
            credit_amount: 100,
            updated_at: 1234567890,
            total_points: 300,
        };

        assert_eq!(state.status, OnChainStatus::Complete);
        assert!(state.user_pubkey.is_some());
    }

    #[test]
    fn test_request_state_unknown() {
        let state = RequestState {
            request_id: [1u8; 32],
            status: OnChainStatus::Unknown,
            user_pubkey: None,
            credit_amount: 0,
            updated_at: 0,
            total_points: 0,
        };

        assert_eq!(state.status, OnChainStatus::Unknown);
        assert!(state.user_pubkey.is_none());
    }

    #[test]
    fn test_node_points_creation() {
        let points = NodePoints {
            node_pubkey: [1u8; 32],
            current_epoch_points: 500,
            lifetime_points: 10000,
            last_withdrawal_epoch: 5,
        };

        assert_eq!(points.current_epoch_points, 500);
        assert_eq!(points.lifetime_points, 10000);
        assert_eq!(points.last_withdrawal_epoch, 5);
    }

    #[test]
    fn test_node_points_overflow_safe() {
        let points = NodePoints {
            node_pubkey: [1u8; 32],
            current_epoch_points: u64::MAX,
            lifetime_points: u64::MAX,
            last_withdrawal_epoch: u64::MAX,
        };

        assert_eq!(points.current_epoch_points, u64::MAX);
        assert_eq!(points.lifetime_points, u64::MAX);
    }

    #[test]
    fn test_on_chain_status_eq() {
        assert_eq!(OnChainStatus::Unknown, OnChainStatus::Unknown);
        assert_eq!(OnChainStatus::Complete, OnChainStatus::Complete);
        assert_eq!(OnChainStatus::Expired, OnChainStatus::Expired);
    }

    #[test]
    fn test_on_chain_status_clone() {
        let status = OnChainStatus::Complete;
        let cloned = status;  // Copy

        assert_eq!(status, cloned);
    }
}
