use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "staking_non_parsed_transaction", schema_name = "public")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub transaction_signature: String,
    pub attempt_timestamp: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
