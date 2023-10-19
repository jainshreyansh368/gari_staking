use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000003_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_user_transaction_history::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(
                            staking_user_transaction_history::Column::TransactionSignature,
                        )
                        .string()
                        .not_null()
                        .primary_key(),
                    )
                    .col(
                        ColumnDef::new(staking_user_transaction_history::Column::BlockTime)
                            .big_integer(),
                    )
                    .col(ColumnDef::new(staking_user_transaction_history::Column::Error).boolean())
                    .col(
                        ColumnDef::new(staking_user_transaction_history::Column::InstructionType)
                            .string(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_user_transaction_history::Column::StakingDataAccount,
                        )
                        .string(),
                    )
                    .col(
                        ColumnDef::new(
                            staking_user_transaction_history::Column::StakingUserDataAccount,
                        )
                        .string(),
                    )
                    .col(
                        ColumnDef::new(staking_user_transaction_history::Column::UserSplTokenOwner)
                            .string(),
                    )
                    .col(ColumnDef::new(staking_user_transaction_history::Column::Amount).decimal())
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
