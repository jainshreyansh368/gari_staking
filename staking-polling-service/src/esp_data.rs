use crate::sql_stmt;
use chrono::Local;
use sea_orm::{prelude::Decimal, ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

pub async fn esp_logs(
    db: &DatabaseConnection,
    client: &reqwest::Client,
    config: &crate::config::Config,
) -> String {
    let total_staked_tokens = get_total_staked(db).await;
    let holders_vs_stakers = get_holders_vs_stakers(
        db,
        client,
        &config.gari_web_api_node,
        &config.x_staking_api_key,
    )
    .await;
    let (subsequent_staking_and_unstaking, weighted_average_holding_time) =
        get_stake_unstake_metrics(db, config.instruction_metrics_row_limit).await;
    let gari_wallet_vs_phantom_staking = get_gari_vs_phantom_users(db).await;

    let current_date = format!("{}", Local::now().format("%Y-%m-%d"));
    let success_metrics = SuccessMetrics {
        total_staked_tokens,
        holders_vs_stakers,
        subsequent_staking_and_unstaking,
        gari_wallet_vs_phantom_staking,
        weighted_average_holding_time,
        current_date,
    };
    let esp_log_message: EspLogMessage<SuccessMetrics> = EspLogMessage {
        message: ESP_MESSAGE.to_owned(),
        event_type: EVENT_TYPE_SUCCESS_METRICS.to_owned(),
        data: success_metrics.clone(),
    };
    let data = serde_json::to_string(&esp_log_message).unwrap();
    info!("{}", data);

    let success_metrics_log = serde_json::to_string(&success_metrics).unwrap();
    if config.clevertap_notification {
        clevertap_event(client, config, success_metrics_log).await;
    }

    data
}

async fn get_total_staked(db: &DatabaseConnection) -> u64 {
    let result = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            SQL_TOTAL_STAKED_AMOUNT.to_string(),
        ))
        .await;
    match result {
        Ok(Some(result)) => {
            let total_staked = result
                .try_get::<Decimal>("", "total_staked_amount")
                .unwrap();
            u64::from_str_radix(&total_staked.to_string(), 10).unwrap()
        }
        Ok(None) => 0,
        Err(error) => {
            warn!("{}", error.to_string());
            0
        }
    }
}

async fn get_gari_vs_phantom_users(db: &DatabaseConnection) -> String {
    let gari_result = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            SQL_GARI_USERS.to_string(),
        ))
        .await;
    let gari_users = match gari_result {
        Ok(Some(gari_result)) => gari_result.try_get::<i64>("", "gari_users").unwrap(),
        Ok(None) => 0,
        Err(error) => {
            warn!("{}", error.to_string());
            0
        }
    };
    let phantom_result = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            SQL_PHANTOM_USERS.to_string(),
        ))
        .await;
    let phantom_users = match phantom_result {
        Ok(Some(phantom_result)) => phantom_result.try_get::<i64>("", "phantom_users").unwrap(),
        Ok(None) => 0,
        Err(error) => {
            warn!("{}", error.to_string());
            0
        }
    };
    gari_users.to_string() + ":" + &phantom_users.to_string()
}

async fn get_stake_unstake_metrics(db: &DatabaseConnection, row_limit: i64) -> (String, u64) {
    let mut start: i64 = 0;
    let mut total_unstaking: u64 = 0;
    let mut pre_user_spl_token_owner = "".to_owned();
    let mut pre_instruction_type = "".to_owned();
    let mut subsequent_staking = 0;
    let mut subsequent_stake_list: Vec<(u64, u64)> = Vec::new();
    // ∑ (user_amount_staked * weighted_holding_days)
    let mut total_per_user_cost: u64 = 0;
    // ∑ (user_amount_staked)
    let mut total_user_amount_staked: u64 = 0;
    loop {
        let result = db
            .query_all(Statement::from_sql_and_values(
                sql_stmt::DB_BACKEND,
                sql_stmt::SUBSEQUENT_STAKING,
                vec![start.into(), row_limit.into()],
            ))
            .await;
        match result {
            Ok(results) => {
                if results.is_empty() {
                    info!("End of results");
                    break;
                }
                for row in &results {
                    let instruction_type: String =
                        row.try_get::<String>("", "instruction_type").unwrap();
                    let user_spl_token_owner =
                        row.try_get::<String>("", "user_spl_token_owner").unwrap();
                    let block_time: u64 = row
                        .try_get::<i64>("", "block_time")
                        .unwrap()
                        .try_into()
                        .unwrap_or_default();
                    let amount = row.try_get::<Decimal>("", "amount").unwrap().to_string();
                    let amount = u64::from_str_radix(&amount, 10).unwrap();
                    if pre_user_spl_token_owner.eq(&user_spl_token_owner) {
                        if pre_instruction_type.eq("stake") && instruction_type.eq("stake") {
                            subsequent_staking += 1;
                        }

                        if instruction_type.eq("stake") {
                            subsequent_stake_list.push((block_time, amount));
                        } else if instruction_type.eq("unstake") {
                            total_unstaking += 1;
                            let mut unstaked_amount = amount;
                            // ∑ (staked_amount * holding_days)
                            let mut per_user_cost = 0;
                            // ∑ (staked_amount || unstaked_amount)
                            let mut user_amount_staked: u64 = 0;
                            for (staked_block_time, staked_amount) in &mut subsequent_stake_list {
                                if *staked_amount == 0 {
                                    continue;
                                }
                                if *staked_amount <= unstaked_amount {
                                    unstaked_amount -= *staked_amount;
                                    let holding_days =
                                        (block_time - *staked_block_time) / SECS_IN_DAY;
                                    if holding_days > 0 {
                                        user_amount_staked += *staked_amount;
                                        per_user_cost += *staked_amount * holding_days;
                                    }
                                    *staked_amount = 0;
                                } else if unstaked_amount != 0 {
                                    let holding_days =
                                        (block_time - *staked_block_time) / SECS_IN_DAY;
                                    if holding_days > 0 {
                                        user_amount_staked += unstaked_amount;
                                        per_user_cost += unstaked_amount * holding_days;
                                    }
                                    *staked_amount -= unstaked_amount;
                                    unstaked_amount = 0;
                                }
                                *staked_block_time = block_time;
                            }
                            if user_amount_staked != 0 {
                                total_user_amount_staked += user_amount_staked;
                                let weighted_holding_days = per_user_cost / user_amount_staked;
                                total_per_user_cost += user_amount_staked * weighted_holding_days;
                                debug!(
                                    "user_spl_token_owner: {}, per_user_cost: {}, user_amount_staked: {}, weighted_holding_days: {}",
                                    user_spl_token_owner, per_user_cost, user_amount_staked, weighted_holding_days
                                );
                            }
                        }
                    } else {
                        subsequent_stake_list.clear();
                        if instruction_type.eq("stake") {
                            subsequent_stake_list.push((block_time, amount));
                        }
                    }
                    pre_user_spl_token_owner = user_spl_token_owner;
                    pre_instruction_type = instruction_type;
                }
                start += results.len() as i64;
            }
            Err(error) => {
                warn!("SUBSEQUENT_STAKING Db Error: {}", error);
            }
        }
    }

    (
        subsequent_staking.to_string() + ":" + &total_unstaking.to_string(),
        total_per_user_cost
            .checked_div(total_user_amount_staked)
            .unwrap_or(0),
    )
}

async fn get_holders_vs_stakers(
    db: &DatabaseConnection,
    client: &reqwest::Client,
    gari_web_api_node: &str,
    x_staking_api_key: &str,
) -> String {
    let gari_web_api = gari_web_api_node.to_owned() + "/activatedWallets";
    let response = client
        .get(gari_web_api)
        .header("X-STAKING-API-KEY", x_staking_api_key.to_owned())
        .header("User-Agent", "Staking Web Api")
        .send()
        .await;
    let gari_holders = match response {
        Ok(response) => match response
            .json::<ResponseData<ActivatedWalletsResponse>>()
            .await
        {
            Ok(response) => match response.data {
                Some(data) => data.total_users,
                None => {
                    warn!("No data received from gari service /activatedWallets");
                    0
                }
            },
            Err(error) => {
                warn!("{}", error);
                0
            }
        },
        Err(error) => {
            warn!("{}", error);
            0
        }
    };

    let gari_result = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            SQL_GARI_STAKED_USERS.to_string(),
        ))
        .await;
    let gari_stakers = match gari_result {
        Ok(Some(gari_result)) => gari_result.try_get::<i64>("", "staked_users").unwrap(),
        Ok(None) => 0,
        Err(error) => {
            warn!("{}", error.to_string());
            0
        }
    };
    gari_holders.to_string() + ":" + &gari_stakers.to_string()
}

async fn clevertap_event(
    client: &reqwest::Client,
    config: &crate::config::Config,
    success_metrics: String,
) {
    let upload_event_url = config.clevertap_api_node.to_owned() + "/upload";
    let log_message =
        "{ \"d\": [ { \"identity\": \"staking-polling-service\", \"type\": \"event\", \"evtName\": \"".to_owned()
            + EVENT_TYPE_SUCCESS_METRICS
            + "\", \"evtData\": "
            + &success_metrics
            + " } ] }";
    let result = client
        .post(upload_event_url)
        .body(log_message)
        .header("Content-Type", "application/json")
        .header(
            "X-CleverTap-Account-Id",
            config.clevertap_account_id.to_owned(),
        )
        .header("X-CleverTap-Passcode", config.clevertap_api_key.to_owned())
        .send()
        .await;
    match result {
        Ok(result) => match result.json::<ClevertapResponse>().await {
            Ok(result) => {
                if result.unprocessed.as_ref().is_some()
                    && result.unprocessed.as_ref().unwrap().len() > 0
                {
                    for unprocessed in result.unprocessed.unwrap() {
                        warn!("Error: {:?}", unprocessed);
                    }
                } else {
                    info!("Successfully sent event to clevertap");
                }
            }
            Err(error) => warn!("Clevertap response: {}", error),
        },
        Err(error) => warn!("Clevertap json parsing: {}", error),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EspLogMessage<T> {
    message: String,
    event_type: String,
    data: T,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SuccessMetrics {
    total_staked_tokens: u64,
    holders_vs_stakers: String,
    subsequent_staking_and_unstaking: String,
    gari_wallet_vs_phantom_staking: String,
    weighted_average_holding_time: u64,
    current_date: String,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ResponseData<T> {
    pub code: Option<u16>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<u16>,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ActivatedWalletsResponse {
    #[serde(rename = "totalUsers")]
    total_users: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClevertapResponse {
    status: String,
    processed: Option<u8>,
    unprocessed: Option<Vec<ClevertapRecord>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClevertapRecord {
    status: String,
    code: String,
    error: String,
    record: String,
}

const SQL_TOTAL_STAKED_AMOUNT: &str =
    r#"SELECT SUM(staked_amount) AS total_staked_amount FROM public.staking_user_data"#;
const SQL_GARI_USERS: &str = r#"SELECT COUNT(is_gari_user) AS gari_users FROM public.staking_user_data 
    WHERE is_gari_user = TRUE"#;
const SQL_PHANTOM_USERS: &str = r#"SELECT COUNT(is_gari_user) AS phantom_users FROM public.staking_user_data 
    WHERE is_gari_user = FALSE"#;
const SQL_GARI_STAKED_USERS: &str = r#"SELECT COUNT(*) AS staked_users FROM public.staking_user_data 
    WHERE is_gari_user = TRUE AND staked_amount > 0.0"#;
const ESP_MESSAGE: &str = "Data for ESP";
const EVENT_TYPE_SUCCESS_METRICS: &str = "Success Metrics";

const SECS_IN_DAY: u64 = 86400;
