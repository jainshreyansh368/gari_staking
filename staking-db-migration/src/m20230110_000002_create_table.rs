use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230110_000002_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_non_parsed_transaction::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(
                            staking_non_parsed_transaction::Column::TransactionSignature,
                        )
                        .string()
                        .not_null()
                        .primary_key(),
                    )
                    .col(
                        ColumnDef::new(staking_non_parsed_transaction::Column::AttemptTimestamp)
                            .big_integer(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(staking_non_parsed_transaction::Entity)
                    .to_owned(),
            )
            .await
    }
}
