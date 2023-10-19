use anchor_lang::{prelude::*, solana_program};
use anchor_spl::*;
use solana_program::program::{invoke, invoke_signed};

pub mod constant;
pub mod context;
pub mod error;
pub mod state;

use crate::error::MandateError;
use context::*;
use context::{
    InitGariTreasury, InitPlatform, InitUserMandate, RevokeUserMandate, TransferGariToTreasury,
    UpdateUserMandate,
};

declare_id!("Gy9zM3rt5JSFanYegsfprbPvJ8Nx69DYMbjhhijvyfUD");

#[program]
pub mod mandate_contract {

    use super::*;

    pub fn init_platform(
        ctx: Context<InitPlatform>,
        min_mandate_amount: u64,
        min_validity: i64,
        max_transaction_amount: u64,
        min_charge_period: i64,
    ) -> Result<()> {
        if ctx.accounts.platform_state.is_initialized {
            return Err(ProgramError::AccountAlreadyInitialized.into());
        }
        ctx.accounts.platform_state.is_initialized = true;
        ctx.accounts.platform_state.admin = ctx.accounts.admin.to_account_info().key();
        ctx.accounts.platform_state.min_mandate_amount = min_mandate_amount;
        ctx.accounts.platform_state.min_validity = min_validity;
        ctx.accounts.platform_state.max_tx_amount = max_transaction_amount;
        ctx.accounts.platform_state.min_charge_period = min_charge_period;

        Ok(())
    }

    pub fn update_platform(
        ctx: Context<UpdatePlatform>,
        min_mandate_amount: Option<u64>,
        min_validity: Option<i64>,
        max_transaction_amount: Option<u64>,
        min_charge_period: Option<i64>,
    ) -> Result<()> {
        let platform_state = &mut ctx.accounts.platform_state;

        if !platform_state.is_initialized {
            return Err(ProgramError::InvalidAccountData.into());
        }

        if let Some(amount) = min_mandate_amount {
            platform_state.min_mandate_amount = amount;
        }

        if let Some(validity) = min_validity {
            platform_state.min_validity = validity;
        }

        if let Some(tx_amount) = max_transaction_amount {
            platform_state.max_tx_amount = tx_amount;
        }

        if let Some(min_charge_period) = min_charge_period {
            platform_state.min_charge_period = min_charge_period;
        }

        Ok(())
    }

    pub fn init_gari_treasury(ctx: Context<InitGariTreasury>) -> Result<()> {
        if ctx.accounts.gari_treasury_state.is_initialized {
            return Err(ProgramError::AccountAlreadyInitialized.into());
        }
        ctx.accounts.gari_treasury_state.is_initialized = true;
        ctx.accounts.gari_treasury_state.treasury_account =
            ctx.accounts.treasury_account.to_account_info().key();

        Ok(())
    }

    pub fn remove_gari_treasury(_ctx: Context<RemoveGariTreasury>) -> Result<()> {
        Ok(())
    }

    pub fn init_user_mandate(
        ctx: Context<InitUserMandate>,
        mandate_amount: u64,
        validity: i64,
        max_transaction_amount: u64,
    ) -> Result<()> {
        let user_mandate_state = &ctx.accounts.user_mandate_state.to_account_info();
        let mandate_state = &mut ctx.accounts.user_mandate_state;
        let set_validity = validity
            .checked_add(Clock::get().unwrap().unix_timestamp)
            .ok_or(MandateError::MathError)?;
        mandate_state.is_initialized = true;
        mandate_state.user = ctx.accounts.user.to_account_info().key();
        mandate_state.user_token_account = ctx.accounts.user_token_account.to_account_info().key();
        mandate_state.approved_amount = mandate_amount;
        mandate_state.mandate_validity = set_validity;
        mandate_state.amount_per_transaction = max_transaction_amount;

        let user_token_account = &ctx.accounts.user_token_account.to_account_info();
        let user = &ctx.accounts.user.to_account_info();
        let token_program = &ctx.accounts.token_program.to_account_info();

        let approve_tx = token::spl_token::instruction::approve(
            token_program.key,
            user_token_account.key,
            user_mandate_state.key,
            user.key,
            &[],
            mandate_amount,
        )?;
        invoke(
            &approve_tx,
            &[
                user_token_account.clone(),
                user_mandate_state.clone(),
                user.clone(),
                token_program.clone(),
            ],
        )?;

        Ok(())
    }

    pub fn revoke_user_mandate(ctx: Context<RevokeUserMandate>) -> Result<()> {
        let user = &ctx.accounts.user.to_account_info();
        let user_token_account = &ctx.accounts.user_token_account.to_account_info();
        let token_program = &ctx.accounts.token_program.to_account_info();

        let approve_tx = token::spl_token::instruction::revoke(
            token_program.key,
            user_token_account.key,
            user.key,
            &[],
        )?;
        invoke(
            &approve_tx,
            &[
                user_token_account.clone(),
                user.clone(),
                token_program.clone(),
            ],
        )?;

        let mandate_state = &mut ctx.accounts.user_mandate_state;
        mandate_state.revoked = true;

        Ok(())
    }

    pub fn update_user_mandate(
        ctx: Context<UpdateUserMandate>,
        mandate_amount: Option<u64>,
        validity: Option<i64>,
        max_transaction_amount: Option<u64>,
    ) -> Result<()> {
        let user_mandate_state = &ctx.accounts.user_mandate_state.to_account_info();
        let platform_state = &ctx.accounts.platform_state;
        let mandate_state = &mut ctx.accounts.user_mandate_state;
        let user_token_account = &ctx.accounts.user_token_account.to_account_info();
        let user = &ctx.accounts.user.to_account_info();
        let token_program = &ctx.accounts.token_program.to_account_info();

        if let Some(val) = validity {
            if val < platform_state.min_validity {
                return Err(MandateError::InvalidValitity.into());
            }
            mandate_state.mandate_validity = val;
        }

        if let Some(max_tx_amt) = max_transaction_amount {
            if max_tx_amt > platform_state.max_tx_amount {
                return Err(MandateError::InvalidMaxTxnAmount.into());
            }
            mandate_state.amount_per_transaction = max_tx_amt;
        }

        if let Some(mandate_amt) = mandate_amount {
            mandate_state.approved_amount = mandate_state
                .approved_amount
                .checked_add(mandate_amt)
                .ok_or(MandateError::MathError)?;

            let approve_tx = token::spl_token::instruction::approve(
                token_program.key,
                user_token_account.key,
                user_mandate_state.key,
                user.key,
                &[],
                mandate_amt,
            )?;
            invoke(
                &approve_tx,
                &[
                    user_token_account.clone(),
                    user_mandate_state.clone(),
                    user.clone(),
                    token_program.clone(),
                ],
            )?;
        }

        Ok(())
    }

    pub fn transfer_to_gari_treasury(
        ctx: Context<TransferGariToTreasury>,
        amount: u64,
        bump: u8,
    ) -> Result<()> {
        let user_token_account = &ctx.accounts.user_token_account.to_account_info();
        let platform_state = &ctx.accounts.platform_state.to_account_info();
        let gari_treasury_account = &ctx.accounts.treasury_account.to_account_info();
        let user_mandate_state = &ctx.accounts.user_mandate_state.to_account_info();
        let user = &ctx.accounts.user.to_account_info();
        let mandate_state = &mut ctx.accounts.user_mandate_state;
        let platform_data = &mut ctx.accounts.platform_state;
        let token_program = &ctx.accounts.token_program.to_account_info();

        let approve_tx = token::spl_token::instruction::transfer(
            token_program.key,
            user_token_account.key,
            gari_treasury_account.key,
            user_mandate_state.key,
            &[],
            amount,
        )?;
        invoke_signed(
            &approve_tx,
            &[
                user_mandate_state.clone(),
                gari_treasury_account.clone(),
                user.clone(),
                user_token_account.clone(),
                token_program.clone(),
            ],
            &[&[b"mandate_data".as_ref(), (user.key.as_ref()), &[bump]]],
        )?;
        mandate_state.amount_transfered = mandate_state
            .amount_transfered
            .checked_add(amount)
            .ok_or(MandateError::MathError)?;

            mandate_state.last_charge_time = Clock::get().unwrap().unix_timestamp;
            if mandate_state.next_charge_time == 0 {
                mandate_state.next_charge_time = Clock::get().unwrap().unix_timestamp
                .checked_add(platform_data.min_charge_period)
                .ok_or(MandateError::MathError)?;
            }
            else {
                mandate_state.next_charge_time = mandate_state.next_charge_time
                .checked_add(platform_data.min_charge_period)
                .ok_or(MandateError::MathError)?;
            }
        Ok(())
    }
}
