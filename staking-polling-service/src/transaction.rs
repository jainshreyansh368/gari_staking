use std::time::SystemTime;

use crate::{
    datadog,
    dto::{Data, NotificationRequest},
    sql_stmt,
};
use datadog_apm::{ErrorInfo, SqlInfo};
use sea_orm::{
    entity::{prelude::DatabaseConnection, Set as EntitySet},
    prelude::Decimal,
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QueryTrait,
};
use serde::{Deserialize, Serialize};
use staking_db_entity::db::staking_encoded_transaction::{
    Column as EncodedTransactionColumn, Entity as EncodedTransaction,
};
use staking_db_entity::db::staking_in_process_user_transaction::{
    ActiveModel as InProcessTransactionActiveModel, Column as InProcessTransactionColumn,
    Entity as InProcessTransaction, Model as InProcessTransactionModel,
};
use staking_db_entity::db::staking_user_transaction_history::{
    Column as TransactionHistoryColumn, Entity as TransactionHistory,
};
use tracing::{info, warn};

pub async fn update_in_process(
    config: &crate::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    transaction: &crate::Transaction,
    user_spl_token_owner: &str,
    amount_withdrawn: &mut Decimal,
) {
    let start_system_time = SystemTime::now();
    let in_process_transaction = InProcessTransaction::find()
        .filter(
            InProcessTransactionColumn::TransactionSignature
                .eq(transaction.transaction_signature.to_owned()),
        )
        .filter(InProcessTransactionColumn::Status.eq(TRANSACTION_PROCESSING))
        .one(db)
        .await;

    match in_process_transaction {
        Ok(Some(in_process_transactions)) => {
            let mut new_gari_transaction_id = None;
            gari_service_update(
                config,
                db,
                client,
                datadog_client,
                transaction,
                in_process_transactions.gari_transaction_id.to_owned(),
                user_spl_token_owner,
                &mut new_gari_transaction_id,
                amount_withdrawn,
            )
            .await;
            let transaction_status = if transaction.error {
                "failed"
            } else {
                "successful"
            };
            send_notification(
                config,
                client,
                user_spl_token_owner,
                transaction.instruction_type.to_owned(),
                transaction.amount.to_string(),
                &transaction.transaction_signature,
                transaction_status,
            )
            .await;
            // update that transaction to processed
            db_update(
                db,
                transaction,
                in_process_transactions,
                new_gari_transaction_id,
            )
            .await;
        }
        Ok(None) => {}
        Err(error) => {
            warn!("{:?}", error);
            datadog::send_trace(
                datadog_client,
                "database".to_string(),
                "POST",
                "/transaction/update_in_process".to_owned(),
                "/transaction",
                400,
                start_system_time,
                "db".to_owned(),
                None,
                Some(SqlInfo {
                    query: InProcessTransaction::find()
                        .filter(
                            InProcessTransactionColumn::TransactionSignature
                                .eq(transaction.transaction_signature.to_owned()),
                        )
                        .filter(InProcessTransactionColumn::Status.eq(TRANSACTION_PROCESSING))
                        .build(sql_stmt::DB_BACKEND)
                        .to_string(),
                    rows: "InProcessTransaction find failed".to_owned(),
                    db: error.to_string(),
                }),
            )
            .await;
        }
    };
}

pub async fn clear_old_encoded_transactions(db: &DatabaseConnection) {
    let _results = EncodedTransaction::delete_many()
        .filter(EncodedTransactionColumn::Timestamp.lt(chrono::Utc::now().timestamp() - 600))
        .exec(db)
        .await;
}

async fn send_notification(
    config: &super::config::Config,
    client: &reqwest::Client,
    user_spl_token_owner: &str,
    instruction_type: String,
    amount: String,
    transaction_id: &str,
    transaction_status: &str,
) {
    let amount = match Decimal::from_str_radix(&amount, 10) {
        Ok(amount) => {
            match amount
                .checked_div(Decimal::from_str_radix("1_000_000_000", 10).unwrap_or(Decimal::ZERO))
            {
                Some(amount) => amount.to_string(),
                None => {
                    warn!("Amount not found.");
                    "0".to_owned()
                }
            }
        }
        Err(error) => {
            warn!("Amount conversion error: {}", error);
            "0".to_owned()
        }
    };
    let data = Data::new(
        user_spl_token_owner.to_owned(),
        amount,
        instruction_type,
        transaction_id.to_owned(),
        transaction_status.to_owned(),
    );
    let request_data = NotificationRequest::new(data);
    match serde_json::to_string(&request_data) {
        Ok(_json) => {}
        Err(error) => {
            warn!("send_notification error: {}", error.to_string());
        }
    };

    let result = client
        .post(config.gari_notification_node.to_owned())
        .json(&request_data)
        .send()
        .await;

    match result {
        Ok(r) => match r.text().await {
            Ok(_r) => info!("Successfuly sent notification"),
            Err(error) => {
                warn!("Error sending notification: {}", error);
            }
        },
        Err(error) => {
            warn!("Error sending notification: {}", error);
        }
    }
}

async fn db_update(
    db: &DatabaseConnection,
    transaction: &super::Transaction,
    in_process_transaction: InProcessTransactionModel,
    new_gari_transaction_id: Option<String>,
) {
    if new_gari_transaction_id.is_some() {
        let new_gari_transaction_id = new_gari_transaction_id.unwrap();
        let insert_in_process_transaction = InProcessTransactionActiveModel {
            gari_transaction_id: EntitySet(new_gari_transaction_id.to_owned()),
            transaction_signature: EntitySet(
                in_process_transaction.transaction_signature.to_owned(),
            ),
            user_spl_token_owner: EntitySet(in_process_transaction.user_spl_token_owner.to_owned()),
            status: EntitySet(TRANSACTION_PROCESSED.to_owned()),
            instruction_type: EntitySet(in_process_transaction.instruction_type.to_owned()),
            amount: EntitySet(in_process_transaction.amount.to_owned()),
            processing_timestamp: EntitySet(in_process_transaction.processing_timestamp),
        };

        let old_gari_transaction_id = in_process_transaction.gari_transaction_id.to_owned();
        info!(
            "Inserting gari_transaction_id Old: {:?}, New: {}",
            old_gari_transaction_id, new_gari_transaction_id
        );
        match insert_in_process_transaction.insert(db).await {
            Ok(_) => {
                let delete_in_process_transaction = in_process_transaction.into_active_model();
                match delete_in_process_transaction.delete(db).await {
                    Ok(_) => {}
                    Err(error) => {
                        warn!(
                            "Could not delete in process transaction {}: {:?}",
                            old_gari_transaction_id, error
                        );
                    }
                }
            }
            Err(error) => {
                warn!(
                    "Could not insert in process transaction {}: {:?}",
                    new_gari_transaction_id, error
                );
            }
        }
    } else {
        // update
        let mut active_in_process_transaction = in_process_transaction.into_active_model();
        active_in_process_transaction.transaction_signature =
            EntitySet(transaction.transaction_signature.to_owned());
        active_in_process_transaction.status = EntitySet(TRANSACTION_PROCESSED.to_owned());

        match active_in_process_transaction.update(db).await {
            Ok(_) => {}
            Err(error) => {
                warn!("Could not insert in process transaction: {:?}", error);
            }
        }
    }
}

async fn gari_service_update(
    config: &crate::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    transaction: &crate::Transaction,
    gari_transaction_id: String,
    user_spl_token_owner: &str,
    new_gari_transaction_id: &mut Option<String>,
    amount_withdrawn: &mut Decimal,
) {
    let start_system_time = SystemTime::now();
    let gari_web_api = config.gari_web_api_node.to_owned() + "/updateStakingTransaction";

    *amount_withdrawn = get_withdrawable_amount(db, transaction, user_spl_token_owner).await;
    let withdrawable_amount = if amount_withdrawn.is_zero() {
        "".to_owned()
    } else {
        amount_withdrawn.to_string()
    };

    let update_transaction = UpdateStakingTransaction {
        transaction_id: &gari_transaction_id,
        signature: &transaction.transaction_signature,
        withdrawable_amount: &withdrawable_amount,
    };

    let response = client
        .post(gari_web_api)
        .header("X-STAKING-API-KEY", config.x_staking_api_key.to_owned())
        .header("User-Agent", "Staking Polling Service")
        .json(&update_transaction)
        .send()
        .await;

    match response {
        Ok(response) => match response.text().await {
            Ok(text) => match serde_json::from_str::<ResponseData<String>>(&text) {
                Ok(json) => {
                    if json.code == 400 || json.code == 404 {
                        let error = match json.error {
                            Some(error) => error,
                            None => "".to_owned(),
                        };
                        let error_stack = format!(
                            "Gari service failed for {}, Message: {}, error: {}, retrying",
                            transaction.transaction_signature, json.message, error
                        );
                        warn!("{}", error_stack);
                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update".to_owned(),
                            "/transaction",
                            json.code,
                            start_system_time,
                            "web".to_owned(),
                            Some(ErrorInfo {
                                r#type: "Gari Service Error".to_owned(),
                                msg: "UpdateStakingTransaction Error".to_owned(),
                                stack: error_stack,
                            }),
                            None,
                        )
                        .await;
                        update_gari_transaction(
                            config,
                            db,
                            client,
                            datadog_client,
                            transaction,
                            &gari_transaction_id,
                            &withdrawable_amount,
                            new_gari_transaction_id,
                        )
                        .await;
                    } else if json.code == 200 {
                        info!(
                            "Gari service update for {}: {}",
                            transaction.transaction_signature.to_owned(),
                            json.message
                        );

                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update".to_owned(),
                            "/transaction",
                            json.code,
                            start_system_time,
                            "web".to_owned(),
                            None,
                            None,
                        )
                        .await;
                    } else {
                        let error_stack = format!(
                            "Gari service failed with unknown reason for {}: {:?}",
                            transaction.transaction_signature.to_owned(),
                            json
                        );
                        warn!("{}", error_stack);
                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update".to_owned(),
                            "/transaction",
                            500,
                            start_system_time,
                            "web".to_owned(),
                            Some(ErrorInfo {
                                r#type: "Gari Service Error".to_owned(),
                                msg: "UpdateStakingTransaction Error".to_owned(),
                                stack: error_stack,
                            }),
                            None,
                        )
                        .await;
                    }
                }
                Err(error) => {
                    info!("Gari text response: {}", text);
                    let error_stack = format!(
                        r#"Error in json parsing for {}: {:?}, 
                            Response: {}"#,
                        transaction.transaction_signature.to_owned(),
                        error,
                        text
                    );
                    warn!("{}", error_stack);

                    datadog::send_trace(
                        datadog_client,
                        "request".to_string(),
                        "POST",
                        "/transaction/update_in_process/gari_service_update".to_owned(),
                        "/transaction",
                        400,
                        start_system_time,
                        "web".to_owned(),
                        Some(ErrorInfo {
                            r#type: "serde_json error".to_owned(),
                            msg: "UpdateStakingTransaction Error".to_owned(),
                            stack: error_stack,
                        }),
                        None,
                    )
                    .await;
                }
            },
            Err(error) => {
                let error_stack = format!(
                    "Error in text parsing for {}: {:?}",
                    transaction.transaction_signature.to_owned(),
                    error
                );
                warn!("{}", error_stack);
                datadog::send_trace(
                    datadog_client,
                    "request".to_string(),
                    "POST",
                    "/transaction/update_in_process/gari_service_update".to_owned(),
                    "/transaction",
                    400,
                    start_system_time,
                    "web".to_owned(),
                    Some(ErrorInfo {
                        r#type: "Reqwest parsing Error".to_owned(),
                        msg: "UpdateStakingTransaction Error".to_owned(),
                        stack: error_stack,
                    }),
                    None,
                )
                .await;
            }
        },
        Err(error) => {
            let error_stack = format!(
                "Error returned by gari service for {}: {}",
                transaction.transaction_signature.to_owned(),
                error
            );
            warn!("{}", error_stack);
            datadog::send_trace(
                datadog_client,
                "request".to_string(),
                "POST",
                "/transaction/update_in_process/gari_service_update".to_owned(),
                "/transaction",
                400,
                start_system_time,
                "web".to_owned(),
                Some(ErrorInfo {
                    r#type: "Reqwest response result error".to_owned(),
                    msg: "UpdateStakingTransaction Error".to_owned(),
                    stack: error_stack,
                }),
                None,
            )
            .await;
        }
    }
}

async fn update_gari_transaction(
    config: &super::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    transaction: &super::Transaction,
    gari_transaction_id: &str,
    withdrawable_amount: &str,
    new_gari_transaction_id: &mut Option<String>,
) {
    let in_process_transaction = InProcessTransaction::find()
        .filter(InProcessTransactionColumn::GariTransactionId.eq(gari_transaction_id))
        .one(db)
        .await;

    match in_process_transaction {
        Ok(Some(in_process_transactions)) => {
            let user_spl_token_owner = in_process_transactions.user_spl_token_owner.to_owned();
            let amount = u128::from_str_radix(&in_process_transactions.amount, 10).unwrap_or(0);
            let instruction_type = in_process_transactions.instruction_type.to_owned();
            gari_service_retry(
                config,
                client,
                datadog_client,
                transaction,
                &user_spl_token_owner,
                amount,
                &instruction_type,
                withdrawable_amount,
                new_gari_transaction_id,
            )
            .await;
            if new_gari_transaction_id.is_some() {}
        }
        Ok(None) => warn!(
            "No transactions found for gari_transaction_id: {}",
            gari_transaction_id
        ),
        Err(error) => warn!("Retry failed: {}", error),
    }
}

async fn gari_service_retry(
    config: &super::config::Config,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    transaction: &super::Transaction,
    user_spl_token_owner: &str,
    amount: u128,
    instruction_type: &str,
    withdrawable_amount: &str,
    new_gari_transaction_id: &mut Option<String>,
) {
    let start_system_time = SystemTime::now();
    let amount = amount.to_string();
    let retry_transaction = RetryStakingTransaction {
        instruction_type: instruction_type.to_owned(),
        amount: amount,
        signature: transaction.transaction_signature.to_owned(),
        user_spl_token_owner: user_spl_token_owner.to_owned(),
        withdrawable_amount: withdrawable_amount.to_owned(),
    };

    let gari_web_api = config.gari_web_api_node.to_owned() + "/retryStakingTransaction";

    let response = client
        .post(gari_web_api)
        .header("X-STAKING-API-KEY", config.x_staking_api_key.to_owned())
        .header("User-Agent", "Staking Polling Service")
        .json(&retry_transaction)
        .send()
        .await;

    match response {
        Ok(response) => match response.text().await {
            Ok(text) => match serde_json::from_str::<ResponseData<RetryResponse>>(&text) {
                Ok(json) => {
                    if json.code == 400 || json.code == 404 {
                        let error_stack = format!(
                            "Retry Gari service failed for {} with code: {}, error: {:?} : {:?}",
                            transaction.transaction_signature, json.code, json.error, json.message
                        );
                        warn!("{}", error_stack);
                        info!(
                            "Json body: {:?}",
                            serde_json::to_string::<RetryStakingTransaction>(&retry_transaction)
                        );

                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                            "/transaction",
                            json.code,
                            start_system_time,
                            "web".to_owned(),
                            Some(ErrorInfo {
                                r#type: "Gari Service Error".to_owned(),
                                msg: "retryStakingTransaction Error".to_owned(),
                                stack: error_stack,
                            }),
                            None,
                        )
                        .await;
                    } else if json.data.is_some() && json.code == 200 {
                        info!("Retry Gari service: {}", json.message);
                        new_gari_transaction_id.replace(json.data.unwrap().transaction_id);

                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                            "/transaction",
                            json.code,
                            start_system_time,
                            "web".to_owned(),
                            None,
                            None,
                        )
                        .await;
                    } else {
                        let error_stack = format!(
                            "Retry Gari service for {} failed: {:?}",
                            transaction.transaction_signature, json
                        );
                        warn!("{}", error_stack);
                        datadog::send_trace(
                            datadog_client,
                            "request".to_string(),
                            "POST",
                            "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                            "/transaction",
                            json.code,
                            start_system_time,
                            "web".to_owned(),
                            Some(ErrorInfo {
                                r#type: "Gari Service Error".to_owned(),
                                msg: "retryStakingTransaction Error".to_owned(),
                                stack: error_stack,
                            }),
                            None,
                        )
                        .await;
                    }
                }
                Err(error) => {
                    let error_stack = format!("Retry Gari text: {}, error: {:?}", text, error);
                    warn!("{}", error_stack);

                    datadog::send_trace(
                        datadog_client,
                        "request".to_string(),
                        "POST",
                        "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                        "/transaction",
                        500,
                        start_system_time,
                        "web".to_owned(),
                        Some(ErrorInfo {
                            r#type: "Gari Service Error".to_owned(),
                            msg: "retryStakingTransaction Error".to_owned(),
                            stack: error_stack,
                        }),
                        None,
                    )
                    .await;
                }
            },
            Err(error) => {
                let error_stack = format!("Retry Error in text parsing: {:?}", error);
                warn!("{}", error_stack);
                datadog::send_trace(
                    datadog_client,
                    "request".to_string(),
                    "POST",
                    "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                    "/transaction",
                    500,
                    start_system_time,
                    "web".to_owned(),
                    Some(ErrorInfo {
                        r#type: "Gari Service Error".to_owned(),
                        msg: "retryStakingTransaction Error".to_owned(),
                        stack: error_stack,
                    }),
                    None,
                )
                .await;
            }
        },
        Err(error) => {
            let error_stack = format!("Retry Error returned by gari service: {:?}", error);
            warn!("{}", error_stack);

            datadog::send_trace(
                datadog_client,
                "request".to_string(),
                "POST",
                "/transaction/update_in_process/gari_service_update/retry".to_owned(),
                "/transaction",
                500,
                start_system_time,
                "web".to_owned(),
                Some(ErrorInfo {
                    r#type: "Gari Service Error".to_owned(),
                    msg: "retryStakingTransaction Error".to_owned(),
                    stack: error_stack,
                }),
                None,
            )
            .await;
        }
    }
}

pub async fn get_withdrawable_amount(
    db: &DatabaseConnection,
    transaction: &crate::Transaction,
    user_spl_token_owner: &str,
) -> Decimal {
    if transaction.instruction_type.ne(UNSTAKE) {
        return Decimal::ZERO;
    }

    let user_transactions = TransactionHistory::find()
        .filter(TransactionHistoryColumn::UserSplTokenOwner.eq(user_spl_token_owner))
        .filter(TransactionHistoryColumn::Error.eq(false))
        .order_by_asc(TransactionHistoryColumn::BlockTime)
        .all(db)
        .await;

    let mut stake_amount: Decimal = Decimal::ZERO;
    let mut unstake_amount: Decimal = Decimal::ZERO;
    match user_transactions {
        Ok(user_transactions) => {
            for trx in user_transactions {
                if trx.block_time > transaction.block_time {
                    break;
                }
                if trx.instruction_type.eq("stake") {
                    stake_amount = stake_amount
                        .checked_add(trx.amount)
                        .unwrap_or(Decimal::ZERO);
                } else if trx.instruction_type.eq("unstake") {
                    unstake_amount = unstake_amount
                        .checked_add(trx.amount)
                        .unwrap_or(Decimal::ZERO);
                    if stake_amount.lt(&unstake_amount) {
                        //let withdrawable_amount = unstake_amount.checked_sub(stake_amount).unwrap();
                        stake_amount = Decimal::ZERO;
                        unstake_amount = Decimal::ZERO;
                    }
                }
            }
        }
        Err(error) => warn!(
            "No transactions found for user {}: {:?}",
            user_spl_token_owner, error
        ),
    }

    // wont have current transaction as its updated later in batch
    unstake_amount = unstake_amount
        .checked_add(
            Decimal::from_str_radix(&transaction.amount.to_string(), 10).unwrap_or(Decimal::ZERO),
        )
        .unwrap_or(Decimal::ZERO);

    let mut withdrawable_amount = Decimal::ZERO;
    if stake_amount.lt(&unstake_amount) {
        withdrawable_amount = unstake_amount
            .checked_sub(stake_amount)
            .unwrap_or(Decimal::ZERO);
    }

    info!(
        "stake: {} unstake: {} withdrawable: {}",
        stake_amount, unstake_amount, withdrawable_amount
    );

    withdrawable_amount
}

pub const UNSTAKE: &str = "unstake";
pub const TRANSACTION_PROCESSING: &str = "processing";
pub const TRANSACTION_PROCESSED: &str = "processed";
pub const TRANSACTION_FAILED: &str = "failed";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct UpdateStakingTransaction<'a> {
    #[serde(rename = "transactionId")]
    transaction_id: &'a str,
    signature: &'a str,
    #[serde(rename = "withdrawableAmount")]
    withdrawable_amount: &'a str,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RetryStakingTransaction {
    #[serde(rename = "transactionCase")]
    pub instruction_type: String,
    amount: String,
    pub signature: String,
    #[serde(rename = "publicKey")]
    user_spl_token_owner: String,
    #[serde(rename = "withdrawableAmount")]
    pub withdrawable_amount: String,
}

impl RetryStakingTransaction {
    pub fn new(
        instruction_type: String,
        amount: String,
        signature: String,
        user_spl_token_owner: String,
        withdrawable_amount: String,
    ) -> RetryStakingTransaction {
        RetryStakingTransaction {
            instruction_type,
            amount,
            signature,
            user_spl_token_owner,
            withdrawable_amount,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct RetryResponse {
    #[serde(rename = "transactionId")]
    transaction_id: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ResponseData<T> {
    pub code: u16,
    pub error: Option<String>,
    pub message: String,
    #[serde(default)]
    pub data: Option<T>,
}

#[cfg(test)]
mod tests {
    // Unit test to cover transaction module

    use super::*;
    use crate::config;
    use figment::{
        providers::{Format, Toml},
        Figment,
    };
    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

    async fn get_db() -> DatabaseConnection {
        let config: crate::config::Config = Figment::new()
            .merge(Toml::file("App.toml"))
            .extract()
            .unwrap();
        crate::config::get_db_connection(&config).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "for testing manual scenarios"]
    async fn test_get_withdrawable_amount() {
        let db: DatabaseConnection = get_db().await;

        let transaction = crate::Transaction {
        block_time: 1674108590,
        error: false,
        instruction_type: "unstake".to_owned(),
        staking_data_account: "BnuHbRrcVGFWoxVge83EfQcHqRq7NyMC3FRjEemq8Byb".to_owned(),
        transaction_signature: "4ZUj35PaxQd23NcubJc6KQ2txgLjbMcU7z3Tf6Ws9rAzfA8Gbqn8Z5TC4EiGebuG8tRMShh3jmit2kzB1KLgD54Q".to_owned(),
        amount: 33210000000,
    };

        let user_spl_token_owner = "2EqMFtQgkVvPS5bTEkQe4dfydGNdYWKfh9FAA9dTY5k7".to_owned();

        let config: config::Config = Figment::new()
            .merge(Toml::file("App.toml"))
            .extract()
            .unwrap();
        std::env::set_var("RUST_LOG", "warn");
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env().add_directive(
                    format!("polling_service_log={}", &config.polling_service_log)
                        .parse()
                        .expect("Error parsing directive"),
                ),
            )
            .with_span_events(FmtSpan::FULL)
            .init();

        let amount = get_withdrawable_amount(&db, &transaction, &user_spl_token_owner).await;
        assert_eq!(amount.to_string(), "237200000".to_owned());
    }
}
