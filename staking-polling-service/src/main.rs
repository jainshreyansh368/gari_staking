mod config;
mod daily;
mod datadog;
mod dto;
mod esp_data;
mod producer;
mod slack;
mod sql_stmt;
mod sync_transactions;
mod transaction;

use chrono::Utc;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use sea_orm::{
    entity::Set as EntitySet, prelude::Decimal, ActiveModelTrait, ActiveValue, ColumnTrait,
    ConnectionTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    Statement,
};
use serde::{Deserialize, Serialize};
use staking_db_entity::db::{
    staking_data as staking_entity, staking_non_parsed_transaction,
    staking_user_data as staking_user_entity, staking_user_transaction_history as user_transaction,
};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::{task, time::sleep};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config: config::Config = Figment::new().merge(Toml::file("App.toml")).extract()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", &config.rust_log);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(
                format!("staking_polling_service={}", &config.polling_service_log)
                    .parse()
                    .expect("Error parsing directive"),
            ),
        )
        .with_span_events(FmtSpan::FULL)
        .init();

    let db: DatabaseConnection = config::get_db_connection(&config).await?;
    let polling_batch_transactions: usize = match config.polling_batch_transactions {
        Some(v) => v,
        None => 1_000,
    };
    let polling_batch_sleep_millis = match config.polling_batch_sleep_millis {
        Some(v) => v,
        None => 100,
    };
    let polling_sleep_secs = match config.polling_sleep_secs {
        Some(v) => v,
        None => 10,
    };

    let client = reqwest::Client::builder()
        .build()
        .expect("Reqwest client failed to initialize!");

    // wait for other instance to shutdown before starting this loop
    sleep(Duration::from_secs(polling_sleep_secs)).await;

    let datadog_client = datadog_apm::Client::new(datadog_apm::Config {
        env: Some("prod-nft".to_owned()),
        service: "prod-staking-polling-service".to_owned(),
        host: config.datadog_host.to_owned(),
        port: config.datadog_port.to_owned(),
        ..Default::default()
    });

    let datadog_client = if config.enable_datadog {
        Some(&datadog_client)
    } else {
        None
    };

    task::spawn(async move {
        let config_for_task: config::Config = Figment::new()
            .merge(Toml::file("App.toml"))
            .extract()
            .unwrap();
        let client_for_task = reqwest::Client::builder()
            .build()
            .expect("Reqwest client failed to initialize!");
        let db_for_task: DatabaseConnection =
            config::get_db_connection(&config_for_task).await.unwrap();
        let datadog_client_for_task = datadog_apm::Client::new(datadog_apm::Config {
            env: Some("prod-nft".to_owned()),
            service: "prod-staking-polling-service".to_owned(),
            host: config_for_task.datadog_host.to_owned(),
            port: config_for_task.datadog_port.to_owned(),
            ..Default::default()
        });
        let datadog_client_for_task = if config_for_task.enable_datadog {
            Some(&datadog_client_for_task)
        } else {
            None
        };
        daily::execute_tasks(
            &config_for_task,
            &db_for_task,
            &client_for_task,
            datadog_client_for_task,
        )
        .await;
    });

    loop {
        let latest_transaction = user_transaction::Entity::find()
            .order_by_desc(user_transaction::Column::BlockTime)
            .one(&db)
            .await;
        let until = match latest_transaction {
            Ok(Some(trx)) => "&until=".to_owned() + &trx.transaction_signature,
            Ok(None) => "".to_owned(),
            Err(error) => {
                error!("Error: {:?}", error);
                sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let (
            mut resp,
            mut last_processed_count,
            mut last_processed_signature,
            mut non_parsed_transactions,
        ) = get_transactions_and_account_info(
            &config,
            "".to_owned(),
            until,
            polling_batch_transactions,
            polling_batch_sleep_millis,
        )
        .await;
        insert_non_parsed_transactions(&db, &mut non_parsed_transactions).await;
        non_parsed_transactions.clear();
        let mut staking_data_accounts =
            update_accounts_transactions(&db, &config, &client, datadog_client, &resp).await;
        info!(
            "Last processed count, transaction: {:?}, {:?}",
            last_processed_count, last_processed_signature
        );

        while last_processed_count == polling_batch_transactions {
            if !last_processed_signature.is_empty() {
                last_processed_signature = "&before=".to_owned() + &last_processed_signature;
            }
            (
                resp,
                last_processed_count,
                last_processed_signature,
                non_parsed_transactions,
            ) = get_transactions_and_account_info(
                &config,
                last_processed_signature.to_owned(),
                "".to_owned(),
                polling_batch_transactions,
                polling_batch_sleep_millis,
            )
            .await;
            insert_non_parsed_transactions(&db, &mut non_parsed_transactions).await;
            non_parsed_transactions.clear();
            info!(
                "Inner Last processed count, transaction: {:?}, {:?}",
                last_processed_count, last_processed_signature
            );
            let new_staking_data_accounts =
                update_accounts_transactions(&db, &config, &client, datadog_client, &resp).await;
            for new_account in new_staking_data_accounts {
                if !staking_data_accounts.contains(&new_account) {
                    staking_data_accounts.push(new_account);
                }
            }
        }

        if !staking_data_accounts.contains(&config.staking_account_address) {
            staking_data_accounts.push(config.staking_account_address.to_owned());
        }

        let staking_data_account_objs = get_staking_data_account_info(
            &config,
            staking_data_accounts,
            polling_batch_sleep_millis,
        )
        .await;
        update_staking_accounts(&db, staking_data_account_objs).await;

        transaction::clear_old_encoded_transactions(&db).await;

        sleep(Duration::from_secs(polling_sleep_secs)).await;
    }
}

async fn get_transactions_and_account_info(
    config: &config::Config,
    before: String,
    until: String,
    limit: usize,
    polling_batch_sleep_millis: u64,
) -> (
    HashMap<String, StakingUserDataAccount>,
    usize,
    String,
    Vec<String>,
) {
    let solana_web_api_url = config.solana_web_api_node.to_owned()
        + "/get_transactions_and_account_info?limit="
        + &limit.to_string()
        + &before
        + &until;
    info!("solana url: {:?}", solana_web_api_url);

    let empty_tuple = (HashMap::new(), 0, before, vec![]);

    let resp = match reqwest::get(&solana_web_api_url).await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp
                .json::<(
                    HashMap<String, StakingUserDataAccount>,
                    usize,
                    String,
                    Vec<String>,
                )>()
                .await
            {
                Ok(tuple_value) => tuple_value,
                Err(error) => {
                    warn!("Converting to json failed!: {:?}", error);
                    empty_tuple
                }
            },
            Err(error) => {
                warn!(
                    "get_transactions_and_account_info reqwest bad status {}: {:?}",
                    error.status().unwrap().to_string(),
                    error
                );
                empty_tuple
            }
        },
        Err(error) => {
            let status = if error.status().is_some() {
                error.status().unwrap().to_string()
            } else {
                "".to_owned()
            };
            warn!("bad response {}: {:?}", status, error);
            empty_tuple
        }
    };

    sleep(Duration::from_millis(polling_batch_sleep_millis)).await;
    resp
}

async fn get_staking_data_account_info(
    config: &config::Config,
    staking_data_accounts: Vec<String>,
    polling_batch_sleep_millis: u64,
) -> Vec<StakingDataAccount> {
    info!(
        "get_staking_data_account_info started: {}",
        staking_data_accounts.len()
    );
    let mut account_objs: Vec<StakingDataAccount> = vec![];
    for staking_account in staking_data_accounts {
        let solana_web_api_url = config.solana_web_api_node.to_owned()
            + "/get_staking_data_account_info?staking_account="
            + &staking_account;
        info!("url: {:?}", solana_web_api_url);

        let parsed_resp = match reqwest::get(&solana_web_api_url).await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => resp.json::<Result<StakingDataAccount, String>>().await,
                Err(error) => {
                    warn!(
                        "get_staking_data_account_info reqwest bad status {}: {:?}",
                        error.status().unwrap().to_string(),
                        error
                    );
                    Err(error)
                }
            },
            Err(error) => {
                warn!(": {:?}", error);
                warn!("Converting to json failed!: {:?}", error);
                Err(error)
            }
        };
        match parsed_resp {
            Ok(Ok(resp)) => account_objs.push(resp),
            Ok(Err(error)) => warn!("Error from solana-web-api {}: {:?}", staking_account, error),
            Err(error) => warn!("json parsing error for {}: {:?}", staking_account, error),
        };
    }
    sleep(Duration::from_millis(polling_batch_sleep_millis)).await;
    info!("get_staking_data_account_info completed");
    account_objs
}

async fn update_accounts_transactions(
    db: &DatabaseConnection,
    config: &config::Config,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    accounts: &HashMap<String, StakingUserDataAccount>,
) -> Vec<String> {
    info!("update_accounts_transactions started: {}", accounts.len());
    let mut user_accounts: Vec<staking_user_entity::ActiveModel> = Vec::new();
    let mut staking_data_accounts: Vec<String> = vec![];
    for (user_spl_token_owner, uda) in accounts {
        let staking_data_account = uda
            .transactions
            .get(0)
            .unwrap()
            .staking_data_account
            .to_owned();
        for transaction in &uda.transactions {
            let mut amount_withdrawn = Decimal::ZERO;
            transaction::update_in_process(
                config,
                db,
                client,
                datadog_client,
                &transaction,
                &user_spl_token_owner,
                &mut amount_withdrawn,
            )
            .await;

            if !staking_data_accounts.contains(&transaction.staking_data_account) {
                staking_data_accounts.push(transaction.staking_data_account.to_owned());
            }

            let amount_withdrawn = if amount_withdrawn.is_zero() {
                sea_orm::ActiveValue::not_set()
            } else {
                sea_orm::ActiveValue::Set(Some(amount_withdrawn))
            };
            let transaction = user_transaction::ActiveModel {
                transaction_signature: ActiveValue::Set(
                    transaction.transaction_signature.to_owned(),
                ),
                block_time: ActiveValue::Set(transaction.block_time),
                error: ActiveValue::Set(transaction.error),
                instruction_type: ActiveValue::Set(transaction.instruction_type.to_owned()),
                staking_data_account: ActiveValue::Set(transaction.staking_data_account.to_owned()),
                staking_user_data_account: ActiveValue::Set(
                    uda.staking_user_data_account.to_owned(),
                ),
                user_spl_token_owner: ActiveValue::Set(user_spl_token_owner.to_owned()),
                amount: ActiveValue::Set(Decimal::from(transaction.amount)),
                amount_withdrawn: amount_withdrawn,
            };

            let transaction_signature = transaction.transaction_signature.to_owned();
            let amount_withdrawn = transaction.amount_withdrawn.to_owned();
            match user_transaction::Entity::insert(transaction).exec(db).await {
                Ok(_) => {}
                Err(db_error) => warn!(
                    "Could not insert user_transaction {:?} for user {:?} with amount {:?}: {:?}",
                    transaction_signature,
                    user_spl_token_owner,
                    amount_withdrawn,
                    db_error.to_string()
                ),
            }
        }

        let result = db
            .query_one(Statement::from_sql_and_values(
                sql_stmt::DB_BACKEND,
                sql_stmt::TOTAL_AMOUNT_WITHDRAWN,
                vec![user_spl_token_owner.into()],
            ))
            .await;
        let total_amount_withdrawn = match result {
            Ok(Some(result)) => match result.try_get::<Decimal>("", "total_amount_withdrawn") {
                Ok(amount) => {
                    if amount.is_zero() {
                        sea_orm::ActiveValue::not_set()
                    } else {
                        sea_orm::ActiveValue::Set(Some(amount))
                    }
                }
                Err(error) => {
                    warn!("Could not parse total_amount_withdrawn: {:?}", error);
                    sea_orm::ActiveValue::not_set()
                }
            },
            _ => sea_orm::ActiveValue::not_set(),
        };

        let account = staking_user_entity::ActiveModel {
            user_spl_token_owner: ActiveValue::Set(user_spl_token_owner.to_owned()),
            staking_user_data_account: ActiveValue::Set(uda.staking_user_data_account.to_owned()),
            user_token_wallet: ActiveValue::Set(Some(uda.user_token_wallet.to_owned())),
            staking_data_account: ActiveValue::Set(staking_data_account),
            is_gari_user: ActiveValue::Set(uda.is_gari_user),
            ownership_share: ActiveValue::Set(Decimal::from(uda.ownership_share)),
            staked_amount: ActiveValue::Set(Decimal::from(uda.staked_amount)),
            locked_amount: ActiveValue::Set(Decimal::from(uda.locked_amount)),
            locked_until: ActiveValue::Set(uda.locked_until),
            last_staking_timestamp: ActiveValue::Set(uda.last_staking_timestamp),
            balance: sea_orm::ActiveValue::Set(Decimal::ZERO),
            amount_withdrawn: total_amount_withdrawn,
        };
        user_accounts.push(account);
    }

    if user_accounts.len() > 0 {
        for account_model in user_accounts {
            // TODO: clone?
            let account_update = account_model.clone();
            let account = staking_user_entity::Entity::find()
                .filter(
                    staking_user_entity::Column::UserSplTokenOwner
                        .eq(account_update.user_spl_token_owner.unwrap().to_owned()),
                )
                .one(db)
                .await;
            match account {
                Ok(Some(ac)) => {
                    let mut ac = ac.into_active_model();
                    ac.ownership_share = account_update.ownership_share;
                    ac.staked_amount = account_update.staked_amount;
                    ac.locked_amount = account_update.locked_amount;
                    ac.locked_until = account_update.locked_until;
                    ac.last_staking_timestamp = account_update.last_staking_timestamp;
                    ac.amount_withdrawn = account_update.amount_withdrawn;

                    match ac.update(db).await {
                        Ok(_) => {}
                        Err(error) => {
                            info!("Error: {:?}", error);
                            warn!("Could not update staking_user_data")
                        }
                    }
                }
                Ok(None) => {
                    match staking_user_entity::Entity::insert(account_model)
                        .exec(db)
                        .await
                    {
                        Ok(_) => {}
                        Err(error) => {
                            info!("Error: {:?}", error);
                            warn!("Could not insert staking_user_data.")
                        }
                    }
                }
                Err(err) => {
                    error!("Db update error reading staking_user_data: {:?}", err);
                    continue;
                }
            };
        }
    }

    info!("update_accounts_transactions completed");
    staking_data_accounts
}

async fn update_staking_accounts(db: &DatabaseConnection, accounts: Vec<StakingDataAccount>) {
    info!("update_staking_accounts started: {}", accounts.len());
    for account in accounts {
        let result = staking_entity::Entity::find()
            .filter(
                staking_entity::Column::StakingAccountToken
                    .eq(account.staking_account_token.to_owned()),
            )
            .one(db)
            .await;
        match result {
            Ok(Some(data)) => {
                let mut data = data.into_active_model();
                data.owner = EntitySet(account.owner);
                data.staking_account_token = EntitySet(account.staking_account_token.to_string());
                data.holding_wallet = EntitySet(account.holding_wallet);
                data.holding_bump = EntitySet(account.holding_bump as i16);
                data.total_staked = EntitySet(Decimal::from(account.total_staked));
                data.total_shares = EntitySet(Decimal::from(account.total_shares));
                data.interest_rate_hourly = EntitySet(account.interest_rate_hourly as i32);
                data.est_apy = EntitySet(account.est_apy as i32);
                data.max_interest_rate_hourly = EntitySet(account.max_interest_rate_hourly as i32);
                data.last_interest_accrued_timestamp =
                    EntitySet(account.last_interest_accrued_timestamp);
                data.minimum_staking_amount =
                    EntitySet(Decimal::from(account.minimum_staking_amount));
                data.minimum_staking_period_sec =
                    EntitySet(account.minimum_staking_period_sec as i64);
                data.is_interest_accrual_paused = EntitySet(account.is_interest_accrual_paused);
                data.is_active = EntitySet(account.is_active);
                info!("Updating staking_entity: {}", account.staking_account_token);
                match data.update(db).await {
                    Ok(_) => {}
                    Err(error) => warn!(
                        "Db update error staking_data_account: {}",
                        error.to_string()
                    ),
                }
            }
            Ok(None) => {
                let active_account = staking_entity::ActiveModel {
                    staking_data_account: ActiveValue::Set(
                        account.staking_data_account.to_string(),
                    ),
                    owner: ActiveValue::Set(account.owner),
                    staking_account_token: ActiveValue::Set(
                        account.staking_account_token.to_string(),
                    ),
                    holding_wallet: ActiveValue::Set(account.holding_wallet),
                    holding_bump: ActiveValue::Set(account.holding_bump as i16),
                    total_staked: ActiveValue::Set(Decimal::from(account.total_staked)),
                    total_shares: ActiveValue::Set(Decimal::from(account.total_shares)),
                    interest_rate_hourly: ActiveValue::Set(account.interest_rate_hourly as i32),
                    est_apy: ActiveValue::Set(account.est_apy as i32),
                    max_interest_rate_hourly: ActiveValue::Set(
                        account.max_interest_rate_hourly as i32,
                    ),
                    last_interest_accrued_timestamp: ActiveValue::Set(
                        account.last_interest_accrued_timestamp,
                    ),
                    minimum_staking_amount: ActiveValue::Set(Decimal::from(
                        account.minimum_staking_amount,
                    )),
                    minimum_staking_period_sec: ActiveValue::Set(
                        account.minimum_staking_period_sec as i64,
                    ),
                    is_interest_accrual_paused: ActiveValue::Set(
                        account.is_interest_accrual_paused,
                    ),
                    is_active: ActiveValue::Set(account.is_active),
                };

                match staking_entity::Entity::insert(active_account)
                    .exec(db)
                    .await
                {
                    Ok(_) => {}
                    Err(error) => warn!(
                        "Db insert error staking_data_account: {}",
                        error.to_string()
                    ),
                }
            }
            Err(error) => warn!(
                "Failed to find by id {}: {:?}",
                account.staking_account_token.to_owned(),
                error
            ),
        }
    }
    info!("update_staking_accounts completed");
}

async fn insert_non_parsed_transactions(
    db: &DatabaseConnection,
    non_parsed_transactions: &Vec<String>,
) {
    let mut transactions: Vec<staking_non_parsed_transaction::ActiveModel> = vec![];
    for trx in non_parsed_transactions {
        let val = staking_non_parsed_transaction::ActiveModel {
            transaction_signature: ActiveValue::Set(trx.to_string()),
            attempt_timestamp: ActiveValue::Set(Utc::now().timestamp()),
        };
        transactions.push(val);
    }
    if transactions.is_empty() {
        return;
    }
    /*match staking_non_parsed_transaction::Entity::insert_many(transactions)
        .on_conflict(
            OnConflict::column(staking_non_parsed_transaction::Column::TransactionSignature)
                .do_nothing()
                .to_owned(),
        )
        .exec(db)
        .await
    {
        Ok(_) => {}
        Err(error) => {
            warn!(
                "Failed to insert non parsed transactions {:?}: {:?}",
                non_parsed_transactions, error
            );
        }
    }*/

    for transaction in transactions {
        match staking_non_parsed_transaction::Entity::insert(transaction)
            .exec(db)
            .await
        {
            Ok(_) => {}
            Err(db_error) => warn!(
                "Failed to insert non parsed transactions {:?}: {:?}",
                non_parsed_transactions, db_error
            ),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    block_time: i64,
    error: bool,
    instruction_type: String,
    staking_data_account: String,
    transaction_signature: String,
    amount: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StakingUserDataAccount {
    staking_user_data_account: String,
    user_token_wallet: String,
    transactions: Vec<Transaction>,
    is_gari_user: bool,
    ownership_share: u128,
    staked_amount: u64,
    locked_amount: u64,
    locked_until: i64,
    last_staking_timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn get_db() -> (crate::config::Config, DatabaseConnection) {
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
        let config: crate::config::Config = Figment::new()
            .merge(Toml::file("App.toml"))
            .extract()
            .unwrap();
        let db = crate::config::get_db_connection(&config).await.unwrap();
        (config, db)
    }

    #[tokio::test]
    //#[ignore = "careful: update_accounts_transactions called and db updated"]
    async fn test_update_accounts_transactions() {
        let (config, db) = get_db().await;
        let client = reqwest::Client::builder()
            .build()
            .expect("Reqwest client failed to initialize!");

        let mut accounts: HashMap<String, StakingUserDataAccount> = HashMap::new();

        let transactions: Vec<Transaction> = vec![Transaction {
            transaction_signature: "AcW1pGz3mh1thB7X74qfBcLLQ5MATkkSU3M2qC5Cdfjam9WoZyREEmWktYmSz12AXsnjNCDTXJhBceMbhf5QwXB".to_owned(),
            block_time: 1677821366,
            instruction_type: "unstake".to_owned(),
            amount: 19547300000,
            error: false,
            staking_data_account: "BnuHbRrcVGFWoxVge83EfQcHqRq7NyMC3FRjEemq8Byb".to_owned(),
        }];

        accounts.insert(
            "GpRPwQj853kZEFJapHNChq5CW7cAA5yj2a8fT1MzzEvG".to_owned(),
            StakingUserDataAccount {
                staking_user_data_account: "3arfqDCJq3wthMEqajd8iVB1SGG8kVPGcwg5MBeMmRj8"
                    .to_owned(),
                user_token_wallet: "".to_owned(),
                transactions: transactions,
                is_gari_user: true,
                ownership_share: 519099493816913857,
                staked_amount: 532932500000,
                locked_amount: 0,
                locked_until: 0,
                last_staking_timestamp: 1677810399,
            },
        );

        let staking_data_accounts: Vec<String> =
            update_accounts_transactions(&db, &config, &client, None, &accounts).await;

        assert!(staking_data_accounts
            .get(0)
            .unwrap()
            .eq(&config.staking_account_address));
    }
}
