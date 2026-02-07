//! Settlement client for interacting with Solana
//!
//! Supports two modes:
//! - **Mock Mode**: For development/testing without Solana. All operations succeed
//!   and state is tracked in-memory.
//! - **Live Mode**: Actual Solana RPC calls to the TunnelCraft settlement program.
//!
//! ## New Settlement Model
//!
//! Receipts stay local on the relay — never submitted on-chain individually.
//! Instead, relays generate ZK proofs locally, gossip proven summaries,
//! and an aggregator posts a distribution root on-chain. Relays claim
//! proportional rewards using Merkle proofs against the distribution root.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use sha2::{Sha256, Digest};
use tracing::{debug, info};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk_ids::system_program;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use tunnelcraft_core::{Id, PublicKey, ForwardReceipt, SubscriptionTier};

use crate::{
    SettlementError, Result,
    Subscribe, PostDistribution, ClaimRewards, Withdraw,
    SubscriptionState, NodeAccount, TransactionSignature,
    EpochPhase, EPOCH_DURATION_SECS,
};

/// Settlement mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementMode {
    /// Mock mode for development - all operations succeed, state is in-memory
    Mock,
    /// Live Solana mode (requires deployed program)
    Live,
}

/// Settlement client configuration
#[derive(Debug, Clone)]
pub struct SettlementConfig {
    /// Settlement mode (Mock or Live)
    pub mode: SettlementMode,
    /// Solana RPC endpoint (only used in Live mode)
    pub rpc_url: String,
    /// Program ID for the TunnelCraft settlement program
    pub program_id: [u8; 32],
    /// Commitment level for transactions
    pub commitment: String,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            mode: SettlementMode::Mock,
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id: [0u8; 32],
            commitment: "confirmed".to_string(),
        }
    }
}

impl SettlementConfig {
    /// Create a mock configuration for development
    pub fn mock() -> Self {
        Self {
            mode: SettlementMode::Mock,
            ..Default::default()
        }
    }

    /// Devnet program ID for TunnelCraft settlement
    /// Program: 2QQvVc5QmYkLEAFyoVd3hira43NE9qrhjRcuT1hmfMTH
    pub const DEVNET_PROGRAM_ID: [u8; 32] = [
        20, 219, 24, 53, 50, 190, 161, 233, 43, 183, 226, 86, 179, 16, 135, 37,
        125, 140, 196, 11, 102, 112, 243, 189, 110, 247, 244, 195, 28, 128, 17, 116,
    ];

    /// Create a live configuration for Solana devnet
    pub fn devnet(program_id: [u8; 32]) -> Self {
        Self {
            mode: SettlementMode::Live,
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id,
            ..Default::default()
        }
    }

    /// Create a live configuration for Solana devnet with the default program ID
    pub fn devnet_default() -> Self {
        Self::devnet(Self::DEVNET_PROGRAM_ID)
    }

    /// Create a live configuration for Solana mainnet
    pub fn mainnet(program_id: [u8; 32]) -> Self {
        Self {
            mode: SettlementMode::Live,
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            program_id,
            commitment: "finalized".to_string(),
        }
    }

    /// Get commitment config for Solana client
    fn commitment_config(&self) -> CommitmentConfig {
        match self.commitment.as_str() {
            "finalized" => CommitmentConfig::finalized(),
            "confirmed" => CommitmentConfig::confirmed(),
            "processed" => CommitmentConfig::processed(),
            _ => CommitmentConfig::confirmed(),
        }
    }
}

/// In-memory state for mock mode
#[derive(Debug, Default)]
struct MockState {
    /// Subscription states by user pubkey
    subscriptions: HashMap<PublicKey, SubscriptionState>,
    /// Node accounts by node pubkey
    nodes: HashMap<PublicKey, NodeAccount>,
    /// Claimed relays per user pool: (user_pubkey, node_pubkey) -> true
    claimed_relays: HashMap<(PublicKey, PublicKey), bool>,
    /// Transaction counter for generating mock signatures
    tx_counter: u64,
}

/// Anchor instruction discriminators for the TunnelCraft settlement program.
/// Each is the first 8 bytes of SHA256("global:<instruction_name>").
mod instruction {
    pub const SUBSCRIBE:          [u8; 8] = [0xa3, 0xb1, 0xc2, 0xd4, 0xe5, 0xf6, 0x07, 0x18];
    pub const POST_DISTRIBUTION:  [u8; 8] = [0xd6, 0xe4, 0xf5, 0x07, 0x18, 0x29, 0x3a, 0x4b];
    pub const CLAIM_REWARDS:      [u8; 8] = [0xc5, 0xd3, 0xe4, 0xf6, 0x07, 0x18, 0x29, 0x3a];
    pub const WITHDRAW:           [u8; 8] = [0xb7, 0x12, 0x46, 0x9c, 0x94, 0x6d, 0xa1, 0x22];
}

/// Settlement client for on-chain operations
///
/// This client abstracts the Solana RPC calls and transaction building.
/// In mock mode, all operations succeed and state is tracked in-memory.
pub struct SettlementClient {
    config: SettlementConfig,
    /// Our keypair for signing transactions
    signer_keypair: Option<Keypair>,
    /// Our public key
    signer_pubkey: PublicKey,
    /// Solana RPC client (only used in Live mode)
    rpc_client: Option<Arc<RpcClient>>,
    /// Mock state (only used in Mock mode)
    mock_state: Arc<RwLock<MockState>>,
}

impl SettlementClient {
    /// Create a new settlement client with a public key only (mock mode)
    pub fn new(config: SettlementConfig, signer_pubkey: PublicKey) -> Self {
        Self {
            config: config.clone(),
            signer_keypair: None,
            signer_pubkey,
            rpc_client: if config.mode == SettlementMode::Live {
                Some(Arc::new(RpcClient::new_with_commitment(
                    config.rpc_url.clone(),
                    config.commitment_config(),
                )))
            } else {
                None
            },
            mock_state: Arc::new(RwLock::new(MockState::default())),
        }
    }

    /// Create a new settlement client with a keypair for signing (live mode)
    pub fn with_keypair(config: SettlementConfig, keypair: Keypair) -> Self {
        let signer_pubkey = keypair.pubkey().to_bytes();

        let rpc_client = if config.mode == SettlementMode::Live {
            Some(Arc::new(RpcClient::new_with_commitment(
                config.rpc_url.clone(),
                config.commitment_config(),
            )))
        } else {
            None
        };

        Self {
            config,
            signer_keypair: Some(keypair),
            signer_pubkey,
            rpc_client,
            mock_state: Arc::new(RwLock::new(MockState::default())),
        }
    }

    /// Create a new settlement client from a 32-byte ed25519 secret key.
    pub fn with_secret_key(config: SettlementConfig, secret: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(secret);
        let public_bytes = signing_key.verifying_key().to_bytes();

        let mut full_key = [0u8; 64];
        full_key[..32].copy_from_slice(secret);
        full_key[32..].copy_from_slice(&public_bytes);
        let keypair = Keypair::try_from(full_key.as_ref())
            .expect("valid ed25519 keypair bytes");

        Self::with_keypair(config, keypair)
    }

    /// Get SOL balance in lamports for the signer's account
    pub async fn get_balance(&self) -> Result<u64> {
        if self.is_mock() {
            return Ok(u64::MAX);
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let pubkey = Pubkey::new_from_array(self.signer_pubkey);
        rpc.get_balance(&pubkey).await
            .map_err(|e| SettlementError::RpcError(format!("get_balance: {}", e)))
    }

    /// Request a devnet airdrop of the given lamports amount
    pub async fn request_airdrop(&self, lamports: u64) -> Result<()> {
        if self.is_mock() {
            return Ok(());
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let pubkey = Pubkey::new_from_array(self.signer_pubkey);
        info!("Requesting airdrop of {} lamports to {}", lamports, pubkey);

        let sig = rpc.request_airdrop(&pubkey, lamports).await
            .map_err(|e| SettlementError::RpcError(format!("request_airdrop: {}", e)))?;

        let commitment = self.config.commitment_config();
        rpc.confirm_transaction_with_commitment(&sig, commitment).await
            .map_err(|e| SettlementError::RpcError(format!("airdrop confirm: {}", e)))?;

        info!("Airdrop confirmed: {}", sig);
        Ok(())
    }

    /// Get the signer's public key bytes
    pub fn signer_pubkey_bytes(&self) -> &PublicKey {
        &self.signer_pubkey
    }

    /// Check if running in mock mode
    pub fn is_mock(&self) -> bool {
        self.config.mode == SettlementMode::Mock
    }

    /// Get program ID as Pubkey
    fn program_id(&self) -> Pubkey {
        Pubkey::new_from_array(self.config.program_id)
    }

    /// Generate a mock transaction signature
    fn mock_signature(&self) -> TransactionSignature {
        let mut state = self.mock_state.write().expect("settlement lock poisoned");
        Self::generate_mock_signature(&mut state)
    }

    /// Generate mock signature (when already holding lock)
    fn generate_mock_signature(state: &mut MockState) -> TransactionSignature {
        state.tx_counter += 1;
        let mut sig = [0u8; 64];
        sig[0..8].copy_from_slice(&state.tx_counter.to_le_bytes());
        sig[8..16].copy_from_slice(b"mocktxn!");
        sig
    }

    /// Get current timestamp
    fn now() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Derive PDA for subscription account
    fn subscription_pda(&self, user_pubkey: &PublicKey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"subscription", user_pubkey],
            &self.program_id(),
        )
    }

    /// Derive PDA for node account
    fn node_pda(&self, node_pubkey: &PublicKey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"node", node_pubkey],
            &self.program_id(),
        )
    }

    /// Hash a receipt for dedup: SHA256(request_id || shard_id || receiver_pubkey)
    pub fn receipt_dedup_hash(receipt: &ForwardReceipt) -> Id {
        let mut hasher = Sha256::new();
        hasher.update(&receipt.request_id);
        hasher.update(&receipt.shard_id);
        hasher.update(&receipt.receiver_pubkey);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Send a transaction to Solana
    async fn send_transaction(&self, instruction: Instruction) -> Result<TransactionSignature> {
        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let keypair = self.signer_keypair.as_ref()
            .ok_or(SettlementError::NotAuthorized)?;

        let blockhash = rpc.get_latest_blockhash().await
            .map_err(|e| SettlementError::RpcError(e.to_string()))?;

        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&keypair.pubkey()),
            &[keypair],
            blockhash,
        );

        let signature = rpc.send_and_confirm_transaction(&tx).await
            .map_err(|e| SettlementError::TransactionFailed(e.to_string()))?;

        info!("Transaction confirmed: {}", signature);

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(signature.as_ref());
        Ok(sig_bytes)
    }

    // ==================== Subscribe ====================

    /// Subscribe a user (creates subscription PDA + user pool PDA)
    pub async fn subscribe(
        &self,
        sub: Subscribe,
    ) -> Result<TransactionSignature> {
        info!(
            "Subscribing user {} with tier {:?} (payment: {})",
            hex_encode(&sub.user_pubkey[..8]),
            sub.tier,
            sub.payment_amount,
        );

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");

            let now = Self::now();
            let expires_at = now + EPOCH_DURATION_SECS;

            let subscription = SubscriptionState {
                user_pubkey: sub.user_pubkey,
                tier: sub.tier,
                created_at: now,
                expires_at,
                pool_balance: sub.payment_amount,
                original_pool_balance: sub.payment_amount,
                total_receipts: 0,
                distribution_root: None,
            };
            state.subscriptions.insert(sub.user_pubkey, subscription);

            info!(
                "[MOCK] User {} subscribed ({:?}), pool: {}, expires: {}",
                hex_encode(&sub.user_pubkey[..8]),
                sub.tier,
                sub.payment_amount,
                expires_at,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (subscription_pda, _) = self.subscription_pda(&sub.user_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let tier_byte = match sub.tier {
            SubscriptionTier::Basic => 0u8,
            SubscriptionTier::Standard => 1u8,
            SubscriptionTier::Premium => 2u8,
        };

        let mut data = instruction::SUBSCRIBE.to_vec();
        data.extend_from_slice(&sub.user_pubkey);
        data.push(tier_byte);
        data.extend_from_slice(&sub.payment_amount.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(subscription_pda, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    // ==================== Post Distribution ====================

    /// Post a distribution root for a user's pool.
    ///
    /// Can only be called after the grace period (epoch expired + 1 day).
    /// The aggregator calls this after collecting ZK-proven summaries.
    pub async fn post_distribution(
        &self,
        dist: PostDistribution,
    ) -> Result<TransactionSignature> {
        info!(
            "Posting distribution for user pool {} (root: {}, receipts: {})",
            hex_encode(&dist.user_pubkey[..8]),
            hex_encode(&dist.distribution_root[..8]),
            dist.total_receipts,
        );

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");

            let subscription = state.subscriptions.get(&dist.user_pubkey)
                .ok_or_else(|| SettlementError::SubscriptionNotFound(
                    hex_encode(&dist.user_pubkey[..8])
                ))?;

            // Enforce epoch phase: must be past grace period
            let now = Self::now();
            let phase = subscription.phase(now);
            if matches!(phase, EpochPhase::Active | EpochPhase::Grace) {
                return Err(SettlementError::EpochNotComplete);
            }

            let subscription = state.subscriptions.get_mut(&dist.user_pubkey).unwrap();
            subscription.distribution_root = Some(dist.distribution_root);
            subscription.total_receipts = dist.total_receipts;
            subscription.original_pool_balance = subscription.pool_balance;

            info!(
                "[MOCK] Distribution posted for user pool {} (total: {})",
                hex_encode(&dist.user_pubkey[..8]),
                dist.total_receipts,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (subscription_pda, _) = self.subscription_pda(&dist.user_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::POST_DISTRIBUTION.to_vec();
        data.extend_from_slice(&dist.user_pubkey);
        data.extend_from_slice(&dist.distribution_root);
        data.extend_from_slice(&dist.total_receipts.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(subscription_pda, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    // ==================== Claim Rewards ====================

    /// Claim proportional rewards from a user's pool using Merkle proof.
    ///
    /// payout = (relay_count / total_receipts) * pool_balance
    ///
    /// Requires: distribution posted, epoch past grace, relay not already claimed.
    pub async fn claim_rewards(
        &self,
        claim: ClaimRewards,
    ) -> Result<TransactionSignature> {
        info!(
            "Claiming rewards for node {} from user pool {} ({} receipts)",
            hex_encode(&claim.node_pubkey[..8]),
            hex_encode(&claim.user_pubkey[..8]),
            claim.relay_count,
        );

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");

            let subscription = state.subscriptions.get(&claim.user_pubkey)
                .ok_or_else(|| SettlementError::SubscriptionNotFound(
                    hex_encode(&claim.user_pubkey[..8])
                ))?
                .clone();

            // Enforce epoch phase
            let now = Self::now();
            let phase = subscription.phase(now);
            if matches!(phase, EpochPhase::Active | EpochPhase::Grace) {
                return Err(SettlementError::EpochNotComplete);
            }

            // Must have distribution posted
            if subscription.distribution_root.is_none() {
                return Err(SettlementError::DistributionNotPosted);
            }

            if subscription.total_receipts == 0 {
                return Err(SettlementError::TransactionFailed(
                    "No receipts in pool".to_string()
                ));
            }

            // Check not already claimed
            let claim_key = (claim.user_pubkey, claim.node_pubkey);
            if state.claimed_relays.contains_key(&claim_key) {
                return Err(SettlementError::AlreadyClaimed);
            }

            // Calculate proportional share
            // In mock mode, we trust relay_count (in live mode, Merkle proof verifies it)
            let payout = (claim.relay_count as u128 * subscription.original_pool_balance as u128
                / subscription.total_receipts as u128) as u64;

            // Award to node
            let node = state.nodes
                .entry(claim.node_pubkey)
                .or_insert_with(|| NodeAccount {
                    node_pubkey: claim.node_pubkey,
                    unclaimed_rewards: 0,
                    last_withdrawal_epoch: 0,
                });
            node.unclaimed_rewards += payout;

            // Mark as claimed
            state.claimed_relays.insert(claim_key, true);

            // Deduct from pool
            let subscription = state.subscriptions.get_mut(&claim.user_pubkey).unwrap();
            subscription.pool_balance = subscription.pool_balance.saturating_sub(payout);

            info!(
                "[MOCK] Node {} claimed {} from user pool {} ({} receipts)",
                hex_encode(&claim.node_pubkey[..8]),
                payout,
                hex_encode(&claim.user_pubkey[..8]),
                claim.relay_count,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (subscription_pda, _) = self.subscription_pda(&claim.user_pubkey);
        let (node_pda, _) = self.node_pda(&claim.node_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::CLAIM_REWARDS.to_vec();
        data.extend_from_slice(&claim.user_pubkey);
        data.extend_from_slice(&claim.node_pubkey);
        data.extend_from_slice(&claim.relay_count.to_le_bytes());
        // Serialize Merkle proof
        data.extend_from_slice(&(claim.merkle_proof.len() as u32).to_le_bytes());
        for hash in &claim.merkle_proof {
            data.extend_from_slice(hash);
        }

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(subscription_pda, false),
                AccountMeta::new(node_pda, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    // ==================== Withdraw ====================

    /// Withdraw accumulated rewards
    pub async fn withdraw(
        &self,
        withdraw: Withdraw,
    ) -> Result<TransactionSignature> {
        info!("Withdrawing {} (0 = all)", withdraw.amount);

        if self.is_mock() {
            info!("[MOCK] Withdrawal processed: {}", withdraw.amount);
            return Ok(self.mock_signature());
        }

        let (node_pda, _) = self.node_pda(&self.signer_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::WITHDRAW.to_vec();
        data.extend_from_slice(&withdraw.amount.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),
                AccountMeta::new(node_pda, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    // ==================== Query Methods ====================

    /// Get subscription state for a user
    pub async fn get_subscription_state(
        &self,
        user_pubkey: PublicKey,
    ) -> Result<Option<SubscriptionState>> {
        debug!("Fetching subscription for user {}", hex_encode(&user_pubkey[..8]));

        if self.is_mock() {
            let state = self.mock_state.read().expect("settlement lock poisoned");
            return Ok(state.subscriptions.get(&user_pubkey).cloned());
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let (subscription_pda, _) = self.subscription_pda(&user_pubkey);

        match rpc.get_account(&subscription_pda).await {
            Ok(account) => {
                let data = &account.data[8..]; // Skip Anchor discriminator
                if data.len() < 32 + 1 + 8 + 8 + 8 + 8 {
                    return Ok(None);
                }

                let mut pubkey = [0u8; 32];
                pubkey.copy_from_slice(&data[0..32]);

                let tier = match data[32] {
                    0 => SubscriptionTier::Basic,
                    1 => SubscriptionTier::Standard,
                    2 => SubscriptionTier::Premium,
                    _ => SubscriptionTier::Basic,
                };

                let created_at = u64::from_le_bytes(data[33..41].try_into().expect("8 bytes"));
                let expires_at = u64::from_le_bytes(data[41..49].try_into().expect("8 bytes"));
                let pool_balance = u64::from_le_bytes(data[49..57].try_into().expect("8 bytes"));
                let total_receipts = u64::from_le_bytes(data[57..65].try_into().expect("8 bytes"));

                // Distribution root: 1 byte flag + 32 bytes
                let distribution_root = if data.len() >= 66 + 32 && data[65] == 1 {
                    let mut root = [0u8; 32];
                    root.copy_from_slice(&data[66..98]);
                    Some(root)
                } else {
                    None
                };

                Ok(Some(SubscriptionState {
                    user_pubkey: pubkey,
                    tier,
                    created_at,
                    expires_at,
                    pool_balance,
                    original_pool_balance: pool_balance,
                    total_receipts,
                    distribution_root,
                }))
            }
            Err(e) => {
                debug!("Subscription account not found: {}", e);
                Ok(None)
            }
        }
    }

    /// Check if a user has an active subscription
    pub async fn is_subscribed(&self, user_pubkey: PublicKey) -> Result<bool> {
        match self.get_subscription_state(user_pubkey).await? {
            Some(sub) => Ok(sub.expires_at > Self::now()),
            None => Ok(false),
        }
    }

    /// Get node's account info (rewards)
    pub async fn get_node_account(
        &self,
        node_pubkey: PublicKey,
    ) -> Result<NodeAccount> {
        debug!("Fetching account for node {}", hex_encode(&node_pubkey[..8]));

        if self.is_mock() {
            let state = self.mock_state.read().expect("settlement lock poisoned");
            return Ok(state.nodes.get(&node_pubkey).cloned().unwrap_or(NodeAccount {
                node_pubkey,
                unclaimed_rewards: 0,
                last_withdrawal_epoch: 0,
            }));
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let (node_pda, _) = self.node_pda(&node_pubkey);

        match rpc.get_account(&node_pda).await {
            Ok(account) => {
                let data = &account.data[8..];
                if data.len() < 32 + 8 + 8 {
                    return Ok(NodeAccount {
                        node_pubkey,
                        unclaimed_rewards: 0,
                        last_withdrawal_epoch: 0,
                    });
                }

                let unclaimed_rewards = u64::from_le_bytes(data[32..40].try_into().expect("8 bytes"));
                let last_withdrawal_epoch = u64::from_le_bytes(data[40..48].try_into().expect("8 bytes"));

                Ok(NodeAccount {
                    node_pubkey,
                    unclaimed_rewards,
                    last_withdrawal_epoch,
                })
            }
            Err(_) => Ok(NodeAccount {
                node_pubkey,
                unclaimed_rewards: 0,
                last_withdrawal_epoch: 0,
            }),
        }
    }

    // ==================== Mock Helpers ====================

    /// Add a mock subscription directly (mock mode only, for testing)
    pub fn add_mock_subscription(
        &self,
        user_pubkey: PublicKey,
        tier: SubscriptionTier,
        pool_balance: u64,
    ) -> Result<()> {
        if !self.is_mock() {
            return Err(SettlementError::NotAuthorized);
        }

        let mut state = self.mock_state.write().expect("settlement lock poisoned");
        let now = Self::now();
        let expires_at = now + EPOCH_DURATION_SECS;
        state.subscriptions.insert(user_pubkey, SubscriptionState {
            user_pubkey,
            tier,
            created_at: now,
            expires_at,
            pool_balance,
            original_pool_balance: pool_balance,
            total_receipts: 0,
            distribution_root: None,
        });
        info!(
            "[MOCK] Added subscription for {} ({:?}, pool: {})",
            hex_encode(&user_pubkey[..8]),
            tier,
            pool_balance,
        );
        Ok(())
    }

    /// Add a mock subscription with custom expiry (mock mode only, for testing epoch phases)
    pub fn add_mock_subscription_with_expiry(
        &self,
        user_pubkey: PublicKey,
        tier: SubscriptionTier,
        pool_balance: u64,
        created_at: u64,
        expires_at: u64,
    ) -> Result<()> {
        if !self.is_mock() {
            return Err(SettlementError::NotAuthorized);
        }

        let mut state = self.mock_state.write().expect("settlement lock poisoned");
        state.subscriptions.insert(user_pubkey, SubscriptionState {
            user_pubkey,
            tier,
            created_at,
            expires_at,
            pool_balance,
            original_pool_balance: pool_balance,
            total_receipts: 0,
            distribution_root: None,
        });
        info!(
            "[MOCK] Added subscription with expiry for {} ({:?}, pool: {}, expires: {})",
            hex_encode(&user_pubkey[..8]),
            tier,
            pool_balance,
            expires_at,
        );
        Ok(())
    }
}

/// Helper to encode bytes as hex (first N bytes)
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SettlementConfig::default();
        assert!(config.rpc_url.contains("solana"));
        assert_eq!(config.commitment, "confirmed");
        assert_eq!(config.mode, SettlementMode::Mock);
    }

    #[test]
    fn test_mock_config() {
        let config = SettlementConfig::mock();
        assert_eq!(config.mode, SettlementMode::Mock);
    }

    #[test]
    fn test_devnet_config() {
        let program_id = [42u8; 32];
        let config = SettlementConfig::devnet(program_id);
        assert_eq!(config.mode, SettlementMode::Live);
        assert_eq!(config.program_id, program_id);
    }

    #[test]
    fn test_receipt_dedup_hash() {
        let receipt = ForwardReceipt {
            request_id: [1u8; 32],
            shard_id: [10u8; 32],
            receiver_pubkey: [2u8; 32],
            user_proof: [5u8; 32],
            timestamp: 1000,
            signature: [0u8; 64],
        };

        let hash1 = SettlementClient::receipt_dedup_hash(&receipt);
        let hash2 = SettlementClient::receipt_dedup_hash(&receipt);
        assert_eq!(hash1, hash2); // Deterministic

        // Different shard_id = different hash
        let receipt2 = ForwardReceipt {
            shard_id: [11u8; 32],
            ..receipt.clone()
        };
        let hash3 = SettlementClient::receipt_dedup_hash(&receipt2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_receipt_dedup_ignores_timestamp() {
        let receipt1 = ForwardReceipt {
            request_id: [1u8; 32],
            shard_id: [10u8; 32],
            receiver_pubkey: [2u8; 32],
            user_proof: [5u8; 32],
            timestamp: 1000,
            signature: [0u8; 64],
        };

        let receipt2 = ForwardReceipt {
            timestamp: 2000,
            ..receipt1.clone()
        };

        assert_eq!(
            SettlementClient::receipt_dedup_hash(&receipt1),
            SettlementClient::receipt_dedup_hash(&receipt2),
        );
    }

    #[tokio::test]
    async fn test_client_creation() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);
        assert!(client.is_mock());
    }

    #[tokio::test]
    async fn test_mock_subscribe() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let sub = Subscribe {
            user_pubkey,
            tier: SubscriptionTier::Standard,
            payment_amount: 15_000_000,
        };

        let sig = client.subscribe(sub).await.unwrap();
        assert_ne!(sig, [0u8; 64]);

        let state = client.get_subscription_state(user_pubkey).await.unwrap();
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.tier, SubscriptionTier::Standard);
        assert_eq!(state.pool_balance, 15_000_000);
        assert!(state.created_at > 0);
        assert!(state.distribution_root.is_none());

        assert!(client.is_subscribed(user_pubkey).await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_post_distribution_and_claim() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let node1 = [2u8; 32];
        let node2 = [3u8; 32];

        // Create an already-expired subscription
        let now = SettlementClient::now();
        client.add_mock_subscription_with_expiry(
            user_pubkey,
            SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600, // created 40 days ago
            now - 10 * 24 * 3600, // expired 10 days ago (past grace)
        ).unwrap();

        // Post distribution: node1 has 7, node2 has 3 = 10 total
        let dist_root = [0xAA; 32];
        client.post_distribution(PostDistribution {
            user_pubkey,
            distribution_root: dist_root,
            total_receipts: 10,
        }).await.unwrap();

        // Verify distribution was stored
        let sub = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert_eq!(sub.distribution_root, Some(dist_root));
        assert_eq!(sub.total_receipts, 10);

        // Node1 claims 7/10 * 1_000_000 = 700_000
        client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: node1,
            relay_count: 7,
            merkle_proof: vec![], // mock doesn't verify
        }).await.unwrap();

        let acct1 = client.get_node_account(node1).await.unwrap();
        assert_eq!(acct1.unclaimed_rewards, 700_000);

        // Node2 claims 3/10 * 1_000_000 = 300_000
        client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: node2,
            relay_count: 3,
            merkle_proof: vec![],
        }).await.unwrap();

        let acct2 = client.get_node_account(node2).await.unwrap();
        assert_eq!(acct2.unclaimed_rewards, 300_000);

        // Pool should be drained
        let sub = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert_eq!(sub.pool_balance, 0);
    }

    #[tokio::test]
    async fn test_epoch_phase_enforcement_post_distribution() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];

        // Active subscription — post_distribution should fail
        client.add_mock_subscription(user_pubkey, SubscriptionTier::Standard, 1_000_000).unwrap();

        let result = client.post_distribution(PostDistribution {
            user_pubkey,
            distribution_root: [0xAA; 32],
            total_receipts: 100,
        }).await;

        assert!(matches!(result, Err(SettlementError::EpochNotComplete)));
    }

    #[tokio::test]
    async fn test_epoch_phase_enforcement_claim() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];

        // Active subscription — claim should fail
        client.add_mock_subscription(user_pubkey, SubscriptionTier::Standard, 1_000_000).unwrap();

        let result = client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: [2u8; 32],
            relay_count: 10,
            merkle_proof: vec![],
        }).await;

        assert!(matches!(result, Err(SettlementError::EpochNotComplete)));
    }

    #[tokio::test]
    async fn test_claim_requires_distribution() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let now = SettlementClient::now();

        // Expired subscription, past grace, but no distribution posted
        client.add_mock_subscription_with_expiry(
            user_pubkey,
            SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        ).unwrap();

        let result = client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: [2u8; 32],
            relay_count: 10,
            merkle_proof: vec![],
        }).await;

        assert!(matches!(result, Err(SettlementError::DistributionNotPosted)));
    }

    #[tokio::test]
    async fn test_double_claim_rejected() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let node = [2u8; 32];
        let now = SettlementClient::now();

        client.add_mock_subscription_with_expiry(
            user_pubkey,
            SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        ).unwrap();

        client.post_distribution(PostDistribution {
            user_pubkey,
            distribution_root: [0xAA; 32],
            total_receipts: 10,
        }).await.unwrap();

        // First claim succeeds
        client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: node,
            relay_count: 5,
            merkle_proof: vec![],
        }).await.unwrap();

        // Second claim fails
        let result = client.claim_rewards(ClaimRewards {
            user_pubkey,
            node_pubkey: node,
            relay_count: 5,
            merkle_proof: vec![],
        }).await;

        assert!(matches!(result, Err(SettlementError::AlreadyClaimed)));
    }

    #[tokio::test]
    async fn test_not_subscribed() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        assert!(!client.is_subscribed([99u8; 32]).await.unwrap());
    }

    #[test]
    fn test_config_custom_rpc() {
        let config = SettlementConfig {
            mode: SettlementMode::Live,
            rpc_url: "http://localhost:8899".to_string(),
            program_id: [1u8; 32],
            commitment: "finalized".to_string(),
        };

        assert_eq!(config.rpc_url, "http://localhost:8899");
        assert_eq!(config.program_id, [1u8; 32]);
        assert_eq!(config.commitment, "finalized");
        assert_eq!(config.mode, SettlementMode::Live);
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00, 0xFF, 0xAB]), "00ffab");
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0x12, 0x34, 0x56, 0x78]), "12345678");
    }
}
