use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "staking_user_data", schema_name = "public")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user_spl_token_owner: String,
    pub staking_user_data_account: String,
    pub user_token_wallet: Option<String>,
    pub staking_data_account: String,
    pub is_gari_user: bool,
    pub ownership_share: Decimal,
    pub staked_amount: Decimal,
    pub locked_amount: Decimal,
    pub locked_until: i64,
    pub last_staking_timestamp: i64,
    pub balance: Decimal,
    pub amount_withdrawn: Option<Decimal>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
