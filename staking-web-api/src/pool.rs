use async_trait::async_trait;
use rocket::serde::Deserialize;
use rocket::Config;
use sea_orm::ConnectOptions;
use sea_orm_rocket::{rocket::figment::Figment, Database};
use std::time::Duration;

#[derive(Database, Debug)]
#[database("sea_orm")]
pub struct Db(SeaOrmPool);

#[derive(Debug, Clone)]
pub struct SeaOrmPool {
    pub conn: sea_orm::DatabaseConnection,
}

#[async_trait]
impl sea_orm_rocket::Pool for SeaOrmPool {
    type Error = sea_orm::DbErr;

    type Connection = sea_orm::DatabaseConnection;

    async fn init(_figment: &Figment) -> Result<Self, Self::Error> {
        let config = Config::figment().extract::<StakingConfig>().unwrap();
        let mut options: ConnectOptions = config.database_url.into();
        options
            .max_connections(config.sqlx_max_connections)
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
                    .web_api_sqlx_logging_level
                    .parse::<log::LevelFilter>()
                {
                    Ok(level) => level,
                    Err(_) => log::LevelFilter::Info,
                },
            );

        let conn = sea_orm::Database::connect(options).await?;

        Ok(SeaOrmPool { conn })
    }

    fn borrow(&self) -> &Self::Connection {
        &self.conn
    }
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingConfig {
    database_url: String,
    pub solana_web_api_node: String,
    pub gari_web_api_node: String,
    pub gari_min_unstake_amount: u64,
    sqlx_max_connections: u32,
    sqlx_min_connections: Option<u32>,
    sqlx_connect_timeout: Option<u64>,
    sqlx_idle_timeout: Option<u64>,
    sqlx_max_lifetime: Option<u64>,
    sqlx_logging: Option<bool>,
    web_api_sqlx_logging_level: String,
    pub rust_log: String,
    pub web_api_log: String,
    pub staking_account_address: String,
    pub cors_allowed_domains: String,
    pub jwt_key: String,
    pub enable_datadog: bool,
    pub datadog_host: String,
    pub datadog_port: String,
    pub enable_maintenance: bool,
}
