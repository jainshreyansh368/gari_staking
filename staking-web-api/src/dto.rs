use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::{Deserialize, Serialize};
use sea_orm::prelude::Decimal;
use sea_orm::QueryResult;
use staking_db_entity::db::staking_data::Model as StakingModel;
use staking_db_entity::db::staking_in_process_user_transaction::Model as InProcessTransactionModel;
use staking_db_entity::db::staking_user_transaction_history::Model as HistoryModel;
use std::fmt;
use strum_macros::Display;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct TxDetails {
    pub instruction_type: String,
    pub failed: Option<bool>,
    pub stake: Option<u128>,
    pub unstake: Option<u128>,
    pub interest: Option<u128>,
    pub total_intrest_accures: Option<u64> 
}

impl TxDetails {
    pub fn new(
        instruction_type: String,
        failed: Option<bool>,
        stake: Option<u128>,
        unstake: Option<u128>,
        interest: Option<u128>,
        total_intrest_accures: Option<u64>
    ) -> Self {
        Self {
            instruction_type,
            failed,
            stake,
            unstake,
            interest,
            total_intrest_accures
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct TransactionDetails {
    pub transaction_signature: String,
    pub error: bool,
    pub instruction_type: String,
    pub staking_data_account: String,
    pub staking_user_data_account: String,
    pub amount: u128,
    pub block_time: i64,
    pub status: String,
}

impl TransactionDetails {
    pub fn new(trx: &HistoryModel) -> TransactionDetails {
        let status = if trx.error {
            String::from("failed")
        } else {
            String::from("success")
        };
        TransactionDetails {
            transaction_signature: trx.transaction_signature.to_owned(),
            error: trx.error,
            instruction_type: trx.instruction_type.to_owned(),
            staking_data_account: trx.staking_data_account.to_owned(),
            staking_user_data_account: trx.staking_user_data_account.to_owned(),
            amount: u128::from_str_radix(&trx.amount.to_string(), 10)
                .unwrap()
                .to_owned(),
            block_time: trx.block_time,
            status: status,
        }
    }

    pub fn new_in_process(trx: &InProcessTransactionModel) -> TransactionDetails {
        TransactionDetails {
            transaction_signature: trx.transaction_signature.to_owned(),
            error: false,
            instruction_type: trx.instruction_type.to_owned(),
            staking_data_account: "".to_owned(),
            staking_user_data_account: "".to_owned(),
            amount: trx.amount.parse::<u128>().unwrap().to_owned(),
            block_time: trx.processing_timestamp,
            status: trx.status.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Transaction {
    pub total_pages: i64,
    pub last_staking_timestamp: Option<i64>,
    pub transaction_details: Vec<TransactionDetails>,
}

impl Transaction {
    pub fn new(
        total_pages: i64,
        last_staking_timestamp: Option<i64>,
        transaction_details: Vec<TransactionDetails>,
    ) -> Transaction {
        Transaction {
            total_pages,
            last_staking_timestamp,
            transaction_details,
        }
    }
}

#[derive(Clone, Debug, PartialEq, FromFormField, Deserialize, Serialize, Display)]
#[serde(crate = "rocket::serde")]
#[strum(serialize_all = "snake_case")]
pub enum InstructionType {
    #[serde(rename = "stake")]
    Stake,
    #[serde(rename = "unstake")]
    Unstake,
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingDataAccount {
    holding_bump: u8,
    pub total_staked: u64,
    pub total_shares: u128,
    pub interest_rate_hourly: u16,
    est_apy: u16,
    max_interest_rate_hourly: u16,
    pub last_interest_accrued_timestamp: i64,
    minimum_staking_amount: u64,
    minimum_staking_period_sec: u32,
    is_interest_accrual_paused: bool,
    current_timestamp: i64,
}

impl StakingDataAccount {
    pub fn new(account: StakingModel) -> StakingDataAccount {
        StakingDataAccount {
            holding_bump: account.holding_bump as u8,
            total_staked: u64::from_str_radix(&account.total_staked.to_string(), 10).unwrap(),
            total_shares: u128::from_str_radix(&account.total_shares.to_string(), 10).unwrap(),
            interest_rate_hourly: account.interest_rate_hourly as u16,
            est_apy: account.est_apy as u16,
            max_interest_rate_hourly: account.max_interest_rate_hourly as u16,
            last_interest_accrued_timestamp: account.last_interest_accrued_timestamp,
            minimum_staking_amount: u64::from_str_radix(
                &account.minimum_staking_amount.to_string(),
                10,
            )
            .unwrap(),
            minimum_staking_period_sec: account.minimum_staking_period_sec as u32,
            is_interest_accrual_paused: account.is_interest_accrual_paused,
            current_timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct UserDetails {
    user_rank: i64,
    user_spl_token_owner: String,
    staking_user_data_account: String,
    staking_data_account: String,
    pub ownership_share: u128,
    pub staked_amount: u128,
    pub rewards_earned: u128,
    rewards_claimed: u128,
    locked_amount: u128,
    locked_until: Option<i64>,
    last_staking_timestamp: Option<i64>,
}

impl UserDetails {
    pub fn new(user: &QueryResult, more_details: bool) -> UserDetails {
        let mut locked_until = None;
        let mut last_staking_timestamp = None;
        if more_details {
            locked_until = Some(user.try_get::<i64>("", "locked_until").unwrap());
            last_staking_timestamp =
                Some(user.try_get::<i64>("", "last_staking_timestamp").unwrap());
        }
        UserDetails {
            user_rank: user.try_get("", "user_rank").unwrap(),
            user_spl_token_owner: user.try_get("", "user_spl_token_owner").unwrap(),
            staking_user_data_account: user.try_get("", "staking_user_data_account").unwrap(),
            staking_data_account: user.try_get("", "staking_data_account").unwrap(),
            ownership_share: u128::from_str_radix(
                &user
                    .try_get::<Decimal>("", "ownership_share")
                    .unwrap_or(Decimal::ZERO)
                    .to_string(),
                10,
            )
            .unwrap_or(0),
            staked_amount: u128::from_str_radix(
                &user
                    .try_get::<Decimal>("", "staked_amount")
                    .unwrap_or(Decimal::ZERO)
                    .to_string(),
                10,
            )
            .unwrap_or(0),
            rewards_earned: 0u128,
            rewards_claimed: u128::from_str_radix(
                &user
                    .try_get::<Decimal>("", "amount_withdrawn")
                    .unwrap_or(Decimal::ZERO)
                    .to_string(),
                10,
            )
            .unwrap_or(0),
            locked_amount: u128::from_str_radix(
                &user
                    .try_get::<Decimal>("", "locked_amount")
                    .unwrap_or(Decimal::ZERO)
                    .to_string(),
                10,
            )
            .unwrap_or(0),
            locked_until: locked_until,
            last_staking_timestamp: last_staking_timestamp,
        }
    }

    pub fn get_balance(user: &QueryResult) -> String {
        user.try_get::<Decimal>("", "balance").unwrap().to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct UserAndStakeDetails {
    user_details: Option<UserDetails>,
    staking_data_account: Option<StakingDataAccount>,
}

impl UserAndStakeDetails {
    pub fn new(
        user_details: Option<UserDetails>,
        staking_data_account: Option<StakingDataAccount>,
    ) -> UserAndStakeDetails {
        UserAndStakeDetails {
            user_details,
            staking_data_account,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct SendTransactionRequestData {
    pub uuid: String,
    pub encoded_transaction: String,
}

impl SendTransactionRequestData {
    pub fn new(uuid: String, encoded_transaction: String) -> SendTransactionRequestData {
        SendTransactionRequestData {
            uuid,
            encoded_transaction,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ResponseData<T> {
    pub code: Option<u16>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<u16>,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ResponseData<T> {
    pub fn new(code: u16, message: String, data: Option<T>) -> ResponseData<T> {
        ResponseData {
            code: Some(code),
            status_code: None,
            message,
            data,
        }
    }
}

pub const RESPONSE_OK: u16 = 200;
pub const RESPONSE_BAD_REQUEST: u16 = 400;
pub const RESPONSE_INTERNAL_ERROR: u16 = 500;

pub const TRANSACTION_PROCESSING: &str = "processing";
pub const TRANSACTION_FAILED: &str = "failed";

#[derive(Debug)]
pub struct AuthToken<'r>(&'r str);

#[derive(Debug)]
pub enum ApiKeyError {
    Missing,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthToken<'r> {
    type Error = ApiKeyError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authorization") {
            None => Outcome::Failure((Status::BadRequest, ApiKeyError::Missing)),
            Some(key) => Outcome::Success(AuthToken(key)),
        }
    }
}

impl<'r> fmt::Display for AuthToken<'r> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let (_, token) = self.0.split_at(7);
        write!(f, "{}", token)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct CreateStakingTransaction {
    #[serde(rename = "transactionCase")]
    transaction_case: String,
    amount: u128,
}

impl CreateStakingTransaction {
    pub fn new(transaction_case: String, amount: u128) -> Self {
        Self {
            transaction_case,
            amount,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct CreateStakingTransactionResponse {
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
}
