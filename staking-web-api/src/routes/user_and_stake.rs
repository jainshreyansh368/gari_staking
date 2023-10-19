use crate::dto::{
    ResponseData, StakingDataAccount, UserAndStakeDetails, UserDetails, RESPONSE_INTERNAL_ERROR,
    RESPONSE_OK,
};
use crate::pool::{Db, StakingConfig};
use rocket::{serde::json::Json, State};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement,
};
use sea_orm_rocket::Connection;
use staking_db_entity::db::staking_data::{Column as StakingColumn, Entity as Staking};
use tracing::{error, warn};

#[get(
    "/user_and_stake_details?<user_spl_token_owner>",
    format = "application/json"
)]
pub async fn get_details(
    conn: Connection<'_, Db>,
    staking_config: &State<crate::pool::StakingConfig>,
    user_spl_token_owner: String,
) -> Json<ResponseData<UserAndStakeDetails>> {
    let db = conn.into_inner();
    let mut user_details = None;
    let token_owner = db
        .query_one(Statement::from_sql_and_values(
            crate::sql_stmt::DB_BACKEND,
            crate::sql_stmt::TOKEN_OWNER,
            vec![
                user_spl_token_owner.to_owned().into(),
                staking_config.staking_account_address.to_owned().into(),
            ],
        ))
        .await;
    let mut messages = String::new();
    let mut response = RESPONSE_OK;
    let mut balance = String::new();
    match token_owner {
        Ok(Some(token_owner)) => {
            user_details = Some(UserDetails::new(&token_owner, true));
            balance = UserDetails::get_balance(&token_owner);
        }
        Ok(None) => {
            messages = format!("User not found: {}. ", user_spl_token_owner);
            warn!("{}", messages);
            response = RESPONSE_OK;
        }
        Err(error) => {
            warn!("Error fetching owner rank: {:?}", error);
            messages = String::from("Error fetching owner rank.");
            response = RESPONSE_INTERNAL_ERROR;
        }
    };

    let (stake_response_code, message, staking_data) = get_staking_data(db, staking_config).await;
    match staking_data {
        Some(data) => match user_details {
            Some(ref mut user) => {
                crate::fin_cal::update_user_amount_and_rewards(user, data, &balance);
            }
            None => {}
        },
        None => {}
    }

    let user_and_stake_details = UserAndStakeDetails::new(user_details, staking_data);
    if !message.is_empty() {
        messages.push_str(&("  ".to_owned() + &message));
    }

    if response < stake_response_code {
        response = stake_response_code;
    }

    Json(ResponseData::new(
        response,
        messages,
        Some(user_and_stake_details),
    ))
}

pub async fn get_staking_data(
    db: &DatabaseConnection,
    staking_config: &State<StakingConfig>,
) -> (u16, String, Option<StakingDataAccount>) {
    let staking_account = Staking::find()
        .filter(StakingColumn::StakingDataAccount.contains(&staking_config.staking_account_address))
        .one(db)
        .await;

    match staking_account {
        Ok(Some(account)) => (
            RESPONSE_OK,
            "".to_owned(),
            Some(StakingDataAccount::new(account)),
        ),
        Ok(None) => {
            let error = format!(
                "Staking account {} not found!",
                staking_config.staking_account_address
            );
            warn!("{}", error);
            (RESPONSE_OK, String::from(error), None)
        }
        Err(error) => {
            error!("Error: {:?}", error);
            (
                RESPONSE_INTERNAL_ERROR,
                String::from("System error. Please contact administrator!"),
                None,
            )
        }
    }
}
