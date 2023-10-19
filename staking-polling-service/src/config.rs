use sea_orm::DbErr;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use serde::Deserialize;
use std::time::Duration;

pub async fn get_db_connection(config: &Config) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(config.database_url.to_owned());
    opt.max_connections(config.sqlx_max_connections)
        .min_connections(match config.sqlx_min_connections {
            Some(v) => v,
            None => 2,
        })
        .connect_timeout(Duration::from_secs(match config.sqlx_connect_timeout {
            Some(v) => v,
            None => 8,
        }))
        .idle_timeout(Duration::from_secs(match config.sqlx_idle_timeout {
            Some(v) => v,
            None => 8,
        }))
        .max_lifetime(Duration::from_secs(match config.sqlx_max_lifetime {
            Some(v) => v,
            None => 8,
        }))
        .sqlx_logging(match config.sqlx_logging {
            Some(v) => v,
            None => false,
        })
        .sqlx_logging_level(
            match config
                .polling_sqlx_logging_level
                .parse::<log::LevelFilter>()
            {
                Ok(level) => level,
                Err(_) => log::LevelFilter::Info,
            },
        );

    Database::connect(opt).await
}

#[derive(Debug, Deserialize)]
pub struct Config {
    database_url: String,
    pub solana_web_api_node: String,
    pub gari_web_api_node: String,
    pub gari_notification_node: String,
    pub x_staking_api_key: String,
    pub staking_account_address: String,
    pub rust_log: String,
    sqlx_max_connections: u32,
    sqlx_min_connections: Option<u32>,
    sqlx_connect_timeout: Option<u64>,
    sqlx_idle_timeout: Option<u64>,
    sqlx_max_lifetime: Option<u64>,
    sqlx_logging: Option<bool>,
    polling_sqlx_logging_level: String,
    pub polling_sleep_secs: Option<u64>,
    pub polling_batch_transactions: Option<usize>,
    pub polling_batch_sleep_millis: Option<u64>,
    pub polling_service_log: String,
    pub polling_daily_sleep_hms: Option<String>,
    pub slack_webhook_url: String,
    pub slack_channel_id: String,
    pub slack_notification: bool,
    pub instruction_metrics_row_limit: i64,
    pub esp_hosts: Option<String>,
    pub kafka_timeout: Option<u64>,
    pub esp_topic: Option<String>,
    pub enable_esp_kafka: bool,
    pub clevertap_api_node: String,
    pub clevertap_account_id: String,
    pub clevertap_api_key: String,
    pub clevertap_notification: bool,
    pub enable_datadog: bool,
    pub datadog_host: String,
    pub datadog_port: String,
}
