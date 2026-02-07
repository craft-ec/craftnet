//! Proof pipeline integration tests
//!
//! Covers the new ZK-proven epoch settlement pipeline:
//! 1. user_proof binding on ForwardReceipts
//! 2. Proof message chaining (prev_root → new_root)
//! 3. Aggregator distribution building from proven summaries
//! 4. End-to-end: aggregator → settlement claim flow
//! 5. Epoch phase enforcement across the full pipeline

use std::sync::Arc;

use sha2::{Digest, Sha256};

use tunnelcraft_aggregator::Aggregator;
use tunnelcraft_crypto::{sign_forward_receipt, verify_forward_receipt, SigningKeypair};
use tunnelcraft_network::{PoolType, ProofMessage};
use tunnelcraft_settlement::{
    ClaimRewards, PostDistribution, SettlementClient, SettlementConfig, Subscribe,
};

// ============================================================================
// 1. user_proof binding
// ============================================================================

/// Verify that user_proof is included in ForwardReceipt signature
#[test]
fn test_user_proof_included_in_receipt_signature() {
    let relay = SigningKeypair::generate();
    let request_id = [1u8; 32];
    let shard_id = [2u8; 32];

    let user_proof_a = [0xAA; 32];
    let user_proof_b = [0xBB; 32];

    let receipt_a = sign_forward_receipt(&relay, &request_id, &shard_id, &user_proof_a);
    let receipt_b = sign_forward_receipt(&relay, &request_id, &shard_id, &user_proof_b);

    // Both verify
    assert!(verify_forward_receipt(&receipt_a));
    assert!(verify_forward_receipt(&receipt_b));

    // But their signatures differ because user_proof is different
    assert_ne!(
        receipt_a.signature, receipt_b.signature,
        "Different user_proofs should produce different signatures"
    );

    // And user_proof is stored
    assert_eq!(receipt_a.user_proof, user_proof_a);
    assert_eq!(receipt_b.user_proof, user_proof_b);
}

/// Verify that tampering with user_proof breaks verification
#[test]
fn test_user_proof_tamper_breaks_verification() {
    let relay = SigningKeypair::generate();
    let request_id = [1u8; 32];
    let shard_id = [2u8; 32];
    let user_proof = [0xAA; 32];

    let mut receipt = sign_forward_receipt(&relay, &request_id, &shard_id, &user_proof);
    assert!(verify_forward_receipt(&receipt));

    // Tamper with user_proof
    receipt.user_proof = [0xFF; 32];
    assert!(
        !verify_forward_receipt(&receipt),
        "Tampered user_proof should fail verification"
    );
}

/// Verify user_proof computation: SHA256(request_id || user_pubkey || user_sig)
#[test]
fn test_user_proof_computation() {
    let request_id = [1u8; 32];
    let user_pubkey = [2u8; 32];
    let user_signature = [3u8; 64]; // mock signature

    let mut hasher = Sha256::new();
    hasher.update(&request_id);
    hasher.update(&user_pubkey);
    hasher.update(&user_signature);
    let result = hasher.finalize();

    let mut expected = [0u8; 32];
    expected.copy_from_slice(&result);

    // Compute again — should be deterministic
    let mut hasher2 = Sha256::new();
    hasher2.update(&request_id);
    hasher2.update(&user_pubkey);
    hasher2.update(&user_signature);
    let result2 = hasher2.finalize();

    let mut expected2 = [0u8; 32];
    expected2.copy_from_slice(&result2);

    assert_eq!(expected, expected2, "user_proof computation should be deterministic");
    assert_ne!(expected, [0u8; 32], "user_proof should not be all zeros");
}

/// Different users produce different user_proofs even for same request
#[test]
fn test_different_users_different_proofs() {
    let request_id = [1u8; 32];
    let user_a = [10u8; 32];
    let user_b = [20u8; 32];
    let sig = [0u8; 64]; // same sig for test

    let proof_a = {
        let mut h = Sha256::new();
        h.update(&request_id);
        h.update(&user_a);
        h.update(&sig);
        let mut p = [0u8; 32];
        p.copy_from_slice(&h.finalize());
        p
    };

    let proof_b = {
        let mut h = Sha256::new();
        h.update(&request_id);
        h.update(&user_b);
        h.update(&sig);
        let mut p = [0u8; 32];
        p.copy_from_slice(&h.finalize());
        p
    };

    assert_ne!(
        proof_a, proof_b,
        "Different users should produce different user_proofs"
    );
}

// ============================================================================
// 2. Proof message chaining
// ============================================================================

/// Verify correct chaining of proof messages (prev_root → new_root)
#[test]
fn test_proof_message_chain_integrity() {
    let mut agg = Aggregator::new();

    let relay = [1u8; 32];
    let pool = [2u8; 32];

    // Batch 1: first proof starts from zeros
    let msg1 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 100,
        cumulative_count: 100,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: 1000,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg1).unwrap();

    // Batch 2: chains from batch 1
    let msg2 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 50,
        cumulative_count: 150,
        prev_root: [0xAA; 32], // matches msg1.new_root
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: 2000,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg2).unwrap();

    // Batch 3: chains from batch 2
    let msg3 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 200,
        cumulative_count: 350,
        prev_root: [0xBB; 32], // matches msg2.new_root
        new_root: [0xCC; 32],
        proof: vec![],
        timestamp: 3000,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg3).unwrap();

    // Verify final state
    let usage = agg.get_pool_usage(&pool);
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].1, 350);
}

/// Chain break (wrong prev_root) is rejected
#[test]
fn test_proof_chain_break_detected() {
    let mut agg = Aggregator::new();

    let relay = [1u8; 32];
    let pool = [2u8; 32];

    let msg1 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 100,
        cumulative_count: 100,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: 1000,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg1).unwrap();

    // Wrong prev_root — chain break
    let msg_bad = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 50,
        cumulative_count: 150,
        prev_root: [0xFF; 32], // WRONG — should be [0xAA; 32]
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: 2000,
        signature: vec![0u8; 64],
    };

    let result = agg.handle_proof(msg_bad);
    assert!(result.is_err(), "Chain break should be rejected");
}

/// Non-increasing cumulative count is rejected
#[test]
fn test_proof_non_increasing_count_rejected() {
    let mut agg = Aggregator::new();

    let relay = [1u8; 32];
    let pool = [2u8; 32];

    let msg1 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 100,
        cumulative_count: 100,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: 1000,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg1).unwrap();

    // Replay with same count
    let msg_replay = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 0,
        cumulative_count: 100, // same as before
        prev_root: [0xAA; 32],
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: 2000,
        signature: vec![0u8; 64],
    };

    let result = agg.handle_proof(msg_replay);
    assert!(result.is_err(), "Non-increasing count should be rejected");

    // Decreasing count
    let msg_dec = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: pool,
        pool_type: PoolType::Subscribed,
        batch_count: 0,
        cumulative_count: 50, // less than 100
        prev_root: [0xAA; 32],
        new_root: [0xCC; 32],
        proof: vec![],
        timestamp: 3000,
        signature: vec![0u8; 64],
    };

    let result = agg.handle_proof(msg_dec);
    assert!(result.is_err(), "Decreasing count should be rejected");
}

// ============================================================================
// 3. Aggregator distribution building
// ============================================================================

/// Multiple relays contribute to same pool, distribution is correct
#[test]
fn test_aggregator_multi_relay_distribution() {
    let mut agg = Aggregator::new();

    let pool = [10u8; 32];

    // 5 relays, each with different receipt counts
    let counts = [100u64, 200, 300, 150, 250]; // total = 1000
    for (i, &count) in counts.iter().enumerate() {
        let relay = [i as u8 + 1; 32];
        let msg = ProofMessage {
            relay_pubkey: relay,
            pool_pubkey: pool,
            pool_type: PoolType::Subscribed,
            batch_count: count,
            cumulative_count: count,
            prev_root: [0u8; 32],
            new_root: [(i as u8 + 1) * 0x11; 32],
            proof: vec![],
            timestamp: 1000,
            signature: vec![0u8; 64],
        };
        agg.handle_proof(msg).unwrap();
    }

    let dist = agg.build_distribution(&pool).unwrap();
    assert_eq!(dist.total, 1000);
    assert_eq!(dist.entries.len(), 5);
    assert_ne!(dist.root, [0u8; 32]);

    // Verify each entry has the right count
    for (relay, count) in &dist.entries {
        let idx = relay[0] as usize - 1;
        assert_eq!(*count, counts[idx]);
    }
}

/// Distribution root is deterministic regardless of insertion order
#[test]
fn test_distribution_root_order_independent() {
    // Build aggregator with relays in one order
    let mut agg1 = Aggregator::new();
    let pool = [10u8; 32];

    for i in 0..3u8 {
        let msg = ProofMessage {
            relay_pubkey: [i + 1; 32],
            pool_pubkey: pool,
            pool_type: PoolType::Subscribed,
            batch_count: (i as u64 + 1) * 100,
            cumulative_count: (i as u64 + 1) * 100,
            prev_root: [0u8; 32],
            new_root: [i + 0xAA; 32],
            proof: vec![],
            timestamp: 1000,
            signature: vec![0u8; 64],
        };
        agg1.handle_proof(msg).unwrap();
    }

    // Build aggregator with relays in reverse order
    let mut agg2 = Aggregator::new();
    for i in (0..3u8).rev() {
        let msg = ProofMessage {
            relay_pubkey: [i + 1; 32],
            pool_pubkey: pool,
            pool_type: PoolType::Subscribed,
            batch_count: (i as u64 + 1) * 100,
            cumulative_count: (i as u64 + 1) * 100,
            prev_root: [0u8; 32],
            new_root: [i + 0xAA; 32],
            proof: vec![],
            timestamp: 1000,
            signature: vec![0u8; 64],
        };
        agg2.handle_proof(msg).unwrap();
    }

    let dist1 = agg1.build_distribution(&pool).unwrap();
    let dist2 = agg2.build_distribution(&pool).unwrap();

    assert_eq!(dist1.root, dist2.root, "Distribution root should be deterministic");
    assert_eq!(dist1.total, dist2.total);
}

// ============================================================================
// 4. End-to-end: aggregator → settlement claim flow
// ============================================================================

/// Full flow: receipts → aggregator → distribution → settlement claims
#[tokio::test]
async fn test_aggregator_to_settlement_claim_flow() {
    let user_pubkey = [1u8; 32];
    let pool_balance = 1_000_000u64;

    // Setup settlement
    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create expired subscription (past grace)
    settlement
        .add_mock_subscription_with_expiry(
            user_pubkey,
            tunnelcraft_core::SubscriptionTier::Standard,
            pool_balance,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        )
        .unwrap();

    // Simulate relay proof accumulation
    let mut agg = Aggregator::new();

    // 3 relays with varying receipt counts
    let relay_counts: Vec<([u8; 32], u64)> = vec![
        ([10u8; 32], 500), // 50% of traffic
        ([20u8; 32], 300), // 30%
        ([30u8; 32], 200), // 20%
    ];

    for (relay, count) in &relay_counts {
        let msg = ProofMessage {
            relay_pubkey: *relay,
            pool_pubkey: user_pubkey,
            pool_type: PoolType::Subscribed,
            batch_count: *count,
            cumulative_count: *count,
            prev_root: [0u8; 32],
            new_root: [relay[0]; 32], // unique root per relay
            proof: vec![],
            timestamp: now,
            signature: vec![0u8; 64],
        };
        agg.handle_proof(msg).unwrap();
    }

    // Build distribution
    let dist = agg.build_distribution(&user_pubkey).unwrap();
    assert_eq!(dist.total, 1000);

    // Post distribution on-chain (mock)
    settlement
        .post_distribution(PostDistribution {
            user_pubkey,
            distribution_root: dist.root,
            total_receipts: dist.total,
        })
        .await
        .unwrap();

    // Each relay claims their share
    let mut total_claimed = 0u64;

    for (relay, count) in &relay_counts {
        settlement
            .claim_rewards(ClaimRewards {
                user_pubkey,
                node_pubkey: *relay,
                relay_count: *count,
                merkle_proof: vec![],
            })
            .await
            .unwrap();

        let acct = settlement.get_node_account(*relay).await.unwrap();
        let expected = (*count as u128 * pool_balance as u128 / dist.total as u128) as u64;
        assert_eq!(acct.unclaimed_rewards, expected);
        total_claimed += acct.unclaimed_rewards;
    }

    // Pool should be fully drained
    let sub = settlement
        .get_subscription_state(user_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.pool_balance, 0);
    assert_eq!(total_claimed, pool_balance);
}

/// End-to-end with chained proofs over multiple batches
#[tokio::test]
async fn test_chained_proofs_to_settlement() {
    let user_pubkey = [5u8; 32];
    let pool_balance = 500_000u64;

    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    settlement
        .add_mock_subscription_with_expiry(
            user_pubkey,
            tunnelcraft_core::SubscriptionTier::Premium,
            pool_balance,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        )
        .unwrap();

    let mut agg = Aggregator::new();
    let relay = [42u8; 32];

    // Relay sends 3 chained batches
    let msg1 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: user_pubkey,
        pool_type: PoolType::Subscribed,
        batch_count: 100,
        cumulative_count: 100,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: now - 100,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg1).unwrap();

    let msg2 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: user_pubkey,
        pool_type: PoolType::Subscribed,
        batch_count: 150,
        cumulative_count: 250,
        prev_root: [0xAA; 32],
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: now - 50,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg2).unwrap();

    let msg3 = ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: user_pubkey,
        pool_type: PoolType::Subscribed,
        batch_count: 50,
        cumulative_count: 300,
        prev_root: [0xBB; 32],
        new_root: [0xCC; 32],
        proof: vec![],
        timestamp: now,
        signature: vec![0u8; 64],
    };
    agg.handle_proof(msg3).unwrap();

    // Build distribution — single relay with 300 total
    let dist = agg.build_distribution(&user_pubkey).unwrap();
    assert_eq!(dist.total, 300);
    assert_eq!(dist.entries.len(), 1);
    assert_eq!(dist.entries[0], (relay, 300));

    // Post and claim
    settlement
        .post_distribution(PostDistribution {
            user_pubkey,
            distribution_root: dist.root,
            total_receipts: dist.total,
        })
        .await
        .unwrap();

    settlement
        .claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: relay,
            relay_count: 300,
            merkle_proof: vec![],
        })
        .await
        .unwrap();

    let acct = settlement.get_node_account(relay).await.unwrap();
    // 300/300 * 500_000 = 500_000 (sole relay gets full pool)
    assert_eq!(acct.unclaimed_rewards, 500_000);
}

// ============================================================================
// 5. Epoch phase enforcement across the pipeline
// ============================================================================

/// Cannot post distribution during active subscription
#[tokio::test]
async fn test_post_distribution_blocked_during_active() {
    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));
    let user = [1u8; 32];

    // Fresh subscription — active
    settlement
        .subscribe(Subscribe {
            user_pubkey: user,
            tier: tunnelcraft_core::SubscriptionTier::Standard,
            payment_amount: 1_000_000,
        })
        .await
        .unwrap();

    let result = settlement
        .post_distribution(PostDistribution {
            user_pubkey: user,
            distribution_root: [0xAA; 32],
            total_receipts: 100,
        })
        .await;

    assert!(result.is_err(), "Should not post distribution during active epoch");
}

/// Cannot claim before distribution is posted
#[tokio::test]
async fn test_claim_blocked_without_distribution() {
    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));
    let user = [1u8; 32];
    let node = [2u8; 32];

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Expired subscription but no distribution posted
    settlement
        .add_mock_subscription_with_expiry(
            user,
            tunnelcraft_core::SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        )
        .unwrap();

    let result = settlement
        .claim_rewards(ClaimRewards {
            user_pubkey: user,
            node_pubkey: node,
            relay_count: 50,
            merkle_proof: vec![],
        })
        .await;

    assert!(result.is_err(), "Should not claim without distribution");
}

/// Double-claim is rejected
#[tokio::test]
async fn test_double_claim_rejected_e2e() {
    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));
    let user = [1u8; 32];
    let node = [2u8; 32];

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    settlement
        .add_mock_subscription_with_expiry(
            user,
            tunnelcraft_core::SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        )
        .unwrap();

    settlement
        .post_distribution(PostDistribution {
            user_pubkey: user,
            distribution_root: [0xDD; 32],
            total_receipts: 100,
        })
        .await
        .unwrap();

    // First claim succeeds
    settlement
        .claim_rewards(ClaimRewards {
            user_pubkey: user,
            node_pubkey: node,
            relay_count: 50,
            merkle_proof: vec![],
        })
        .await
        .unwrap();

    // Second claim fails
    let result = settlement
        .claim_rewards(ClaimRewards {
            user_pubkey: user,
            node_pubkey: node,
            relay_count: 50,
            merkle_proof: vec![],
        })
        .await;

    assert!(result.is_err(), "Double claim should be rejected");
}

// ============================================================================
// 6. Free-tier tracking
// ============================================================================

/// Free-tier proofs are tracked but don't interfere with subscribed pools
#[test]
fn test_free_tier_tracking_separate() {
    let mut agg = Aggregator::new();

    let relay = [1u8; 32];
    let subscribed_pool = [10u8; 32];
    let free_pool = [20u8; 32];

    // Subscribed pool: 100 receipts
    agg.handle_proof(ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: subscribed_pool,
        pool_type: PoolType::Subscribed,
        batch_count: 100,
        cumulative_count: 100,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: 1000,
        signature: vec![0u8; 64],
    })
    .unwrap();

    // Free pool: 200 receipts
    agg.handle_proof(ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: free_pool,
        pool_type: PoolType::Free,
        batch_count: 200,
        cumulative_count: 200,
        prev_root: [0u8; 32],
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: 1000,
        signature: vec![0u8; 64],
    })
    .unwrap();

    // Only subscribed pool should be in subscribed_pools()
    let subscribed = agg.subscribed_pools();
    assert_eq!(subscribed.len(), 1);
    assert_eq!(subscribed[0], subscribed_pool);

    // Free tier stats should only count the free pool
    let free_stats = agg.get_free_tier_stats();
    assert_eq!(free_stats.len(), 1);
    assert_eq!(free_stats[0].1, 200);

    // Network stats should show both
    let stats = agg.get_network_stats();
    assert_eq!(stats.total_shards, 300);
    assert_eq!(stats.subscribed_shards, 100);
    assert_eq!(stats.free_shards, 200);
    assert_eq!(stats.active_pools, 2);
}

// ============================================================================
// 7. ProofMessage serialization roundtrip
// ============================================================================

/// ProofMessage survives serialization/deserialization
#[test]
fn test_proof_message_gossip_roundtrip() {
    let msg = ProofMessage {
        relay_pubkey: [42u8; 32],
        pool_pubkey: [7u8; 32],
        pool_type: PoolType::Subscribed,
        batch_count: 10_000,
        cumulative_count: 50_000,
        prev_root: [0xAA; 32],
        new_root: [0xBB; 32],
        proof: vec![0xCC; 256],
        timestamp: 1700000000,
        signature: vec![0xDD; 64],
    };

    // Serialize (as it would go over gossipsub)
    let bytes = msg.to_bytes();
    assert!(!bytes.is_empty());

    // Deserialize (as aggregator would receive it)
    let decoded = ProofMessage::from_bytes(&bytes).unwrap();

    assert_eq!(decoded.relay_pubkey, msg.relay_pubkey);
    assert_eq!(decoded.pool_pubkey, msg.pool_pubkey);
    assert_eq!(decoded.pool_type, msg.pool_type);
    assert_eq!(decoded.batch_count, msg.batch_count);
    assert_eq!(decoded.cumulative_count, msg.cumulative_count);
    assert_eq!(decoded.prev_root, msg.prev_root);
    assert_eq!(decoded.new_root, msg.new_root);
    assert_eq!(decoded.proof, msg.proof);
    assert_eq!(decoded.timestamp, msg.timestamp);
    assert_eq!(decoded.signature, msg.signature);
}

// ============================================================================
// 8. Multi-pool aggregation
// ============================================================================

/// Aggregator handles multiple users' pools independently
#[tokio::test]
async fn test_multi_pool_aggregation_and_claims() {
    let user_a = [10u8; 32];
    let user_b = [20u8; 32];
    let relay = [1u8; 32];

    let settlement = Arc::new(SettlementClient::new(SettlementConfig::mock(), [0u8; 32]));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Both users have expired subscriptions
    for (user, balance) in [(user_a, 1_000_000u64), (user_b, 500_000)] {
        settlement
            .add_mock_subscription_with_expiry(
                user,
                tunnelcraft_core::SubscriptionTier::Standard,
                balance,
                now - 40 * 24 * 3600,
                now - 10 * 24 * 3600,
            )
            .unwrap();
    }

    // Aggregator tracks both pools
    let mut agg = Aggregator::new();

    // Relay served 700 receipts for user_a, 300 for user_b
    agg.handle_proof(ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: user_a,
        pool_type: PoolType::Subscribed,
        batch_count: 700,
        cumulative_count: 700,
        prev_root: [0u8; 32],
        new_root: [0xAA; 32],
        proof: vec![],
        timestamp: now,
        signature: vec![0u8; 64],
    })
    .unwrap();

    agg.handle_proof(ProofMessage {
        relay_pubkey: relay,
        pool_pubkey: user_b,
        pool_type: PoolType::Subscribed,
        batch_count: 300,
        cumulative_count: 300,
        prev_root: [0u8; 32],
        new_root: [0xBB; 32],
        proof: vec![],
        timestamp: now,
        signature: vec![0u8; 64],
    })
    .unwrap();

    // Post distribution and claim for each pool independently
    for (user, expected_count) in [(user_a, 700u64), (user_b, 300)] {
        let dist = agg.build_distribution(&user).unwrap();
        assert_eq!(dist.total, expected_count);

        settlement
            .post_distribution(PostDistribution {
                user_pubkey: user,
                distribution_root: dist.root,
                total_receipts: dist.total,
            })
            .await
            .unwrap();

        settlement
            .claim_rewards(ClaimRewards {
                user_pubkey: user,
                node_pubkey: relay,
                relay_count: expected_count,
                merkle_proof: vec![],
            })
            .await
            .unwrap();
    }

    // Relay should have accumulated rewards from both pools
    let acct = settlement.get_node_account(relay).await.unwrap();
    // 700/700 * 1_000_000 + 300/300 * 500_000 = 1_000_000 + 500_000 = 1_500_000
    assert_eq!(acct.unclaimed_rewards, 1_500_000);

    // Both pools drained
    for user in [user_a, user_b] {
        let sub = settlement
            .get_subscription_state(user)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sub.pool_balance, 0);
    }
}
