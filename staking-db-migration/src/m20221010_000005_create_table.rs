use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000005_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_encoded_transaction::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(staking_encoded_transaction::Column::Id)
                            .big_integer()
                            .not_null()
                            .primary_key()
                            .auto_increment(),
                    )
                    .col(
                        ColumnDef::new(staking_encoded_transaction::Column::Timestamp)
                            .big_integer(),
                    )
                    .col(ColumnDef::new(staking_encoded_transaction::Column::Uuid).uuid())
                    .col(
                        ColumnDef::new(staking_encoded_transaction::Column::UserSplTokenOwner)
                            .string(),
                    )
                    .col(
                        ColumnDef::new(staking_encoded_transaction::Column::InstructionType)
                            .string(),
                    )
                    .col(ColumnDef::new(staking_encoded_transaction::Column::Amount).string())
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
