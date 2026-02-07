//! TunnelCraft Aggregator
//!
//! Standalone service that any node can run. Subscribes to the proof
//! gossipsub topic, collects ZK-proven summaries from relays, builds
//! per-pool Merkle distributions, and posts them on-chain.
//!
//! Tracks both subscribed and free-tier traffic — free-tier stats feed
//! a future ecosystem reward pool.

use std::collections::HashMap;

use sha2::{Sha256, Digest};
use tracing::{debug, info, warn};

use tunnelcraft_core::PublicKey;
use tunnelcraft_network::{ProofMessage, PoolType};

/// A single relay's proven claim for a pool
#[derive(Debug, Clone)]
struct ProofClaim {
    /// Running total of receipts this relay has proven for the pool
    cumulative_count: u64,
    /// Latest Merkle root
    latest_root: [u8; 32],
    /// Unix timestamp of last proof received
    last_updated: u64,
}

/// Tracks all relay claims for a single pool (user)
#[derive(Debug, Clone)]
struct PoolTracker {
    /// Whether the user is subscribed or free-tier
    pool_type: PoolType,
    /// Relay pubkey → latest cumulative proof
    relay_claims: HashMap<PublicKey, ProofClaim>,
}

/// Merkle distribution for a pool (ready for on-chain posting)
#[derive(Debug, Clone)]
pub struct Distribution {
    /// Merkle root of (relay, count) entries
    pub root: [u8; 32],
    /// Total receipts across all relays
    pub total: u64,
    /// Individual entries: (relay_pubkey, receipt_count)
    pub entries: Vec<(PublicKey, u64)>,
}

/// Network-wide statistics
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    /// Total shards tracked (subscribed + free)
    pub total_shards: u64,
    /// Number of active pools (users)
    pub active_pools: usize,
    /// Number of active relays
    pub active_relays: usize,
    /// Total subscribed shards
    pub subscribed_shards: u64,
    /// Total free-tier shards
    pub free_shards: u64,
}

/// The aggregator service
///
/// Collects ZK-proven summaries from relays via gossipsub, builds
/// Merkle distributions per pool, and provides query APIs.
pub struct Aggregator {
    /// Per pool: relay → latest cumulative proof
    pools: HashMap<PublicKey, PoolTracker>,
}

impl Aggregator {
    /// Create a new aggregator
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
        }
    }

    /// Handle an incoming proof message from gossipsub.
    ///
    /// Verifies the proof chain (prev_root matches last known root)
    /// and updates the pool tracker.
    pub fn handle_proof(&mut self, msg: ProofMessage) -> Result<(), AggregatorError> {
        let pool = self.pools.entry(msg.pool_pubkey).or_insert_with(|| PoolTracker {
            pool_type: msg.pool_type,
            relay_claims: HashMap::new(),
        });

        // Update pool_type if it changed (e.g., user subscribed)
        pool.pool_type = msg.pool_type;

        // Verify chain continuity: prev_root should match our last known root
        if let Some(existing) = pool.relay_claims.get(&msg.relay_pubkey) {
            if existing.latest_root != msg.prev_root {
                warn!(
                    "Proof chain break for relay {} on pool {}: expected root {:?}, got prev_root {:?}",
                    hex::encode(&msg.relay_pubkey[..8]),
                    hex::encode(&msg.pool_pubkey[..8]),
                    &existing.latest_root[..8],
                    &msg.prev_root[..8],
                );
                return Err(AggregatorError::ChainBreak);
            }

            // Cumulative count should be increasing
            if msg.cumulative_count <= existing.cumulative_count {
                warn!(
                    "Non-increasing cumulative count for relay {} on pool {}: {} <= {}",
                    hex::encode(&msg.relay_pubkey[..8]),
                    hex::encode(&msg.pool_pubkey[..8]),
                    msg.cumulative_count,
                    existing.cumulative_count,
                );
                return Err(AggregatorError::NonIncreasingCount);
            }
        } else {
            // First proof from this relay for this pool — prev_root should be zeros
            if msg.prev_root != [0u8; 32] && msg.cumulative_count != msg.batch_count {
                debug!(
                    "First proof from relay {} has non-zero prev_root — may have missed earlier proofs",
                    hex::encode(&msg.relay_pubkey[..8]),
                );
                // Accept anyway — we can't verify history we didn't see
            }
        }

        // Update relay claim
        pool.relay_claims.insert(msg.relay_pubkey, ProofClaim {
            cumulative_count: msg.cumulative_count,
            latest_root: msg.new_root,
            last_updated: msg.timestamp,
        });

        debug!(
            "Updated proof for relay {} on pool {} ({:?}): cumulative={}",
            hex::encode(&msg.relay_pubkey[..8]),
            hex::encode(&msg.pool_pubkey[..8]),
            msg.pool_type,
            msg.cumulative_count,
        );

        Ok(())
    }

    /// Build a Merkle distribution for a subscribed pool.
    ///
    /// Returns the distribution root and entries that can be posted
    /// on-chain via `post_distribution()`.
    pub fn build_distribution(&self, pool: &PublicKey) -> Option<Distribution> {
        let tracker = self.pools.get(pool)?;

        let mut entries: Vec<(PublicKey, u64)> = tracker.relay_claims.iter()
            .map(|(relay, claim)| (*relay, claim.cumulative_count))
            .collect();

        if entries.is_empty() {
            return None;
        }

        // Sort by relay pubkey for deterministic root
        entries.sort_by_key(|(relay, _)| *relay);

        let total: u64 = entries.iter().map(|(_, count)| count).sum();
        let root = Self::compute_distribution_root(&entries);

        Some(Distribution {
            root,
            total,
            entries,
        })
    }

    /// Compute Merkle root of distribution entries.
    ///
    /// Simple hash chain for now — a real implementation would build
    /// a proper Merkle tree for proof generation.
    fn compute_distribution_root(entries: &[(PublicKey, u64)]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for (relay, count) in entries {
            hasher.update(relay);
            hasher.update(&count.to_le_bytes());
        }
        let result = hasher.finalize();
        let mut root = [0u8; 32];
        root.copy_from_slice(&result);
        root
    }

    // =========================================================================
    // Query APIs
    // =========================================================================

    /// Get per-relay usage breakdown for a specific pool
    pub fn get_pool_usage(&self, pool: &PublicKey) -> Vec<(PublicKey, u64)> {
        self.pools.get(pool)
            .map(|tracker| {
                tracker.relay_claims.iter()
                    .map(|(relay, claim)| (*relay, claim.cumulative_count))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get per-pool breakdown for a specific relay
    pub fn get_relay_stats(&self, relay: &PublicKey) -> Vec<(PublicKey, u64)> {
        self.pools.iter()
            .filter_map(|(pool, tracker)| {
                tracker.relay_claims.get(relay)
                    .map(|claim| (*pool, claim.cumulative_count))
            })
            .collect()
    }

    /// Get network-wide statistics
    pub fn get_network_stats(&self) -> NetworkStats {
        let mut stats = NetworkStats::default();
        let mut all_relays: std::collections::HashSet<PublicKey> = std::collections::HashSet::new();

        for (_, tracker) in &self.pools {
            stats.active_pools += 1;
            for (relay, claim) in &tracker.relay_claims {
                all_relays.insert(*relay);
                stats.total_shards += claim.cumulative_count;
                match tracker.pool_type {
                    PoolType::Subscribed => stats.subscribed_shards += claim.cumulative_count,
                    PoolType::Free => stats.free_shards += claim.cumulative_count,
                }
            }
        }

        stats.active_relays = all_relays.len();
        stats
    }

    /// Get free-tier relay statistics (for ecosystem reward distribution)
    pub fn get_free_tier_stats(&self) -> Vec<(PublicKey, u64)> {
        let mut relay_totals: HashMap<PublicKey, u64> = HashMap::new();

        for tracker in self.pools.values() {
            if tracker.pool_type == PoolType::Free {
                for (relay, claim) in &tracker.relay_claims {
                    *relay_totals.entry(*relay).or_default() += claim.cumulative_count;
                }
            }
        }

        relay_totals.into_iter().collect()
    }

    /// Get all subscribed pools (for epoch-end distribution posting)
    pub fn subscribed_pools(&self) -> Vec<PublicKey> {
        self.pools.iter()
            .filter(|(_, tracker)| tracker.pool_type == PoolType::Subscribed)
            .map(|(pool, _)| *pool)
            .collect()
    }

    /// Get the total number of tracked pools
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregator errors
#[derive(Debug, thiserror::Error)]
pub enum AggregatorError {
    #[error("Proof chain break: prev_root doesn't match")]
    ChainBreak,

    #[error("Non-increasing cumulative count")]
    NonIncreasingCount,

    #[error("Invalid proof")]
    InvalidProof,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proof(relay: u8, pool: u8, pool_type: PoolType, batch: u64, cumulative: u64, prev_root: [u8; 32], new_root: [u8; 32]) -> ProofMessage {
        ProofMessage {
            relay_pubkey: [relay; 32],
            pool_pubkey: [pool; 32],
            pool_type,
            batch_count: batch,
            cumulative_count: cumulative,
            prev_root,
            new_root,
            proof: vec![],
            timestamp: 1700000000,
            signature: vec![0u8; 64],
        }
    }

    #[test]
    fn test_aggregator_creation() {
        let agg = Aggregator::new();
        assert_eq!(agg.pool_count(), 0);
    }

    #[test]
    fn test_handle_single_proof() {
        let mut agg = Aggregator::new();

        let msg = make_proof(1, 2, PoolType::Subscribed, 100, 100, [0u8; 32], [0xAA; 32]);
        agg.handle_proof(msg).unwrap();

        assert_eq!(agg.pool_count(), 1);
        let usage = agg.get_pool_usage(&[2u8; 32]);
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].1, 100);
    }

    #[test]
    fn test_handle_chained_proofs() {
        let mut agg = Aggregator::new();

        // First batch
        let msg1 = make_proof(1, 2, PoolType::Subscribed, 100, 100, [0u8; 32], [0xAA; 32]);
        agg.handle_proof(msg1).unwrap();

        // Second batch (chains from first)
        let msg2 = make_proof(1, 2, PoolType::Subscribed, 50, 150, [0xAA; 32], [0xBB; 32]);
        agg.handle_proof(msg2).unwrap();

        let usage = agg.get_pool_usage(&[2u8; 32]);
        assert_eq!(usage[0].1, 150);
    }

    #[test]
    fn test_chain_break_rejected() {
        let mut agg = Aggregator::new();

        let msg1 = make_proof(1, 2, PoolType::Subscribed, 100, 100, [0u8; 32], [0xAA; 32]);
        agg.handle_proof(msg1).unwrap();

        // Wrong prev_root — should fail
        let msg2 = make_proof(1, 2, PoolType::Subscribed, 50, 150, [0xCC; 32], [0xDD; 32]);
        let result = agg.handle_proof(msg2);
        assert!(matches!(result, Err(AggregatorError::ChainBreak)));
    }

    #[test]
    fn test_non_increasing_count_rejected() {
        let mut agg = Aggregator::new();

        let msg1 = make_proof(1, 2, PoolType::Subscribed, 100, 100, [0u8; 32], [0xAA; 32]);
        agg.handle_proof(msg1).unwrap();

        // Same cumulative count — should fail
        let msg2 = make_proof(1, 2, PoolType::Subscribed, 0, 100, [0xAA; 32], [0xBB; 32]);
        let result = agg.handle_proof(msg2);
        assert!(matches!(result, Err(AggregatorError::NonIncreasingCount)));
    }

    #[test]
    fn test_multiple_relays_per_pool() {
        let mut agg = Aggregator::new();

        let msg1 = make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32]);
        let msg2 = make_proof(2, 10, PoolType::Subscribed, 30, 30, [0u8; 32], [0xBB; 32]);
        agg.handle_proof(msg1).unwrap();
        agg.handle_proof(msg2).unwrap();

        let usage = agg.get_pool_usage(&[10u8; 32]);
        assert_eq!(usage.len(), 2);

        let total: u64 = usage.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_build_distribution() {
        let mut agg = Aggregator::new();

        let msg1 = make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32]);
        let msg2 = make_proof(2, 10, PoolType::Subscribed, 30, 30, [0u8; 32], [0xBB; 32]);
        agg.handle_proof(msg1).unwrap();
        agg.handle_proof(msg2).unwrap();

        let dist = agg.build_distribution(&[10u8; 32]).unwrap();
        assert_eq!(dist.total, 100);
        assert_eq!(dist.entries.len(), 2);
        assert_ne!(dist.root, [0u8; 32]);
    }

    #[test]
    fn test_build_distribution_empty_pool() {
        let agg = Aggregator::new();
        assert!(agg.build_distribution(&[99u8; 32]).is_none());
    }

    #[test]
    fn test_distribution_root_deterministic() {
        let mut agg = Aggregator::new();

        let msg1 = make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32]);
        let msg2 = make_proof(2, 10, PoolType::Subscribed, 30, 30, [0u8; 32], [0xBB; 32]);
        agg.handle_proof(msg1).unwrap();
        agg.handle_proof(msg2).unwrap();

        let dist1 = agg.build_distribution(&[10u8; 32]).unwrap();
        let dist2 = agg.build_distribution(&[10u8; 32]).unwrap();
        assert_eq!(dist1.root, dist2.root);
    }

    #[test]
    fn test_network_stats() {
        let mut agg = Aggregator::new();

        // Subscribed pool
        agg.handle_proof(make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32])).unwrap();
        agg.handle_proof(make_proof(2, 10, PoolType::Subscribed, 30, 30, [0u8; 32], [0xBB; 32])).unwrap();

        // Free pool
        agg.handle_proof(make_proof(1, 20, PoolType::Free, 50, 50, [0u8; 32], [0xCC; 32])).unwrap();

        let stats = agg.get_network_stats();
        assert_eq!(stats.active_pools, 2);
        assert_eq!(stats.active_relays, 2); // relay 1 and 2
        assert_eq!(stats.subscribed_shards, 100);
        assert_eq!(stats.free_shards, 50);
        assert_eq!(stats.total_shards, 150);
    }

    #[test]
    fn test_relay_stats() {
        let mut agg = Aggregator::new();

        agg.handle_proof(make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32])).unwrap();
        agg.handle_proof(make_proof(1, 20, PoolType::Free, 50, 50, [0u8; 32], [0xBB; 32])).unwrap();

        let relay_stats = agg.get_relay_stats(&[1u8; 32]);
        assert_eq!(relay_stats.len(), 2);
        let total: u64 = relay_stats.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 120);
    }

    #[test]
    fn test_free_tier_stats() {
        let mut agg = Aggregator::new();

        agg.handle_proof(make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32])).unwrap();
        agg.handle_proof(make_proof(1, 20, PoolType::Free, 50, 50, [0u8; 32], [0xBB; 32])).unwrap();
        agg.handle_proof(make_proof(2, 20, PoolType::Free, 30, 30, [0u8; 32], [0xCC; 32])).unwrap();

        let free_stats = agg.get_free_tier_stats();
        assert_eq!(free_stats.len(), 2);
        let total: u64 = free_stats.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 80); // 50 + 30
    }

    #[test]
    fn test_subscribed_pools() {
        let mut agg = Aggregator::new();

        agg.handle_proof(make_proof(1, 10, PoolType::Subscribed, 70, 70, [0u8; 32], [0xAA; 32])).unwrap();
        agg.handle_proof(make_proof(1, 20, PoolType::Free, 50, 50, [0u8; 32], [0xBB; 32])).unwrap();

        let pools = agg.subscribed_pools();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0], [10u8; 32]);
    }
}
