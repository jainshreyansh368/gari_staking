use crate::dto::*;
use chrono::Utc;
use rocket::{serde::json::Json, State};
use sea_orm::{ActiveValue, EntityTrait};
use sea_orm::{ColumnTrait, DatabaseConnection, QueryFilter};
use staking_db_entity::db::staking_encoded_transaction::{
    Column as EncodedTransactionColumn, Entity as EncodedTransaction,
    Model as EncodedTransactionModel,
};
use staking_db_entity::db::staking_in_process_user_transaction::{
    ActiveModel as InProcessTransactionActiveModel, Entity as InProcessTransaction,
};
use tracing::warn;
use uuid::Uuid;

pub async fn create_transaction_in_gari(
    client: &State<reqwest::Client>,
    db: &DatabaseConnection,
    auth_token: AuthToken<'_>,
    send_transaction_request_data: &Json<SendTransactionRequestData>,
    gari_web_api_node: String,
) -> (String, String, Option<EncodedTransactionModel>) {
    let uuid = match Uuid::parse_str(&send_transaction_request_data.uuid) {
        Ok(uuid) => uuid,
        Err(error) => {
            let error_message = format!("Invalid uuid syntax: {}", error);
            warn!("{}", error_message);
            return ("".to_owned(), error_message, None);
        }
    };
    let transaction_details = EncodedTransaction::find()
        .filter(EncodedTransactionColumn::Uuid.eq(uuid))
        .one(db)
        .await;
    let transaction_details = match transaction_details {
        Ok(Some(transaction_details)) => transaction_details,
        Ok(None) => {
            let error_message = format!("No such transaction found.");
            warn!("{}", error_message);
            return ("".to_owned(), error_message, None);
        }
        Err(error) => {
            let error_message = format!("Encoded transaction error: {}", error.to_string());
            warn!("{}", error_message);
            return ("".to_owned(), error_message, None);
        }
    };
    let create_transaction = CreateStakingTransaction::new(
        transaction_details.instruction_type.to_string(),
        u128::from_str_radix(&transaction_details.amount, 10).unwrap(),
    );
    let gari_web_api = gari_web_api_node + "/createStakingTransaction";
    let result = client
        .post(gari_web_api)
        .bearer_auth(auth_token.to_string())
        .json(&create_transaction)
        .header("User-Agent", "Staking Web Api")
        .send()
        .await;

    let result_txt = format!("{:?}", result);
    let (transaction_id, gari_error) = match result {
        Ok(response) => {
            let json_response = response
                .json::<ResponseData<CreateStakingTransactionResponse>>()
                .await;
            match json_response {
                Ok(result) => {
                    if result.code == Some(200) || result.code == Some(201) {
                        (result.data.unwrap().transaction_id, "".to_owned())
                    } else {
                        let code = if result.code.is_some() {
                            result.code.unwrap()
                        } else {
                            result.status_code.unwrap()
                        };
                        let error_message = "Code: ".to_owned()
                            + &code.to_string()
                            + ". Gari createStakingTransaction Error: "
                            + &result.message;
                        ("".to_owned(), error_message)
                    }
                }
                Err(error) => {
                    let error = format!(
                        "Error in parsing createStakingTransaction response: {}",
                        error
                    );
                    warn!("{}", error);
                    warn!("createStakingTransaction Result: {}", result_txt);
                    ("".to_owned(), error)
                }
            }
        }
        Err(error) => {
            let error = format!(
                "Error returned by gari createStakingTransaction service: {}",
                error
            );
            warn!("{}", error);
            ("".to_owned(), error)
        }
    };
    (transaction_id, gari_error, Some(transaction_details))
}

pub async fn insert_to_db_processing(
    db: &DatabaseConnection,
    user_spl_token_owner: String,
    instruction_type: String,
    amount: String,
    transaction_id: &str,
    transaction_signature: &str,
) {
    let processing_transaction = InProcessTransactionActiveModel {
        gari_transaction_id: ActiveValue::Set(transaction_id.to_owned()),
        transaction_signature: ActiveValue::Set(transaction_signature.to_owned()),
        user_spl_token_owner: ActiveValue::Set(user_spl_token_owner.to_owned()),
        status: ActiveValue::Set(TRANSACTION_PROCESSING.to_owned()),
        instruction_type: ActiveValue::Set(instruction_type),
        amount: ActiveValue::Set(amount),
        processing_timestamp: ActiveValue::Set(Utc::now().timestamp()),
    };

    match InProcessTransaction::insert(processing_transaction)
        .exec(db)
        .await
    {
        Ok(_) => {}
        Err(error) => {
            warn!("Could not insert in process transaction: {}", error);
        }
    }
}
