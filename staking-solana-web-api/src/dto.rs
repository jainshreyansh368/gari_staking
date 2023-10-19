use borsh::BorshDeserialize;
use rocket::serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::fmt;

const INTEREST_MUL_FACTOR: u64 = 100_000_000;
/// Limits the exponent for one interest calc iteration to avoid overflows
const MAX_HOURS_INTEREST_ACCRUE: u64 = 15;
const SECONDS_PER_HOUR: u64 = 3600;
const MAX_INTEREST_RATE_PERIOD_HOURS: u64 = 14 * 24; // 2 weeks
use uint::construct_uint;

construct_uint! {
    pub struct U256(4);
}

construct_uint! {
    pub struct U512(8);
}

#[derive(BorshDeserialize, PartialEq, Debug)]
pub struct StakingData {
    program_accounts: u64,
    /// Staking pool owner
    owner: Pubkey,
    /// Staking token
    staking_token: Pubkey,
    /// Wallet for storing staking token
    holding_wallet: Pubkey,
    /// PDA bump for holding wallet (needs for signatures)
    holding_bump: u8,
    total_staked: u64,
    total_shares: u128,
    /// Hourly interest rate in 1e-8 (1/10000 of a basis point)
    interest_rate_hourly: u16,
    max_interest_rate_hourly: u16,
    last_interest_accrued_timestamp: i64,
    minimum_staking_amount: u64,
    minimum_staking_period_sec: u32,
    is_interest_accrual_paused: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingDataAccount {
    pub staking_data_account: String,
    pub owner: String,
    pub staking_account_token: String,
    pub holding_wallet: String,
    pub holding_bump: u8,
    pub total_staked: u64,
    pub total_shares: u128,
    pub interest_rate_hourly: u16,
    pub est_apy: u16,
    pub max_interest_rate_hourly: u16,
    pub last_interest_accrued_timestamp: i64,
    pub minimum_staking_amount: u64,
    pub minimum_staking_period_sec: u32,
    pub is_interest_accrual_paused: bool,
    pub is_active: bool,
}

impl StakingDataAccount {
    pub fn new(staking_data_account: String, account: StakingData) -> StakingDataAccount {
        StakingDataAccount {
            staking_data_account: staking_data_account,
            owner: account.owner.to_string(),
            staking_account_token: account.staking_token.to_string(),
            holding_wallet: account.holding_wallet.to_string(),
            holding_bump: account.holding_bump,
            total_staked: account.total_staked,
            total_shares: account.total_shares,
            interest_rate_hourly: account.interest_rate_hourly,
            est_apy: Self::calculate_est_apy(account.interest_rate_hourly),
            max_interest_rate_hourly: account.max_interest_rate_hourly,
            last_interest_accrued_timestamp: account.last_interest_accrued_timestamp,
            minimum_staking_amount: account.minimum_staking_amount,
            minimum_staking_period_sec: account.minimum_staking_period_sec,
            is_interest_accrual_paused: account.is_interest_accrual_paused,
            is_active: true,
        }
    }

    pub fn calculate_est_apy(apr: u16) -> u16 {
        let ten_pow = 100_000_000.0;
        let nop = 8760.0;
        let apy = ((((ten_pow + apr as f64) / ten_pow).powf(nop)) * 10000.0) - 10000.0;
        let apy = apy.round() as u16;
        info!("apr: {:?} | apy: {:?}", apr, apy);
        apy
    }
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

#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub enum UserActions {
    None,
    FundFeePayer,
}

impl fmt::Display for UserActions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UserActions::FundFeePayer => write!(f, "Fund Fee Payer"),
            UserActions::None => write!(f, "None"),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct NotificationData {
    pub sol_balance: String,
    pub gari_balance: String,
    pub holding_wallet_balance: String,
    pub total_staked_balance: String,
    pub needed_interest: String,
    pub last_interest_accrued_time: String,
    pub user_action: String,
}
