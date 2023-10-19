use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(crate = "serde")]
pub struct SlackNotificationData {
    pub channel: String,
    pub text: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "serde")]
pub struct NotificationData {
    pub sol_balance: String,
    pub gari_balance: String,
    pub holding_wallet_balance: String,
    pub total_staked_balance: String,
    pub needed_interest: String,
    pub last_interest_accrued_time: String,
    pub user_action: String,
}


#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "serde")]
pub struct NotificationRequest {
    #[serde(rename = "type")]
    notification_type: String,
    payload: Payload,
}

impl NotificationRequest {
    pub fn new(data: Data) -> NotificationRequest {
        NotificationRequest {
            notification_type: "STAKING".to_owned(),
            payload: Payload::new(data),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "serde")]
struct Payload {
    data: Data,
}
impl Payload {
    pub fn new(data: Data) -> Payload {
        Payload { data }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(crate = "serde")]
pub struct Data {
    recevier_user_id: String,
    coins: String,
    transaction_case: String,
    transaction_id: String,
    transaction_status: String,
    mutable_content: u8,
}
impl Data {
    pub fn new(
        recevier_user_id: String,
        coins: String,
        transaction_case: String,
        transaction_id: String,
        transaction_status: String,
    ) -> Data {
        Data {
            recevier_user_id,
            coins,
            transaction_case,
            transaction_id,
            transaction_status,
            mutable_content: 1,
        }
    }
}
