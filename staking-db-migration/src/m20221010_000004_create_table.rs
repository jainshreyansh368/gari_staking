use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000004_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_in_process_user_transaction::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(
                            staking_in_process_user_transaction::Column::GariTransactionId,
                        )
                        .string()
                        .not_null()
                        .primary_key(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_in_process_user_transaction::Column::TransactionSignature,
                        )
                        .string(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_in_process_user_transaction::Column::UserSplTokenOwner,
                        )
                        .string(),
                    )
                    .col(
                        ColumnDef::new(staking_in_process_user_transaction::Column::Status)
                            .string(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_in_process_user_transaction::Column::InstructionType,
                        )
                        .string(),
                    )
                    .col(
                        ColumnDef::new(staking_in_process_user_transaction::Column::Amount)
                            .string(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_in_process_user_transaction::Column::ProcessingTimestamp,
                        )
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
                    .table(staking_user_transaction_history::Entity)
                    .to_owned(),
            )
            .await
    }
}
