use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(
    table_name = "staking_in_process_user_transaction",
    schema_name = "public"
)]
pub struct Model {
    #[sea_orm(primary_key)]
    pub gari_transaction_id: String,
    pub transaction_signature: String,
    pub user_spl_token_owner: String,
    pub status: String,
    pub instruction_type: String,
    pub amount: String,
    pub processing_timestamp: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
