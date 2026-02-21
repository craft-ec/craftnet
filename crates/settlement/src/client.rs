//! Settlement client for interacting with Solana
//!
//! Supports two modes:
//! - **Mock Mode**: For development/testing without Solana. All operations succeed
//!   and state is tracked in-memory.
//! - **Live Mode**: Actual Solana RPC calls to the CraftNet settlement program.
//!
//! ## New Settlement Model
//!
//! Per-epoch subscriptions with direct payout. Each subscribe() creates a new
//! epoch (monotonic counter per user via UserMeta PDA). Claims pay directly
//! from pool PDA to relay wallet — no NodeAccount accumulation step.
//! Double-claim prevented by Light Protocol compressed ClaimReceipt
//! (in mock: HashSet dedup simulates compressed account uniqueness).

use std::collections::{HashMap, HashSet};
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

use craftnet_core::{Id, PublicKey, ForwardReceipt, SubscriptionTier};

use crate::{
    SettlementError, Result,
    Subscribe, PostDistribution, ClaimRewards,
    SubscriptionState, TransactionSignature,
    EpochPhase, PricingPlanState,
    USDC_MINT_DEVNET, USDC_MINT_MAINNET,
    LightTreeConfig,
};
use crate::light::{self, PhotonClient};

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
    /// Program ID for the CraftNet settlement program
    pub program_id: [u8; 32],
    /// USDC mint address (6 decimal SPL token)
    pub usdc_mint: [u8; 32],
    /// Commitment level for transactions
    pub commitment: String,
    /// Helius API key for Photon RPC (Light Protocol validity proofs).
    /// If None, falls back to `rpc_url` for Photon calls.
    pub helius_api_key: Option<String>,
    /// Light Protocol tree configuration for compressed ClaimReceipts.
    /// If None, auto-fetch of Light params in `claim_rewards()` is disabled.
    pub light_trees: Option<LightTreeConfig>,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            mode: SettlementMode::Mock,
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id: [0u8; 32],
            usdc_mint: USDC_MINT_DEVNET,
            commitment: "confirmed".to_string(),
            helius_api_key: None,
            light_trees: None,
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

    /// Devnet program ID for CraftNet settlement
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
            usdc_mint: USDC_MINT_DEVNET,
            light_trees: Some(LightTreeConfig::devnet_v2()),
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
            usdc_mint: USDC_MINT_MAINNET,
            commitment: "finalized".to_string(),
            helius_api_key: None,
            light_trees: None,
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
    /// Subscription states by pool_pubkey
    subscriptions: HashMap<PublicKey, SubscriptionState>,
    /// Claimed relays: (pool_pubkey, relay_pubkey) — simulates
    /// Light Protocol compressed ClaimReceipt uniqueness
    claimed_relays: HashSet<(PublicKey, PublicKey)>,
    /// Pricing plans: (tier, billing_period) → plan state
    pricing_plans: HashMap<(u8, u8), PricingPlanState>,
    /// Whether config has been initialized (admin set)
    config_admin: Option<PublicKey>,
    /// Transaction counter for generating mock signatures
    tx_counter: u64,
}

/// Anchor instruction discriminators for the CraftNet settlement program.
/// Each is the first 8 bytes of SHA256("global:<instruction_name>").
mod instruction {
    pub const SUBSCRIBE:            [u8; 8] = [0xfe, 0x1c, 0xbf, 0x8a, 0x9c, 0xb3, 0xb7, 0x35];
    pub const POST_DISTRIBUTION:    [u8; 8] = [0x0e, 0xa8, 0xf7, 0x4a, 0xbf, 0x7b, 0x15, 0xe8];
    pub const CLAIM:                [u8; 8] = [0x3e, 0xc6, 0xd6, 0xc1, 0xd5, 0x9f, 0x6c, 0xd2];
    pub const INITIALIZE_CONFIG:    [u8; 8] = [0xd0, 0x7f, 0x15, 0x01, 0xc2, 0xbe, 0xc4, 0x46];
    pub const CREATE_PLAN:          [u8; 8] = [0x4d, 0x2b, 0x8d, 0xfe, 0xd4, 0x76, 0x29, 0xba];
    pub const UPDATE_PLAN:          [u8; 8] = [0x77, 0x70, 0x3a, 0x3c, 0x4c, 0xcd, 0x01, 0x64];
    pub const DELETE_PLAN:          [u8; 8] = [0x29, 0x6f, 0xa9, 0xd2, 0x5d, 0x8d, 0x6c, 0x35];
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

    /// Create a Photon client from the current config.
    fn photon_client(&self) -> Result<PhotonClient> {
        Ok(PhotonClient::from_config(
            &self.config.rpc_url,
            self.config.helius_api_key.as_deref(),
        ))
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

    /// Get current on-chain time (slot timestamp) via RPC.
    /// In mock mode, uses local system time.
    async fn get_chain_time(&self) -> Option<i64> {
        if let Some(rpc) = self.rpc_client.as_ref() {
            let slot = rpc.get_slot().await.ok()?;
            let block_time = rpc.get_block_time(slot).await.ok()?;
            Some(block_time)
        } else {
            Some(Self::now() as i64)
        }
    }

    /// Derive PDA for pool subscription account: ["pool", pool_pubkey]
    fn subscription_pda(&self, pool_pubkey: &PublicKey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"pool", pool_pubkey],
            &self.program_id(),
        )
    }

    /// Derive PDA for global config: ["config"]
    fn config_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"config"],
            &self.program_id(),
        )
    }

    /// Derive PDA for pricing plan: ["plan", &[tier], &[billing_period]]
    fn pricing_plan_pda(&self, tier: u8, billing_period: u8) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"plan", &[tier], &[billing_period]],
            &self.program_id(),
        )
    }

    /// Derive associated token account address for a given wallet and mint.
    ///
    /// ATA PDA = find_program_address(
    ///   [wallet, TOKEN_PROGRAM_ID, mint],
    ///   ASSOCIATED_TOKEN_PROGRAM_ID,
    /// )
    fn associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
        // SPL Associated Token Account program ID
        let ata_program_id = Pubkey::new_from_array([
            140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131,
            11, 90, 19, 153, 218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
        ]); // ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL
        let token_program_id = Pubkey::new_from_array([
            6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
            28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
        ]); // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA

        let (ata, _) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_program_id,
        );
        ata
    }

    /// USDC mint pubkey from config
    fn usdc_mint(&self) -> Pubkey {
        Pubkey::new_from_array(self.config.usdc_mint)
    }

    /// Hash a receipt for dedup: SHA256(shard_id || sender_pubkey || receiver_pubkey)
    pub fn receipt_dedup_hash(receipt: &ForwardReceipt) -> Id {
        let mut hasher = Sha256::new();
        hasher.update(receipt.shard_id);
        hasher.update(receipt.sender_pubkey);
        hasher.update(receipt.receiver_pubkey);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Send a transaction with a single instruction to Solana
    async fn send_transaction(&self, instruction: Instruction) -> Result<TransactionSignature> {
        self.send_transaction_multi(vec![instruction]).await
    }

    /// Send a transaction with multiple instructions to Solana
    async fn send_transaction_multi(&self, instructions: Vec<Instruction>) -> Result<TransactionSignature> {
        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let keypair = self.signer_keypair.as_ref()
            .ok_or(SettlementError::NotAuthorized)?;

        let blockhash = rpc.get_latest_blockhash().await
            .map_err(|e| SettlementError::RpcError(e.to_string()))?;

        let tx = Transaction::new_signed_with_payer(
            &instructions,
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

    // ==================== Config & Pricing Plans ====================

    /// Initialize the global config PDA (sets admin). One-time call.
    pub async fn initialize_config(&self) -> Result<TransactionSignature> {
        info!("Initializing config with admin {}", hex_encode(&self.signer_pubkey[..8]));

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");
            if state.config_admin.is_some() {
                return Err(SettlementError::TransactionFailed(
                    "Config already initialized".to_string()
                ));
            }
            state.config_admin = Some(self.signer_pubkey);
            info!("[MOCK] Config initialized, admin: {}", hex_encode(&self.signer_pubkey[..8]));
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (config_pda, _) = self.config_pda();
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                         // admin (signer + payer)
                AccountMeta::new(config_pda, false),                    // config PDA (init)
                AccountMeta::new_readonly(system_program::id(), false), // system_program
            ],
            data: instruction::INITIALIZE_CONFIG.to_vec(),
        };

        self.send_transaction(instruction).await
    }

    /// Create a new pricing plan. Requires admin signer.
    pub async fn create_plan(
        &self,
        tier: u8,
        billing_period: u8,
        price_usdc: u64,
    ) -> Result<TransactionSignature> {
        info!(
            "Creating plan: tier={}, period={}, price={}",
            tier, billing_period, price_usdc,
        );

        if tier > 2 {
            return Err(SettlementError::TransactionFailed("tier must be 0-2".to_string()));
        }
        if billing_period > 1 {
            return Err(SettlementError::TransactionFailed("billing_period must be 0-1".to_string()));
        }
        if price_usdc == 0 {
            return Err(SettlementError::TransactionFailed("price must be > 0".to_string()));
        }

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");
            // Verify admin
            match state.config_admin {
                Some(admin) if admin == self.signer_pubkey => {}
                Some(_) => return Err(SettlementError::NotAuthorized),
                None => return Err(SettlementError::TransactionFailed("Config not initialized".to_string())),
            }

            let key = (tier, billing_period);
            if state.pricing_plans.contains_key(&key) {
                return Err(SettlementError::TransactionFailed(
                    format!("Plan ({}, {}) already exists", tier, billing_period)
                ));
            }

            let now = Self::now() as i64;
            state.pricing_plans.insert(key, PricingPlanState {
                tier,
                billing_period,
                price_usdc,
                active: true,
                updated_at: now,
            });
            info!("[MOCK] Plan created: tier={}, period={}, price={}", tier, billing_period, price_usdc);
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (config_pda, _) = self.config_pda();
        let (plan_pda, _) = self.pricing_plan_pda(tier, billing_period);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::CREATE_PLAN.to_vec();
        data.push(tier);
        data.push(billing_period);
        data.extend_from_slice(&price_usdc.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                         // admin
                AccountMeta::new_readonly(config_pda, false),           // config (has_one admin)
                AccountMeta::new(plan_pda, false),                      // pricing_plan (init)
                AccountMeta::new_readonly(system_program::id(), false), // system_program
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    /// Update a pricing plan's price. Requires admin signer.
    pub async fn update_plan(
        &self,
        tier: u8,
        billing_period: u8,
        new_price_usdc: u64,
    ) -> Result<TransactionSignature> {
        info!(
            "Updating plan: tier={}, period={}, new_price={}",
            tier, billing_period, new_price_usdc,
        );

        if new_price_usdc == 0 {
            return Err(SettlementError::TransactionFailed("price must be > 0".to_string()));
        }

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");
            match state.config_admin {
                Some(admin) if admin == self.signer_pubkey => {}
                Some(_) => return Err(SettlementError::NotAuthorized),
                None => return Err(SettlementError::TransactionFailed("Config not initialized".to_string())),
            }

            let key = (tier, billing_period);
            let plan = state.pricing_plans.get_mut(&key)
                .ok_or(SettlementError::PlanNotFound)?;
            plan.price_usdc = new_price_usdc;
            plan.updated_at = Self::now() as i64;
            info!("[MOCK] Plan updated: tier={}, period={}, price={}", tier, billing_period, new_price_usdc);
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (config_pda, _) = self.config_pda();
        let (plan_pda, _) = self.pricing_plan_pda(tier, billing_period);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::UPDATE_PLAN.to_vec();
        data.extend_from_slice(&new_price_usdc.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                // admin
                AccountMeta::new_readonly(config_pda, false),  // config (has_one admin)
                AccountMeta::new(plan_pda, false),             // pricing_plan (mut)
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    /// Delete (deactivate) a pricing plan. Requires admin signer.
    pub async fn delete_plan(
        &self,
        tier: u8,
        billing_period: u8,
    ) -> Result<TransactionSignature> {
        info!("Deleting plan: tier={}, period={}", tier, billing_period);

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");
            match state.config_admin {
                Some(admin) if admin == self.signer_pubkey => {}
                Some(_) => return Err(SettlementError::NotAuthorized),
                None => return Err(SettlementError::TransactionFailed("Config not initialized".to_string())),
            }

            let key = (tier, billing_period);
            let plan = state.pricing_plans.get_mut(&key)
                .ok_or(SettlementError::PlanNotFound)?;
            plan.active = false;
            plan.updated_at = Self::now() as i64;
            info!("[MOCK] Plan deactivated: tier={}, period={}", tier, billing_period);
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (config_pda, _) = self.config_pda();
        let (plan_pda, _) = self.pricing_plan_pda(tier, billing_period);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                // admin
                AccountMeta::new_readonly(config_pda, false),  // config (has_one admin)
                AccountMeta::new(plan_pda, false),             // pricing_plan (mut)
            ],
            data: instruction::DELETE_PLAN.to_vec(),
        };

        self.send_transaction(instruction).await
    }

    /// Get a pricing plan by tier and billing period
    pub async fn get_pricing_plan(
        &self,
        tier: u8,
        billing_period: u8,
    ) -> Result<Option<PricingPlanState>> {
        if self.is_mock() {
            let state = self.mock_state.read().expect("settlement lock poisoned");
            return Ok(state.pricing_plans.get(&(tier, billing_period)).cloned());
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let (plan_pda, _) = self.pricing_plan_pda(tier, billing_period);

        match rpc.get_account(&plan_pda).await {
            Ok(account) => {
                let data = &account.data;
                // PricingPlan layout (after 8-byte discriminator):
                //  0..1:  tier u8
                //  1..2:  billing_period u8
                //  2..10: price_usdc u64
                // 10..11: active bool
                // 11..19: updated_at i64
                const MIN_LEN: usize = 8 + 1 + 1 + 8 + 1 + 8; // = 27
                if data.len() < MIN_LEN {
                    return Ok(None);
                }
                let d = &data[8..]; // skip discriminator

                Ok(Some(PricingPlanState {
                    tier: d[0],
                    billing_period: d[1],
                    price_usdc: u64::from_le_bytes(d[2..10].try_into().expect("8 bytes")),
                    active: d[10] != 0,
                    updated_at: i64::from_le_bytes(d[11..19].try_into().expect("8 bytes")),
                }))
            }
            Err(_) => Ok(None),
        }
    }

    /// Get all active pricing plans.
    ///
    /// Queries all 6 possible (tier, billing_period) combinations: 3 tiers x 2 periods.
    pub async fn get_all_plans(&self) -> Result<Vec<PricingPlanState>> {
        if self.is_mock() {
            let state = self.mock_state.read().expect("settlement lock poisoned");
            return Ok(state.pricing_plans.values().cloned().collect());
        }

        let mut plans = Vec::new();
        for tier in 0..=2u8 {
            for period in 0..=1u8 {
                if let Some(plan) = self.get_pricing_plan(tier, period).await? {
                    plans.push(plan);
                }
            }
        }
        Ok(plans)
    }

    // ==================== Subscribe ====================

    /// Subscribe a user (creates pool subscription PDA)
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
            // start_date: 0 means use current time, positive means future-dated (yearly months)
            let start_date = if sub.start_date <= 0 { now } else { sub.start_date as u64 };
            let expires_at = start_date + sub.duration_secs;

            let subscription = SubscriptionState {
                pool_pubkey: sub.user_pubkey,
                tier: sub.tier,
                start_date,
                created_at: now,
                expires_at,
                pool_balance: sub.payment_amount,
                original_pool_balance: sub.payment_amount,
                total_bytes: 0,
                distribution_posted: false,
                distribution_root: [0u8; 32],
            };
            state.subscriptions.insert(sub.user_pubkey, subscription);

            info!(
                "[MOCK] User {} subscribed ({:?}, pool: {}, start: {}, expires: {})",
                hex_encode(&sub.user_pubkey[..8]),
                sub.tier,
                sub.payment_amount,
                start_date,
                expires_at,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (subscription_pda, _) = self.subscription_pda(&sub.user_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);
        let usdc_mint = self.usdc_mint();

        let payer_token_account = Self::associated_token_address(&signer, &usdc_mint);
        let pool_token_account = Self::associated_token_address(&subscription_pda, &usdc_mint);

        let tier_byte = match sub.tier {
            SubscriptionTier::Basic => 0u8,
            SubscriptionTier::Standard => 1u8,
            SubscriptionTier::Premium => 2u8,
            SubscriptionTier::Ultra => 3u8,
        };

        let mut data = instruction::SUBSCRIBE.to_vec();
        data.extend_from_slice(&sub.user_pubkey);
        data.push(tier_byte);
        data.extend_from_slice(&sub.payment_amount.to_le_bytes());
        data.extend_from_slice(&sub.duration_secs.to_le_bytes());
        data.extend_from_slice(&sub.start_date.to_le_bytes());

        // SPL Token and ATA program IDs
        let token_program_id = Pubkey::new_from_array([
            6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
            28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
        ]);
        let ata_program_id = Pubkey::new_from_array([
            140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131,
            11, 90, 19, 153, 218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
        ]);

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                              // payer
                AccountMeta::new(subscription_pda, false),                   // subscription_account
                AccountMeta::new(payer_token_account, false),                // payer_token_account
                AccountMeta::new(pool_token_account, false),                 // pool_token_account
                AccountMeta::new_readonly(usdc_mint, false),                 // usdc_mint
                AccountMeta::new_readonly(token_program_id, false),          // token_program
                AccountMeta::new_readonly(ata_program_id, false),            // associated_token_program
                AccountMeta::new_readonly(system_program::id(), false),      // system_program
            ],
            data,
        };

        self.send_transaction(instruction).await
    }

    /// Subscribe for a full year as 12 independent monthly pool PDAs.
    ///
    /// Each month gets its own pool keypair and SubscriptionAccount.
    /// Month 0 starts at `now`, month N starts at `now + N*30d`.
    /// Payment per month: `yearly_price / 12` (month 11 gets remainder).
    ///
    /// Returns 12 (pool_pubkey, tx_signature) pairs.
    pub async fn subscribe_yearly(
        &self,
        user_pubkey: PublicKey,
        tier: SubscriptionTier,
        yearly_price: u64,
        period_secs: u64,
    ) -> Result<Vec<(PublicKey, TransactionSignature)>> {
        info!(
            "Creating yearly subscription for {} ({:?}, total: {}, period={}s)",
            hex_encode(&user_pubkey[..8]),
            tier,
            yearly_price,
            period_secs,
        );

        let monthly_amount = yearly_price / 12;
        let month_duration = period_secs / 12; // period_secs is total, each month = total / 12
        let nonce = (Self::now() as u64).to_le_bytes();

        // Get on-chain time as base — avoids client/chain clock skew.
        // All 12 months are independent and can be created atomically.
        let base_start = self.get_chain_time().await
            .ok_or_else(|| SettlementError::RpcError("Failed to get on-chain time".into()))?;

        let mut results = Vec::with_capacity(12);

        for month in 0u8..12 {
            let mut pool_pubkey = user_pubkey;
            pool_pubkey[24..32].copy_from_slice(&nonce);
            pool_pubkey[23] = month;

            let payment = if month == 11 {
                yearly_price - monthly_amount * 11 // remainder
            } else {
                monthly_amount
            };

            let start_date = base_start + (month as i64) * (month_duration as i64);

            let sig = self.subscribe(Subscribe {
                user_pubkey: pool_pubkey,
                tier,
                payment_amount: payment,
                duration_secs: month_duration,
                start_date,
            }).await?;

            results.push((pool_pubkey, sig));
        }

        info!(
            "[YEARLY] Created 12 monthly pools for {} ({:?})",
            hex_encode(&user_pubkey[..8]),
            tier,
        );
        Ok(results)
    }

    // ==================== Post Distribution ====================

    /// Post a distribution root for a pool.
    ///
    /// Can only be called after the grace period (subscription expired + grace).
    /// The aggregator calls this after collecting ZK-proven summaries.
    pub async fn post_distribution(
        &self,
        dist: PostDistribution,
    ) -> Result<TransactionSignature> {
        info!(
            "Posting distribution for pool {} (root: {}, bytes: {})",
            hex_encode(&dist.pool_pubkey[..8]),
            hex_encode(&dist.distribution_root[..8]),
            dist.total_bytes,
        );

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");

            let subscription = state.subscriptions.get(&dist.pool_pubkey)
                .ok_or_else(|| SettlementError::SubscriptionNotFound(
                    format!("{}", hex_encode(&dist.pool_pubkey[..8]))
                ))?;

            // Enforce phase: must be past grace period
            let now = Self::now();
            let phase = subscription.phase(now);
            if matches!(phase, EpochPhase::Active | EpochPhase::Grace) {
                return Err(SettlementError::PoolNotClaimable);
            }

            // First-writer-wins: reject if distribution already posted
            if subscription.distribution_posted {
                return Err(SettlementError::DistributionAlreadyPosted);
            }

            let subscription = state.subscriptions.get_mut(&dist.pool_pubkey).unwrap();
            subscription.distribution_posted = true;
            subscription.distribution_root = dist.distribution_root;
            subscription.total_bytes = dist.total_bytes;
            subscription.original_pool_balance = subscription.pool_balance;

            info!(
                "[MOCK] Distribution posted for pool {} (total: {})",
                hex_encode(&dist.pool_pubkey[..8]),
                dist.total_bytes,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode
        let (subscription_pda, _) = self.subscription_pda(&dist.pool_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);

        let mut data = instruction::POST_DISTRIBUTION.to_vec();
        data.extend_from_slice(&dist.pool_pubkey);
        data.extend_from_slice(&dist.distribution_root);
        data.extend_from_slice(&dist.total_bytes.to_le_bytes());
        // Serialize Groth16 proof (4-byte LE length prefix + bytes)
        data.extend_from_slice(&(dist.groth16_proof.len() as u32).to_le_bytes());
        data.extend_from_slice(&dist.groth16_proof);
        // Serialize SP1 public inputs (4-byte LE length prefix + bytes)
        data.extend_from_slice(&(dist.sp1_public_inputs.len() as u32).to_le_bytes());
        data.extend_from_slice(&dist.sp1_public_inputs);

        // Prepend compute budget if proof is present (Groth16 verification needs more CUs)
        let mut instructions = Vec::new();
        if !dist.groth16_proof.is_empty() {
            use solana_sdk::compute_budget::ComputeBudgetInstruction;
            instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(400_000));
        }

        let instruction = Instruction {
            program_id: self.program_id(),
            accounts: vec![
                AccountMeta::new(signer, true),                 // signer
                AccountMeta::new(subscription_pda, false),      // subscription_account
            ],
            data,
        };
        instructions.push(instruction);

        self.send_transaction_multi(instructions).await
    }

    // ==================== Claim Rewards ====================

    /// Claim proportional rewards from a pool using Merkle proof.
    ///
    /// Payout transfers directly from pool PDA to relay wallet (no NodeAccount).
    /// payout = (relay_bytes / total_bytes) * pool_balance
    ///
    /// Requires: distribution posted, pool past grace, relay not already claimed.
    /// Double-claim prevented by compressed ClaimReceipt (mock: HashSet dedup).
    pub async fn claim_rewards(
        &self,
        claim: ClaimRewards,
    ) -> Result<TransactionSignature> {
        info!(
            "Claiming rewards for node {} from pool {} ({} bytes)",
            hex_encode(&claim.node_pubkey[..8]),
            hex_encode(&claim.pool_pubkey[..8]),
            claim.relay_bytes,
        );

        if self.is_mock() {
            let mut state = self.mock_state.write().expect("settlement lock poisoned");

            let subscription = state.subscriptions.get(&claim.pool_pubkey)
                .ok_or_else(|| SettlementError::SubscriptionNotFound(
                    format!("{}", hex_encode(&claim.pool_pubkey[..8]))
                ))?
                .clone();

            // Enforce phase
            let now = Self::now();
            let phase = subscription.phase(now);
            if matches!(phase, EpochPhase::Active | EpochPhase::Grace) {
                return Err(SettlementError::PoolNotClaimable);
            }

            // Must have distribution posted
            if !subscription.distribution_posted {
                return Err(SettlementError::DistributionNotPosted);
            }

            if subscription.total_bytes == 0 {
                return Err(SettlementError::TransactionFailed(
                    "No bytes in pool".to_string()
                ));
            }

            // Check not already claimed (simulates compressed account uniqueness)
            let claim_key = (claim.pool_pubkey, claim.node_pubkey);
            if state.claimed_relays.contains(&claim_key) {
                return Err(SettlementError::AlreadyClaimed);
            }

            // Verify Merkle proof if distribution root and proof are provided
            if subscription.distribution_posted && !claim.merkle_proof.is_empty() {
                use craftnet_prover::{merkle_leaf, MerkleProof, MerkleTree};
                let leaf = merkle_leaf(&claim.node_pubkey, claim.relay_bytes);
                let proof = MerkleProof {
                    siblings: claim.merkle_proof.clone(),
                    leaf_index: claim.leaf_index as usize,
                };
                if !MerkleTree::verify(&subscription.distribution_root, &leaf, &proof) {
                    return Err(SettlementError::InvalidMerkleProof);
                }
            }

            // Calculate proportional share (direct payout)
            let payout = (claim.relay_bytes as u128 * subscription.original_pool_balance as u128
                / subscription.total_bytes as u128) as u64;

            // Mark as claimed (simulates compressed ClaimReceipt creation)
            state.claimed_relays.insert(claim_key);

            // Deduct from pool (direct transfer to relay wallet)
            let subscription = state.subscriptions.get_mut(&claim.pool_pubkey).unwrap();
            subscription.pool_balance = subscription.pool_balance.saturating_sub(payout);

            info!(
                "[MOCK] Node {} claimed {} from pool {} ({} bytes, direct payout)",
                hex_encode(&claim.node_pubkey[..8]),
                payout,
                hex_encode(&claim.pool_pubkey[..8]),
                claim.relay_bytes,
            );
            return Ok(Self::generate_mock_signature(&mut state));
        }

        // Live mode — auto-fetch Light params if not provided
        let trees = self.config.light_trees.as_ref()
            .ok_or_else(|| SettlementError::TransactionFailed(
                "light_trees config required for live-mode claim".to_string()
            ))?;

        let (light, remaining_accounts) = match claim.light_params {
            Some(ref params) => {
                // Caller provided params; still build remaining accounts
                let remaining = light::build_claim_remaining_accounts(
                    &self.config.program_id,
                    trees,
                );
                (params.clone(), remaining.accounts)
            }
            None => {
                // Auto-fetch from Photon
                let photon = self.photon_client()?;
                let result = light::prepare_claim_light_params(
                    &photon,
                    &claim.pool_pubkey,
                    &claim.node_pubkey,
                    &self.config.program_id,
                    trees,
                ).await?;
                (result.light_params, result.remaining_accounts)
            }
        };

        let (subscription_pda, _) = self.subscription_pda(&claim.pool_pubkey);
        let signer = Pubkey::new_from_array(self.signer_pubkey);
        let usdc_mint = self.usdc_mint();

        let pool_token_account = Self::associated_token_address(&subscription_pda, &usdc_mint);
        let relay_wallet = Pubkey::new_from_array(claim.node_pubkey);
        let relay_token_account = Self::associated_token_address(&relay_wallet, &usdc_mint);

        let token_program_id = Pubkey::new_from_array([
            6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
            28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
        ]);

        let mut data = instruction::CLAIM.to_vec();
        data.extend_from_slice(&claim.pool_pubkey);
        data.extend_from_slice(&claim.node_pubkey);
        data.extend_from_slice(&claim.relay_bytes.to_le_bytes());
        data.extend_from_slice(&claim.leaf_index.to_le_bytes());
        // Serialize Merkle proof (Anchor Vec: 4-byte LE length prefix + elements)
        data.extend_from_slice(&(claim.merkle_proof.len() as u32).to_le_bytes());
        for hash in &claim.merkle_proof {
            data.extend_from_slice(hash);
        }

        // Serialize LightClaimParams
        // LightValidityProof { a: [u8;32], b: [u8;64], c: [u8;32] }
        data.extend_from_slice(&light.proof_a);
        data.extend_from_slice(&light.proof_b);
        data.extend_from_slice(&light.proof_c);
        // LightAddressTreeInfo { pubkey_index: u8, queue_index: u8, root_index: u16 }
        data.push(light.address_merkle_tree_pubkey_index);
        data.push(light.address_queue_pubkey_index);
        data.extend_from_slice(&light.root_index.to_le_bytes());
        // output_tree_index: u8
        data.push(light.output_tree_index);

        // Build accounts: fixed accounts + Light Protocol remaining accounts
        let mut accounts = vec![
            AccountMeta::new(signer, true),                         // signer
            AccountMeta::new(subscription_pda, false),              // subscription_account
            AccountMeta::new(pool_token_account, false),            // pool_token_account
            AccountMeta::new_readonly(relay_wallet, false),         // relay_wallet
            AccountMeta::new(relay_token_account, false),           // relay_token_account
            AccountMeta::new_readonly(usdc_mint, false),            // usdc_mint
            AccountMeta::new_readonly(token_program_id, false),     // token_program
            AccountMeta::new_readonly(system_program::id(), false), // system_program
        ];
        accounts.extend(remaining_accounts);

        let claim_ix = Instruction {
            program_id: self.program_id(),
            accounts,
            data,
        };

        // Create relay ATA idempotently (noop if already exists)
        let ata_program_id = Pubkey::new_from_array([
            140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131,
            11, 90, 19, 153, 218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
        ]); // ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL
        let create_ata_ix = Instruction {
            program_id: ata_program_id,
            accounts: vec![
                AccountMeta::new(signer, true),                         // funding
                AccountMeta::new(relay_token_account, false),           // associated token
                AccountMeta::new_readonly(relay_wallet, false),         // wallet
                AccountMeta::new_readonly(usdc_mint, false),            // mint
                AccountMeta::new_readonly(system_program::id(), false), // system program
                AccountMeta::new_readonly(token_program_id, false),     // token program
            ],
            data: vec![1], // CreateIdempotent discriminant
        };

        self.send_transaction_multi(vec![create_ata_ix, claim_ix]).await
    }

    // ==================== Query Methods ====================

    /// Get subscription state for a pool
    pub async fn get_subscription_state(
        &self,
        pool_pubkey: PublicKey,
    ) -> Result<Option<SubscriptionState>> {
        debug!("Fetching subscription for pool {}", hex_encode(&pool_pubkey[..8]));

        if self.is_mock() {
            let state = self.mock_state.read().expect("settlement lock poisoned");
            return Ok(state.subscriptions.get(&pool_pubkey).cloned());
        }

        let rpc = self.rpc_client.as_ref()
            .ok_or_else(|| SettlementError::RpcError("RPC client not initialized".to_string()))?;

        let (subscription_pda, _) = self.subscription_pda(&pool_pubkey);

        match rpc.get_account(&subscription_pda).await {
            Ok(account) => {
                let data = &account.data;
                // SubscriptionAccount layout (after 8-byte discriminator):
                //   0..32:  pool_pubkey [u8; 32]
                //  32..33:  tier u8
                //  33..41:  start_date i64
                //  41..49:  created_at i64
                //  49..57:  expires_at i64
                //  57..65:  pool_balance u64
                //  65..73:  original_pool_balance u64
                //  73..81:  total_bytes u64
                //  81..113: distribution_root [u8; 32]
                // 113..114: distribution_posted bool
                const MIN_LEN: usize = 8 + 32 + 1 + 8 + 8 + 8 + 8 + 8 + 8 + 32 + 1; // = 122
                if data.len() < MIN_LEN {
                    return Ok(None);
                }
                let d = &data[8..]; // skip discriminator

                let mut pubkey = [0u8; 32];
                pubkey.copy_from_slice(&d[0..32]);

                let tier = match d[32] {
                    0 => SubscriptionTier::Basic,
                    1 => SubscriptionTier::Standard,
                    2 => SubscriptionTier::Premium,
                    3 => SubscriptionTier::Ultra,
                    _ => SubscriptionTier::Basic,
                };

                let start_date = i64::from_le_bytes(d[33..41].try_into().expect("8 bytes"));
                let created_at = i64::from_le_bytes(d[41..49].try_into().expect("8 bytes"));
                let expires_at = i64::from_le_bytes(d[49..57].try_into().expect("8 bytes"));
                let pool_balance = u64::from_le_bytes(d[57..65].try_into().expect("8 bytes"));
                let original_pool_balance = u64::from_le_bytes(d[65..73].try_into().expect("8 bytes"));
                let total_bytes = u64::from_le_bytes(d[73..81].try_into().expect("8 bytes"));

                let mut distribution_root = [0u8; 32];
                distribution_root.copy_from_slice(&d[81..113]);
                let distribution_posted = d[113] != 0;

                Ok(Some(SubscriptionState {
                    pool_pubkey: pubkey,
                    tier,
                    start_date: start_date as u64,
                    created_at: created_at as u64,
                    expires_at: expires_at as u64,
                    pool_balance,
                    original_pool_balance,
                    total_bytes,
                    distribution_posted,
                    distribution_root,
                }))
            }
            Err(e) => {
                debug!("Subscription account not found: {}", e);
                Ok(None)
            }
        }
    }

    /// Get the subscription state for a pool by its pubkey.
    ///
    /// In mock mode, looks up directly by pool_pubkey.
    /// In live mode, queries the subscription PDA.
    pub async fn get_latest_subscription(
        &self,
        pool_pubkey: PublicKey,
    ) -> Result<Option<SubscriptionState>> {
        self.get_subscription_state(pool_pubkey).await
    }

    /// Check if a pool has an active subscription
    pub async fn is_subscribed(&self, pool_pubkey: PublicKey) -> Result<bool> {
        match self.get_latest_subscription(pool_pubkey).await? {
            Some(sub) => Ok(sub.expires_at > Self::now()),
            None => Ok(false),
        }
    }

    /// Get verified subscription info for gossip verification.
    ///
    /// Returns the on-chain tier and active window (start_date, expires_at).
    /// Used by relays to verify that a peer's gossiped subscription claim
    /// matches what's actually on-chain.
    pub async fn get_subscription(
        &self,
        pool_pubkey: PublicKey,
    ) -> Result<Option<(SubscriptionTier, u64, u64)>> {
        match self.get_latest_subscription(pool_pubkey).await? {
            Some(sub) => Ok(Some((sub.tier, sub.start_date, sub.expires_at))),
            None => Ok(None),
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
        let expires_at = now + 30 * 24 * 3600; // 30 days default
        state.subscriptions.insert(user_pubkey, SubscriptionState {
            pool_pubkey: user_pubkey,
            tier,
            start_date: now,
            created_at: now,
            expires_at,
            pool_balance,
            original_pool_balance: pool_balance,
            total_bytes: 0,
            distribution_posted: false,
            distribution_root: [0u8; 32],
        });
        info!(
            "[MOCK] Added subscription for {} ({:?}, pool: {})",
            hex_encode(&user_pubkey[..8]),
            tier,
            pool_balance,
        );
        Ok(())
    }

    /// Add a mock subscription with custom expiry (mock mode only, for testing pool phases)
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
            pool_pubkey: user_pubkey,
            tier,
            start_date: created_at,
            created_at,
            expires_at,
            pool_balance,
            original_pool_balance: pool_balance,
            total_bytes: 0,
            distribution_posted: false,
            distribution_root: [0u8; 32],
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
            shard_id: [10u8; 32],
            sender_pubkey: [0xFFu8; 32],
            receiver_pubkey: [2u8; 32],
            pool_pubkey: [5u8; 32],
            payload_size: 1024,
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
            shard_id: [10u8; 32],
            sender_pubkey: [0xFFu8; 32],
            receiver_pubkey: [2u8; 32],
            pool_pubkey: [5u8; 32],
            payload_size: 1024,
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
            duration_secs: 30 * 24 * 3600,
            start_date: 0,
        };

        let sig = client.subscribe(sub).await.unwrap();
        assert_ne!(sig, [0u8; 64]);

        let state = client.get_subscription_state(user_pubkey).await.unwrap();
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.tier, SubscriptionTier::Standard);
        assert_eq!(state.pool_balance, 15_000_000);
        assert!(state.created_at > 0);
        assert!(!state.distribution_posted);

        assert!(client.is_subscribed(user_pubkey).await.unwrap());

        // Second subscribe overwrites
        client.subscribe(Subscribe {
            user_pubkey,
            tier: SubscriptionTier::Premium,
            payment_amount: 40_000_000,
            duration_secs: 30 * 24 * 3600,
            start_date: 0,
        }).await.unwrap();

        let state2 = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert_eq!(state2.tier, SubscriptionTier::Premium);
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
            pool_pubkey: user_pubkey,
            distribution_root: dist_root,
            total_bytes: 10,
            groth16_proof: vec![],
            sp1_public_inputs: vec![],
        }).await.unwrap();

        // Verify distribution was stored
        let sub = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert!(sub.distribution_posted);
        assert_eq!(sub.distribution_root, dist_root);
        assert_eq!(sub.total_bytes, 10);

        // Node1 claims 7/10 * 1_000_000 = 700_000 (direct payout)
        client.claim_rewards(ClaimRewards {
            pool_pubkey: user_pubkey,
            node_pubkey: node1,
            relay_bytes: 7,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
        }).await.unwrap();

        // Node2 claims 3/10 * 1_000_000 = 300_000 (direct payout)
        client.claim_rewards(ClaimRewards {
            pool_pubkey: user_pubkey,
            node_pubkey: node2,
            relay_bytes: 3,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
        }).await.unwrap();

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
            pool_pubkey: user_pubkey,
            distribution_root: [0xAA; 32],
            total_bytes: 100,
            groth16_proof: vec![],
            sp1_public_inputs: vec![],
        }).await;

        assert!(matches!(result, Err(SettlementError::PoolNotClaimable)));
    }

    #[tokio::test]
    async fn test_epoch_phase_enforcement_claim() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];

        // Active subscription — claim should fail
        client.add_mock_subscription(user_pubkey, SubscriptionTier::Standard, 1_000_000).unwrap();

        let result = client.claim_rewards(ClaimRewards {
            pool_pubkey: user_pubkey,
            node_pubkey: [2u8; 32],
            relay_bytes: 10,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
        }).await;

        assert!(matches!(result, Err(SettlementError::PoolNotClaimable)));
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
            pool_pubkey: user_pubkey,
            node_pubkey: [2u8; 32],
            relay_bytes: 10,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
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
            pool_pubkey: user_pubkey,
            distribution_root: [0xAA; 32],
            total_bytes: 10,
            groth16_proof: vec![],
            sp1_public_inputs: vec![],
        }).await.unwrap();

        // First claim succeeds
        client.claim_rewards(ClaimRewards {
            pool_pubkey: user_pubkey,
            node_pubkey: node,
            relay_bytes: 5,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
        }).await.unwrap();

        // Second claim fails
        let result = client.claim_rewards(ClaimRewards {
            pool_pubkey: user_pubkey,
            node_pubkey: node,
            relay_bytes: 5,
            leaf_index: 0,
            merkle_proof: vec![],
            light_params: None,
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
            usdc_mint: USDC_MINT_DEVNET,
            commitment: "finalized".to_string(),
            helius_api_key: None,
            light_trees: None,
        };

        assert_eq!(config.rpc_url, "http://localhost:8899");
        assert_eq!(config.program_id, [1u8; 32]);
        assert_eq!(config.commitment, "finalized");
        assert_eq!(config.mode, SettlementMode::Live);
    }

    #[tokio::test]
    async fn test_first_writer_wins_distribution() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let now = SettlementClient::now();

        client.add_mock_subscription_with_expiry(
            user_pubkey,
            SubscriptionTier::Standard,
            1_000_000,
            now - 40 * 24 * 3600,
            now - 10 * 24 * 3600,
        ).unwrap();

        // First post succeeds
        client.post_distribution(PostDistribution {
            pool_pubkey: user_pubkey,
            distribution_root: [0xAA; 32],
            total_bytes: 100,
            groth16_proof: vec![],
            sp1_public_inputs: vec![],
        }).await.unwrap();

        // Second post fails — first-writer-wins
        let result = client.post_distribution(PostDistribution {
            pool_pubkey: user_pubkey,
            distribution_root: [0xBB; 32],
            total_bytes: 200,
            groth16_proof: vec![],
            sp1_public_inputs: vec![],
        }).await;

        assert!(matches!(result, Err(SettlementError::DistributionAlreadyPosted)));

        // Original distribution is preserved
        let sub = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert!(sub.distribution_posted);
        assert_eq!(sub.distribution_root, [0xAA; 32]);
        assert_eq!(sub.total_bytes, 100);
    }

    #[tokio::test]
    async fn test_per_pool_isolation() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let pool0 = [1u8; 32];
        let pool1 = [2u8; 32];
        let now = SettlementClient::now();

        // Create two pools (different pubkeys)
        client.add_mock_subscription_with_expiry(
            pool0, SubscriptionTier::Standard, 1_000_000,
            now - 80 * 24 * 3600, now - 50 * 24 * 3600,
        ).unwrap();
        client.add_mock_subscription_with_expiry(
            pool1, SubscriptionTier::Premium, 2_000_000,
            now - 40 * 24 * 3600, now - 10 * 24 * 3600,
        ).unwrap();

        // Each pool has independent state
        let sub0 = client.get_subscription_state(pool0).await.unwrap().unwrap();
        let sub1 = client.get_subscription_state(pool1).await.unwrap().unwrap();
        assert_eq!(sub0.pool_balance, 1_000_000);
        assert_eq!(sub1.pool_balance, 2_000_000);
        assert_eq!(sub0.tier, SubscriptionTier::Standard);
        assert_eq!(sub1.tier, SubscriptionTier::Premium);

        // Claiming on pool0 doesn't affect pool1
        client.post_distribution(PostDistribution {
            pool_pubkey: pool0,
            distribution_root: [0xAA; 32], total_bytes: 10,
            groth16_proof: vec![], sp1_public_inputs: vec![],
        }).await.unwrap();
        client.claim_rewards(ClaimRewards {
            pool_pubkey: pool0, node_pubkey: [3u8; 32],
            relay_bytes: 10, leaf_index: 0, merkle_proof: vec![],
            light_params: None,
        }).await.unwrap();

        let sub0_after = client.get_subscription_state(pool0).await.unwrap().unwrap();
        let sub1_after = client.get_subscription_state(pool1).await.unwrap().unwrap();
        assert_eq!(sub0_after.pool_balance, 0);
        assert_eq!(sub1_after.pool_balance, 2_000_000); // Untouched
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00, 0xFF, 0xAB]), "00ffab");
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0x12, 0x34, 0x56, 0x78]), "12345678");
    }

    #[tokio::test]
    async fn test_initialize_config() {
        let config = SettlementConfig::mock();
        let admin_pubkey = [10u8; 32];
        let client = SettlementClient::new(config, admin_pubkey);

        // First init succeeds
        client.initialize_config().await.unwrap();

        // Second init fails
        let result = client.initialize_config().await;
        assert!(matches!(result, Err(SettlementError::TransactionFailed(_))));
    }

    #[tokio::test]
    async fn test_create_and_get_plan() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config, admin);
        client.initialize_config().await.unwrap();

        // Create a Basic Monthly plan at 5 USDC
        client.create_plan(0, 0, 5_000_000).await.unwrap();

        let plan = client.get_pricing_plan(0, 0).await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.tier, 0);
        assert_eq!(plan.billing_period, 0);
        assert_eq!(plan.price_usdc, 5_000_000);
        assert!(plan.active);
    }

    #[tokio::test]
    async fn test_create_plan_requires_admin() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config.clone(), admin);
        client.initialize_config().await.unwrap();

        // Different signer should fail
        let non_admin = [20u8; 32];
        let client2 = SettlementClient::new(config, non_admin);
        // client2 shares no mock state with client, so it has no config
        let result = client2.create_plan(0, 0, 5_000_000).await;
        assert!(matches!(result, Err(SettlementError::TransactionFailed(_))));
    }

    #[tokio::test]
    async fn test_create_plan_validation() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config, admin);
        client.initialize_config().await.unwrap();

        // Invalid tier
        assert!(client.create_plan(3, 0, 5_000_000).await.is_err());
        // Invalid period
        assert!(client.create_plan(0, 2, 5_000_000).await.is_err());
        // Zero price
        assert!(client.create_plan(0, 0, 0).await.is_err());
    }

    #[tokio::test]
    async fn test_update_plan() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config, admin);
        client.initialize_config().await.unwrap();
        client.create_plan(1, 0, 15_000_000).await.unwrap();

        // Update price
        client.update_plan(1, 0, 20_000_000).await.unwrap();

        let plan = client.get_pricing_plan(1, 0).await.unwrap().unwrap();
        assert_eq!(plan.price_usdc, 20_000_000);
    }

    #[tokio::test]
    async fn test_delete_plan() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config, admin);
        client.initialize_config().await.unwrap();
        client.create_plan(2, 1, 400_000_000).await.unwrap();

        // Delete (deactivate)
        client.delete_plan(2, 1).await.unwrap();

        let plan = client.get_pricing_plan(2, 1).await.unwrap().unwrap();
        assert!(!plan.active);
    }

    #[tokio::test]
    async fn test_get_all_plans() {
        let config = SettlementConfig::mock();
        let admin = [10u8; 32];
        let client = SettlementClient::new(config, admin);
        client.initialize_config().await.unwrap();

        client.create_plan(0, 0, 5_000_000).await.unwrap();
        client.create_plan(1, 0, 15_000_000).await.unwrap();
        client.create_plan(2, 0, 40_000_000).await.unwrap();
        client.create_plan(0, 1, 50_000_000).await.unwrap();

        let plans = client.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 4);
    }

    #[tokio::test]
    async fn test_subscribe_yearly() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [5u8; 32];
        let yearly_price: u64 = 120_000_000; // 120 USDC total

        let results = client.subscribe_yearly(
            user_pubkey,
            SubscriptionTier::Standard,
            yearly_price,
            30 * 24 * 3600, // 30 days per period
        ).await.unwrap();

        assert_eq!(results.len(), 12);

        // Each month should have its own pool
        let mut pool_pubkeys: Vec<PublicKey> = results.iter().map(|(pk, _)| *pk).collect();
        pool_pubkeys.sort();
        pool_pubkeys.dedup();
        assert_eq!(pool_pubkeys.len(), 12); // all unique

        // Check month 0 starts now-ish
        let month0 = client.get_subscription_state(results[0].0).await.unwrap().unwrap();
        assert_eq!(month0.tier, SubscriptionTier::Standard);
        assert_eq!(month0.pool_balance, 10_000_000); // 120M / 12

        // Check month 11 gets remainder
        let month11 = client.get_subscription_state(results[11].0).await.unwrap().unwrap();
        let expected_remainder = yearly_price - (yearly_price / 12) * 11;
        assert_eq!(month11.pool_balance, expected_remainder);

        // Month 6 should start 6 * month_duration in the future
        let month6 = client.get_subscription_state(results[6].0).await.unwrap().unwrap();
        let month_duration: u64 = 30 * 24 * 3600 / 12; // period_secs / 12
        let six_months_secs: u64 = 6 * month_duration;
        assert!(month6.start_date > month0.start_date);
        assert!(month6.start_date >= month0.start_date + six_months_secs - 2);
    }

    #[tokio::test]
    async fn test_get_subscription() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        client.add_mock_subscription(user_pubkey, SubscriptionTier::Premium, 40_000_000).unwrap();

        let result = client.get_subscription(user_pubkey).await.unwrap();
        assert!(result.is_some());
        let (tier, start_date, expires_at) = result.unwrap();
        assert_eq!(tier, SubscriptionTier::Premium);
        assert!(start_date > 0);
        assert!(expires_at > start_date);

        // Non-existent returns None
        assert!(client.get_subscription([99u8; 32]).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_subscribe_with_start_date() {
        let config = SettlementConfig::mock();
        let client = SettlementClient::new(config, [0u8; 32]);

        let user_pubkey = [1u8; 32];
        let future_start = (SettlementClient::now() + 30 * 24 * 3600) as i64;
        let duration = 30 * 24 * 3600;

        client.subscribe(Subscribe {
            user_pubkey,
            tier: SubscriptionTier::Standard,
            payment_amount: 15_000_000,
            duration_secs: duration,
            start_date: future_start,
        }).await.unwrap();

        let state = client.get_subscription_state(user_pubkey).await.unwrap().unwrap();
        assert_eq!(state.start_date, future_start as u64);
        assert_eq!(state.expires_at, future_start as u64 + duration);
        // created_at should be ~now, not start_date
        assert!(state.created_at < state.start_date);
    }
}
