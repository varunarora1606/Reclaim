use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

declare_id!("D6KsfpptWHAWd6YUSeCMokhk2ESGxMPbP2qjGF7F7HE");

#[program]
pub mod reclaim {

    use anchor_lang::system_program::{transfer, Transfer};
    use anchor_spl::token::{approve, burn, mint_to, Approve, Burn, MintTo};

    use super::*;

    pub fn initialize_global_state(ctx: Context<InitializeGlobalState>) -> Result<()> {
        let global_state = &mut ctx.accounts.global_state;
        global_state.token_mint = ctx.accounts.token_mint.key();
        global_state.sol_vault = ctx.accounts.sol_vault.key();
        global_state.total_shares = 0;
        global_state.bump = ctx.bumps.global_state;
        Ok(())
    }

    pub fn create_escrow(ctx: Context<CreateEscrow>, inactivity_period: i64) -> Result<()> {
        require!(inactivity_period > 0, ErrorCode::InvalidInactivityPeriod);

        let clock = Clock::get()?;
        let escrow = &mut ctx.accounts.escrow_vault;
        escrow.owner = ctx.accounts.owner.key();
        escrow.beneficiary = ctx.accounts.beneficiary.key();
        escrow.last_check_in = clock.unix_timestamp;
        escrow.inactivity_period = inactivity_period;
        escrow.status = VaultStatus::Active;
        escrow.shares = 0;
        escrow.bump = ctx.bumps.escrow_vault;
        Ok(())
    }

    pub fn deposite_sol(ctx: Context<DepositSol>, shares: u64) -> Result<()> {
        // require!(shares > 0, ErrorCode::InvalidAmount);

        let escrow = &mut ctx.accounts.escrow_vault;
        let global = &mut ctx.accounts.global_state;

        require!(
            escrow.owner == ctx.accounts.owner.key(),
            ErrorCode::InvalidEscrowOwner
        );

        require!(
            escrow.status != VaultStatus::Finished,
            ErrorCode::EscrowAlreadyFinished
        );

        // Transfer SOL to sol_vault
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.owner.to_account_info(),
                    to: ctx.accounts.sol_vault.to_account_info(),
                },
            ),
            shares,
        )?;

        let signer: &[&[&[u8]]] = &[&[b"global_state", &[global.bump]]];

        // Mint Token
        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.associated_token_account.to_account_info(),
                    authority: global.to_account_info(),
                },
                signer,
            ),
            shares,
        )?;

        escrow.shares += shares;
        global.total_shares += shares;

        // Delegate token to respective EscrowVault
        approve(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Approve {
                    to: ctx.accounts.associated_token_account.to_account_info(),
                    delegate: escrow.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            escrow.shares,
        )?;
        reset_last_check_in(&mut ctx.accounts.escrow_vault)?;

        Ok(())
    }

    pub fn check_in(ctx: Context<CheckIn>) -> Result<()> {
        require!(
            ctx.accounts.escrow_vault.owner == ctx.accounts.owner.key(),
            ErrorCode::InvalidEscrowOwner
        );

        require!(
            ctx.accounts.escrow_vault.status != VaultStatus::Finished,
            ErrorCode::EscrowAlreadyFinished
        );

        reset_last_check_in(&mut ctx.accounts.escrow_vault)?;

        Ok(())
    }

    pub fn redeem_token(ctx: Context<RedeemToken>, shares: u64) -> Result<()> {
        // Update lastcheckin if escrow account exist.
        let global = &mut ctx.accounts.global_state;

        require!(
            ctx.accounts.associated_token_account.amount >= shares,
            ErrorCode::InsufficientTokenBalance
        );

        // Burn token
        burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    authority: ctx.accounts.owner.to_account_info(),
                    mint: ctx.accounts.token_mint.to_account_info(),
                    from: ctx.accounts.associated_token_account.to_account_info(),
                },
            ),
            shares,
        )?;

        // Transfer SOL: sol_vault -> owner
        let sol_vault = &mut ctx.accounts.sol_vault.to_account_info();
        let owner = &mut ctx.accounts.owner.to_account_info();

        **sol_vault.try_borrow_mut_lamports()? -= shares;
        **owner.try_borrow_mut_lamports()? += shares;

        global.total_shares -= shares;

        Ok(())
    }

    pub fn claim_inheritance(ctx: Context<ClaimInheritaince>) -> Result<()> {
        let clock = Clock::get()?;
        let escrow_vault = &mut ctx.accounts.escrow_vault;

        require!(
            escrow_vault.beneficiary == ctx.accounts.beneficiary.key(),
            ErrorCode::InvalidBeneficiary
        );

        require!(
            escrow_vault.last_check_in + escrow_vault.inactivity_period < clock.unix_timestamp,
            ErrorCode::InactivityPeriodNotPassed
        );

        let shares = escrow_vault
            .shares
            .min(ctx.accounts.associated_token_account.amount);

        let signer: &[&[&[u8]]] = &[&[
            b"escrow_vault",
            ctx.accounts.escrow_owner.key.as_ref(),
            &[escrow_vault.bump],
        ]];

        // Burn escrow_owner token (quantity = shares)
        burn(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    from: ctx.accounts.associated_token_account.to_account_info(),
                    authority: escrow_vault.to_account_info(),
                },
                signer,
            ),
            shares,
        )?;

        // Tranfer sol: sol_vault -> beneficiary
        let sol_vault = &mut ctx.accounts.sol_vault.to_account_info();
        let beneficiary = &mut ctx.accounts.beneficiary.to_account_info();

        **sol_vault.try_borrow_mut_lamports()? -= shares;
        **beneficiary.try_borrow_mut_lamports()? += shares;

        escrow_vault.shares = 0;
        escrow_vault.status = VaultStatus::Finished;
        ctx.accounts.global_state.total_shares -= shares;
        Ok(())
    }
}

pub fn reset_last_check_in(escrow: &mut Account<EscrowVault>) -> Result<()> {
    let clock = Clock::get()?;
    escrow.last_check_in = clock.unix_timestamp;
    escrow.status = VaultStatus::Active;
    Ok(())
}

#[account]
pub struct GlobalState {
    pub sol_vault: Pubkey,
    pub token_mint: Pubkey,
    pub total_shares: u64,
    pub bump: u8,
}

#[account]
pub struct EscrowVault {
    pub owner: Pubkey,
    pub beneficiary: Pubkey,
    pub shares: u64,
    pub last_check_in: i64,
    pub inactivity_period: i64,
    pub status: VaultStatus,
    pub bump: u8,
}

#[account]
pub struct SolVault {}

#[derive(Accounts)]
pub struct InitializeGlobalState<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(init, payer = payer, space = 8 + 32 + 32 + 8 + 1, seeds = [b"global_state"], bump)]
    pub global_state: Account<'info, GlobalState>,

    #[account(init, payer = payer, space = 8, seeds = [b"sol_vault"], bump)]
    pub sol_vault: Account<'info, SolVault>,

    #[account(init, payer = payer, mint::decimals = 9,  mint::authority = global_state)]
    pub token_mint: Account<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CreateEscrow<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(init, payer = owner, space = 8 + 32 + 32 + 8 + 8 + 8 + 1 +1 , seeds = [b"escrow_vault", owner.key().as_ref()], bump)]
    pub escrow_vault: Account<'info, EscrowVault>,

    pub beneficiary: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositSol<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, seeds = [b"global_state"], bump)]
    pub global_state: Account<'info, GlobalState>,

    #[account(mut, seeds = [b"escrow_vault", owner.key().as_ref()], bump)]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut, seeds = [b"sol_vault"], bump)]
    pub sol_vault: Account<'info, SolVault>,

    #[account(mut, address = global_state.token_mint)]
    pub token_mint: Account<'info, Mint>,

    #[account(init_if_needed, payer = owner, associated_token::authority = owner, associated_token::mint = token_mint)]
    pub associated_token_account: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CheckIn<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, seeds = [b"escrow_vault", owner.key().as_ref()], bump)]
    pub escrow_vault: Account<'info, EscrowVault>,
}

#[derive(Accounts)]
pub struct RedeemToken<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, seeds = [b"global_state"], bump)]
    pub global_state: Account<'info, GlobalState>,

    #[account(mut, seeds = [b"sol_vault"], bump, address = global_state.sol_vault)]
    pub sol_vault: Account<'info, SolVault>,

    #[account(mut, address = global_state.token_mint)]
    pub token_mint: Account<'info, Mint>,

    #[account(mut, associated_token::authority = owner, associated_token::mint = token_mint)]
    pub associated_token_account: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimInheritaince<'info> {
    #[account(mut)]
    pub escrow_owner: SystemAccount<'info>,

    #[account(mut, seeds = [b"escrow_vault", escrow_owner.key().as_ref()], bump)]
    pub escrow_vault: Account<'info, EscrowVault>,

    #[account(mut, address = escrow_vault.beneficiary)]
    pub beneficiary: Signer<'info>,

    #[account(mut, seeds = [b"global_state"], bump)]
    pub global_state: Account<'info, GlobalState>,

    #[account(mut, seeds = [b"sol_vault"], bump, address = global_state.sol_vault)]
    pub sol_vault: Account<'info, SolVault>,

    #[account(mut, associated_token::authority = escrow_owner, associated_token::mint = token_mint)]
    pub associated_token_account: Account<'info, TokenAccount>,

    #[account(mut, address = global_state.token_mint)]
    pub token_mint: Account<'info, Mint>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(AnchorDeserialize, AnchorSerialize, Clone, Copy, PartialEq, Eq)]
pub enum VaultStatus {
    Active,
    Claimable,
    Finished,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid Amount")]
    InvalidAmount,

    #[msg("Inactivity period should be greater than 0")]
    InvalidInactivityPeriod,

    // ───────────── Escrow / Vault ─────────────
    #[msg("Escrow vault is not active")]
    EscrowNotActive,

    #[msg("Escrow vault is not claimable yet")]
    EscrowNotClaimable,

    #[msg("Escrow vault is already finished")]
    EscrowAlreadyFinished,

    #[msg("Inactivity period has not passed")]
    InactivityPeriodNotPassed,

    #[msg("Invalid escrow owner")]
    InvalidEscrowOwner,

    #[msg("Invalid beneficiary")]
    InvalidBeneficiary,

    // ───────────── Deposits ─────────────
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,

    #[msg("Insufficient SOL balance")]
    InsufficientSolBalance,

    // ───────────── Tokens ─────────────
    #[msg("Insufficient token balance")]
    InsufficientTokenBalance,

    // ───────────── Redemption ─────────────
    #[msg("Redemption is not allowed in current vault status")]
    RedemptionNotAllowed,
}
