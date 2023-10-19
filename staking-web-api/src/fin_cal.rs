use crate::dto::{StakingDataAccount, UserDetails};
use sea_orm::prelude::Decimal;
use std::{
    ops::{Div, Mul},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::warn;
use uint::construct_uint;

const INTEREST_MUL_FACTOR: u64 = 100_000_000;
/// Limits the exponent for one interest calc iteration to avoid overflows
const MAX_HOURS_INTEREST_ACCRUE: u64 = 15;
const SECONDS_PER_HOUR: u64 = 3600;
const MAX_INTEREST_RATE_PERIOD_HOURS: u64 = 14 * 24; // 2 weeks

construct_uint! {
    pub struct U256(4);
}

construct_uint! {
    pub struct U512(8);
}

pub fn update_user_amount_and_rewards(
    user: &mut UserDetails,
    data: StakingDataAccount,
    balance: &str,
) {
    let result = total_staked_for(user.ownership_share, data);
    match result {
        Ok(amount) => {
            let balance = match Decimal::from_str_radix(balance, 10) {
                Ok(b) => b,
                Err(error) => {
                    warn!("Error converting balance for user: {}", error);
                    Decimal::ZERO
                }
            };
            let amount_decimal =
                Decimal::from_str_radix(&amount.to_string(), 10).unwrap_or(Decimal::ZERO);

            user.rewards_earned = match amount_decimal.checked_sub(balance) {
                Some(rewards_earned) => {
                    user.staked_amount = amount;
                    u128::from_str_radix(&rewards_earned.to_string(), 10).unwrap_or(0)
                }
                None => 0u128,
            }
        }
        Err(error) => {
            warn!("Error calculating total staked for user: {}", error);
        }
    }
}

pub fn get_user_amount_and_rewards(
    ownership_share: u128,
    data: StakingDataAccount,
    balance: &str,
) -> u128 {
    let mut reward_generated: u128 = 0;
    let result = total_staked_for(ownership_share, data);
    match result {
        Ok(amount) => {
            let balance = match Decimal::from_str_radix(balance, 10) {
                Ok(b) => b,
                Err(error) => {
                    warn!("Error converting balance for user: {}", error);
                    Decimal::ZERO
                }
            };
            let amount_decimal =
                Decimal::from_str_radix(&amount.to_string(), 10).unwrap_or(Decimal::ZERO);

            reward_generated = match amount_decimal.checked_sub(balance) {
                Some(rewards_earned) => {
                    u128::from_str_radix(&rewards_earned.to_string(), 10).unwrap_or(0)
                }
                None => 0u128,
            }
        }
        Err(error) => {
            warn!("Error calculating total staked for user: {}", error);
        }
    }
    reward_generated
}

fn total_staked_for(
    ownership_share: u128,
    staking_data: StakingDataAccount,
) -> Result<u128, String> {
    if staking_data.total_shares == 0 {
        return Ok(0);
    }

    let (unminted_interest, _) = calculate_accrued_interest(
        staking_data.last_interest_accrued_timestamp,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        staking_data.total_staked,
        staking_data.interest_rate_hourly,
    )?;
    let total_staked_with_interest = staking_data
        .total_staked
        .checked_add(unminted_interest)
        .unwrap();
    let total_staked_with_interest: U256 = total_staked_with_interest.into();
    let ownership_share: U256 = ownership_share.into();
    let total_shares: U256 = staking_data.total_shares.into();

    let total_staked_for = total_staked_with_interest
        .mul(ownership_share)
        .div(total_shares)
        .as_u128();

    Ok(total_staked_for)
}

pub fn calculate_accrued_interest(
    last_interest_accrued_timestamp: i64,
    current_timestamp: i64,
    total_staked: u64,
    interest_rate: u16,
) -> Result<(u64, i64), String> {
    let mut timestamp = last_interest_accrued_timestamp;
    let mut interest = 0;
    let timestamp_diff = current_timestamp
        .checked_sub(last_interest_accrued_timestamp)
        .unwrap();
    // If there is more than one hour passed by last accrued interest
    if timestamp_diff >= SECONDS_PER_HOUR as i64 {
        let hours_elapsed = (timestamp_diff as u64)
            .checked_div(SECONDS_PER_HOUR)
            .unwrap();

        if hours_elapsed > MAX_INTEREST_RATE_PERIOD_HOURS {
            return Err("StakingError::AccruedInterestRequired.into()".to_string());
        }
        // U512 is used here in order to store big numbers when we calculate `pow`
        let mut new_balance: U512 = total_staked.into();
        let interest_mul_factor: U512 = INTEREST_MUL_FACTOR.into();
        let hourly_rate: U512 = INTEREST_MUL_FACTOR
            .checked_add(interest_rate as u64)
            .unwrap()
            .into();

        let mut hours_remain = hours_elapsed;
        let hourly_rate_pow_max = hourly_rate.pow(MAX_HOURS_INTEREST_ACCRUE.into());
        let interest_mul_factor_pow_max = interest_mul_factor.pow(MAX_HOURS_INTEREST_ACCRUE.into());
        while hours_remain > 0 {
            if hours_remain < MAX_HOURS_INTEREST_ACCRUE {
                new_balance = new_balance
                    .checked_mul(hourly_rate.pow(hours_remain.into()))
                    .unwrap()
                    .checked_div(interest_mul_factor.pow(hours_remain.into()))
                    .unwrap();
                hours_remain = 0;
            } else {
                new_balance = new_balance
                    .checked_mul(hourly_rate_pow_max)
                    .unwrap()
                    .checked_div(interest_mul_factor_pow_max)
                    .unwrap();
                hours_remain = hours_remain.checked_sub(MAX_HOURS_INTEREST_ACCRUE).unwrap();
            }
        }

        interest = new_balance.as_u64().checked_sub(total_staked).unwrap();
        timestamp = last_interest_accrued_timestamp
            .checked_add(SECONDS_PER_HOUR.checked_mul(hours_elapsed).unwrap() as i64)
            .unwrap();
    }
    Ok((interest, timestamp))
}
