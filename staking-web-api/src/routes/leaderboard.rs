use crate::dto::{ResponseData, UserDetails, RESPONSE_INTERNAL_ERROR, RESPONSE_OK};
use crate::fin_cal;
use crate::pool::{Db, StakingConfig};
use crate::sql_stmt::{DB_BACKEND, GARI_USERS, USERS};
use rocket::{serde::json::Json, State};
use sea_orm::{ConnectionTrait, Statement};
use sea_orm_rocket::Connection;
use tracing::error;

#[get("/leaderboard?<page>&<limit>&<is_gari_user>")]
pub async fn get(
    conn: Connection<'_, Db>,
    staking_config: &State<StakingConfig>,
    page: i64,
    limit: i64,
    is_gari_user: Option<bool>,
) -> Json<ResponseData<Vec<UserDetails>>> {
    let db = conn.into_inner();
    let start = (page - 1) * limit;

    let (query, params) = if is_gari_user.is_some() {
        (
            GARI_USERS,
            vec![is_gari_user.unwrap().into(), start.into(), limit.into()],
        )
    } else {
        (USERS, vec![start.into(), limit.into()])
    };
    let users = db
        .query_all(Statement::from_sql_and_values(DB_BACKEND, query, params))
        .await;
    let mut messages = String::new();
    let mut response = RESPONSE_OK;
    let users = match users {
        Ok(trx) => {
            if trx.is_empty() {
                messages = String::from("No users found");
            }
            trx
        }
        Err(err) => {
            error!("Error fetching users: {:?}", err);
            messages = String::from("Error fetching users");
            response = RESPONSE_INTERNAL_ERROR;
            vec![]
        }
    };
    let (staking_response_code, error_message_staking, staking_data) =
        crate::routes::user_and_stake::get_staking_data(db, staking_config).await;

    let mut user_leaderboard: Vec<UserDetails> = vec![];
    for user in &users {
        let mut user_details = UserDetails::new(user, false);
        fin_cal::update_user_amount_and_rewards(
            &mut user_details,
            staking_data.unwrap(),
            &UserDetails::get_balance(&user),
        );
        user_leaderboard.push(user_details);
    }

    let mut leaderboard = None;
    if !users.is_empty() {
        leaderboard = Some(user_leaderboard)
    }

    if !error_message_staking.is_empty() {
        messages.push_str(&error_message_staking);
    }
    if response < staking_response_code {
        response = staking_response_code;
    }
    Json(ResponseData::new(response, messages, leaderboard))
}
