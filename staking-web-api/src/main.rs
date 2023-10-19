mod cors;
mod datadog;
mod dto;
mod fin_cal;
mod gari_service;
mod maintenance;
mod pool;
mod routes;
mod sql_stmt;

use dto::{ResponseData, RESPONSE_BAD_REQUEST, RESPONSE_INTERNAL_ERROR};
use pool::Db;
use rocket::{serde::json::Json, Config, Request};
use sea_orm_rocket::Database;
use std::collections::HashSet;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[macro_use]
extern crate rocket;

#[get("/")]
async fn health_ping() -> &'static str {
    ""
}

#[get("/maintenance_mode")]
async fn maintenance_mode() -> Json<ResponseData<&'static str>> {
    let response = ResponseData {
        code: Some(503),
        status_code: None,
        message: "".to_string(),
        data: None,
    };
    Json(response)
}

#[catch(404)]
async fn bad_request(req: &Request<'_>) -> Json<ResponseData<String>> {
    let message = format!("Couldn't find '{}'", req.uri());
    Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None))
}

#[catch(500)]
async fn internal_error() -> Json<ResponseData<String>> {
    Json(ResponseData::new(
        RESPONSE_INTERNAL_ERROR,
        "Whoops! Looks like we messed up.".to_owned(),
        None,
    ))
}

#[catch(404)]
async fn user_history_bad_data() -> Json<ResponseData<String>> {
    let message = format!("Please check params. 'instruction_type' should be stake or unstake only. 'page' & 'limit' are numeric.");
    Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None))
}

#[catch(404)]
async fn get_encoded_transaction_bad_data() -> Json<ResponseData<String>> {
    let message = format!("Please check params. 'instruction_type' should be stake or unstake only. user_spl_token_owner should be valid Pubkey. amount should be numeric.");
    Json(ResponseData::new(RESPONSE_BAD_REQUEST, message, None))
}

#[launch]
async fn rocket() -> _ {
    let staking_config = Config::figment().extract::<pool::StakingConfig>().unwrap();
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", &staking_config.rust_log);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(
                format!("staking_web_api={}", &staking_config.web_api_log)
                    .parse()
                    .expect("Error parsing directive"),
            ),
        )
        .with_span_events(FmtSpan::FULL)
        .init();

    let reqwest_client = reqwest::Client::builder()
        .build()
        .expect("Reqwest client failed to initialize!");

    let allowed_domains: HashSet<String> = staking_config
        .cors_allowed_domains
        .split(',')
        .map(|s| s.to_owned())
        .collect();

    let datadog_client = datadog_apm::Client::new(datadog_apm::Config {
        env: Some("prod-nft".to_owned()),
        service: "prod-staking-web-api".to_owned(),
        host: staking_config.datadog_host.to_owned(),
        port: staking_config.datadog_port.to_owned(),
        ..Default::default()
    });

    rocket::build()
        .register(
            "/get_encoded_transaction",
            catchers![get_encoded_transaction_bad_data],
        )
        .register("/user_history", catchers![user_history_bad_data])
        .register("/", catchers![internal_error, bad_request])
        .attach(Db::init())
        .attach(datadog::RequestTimer)
        .attach(maintenance::MaintenanceMode)
        .manage(staking_config)
        .manage(reqwest_client)
        .manage(datadog_client)
        .attach(cors::OriginHeader { allowed_domains })
        .attach(routes::mount())
        .mount("/", routes![health_ping, maintenance_mode])
}
