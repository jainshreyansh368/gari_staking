use crate::{
    datadog, sql_stmt,
    transaction::RetryStakingTransaction,
    transaction::{ResponseData, TRANSACTION_FAILED, TRANSACTION_PROCESSING},
};
use chrono::{DateTime, Datelike, Local, TimeZone};
use sea_orm::{
    entity::Set as EntitySet, prelude::*, sea_query::Expr, ColumnTrait, ConnectionTrait,
    EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, Statement,
};
use serde::{Deserialize, Serialize};
use staking_db_entity::db::staking_in_process_user_transaction::{
    Column as InProcessTransactionColumn, Entity as InProcessTransaction,
};
use staking_db_entity::db::staking_user_transaction_history::{
    Column as TransactionHistoryColumn, Entity as TransactionHistory,
    Model as TransactionHistoryModel,
};
use std::{collections::HashMap, time::SystemTime};
use tracing::{info, warn};

pub async fn recheck_in_process_transactions(
    config: &crate::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
) {
    let days = 2;
    let multiplier = 86400;
    let end_timestamp = Local::now().timestamp() - (days * multiplier);
    let polling_batch_sleep_millis = match config.polling_batch_sleep_millis {
        Some(v) => v,
        None => 100,
    };

    let in_process_transactions = InProcessTransaction::find()
        .filter(InProcessTransactionColumn::ProcessingTimestamp.lt(end_timestamp))
        .filter(InProcessTransactionColumn::Status.eq(TRANSACTION_PROCESSING))
        .all(db)
        .await;
    match in_process_transactions {
        Ok(transactions) => {
            if transactions.is_empty() {
                return;
            }
            let mut signatures: Vec<String> = vec![];
            let mut user_tokens: Vec<String> = vec![];
            for transaction in &transactions {
                signatures.push(transaction.transaction_signature.to_owned());
                user_tokens.push(transaction.user_spl_token_owner.to_owned());
            }
            let gari_web_api =
                config.solana_web_api_node.to_owned() + "/get_transactions_info?to_retry=false";
            let response = client.post(gari_web_api).json(&signatures).send().await;

            match response {
                Ok(response) => match response
                    .json::<(HashMap<String, crate::StakingUserDataAccount>, Vec<String>)>()
                    .await
                {
                    Ok((user_transactions, _non_parsed_transactions)) => {
                        let mut remaining_signatures: Vec<String> = vec![];
                        if !user_transactions.is_empty() {
                            let staking_data_accounts = crate::update_accounts_transactions(
                                db,
                                config,
                                client,
                                datadog_client,
                                &user_transactions,
                            )
                            .await;
                            let staking_data_account_objs = crate::get_staking_data_account_info(
                                &config,
                                staking_data_accounts,
                                polling_batch_sleep_millis,
                            )
                            .await;
                            crate::update_staking_accounts(&db, staking_data_account_objs).await;
                            for signature in signatures {
                                if !user_transactions.contains_key(&signature) {
                                    remaining_signatures.push(signature.to_owned());
                                }
                            }
                            mark_transactions_failed(db, &remaining_signatures, end_timestamp)
                                .await;
                        } else {
                            mark_transactions_failed(db, &signatures, end_timestamp).await;
                        }
                    }
                    Err(error) => warn!("Could not parse list: {:?}", error),
                },
                Err(error) => warn!("Error from solana web api: {:?}", error),
            }
        }
        Err(error) => warn!("Error fetching in_process_transactions: {:?}", error),
    }
}

async fn mark_transactions_failed(
    db: &DatabaseConnection,
    _signatures: &Vec<String>,
    end_timestamp: i64,
) {
    let result = InProcessTransaction::update_many()
        .col_expr(
            InProcessTransactionColumn::Status,
            Expr::value(TRANSACTION_FAILED),
        )
        .filter(InProcessTransactionColumn::ProcessingTimestamp.lt(end_timestamp))
        .filter(InProcessTransactionColumn::Status.eq(TRANSACTION_PROCESSING))
        .exec(db)
        .await;

    match result {
        Ok(_) => {}
        Err(error) => warn!("Could not update to failed: {:?}", error.to_string()),
    }

    // send notifications to signatures
}

pub async fn update_transactions_in_gari_service(
    config: &crate::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
    start_block_time: i64,
    end_block_time: i64,
) {
    let start_system_time = SystemTime::now();
    let (user_transactions, transaction_idx) =
        match get_user_transactions(db, start_block_time, end_block_time).await {
            Some((user_transactions, transaction_idx)) => (user_transactions, transaction_idx),
            None => return,
        };
    match get_transactions(db, start_block_time, end_block_time).await {
        Some(transactions) => {
            let mut transaction_batch = TransactionBatch {
                transactions: vec![],
            };
            let mut transaction_withdrawables: Vec<Option<Decimal>> = vec![];

            for trx in transactions {
                let idx = user_transactions.get(&trx.user_spl_token_owner);
                if idx.is_none() {
                    info!("No withdrawable_amount for {}", trx.user_spl_token_owner);
                    continue;
                }
                let idx = idx.unwrap();
                let mut withdrawable_amount: Option<Decimal> = None;

                let mut stake_amount: Decimal = Decimal::ZERO;
                let mut unstake_amount: Decimal = Decimal::ZERO;
                for user_trx in &transaction_idx[*idx] {
                    if user_trx.block_time > trx.block_time {
                        break;
                    }
                    if user_trx.instruction_type.eq("stake") {
                        stake_amount = stake_amount.checked_add(user_trx.amount).unwrap();
                        withdrawable_amount = None;
                    } else if user_trx.instruction_type.eq("unstake") {
                        unstake_amount = unstake_amount.checked_add(user_trx.amount).unwrap();
                        if stake_amount.lt(&unstake_amount) {
                            withdrawable_amount =
                                Some(unstake_amount.checked_sub(stake_amount).unwrap());
                            stake_amount = Decimal::ZERO;
                            unstake_amount = Decimal::ZERO;
                        }
                    }
                }

                let withdrawable_amount = if withdrawable_amount.is_none() {
                    "".to_owned()
                } else {
                    withdrawable_amount.unwrap().to_string()
                };

                let retry_staking_transaction = RetryStakingTransaction::new(
                    trx.instruction_type,
                    trx.amount.to_string(),
                    trx.transaction_signature,
                    trx.user_spl_token_owner,
                    withdrawable_amount,
                );
                transaction_withdrawables.push(trx.amount_withdrawn);
                transaction_batch.push_transaction(retry_staking_transaction);
                if transaction_batch.len_of_transactions() % 100 == 0 {
                    update_transactions(db, &transaction_batch, &transaction_withdrawables).await;
                    send_gari_service_request(config, client, &transaction_batch).await;
                    let message = format!(
                        "Processed 100 transactions from {} to {}",
                        start_block_time, end_block_time
                    );
                    info!("{}", message);
                    datadog::send_trace(
                        datadog_client,
                        "request".to_owned(),
                        "POST",
                        "/daily/sync_transactions/update_transactions_in_gari_service".to_owned(),
                        "/daily",
                        200,
                        start_system_time,
                        "web".to_owned(),
                        None,
                        None,
                    )
                    .await;
                    transaction_batch.clear_transactions();
                    transaction_withdrawables.clear();
                }
            }
            if transaction_batch.len_of_transactions() > 0 {
                info!(
                    "Processed {} transactions from {} to {}",
                    transaction_batch.len_of_transactions(),
                    start_block_time,
                    end_block_time
                );
                update_transactions(db, &transaction_batch, &transaction_withdrawables).await;
                send_gari_service_request(config, client, &transaction_batch).await;
            }
        }
        None => info!("No transactions to update to gari service"),
    }
}

async fn get_user_transactions(
    db: &DatabaseConnection,
    start_block_time: i64,
    end_block_time: i64,
) -> Option<(HashMap<String, usize>, Vec<Vec<TransactionWithdrawable>>)> {
    let result = db
        .query_all(Statement::from_sql_and_values(
            sql_stmt::DB_BACKEND,
            sql_stmt::USER_TRANSACTIONS,
            vec![start_block_time.into(), end_block_time.into()],
        ))
        .await;
    let mut user_transactions: HashMap<String, usize> = HashMap::new();
    let mut transaction_idx: Vec<Vec<TransactionWithdrawable>> = Vec::new();
    match result {
        Ok(transaction) => {
            for trx in transaction {
                match trx.try_get::<String>("", "user_spl_token_owner") {
                    Ok(spl_token) => {
                        let transaction_withdrawable = TransactionWithdrawable {
                            block_time: trx.try_get::<i64>("", "block_time").unwrap_or(0),
                            instruction_type: trx
                                .try_get::<String>("", "instruction_type")
                                .unwrap_or("".to_owned()),
                            amount: trx
                                .try_get::<Decimal>("", "amount")
                                .unwrap_or(Decimal::ZERO),
                        };
                        match user_transactions.get(&spl_token) {
                            Some(idx) => {
                                transaction_idx[*idx].push(transaction_withdrawable);
                            }
                            None => {
                                transaction_idx.push(vec![transaction_withdrawable]);
                                user_transactions.insert(spl_token, transaction_idx.len() - 1);
                            }
                        }
                    }
                    Err(error) => {
                        warn!("Error in fetching record: {:?}", error);
                    }
                }
            }
            Some((user_transactions, transaction_idx))
        }
        Err(error) => {
            warn!("Failed to fetch user transactions: {:?}", error);
            None
        }
    }
}

async fn get_transactions(
    db: &DatabaseConnection,
    start_block_time: i64,
    end_block_time: i64,
) -> Option<Vec<TransactionHistoryModel>> {
    match TransactionHistory::find()
        .filter(TransactionHistoryColumn::BlockTime.gte(start_block_time))
        .filter(TransactionHistoryColumn::BlockTime.lt(end_block_time))
        .order_by_asc(TransactionHistoryColumn::BlockTime)
        .all(db)
        .await
    {
        Ok(trx) => Some(trx),
        Err(error) => {
            warn!("Error fetching transactions: {:?}", error);
            None
        }
    }
}

async fn update_transactions(
    db: &DatabaseConnection,
    transaction_batch: &TransactionBatch,
    pre_withdrawables: &Vec<Option<Decimal>>,
) {
    let mut batch_count = 0;
    let mut signatures: Vec<String> = vec![];
    let mut signature_withdrawable_map: HashMap<String, String> = HashMap::new();
    for transaction in &transaction_batch.transactions {
        if !transaction.instruction_type.eq("unstake")
            || (pre_withdrawables[batch_count].is_none()
                && transaction.withdrawable_amount.is_empty())
            || (pre_withdrawables[batch_count].is_some()
                && transaction.withdrawable_amount
                    == pre_withdrawables[batch_count].unwrap().to_string())
        { // skip transaction
        } else {
            signatures.push(transaction.signature.to_owned());
            signature_withdrawable_map.insert(
                transaction.signature.to_owned(),
                transaction.withdrawable_amount.to_owned(),
            );
        }
        batch_count += 1;
    }
    let transactions = TransactionHistory::find()
        .filter(TransactionHistoryColumn::TransactionSignature.is_in(signatures))
        .all(db)
        .await;
    match transactions {
        Ok(transactions) => {
            info!("Updating {} transactions", transactions.len());
            for transaction in transactions {
                let amount_withdrawn = signature_withdrawable_map
                    .get(&transaction.transaction_signature)
                    .unwrap();

                let transaction_signature = transaction.transaction_signature.to_owned();

                let amount_withdrawn = if amount_withdrawn.is_empty() {
                    EntitySet(None)
                } else {
                    EntitySet(Some(Decimal::from_str_radix(amount_withdrawn, 10).unwrap()))
                };
                let mut active_transaction = transaction.into_active_model();
                active_transaction.amount_withdrawn = amount_withdrawn;
                match active_transaction.update(db).await {
                    Ok(_) => {}
                    Err(error) => warn!(
                        "Could not update transaction {}: {:?}",
                        transaction_signature, error
                    ),
                }
            }
        }
        Err(error) => warn!(
            "Error fetching transaction {:?}: {:?}",
            transaction_batch, error
        ),
    }
}

async fn send_gari_service_request(
    config: &crate::config::Config,
    client: &reqwest::Client,
    transaction_batch: &TransactionBatch,
) {
    let gari_web_api = config.gari_web_api_node.to_owned() + "/insertDailyPollingData";
    let response = client
        .post(gari_web_api)
        .header("X-STAKING-API-KEY", config.x_staking_api_key.to_owned())
        .header("User-Agent", "Staking Polling Service")
        .json(transaction_batch)
        .send()
        .await;
    match response {
        Ok(response) => {
            match response.text().await {
                Ok(text) => match serde_json::from_str::<ResponseData<String>>(&text) {
                    Ok(json) => {
                        if json.code == 400 || json.code == 404 {
                            let error = match json.error {
                                Some(error) => error,
                                None => "".to_owned(),
                            };
                            warn!(
                                "Gari service insertDailyPollingData failed: {}, error: {}",
                                json.message, error
                            );
                        } else if json.code == 200 {
                            info!("Gari service insertDailyPollingData: {}", json.message);
                        } else {
                            warn!("Gari service failed with unknown reason insertDailyPollingData: {:?}", json);
                        }
                    }
                    Err(error) => {
                        info!("Gari text response insertDailyPollingData: {}", text);
                        warn!("Error in json parsing insertDailyPollingData: {}", error);
                    }
                },
                Err(error) => warn!("Error in text parsing insertDailyPollingData: {}", error),
            }
        }
        Err(error) => warn!(
            "Error returned by gari service insertDailyPollingData: {}",
            error
        ),
    }
}

pub async fn get_start_end_block_time(
    start_day: DateTime<Local>,
    hour: u32,
    min: u32,
    sec: u32,
) -> (i64, i64) {
    let start_block_time = start_day;
    let end_block_time = Local
        .with_ymd_and_hms(
            start_block_time.year(),
            start_block_time.month(),
            start_block_time.day(),
            hour,
            min,
            sec,
        )
        .unwrap();

    let start_block_time = end_block_time
        .checked_sub_signed(chrono::Duration::days(1))
        .unwrap()
        .checked_sub_signed(chrono::Duration::hours(1))
        .unwrap();

    let end_block_time = end_block_time
        .checked_sub_signed(chrono::Duration::hours(1))
        .unwrap();
    (start_block_time.timestamp(), end_block_time.timestamp())
}

#[derive(Debug, Clone)]
pub struct TransactionWithdrawable {
    block_time: i64,
    instruction_type: String,
    amount: Decimal,
}

#[derive(Debug, Deserialize, Serialize)]
struct TransactionBatch {
    #[serde(rename = "stakingData")]
    transactions: Vec<crate::transaction::RetryStakingTransaction>,
}

impl<'a> TransactionBatch {
    fn push_transaction(&mut self, value: crate::transaction::RetryStakingTransaction) {
        self.transactions.push(value);
    }

    fn clear_transactions(&mut self) {
        self.transactions.clear();
    }

    fn len_of_transactions(&self) -> usize {
        self.transactions.len()
    }
}

#[cfg(test)]
mod tests {

    use crate::config;

    use super::*;
    use figment::{
        providers::{Format, Toml},
        Figment,
    };
    use staking_db_entity::db::staking_user_data::{
        Column as UserColumn, Entity as User, Model as UserModel,
    };
    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

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
    async fn test_json_load_transaction_batch() {
        let data = r#"
        {
          "stakingData": [
            {
              "transactionCase": "stake",
              "amount": "10000000000",
              "signature": "ckx2Ana4XkjNEDtMgDCrPr3oekw6EeGviLLScPEjwKCfzD6YGPpNumRGov5APb85VphfsMnfN5bRqqjMLegG879",
              "publicKey": "CzRtpwB9txwM3FEhdEpo7esaJzPQFeCX41a4sSswRQ3p",
              "withdrawableAmount": ""
            },
         {
              "transactionCase": "unstake",
              "amount": "10000000000",
              "signature": "ckx2Ana4XkjNEDtMgDCrPr3oekw6EeGviLLScPEjwKCfzD6YGPpNumRGov5APb85VphfsMnfN5bRqqjMLegG878",
              "publicKey": "CzRtpwB9txwM3FEhdEpo7esaJzPQFeCX41a4sSswRQ3p",
              "withdrawableAmount": "10000"
            }
          ]
        }"#;
        let v: TransactionBatch = serde_json::from_str(data).unwrap();
        assert_eq!(v.transactions.len(), 2);
    }

    #[tokio::test]
    #[ignore = "careful: update all transactions withdrawable amount, disable gari service calls"]
    async fn update_all_transactions_withdrawable_amount() {
        let (config, db) = get_db().await;
        let client = reqwest::Client::builder()
            .build()
            .expect("Reqwest client failed to initialize!");

        let days = 30;
        let multiplier = 86400;

        let mut start_block_time = 1668038400;
        let mut end_block_time = start_block_time + (days * multiplier);

        let today = Local::now().timestamp();

        while start_block_time < today {
            info!("{} to {} till {}", start_block_time, end_block_time, today);
            update_transactions_in_gari_service(
                &config,
                &db,
                &client,
                None,
                start_block_time,
                end_block_time,
            )
            .await;
            start_block_time = end_block_time;
            end_block_time = start_block_time + (days * multiplier);
        }
    }

    #[tokio::test]
    #[ignore = "careful: update user staked data from solana"]
    async fn update_user_staked_amount() {
        let (config, db) = get_db().await;
        let mut tokens: Vec<(String, String, String)> = vec![];
        /*tokens.push((
            "HnxDB57oUhboa4jMzVmBLcb7kkaT3qqT4fzZy6HXygju".to_owned(),
            "7YGmiw2tk9aW3WCMDfTmmXQsRwLGuf6huWv7mhiUaAkr".to_owned(),
            "3imGkUC3hVWHMoCWwWGpMd5W4VDvMzasxDhywEyqQwS5".to_owned(),
        ));*/
        let mut token_amount_withdrawn: HashMap<String, u64> = HashMap::new();
        //token_amount_withdrawn.insert("HnxDB57oUhboa4jMzVmBLcb7kkaT3qqT4fzZy6HXygju".to_owned(), 0);

        /*let users_stmt = r#"SELECT * FROM public.staking_user_data
            ORDER BY user_spl_token_owner ASC
            OFFSET 1 ROWS LIMIT 50"#;

        let result = db
            .query_all(Statement::from_string(
                sql_stmt::DB_BACKEND,
                users_stmt.to_string(),
            ))
            .await;*/

        let result = User::find().all(&db).await;
        match result {
            Ok(users) => {
                for user in users {
                    /*let user_spl_token_owner: String =
                        user.try_get("", "user_spl_token_owner").unwrap();
                    tokens.push((
                        user_spl_token_owner.to_owned(),
                        user.try_get("", "staking_user_data_account").unwrap(),
                        user.try_get("", "user_token_wallet")
                            .unwrap_or("".to_owned()),
                    ));
                    let staked_amount = user
                    .try_get::<Decimal>("", "staked_amount")
                    .unwrap()
                    .to_string();*/
                    tokens.push((
                        user.user_spl_token_owner.to_owned(),
                        user.staking_user_data_account,
                        user.user_token_wallet.unwrap_or("".to_owned()),
                    ));
                    let staked_amount = user.staked_amount.to_string();
                    token_amount_withdrawn.insert(
                        user.user_spl_token_owner.to_owned(),
                        u64::from_str_radix(&staked_amount, 10).unwrap(),
                    );
                }
            }
            Err(error) => warn!("Error fetching users data: {:?}", error),
        }

        let solana_web_api_url =
            config.solana_web_api_node.to_owned() + "/get_staking_user_account_info";

        let reqwest_client = reqwest::Client::builder()
            .build()
            .expect("Reqwest client failed to initialize!");

        info!("Starting with: {}", tokens.len());

        match serde_json::to_string(&tokens) {
            Ok(json) => {
                let result = reqwest_client
                    .post(&solana_web_api_url)
                    .body(json)
                    .send()
                    .await;
                match result {
                    Ok(result) => {
                        match result
                            .json::<HashMap<String, crate::StakingUserDataAccount>>()
                            .await
                        {
                            Ok(users) => {
                                for (token, _user_data_account, _user_token_wallet) in &tokens {
                                    let user_result = User::find()
                                        .filter(UserColumn::UserSplTokenOwner.eq(token.to_owned()))
                                        .one(&db)
                                        .await;
                                    let db_user = match user_result {
                                        Ok(db_user) => db_user,
                                        Err(error) => {
                                            warn!("Error: {:?}", error);
                                            None
                                        }
                                    };
                                    if db_user.is_none() {
                                        info!("not found: {}", token);
                                        continue;
                                    }
                                    let db_user = db_user.unwrap();
                                    let old_staked_amount =
                                        token_amount_withdrawn.get(&token.to_owned()).unwrap();
                                    match users.get(&token.to_owned()) {
                                        Some(user) => {
                                            if user.staked_amount != 0
                                            /*&& (user.last_staking_timestamp
                                            != db_user.last_staking_timestamp
                                            || db_user.ownership_share.eq(&Decimal::ZERO)
                                            || db_user.user_token_wallet.is_none())*/
                                            {
                                                println!(
                                                    "{}, {}, {} : {}, {} : {}, {} : {}",
                                                    token,
                                                    user.user_token_wallet,
                                                    old_staked_amount,
                                                    user.staked_amount,
                                                    db_user.ownership_share,
                                                    user.ownership_share,
                                                    db_user.last_staking_timestamp,
                                                    user.last_staking_timestamp,
                                                );
                                                let mut active_user = db_user.into_active_model();
                                                active_user.ownership_share =
                                                    EntitySet(user.ownership_share.into());
                                                active_user.staked_amount =
                                                    EntitySet(user.staked_amount.into());
                                                active_user.last_staking_timestamp =
                                                    EntitySet(user.last_staking_timestamp.into());
                                                active_user.user_token_wallet = EntitySet(
                                                    user.user_token_wallet.to_owned().into(),
                                                );
                                                match active_user.update(&db).await {
                                                    Ok(_) => {}
                                                    Err(error) => {
                                                        warn!("error updating user: {:?}", error)
                                                    }
                                                }
                                            } else {
                                                //skip
                                            }
                                        }
                                        None => info!("No user found"),
                                    }
                                }
                            }
                            Err(error) => warn!("Error: {:?}", error),
                        }
                    }
                    Err(error) => {
                        warn!("Failed to send request to solana-web-api: {:?}", error)
                    }
                }
            }
            Err(error) => warn!("Could not create json: {:?}", error),
        };
    }

    #[tokio::test]
    async fn update_users_amount_withdrawn() {
        let to_update = false;
        let stmt = r#"SELECT user_spl_token_owner, SUM(amount_withdrawn) AS sum_amunt_withdrawn
            FROM public.staking_user_transaction_history 
            WHERE error = false AND instruction_type = 'unstake' AND amount_withdrawn IS NOT NULL AND block_time < $1
            GROUP BY user_spl_token_owner"#;
        let (_config, db) = get_db().await;
        let end_block_time = Local::now().timestamp() - (86400 * 2);
        let results = db
            .query_all(Statement::from_sql_and_values(
                sql_stmt::DB_BACKEND,
                stmt,
                vec![end_block_time.into()],
            ))
            .await;
        let mut sum_map: HashMap<String, Decimal> = HashMap::new();
        match results {
            Ok(results) => {
                for result in results {
                    match result.try_get("", "user_spl_token_owner") {
                        Ok(user_spl_token_owner) => {
                            let sum = result
                                .try_get::<Decimal>("", "sum_amunt_withdrawn")
                                .unwrap_or(Decimal::ZERO);
                            sum_map.insert(user_spl_token_owner, sum);
                        }
                        Err(error) => warn!("Error: {:?}", error),
                    }
                }
            }
            Err(error) => warn!(
                "Error in fetching sum of user amount_withdrawn: {:?}",
                error
            ),
        }

        let users = User::find().all(&db).await;
        match users {
            Ok(users) => {
                for user in users {
                    let user_spl_token_owner: String =
                        user.get(UserColumn::UserSplTokenOwner).unwrap();
                    let amount_withdrawn = user.get(UserColumn::AmountWithdrawn).to_string();
                    let amount_withdrawn =
                        Decimal::from_str_radix(&amount_withdrawn, 10).unwrap_or(Decimal::ZERO);

                    let sum_amount_withdrawn = match sum_map.get(&user_spl_token_owner) {
                        Some(sum) => *sum,
                        None => {
                            continue;
                        }
                    };
                    if sum_amount_withdrawn.eq(&Decimal::ZERO)
                        || amount_withdrawn.ge(&sum_amount_withdrawn)
                    {
                        continue;
                    }
                    println!(
                        "user token: {}, old amount: {}, new amount: {}",
                        user_spl_token_owner, amount_withdrawn, sum_amount_withdrawn
                    );
                    if to_update {
                        let mut active_user = user.into_active_model();
                        active_user.amount_withdrawn = EntitySet(Some(sum_amount_withdrawn));
                        match active_user.save(&db).await {
                            Ok(_) => {}
                            Err(error) => warn!("Error: {:?}", error),
                        }
                    }
                }
            }
            Err(_error) => {}
        }
    }

    #[tokio::test]
    #[ignore = "careful: update in process tasks"]
    async fn test_recheck_in_process_transactions() {
        let (config, db) = get_db().await;
        let client = reqwest::Client::builder()
            .build()
            .expect("Reqwest client failed to initialize!");
        recheck_in_process_transactions(&config, &db, &client, None).await;
    }

    #[allow(dead_code)]
    async fn is_stake_le_unstake(
        db: &DatabaseConnection,
        users: &Vec<UserModel>,
    ) -> HashMap<String, bool> {
        // check stake/unstake balance
        let sql = r#"SELECT user_spl_token_owner, SUM(amount) AS amount FROM public.staking_user_transaction_history
            WHERE instruction_type = $1 AND error = false
            GROUP BY user_spl_token_owner"#;

        let unstake_result = db
            .query_all(Statement::from_sql_and_values(
                sql_stmt::DB_BACKEND,
                sql,
                vec!["unstake".to_owned().into()],
            ))
            .await;
        let stake_result = db
            .query_all(Statement::from_sql_and_values(
                sql_stmt::DB_BACKEND,
                sql,
                vec!["stake".to_owned().into()],
            ))
            .await;

        let mut unstake_map: HashMap<String, Decimal> = HashMap::new();
        match unstake_result {
            Ok(unstakes) => {
                for unstake in unstakes {
                    let amount = unstake
                        .try_get::<Decimal>("", "amount")
                        .unwrap_or(Decimal::ZERO);
                    let token = unstake
                        .try_get::<String>("", "user_spl_token_owner")
                        .unwrap();
                    unstake_map.insert(token, amount);
                }
            }
            Err(_) => {}
        }

        let mut stake_map: HashMap<String, Decimal> = HashMap::new();
        match stake_result {
            Ok(stakes) => {
                for stake in stakes {
                    let amount = stake
                        .try_get::<Decimal>("", "amount")
                        .unwrap_or(Decimal::ZERO);
                    let token = stake.try_get::<String>("", "user_spl_token_owner").unwrap();
                    stake_map.insert(token, amount);
                }
            }
            Err(_) => {}
        }

        let mut map: HashMap<String, bool> = HashMap::new();

        for user in users {
            let unstake = unstake_map
                .get(&user.user_spl_token_owner)
                .unwrap_or(&Decimal::ZERO);
            let stake = stake_map
                .get(&user.user_spl_token_owner)
                .unwrap_or(&Decimal::ZERO);
            let flag = if stake.le(&unstake) { true } else { false };
            map.insert(user.user_spl_token_owner.to_owned(), flag);
        }

        map
    }
}
