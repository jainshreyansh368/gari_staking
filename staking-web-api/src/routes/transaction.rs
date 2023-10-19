use crate::dto::{
    AuthToken, InstructionType, ResponseData, SendTransactionRequestData, RESPONSE_BAD_REQUEST,
    RESPONSE_INTERNAL_ERROR, RESPONSE_OK,
};
use crate::gari_service;
use crate::pool::{Db, StakingConfig};
use rocket::{serde::json::Json, State};
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use sea_orm_rocket::Connection;
use staking_db_entity::db::staking_data::{Column as StakingColumn, Entity as Staking};
use staking_db_entity::db::staking_encoded_transaction::{
    ActiveModel as EncodedTransactionActiveModel, Entity as EncodedTransaction,
    Model as EncodedTransactionModel,
};
use staking_db_entity::db::staking_user_data::{
    Column as StakingUserColumn, Entity as StakingUser, Model as StakingUserModel,
};
use tracing::{info, warn};
use uuid::Uuid;

#[get("/get_encoded_transaction?<user_spl_token_owner>&<instruction_type>&<amount>")]
pub async fn encode(
    conn: Connection<'_, Db>,
    staking_config: &State<StakingConfig>,
    user_spl_token_owner: String,
    instruction_type: InstructionType,
    amount: u64,
) -> Json<ResponseData<SendTransactionRequestData>> {
    info!("get_encoded_transaction started");

    if InstructionType::Unstake.eq(&instruction_type)
        && amount < staking_config.gari_min_unstake_amount
    {
        return Json(ResponseData::new(
            RESPONSE_BAD_REQUEST,
            "Minimum amount violation.".to_owned(),
            None,
        ));
    };

    let db = conn.into_inner();
    let user: Result<Option<StakingUserModel>, sea_orm::DbErr> = StakingUser::find()
        .filter(StakingUserColumn::UserSplTokenOwner.eq(user_spl_token_owner.to_owned()))
        .one(db)
        .await;

    let mut query_params = vec![
        ("user_spl_token_owner", user_spl_token_owner.to_owned()),
        ("instruction_type", instruction_type.to_string()),
        ("amount", amount.to_string()),
    ];

    match user {
        Ok(Some(user)) => {
            query_params.push(("staking_user_data_account", user.staking_user_data_account));
        }
        Ok(None) => {}
        Err(error) => {
            info!("Error in db: {:?}", error);
        }
    };

    // get from db user_spl_token_account

    let staking_account = Staking::find()
        .filter(
            StakingColumn::StakingDataAccount.eq(staking_config.staking_account_address.to_owned()),
        )
        .one(db)
        .await;

    match staking_account {
        Ok(Some(staking_account)) => {
            query_params.push(("staking_holding_wallet", staking_account.holding_wallet));
        }
        _ => {}
    };

    // TODO: get from db
    //query_vec.push("user_spl_token_account", user_spl_token_account);

    let url = staking_config.solana_web_api_node.to_owned() + "/get_encoded_transaction";

    let client = reqwest::Client::builder()
        .build()
        .expect("Failed to create reqwest client!");
    let get_encoded_transaction = client.get(url).query(&query_params.as_slice()).send();

    match get_encoded_transaction.await {
        Ok(response) => {
            let json_response = response.json::<Result<String, String>>().await;
            match json_response {
                Ok(Ok(transaction)) => {
                    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_DNS, transaction.as_bytes());
                    let active_model = EncodedTransactionActiveModel {
                        id: ActiveValue::NotSet,
                        timestamp: ActiveValue::Set(chrono::Utc::now().timestamp()),
                        uuid: ActiveValue::Set(uuid),
                        user_spl_token_owner: ActiveValue::Set(user_spl_token_owner),
                        instruction_type: ActiveValue::Set(instruction_type.to_string()),
                        amount: ActiveValue::Set(amount.to_string()),
                    };
                    match EncodedTransaction::insert(active_model).exec(db).await {
                        Ok(_) => {}
                        Err(error) => {
                            info!("Error: {:?}", error);
                            let err_msg = "Could not insert encoded_transaction.";
                            warn!("{}", err_msg);
                            return Json(ResponseData::new(
                                RESPONSE_BAD_REQUEST,
                                err_msg.to_owned(),
                                None,
                            ));
                        }
                    }
                    Json(ResponseData::new(
                        RESPONSE_OK,
                        "".to_owned(),
                        Some(SendTransactionRequestData::new(
                            uuid.to_string(),
                            transaction,
                        )),
                    ))
                }
                Ok(Err(error)) => {
                    warn!("Response error: {:?}", error);
                    Json(ResponseData::new(RESPONSE_BAD_REQUEST, error, None))
                }
                Err(error) => {
                    warn!("Error: {:?}", error);
                    Json(ResponseData::new(
                        RESPONSE_INTERNAL_ERROR,
                        "Error connecting service".to_owned(),
                        None,
                    ))
                }
            }
        }
        Err(error) => {
            warn!("Error: {:?}", error);
            Json(ResponseData::new(
                RESPONSE_INTERNAL_ERROR,
                "Error connecting service".to_owned(),
                None,
            ))
        }
    }
}

#[post(
    "/send_transaction",
    format = "application/json",
    data = "<send_transaction_request_data>"
)]
pub async fn send(
    conn: Connection<'_, Db>,
    staking_config: &State<StakingConfig>,
    client: &State<reqwest::Client>,
    auth_token: AuthToken<'_>,
    send_transaction_request_data: Json<SendTransactionRequestData>,
) -> Json<ResponseData<String>> {
    let db = conn.into_inner();
    let (transaction_id, gari_error, transaction_details): (
        String,
        String,
        Option<EncodedTransactionModel>,
    ) = gari_service::create_transaction_in_gari(
        client,
        db,
        auth_token,
        &send_transaction_request_data,
        staking_config.gari_web_api_node.to_owned(),
    )
    .await;

    if transaction_id.is_empty() {
        return Json(ResponseData::new(RESPONSE_BAD_REQUEST, gari_error, None));
    }
    let transaction_details = transaction_details.unwrap();

    let url = staking_config.solana_web_api_node.to_owned() + "/send_transaction";

    let result = client
        .post(url)
        .body(send_transaction_request_data.encoded_transaction.to_owned())
        .send()
        .await;

    match result {
        Ok(response) => match response.json::<Result<String, String>>().await {
            Ok(Ok(signature)) => {
                gari_service::insert_to_db_processing(
                    db,
                    transaction_details.user_spl_token_owner.to_owned(),
                    transaction_details.instruction_type.to_owned(),
                    transaction_details.amount.to_owned(),
                    &transaction_id,
                    &signature,
                )
                .await;
                match transaction_details.delete(db).await {
                    Ok(_) => {}
                    Err(error) => {
                        warn!("Deleting encoded_transaction failed: {}", error.to_string());
                    }
                }
                Json(ResponseData::new(
                    RESPONSE_OK,
                    "".to_owned(),
                    Some(signature),
                ))
            }
            Ok(Err(error)) => {
                let error_message = format!("Service Error: {}", error);
                warn!("{}", error_message);
                Json(ResponseData::new(RESPONSE_BAD_REQUEST, error_message, None))
            }
            Err(error) => {
                let error_message = format!("Json Error: {}", error.to_string());
                warn!("{}", error_message);
                Json(ResponseData::new(
                    RESPONSE_INTERNAL_ERROR,
                    error_message,
                    None,
                ))
            }
        },
        Err(error) => {
            let error_message = format!("Json2 Error: {}", error.to_string());
            warn!("{}", error_message);
            Json(ResponseData::new(
                RESPONSE_INTERNAL_ERROR,
                error_message,
                Some("".to_owned()),
            ))
        }
    }
}
