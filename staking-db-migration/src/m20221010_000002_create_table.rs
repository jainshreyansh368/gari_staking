use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000002_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_user_data::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(staking_user_data::Column::UserSplTokenOwner)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(staking_user_data::Column::StakingUserDataAccount).string())
                    .col(ColumnDef::new(staking_user_data::Column::StakingDataAccount).string())
                    .col(ColumnDef::new(staking_user_data::Column::IsGariUser).boolean())
                    .col(ColumnDef::new(staking_user_data::Column::OwnershipShare).decimal())
                    .col(ColumnDef::new(staking_user_data::Column::StakedAmount).decimal())
                    .col(ColumnDef::new(staking_user_data::Column::LockedAmount).decimal())
                    .col(ColumnDef::new(staking_user_data::Column::LockedUntil).big_integer())
                    .col(
                        ColumnDef::new(staking_user_data::Column::LastStakingTimestamp)
                            .big_integer(),
                    )
                    .col(ColumnDef::new(staking_user_data::Column::Balance).decimal())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(staking_user_data::Entity).to_owned())
            .await
    }
}
