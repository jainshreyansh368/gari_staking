use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230210_000001_update_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(staking_user_transaction_history::Entity)
                    .add_column_if_not_exists(
                        ColumnDef::new(staking_user_transaction_history::Column::AmountWithdrawn)
                            .decimal(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(staking_user_transaction_history::Entity)
                    .drop_column(staking_user_transaction_history::Column::AmountWithdrawn)
                    .to_owned(),
            )
            .await
    }
}
