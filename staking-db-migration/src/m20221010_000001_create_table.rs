use sea_orm_migration::prelude::*;
use staking_db_entity::db::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000001_create_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(staking_data::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(staking_data::Column::StakingDataAccount)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(staking_data::Column::Owner).string())
                    .col(ColumnDef::new(staking_data::Column::StakingAccountToken).string())
                    .col(ColumnDef::new(staking_data::Column::HoldingWallet).string())
                    .col(ColumnDef::new(staking_data::Column::HoldingBump).small_integer())
                    .col(ColumnDef::new(staking_data::Column::TotalStaked).decimal())
                    .col(ColumnDef::new(staking_data::Column::TotalShares).decimal())
                    .col(ColumnDef::new(staking_data::Column::InterestRateHourly).integer())
                    .col(ColumnDef::new(staking_data::Column::EstApy).integer())
                    .col(ColumnDef::new(staking_data::Column::MaxInterestRateHourly).integer())
                    .col(
                        ColumnDef::new(staking_data::Column::LastInterestAccruedTimestamp)
                            .big_integer(),
                    )
                    .col(ColumnDef::new(staking_data::Column::MinimumStakingAmount).decimal())
                    .col(
                        ColumnDef::new(staking_data::Column::MinimumStakingPeriodSec).big_integer(),
                    )
                    .col(ColumnDef::new(staking_data::Column::IsInterestAccrualPaused).boolean())
                    .col(ColumnDef::new(staking_data::Column::IsActive).boolean())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(staking_data::Entity).to_owned())
            .await
    }
}
