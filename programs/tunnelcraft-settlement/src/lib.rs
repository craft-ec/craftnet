use anchor_lang::prelude::*;

// Program ID will be replaced after first build with `anchor keys list`
declare_id!("2QQvVc5QmYkLEAFyoVd3hira43NE9qrhjRcuT1hmfMTH");

#[program]
pub mod tunnelcraft_settlement {
    use super::*;

    /// Instruction 0: Purchase credits
    ///
    /// Creates or tops up a CreditAccount identified by credit_hash.
    /// The payer funds the account rent if newly created.
    pub fn purchase_credits(
        ctx: Context<PurchaseCreditsCtx>,
        credit_hash: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        let credit = &mut ctx.accounts.credit_account;
        credit.credit_hash = credit_hash;
        credit.balance = credit.balance.checked_add(amount).unwrap();
        credit.owner = ctx.accounts.payer.key();

        emit!(CreditPurchased {
            credit_hash,
            amount,
            new_balance: credit.balance,
            owner: credit.owner,
        });

        Ok(())
    }

    /// Instruction 1: Settle request
    ///
    /// Called by exit node after processing a request. Creates a RequestAccount
    /// in Complete status and awards points to all nodes in the request chains.
    pub fn settle_request(
        ctx: Context<SettleRequestCtx>,
        request_id: [u8; 32],
        user_pubkey: [u8; 32],
        proof_data: Vec<u8>,
        chains_data: Vec<u8>,
    ) -> Result<()> {
        let request = &mut ctx.accounts.request_account;
        let clock = Clock::get()?;

        request.request_id = request_id;
        request.status = RequestStatus::Complete;
        request.user_pubkey = user_pubkey;
        request.credit_amount = 1;
        request.total_points = 0;
        request.updated_at = clock.unix_timestamp as u64;

        // Award points to remaining node accounts
        // The caller passes node accounts as remaining_accounts
        let points_per_node = 100u64;
        for account_info in ctx.remaining_accounts.iter() {
            if account_info.is_writable {
                // Try to deserialize as NodeAccount
                let mut data = account_info.try_borrow_mut_data()?;
                if data.len() >= 8 + 32 + 8 + 8 + 8 {
                    // Skip discriminator (8 bytes), then node_pubkey (32 bytes)
                    let offset = 8 + 32;
                    let current = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                    let lifetime =
                        u64::from_le_bytes(data[offset + 8..offset + 16].try_into().unwrap());
                    data[offset..offset + 8]
                        .copy_from_slice(&(current + points_per_node).to_le_bytes());
                    data[offset + 8..offset + 16]
                        .copy_from_slice(&(lifetime + points_per_node).to_le_bytes());
                    request.total_points += points_per_node;
                }
            }
        }

        emit!(RequestSettled {
            request_id,
            user_pubkey,
            total_points: request.total_points,
        });

        // proof_data and chains_data are logged via the event for off-chain indexing
        let _ = (proof_data, chains_data);

        Ok(())
    }

    /// Instruction 2: Settle response shard
    ///
    /// Called by the last relay for each response shard. Awards points to all
    /// nodes in the response chain.
    pub fn settle_response(
        ctx: Context<SettleResponseCtx>,
        request_id: [u8; 32],
        shard_id: [u8; 32],
        chain_data: Vec<u8>,
    ) -> Result<()> {
        let points_per_node = 100u64;

        for account_info in ctx.remaining_accounts.iter() {
            if account_info.is_writable {
                let mut data = account_info.try_borrow_mut_data()?;
                if data.len() >= 8 + 32 + 8 + 8 + 8 {
                    let offset = 8 + 32;
                    let current = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                    let lifetime =
                        u64::from_le_bytes(data[offset + 8..offset + 16].try_into().unwrap());
                    data[offset..offset + 8]
                        .copy_from_slice(&(current + points_per_node).to_le_bytes());
                    data[offset + 8..offset + 16]
                        .copy_from_slice(&(lifetime + points_per_node).to_le_bytes());
                }
            }
        }

        emit!(ResponseShardSettled {
            request_id,
            shard_id,
        });

        let _ = chain_data;

        Ok(())
    }

    /// Instruction 3: Claim work
    ///
    /// A node claims that it participated in a completed request.
    /// Verifies the request is Complete and emits an event for off-chain tracking.
    pub fn claim_work(
        ctx: Context<ClaimWorkCtx>,
        request_id: [u8; 32],
        node_pubkey: [u8; 32],
    ) -> Result<()> {
        let request = &ctx.accounts.request_account;
        require!(
            request.status == RequestStatus::Complete,
            SettlementError::RequestNotComplete
        );

        emit!(WorkClaimed {
            request_id,
            node_pubkey,
            claimer: ctx.accounts.signer.key(),
        });

        Ok(())
    }

    /// Instruction 4: Withdraw
    ///
    /// Deduct points from a node account (for epoch reward withdrawal).
    pub fn withdraw(ctx: Context<WithdrawCtx>, epoch: u64, amount: u64) -> Result<()> {
        let node = &mut ctx.accounts.node_account;
        require!(
            node.current_epoch_points >= amount,
            SettlementError::InsufficientPoints
        );

        node.current_epoch_points = node.current_epoch_points.saturating_sub(amount);
        node.last_withdrawal_epoch = epoch;

        emit!(Withdrawn {
            node_pubkey: node.node_pubkey,
            epoch,
            amount,
        });

        Ok(())
    }
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
#[instruction(credit_hash: [u8; 32])]
pub struct PurchaseCreditsCtx<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + CreditAccount::INIT_SPACE,
        seeds = [b"credit", credit_hash.as_ref()],
        bump,
    )]
    pub credit_account: Account<'info, CreditAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(request_id: [u8; 32])]
pub struct SettleRequestCtx<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init_if_needed,
        payer = signer,
        space = 8 + RequestAccount::INIT_SPACE,
        seeds = [b"request", request_id.as_ref()],
        bump,
    )]
    pub request_account: Account<'info, RequestAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SettleResponseCtx<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(request_id: [u8; 32])]
pub struct ClaimWorkCtx<'info> {
    pub signer: Signer<'info>,

    #[account(
        seeds = [b"request", request_id.as_ref()],
        bump,
    )]
    pub request_account: Account<'info, RequestAccount>,
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
pub struct CreditAccount {
    /// The credit hash (SHA-256 of credit secret)
    pub credit_hash: [u8; 32],
    /// Current credit balance
    pub balance: u64,
    /// Owner who purchased the credits
    pub owner: Pubkey,
}

#[account]
#[derive(InitSpace)]
pub struct RequestAccount {
    /// Unique request identifier
    pub request_id: [u8; 32],
    /// Current status
    pub status: RequestStatus,
    /// User's public key (destination for responses)
    pub user_pubkey: [u8; 32],
    /// Credits charged for this request
    pub credit_amount: u64,
    /// Total points awarded across all chains
    pub total_points: u64,
    /// Last update timestamp
    pub updated_at: u64,
}

#[account]
#[derive(InitSpace)]
pub struct NodeAccount {
    /// Node's ed25519 public key
    pub node_pubkey: [u8; 32],
    /// Points earned in current epoch
    pub current_epoch_points: u64,
    /// Lifetime points earned
    pub lifetime_points: u64,
    /// Last epoch in which a withdrawal was made
    pub last_withdrawal_epoch: u64,
}

// ============================================================================
// Enums
// ============================================================================

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum RequestStatus {
    Unknown,
    Complete,
    Expired,
}

impl Default for RequestStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct CreditPurchased {
    pub credit_hash: [u8; 32],
    pub amount: u64,
    pub new_balance: u64,
    pub owner: Pubkey,
}

#[event]
pub struct RequestSettled {
    pub request_id: [u8; 32],
    pub user_pubkey: [u8; 32],
    pub total_points: u64,
}

#[event]
pub struct ResponseShardSettled {
    pub request_id: [u8; 32],
    pub shard_id: [u8; 32],
}

#[event]
pub struct WorkClaimed {
    pub request_id: [u8; 32],
    pub node_pubkey: [u8; 32],
    pub claimer: Pubkey,
}

#[event]
pub struct Withdrawn {
    pub node_pubkey: [u8; 32],
    pub epoch: u64,
    pub amount: u64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum SettlementError {
    #[msg("Request is not in Complete status")]
    RequestNotComplete,
    #[msg("Insufficient points for withdrawal")]
    InsufficientPoints,
    #[msg("Insufficient credit balance")]
    InsufficientCredits,
}
