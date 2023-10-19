use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(
    table_name = "staking_user_transaction_history",
    schema_name = "public"
)]
pub struct Model {
    #[sea_orm(primary_key)]
    pub transaction_signature: String,
    pub block_time: i64,
    pub error: bool,
    pub instruction_type: String,
    pub staking_data_account: String,
    pub staking_user_data_account: String,
    pub user_spl_token_owner: String,
    pub amount: Decimal,
    pub amount_withdrawn: Option<Decimal>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
