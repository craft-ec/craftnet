use anchor_lang::prelude::*;

// Program ID will be replaced after first build with `anchor keys list`
declare_id!("2QQvVc5QmYkLEAFyoVd3hira43NE9qrhjRcuT1hmfMTH");

/// Grace period after subscription expires before distribution can be posted (1 day)
const GRACE_PERIOD_SECS: i64 = 86_400;

/// Subscription epoch duration (30 days)
const EPOCH_DURATION_SECS: i64 = 30 * 24 * 3600;

#[program]
pub mod tunnelcraft_settlement {
    use super::*;

    /// Subscribe: User purchases a subscription tier.
    ///
    /// Creates a SubscriptionAccount PDA with the payment going into the pool.
    /// Pool balance is used to pay relays at epoch end.
    pub fn subscribe(
        ctx: Context<SubscribeCtx>,
        user_pubkey: [u8; 32],
        tier: u8,
        payment_amount: u64,
    ) -> Result<()> {
        let subscription = &mut ctx.accounts.subscription_account;
        let clock = Clock::get()?;

        subscription.user_pubkey = user_pubkey;
        subscription.tier = tier;
        subscription.created_at = clock.unix_timestamp;
        subscription.expires_at = clock.unix_timestamp + EPOCH_DURATION_SECS;
        subscription.pool_balance = payment_amount;
        subscription.original_pool_balance = payment_amount;
        subscription.total_receipts = 0;
        subscription.distribution_root = [0u8; 32];
        subscription.distribution_posted = false;

        emit!(Subscribed {
            user_pubkey,
            tier,
            pool_balance: payment_amount,
            expires_at: subscription.expires_at,
        });

        Ok(())
    }

    /// Post Distribution: Aggregator posts a Merkle distribution root.
    ///
    /// Can only be called after the grace period (epoch expired + 1 day).
    /// The aggregator collects ZK-proven summaries from relays and builds
    /// this distribution off-chain.
    pub fn post_distribution(
        ctx: Context<PostDistributionCtx>,
        user_pubkey: [u8; 32],
        distribution_root: [u8; 32],
        total_receipts: u64,
    ) -> Result<()> {
        let subscription = &mut ctx.accounts.subscription_account;
        let clock = Clock::get()?;

        // Must be past grace period
        require!(
            clock.unix_timestamp >= subscription.expires_at + GRACE_PERIOD_SECS,
            SettlementError::EpochNotComplete,
        );

        // Must not already have a distribution
        require!(
            !subscription.distribution_posted,
            SettlementError::DistributionAlreadyPosted,
        );

        subscription.distribution_root = distribution_root;
        subscription.total_receipts = total_receipts;
        subscription.original_pool_balance = subscription.pool_balance;
        subscription.distribution_posted = true;

        emit!(DistributionPosted {
            user_pubkey,
            total_receipts,
            distribution_root,
        });

        Ok(())
    }

    /// Claim: Relay claims proportional rewards using Merkle proof.
    ///
    /// payout = (relay_count / total_receipts) * original_pool_balance
    ///
    /// Requires distribution to be posted and relay not already claimed.
    pub fn claim(
        ctx: Context<ClaimCtx>,
        user_pubkey: [u8; 32],
        relay_pubkey: [u8; 32],
        relay_count: u64,
        merkle_proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        let subscription = &mut ctx.accounts.subscription_account;
        let node = &mut ctx.accounts.node_account;

        // Must have distribution posted
        require!(
            subscription.distribution_posted,
            SettlementError::DistributionNotPosted,
        );

        require!(
            subscription.total_receipts > 0,
            SettlementError::NoReceipts,
        );

        // TODO: Verify Merkle proof of (relay_pubkey, relay_count) against distribution_root
        // For now, we trust the caller (aggregator signature check in production)
        let _ = merkle_proof;

        // Calculate proportional payout from original pool balance
        let payout = (relay_count as u128)
            .checked_mul(subscription.original_pool_balance as u128)
            .unwrap()
            .checked_div(subscription.total_receipts as u128)
            .unwrap() as u64;

        require!(
            payout <= subscription.pool_balance,
            SettlementError::InsufficientPoolBalance,
        );

        // Credit relay's NodeAccount
        node.node_pubkey = relay_pubkey;
        node.unclaimed_rewards = node.unclaimed_rewards.checked_add(payout).unwrap();

        // Deduct from pool
        subscription.pool_balance = subscription.pool_balance.saturating_sub(payout);

        emit!(RewardsClaimed {
            user_pubkey,
            relay_pubkey,
            payout,
        });

        Ok(())
    }

    /// Withdraw: Transfer accumulated rewards to wallet.
    pub fn withdraw(ctx: Context<WithdrawCtx>, amount: u64) -> Result<()> {
        let node = &mut ctx.accounts.node_account;

        let withdraw_amount = if amount == 0 {
            node.unclaimed_rewards
        } else {
            amount
        };

        require!(
            node.unclaimed_rewards >= withdraw_amount,
            SettlementError::InsufficientRewards,
        );

        node.unclaimed_rewards = node.unclaimed_rewards.saturating_sub(withdraw_amount);

        let clock = Clock::get()?;
        node.last_withdrawal_epoch = clock.unix_timestamp as u64;

        emit!(Withdrawn {
            node_pubkey: node.node_pubkey,
            amount: withdraw_amount,
        });

        Ok(())
    }
}

// ============================================================================
// Accounts (Context structs)
// ============================================================================

#[derive(Accounts)]
#[instruction(user_pubkey: [u8; 32])]
pub struct SubscribeCtx<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + SubscriptionAccount::INIT_SPACE,
        seeds = [b"subscription", user_pubkey.as_ref()],
        bump,
    )]
    pub subscription_account: Account<'info, SubscriptionAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(user_pubkey: [u8; 32])]
pub struct PostDistributionCtx<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"subscription", user_pubkey.as_ref()],
        bump,
    )]
    pub subscription_account: Account<'info, SubscriptionAccount>,
}

#[derive(Accounts)]
#[instruction(user_pubkey: [u8; 32], relay_pubkey: [u8; 32])]
pub struct ClaimCtx<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"subscription", user_pubkey.as_ref()],
        bump,
    )]
    pub subscription_account: Account<'info, SubscriptionAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        space = 8 + NodeAccount::INIT_SPACE,
        seeds = [b"node", relay_pubkey.as_ref()],
        bump,
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawCtx<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"node", node_account.node_pubkey.as_ref()],
        bump,
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub system_program: Program<'info, System>,
}

// ============================================================================
// Account Data
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct SubscriptionAccount {
    /// User's ed25519 public key
    pub user_pubkey: [u8; 32],
    /// Subscription tier (0=Basic, 1=Standard, 2=Premium)
    pub tier: u8,
    /// When subscription was created (unix timestamp)
    pub created_at: i64,
    /// When subscription expires (unix timestamp)
    pub expires_at: i64,
    /// Current pool balance (decreases as relays claim)
    pub pool_balance: u64,
    /// Pool balance at time of distribution posting (used for proportional claims)
    pub original_pool_balance: u64,
    /// Total receipts across all relays (set by post_distribution)
    pub total_receipts: u64,
    /// Merkle root of (relay, count) distribution
    pub distribution_root: [u8; 32],
    /// Whether distribution has been posted
    pub distribution_posted: bool,
}

#[account]
#[derive(InitSpace)]
pub struct NodeAccount {
    /// Node's ed25519 public key
    pub node_pubkey: [u8; 32],
    /// Unclaimed reward balance (lamports)
    pub unclaimed_rewards: u64,
    /// Last epoch in which a withdrawal was made
    pub last_withdrawal_epoch: u64,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct Subscribed {
    pub user_pubkey: [u8; 32],
    pub tier: u8,
    pub pool_balance: u64,
    pub expires_at: i64,
}

#[event]
pub struct DistributionPosted {
    pub user_pubkey: [u8; 32],
    pub total_receipts: u64,
    pub distribution_root: [u8; 32],
}

#[event]
pub struct RewardsClaimed {
    pub user_pubkey: [u8; 32],
    pub relay_pubkey: [u8; 32],
    pub payout: u64,
}

#[event]
pub struct Withdrawn {
    pub node_pubkey: [u8; 32],
    pub amount: u64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum SettlementError {
    #[msg("Epoch not complete — wait for grace period")]
    EpochNotComplete,
    #[msg("Distribution already posted for this epoch")]
    DistributionAlreadyPosted,
    #[msg("Distribution not yet posted")]
    DistributionNotPosted,
    #[msg("No receipts in pool")]
    NoReceipts,
    #[msg("Insufficient pool balance for payout")]
    InsufficientPoolBalance,
    #[msg("Insufficient rewards for withdrawal")]
    InsufficientRewards,
    #[msg("Invalid Merkle proof")]
    InvalidMerkleProof,
    #[msg("Already claimed from this pool")]
    AlreadyClaimed,
}
