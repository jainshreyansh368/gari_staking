use anchor_lang::prelude::*;

use anchor_spl::token::{Token, TokenAccount};

use crate::constant::{admin_key, gari_mint};
use crate::error::*;
use crate::state::{GariTreasuryState, PlatformData, UserMandateData};

#[derive(Accounts)]
pub struct InitPlatform<'info> {
    #[account(mut, constraint = admin.key() == admin_key::id())]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + PlatformData::LEN,
        seeds=[b"mandate_data".as_ref()], 
        bump
    )]
    pub platform_state: Account<'info, PlatformData>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdatePlatform<'info> {
    #[account(mut,
        constraint = platform_state.is_initialized,
        constraint = admin.key() == admin_key::id()
    )]
    pub admin: Signer<'info>,
    #[account(mut, seeds=[b"mandate_data".as_ref()], bump)]
    pub platform_state: Account<'info, PlatformData>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitGariTreasury<'info> {
    #[account(constraint = admin.key() == admin_key::id())]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + GariTreasuryState::LEN,
        seeds=[b"gari_treasury".as_ref(), treasury_account.to_account_info().key().as_ref()], 
        bump
    )]
    pub gari_treasury_state: Account<'info, GariTreasuryState>,
    #[account(constraint = treasury_account.mint == gari_mint::id())]
    pub treasury_account: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RemoveGariTreasury<'info> {
    #[account(constraint = admin.key() == admin_key::id())]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub payer: SystemAccount<'info>,
    #[account(
        mut,
        seeds=[b"gari_treasury".as_ref(), treasury_account.to_account_info().key().as_ref()],
        bump,
        close = payer
    )]
    pub gari_treasury_state: Account<'info, GariTreasuryState>,
    #[account(constraint = treasury_account.mint == gari_mint::id())]
    pub treasury_account: Account<'info, TokenAccount>,
}

#[derive(Accounts)]
#[instruction(
    mandate_amount: u64,
    validity: i64,
    max_transaction_amount: u64
)]
pub struct InitUserMandate<'info> {
    pub user: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds=[b"mandate_data".as_ref()], bump)]
    pub platform_state: Account<'info, PlatformData>,
    #[account(
        init,
        payer = payer,
        space = 8 + UserMandateData::LEN,
        seeds=[b"mandate_data".as_ref(), user.key().as_ref()], 
        bump
    )]
    pub user_mandate_state: Account<'info, UserMandateData>,
    #[account(
        mut,
        constraint = user_token_account.amount >= mandate_amount @ MandateError::InsufficientUserTokenATAAmount,
        constraint = mandate_amount >= platform_state.min_mandate_amount @ MandateError::InvalidMandateAmount,
        constraint = validity >= platform_state.min_validity @ MandateError::InvalidValitity,
        constraint = max_transaction_amount <= platform_state.max_tx_amount @ MandateError::InvalidMaxTxnAmount,
        constraint = user_token_account.mint == gari_mint::id(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RevokeUserMandate<'info> {
    #[account(constraint = user.key() == user_mandate_state.user)]
    pub user: Signer<'info>,
    #[account(seeds=[b"mandate_data".as_ref()],bump)]
    pub platform_state: Account<'info, PlatformData>,
    #[account(
        mut,
        seeds=[b"mandate_data".as_ref(), user.key().as_ref()], 
        bump,
        constraint = user_mandate_state.is_initialized @ ProgramError::InvalidAccountData,
        constraint = !user_mandate_state.revoked @ MandateError::UserRevokedAlready
    )]
    pub user_mandate_state: Account<'info, UserMandateData>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateUserMandate<'info> {
    #[account(constraint = user.key() == user_mandate_state.user)]
    pub user: Signer<'info>,
    #[account(seeds=[b"mandate_data".as_ref()],bump)]
    pub platform_state: Account<'info, PlatformData>,
    #[account(
        mut,
        seeds=[b"mandate_data".as_ref(), user.key().as_ref()], 
        bump,
        constraint = user_mandate_state.is_initialized @ ProgramError::InvalidAccountData,
        constraint = !user_mandate_state.revoked @ MandateError::UserRevokedAlready
    )]
    pub user_mandate_state: Account<'info, UserMandateData>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(amount:u64)]
pub struct TransferGariToTreasury<'info> {
    pub user: SystemAccount<'info>,
    #[account(seeds=[b"mandate_data".as_ref()],bump)]
    pub platform_state: Account<'info, PlatformData>,
    #[account(
        seeds=[b"gari_treasury".as_ref(), 
        treasury_account.to_account_info().key().as_ref()],
        bump,
        constraint = gari_treasury_state.is_initialized,
    )]
    pub gari_treasury_state: Account<'info, GariTreasuryState>,
    #[account(
        mut,
        seeds=[b"mandate_data".as_ref(), user.key().as_ref()],
        bump,
        constraint = user_mandate_state.is_initialized @ ProgramError::InvalidAccountData,
        constraint = !user_mandate_state.revoked @ MandateError::UserRevokedAlready,
        constraint = amount <= user_mandate_state.amount_per_transaction,
        constraint = amount <= user_mandate_state.approved_amount.checked_sub(user_mandate_state.amount_transfered).ok_or(MandateError::MathError).unwrap(),
        constraint = user_mandate_state.mandate_validity >= Clock::get().unwrap().unix_timestamp,
        constraint = user_mandate_state.next_charge_time <= Clock::get().unwrap().unix_timestamp,
    )]
    pub user_mandate_state: Account<'info, UserMandateData>,
    #[account(
        mut,
        constraint = gari_treasury_state.treasury_account == treasury_account.to_account_info().key()
    )]
    pub treasury_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_token_account.amount >= amount)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
