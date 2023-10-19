use std::time::{SystemTime, UNIX_EPOCH};

use crate::dto::{
    InstructionType, ResponseData, StakingDataAccount, Transaction, TransactionDetails, TxDetails,
    RESPONSE_BAD_REQUEST, RESPONSE_INTERNAL_ERROR, RESPONSE_OK, TRANSACTION_FAILED,
    TRANSACTION_PROCESSING,
};
use crate::fin_cal::{get_user_amount_and_rewards, calculate_accrued_interest};
use crate::pool::Db;
use crate::sql_stmt::{DB_BACKEND, USER_HISTORY, USER_HISTORY_COUNT};

use rocket::serde::json::Json;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Statement};
use sea_orm_rocket::Connection;
use staking_db_entity::db::staking_in_process_user_transaction::{
    Column as InProcessTransactionColumn, Entity as InProcessTransaction,
};
use staking_db_entity::db::staking_user_data::{
    Column as StakingUserColumn, Entity as StakingUser,
};
use staking_db_entity::db::staking_user_transaction_history::{
    Column as HistoryColumn, Entity as History,
};

use staking_db_entity::db::staking_data::{Column as StakingDataColun, Entity as StakingData};
use tracing::{error, warn};

#[get(
    "/user_history?<user_spl_token_owner>&<instruction_type>&<page>&<limit>",
    format = "application/json"
)]
pub async fn get_user_history(
    conn: Connection<'_, Db>,
    user_spl_token_owner: String,
    instruction_type: InstructionType,
    page: i64,
    limit: i64,
) -> Json<ResponseData<Transaction>> {
    if limit > 15 {
        return Json(ResponseData::new(
            RESPONSE_BAD_REQUEST,
            "'limit' can not be more than 15".to_string(),
            None,
        ));
    }
    let db = conn.into_inner();
    let start = (page - 1) * limit;

    let instruction_type = instruction_type.to_string().to_lowercase();
    let total_records = db
        .query_one(Statement::from_sql_and_values(
            DB_BACKEND,
            USER_HISTORY_COUNT,
            vec![
                user_spl_token_owner.to_owned().into(),
                instruction_type.to_owned().into(),
            ],
        ))
        .await;

    let mut messages = String::new();
    let mut response = RESPONSE_OK;
    let mut total_pages = 0;
    match total_records {
        Ok(Some(total_records)) => {
            let total_records = total_records.try_get::<i64>("", "total_records").unwrap();
            let if_remainder = if total_records % limit > 0 { 1 } else { 0 };
            total_pages = (total_records / limit) + if_remainder;
        }
        Ok(None) => {}
        Err(error) => {
            warn!("Error fetching owner rank for history: {:?}", error);
            messages = String::from("Error fetching owner rank");
            response = RESPONSE_INTERNAL_ERROR;
        }
    };

    let in_process_transactions = InProcessTransaction::find()
        .filter(InProcessTransactionColumn::UserSplTokenOwner.eq(user_spl_token_owner.to_owned()))
        .filter(
            InProcessTransactionColumn::Status
                .is_in(vec![TRANSACTION_PROCESSING, TRANSACTION_FAILED]),
        )
        .filter(InProcessTransactionColumn::InstructionType.eq(instruction_type.to_owned()))
        .all(db)
        .await;

    let in_process_transactions = match in_process_transactions {
        Ok(in_process_transactions) => {
            if in_process_transactions.len() > 0 {
                Some(in_process_transactions)
            } else {
                None
            }
        }
        Err(error) => {
            warn!("{}", error);
            None
        }
    };

    if total_pages == 0 && in_process_transactions.is_none() {
        let message = "No user transactions found.";
        warn!("{}", message);
        return Json(ResponseData::new(RESPONSE_OK, String::from(message), None));
    }

    let mut transaction_history: Vec<TransactionDetails> = vec![];
    if in_process_transactions.is_some() {
        let in_process_transactions = in_process_transactions.unwrap();
        for trx in in_process_transactions {
            transaction_history.push(TransactionDetails::new_in_process(&trx));
        }
    }

    let transactions = History::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DB_BACKEND,
            USER_HISTORY,
            vec![
                user_spl_token_owner.to_owned().into(),
                instruction_type.into(),
                start.into(),
                limit.into(),
            ],
        ))
        .all(db)
        .await;

    let transactions = match transactions {
        Ok(trx) => {
            if trx.is_empty() {
                response = RESPONSE_OK;
            }
            trx
        }
        Err(err) => {
            error!("Error fetching transaction history: {:?}", err);
            response = RESPONSE_INTERNAL_ERROR;
            vec![]
        }
    };
    for trx in &transactions {
        transaction_history.push(TransactionDetails::new(trx));
    }

    let mut history = None;
    if transaction_history.is_empty() {
        let message = "No transactions found for user";
        warn!("{}", message);
        messages = String::from(message);
    } else {
        let user = StakingUser::find()
            .filter(StakingUserColumn::UserSplTokenOwner.eq(user_spl_token_owner))
            .one(db)
            .await;
        let last_staking_timestamp = match user {
            Ok(Some(user)) => Some(user.last_staking_timestamp),
            Ok(None) => {
                warn!("User not found");
                None
            }
            Err(error) => {
                warn!("Error: {}", error);
                None
            }
        };
        transaction_history.sort_unstable_by(|a, b| b.block_time.cmp(&a.block_time));
        let transactions =
            Transaction::new(total_pages, last_staking_timestamp, transaction_history);
        history = Some(transactions);
    }

    Json(ResponseData::new(response, messages, history))
}

#[get("/user_transaction_details?<transaction>", format = "application/json")]
pub async fn user_transaction_details(
    conn: Connection<'_, Db>,
    transaction: String,
) -> Json<ResponseData<TxDetails>> {
    let db = conn.into_inner();

    let mut message = String::new();

    let tx = History::find()
        .filter(HistoryColumn::TransactionSignature.eq(transaction))
        .one(db)
        .await;

    match tx {
        Ok(Some(tx_data)) => {
            let staking_data_account = StakingData::find()
                .filter(StakingDataColun::StakingDataAccount.eq(tx_data.staking_data_account))
                .one(db)
                .await;

            match staking_data_account {
                Ok(Some(sda_data)) => {
                    let staking_user_data = StakingUser::find()
                        .filter(
                            StakingUserColumn::UserSplTokenOwner.eq(tx_data.user_spl_token_owner),
                        )
                        .one(db)
                        .await;

                    match staking_user_data {
                        Ok(Some(sud_data)) => {
                            if tx_data.instruction_type.eq("stake") {
                                let amount = u128::from_str_radix(&tx_data.amount.to_string(), 10)
                                    .unwrap_or(0);
                                let tx_details = TxDetails::new(
                                    String::from("stake"),
                                    Some(tx_data.error),
                                    Some(amount),
                                    None,
                                    None,
                                    None,
                                );

                                return Json(ResponseData::new(
                                    RESPONSE_OK,
                                    message,
                                    Some(tx_details),
                                ));
                            } else if tx_data.instruction_type.eq("unstake") {
                                let amount = u128::from_str_radix(&tx_data.amount.to_string(), 10)
                                    .unwrap_or(0);

                                let ownership_share =
                                    u128::from_str_radix(&sud_data.ownership_share.to_string(), 10)
                                        .unwrap_or(0);

                                let balance =
                                    u128::from_str_radix(&sud_data.balance.to_string(), 10)
                                        .unwrap_or(0)
                                        .to_string();

                                let staking_data_account = StakingDataAccount::new(sda_data);

                                let interest = get_user_amount_and_rewards(
                                    ownership_share,
                                    staking_data_account,
                                    &balance,
                                );

                                let (total_interest_accured, interest_timestamp) = calculate_accrued_interest(
                                    staking_data_account.last_interest_accrued_timestamp,
                                    SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs() as i64,
                                    staking_data_account.total_staked,
                                    staking_data_account.interest_rate_hourly
                                ).unwrap();

                                let tx_details = TxDetails::new(
                                    String::from("unstake"),
                                    Some(tx_data.error),
                                    None,
                                    Some(amount),
                                    Some(interest),
                                    Some(total_interest_accured)
                                );

                                return Json(ResponseData::new(
                                    RESPONSE_OK,
                                    message,
                                    Some(tx_details),
                                ));
                            }
                        }
                        Ok(None) => {
                            warn!("No Staking User Data found for the transaction");
                            message = String::from("Not Found");
                            return Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None));
                        }
                        Err(e) => {
                            error!("Error occured: {:?}", e);
                            message = String::from("Internal Server Error");
                            return Json(ResponseData::new(RESPONSE_INTERNAL_ERROR, message, None));
                        }
                    }
                }
                Ok(None) => {
                    warn!("No Staking Data found for the transaction");
                    message = String::from("Not Found");
                    return Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None));
                }
                Err(e) => {
                    error!("Error occured: {:?}", e);
                    message = String::from("Internal Server Error");
                    return Json(ResponseData::new(RESPONSE_INTERNAL_ERROR, message, None));
                }
            }
        }
        Ok(None) => {
            warn!("No data found for the transaction");
            message = String::from("Not Found");
            return Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None));
        }
        Err(e) => {
            error!("Error occured: {:?}", e);
            message = String::from("Internal Server Error");
            return Json(ResponseData::new(RESPONSE_INTERNAL_ERROR, message, None));
        }
    }

    Json(ResponseData::new(
        RESPONSE_BAD_REQUEST,
        String::from("RESPONSE_BAD_REQUEST"),
        Some(TxDetails::new("NA".to_string(), None, None, None, None, None)),
    ))
}
