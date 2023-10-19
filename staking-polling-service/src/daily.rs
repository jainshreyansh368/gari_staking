use crate::{
    esp_data, producer, slack,
    sync_transactions::{
        get_start_end_block_time, recheck_in_process_transactions,
        update_transactions_in_gari_service,
    },
};
use chrono::{Datelike, Local, TimeZone, Timelike};
use sea_orm::DatabaseConnection;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

pub async fn execute_tasks(
    config: &crate::config::Config,
    db: &DatabaseConnection,
    client: &reqwest::Client,
    datadog_client: Option<&datadog_apm::Client>,
) {
    let polling_daily_sleep_hms = match config.polling_daily_sleep_hms {
        Some(ref v) => v.to_owned(),
        None => "4:0:0".to_owned(),
    };
    let hms: Vec<&str> = polling_daily_sleep_hms.split(":").collect();
    let hour = u32::from_str_radix(hms[0], 10).unwrap();
    let min = u32::from_str_radix(hms[1], 10).unwrap();
    let sec = u32::from_str_radix(hms[2], 10).unwrap();

    let hosts: Vec<String> = match config.esp_hosts {
        Some(ref v) => v.to_owned().split(",").map(|e| String::from(e)).collect(),
        None => vec![String::from("")],
    };

    let timeout: u64 = match config.kafka_timeout {
        Some(ref v) => *v,
        None => 5000,
    };

    let topic: &str = match config.esp_topic {
        Some(ref v) => v,
        None => "",
    };

    let mut kafka_producer: Option<producer::KafkaProducer> = None;
    if config.enable_esp_kafka && topic != "" && hosts.len() > 0 {
        kafka_producer = match producer::KafkaProducer::init(hosts, timeout) {
            Ok(p) => Some(p),
            Err(e) => {
                warn!("Error in kafka producer {:?}", e);
                None
            }
        };
    } else {
        if !config.enable_esp_kafka {
            warn!("Kafka push disabled");
        } else {
            warn!("Missing topic/hosts in config");
        }
    }
    info!("Daily Tasks initialized");
    loop {
        wait_until_next_execution(hour, min, sec).await;

        info!("Execute Tasks started");

        if config.slack_notification {
            slack::post_notification(config, client).await;
        }

        let data = esp_data::esp_logs(db, client, config).await;

        match kafka_producer {
            Some(ref mut p) => info!("{:?}", p.send_message_to_topic(data, topic)),
            None => warn!("Kafka producer is not initialized"),
        };

        execute_accrue_interest(client, config).await;

        let (start_block_time, end_block_time) =
            get_start_end_block_time(Local::now(), hour, min, sec).await;
        update_transactions_in_gari_service(
            config,
            db,
            client,
            datadog_client,
            start_block_time,
            end_block_time,
        )
        .await;
        recheck_in_process_transactions(config, db, client, datadog_client).await;

        info!("Execute Tasks completed");
    }
}

async fn wait_until_next_execution(hour: u32, min: u32, sec: u32) {
    let current = Local::now();
    let mut target = Local
        .with_ymd_and_hms(
            current.year(),
            current.month(),
            current.day(),
            hour,
            min,
            sec,
        )
        .unwrap();
    if hour < current.hour()
        || (hour == current.hour() && min < current.minute())
        || (hour == current.hour() && min == current.minute() && sec < current.second())
    {
        target = target
            .checked_add_signed(chrono::Duration::days(1))
            .unwrap();
    }
    let diff = target.timestamp() - current.timestamp();
    sleep(Duration::from_secs(diff.try_into().unwrap())).await;
}

async fn execute_accrue_interest(client: &reqwest::Client, config: &crate::config::Config) {
    let solana_web_api = config.solana_web_api_node.to_owned() + "/accrue_interest";
    let response = client.get(solana_web_api).send().await;
    match response {
        Ok(_r) => info!("Successfully executed accrue_interest"),
        Err(error) => warn!("Error executing accrue_interest: {}", error),
    }
}
