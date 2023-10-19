use sea_orm::entity::prelude::*;
use sea_orm::prelude::Decimal;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "staking_data", schema_name = "public")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub staking_data_account: String,
    pub owner: String,
    pub staking_account_token: String,
    pub holding_wallet: String,
    pub holding_bump: i16,
    pub total_staked: Decimal,
    pub total_shares: Decimal,
    pub interest_rate_hourly: i32,
    pub est_apy: i32,
    pub max_interest_rate_hourly: i32,
    pub last_interest_accrued_timestamp: i64,
    pub minimum_staking_amount: Decimal,
    pub minimum_staking_period_sec: i64,
    pub is_interest_accrual_paused: bool,
    pub is_active: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
