use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230110_000001_update_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(staking_user_data::Entity)
                    .add_column_if_not_exists(
                        ColumnDef::new(staking_user_data::Column::AmountWithdrawn).decimal(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(staking_user_data::Entity)
                    .drop_column(staking_user_data::Column::AmountWithdrawn)
                    .to_owned(),
            )
            .await
    }
}
