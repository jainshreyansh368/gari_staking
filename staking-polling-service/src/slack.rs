use crate::dto;
use tracing::{info, warn};

pub async fn post_notification(config: &crate::config::Config, client: &reqwest::Client) {
    let slack_webhook_url = config.slack_webhook_url.parse::<String>().unwrap();
    let slack_channel_id = config.slack_channel_id.parse::<String>().unwrap();

    let url = config.solana_web_api_node.to_owned() + "/get_notification_data";
    let response = client.get(url).send().await;
    match response {
        Ok(data) => {
            let json_response = data.json::<Result<String, String>>().await;
            match json_response {
                Ok(Ok(json)) => {
                    send_to_slack(client, &slack_webhook_url, &slack_channel_id, json).await;
                    info!("post_slack_notification completed");
                }
                Ok(Err(error)) => {
                    warn!("Json parsing error: {:?}", error);
                }
                Err(error) => {
                    warn!("Response error: {:?}", error);
                }
            }
        }
        Err(error) => {
            warn!("Error: {:?}", error);
            info!("Response error: {:?}", error);
        }
    }
}

async fn send_to_slack(
    client: &reqwest::Client,
    url: &str,
    slack_channel_id: &str,
    encoded_data: String,
) -> String {
    let data: dto::NotificationData = match serde_json::from_str(&encoded_data) {
        Ok(d) => d,
        Err(err) => return format!("error in deserializing encoded data {}", err),
    };

    let msg = std::format!(
        "{} \n{} \n{} \n{} \n{} \n{} \n{}",
        data.sol_balance,
        data.gari_balance,
        data.holding_wallet_balance,
        data.total_staked_balance,
        data.needed_interest,
        data.last_interest_accrued_time,
        data.user_action
    );
    let serialized_data = match serde_json::to_string(&dto::SlackNotificationData {
        channel: slack_channel_id.to_string(),
        text: msg,
    }) {
        Ok(json) => json,
        Err(err) => return format!("error in deserializing data {}", err),
    };
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .body(serialized_data)
        .send()
        .await;
    match response {
        Ok(resp) => match resp.status() {
            reqwest::StatusCode::OK => std::format!("Posted to slack channel {}", slack_channel_id),
            _ => String::from("err"),
        },
        _ => String::from("err"),
    }
}
