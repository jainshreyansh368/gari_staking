use crate::sea_orm::Database;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use sea_orm_cli::MigrateSubcommands;
use sea_orm_migration::prelude::*;
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
struct AppConfig {
    database_url: String,
    db_migration_instruction: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), figment::Error> {
    let key = "DATABASE_URL";
    let app_config: AppConfig = Figment::new().merge(Toml::file("App.toml")).extract()?;
    if std::env::var(key).is_err() {
        std::env::set_var(key, &app_config.database_url);
    }

    if None != app_config.db_migration_instruction
        && ("fresh".eq(app_config.db_migration_instruction.as_ref().unwrap())
            || "status".eq(app_config.db_migration_instruction.as_ref().unwrap()))
    {
        let subcommand = if "fresh".eq(app_config.db_migration_instruction.as_ref().unwrap()) {
            Some(MigrateSubcommands::Fresh)
        } else if "status".eq(app_config.db_migration_instruction.as_ref().unwrap()) {
            Some(MigrateSubcommands::Status)
        } else {
            None
        };
        let db = &Database::connect(&app_config.database_url).await.unwrap();
        let migrate_result =
            cli::run_migrate(staking_db_migration::Migrator, db, subcommand, false).await;
        match migrate_result {
            Ok(_) => println!("Migration succesfull!"),
            Err(error) => println!("{}", error.to_string()),
        }
    } else {
        cli::run_cli(staking_db_migration::Migrator).await;
    }

    Ok(())
}
